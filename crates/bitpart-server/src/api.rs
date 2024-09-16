use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};

use csml_interpreter::{
    data::{Client, CsmlBot, CsmlResult},
    search_for_modules, validate_bot,
};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

//use std::env;

use crate::{
    data::RunRequest,
    db::{self, entities::conversation},
    error::BitpartError,
    start_conversation,
};

#[derive(Deserialize)]
pub struct QueryPagination {
    limit: Option<u64>,
    offset: Option<u64>,
}

#[derive(Deserialize)]
pub struct QueryClient {
    user_id: String,
    bot_id: Uuid,
    channel_id: String,
}

#[derive(Deserialize)]
pub struct QueryClientPagination {
    user_id: String,
    bot_id: Uuid,
    channel_id: String,
    status: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

#[derive(Deserialize)]
pub struct MemoryData {
    key: String,
    value: String,
}

/*
Bot
*/

pub async fn post_bot(
    State(db): State<DatabaseConnection>,
    Json(mut bot): Json<CsmlBot>,
) -> Result<impl IntoResponse, BitpartError> {
    if let Err(err) = search_for_modules(&mut bot) {
        return Err(BitpartError::Interpreter(format!("{:?}", err)));
    }

    match validate_bot(&bot) {
        CsmlResult {
            errors: Some(errors),
            ..
        } => Err(BitpartError::Interpreter(format!("{:?}", errors))),
        CsmlResult { .. } => {
            let created = db::bot::create(bot, &db).await?;
            Ok((StatusCode::CREATED, serde_json::to_string(&created)?))
        }
    }
}

pub async fn get_bot(
    Path(id): Path<Uuid>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    let bot = db::bot::get_latest_by_bot_id(&id.to_string(), &db).await?;
    Ok((StatusCode::FOUND, serde_json::to_string(&bot)?))
}

pub async fn delete_bot(
    Path(id): Path<Uuid>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    db::bot::delete_by_bot_id(&id.to_string(), &db).await
}

pub async fn get_bot_versions(
    Path(id): Path<Uuid>,
    Query(params): Query<QueryPagination>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    match db::bot::get(&id.to_string(), params.limit, params.offset, &db).await {
        Ok(v) if v.len() > 0 => Ok((StatusCode::FOUND, serde_json::to_string(&v)?).into_response()),
        _ => Ok((StatusCode::NOT_FOUND, ()).into_response()),
    }
}

pub async fn get_bot_version(
    Path((_, vid)): Path<(Uuid, Uuid)>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    let bot = db::bot::get_by_id(&vid.to_string(), &db).await?;
    Ok((StatusCode::FOUND, serde_json::to_string(&bot)?))
}

pub async fn delete_bot_version(
    Path((_, vid)): Path<(Uuid, Uuid)>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    db::bot::delete_by_id(&vid.to_string(), &db).await
}

/*
Conversations
*/

pub async fn get_conversations(
    Query(params): Query<QueryClientPagination>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    let client = Client {
        bot_id: params.bot_id.to_string(),
        channel_id: params.channel_id,
        user_id: params.user_id,
    };

    match db::conversation::get_by_client(&client, params.limit, params.offset, &db).await {
        Ok(v) if v.len() > 0 => Ok((StatusCode::FOUND, serde_json::to_string(&v)?).into_response()),
        _ => Ok((StatusCode::NOT_FOUND, ()).into_response()),
    }
}

// pub async fn patch_conversation(
//     Query(params): Query<QueryClient>,
//     State(db): State<DatabaseConnection>,
//     Json(body): Json<conversation::Model>,
// ) -> Result<impl IntoResponse, BitpartError> {
//     let client = Client {
//         bot_id: params.bot_id.to_string(),
//         channel_id: params.channel_id,
//         user_id: params.user_id,
//     };
//     db::conversation::set_status_by_client(&client, &body.status, &db).await
// }

/*
Memories
*/

pub async fn post_memory(
    Query(params): Query<QueryClient>,
    State(db): State<DatabaseConnection>,
    Json(body): Json<MemoryData>,
) -> Result<impl IntoResponse, BitpartError> {
    let client = Client {
        bot_id: params.bot_id.to_string(),
        channel_id: params.channel_id,
        user_id: params.user_id,
    };
    db::memory::create(&client, &body.key, &body.value, None, &db).await
}

pub async fn get_memories(
    Query(params): Query<QueryClientPagination>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    let client = Client {
        bot_id: params.bot_id.to_string(),
        channel_id: params.channel_id,
        user_id: params.user_id,
    };
    match db::memory::get_by_client(&client, params.limit, params.offset, &db).await {
        Ok(v) if v.len() > 0 => Ok((StatusCode::FOUND, serde_json::to_string(&v)?).into_response()),
        _ => Ok((StatusCode::NOT_FOUND, ()).into_response()),
    }
}

pub async fn get_memory(
    Path(id): Path<String>,
    Query(params): Query<QueryClient>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    let client = Client {
        bot_id: params.bot_id.to_string(),
        channel_id: params.channel_id,
        user_id: params.user_id,
    };
    match db::memory::get(&client, &id, &db).await? {
        Some(mem) => Ok((StatusCode::FOUND, serde_json::to_string(&mem)?).into_response()),
        None => Ok((StatusCode::NOT_FOUND, ()).into_response()),
    }
}

pub async fn delete_memory(
    Path(id): Path<String>,
    Query(params): Query<QueryClient>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    let client = Client {
        bot_id: params.bot_id.to_string(),
        channel_id: params.channel_id,
        user_id: params.user_id,
    };
    db::memory::delete(&client, &id, &db).await
}

pub async fn delete_memories(
    Query(params): Query<QueryClient>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    let client = Client {
        bot_id: params.bot_id.to_string(),
        channel_id: params.channel_id,
        user_id: params.user_id,
    };
    db::memory::delete_by_client(&client, &db).await
}

/*
Messages
*/

pub async fn get_messages(
    Query(params): Query<QueryClientPagination>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    let client = Client {
        bot_id: params.bot_id.to_string(),
        channel_id: params.channel_id,
        user_id: params.user_id,
    };

    match db::memory::get_by_client(&client, params.limit, params.offset, &db).await {
        Ok(v) if v.len() > 0 => Ok((StatusCode::FOUND, serde_json::to_string(&v)?).into_response()),
        _ => Ok((StatusCode::NOT_FOUND, ()).into_response()),
    }
}

/*
State
*/

pub async fn get_state(
    Query(params): Query<QueryClient>,
    State(db): State<DatabaseConnection>,
) -> Result<impl IntoResponse, BitpartError> {
    let client = Client {
        bot_id: params.bot_id.to_string(),
        channel_id: params.channel_id,
        user_id: params.user_id,
    };

    match db::state::get_by_client(&client, &db).await {
        Ok(v) if v.len() > 0 => Ok((StatusCode::FOUND, serde_json::to_string(&v)?).into_response()),
        _ => Ok((StatusCode::NOT_FOUND, ()).into_response()),
    }
}

/*
Request
*/

pub async fn post_request(
    State(db): State<DatabaseConnection>,
    Json(body): Json<RunRequest>,
) -> Result<impl IntoResponse, BitpartError> {
    let mut request = body.event.to_owned();

    let bot_opt = match body.get_bot_opt() {
        Ok(bot_opt) => bot_opt,
        _ => return Ok((StatusCode::BAD_REQUEST, ()).into_response()),
    };

    // request metadata should be an empty object by default
    request.metadata = match request.metadata {
        Value::Null => json!({}),
        val => val,
    };

    match start_conversation(request, bot_opt, &db).await {
        Ok(r) => Ok((StatusCode::OK, serde_json::to_string(&r)?).into_response()),
        Err(err) => Err(err),
    }
}
