// ErrorInfo is large by design; boxing it everywhere is not worth the churn.
#![allow(clippy::result_large_err)]
// Evaluation functions intentionally thread many parameters by design.
#![allow(clippy::too_many_arguments)]
// Triggered by the `#[typetag::serde]` macro on the Primitive trait.
#![allow(non_local_definitions)]

pub mod data;
pub mod error_format;
pub mod fold_bot;
pub mod interpreter;
pub mod linter;
pub mod parser;

pub use data::csml_logs;
pub use interpreter::components::load_components;
pub use parser::step_checksum::get_step;

use interpreter::{interpret_scope, json_to_literal};
use parser::parse_flow;

use data::ast::{Expr, Flow, InsertStep, InstructionScope, Interval};
use data::context::{get_hashmap_from_mem, ContextStepInfo};
use data::error_info::ErrorInfo;
use data::event::Event;
use data::literal::create_error_info;
use data::message_data::MessageData;
use data::msg::MSG;
use data::CsmlResult;
use data::{csml_bot::CsmlBot, CsmlFlow};
use data::{Context, Data, Position, STEP_LIMIT};
use error_format::*;
use fold_bot::fold_bot as fold;
use linter::{linter::lint_bot, FlowToValidate};
use parser::ExitCondition;

use std::collections::HashMap;
use std::env;
use std::sync::mpsc;

////////////////////////////////////////////////////////////////////////////////
// PRIVATE FUNCTIONS
////////////////////////////////////////////////////////////////////////////////

async fn execute_step(
    step: &str,
    flow: &Flow,
    data: &mut Data<'_>,
    sender: &Option<mpsc::Sender<MSG>>,
) -> MessageData {
    // stop execution if step_count >= STEP_LIMIT in order to avoid infinite loops
    if *data.step_count >= data.step_limit {
        let msg_data = Err(gen_error_info(
            Position::new(
                Interval::new_as_u32(0, 0, 0, None, None),
                &data.context.flow,
            ),
            format!("{}, stop at step {}", ERROR_STEP_LIMIT, step),
        ));

        return MessageData::error_to_message(msg_data, sender);
    }

    let mut msg_data = match flow
        .flow_instructions
        .get(&InstructionScope::StepScope(step.to_owned()))
    {
        Some(Expr::Scope { scope, .. }) => {
            *data.step_count += 1;
            interpret_scope(scope, data, sender).await
        }
        _ => Err(gen_error_info(
            Position::new(
                Interval::new_as_u32(0, 0, 0, None, None),
                &data.context.flow,
            ),
            format!("[{}] {}", step, ERROR_STEP_EXIST),
        )),
    };

    if let Ok(msg_data) = &mut msg_data {
        match &mut msg_data.exit_condition {
            Some(condition) if *condition == ExitCondition::Goto => {
                msg_data.exit_condition = None;
            }
            Some(_) => (),
            // if no goto at the end of the scope end conversation
            None => {
                msg_data.exit_condition = Some(ExitCondition::End);
                data.context.step = ContextStepInfo::Normal("end".to_string());
                MSG::send(
                    sender,
                    MSG::Next {
                        flow: None,
                        step: Some(ContextStepInfo::Normal("end".to_owned())),
                        bot: None,
                    },
                );
            }
        }
    }

    MessageData::error_to_message(msg_data, sender)
}

fn get_step_limit(event: &Event) -> usize {
    match (event.step_limit, env::var("STEP_LIMIT").ok()) {
        (Some(step_limit), _) => step_limit,
        (None, Some(step_limit)) => step_limit.parse::<usize>().unwrap_or(STEP_LIMIT),
        _ => STEP_LIMIT,
    }
}

fn get_flow_ast<'a, 'b>(
    flows: &'a HashMap<String, Flow>,
    flow: &'b str,
    bot_id: &'b str,
    sender: &Option<mpsc::Sender<MSG>>,
) -> Result<&'a Flow, MessageData> {
    match flows.get(flow) {
        Some(result) => Ok(result),
        None => {
            let error_message = format!("flow: [{}] does not exist in bot: [{}]", flow, bot_id);
            let error_info = create_error_info(&error_message, Interval::default());

            Err(MessageData::error_to_message(
                Err(ErrorInfo {
                    position: Position {
                        flow: flow.to_owned(),
                        interval: Interval::default(),
                    },
                    message: error_message,
                    additional_info: Some(error_info),
                }),
                sender,
            ))
        }
    }
}

fn get_inserted_ast<'a>(
    flows: &'a HashMap<String, Flow>,
    ast: &'a Flow,
    step: &ContextStepInfo,
    bot_id: &str,
    sender: &Option<mpsc::Sender<MSG>>,
) -> (bool, Option<&'a Flow>) {
    match &step {
        ContextStepInfo::Normal(step) => {
            let missing_step = !ast
                .flow_instructions
                .contains_key(&InstructionScope::StepScope(step.to_owned()));

            (missing_step, None)
        }
        ContextStepInfo::UnknownFlow(step_name) => {
            let missing_step = !ast
                .flow_instructions
                .contains_key(&InstructionScope::StepScope(step_name.to_owned()));

            if missing_step {
                match ast
                    .flow_instructions
                    .get_key_value(&InstructionScope::InsertStep(InsertStep {
                        name: step_name.to_owned(),
                        original_name: None,
                        from_flow: "".to_owned(),
                        interval: Interval::default(),
                    })) {
                    Some((InstructionScope::InsertStep(insert_step), _expr)) => {
                        let step = ContextStepInfo::InsertedStep {
                            step: step_name.to_owned(),
                            flow: insert_step.from_flow.clone(),
                        };

                        get_inserted_ast(flows, ast, &step, bot_id, sender)
                    }
                    _ => (missing_step, None),
                }
            } else {
                (missing_step, None)
            }
        }
        ContextStepInfo::InsertedStep { step, flow } => {
            match get_flow_ast(flows, flow, bot_id, sender) {
                Ok(inserted_ast) => {
                    let missing_step = !inserted_ast
                        .flow_instructions
                        .contains_key(&InstructionScope::StepScope(step.to_owned()));

                    (missing_step, Some(inserted_ast))
                }
                Err(_) => (true, None),
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// PUBLIC FUNCTIONS
////////////////////////////////////////////////////////////////////////////////

pub fn get_steps_from_flow(bot: CsmlBot) -> HashMap<String, Vec<String>> {
    csml_logs::init_logger();

    let mut result = HashMap::new();

    for flow in bot.flows.iter() {
        if let Ok(parsed_flow) = parse_flow(&flow.content, &flow.name) {
            let mut vec = vec![];

            for instruction_type in parsed_flow.flow_instructions.keys() {
                if let InstructionScope::StepScope(step_name, ..) = instruction_type {
                    vec.push(step_name.to_owned());
                }
            }
            result.insert(flow.name.to_owned(), vec);
        }
    }
    result
}

/// Parse every flow in `bot` (both top-level flows and module flows) into
/// `FlowToValidate` entries. Returns `(flows, modules, errors)`; any flow that
/// fails to parse contributes an error instead of an entry.
#[allow(clippy::type_complexity)]
fn collect_flows_to_validate(
    bot: &CsmlBot,
) -> (
    Vec<FlowToValidate<'_>>,
    Vec<FlowToValidate<'_>>,
    Vec<ErrorInfo>,
) {
    let mut flows = vec![];
    let mut modules = vec![];
    let mut errors = Vec::new();

    for flow in bot.flows.iter() {
        match parse_flow(&flow.content, &flow.name) {
            Ok(ast_flow) => {
                flows.push(FlowToValidate {
                    flow_name: flow.name.to_owned(),
                    ast: ast_flow,
                    raw_flow: &flow.content,
                });
            }
            Err(error) => {
                errors.push(error);
            }
        }
    }

    if let Some(ref mods) = bot.modules {
        for module in mods.iter() {
            if let Some(flow) = &module.flow {
                match parse_flow(&flow.content, &flow.name) {
                    Ok(ast_flow) => {
                        modules.push(FlowToValidate {
                            flow_name: flow.name.to_owned(),
                            ast: ast_flow,
                            raw_flow: &flow.content,
                        });
                    }
                    Err(error) => {
                        errors.push(error);
                    }
                }
            }
        }
    }

    (flows, modules, errors)
}

pub fn validate_bot(bot: &CsmlBot) -> CsmlResult {
    csml_logs::init_logger();

    let (flows, modules, mut errors) = collect_flows_to_validate(bot);

    let mut warnings = vec![];
    // only use the linter if there is no error in the paring otherwise the linter will catch false errors
    if errors.is_empty() {
        lint_bot(
            &flows,
            &modules,
            &mut errors,
            &mut warnings,
            &bot.native_components,
            &bot.default_flow,
        );
    }

    CsmlResult::new(
        FlowToValidate::get_flows(flows),
        FlowToValidate::get_flows(modules),
        warnings,
        errors,
    )
}

pub fn fold_bot(bot: &CsmlBot) -> String {
    csml_logs::init_logger();

    let (flows, modules, mut errors) = collect_flows_to_validate(bot);

    let mut warnings = vec![];
    // only use the fold if there is no error in the paring otherwise the linter will catch false errors

    fold(
        &flows,
        &modules,
        &mut errors,
        &mut warnings,
        &bot.native_components,
        &bot.default_flow,
    )
}

fn get_flows(bot: &CsmlBot) -> (HashMap<String, Flow>, HashMap<String, Flow>) {
    match &bot.bot_ast {
        Some(bot) => {
            let base64decoded = base64::decode(bot).unwrap();
            bincode::deserialize(&base64decoded[..]).unwrap()
        }
        None => {
            let bot = validate_bot(bot);

            let flows = bot.flows.unwrap_or_default();

            let extern_flows = bot.extern_flows.unwrap_or_default();

            (flows, extern_flows)
        }
    }
}

pub async fn search_for_modules(bot: &mut CsmlBot) -> Result<(), String> {
    let default_auth = env::var("MODULES_AUTH").ok();
    let default_url = env::var("MODULES_URL").ok();

    if let Some(ref mut modules) = bot.modules {
        for module in modules.iter_mut() {
            if module.flow.is_some() {
                // module already downloaded
                continue;
            }

            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .map_err(|err| err.to_string())?;
            let request = match (&module.url, &default_url) {
                (Some(url), _) => {
                    let request = client.get(url);
                    match &module.auth {
                        Some(auth) => {
                            let authorization =
                                format!("Basic {}", base64::encode(auth.as_bytes()));

                            request.header("Authorization", &authorization)
                        }
                        _ => request,
                    }
                }
                (None, Some(url)) => {
                    let request = client.get(url);

                    match &default_auth {
                        Some(auth) => {
                            let authorization =
                                format!("Basic {}", base64::encode(auth.as_bytes()));

                            request.header("Authorization", &authorization)
                        }
                        _ => request,
                    }
                }
                _ => {
                    return Err(format!(
                        "missing url in order to get module [{}]",
                        module.name
                    ));
                }
            };

            let result: Result<String, reqwest::Error> = async move {
                let response = request.send().await?.error_for_status()?;
                response.text().await
            }
            .await;

            match result {
                Ok(flow_content) => {
                    module.flow = Some(CsmlFlow {
                        id: module.name.clone(),
                        name: module.name.clone(),
                        content: flow_content,
                        commands: vec![],
                    });
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    Ok(())
}

pub async fn interpret(
    bot: CsmlBot,
    mut context: Context,
    event: Event,
    sender: Option<mpsc::Sender<MSG>>,
) -> MessageData {
    csml_logs::init_logger();

    let mut msg_data = MessageData::default();

    let mut flow = context.flow.to_owned();
    let mut step = context.step.to_owned();

    let mut step_count = 0;
    let step_limit = get_step_limit(&event);

    let mut step_vars = match &context.hold {
        Some(hold) => get_hashmap_from_mem(&hold.step_vars, &flow),
        None => HashMap::new(),
    };

    let native = match bot.native_components {
        Some(ref obj) => obj.to_owned(),
        None => serde_json::Map::new(),
    };

    let custom = match bot.custom_components {
        Some(serde_json::Value::Object(ref obj)) => obj.to_owned(),
        _ => serde_json::Map::new(),
    };

    let (flows, extern_flows) = get_flows(&bot);

    let env = match bot.env {
        Some(env) => json_to_literal(&env, Interval::default(), &flow).unwrap(),
        None => data::primitive::PrimitiveNull::get_literal(Interval::default()),
    };

    let mut previous_info = match &context.hold {
        Some(hold) => hold.previous.clone(),
        None => None,
    };

    while msg_data.exit_condition.is_none() {
        let ast = match get_flow_ast(&flows, &flow, &bot.id, &sender) {
            Ok(ast) => ast,
            Err(message_data) => return message_data,
        };

        let (missing_step, inserted_ast) = get_inserted_ast(&flows, ast, &step, &bot.id, &sender);

        // if the target flow dose not contains a 'start' flow change the target to the default_flow
        if step.is_step("start") && missing_step {
            flow = bot.default_flow.clone();
            continue;
        }

        let mut data = Data::new(
            &flows,
            &extern_flows,
            ast,
            bot.default_flow.clone(),
            &mut context,
            &event,
            &env,
            vec![],
            0,
            &mut step_count,
            step_limit,
            step_vars,
            previous_info.clone(),
            &custom,
            &native,
        );

        msg_data = match inserted_ast {
            Some(inserted_ast) => {
                msg_data + execute_step(&step.get_step(), inserted_ast, &mut data, &sender).await
            }
            None => msg_data + execute_step(&step.get_step(), ast, &mut data, &sender).await,
        };

        previous_info = data.previous_info.clone();
        flow = data.context.flow.to_string();
        step = data.context.step.clone();

        // add reset loops index
        step_vars = HashMap::new();
    }

    msg_data
}
