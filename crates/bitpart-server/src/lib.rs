mod actions;
pub mod api;
pub mod csml;
mod data;
mod db;
pub mod error;
mod event;
mod utils;
use sea_orm::*;

use async_recursion::async_recursion;
use chrono::Utc;
use csml_interpreter::data::{
    ast::Flow,
    context::{get_hashmap_from_json, get_hashmap_from_mem, ContextStepInfo},
    ApiInfo, Client, Context, CsmlBot, CsmlFlow, CsmlResult, Event, Message, PreviousBot,
};
use csml_interpreter::{load_components, search_for_modules, validate_bot};
use data::{BotOpt, ConversationData, Request, SwitchBot};
use error::BitpartError;
use std::collections::HashMap;

async fn create_new_conversation<'a>(
    context: &mut Context,
    bot: &'a CsmlBot,
    flow_found: Option<(&'a CsmlFlow, String)>,
    client: &Client,
    ttl: Option<chrono::Duration>,
    db: &DatabaseConnection,
) -> Result<String, BitpartError> {
    let (flow, step) = match flow_found {
        Some((flow, step)) => (flow, step),
        None => (utils::get_default_flow(bot)?, "start".to_owned()),
    };

    let conversation_id = db::conversation::create(
        &flow.id,
        &step,
        client,
        ttl.map(|t| Utc::now().naive_utc() + t),
        db,
    )
    .await?;

    context.step = ContextStepInfo::UnknownFlow(step);
    context.flow = flow.name.to_owned();

    Ok(conversation_id)
}

async fn get_or_create_conversation<'a>(
    context: &mut Context,
    bot: &'a CsmlBot,
    flow_found: Option<(&'a CsmlFlow, String)>,
    client: &Client,
    ttl: Option<chrono::Duration>,
    db: &DatabaseConnection,
) -> Result<String, BitpartError> {
    match db::conversation::get_latest_by_client(client, db).await? {
        Some(conversation) => {
            match flow_found {
                Some((flow, step)) => {
                    context.step = ContextStepInfo::UnknownFlow(step);
                    context.flow = flow.name.to_owned();
                }
                None => {
                    let flow = match utils::get_flow_by_id(&conversation.flow_id, &bot.flows) {
                        Ok(flow) => flow,
                        Err(..) => {
                            // if flow id exist in db but not in bot close conversation
                            db::conversation::set_status_by_id(&conversation.id, "CLOSED", db)
                                .await?;
                            // start new conversation at default flow
                            return create_new_conversation(
                                context, bot, flow_found, client, ttl, db,
                            )
                            .await;
                        }
                    };

                    context.step = ContextStepInfo::UnknownFlow(conversation.step_id.to_owned());
                    context.flow = flow.name.to_owned();
                }
            };

            Ok(conversation.id)
        }
        None => create_new_conversation(context, bot, flow_found, client, ttl, db).await,
    }
}

async fn get_previous_bot(client: &Client, db: &DatabaseConnection) -> Option<PreviousBot> {
    match db::state::get(client, "bot", "previous", db).await {
        Ok(bot) => serde_json::from_value(bot).ok(),
        _ => None,
    }
}

pub async fn init_context(
    flow: String,
    client: Client,
    apps_endpoint: &Option<String>,
    db: &DatabaseConnection,
) -> Context {
    let previous_bot = get_previous_bot(&client, db).await;

    let api_info = match apps_endpoint {
        Some(value) => Some(ApiInfo {
            client,
            apps_endpoint: value.to_owned(),
        }),
        None => None,
    };

    Context {
        current: HashMap::new(),
        metadata: HashMap::new(),
        api_info,
        hold: None,
        step: ContextStepInfo::Normal("start".to_owned()),
        flow,
        previous_bot,
    }
}

pub async fn init_conversation_info<'a>(
    default_flow: String,
    event: &Event,
    request: &'a Request,
    bot: &'a CsmlBot,
    db: &DatabaseConnection,
) -> Result<ConversationData, BitpartError> {
    // Create a new interaction. An interaction is basically each request,
    // initiated from the bot or the user.

    let mut context =
        init_context(default_flow, request.client.clone(), &bot.apps_endpoint, db).await;
    let ttl = utils::get_ttl_duration_value(Some(event));
    let low_data = utils::get_low_data_mode_value(event);

    // Do we have a flow matching the request? If the user is requesting a flow in one way
    // or another, this takes precedence over any previously open conversation
    // and a new conversation is created with the new flow as a starting point.
    let flow_found = utils::search_flow(event, &bot, &request.client, db)
        .await
        .ok();
    let conversation_id =
        get_or_create_conversation(&mut context, &bot, flow_found, &request.client, ttl, db)
            .await?;

    context.metadata = get_hashmap_from_json(&request.metadata, &context.flow);
    let memories = db::memory::get_by_client(&request.client, None, None, db).await?;
    let mut map = serde_json::Map::new();
    for mem in memories {
        if !map.contains_key(&mem.key) {
            map.insert(mem.key, serde_json::json!(mem.value));
        }
    }

    context.current = get_hashmap_from_mem(&serde_json::json!(map), &context.flow);

    let data = ConversationData {
        conversation_id,
        context,
        metadata: request.metadata.clone(), // ??
        request_id: request.id.clone(),
        callback_url: request.callback_url.clone(),
        client: request.client.clone(),
        messages: vec![],
        ttl,
        low_data,
    };

    let flow = data.context.flow.to_owned();
    let step = data.context.step.to_owned();

    // Now that everything is correctly setup, update the conversation with wherever
    // we are now and continue with the rest of the request!
    db::conversation::update(&data.conversation_id, Some(flow), Some(step.get_step()), db).await?;

    Ok(data)
}

/**
 * Initialize the bot
 */
pub fn init_bot(bot: &mut CsmlBot) -> Result<(), BitpartError> {
    // load native components into the bot
    bot.native_components = match load_components() {
        Ok(components) => Some(components),
        Err(err) => return Err(BitpartError::Interpreter(err.format_error())),
    };

    if let Err(err) = search_for_modules(bot) {
        return Err(BitpartError::Interpreter(format!("{:?}", err)));
    }

    set_bot_ast(bot)
}

/**
 * Initialize bot ast
 */
fn set_bot_ast(bot: &mut CsmlBot) -> Result<(), BitpartError> {
    match validate_bot(&bot) {
        CsmlResult {
            flows: Some(flows),
            extern_flows: Some(extern_flows),
            errors: None,
            ..
        } => {
            bot.bot_ast = Some(base64::encode(
                bincode::serialize(&(&flows, &extern_flows)).unwrap(),
            ));
        }
        CsmlResult {
            flows: Some(flows),
            extern_flows: None,
            errors: None,
            ..
        } => {
            let extern_flows: HashMap<String, Flow> = HashMap::new();

            bot.bot_ast = Some(base64::encode(
                bincode::serialize(&(&flows, &extern_flows)).unwrap(),
            ));
        }
        CsmlResult {
            errors: Some(errors),
            ..
        } => {
            return Err(BitpartError::Interpreter(format!(
                "invalid bot {:?}",
                errors
            )))
        }
        _ => return Err(BitpartError::Interpreter(format!("empty bot"))),
    }

    Ok(())
}

pub async fn switch_bot(
    data: &mut ConversationData,
    bot: &mut CsmlBot,
    next_bot: SwitchBot,
    bot_opt: &mut BotOpt,
    event: &mut Event,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    // update data info with new bot |ex| client bot_id, create new conversation
    *bot_opt = match next_bot.version_id {
        Some(version_id) => BotOpt::Id {
            version_id,
            bot_id: next_bot.bot_id,
            apps_endpoint: bot.apps_endpoint.take(),
            multibot: bot.multibot.take(),
        },
        None => BotOpt::BotId {
            bot_id: next_bot.bot_id,
            apps_endpoint: bot.apps_endpoint.take(),
            multibot: bot.multibot.take(),
        },
    };

    let mut new_bot = bot_opt.search_bot(db).await?;
    new_bot.custom_components = bot.custom_components.take();
    new_bot.native_components = bot.native_components.take();

    *bot = new_bot;

    set_bot_ast(bot)?;

    data.context.step = ContextStepInfo::UnknownFlow(next_bot.step);
    data.context.flow = match next_bot.flow {
        Some(flow) => flow,
        None => bot.get_default_flow_name(),
    };

    // update client with the new bot id
    data.client.bot_id = bot.id.to_owned();

    let (flow, step) = match utils::get_flow_by_id(&data.context.flow, &bot.flows) {
        Ok(flow) => (flow, data.context.step.clone()),
        Err(_) => {
            let error_message = format!(
                "flow: [{}] not found in bot: [{}], switching to start@default_flow",
                data.context.flow, bot.name
            );

            let message = Message {
                content_type: "error".to_owned(),
                content: serde_json::json!({"error": error_message.clone()}),
            };

            // save message
            data.messages.push(message.clone());
            // send message
            utils::send_msg_to_callback_url(data, vec![message], 0, false);

            // setting default step && flow
            data.context.step = ContextStepInfo::Normal("start".to_owned());
            data.context.flow = bot.get_default_flow_name();

            (
                utils::get_flow_by_id(&bot.default_flow, &bot.flows)?,
                ContextStepInfo::Normal("start".to_owned()),
            )
        }
    };

    // update event to flow trigger
    event.content_type = "flow_trigger".to_owned();
    event.content = serde_json::json!({
            "flow_id": flow.id,
            "step_id": step
        }
    );

    // create new conversation for the new client
    data.conversation_id = db::conversation::create(
        &flow.id,
        &step.get_step(),
        &data.client,
        data.ttl.map(|t| Utc::now().naive_utc() + t),
        db,
    )
    .await?;

    let memories = db::memory::get_by_client(&data.client, None, None, db).await?;
    let mut map = serde_json::Map::new();
    for mem in memories {
        if !map.contains_key(&mem.key) {
            map.insert(mem.key, serde_json::json!(mem.value));
        }
    }

    // and get memories of the new bot form db,
    // clearing the permanent memories form scope of the previous bot
    data.context.current = get_hashmap_from_mem(&serde_json::json!(map), &data.context.flow);

    Ok(())
}

#[async_recursion]
async fn check_switch_bot(
    result: Result<
        (
            serde_json::Map<String, serde_json::Value>,
            Option<SwitchBot>,
        ),
        BitpartError,
    >,
    data: &mut ConversationData,
    bot: &mut CsmlBot,
    bot_opt: &mut BotOpt,
    event: &mut Event,
    db: &DatabaseConnection,
) -> Result<serde_json::Map<String, serde_json::Value>, BitpartError> {
    match result {
        Ok((mut messages, Some(next_bot))) => {
            if let Err(err) = switch_bot(data, bot, next_bot, bot_opt, event, db).await {
                // End no interruption delay
                if let Some(_) = bot.no_interruption_delay {
                    db::state::delete(&data.client, "delay", "content", db).await?;
                }
                return Err(err);
            };

            let result = actions::step(data, event.clone(), &bot, db).await;

            let mut new_messages = check_switch_bot(result, data, bot, bot_opt, event, db).await?;

            messages.append(&mut new_messages);

            Ok(messages)
        }
        Ok((messages, None)) => {
            // End no interruption delay
            if let Some(_) = bot.no_interruption_delay {
                db::state::delete(&data.client, "delay", "content", db).await?;
            }

            Ok(messages)
        }
        Err(err) => {
            // End no interruption delay
            if let Some(_) = bot.no_interruption_delay {
                db::state::delete(&data.client, "delay", "content", db).await?;
            }

            Err(err)
        }
    }
}

pub async fn start_conversation(
    request: Request,
    mut bot_opt: BotOpt,
    db: &DatabaseConnection,
) -> Result<serde_json::Map<String, serde_json::Value>, BitpartError> {
    //init_logger();

    let mut formatted_event = Event::try_from(&request)?;
    //let mut db = init_db()?;

    let mut bot = bot_opt.search_bot(db).await?;
    init_bot(&mut bot)?;

    let mut data = init_conversation_info(
        utils::get_default_flow(&bot)?.name.to_owned(),
        &formatted_event,
        &request,
        &bot,
        db,
    )
    .await?;

    //check_for_hold(&mut data, &bot, &mut formatted_event)?;

    /////////// block user event if delay variable si on and delay_time is bigger than current time
    if let Some(delay) = bot.no_interruption_delay {
        if let Ok(delay) = db::state::get(&data.client, "delay", "content", db).await {
            match (delay["delay_value"].as_i64(), delay["timestamp"].as_i64()) {
                (Some(delay), Some(timestamp)) if timestamp + delay >= Utc::now().timestamp() => {
                    return Ok(serde_json::Map::new())
                }
                _ => {}
            }
        }

        let delay: serde_json::Value = serde_json::json!({
            "delay_value": delay,
            "timestamp": Utc::now().timestamp()
        });

        db::state::set(
            &data.client,
            "delay",
            "content",
            &delay,
            data.ttl.map(|t| Utc::now().naive_utc() + t),
            db,
        )
        .await?;
    }
    //////////////////////////////////////

    // save event in db as message RECEIVE
    match (data.low_data, formatted_event.secure) {
        (false, true) => {
            let msgs = vec![serde_json::json!({"content_type": "secure"})];

            db::message::create(&mut data, &msgs, 0, "RECEIVE", None, db).await?;
        }
        (false, false) => {
            let msgs = vec![request.payload.to_owned()];

            db::message::create(&mut data, &msgs, 0, "RECEIVE", None, db).await?;
        }
        (true, _) => {}
    }

    let result = actions::step(&mut data, formatted_event.to_owned(), &bot, db).await;

    check_switch_bot(
        result,
        &mut data,
        &mut bot,
        &mut bot_opt,
        &mut formatted_event,
        db,
    )
    .await
}
