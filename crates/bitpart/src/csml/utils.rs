use base64::prelude::*;
use chrono::{SecondsFormat, Utc};
use csml_interpreter::data::{
    ast::{Flow, InsertStep, InstructionScope},
    context::ContextStepInfo,
    Client, Context, CsmlBot, CsmlFlow, Event, Interval, Memory, Message,
};
use csml_interpreter::get_step;
use csml_interpreter::interpreter::json_to_literal;
use md5::{Digest, Md5};
use rand::{thread_rng, Rng};
use regex::Regex;
use sea_orm::DatabaseConnection;
use serde_json::{json, map::Map, Value};
use std::collections::HashMap;
use std::env;
use tracing::debug;

use super::data::{ConversationData, FlowTrigger};
use crate::db;
use crate::error::BitpartError;

fn add_info_to_message(data: &ConversationData, mut msg: Message, interaction_order: i32) -> Value {
    let payload = msg.message_to_json();

    let mut map_msg: Map<String, Value> = Map::new();
    map_msg.insert("payload".to_owned(), payload);
    map_msg.insert("interaction_order".to_owned(), json!(interaction_order));
    map_msg.insert("conversation_id".to_owned(), json!(data.conversation_id));
    map_msg.insert("direction".to_owned(), json!("SEND"));

    Value::Object(map_msg)
}

pub fn messages_formatter(
    data: &mut ConversationData,
    vec_msg: Vec<Message>,
    interaction_order: i32,
    end: bool,
) -> Map<String, Value> {
    let msgs = vec_msg
        .into_iter()
        .map(|msg| add_info_to_message(data, msg, interaction_order))
        .collect();
    let mut map: Map<String, Value> = Map::new();

    map.insert("messages".to_owned(), Value::Array(msgs));
    map.insert("conversation_end".to_owned(), Value::Bool(end));
    map.insert("request_id".to_owned(), json!(data.request_id));

    map.insert(
        "received_at".to_owned(),
        json!(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)),
    );

    let mut map_client: Map<String, Value> = Map::new();

    map_client.insert("bot_id".to_owned(), json!(data.client.bot_id));
    map_client.insert("user_id".to_owned(), json!(data.client.user_id));
    map_client.insert("channel_id".to_owned(), json!(data.client.channel_id));

    map.insert("client".to_owned(), Value::Object(map_client));

    map
}

fn format_and_transfer(callback_url: &str, msg: serde_json::Value) {
    let mut request = ureq::post(callback_url);

    request = request
        .set("Accept", "application/json")
        .set("Content-Type", "application/json");

    let response = request.send_json(msg);

    if let Err(err) = response {
        eprintln!("callback_url call failed: {:?}", err.to_string());
    }
}

/**
 * If a callback_url is defined, we must send each message to its endpoint as it comes.
 * Otherwise, just continue!
 */
fn send_to_callback_url(data: &mut ConversationData, msg: serde_json::Value) {
    let callback_url = match &data.callback_url {
        Some(callback_url) => callback_url,
        None => return,
    };

    format_and_transfer(callback_url, msg)
}

pub fn send_msg_to_callback_url(
    data: &mut ConversationData,
    msg: Vec<Message>,
    interaction_order: i32,
    end: bool,
) {
    let messages = messages_formatter(data, msg, interaction_order, end);

    debug!(
        bot_id = data.client.bot_id.to_string(),
        user_id = data.client.user_id.to_string(),
        channel_id = data.client.channel_id.to_string(),
        flow = data.context.flow.to_string(),
        "conversation_end: {:?}",
        messages["conversation_end"]
    );

    send_to_callback_url(data, serde_json::json!(messages))
}

pub fn update_current_context(data: &mut ConversationData, memories: &HashMap<String, Memory>) {
    for (_key, mem) in memories.iter() {
        let lit = json_to_literal(&mem.value, Interval::default(), &data.context.flow).unwrap();

        data.context.current.insert(mem.key.to_owned(), lit);
    }
}

/**
 * Retrieve a flow in a given bot by an identifier:
 * - matching method is case insensitive
 * - as name is similar to a flow's alias, both flow.name and flow.id can be matched.
 */
pub fn get_flow_by_id<'a>(f_id: &str, flows: &'a [CsmlFlow]) -> Result<&'a CsmlFlow, BitpartError> {
    let id = f_id.to_ascii_lowercase();
    // TODO: move to_lowercase at creation of vars
    match flows
        .iter()
        .find(|&val| val.id.to_ascii_lowercase() == id || val.name.to_ascii_lowercase() == id)
    {
        Some(ref f) => Ok(f),
        None => Err(BitpartError::Interpreter(format!(
            "Flow '{}' does not exist",
            f_id
        ))),
    }
}

/**
 * Retrieve a bot's default flow.
 * The default flow must exist!
 */
pub fn get_default_flow<'a>(bot: &'a CsmlBot) -> Result<&'a CsmlFlow, BitpartError> {
    match bot
        .flows
        .iter()
        .find(|&flow| flow.id == bot.default_flow || flow.name == bot.default_flow)
    {
        Some(flow) => Ok(flow),
        None => Err(BitpartError::Interpreter(
            "The bot's default_flow does not exist".to_owned(),
        )),
    }
}

pub async fn clean_hold_and_restart(
    data: &mut ConversationData,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    db::state::delete(&data.client, "hold", "position", db).await?;
    data.context.hold = None;
    return Ok(());
}

pub fn get_current_step_hash(context: &Context, bot: &CsmlBot) -> Result<String, BitpartError> {
    let mut hash = Md5::new();

    let step = match &context.step {
        ContextStepInfo::Normal(step) => {
            let flow = &get_flow_by_id(&context.flow, &bot.flows)?.content;

            let ast = match &bot.bot_ast {
                Some(ast) => {
                    let base64decoded = BASE64_STANDARD.decode(&ast).unwrap();
                    let csml_bot: HashMap<String, Flow> =
                        bincode::deserialize(&base64decoded[..]).unwrap();
                    match csml_bot.get(&context.flow) {
                        Some(flow) => flow.to_owned(),
                        None => csml_bot
                            .get(&get_default_flow(&bot)?.name)
                            .unwrap()
                            .to_owned(),
                    }
                }
                None => return Err(BitpartError::Interpreter(format!("not valid ast"))),
            };

            get_step(step, &flow, &ast)
        }
        ContextStepInfo::UnknownFlow(step) => {
            let flow = &get_flow_by_id(&context.flow, &bot.flows)?.content;

            match &bot.bot_ast {
                Some(ast) => {
                    let base64decoded = BASE64_STANDARD.decode(&ast).unwrap();
                    let csml_bot: HashMap<String, Flow> =
                        bincode::deserialize(&base64decoded[..]).unwrap();

                    let default_flow = csml_bot.get(&get_default_flow(&bot)?.name).unwrap();

                    match csml_bot.get(&context.flow) {
                        Some(target_flow) => {
                            // check if there is a inserted step with the same name as the target step
                            let insertion_expr = target_flow.flow_instructions.get_key_value(
                                &InstructionScope::InsertStep(InsertStep {
                                    name: step.clone(),
                                    original_name: None,
                                    from_flow: "".to_owned(),
                                    interval: Interval::default(),
                                }),
                            );

                            // if there is a inserted step get the flow of the target step and
                            if let Some((InstructionScope::InsertStep(insert), _)) = insertion_expr
                            {
                                match csml_bot.get(&insert.from_flow) {
                                    Some(inserted_step_flow) => {
                                        let inserted_raw_flow =
                                            &get_flow_by_id(&insert.from_flow, &bot.flows)?.content;

                                        get_step(step, &inserted_raw_flow, &inserted_step_flow)
                                    }
                                    None => get_step(step, &flow, &default_flow),
                                }
                            } else {
                                get_step(step, &flow, &target_flow)
                            }
                        }
                        None => get_step(step, &flow, &default_flow),
                    }
                }
                None => return Err(BitpartError::Interpreter(format!("not valid ast"))),
            }
        }
        ContextStepInfo::InsertedStep {
            step,
            flow: inserted_flow,
        } => {
            let flow = &get_flow_by_id(inserted_flow, &bot.flows)?.content;

            let ast = match &bot.bot_ast {
                Some(ast) => {
                    let base64decoded = BASE64_STANDARD.decode(&ast).unwrap();
                    let csml_bot: HashMap<String, Flow> =
                        bincode::deserialize(&base64decoded[..]).unwrap();

                    match csml_bot.get(inserted_flow) {
                        Some(flow) => flow.to_owned(),
                        None => csml_bot
                            .get(&get_default_flow(&bot)?.name)
                            .unwrap()
                            .to_owned(),
                    }
                }
                None => return Err(BitpartError::Interpreter(format!("not valid ast"))),
            };

            get_step(step, &flow, &ast)
        }
    };

    hash.update(step.as_bytes());

    Ok(format!("{:x}", hash.finalize()))
}

pub fn get_ttl_duration_value(event: Option<&Event>) -> Option<chrono::Duration> {
    if let Some(event) = event {
        if let Some(ttl) = event.ttl_duration {
            return Some(chrono::Duration::days(ttl));
        }
    }

    if let Ok(ttl) = env::var("TTL_DURATION") {
        if let Some(ttl) = ttl.parse::<i64>().ok() {
            return Some(chrono::Duration::days(ttl));
        }
    }

    return None;
}

pub fn get_low_data_mode_value(event: &Event) -> bool {
    if let Some(low_data) = event.low_data_mode {
        return low_data;
    }

    if let Ok(low_data) = env::var("LOW_DATA_MODE") {
        if let Ok(low_data) = low_data.parse::<bool>() {
            return low_data;
        }
    }

    return false;
}

pub async fn search_flow<'a>(
    event: &Event,
    bot: &'a CsmlBot,
    client: &Client,
    db: &DatabaseConnection,
) -> Result<(&'a CsmlFlow, String), BitpartError> {
    match event {
        event if event.content_type == "flow_trigger" => {
            db::state::delete(&client, "hold", "position", db).await?;

            let flow_trigger: FlowTrigger = serde_json::from_str(&event.content_value)?;

            match get_flow_by_id(&flow_trigger.flow_id, &bot.flows) {
                Ok(flow) => match flow_trigger.step_id {
                    Some(step_id) => Ok((flow, step_id)),
                    None => Ok((flow, "start".to_owned())),
                },
                Err(_) => Ok((
                    get_flow_by_id(&bot.default_flow, &bot.flows)?,
                    "start".to_owned(),
                )),
            }
        }
        event if event.content_type == "regex" => {
            let mut random_flows = vec![];

            for flow in bot.flows.iter() {
                let contains_command = flow.commands.iter().any(|cmd| {
                    if let Ok(action) = Regex::new(&event.content_value) {
                        action.is_match(&cmd)
                    } else {
                        false
                    }
                });

                if contains_command {
                    random_flows.push(flow)
                }
            }

            // gen_range will panic if range is empty
            let random = if random_flows.len() != 0 {
                thread_rng().gen_range(0..random_flows.len())
            } else {
                0
            };
            match random_flows.get(random) {
                Some(flow) => {
                    db::state::delete(&client, "hold", "position", db).await?;
                    Ok((flow, "start".to_owned()))
                }
                None => Err(BitpartError::Interpreter(format!(
                    "no match found for regex: {}",
                    event.content_value
                ))),
            }
        }
        event => {
            let mut random_flows = vec![];

            for flow in bot.flows.iter() {
                let contains_command = flow
                    .commands
                    .iter()
                    .any(|cmd| &cmd.as_str().to_lowercase() == &event.content_value.to_lowercase());

                if contains_command {
                    random_flows.push(flow)
                }
            }

            // gen_range will panic if range is empty
            let random = if random_flows.len() != 0 {
                thread_rng().gen_range(0..random_flows.len())
            } else {
                0
            };
            match random_flows.get(random) {
                Some(flow) => {
                    db::state::delete(&client, "hold", "position", db).await?;
                    Ok((flow, "start".to_owned()))
                }
                None => Err(BitpartError::Interpreter(format!(
                    "Flow '{}' does not exist",
                    event.content_value
                ))),
            }
        }
    }
}
