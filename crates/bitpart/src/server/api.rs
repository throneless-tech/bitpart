use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use csml_interpreter::{
    data::{CsmlBot, CsmlResult},
    search_for_modules, validate_bot,
};
use presage::libsignal_service::configuration::SignalServers;
// use presage::libsignal_service::prelude::phonenumber::PhoneNumber;
// use presage::model::identity::OnNewIdentity;
// use presage_store_bitpart::{BitpartStore, MigrationConflictStrategy};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use url::Url;
use uuid::Uuid;

//use std::env;

use super::conversation::start_conversation;
use super::data::Request;
use super::db;
use crate::error::BitpartError;

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
    limit: Option<u64>,
    offset: Option<u64>,
}

#[derive(Deserialize)]
pub struct MemoryData {
    key: String,
    value: String,
}

#[derive(Deserialize)]
pub struct ChannelRequest {
    id: String,
    bot_id: String,
}

#[derive(Deserialize)]
pub struct ChannelLinkRequest {
    servers: SignalServers,
    device_name: String,
}

#[derive(Deserialize)]
pub struct ChannelAddRequest {
    url: Url,
}

#[derive(Clone)]
pub struct ApiState {
    pub db: DatabaseConnection,
    pub auth: String,
}

/*
Bot
*/

pub async fn post_bot(
    State(state): State<ApiState>,
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
            println!("Validated!");
            let created = db::bot::create(bot, &state.db).await?;
            Ok((StatusCode::CREATED, Json(created)))
        }
    }
}

pub async fn list_bots(State(state): State<ApiState>) -> Result<impl IntoResponse, BitpartError> {
    let list = db::bot::list(None, None, &state.db).await?;
    Ok((StatusCode::OK, Json(list)).into_response())
}

pub async fn get_bot(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, BitpartError> {
    if let Some(bot) = db::bot::get_latest_by_bot_id(&id.to_string(), &state.db).await? {
        Ok((StatusCode::OK, Json(bot)).into_response())
    } else {
        let response = Ok((StatusCode::NOT_FOUND, ()).into_response());
        response
    }
}

pub async fn delete_bot(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, BitpartError> {
    db::bot::delete_by_bot_id(&id.to_string(), &state.db).await
}

pub async fn get_bot_versions(
    Path(id): Path<String>,
    Query(params): Query<QueryPagination>,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, BitpartError> {
    match db::bot::get(&id.to_string(), params.limit, params.offset, &state.db).await {
        Ok(v) if v.len() > 0 => {
            println!("v: {:?}", v);
            Ok((StatusCode::OK, Json(v)).into_response())
        }
        _ => Ok((StatusCode::NOT_FOUND, ()).into_response()),
    }
}

pub async fn get_bot_version(
    Path((_, vid)): Path<(String, Uuid)>,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, BitpartError> {
    let bot = db::bot::get_by_id(&vid.to_string(), &state.db).await?;
    Ok((StatusCode::FOUND, Json(bot)))
}

pub async fn delete_bot_version(
    Path((_, vid)): Path<(String, Uuid)>,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, BitpartError> {
    db::bot::delete_by_id(&vid.to_string(), &state.db).await
}

#[cfg(test)]
mod test_bot {
    use axum::{
        routing::{delete, get, post},
        Router,
    };

    use crate::server::data::BotVersion;
    use crate::utils::get_test_server;

    use super::*;

    #[tokio::test]
    async fn it_should_create_a_bot() {
        let app = Router::new().route("/bots", post(post_bot));
        let server = get_test_server(app).await;

        server
            .post("/bots")
            .json(&json!({
                "id": "bot_id",
                "name": "test",
                "flows": [
                  {
                    "id": "Default",
                    "name": "Default",
                    "content": "start: say \"Hello\" goto end",
                    "commands": [],
                  }
                ],
                "default_flow": "Default",
            }))
            .await
            .assert_status_success();
    }

    #[tokio::test]
    async fn it_should_get_a_bot() {
        let app = Router::new()
            .route("/bots", post(post_bot))
            .route("/bots/:id", get(get_bot));
        let server = get_test_server(app).await;

        let response: BotVersion = server
            .post("/bots")
            .json(&json!({
                "id": "bot_id",
                "name": "test",
                "flows": [
                  {
                    "id": "Default",
                    "name": "Default",
                    "content": "start: say \"Hello\" goto end",
                    "commands": [],
                  }
                ],
                "default_flow": "Default",
            }))
            .await
            .json();

        let bot_id = response.bot.id;
        let path = format!("/bots/{bot_id}");

        server.get(&path).await.assert_status_success();
    }

    #[tokio::test]
    async fn it_should_delete_a_bot() {
        let app = Router::new()
            .route("/bots", post(post_bot))
            .route("/bots/:id", get(get_bot_versions))
            .route("/bots/:id", delete(delete_bot));
        let server = get_test_server(app).await;

        let response: BotVersion = server
            .post("/bots")
            .json(&json!({
                "id": "bot_id",
                "name": "test",
                "flows": [
                  {
                    "id": "Default",
                    "name": "Default",
                    "content": "start: say \"Hello\" goto end",
                    "commands": [],
                  }
                ],
                "default_flow": "Default",
            }))
            .await
            .json();

        let bot_id = response.bot.id;
        let path = format!("/bots/{bot_id}");

        server.get(&path).await.assert_status_success();
        server.delete(&path).await;
        server.get(&path).await.assert_status_not_found();
    }

    #[tokio::test]
    async fn it_should_get_multiple_versions() {
        let app = Router::new()
            .route("/bots", post(post_bot))
            .route("/bots/:id/versions", get(get_bot_versions));
        let server = get_test_server(app).await;

        server
            .post("/bots")
            .json(&json!({
                "id": "bot_id",
                "name": "test",
                "flows": [
                  {
                    "id": "Default",
                    "name": "Default",
                    "content": "start: say \"Hello\" goto end",
                    "commands": [],
                  }
                ],
                "default_flow": "Default",
            }))
            .await;

        server
            .post("/bots")
            .json(&json!({
                "id": "bot_id",
                "name": "test",
                "flows": [
                  {
                    "id": "Default",
                    "name": "Default",
                    "content": "start: say \"Hello\" goto end",
                    "commands": [],
                  }
                ],
                "default_flow": "Default",
            }))
            .await;

        let path = "/bots/bot_id/versions";

        let response: Vec<BotVersion> = server.get(&path).await.json();
        assert!(response.len() == 2);
    }
}

/*
Request
*/

pub async fn post_request(
    State(state): State<ApiState>,
    Json(body): Json<Request>,
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

    match start_conversation(request, bot_opt, &state.db).await {
        Ok(r) => Ok((StatusCode::OK, Json(r)).into_response()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod test_request {
    use super::*;
    use crate::utils::get_test_server;
    use axum::{routing::post, Router};

    #[tokio::test]
    async fn it_should_post_request() {
        let app = Router::new()
            .route("/request", post(post_request))
            .route("/bots", post(post_bot));
        let server = get_test_server(app).await;

        // server
        //     .post("/bots")
        //     .json(&json!({
        //         "id": "bot_id",
        //         "name": "test",
        //         "flows": [
        //           {
        //             "id": "Default",
        //             "name": "Default",
        //             "content": "start: say \"Hello\" goto end",
        //             "commands": [],
        //           }
        //         ],
        //         "default_flow": "Default",
        //     }))
        //     .await;

        let result = server
            .post("/request")
            .json(&json!({
                "bot": {
                    "id": "test_run",
                    "name": "test_run",
                    "flows": [
                        {
                                "id": "Default",
                                "name": "Default",
                                "content": "start: say \"Hello\" goto end",
                                "commands": [],
                              }
                            ],
                            "default_flow": "Default",
                        },
                        "event": {
                            "id": "request_id",
                            "client": {
                                "user_id": "user_id",
                                "channel_id": "channel_id",
                                "bot_id": "test_run"
                            },
                            "payload": {
                              "content_type": "text" ,
                              "content": {
                                "text": "toto"
                              }
                            },
                            "metadata": Value::Null,
                }
            }))
            .await;
        result.assert_status_ok();
    }
}

/*
Runners
*/
pub async fn post_runner(
    State(state): State<ApiState>,
    Json(body): Json<ChannelRequest>,
) -> Result<impl IntoResponse, BitpartError> {
    db::runner::create(&body.id, &body.bot_id, &state.db).await
}

pub async fn get_runner(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, BitpartError> {
    let runner = db::runner::get_by_id(&id, &state.db).await?;
    Ok((StatusCode::FOUND, Json(runner)))
}

pub async fn get_runners(
    Query(params): Query<QueryClientPagination>,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, BitpartError> {
    match db::runner::list(params.limit, params.offset, &state.db).await {
        Ok(v) if v.len() > 0 => Ok((StatusCode::FOUND, serde_json::to_string(&v)?).into_response()),
        _ => Ok((StatusCode::NOT_FOUND, ()).into_response()),
    }
}

pub async fn delete_runner(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, BitpartError> {
    db::runner::delete_by_id(&id, &state.db).await
}

// pub async fn link_device_channel(
//     Path(id): Path<String>,
//     State(state): State<ApiState>,
//     Json(body): Json<ChannelLinkRequest>,
// ) -> Result<impl IntoResponse, BitpartError> {
//     let config_store = BitpartStore::open(
//         &id,
//         &state.db,
//         MigrationConflictStrategy::Raise,
//         OnNewIdentity::Trust,
//     )
//     .await?;
//     signal::link_device(config_store, body.servers, body.device_name)
//         .await
//         .map(|url| url.to_string())
//         .map_err(|e| BitpartError::Signal(e))
// }

// pub async fn add_device_channel(
//     Path(id): Path<String>,
//     State(state): State<ApiState>,
//     Json(body): Json<ChannelAddRequest>,
// ) -> Result<impl IntoResponse, BitpartError> {
//     let config_store = BitpartStore::open(
//         &id,
//         &state.db,
//         MigrationConflictStrategy::Raise,
//         OnNewIdentity::Trust,
//     )
//     .await?;
//     signal::add_device(config_store, body.url)
//         .await
//         .map_err(|e| BitpartError::Signal(e))
// }
