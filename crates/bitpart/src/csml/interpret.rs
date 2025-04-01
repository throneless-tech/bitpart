// Bitpart
// Copyright (C) 2025 Throneless Tech
//
// This code is derived in part from code from the CSML project:
// Copyright (C) 2020 CSML

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use chrono::Utc;
use csml_interpreter::csml_logs::LogLvl;
use csml_interpreter::data::{
    Client, CsmlBot, CsmlFlow, Hold, MSG, Memory, Message, MultiBot, ast::ForgetMemory,
    context::ContextStepInfo, event::Event,
};
use csml_interpreter::interpret;
use sea_orm::DatabaseConnection;
use serde_json::{Value, map::Map};
use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use tracing::{debug, error, info, trace, warn};

use super::data::{ConversationData, SwitchBot};
use super::utils::{
    get_current_step_hash, get_flow_by_id, messages_formatter, send_msg_to_callback_url,
    update_current_context,
};
use crate::db;
use crate::error::BitpartError;

#[derive(Debug, Clone)]
enum InterpreterReturn {
    Continue,
    End,
    SwitchBot(SwitchBot),
}

pub async fn step(
    data: &mut ConversationData,
    event: Event,
    bot: &CsmlBot,
    db: &DatabaseConnection,
) -> Result<(Map<String, Value>, Option<SwitchBot>), BitpartError> {
    let mut current_flow: &CsmlFlow = get_flow_by_id(&data.context.flow, &bot.flows)?;
    let mut interaction_order = 0;
    let mut conversation_end = false;
    let (sender, receiver) = mpsc::channel::<MSG>();
    let context = data.context.clone();
    let mut switch_bot = None;
    info!(
        flow = data.context.flow.to_string(),
        "interpreter: start interpretations of bot {:?}", bot.id
    );
    debug!(
        bot_id = data.client.bot_id.to_string(),
        user_id = data.client.user_id.to_string(),
        channel_id = data.client.channel_id.to_string(),
        flow = data.context.flow.to_string(),
        "interpreter: start interpretations of bot {:?}, with ",
        bot.id
    );
    let new_bot = bot.clone();
    thread::spawn(move || {
        interpret(new_bot, context, event, Some(sender));
    });

    let mut memories = HashMap::new();

    for received in receiver {
        match received {
            MSG::Remember(mem) => {
                memories.insert(mem.key.clone(), mem);
            }
            MSG::Forget(mem) => match mem {
                ForgetMemory::ALL => {
                    memories.clear();
                    db::memory::delete_by_client(&data.client, db).await?;
                }
                ForgetMemory::SINGLE(memory) => {
                    memories.remove(&memory.ident);
                    db::memory::delete(&data.client, &memory.ident, db).await?;
                }
                ForgetMemory::LIST(mem_list) => {
                    for mem in mem_list.iter() {
                        memories.remove(&mem.ident);
                        db::memory::delete(&data.client, &mem.ident, db).await?;
                    }
                }
            },
            MSG::Message(msg) => {
                info!(flow = data.context.flow.to_string(), "sending message");
                debug!(
                    bot_id = data.client.bot_id.to_string(),
                    user_id = data.client.user_id.to_string(),
                    channel_id = data.client.channel_id.to_string(),
                    flow = data.context.flow.to_string(),
                    "sending message {:?}",
                    msg
                );

                debug!("CONTEXT {:?}", data.context);
                send_msg_to_callback_url(data, vec![msg.clone()], interaction_order, false);
                data.messages.push(msg);
            }
            MSG::Shout(msg) => {
                info!(flow = data.context.flow.to_string(), "sending message");
                debug!(
                    bot_id = data.client.bot_id.to_string(),
                    user_id = data.client.user_id.to_string(),
                    channel_id = data.client.channel_id.to_string(),
                    flow = data.context.flow.to_string(),
                    "sending message {:?}",
                    msg
                );

                debug!("CONTEXT {:?}", data.context);

                send_msg_to_callback_url(data, vec![msg.clone()], interaction_order, false);

                let convos =
                    db::conversation::get_by_bot_id(&data.client.bot_id, None, None, db).await?;

                for c in convos.iter() {
                    let mut msg_copy = msg.clone();
                    if let Value::Object(ref mut content) = msg_copy.content {
                        content.insert(
                            "client".to_owned(),
                            serde_json::json!({ "bot_id": c.bot_id, "user_id": c.user_id, "channel_id": c.channel_id }),
                        );
                    };

                    data.messages.push(msg_copy);
                }
            }
            MSG::Log {
                flow,
                line,
                message,
                log_lvl,
            } => {
                match log_lvl {
                    LogLvl::Error => error!(
                        bot_id = data.client.bot_id.to_string(),
                        user_id = data.client.user_id.to_string(),
                        channel_id = data.client.channel_id.to_string(),
                        flow,
                        line,
                        message
                    ),
                    LogLvl::Warn => warn!(
                        bot_id = data.client.bot_id.to_string(),
                        user_id = data.client.user_id.to_string(),
                        channel_id = data.client.channel_id.to_string(),
                        flow,
                        line,
                        message
                    ),
                    LogLvl::Info => info!(
                        bot_id = data.client.bot_id.to_string(),
                        user_id = data.client.user_id.to_string(),
                        channel_id = data.client.channel_id.to_string(),
                        flow,
                        line,
                        message
                    ),
                    LogLvl::Debug => debug!(
                        bot_id = data.client.bot_id.to_string(),
                        user_id = data.client.user_id.to_string(),
                        channel_id = data.client.channel_id.to_string(),
                        flow,
                        line,
                        message
                    ),
                    LogLvl::Trace => trace!(
                        bot_id = data.client.bot_id.to_string(),
                        user_id = data.client.user_id.to_string(),
                        channel_id = data.client.channel_id.to_string(),
                        flow,
                        line,
                        message
                    ),
                };
            }
            MSG::Hold(Hold {
                index,
                step_vars,
                step_name,
                flow_name,
                previous,
                secure,
            }) => {
                let hash = get_current_step_hash(&data.context, bot)?;
                let state_hold: Value = serde_json::json!({
                    "index": index,
                    "step_vars": step_vars,
                    "hash": hash,
                    "previous": previous,
                    "secure": secure
                });
                info!(flow = data.context.flow.to_string(), "hold bot");
                debug!(
                    bot_id = data.client.bot_id.to_string(),
                    user_id = data.client.user_id.to_string(),
                    channel_id = data.client.channel_id.to_string(),
                    flow = data.context.flow.to_string(),
                    "hold bot, state_hold {:?}",
                    state_hold
                );

                db::state::set(
                    &data.client,
                    "hold",
                    "position",
                    &state_hold,
                    data.ttl.map(|t| Utc::now().naive_utc() + t),
                    db,
                )
                .await?;
                data.context.hold = Some(Hold {
                    index,
                    step_vars,
                    step_name,
                    flow_name,
                    previous,
                    secure,
                });
            }
            MSG::Next {
                flow,
                step,
                bot: None,
            } => {
                if let Ok(InterpreterReturn::End) = manage_internal_goto(
                    data,
                    &mut conversation_end,
                    &mut interaction_order,
                    &mut current_flow,
                    bot,
                    &mut memories,
                    flow,
                    step,
                    db,
                )
                .await
                {
                    break;
                }
            }

            MSG::Next {
                flow,
                step,
                bot: Some(target_bot),
            } => {
                if let Ok(InterpreterReturn::SwitchBot(s_bot)) = manage_switch_bot(
                    data,
                    &mut interaction_order,
                    bot,
                    flow,
                    step,
                    target_bot,
                    db,
                )
                .await
                {
                    switch_bot = Some(s_bot);
                    break;
                }
            }

            MSG::Error(err_msg) => {
                conversation_end = true;
                error!(
                    bot_id = data.client.bot_id.to_string(),
                    user_id = data.client.user_id.to_string(),
                    channel_id = data.client.channel_id.to_string(),
                    flow = data.context.flow.to_string(),
                    "interpreter error: {:?}",
                    err_msg
                );

                send_msg_to_callback_url(data, vec![err_msg.clone()], interaction_order, true);
                data.messages.push(err_msg);
                db::conversation::set_status_by_id(&data.conversation_id, "CLOSED", db).await?;
            }
        }
    }

    // save in db
    let msgs: Vec<serde_json::Value> = data
        .messages
        .iter()
        .map(|var| var.clone().message_to_json())
        .collect();

    if !data.low_data {
        db::message::create(data, &msgs, interaction_order, "SEND", None, db).await?;
    }

    db::memory::create_many(&data.client, &memories, None, db).await?;

    Ok((
        messages_formatter(
            data,
            data.messages.clone(),
            interaction_order,
            conversation_end,
        ),
        switch_bot,
    ))
}

async fn manage_switch_bot(
    data: &mut ConversationData,
    interaction_order: &mut i32,
    bot: &CsmlBot,
    flow: Option<String>,
    step: Option<ContextStepInfo>,
    target_bot: String,
    db: &DatabaseConnection,
) -> Result<InterpreterReturn, BitpartError> {
    // check if we are allow to switch to 'target_bot'

    let next_bot = if let Some(multibot) = &bot.multibot {
        multibot.iter().find(
            |&MultiBot {
                 id,
                 name,
                 version_id: _,
             }| match name {
                Some(name) => target_bot == *id || target_bot == *name,
                None => target_bot == *id,
            },
        )
    } else {
        None
    };

    let next_bot = match next_bot {
        Some(next_bot) => next_bot,
        None => {
            let error_message = format!("Switching to Bot: ({}) is not allowed", target_bot);
            // send message
            send_msg_to_callback_url(
                data,
                vec![Message {
                    content_type: "error".to_owned(),
                    content: serde_json::json!({
                        "error": error_message.clone()
                    }),
                }],
                *interaction_order,
                true,
            );

            error!(
                flow = data.context.flow.to_string(),
                message = error_message
            );
            return Ok(InterpreterReturn::End);
        }
    };

    let (flow, step) = match (flow, step) {
        (Some(flow), Some(step)) => {
            info!(
                bot_id = data.client.bot_id.to_string(),
                user_id = data.client.user_id.to_string(),
                channel_id = data.client.channel_id.to_string(),
                flow = data.context.flow.to_string(),
                "goto step: {:?}",
                data.context.step.get_step()
            );

            (Some(flow), step)
        }
        (Some(flow), None) => {
            info!(
                bot_id = data.client.bot_id.to_string(),
                user_id = data.client.user_id.to_string(),
                channel_id = data.client.channel_id.to_string(),
                flow = data.context.flow.to_string(),
                "goto step: {:?}",
                data.context.step.get_step()
            );

            (Some(flow), ContextStepInfo::Normal("start".to_owned()))
        }
        (None, Some(step)) => {
            info!(
                bot_id = data.client.bot_id.to_string(),
                user_id = data.client.user_id.to_string(),
                channel_id = data.client.channel_id.to_string(),
                flow = data.context.flow.to_string(),
                "goto step: {:?}",
                data.context.step.get_step()
            );

            (None, step)
        }
        (None, None) => {
            info!(
                bot_id = data.client.bot_id.to_string(),
                user_id = data.client.user_id.to_string(),
                channel_id = data.client.channel_id.to_string(),
                flow = data.context.flow.to_string(),
                "goto step: {:?}",
                data.context.step.get_step()
            );

            (None, ContextStepInfo::Normal("start".to_owned()))
        }
    };

    let message = Message::switch_bot_message(&next_bot.id, &data.client);
    // save message
    data.messages.push(message.clone());
    // send message switch bot
    send_msg_to_callback_url(data, vec![message], *interaction_order, true);

    info!(flow = data.context.flow.to_string(), "switch bot");

    db::conversation::set_status_by_id(&data.conversation_id, "CLOSED", db).await?;

    let previous_bot: Value = serde_json::json!({
        "bot": data.client.bot_id,
        "flow": data.context.flow,
        "step": data.context.step,
    });

    db::state::set(
        &Client::new(
            next_bot.id.to_owned(),
            data.client.channel_id.clone(),
            data.client.user_id.clone(),
        ),
        "bot",
        "previous",
        &previous_bot,
        data.ttl.map(|t| Utc::now().naive_utc() + t),
        db,
    )
    .await?;

    Ok(InterpreterReturn::SwitchBot(SwitchBot {
        bot_id: next_bot.id.to_owned(),
        version_id: next_bot.version_id.to_owned(),
        flow,
        step: step.get_step(),
    }))
}

async fn manage_internal_goto<'a>(
    data: &mut ConversationData,
    conversation_end: &mut bool,
    interaction_order: &mut i32,
    current_flow: &mut &'a CsmlFlow,
    bot: &'a CsmlBot,
    memories: &mut HashMap<String, Memory>,
    flow: Option<String>,
    step: Option<ContextStepInfo>,
    db: &DatabaseConnection,
) -> Result<InterpreterReturn, BitpartError> {
    match (flow, step) {
        (Some(flow), Some(step)) => {
            debug!(
                bot_id = data.client.bot_id.to_string(),
                user_id = data.client.user_id.to_string(),
                channel_id = data.client.channel_id.to_string(),
                flow = data.context.flow.to_string(),
                "goto step: {:?}",
                data.context.step.get_step()
            );

            update_current_context(data, memories);
            goto_flow(data, interaction_order, current_flow, bot, flow, step, db).await?
        }
        (Some(flow), None) => {
            debug!(
                bot_id = data.client.bot_id.to_string(),
                user_id = data.client.user_id.to_string(),
                channel_id = data.client.channel_id.to_string(),
                flow = data.context.flow.to_string(),
                "goto step: {:?}",
                data.context.step.get_step()
            );

            update_current_context(data, memories);
            let step = ContextStepInfo::Normal("start".to_owned());

            goto_flow(data, interaction_order, current_flow, bot, flow, step, db).await?
        }
        (None, Some(step)) => {
            debug!(
                bot_id = data.client.bot_id.to_string(),
                user_id = data.client.user_id.to_string(),
                channel_id = data.client.channel_id.to_string(),
                flow = data.context.flow.to_string(),
                "goto step: {:?}",
                data.context.step.get_step()
            );

            if goto_step(data, conversation_end, interaction_order, step, db).await? {
                return Ok(InterpreterReturn::End);
            }
        }
        (None, None) => {
            debug!(
                bot_id = data.client.bot_id.to_string(),
                user_id = data.client.user_id.to_string(),
                channel_id = data.client.channel_id.to_string(),
                flow = data.context.flow.to_string(),
                "goto end: {:?}",
                data.context.step.get_step()
            );

            let step = ContextStepInfo::Normal("end".to_owned());
            if goto_step(data, conversation_end, interaction_order, step, db).await? {
                return Ok(InterpreterReturn::End);
            }
        }
    }

    Ok(InterpreterReturn::Continue)
}

/**
 * CSML `goto flow` action
 */
async fn goto_flow<'a>(
    data: &mut ConversationData,
    interaction_order: &mut i32,
    current_flow: &mut &'a CsmlFlow,
    bot: &'a CsmlBot,
    nextflow: String,
    nextstep: ContextStepInfo,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    *current_flow = get_flow_by_id(&nextflow, &bot.flows)?;
    data.context.flow = nextflow;
    data.context.step = nextstep;

    db::conversation::update(
        &data.conversation_id,
        Some(current_flow.id.clone()),
        Some(data.context.step.get_step()),
        db,
    )
    .await?;

    *interaction_order += 1;

    Ok(())
}

/**
 * CSML `goto step` action
 */
async fn goto_step(
    data: &mut ConversationData,
    conversation_end: &mut bool,
    interaction_order: &mut i32,
    nextstep: ContextStepInfo,
    db: &DatabaseConnection,
) -> Result<bool, BitpartError> {
    if nextstep.is_step("end") {
        *conversation_end = true;

        // send end of conversation
        send_msg_to_callback_url(data, vec![], *interaction_order, *conversation_end);
        db::conversation::set_status_by_id(&data.conversation_id, "CLOSED", db).await?;

        // break interpret_step loop
        return Ok(*conversation_end);
    } else {
        data.context.step = nextstep;
        db::conversation::update(
            &data.conversation_id,
            None,
            Some(data.context.step.get_step()),
            db,
        )
        .await?;
    }

    *interaction_order += 1;
    Ok(false)
}
