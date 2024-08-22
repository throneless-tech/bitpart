mod actions;
pub mod csml;
mod data;
mod db;
mod entities;
mod error;
mod event;
mod utils;

use csml_interpreter::data::Event;
use data::{BotOpt, Request};
use error::BitpartError;

pub fn start_conversation(
    request: Request,
    mut bot_opt: BotOpt,
) -> Result<serde_json::Map<String, serde_json::Value>, BitpartError> {
    //init_logger();

    let mut formatted_event = Event::try_from(&request)?;
    //let mut db = init_db()?;

    let mut bot = bot_opt.search_bot(&mut db)?;
    init_bot(&mut bot)?;

    let mut data = init_conversation_info(
        get_default_flow(&bot)?.name.to_owned(),
        &formatted_event,
        &request,
        &bot,
        db,
    )?;

    //check_for_hold(&mut data, &bot, &mut formatted_event)?;

    /////////// block user event if delay variable si on and delay_time is bigger than current time
    if let Some(delay) = bot.no_interruption_delay {
        if let Some(delay) = state::get_state_key(&data.client, "delay", "content", &mut data.db)? {
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

        set_state_items(
            &data.client,
            "delay",
            vec![("content", &delay)],
            data.ttl,
            &mut data.db,
        )?;
    }
    //////////////////////////////////////

    // save event in db as message RECEIVE
    match (data.low_data, formatted_event.secure) {
        (false, true) => {
            let msgs = vec![serde_json::json!({"content_type": "secure"})];

            messages::add_messages_bulk(&mut data, msgs, 0, "RECEIVE")?;
        }
        (false, false) => {
            let msgs = vec![request.payload.to_owned()];

            messages::add_messages_bulk(&mut data, msgs, 0, "RECEIVE")?;
        }
        (true, _) => {}
    }

    let result = interpret_step(&mut data, formatted_event.to_owned(), &bot);

    check_switch_bot(
        result,
        &mut data,
        &mut bot,
        &mut bot_opt,
        &mut formatted_event,
    )
}
