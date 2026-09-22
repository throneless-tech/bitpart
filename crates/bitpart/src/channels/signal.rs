// Bitpart
// Copyright (C) 2025 Throneless Tech
//
// This code is derived in part from code from the Presage project:
// Copyright (C) 2024 Gabriel Féron

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

use base64::prelude::{BASE64_STANDARD, Engine};
use bitpart_common::{
    csml::{Request, SerializedEvent},
    error::{BitpartErrorKind, Result},
};
use bitpart_csml::data::Client;
use futures::StreamExt;
use futures::{channel::oneshot, pin_mut};
use presage::libsignal_service::configuration::SignalServers;
use presage::libsignal_service::content::Reaction;
use presage::libsignal_service::prelude::ProtobufMessage;
use presage::libsignal_service::prelude::Uuid;
use presage::libsignal_service::proto::AttachmentPointer;
use presage::libsignal_service::proto::data_message::{Flags, Quote};
use presage::libsignal_service::proto::sync_message::{Content as SyncContent, Sent};
use presage::libsignal_service::protocol::ServiceId;
use presage::libsignal_service::zkgroup::GroupMasterKeyBytes;
use presage::model::identity::OnNewIdentity;
use presage::model::messages::Received;
use presage::proto::EditMessage;
use presage::proto::ReceiptMessage;
use presage::proto::SyncMessage;
use presage::proto::receipt_message;
use presage::store::ContentExt;
use presage::{
    Manager,
    libsignal_service::content::{Content, ContentBody, DataMessage, GroupContextV2},
    manager::Registered,
    store::{Store, Thread},
};
use presage_store_bitpart::BitpartStore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cell::Cell;
use std::time::UNIX_EPOCH;
use tokio::{
    runtime::Builder as TokioBuilder,
    sync::{mpsc, oneshot as tokio_oneshot},
    task::{LocalSet, spawn_local},
    time::{Duration, sleep},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::warn;
use tracing::{debug, error, info};
use uuid;

use crate::api;
use crate::db;
use crate::metrics::{BotMetrics, ConnectionStatus, MetricsRegistry};
use std::sync::Arc;

// === manager + dispatch ===

#[derive(Serialize, Deserialize)]
pub enum ChannelMessageContents {
    LinkChannel { id: String, device_name: String },
    StartChannel { id: String },
    ResetSessions { id: String },
}

pub struct ChannelMessage {
    pub msg: ChannelMessageContents,
    pub pool: bitpart_common::db::Pool,
    pub token: CancellationToken,
    pub tracker: TaskTracker,
    pub sender: tokio_oneshot::Sender<String>,
    pub metrics: MetricsRegistry,
}

const CHANNEL_MESSAGE_BUFFER: usize = 32;

#[async_trait::async_trait]
pub trait ChannelBackend: Send + Sync {
    async fn send(&self, msg: ChannelMessage) -> Result<()>;
}

#[derive(Clone)]
pub struct SignalManager {
    inner: mpsc::Sender<ChannelMessage>,
}

impl Default for SignalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalManager {
    pub fn new() -> Self {
        let (send, mut recv) = mpsc::channel(CHANNEL_MESSAGE_BUFFER);

        let rt = TokioBuilder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create thread builder");

        let _ = std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                let local = LocalSet::new();

                local.spawn_local(async move {
                    while let Some(msg) = recv.recv().await {
                        tokio::task::spawn_local(process_channel_message(msg));
                    }
                });

                rt.block_on(local);
            });

        Self { inner: send }
    }
}

#[async_trait::async_trait]
impl ChannelBackend for SignalManager {
    async fn send(&self, msg: ChannelMessage) -> Result<()> {
        self.inner
            .send(msg)
            .await
            .map_err(|_| BitpartErrorKind::Signal("SignalManager has shut down".to_owned()))?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ChannelState {
    id: String,
    channel_id: String,
    pool: bitpart_common::db::Pool,
    metrics: Arc<BotMetrics>,
}

// === device linking ===

async fn start_channel_recv(
    id: String,
    pool: bitpart_common::db::Pool,
    metrics: MetricsRegistry,
    manager: &mut Cell<Manager<BitpartStore, Registered>>,
) -> Result<()> {
    let channel = crate::db::channel::get_by_id(&id, &pool)
        .await?
        .ok_or_else(|| BitpartErrorKind::Signal("No such channel.".to_owned()))?;
    let bot_metrics = metrics.get_or_create(&channel.bot_id);
    let state = ChannelState {
        id: channel.bot_id,
        channel_id: id,
        pool,
        metrics: bot_metrics,
    };
    receive(manager, &state).await?;
    Ok(())
}

async fn process_channel_message(msg: ChannelMessage) -> Result<()> {
    let ChannelMessage {
        msg,
        pool,
        token,
        tracker: _,
        sender,
        metrics,
    } = msg;
    match msg {
        ChannelMessageContents::LinkChannel { id, device_name } => {
            let config_store = BitpartStore::open(&id, &pool, OnNewIdentity::Trust).await?;
            let (provisioning_link_tx, provisioning_link_rx) = oneshot::channel();

            spawn_local(async move {
                tokio::select! {
                    _ = async {
                        match Manager::link_secondary_device(
                            config_store,
                            SignalServers::Production,
                            device_name.clone(),
                            provisioning_link_tx,
                        )
                        .await
                        {
                            Ok(mut manager) => {
                                if let Err(err) = manager.request_contacts().await {
                                    error!("Failed to sync contacts after linking device: {}", err);
                                }
                                let mut manager_ref = Cell::new(manager);
                                let res = start_channel_recv(
                                    id,
                                    pool.clone(),
                                    metrics.clone(),
                                    &mut manager_ref).await;
                                error!("Link device receiver channel exited early: {:?}", res);
                            }
                            Err(err) => {
                                warn!("Skipping startup of just-linked channel: {:?}", err);
                            }
                        }
                    } => {info!("Channel message LinkChannel task exited")},
                    () = token.cancelled() => {debug!("Channel message LinkChannel task exited...")}
                }
            });

            let res = provisioning_link_rx
                .await
                .map(|url| url.to_string())
                .map_err(|_e| BitpartErrorKind::Signal("Linking error".to_owned()))?;
            Ok(sender.send(res).map_err(BitpartErrorKind::Signal)?)
        }
        ChannelMessageContents::StartChannel { id } => {
            let store = BitpartStore::open(&id, &pool, OnNewIdentity::Trust).await?;

            spawn_local(async move {
                tokio::select! {
                    _ = async {
                        match Manager::load_registered(store).await {
                            Ok(manager) => {
                                let mut manager_ref = Cell::new(manager);
                                let res =
                                    start_channel_recv(id, pool.clone(), metrics.clone(), &mut manager_ref).await;

                                error!(
                                    "Channel message StartChannel receive task exited early: {:?}",
                                    res
                                );

                            },
                            Err(err) => {
                                error!("Skipping startup of unregistered channel: {:?}", err);

                            }
                        }
                    } => {info!("Channel message StartChannel task exited")},
                    () = token.cancelled() => {debug!("Channel message StartChannel task exited...")}
                }
            });

            Ok(sender
                .send("".to_owned())
                .map_err(BitpartErrorKind::Signal)?)
        }
        ChannelMessageContents::ResetSessions { id } => {
            let store = BitpartStore::open(&id, &pool, OnNewIdentity::Trust).await?;

            match Manager::load_registered(store).await {
                Ok(mut manager) => {
                    let sessions: Vec<(String, Vec<u8>)> = manager.store().aci_sessions().await?;
                    for (address, _) in sessions {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or(Duration::ZERO)
                            .as_millis() as u64;
                        let addr = address.split('.').next().ok_or_else(|| {
                            BitpartErrorKind::Signal(format!("Empty session address: {address}"))
                        })?;
                        let uuid = uuid::Uuid::parse_str(addr)?;
                        manager
                            .send_session_reset(&ServiceId::Aci(uuid.into()), timestamp)
                            .await?
                    }
                    Ok(sender
                        .send("".to_owned())
                        .map_err(BitpartErrorKind::Signal)?)
                }
                Err(err) => {
                    error!("Skipping startup of unregistered channel: {:?}", err);
                    Ok(sender
                        .send("".to_owned())
                        .map_err(BitpartErrorKind::Signal)?)
                }
            }
        }
    }
}

// === outbound send ===

enum Recipient {
    Contact(Uuid),
    Group(GroupMasterKeyBytes),
}

async fn send<S: Store>(
    manager: &mut Manager<S, Registered>,
    recipient: Recipient,
    msg: String,
    attachments: Vec<AttachmentPointer>,
) -> Result<()> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64;

    match recipient {
        Recipient::Contact(uuid) => {
            info!(recipient =% uuid, "sending message to contact");
            let mut data_message: ContentBody = DataMessage {
                body: Some(msg),
                attachments,
                ..Default::default()
            }
            .into();
            if let ContentBody::DataMessage(d) = &mut data_message {
                d.timestamp = Some(timestamp);
            }
            manager
                .send_message(ServiceId::Aci(uuid.into()), data_message, timestamp)
                .await
                .map_err(|e| BitpartErrorKind::PresageStore(e.to_string()))?;
        }
        Recipient::Group(master_key) => {
            info!("sending message to group");
            let mut data_message: ContentBody = DataMessage {
                body: Some(msg),
                attachments,
                group_v2: Some(GroupContextV2 {
                    master_key: Some(master_key.to_vec()),
                    revision: Some(0),
                    ..Default::default()
                }),
                ..Default::default()
            }
            .into();
            if let ContentBody::DataMessage(d) = &mut data_message {
                d.timestamp = Some(timestamp);
            }
            manager
                .send_message_to_group(&master_key, data_message, timestamp)
                .await
                .map_err(|e| BitpartErrorKind::PresageStore(e.to_string()))?;
        }
    }

    Ok(())
}

async fn send_delivery_receipt<S: Store>(
    manager: &mut Manager<S, Registered>,
    content: &Content,
) -> Result<()> {
    // Only acknowledge actual inbound messages, not sync/receipt/typing content.
    if !matches!(content.body, ContentBody::DataMessage(_)) {
        return Ok(());
    }

    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64;

    let receipt: ContentBody = ReceiptMessage {
        r#type: Some(receipt_message::Type::Delivery as i32),
        timestamp: vec![content.metadata.client_timestamp.timestamp_millis() as u64],
    }
    .into();

    manager
        .send_message(content.metadata.sender, receipt, now)
        .await
        .map_err(|e| BitpartErrorKind::PresageStore(e.to_string()))?;

    Ok(())
}

// === message formatting ===

async fn process_signal_message<S: Store>(
    manager: &mut Manager<S, Registered>,
    content: &Content,
    state: &ChannelState,
) -> Result<()> {
    let thread = Thread::try_from(content).map_err(|e| BitpartErrorKind::Signal(e.to_string()))?;

    let attachments = describe_attachments(content);

    async fn format_data_message<S: Store>(
        thread: &Thread,
        data_message: &DataMessage,
        manager: &mut Manager<S, Registered>,
    ) -> Option<String> {
        match data_message {
            DataMessage {
                quote:
                    Some(Quote {
                        text: Some(_quoted_text),
                        ..
                    }),
                body: Some(_body),
                ..
            } => Some("Answer to message \"REDACTED\": REDACTED".to_string()),
            DataMessage {
                reaction:
                    Some(Reaction {
                        target_sent_timestamp: Some(ts),
                        emoji: Some(emoji),
                        ..
                    }),
                ..
            } => {
                let Ok(Some(message)) = manager.store().message(thread, *ts).await else {
                    debug!(%thread, sent_at = ts, "no message found in thread");
                    return None;
                };

                let ContentBody::DataMessage(DataMessage {
                    body: Some(_body), ..
                }) = message.body
                else {
                    warn!("message reacted to has no body");
                    return None;
                };

                Some(format!("Reacted with {emoji} to message: \"REDACTED\""))
            }
            DataMessage {
                body: Some(body), ..
            } => Some(body.to_string()),
            _ => {
                debug!("Empty data message");
                None
            }
        }
    }

    async fn format_contact<S: Store>(
        service_id: &ServiceId,
        manager: &mut Manager<S, Registered>,
    ) -> String {
        let uuid = service_id.raw_uuid();
        manager
            .store()
            .contact_by_id(service_id)
            .await
            .ok()
            .flatten()
            .filter(|c| !c.name.is_empty())
            .map(|c| format!("{}: {}", c.name, uuid))
            .unwrap_or_else(|| uuid.to_string())
    }

    async fn format_group<S: Store>(key: [u8; 32], manager: &mut Manager<S, Registered>) -> String {
        manager
            .store()
            .group(key)
            .await
            .ok()
            .flatten()
            .map(|g| g.title)
            .unwrap_or_else(|| "<missing group>".to_string())
    }

    enum Msg<'a> {
        Replyable(&'a Thread, String),
        Received(&'a Thread, String),
        Sent(&'a Thread, String),
    }

    if let Some(msg) = match &content.body {
        ContentBody::NullMessage(_) => Some(Msg::Received(
            &thread,
            "Null message (for example deleted)".to_string(),
        )),
        ContentBody::DataMessage(data_message) => {
            format_data_message(&thread, data_message, manager)
                .await
                .map(|body| Msg::Replyable(&thread, body))
        }
        ContentBody::EditMessage(EditMessage {
            data_message: Some(data_message),
            ..
        }) => format_data_message(&thread, data_message, manager)
            .await
            .map(|body| Msg::Received(&thread, body)),
        ContentBody::EditMessage(EditMessage { .. }) => None,
        ContentBody::SynchronizeMessage(SyncMessage {
            content:
                Some(SyncContent::Sent(Sent {
                    message: Some(data_message),
                    ..
                })),
            ..
        }) => format_data_message(&thread, data_message, manager)
            .await
            .map(|body| Msg::Sent(&thread, body)),
        ContentBody::SynchronizeMessage(SyncMessage {
            content:
                Some(SyncContent::Sent(Sent {
                    edit_message:
                        Some(EditMessage {
                            data_message: Some(data_message),
                            ..
                        }),
                    ..
                })),
            ..
        }) => format_data_message(&thread, data_message, manager)
            .await
            .map(|body| Msg::Sent(&thread, body)),
        ContentBody::SynchronizeMessage(SyncMessage { .. }) => None,
        ContentBody::CallMessage(_) => Some(Msg::Received(&thread, "is calling!".into())),
        ContentBody::TypingMessage(_) => Some(Msg::Received(&thread, "is typing...".into())),
        ContentBody::ReceiptMessage(ReceiptMessage {
            r#type: receipt_type,
            timestamp,
        }) => Some(Msg::Received(
            &thread,
            format!(
                "got {:?} receipt for messages sent at {timestamp:?}",
                receipt_message::Type::try_from(receipt_type.unwrap_or_default())?
            ),
        )),
        ContentBody::StoryMessage(story) => {
            Some(Msg::Received(&thread, format!("new story: {story:?}")))
        }
        ContentBody::DecryptionErrorMessage(_) => Some(Msg::Received(
            &thread,
            "got decryption error message".into(),
        )),
    } {
        let ts = content.timestamp();
        let (prefix, _body) = match msg {
            Msg::Received(Thread::Contact(sender), body) => {
                let contact = format_contact(sender, manager).await;
                (format!("From {contact} @ {ts}: "), body)
            }
            Msg::Replyable(Thread::Contact(sender), body) => {
                let contact = format_contact(sender, manager).await;
                if let Err(err) = reply(
                    sender.raw_uuid().to_string(),
                    body.clone(),
                    attachments.clone(),
                    state,
                    manager,
                )
                .await
                {
                    warn!("Problem with replying to message: {:?}", err);
                }
                (format!("From {contact} @ {ts}: "), body)
            }
            Msg::Sent(Thread::Contact(recipient), body) => {
                let contact = format_contact(recipient, manager).await;
                (format!("To {contact} @ {ts}"), body)
            }
            Msg::Received(Thread::Group(key), body) => {
                let sender = format_contact(&content.metadata.sender, manager).await;
                let group = format_group(*key, manager).await;
                (format!("From {sender} to group {group} @ {ts}: "), body)
            }
            Msg::Replyable(Thread::Group(key), body) => {
                let sender = format_contact(&content.metadata.sender, manager).await;
                let group = format_group(*key, manager).await;
                (format!("From {sender} to group {group} @ {ts}: "), body)
            }
            Msg::Sent(Thread::Group(key), body) => {
                let group = format_group(*key, manager).await;
                (format!("To group {group} @ {ts}"), body)
            }
        };

        debug!("{prefix} / REDACTED");
    }

    Ok(())
}

fn describe_attachments(content: &Content) -> Vec<serde_json::Value> {
    let ContentBody::DataMessage(DataMessage { attachments, .. }) = &content.body else {
        return Vec::new();
    };

    attachments
        .iter()
        .map(|pointer| {
            let mut described = json!({
                "_ref": BASE64_STANDARD.encode(pointer.encode_to_vec()),
            });
            if let Some(content_type) = &pointer.content_type {
                described["content_type"] = json!(content_type);
            }
            if let Some(file_name) = &pointer.file_name {
                described["file_name"] = json!(file_name);
            }
            if let Some(size) = pointer.size {
                described["size"] = json!(size);
            }
            described
        })
        .collect()
}

async fn send_expire_timer<S: Store>(
    manager: &mut Manager<S, Registered>,
    uuid: Uuid,
    timer: u32,
) -> Result<()> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64;
    info!(recipient =% uuid, timer, "setting disappearing message timer");
    let data_message = DataMessage {
        flags: Some(Flags::ExpirationTimerUpdate as u32),
        expire_timer: Some(timer),
        timestamp: Some(timestamp),
        ..Default::default()
    };
    manager
        .send_message(ServiceId::Aci(uuid.into()), data_message, timestamp)
        .await
        .map_err(|e| BitpartErrorKind::PresageStore(e.to_string()))?;
    Ok(())
}

// === message listener ===

async fn reply<S: Store>(
    user_id: String,
    body: String,
    attachments: Vec<serde_json::Value>,
    state: &ChannelState,
    manager: &mut Manager<S, Registered>,
) -> Result<()> {
    let mut content = json!({ "text": body });
    if !attachments.is_empty() {
        content["attachments"] = json!(attachments);
    }

    let payload = json!({
        "content_type": "text",
        "content": content
    });

    let client = Client {
        bot_id: state.id.clone(),
        channel_id: "signal".to_owned(),
        user_id: user_id.clone(),
    };

    let first_contact = db::conversation::get_by_client(&client, Some(1), None, &state.pool)
        .await?
        .is_empty();
    if first_contact
        && let Ok(Recipient::Contact(uuid)) = try_user_id_to_recipient(&user_id)
        && let Some(bot) = db::bot::get_latest_by_bot_id(&state.id, &state.pool).await?
        && let Some(timer) = bot.expire_timer.filter(|t| *t > 0)
        && !manager
            .store()
            .expire_timer(&Thread::Contact(ServiceId::Aci(uuid.into())))
            .await
            .ok()
            .flatten()
            .is_some_and(|(t, _)| t > 0)
    {
        send_expire_timer(manager, uuid, timer).await?;
    }

    let event = SerializedEvent {
        id: uuid::Uuid::new_v4().to_string(),
        client,
        metadata: serde_json::Value::Null,
        payload,
        step_limit: None,
        callback_url: None,
    };

    let request = Request {
        bot: None,
        bot_id: Some(state.id.clone()),
        version_id: None,
        apps_endpoint: None,
        multibot: None,
        event,
    };

    let res = api::process_request(&request, &state.pool).await?;
    if let Some(messages) = res.get("messages") {
        for i in messages
            .as_array()
            .ok_or(BitpartErrorKind::Signal(
                "Got invalid message from interpreter".to_owned(),
            ))?
            .iter()
        {
            send(
                manager,
                try_user_id_to_recipient(&reply_get_user_id(i, &user_id))?,
                reply_get_text(i),
                reply_get_attachments(i),
            )
            .await
            .map_err(|err| BitpartErrorKind::Signal(err.to_string()))?;
            state.metrics.incr_sent();
        }
    }

    if res.get("deleted").and_then(Value::as_bool).unwrap_or(false) {
        let store =
            BitpartStore::open(&state.channel_id, &state.pool, OnNewIdentity::Trust).await?;
        let errors = store.purge_conversation(&user_id).await;
        if errors.is_empty() {
            info!("purged signal state for deleted conversation");
        } else {
            warn!(
                count = errors.len(),
                "purged signal state with errors, some data may remain"
            );
        }
    }

    Ok(())
}

fn unescape(input: &str) -> String {
    input
        .trim_matches(|c| c == '\"' || c == '\'')
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

fn try_user_id_to_recipient(user_id: &str) -> Result<Recipient> {
    match Uuid::try_parse(user_id) {
        Ok(uuid) => Ok(Recipient::Contact(uuid)),
        Err(_) => {
            let key: [u8; 32] = user_id.as_bytes().try_into()?;
            Ok(Recipient::Group(key))
        }
    }
}

fn reply_get_user_id(res: &serde_json::Value, default_user_id: &str) -> String {
    if let Some(payload) = res.get("payload")
        && let Some(content) = payload.get("content")
        && let Some(client) = content.get("client")
        && let Some(user_id) = client.get("user_id")
    {
        return unescape(&user_id.to_string()).to_string();
    }
    default_user_id.to_string()
}

fn reply_get_text(res: &serde_json::Value) -> String {
    if let Some(payload) = res.get("payload")
        && let Some(content) = payload.get("content")
        && let Some(text) = content.get("text")
    {
        return unescape(&text.to_string()).to_string();
    }
    "".to_owned()
}

fn reply_get_attachments(res: &serde_json::Value) -> Vec<AttachmentPointer> {
    let Some(items) = res
        .get("payload")
        .and_then(|payload| payload.get("content"))
        .and_then(|content| content.get("attachments"))
        .and_then(|attachments| attachments.as_array())
    else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| {
            let encoded = match item.get("_ref").and_then(|r| r.as_str()) {
                Some(encoded) => encoded,
                None => {
                    warn!("outgoing attachment has no _ref, skipping");
                    return None;
                }
            };
            let decoded = match BASE64_STANDARD.decode(encoded) {
                Ok(decoded) => decoded,
                Err(error) => {
                    warn!(%error, "outgoing attachment _ref is not valid base64");
                    return None;
                }
            };
            match AttachmentPointer::decode(decoded.as_slice()) {
                Ok(pointer) => Some(pointer),
                Err(error) => {
                    warn!(%error, "outgoing attachment _ref is not a valid pointer");
                    None
                }
            }
        })
        .collect()
}

async fn receive(
    manager_ref: &mut Cell<Manager<BitpartStore, Registered>>,
    state: &ChannelState,
) -> Result<()> {
    loop {
        'inner: loop {
            tokio::time::sleep(Duration::from_millis(2)).await;
            let manager = manager_ref.get_mut();
            match manager.receive_messages().await {
                Ok(messages) => {
                    state.metrics.set_status(ConnectionStatus::Linked);
                    pin_mut!(messages);
                    while let Some(content) = messages.next().await {
                        match content {
                            Received::QueueEmpty => debug!("done with synchronization"),
                            Received::Contacts => debug!("got contacts synchronization"),
                            Received::Content(content) => {
                                if matches!(content.body, ContentBody::DataMessage(_)) {
                                    state.metrics.incr_received();
                                }
                                if let Err(err) = send_delivery_receipt(manager, &content).await {
                                    warn!("Failed to send delivery receipt: {:?}", err);
                                }
                                if let Err(err) =
                                    process_signal_message(manager, &content, state).await
                                {
                                    warn!("Failed to extract message thread: {:?}", err);
                                }
                            }
                            Received::DecryptionError(contact) => {
                                warn!("Failed to decrypt message from contact: {:?}", contact);
                            }
                        }
                    }
                }
                Err(err) => {
                    error!("Failed to receive messages: {:?}", err);
                    state.metrics.set_status(ConnectionStatus::Failing);
                    sleep(Duration::from_secs(30)).await;
                    break 'inner;
                }
            }
        }
        let store =
            BitpartStore::open(&state.channel_id, &state.pool, OnNewIdentity::Trust).await?;
        match Manager::load_registered(store).await {
            Ok(manager) => {
                warn!("Replacing manager!");
                manager_ref.replace(manager);
            }
            Err(err) => {
                warn!("Failed to reload manager. {:?}", err);
            }
        }
    }
}
