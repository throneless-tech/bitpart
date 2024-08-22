use crate::db_connectors::{conversations::*, memories::*, state};
use crate::interpreter_actions::SwitchBot;
use crate::{
    data::{ConversationInfo, CsmlRequest, Database, EngineError},
    utils::{
        get_default_flow, get_flow_by_id, get_low_data_mode_value, get_ttl_duration_value,
        search_flow, send_msg_to_callback_url,
    },
    BotOpt, Context, CsmlBot, CsmlFlow, CsmlResult,
};

use csml_interpreter::data::context::ContextStepInfo;
use csml_interpreter::{
    data::{
        ast::Flow,
        context::{get_hashmap_from_json, get_hashmap_from_mem},
        ApiInfo, Client, Event, Message, PreviousBot,
    },
    load_components, search_for_modules, validate_bot,
};

use std::collections::HashMap;

pub fn init_conversation_info<'a>(
    default_flow: String,
    event: &Event,
    request: &'a CsmlRequest,
    bot: &'a CsmlBot,
    mut db: Database,
) -> Result<ConversationInfo, EngineError> {
    // Create a new interaction. An interaction is basically each request,
    // initiated from the bot or the user.

    let mut context = init_context(
        default_flow,
        request.client.clone(),
        &bot.apps_endpoint,
        &mut db,
    );
    let ttl = get_ttl_duration_value(Some(event));
    let low_data = get_low_data_mode_value(event);

    // Do we have a flow matching the request? If the user is requesting a flow in one way
    // or another, this takes precedence over any previously open conversation
    // and a new conversation is created with the new flow as a starting point.
    let flow_found = search_flow(event, &bot, &request.client, &mut db).ok();
    let conversation_id = get_or_create_conversation(
        &mut context,
        &bot,
        flow_found,
        &request.client,
        ttl,
        &mut db,
    )?;

    context.metadata = get_hashmap_from_json(&request.metadata, &context.flow);
    context.current = get_hashmap_from_mem(
        &internal_use_get_memories(&request.client, &mut db)?,
        &context.flow,
    );

    let mut data = ConversationInfo {
        conversation_id,
        context,
        metadata: request.metadata.clone(), // ??
        request_id: request.request_id.clone(),
        callback_url: request.callback_url.clone(),
        client: request.client.clone(),
        messages: vec![],
        ttl,
        low_data,
        db,
    };

    let flow = data.context.flow.to_owned();
    let step = data.context.step.to_owned();

    // Now that everything is correctly setup, update the conversation with wherever
    // we are now and continue with the rest of the request!
    update_conversation(&mut data, Some(flow), Some(step.get_step()))?;

    Ok(data)
}
