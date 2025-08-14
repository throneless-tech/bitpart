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

use bitpart_common::{
    csml::{Request, SerializedEvent},
    error::{BitpartErrorKind, Result},
};
use chrono::Local;
use csml_interpreter::data::Client;
use futures::StreamExt;
use futures::{channel::oneshot, pin_mut};
use presage::libsignal_service::configuration::SignalServers;
use presage::libsignal_service::content::Reaction;
use presage::libsignal_service::prelude::Uuid;
use presage::libsignal_service::proto::data_message::Quote;
use presage::libsignal_service::proto::sync_message::Sent;
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
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tokio::{
    fs,
    runtime::Builder as TokioBuilder,
    sync::{mpsc, oneshot as tokio_oneshot},
    task::LocalSet,
};
use tracing::warn;
use tracing::{debug, error, info};
use url::Url;
use uuid;

use crate::api;
use crate::db;

#[derive(Serialize, Deserialize)]
pub enum ChannelMessageContents {
    LinkChannel {
        id: String,
        attachments_dir: PathBuf,
        device_name: String,
    },
    StartChannel {
        id: String,
        attachments_dir: PathBuf,
    },
}

pub struct ChannelMessage {
    pub msg: ChannelMessageContents,
    pub db: DatabaseConnection,
    pub sender: tokio_oneshot::Sender<String>,
}

#[derive(Clone)]
pub struct SignalManager {
    inner: mpsc::UnboundedSender<ChannelMessage>,
}

impl Default for SignalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalManager {
    pub fn new() -> Self {
        let (send, mut recv) = mpsc::unbounded_channel();

        let rt = TokioBuilder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create thread builder");

        let _ = std::thread::Builder::new()
            .stack_size(4 * 1024 * 1024)
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

    pub fn send(&self, msg: ChannelMessage) {
        self.inner.send(msg).expect("Thread has shutdown")
    }
}

#[derive(Debug)]
pub struct ChannelState {
    id: String,
    db: DatabaseConnection,
    tx: mpsc::Sender<(Recipient, String)>,
}

async fn start_channel_recv(
    id: String,
    attachments_dir: PathBuf,
    db: DatabaseConnection,
    mut manager: Manager<BitpartStore, Registered>,
    tx: mpsc::Sender<(Recipient, String)>,
) -> Result<()> {
    let channel = db::channel::get_by_id(&id, &db)
        .await?
        .ok_or_else(|| BitpartErrorKind::Signal("No such channel.".to_owned()))?;
    let state = ChannelState {
        id: channel.bot_id,
        db,
        tx,
    };
    receive(&mut manager, &attachments_dir, Some(state)).await
}

async fn start_channel_send(
    mut manager: Manager<BitpartStore, Registered>,
    mut rx: mpsc::Receiver<(Recipient, String)>,
) -> Result<()> {
    while let Some((recipient, msg)) = rx.recv().await {
        match recipient {
            Recipient::Contact(_) => {
                let data_message = DataMessage {
                    body: Some(msg),
                    ..Default::default()
                };

                send(&mut manager, recipient, data_message).await?
            }
            Recipient::Group(group) => {
                let data_message = DataMessage {
                    body: Some(msg),
                    group_v2: Some(GroupContextV2 {
                        master_key: Some(group.to_vec()),
                        revision: Some(0),
                        ..Default::default()
                    }),
                    ..Default::default()
                };

                send(&mut manager, recipient, data_message).await?
            }
        }
    }
    Ok(())
}

async fn process_channel_message(msg: ChannelMessage) -> Result<()> {
    let ChannelMessage { msg, db, sender } = msg;
    match msg {
        ChannelMessageContents::LinkChannel {
            id,
            attachments_dir,
            device_name,
        } => {
            let config_store = BitpartStore::open(&id, &db, OnNewIdentity::Trust).await?;
            let (provisioning_link_tx, provisioning_link_rx) = oneshot::channel();
            tokio::task::spawn_local(link_device(
                id,
                config_store,
                SignalServers::Production,
                attachments_dir,
                device_name,
                db,
                provisioning_link_tx,
            ));

            let res = provisioning_link_rx
                .await
                .map(|url| url.to_string())
                .map_err(|_e| BitpartErrorKind::Signal("Linking error".to_owned()))?;
            Ok(sender.send(res).map_err(BitpartErrorKind::Signal)?)
        }
        ChannelMessageContents::StartChannel {
            id,
            attachments_dir,
        } => {
            let (tx, rx) = mpsc::channel(100);
            let store = BitpartStore::open(&id, &db, OnNewIdentity::Trust).await?;
            if let Ok(manager) = Manager::load_registered(store).await {
                tokio::task::spawn_local(start_channel_send(manager.clone(), rx));
                tokio::task::spawn_local(start_channel_recv(
                    id,
                    attachments_dir,
                    db.clone(),
                    manager.clone(),
                    tx,
                ));
                Ok(sender
                    .send("".to_owned())
                    .map_err(BitpartErrorKind::Signal)?)
            } else {
                warn!("Skipping startup of unregistered channel");
                Ok(sender
                    .send("".to_owned())
                    .map_err(BitpartErrorKind::Signal)?)
            }
        }
    }
}

enum Recipient {
    Contact(Uuid),
    Group(GroupMasterKeyBytes),
}

async fn send<S: Store>(
    manager: &mut Manager<S, Registered>,
    recipient: Recipient,
    msg: impl Into<ContentBody>,
) -> Result<()> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64;

    let mut content_body = msg.into();
    if let ContentBody::DataMessage(d) = &mut content_body {
        d.timestamp = Some(timestamp);
    }

    match recipient {
        Recipient::Contact(uuid) => {
            info!(recipient =% uuid, "sending message to contact");
            manager
                .send_message(ServiceId::Aci(uuid.into()), content_body, timestamp)
                .await
                .map_err(|_| BitpartErrorKind::PresageStore)?;
        }
        Recipient::Group(master_key) => {
            info!("sending message to group");
            manager
                .send_message_to_group(&master_key, content_body, timestamp)
                .await
                .map_err(|_| BitpartErrorKind::PresageStore)?;
        }
    }

    Ok(())
}

async fn process_signal_message<S: Store>(
    manager: &mut Manager<S, Registered>,
    attachments_dir: &Path,
    content: &Content,
    state: &Option<ChannelState>,
) -> Result<()> {
    let thread = Thread::try_from(content)?;

    async fn format_data_message<S: Store>(
        thread: &Thread,
        data_message: &DataMessage,
        manager: &Manager<S, Registered>,
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
                    warn!(%thread, sent_at = ts, "no message found in thread");
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

    async fn format_contact<S: Store>(uuid: &Uuid, manager: &Manager<S, Registered>) -> String {
        manager
            .store()
            .contact_by_id(uuid)
            .await
            .ok()
            .flatten()
            .filter(|c| !c.name.is_empty())
            .map(|c| format!("{}: {}", c.name, uuid))
            .unwrap_or_else(|| uuid.to_string())
    }

    async fn format_group<S: Store>(key: [u8; 32], manager: &Manager<S, Registered>) -> String {
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
            sent:
                Some(Sent {
                    message: Some(data_message),
                    ..
                }),
            ..
        }) => format_data_message(&thread, data_message, manager)
            .await
            .map(|body| Msg::Sent(&thread, body)),
        ContentBody::SynchronizeMessage(SyncMessage {
            sent:
                Some(Sent {
                    edit_message:
                        Some(EditMessage {
                            data_message: Some(data_message),
                            ..
                        }),
                    ..
                }),
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
        ContentBody::PniSignatureMessage(_) => {
            Some(Msg::Received(&thread, "got PNI signature message".into()))
        }
    } {
        let ts = content.timestamp();
        let (prefix, _body) = match msg {
            Msg::Received(Thread::Contact(sender), body) => {
                let contact = format_contact(sender, manager).await;
                (format!("From {contact} @ {ts}: "), body)
            }
            Msg::Replyable(Thread::Contact(sender), body) => {
                let contact = format_contact(sender, manager).await;
                if let Some(state) = state
                    && let Err(err) = reply(sender.to_string(), body.clone(), state).await
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
                let sender = format_contact(&content.metadata.sender.raw_uuid(), manager).await;
                let group = format_group(*key, manager).await;
                (format!("From {sender} to group {group} @ {ts}: "), body)
            }
            Msg::Replyable(Thread::Group(key), body) => {
                let sender = format_contact(&content.metadata.sender.raw_uuid(), manager).await;
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

    let sender = content.metadata.sender.raw_uuid();
    if let ContentBody::DataMessage(DataMessage { attachments, .. }) = &content.body {
        for attachment_pointer in attachments {
            let Ok(attachment_data) = manager.get_attachment(attachment_pointer).await else {
                warn!("failed to fetch attachment");
                continue;
            };

            let extensions = mime_guess::get_mime_extensions_str(
                attachment_pointer
                    .content_type
                    .as_deref()
                    .unwrap_or("application/octet-stream"),
            );
            let extension = extensions.and_then(|e| e.first()).unwrap_or(&"bin");
            let filename = attachment_pointer
                .file_name
                .clone()
                .unwrap_or_else(|| Local::now().format("%Y-%m-%d-%H-%M-%s").to_string());
            let file_path = attachments_dir.join(format!("bitpart-{filename}.{extension}",));
            match fs::write(&file_path, &attachment_data).await {
                Ok(_) => info!(%sender, file_path =% file_path.display(), "saved attachment"),
                Err(error) => error!(
                    %sender,
                    file_path =% file_path.display(),
                    %error,
                    "failed to write attachment"
                ),
            }
        }
    }
    Ok(())
}

async fn reply(user_id: String, body: String, state: &ChannelState) -> Result<()> {
    let payload = json!({
        "content_type": "text",
        "content": {
            "text": body
        }
    });

    let client = Client {
        bot_id: state.id.clone(),
        channel_id: "signal".to_owned(),
        user_id: user_id.clone(),
    };

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

    let res = api::process_request(&request, &state.db).await?;
    if let Some(messages) = res.get("messages") {
        for i in messages
            .as_array()
            .ok_or(BitpartErrorKind::Signal(
                "Got invalid message from interpreter".to_owned(),
            ))?
            .iter()
        {
            state
                .tx
                .send((
                    try_user_id_to_recipient(&reply_get_user_id(i, &user_id))?,
                    reply_get_text(i),
                ))
                .await
                .map_err(|err| BitpartErrorKind::Signal(err.to_string()))?;
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

async fn receive<S: Store>(
    manager: &mut Manager<S, Registered>,
    attachments_dir: &Path,
    state: Option<ChannelState>,
) -> Result<()> {
    info!(
        path =% attachments_dir.display(),
        "attachments will be stored"
    );

    let messages = manager.receive_messages().await.map_err(|_e| {
        BitpartErrorKind::Signal("failed to initialize messages stream".to_owned())
    })?;
    pin_mut!(messages);

    while let Some(content) = messages.next().await {
        match content {
            Received::QueueEmpty => debug!("done with synchronization"),
            Received::Contacts => debug!("got contacts synchronization"),
            Received::Content(content) => {
                if let Err(err) =
                    process_signal_message(manager, attachments_dir, &content, &state).await
                {
                    warn!("Failed to extract message thread: {:?}", err);
                }
            }
        }
    }

    Ok(())
}

// API functions

async fn link_device(
    id: String,
    config_store: BitpartStore,
    servers: SignalServers,
    attachments_dir: PathBuf,
    device_name: String,
    db: DatabaseConnection,
    provisioning_link_tx: oneshot::Sender<Url>,
) -> Result<()> {
    let (tx, rx) = mpsc::channel(100);
    if let Ok(manager) = Manager::link_secondary_device(
        config_store,
        servers,
        device_name.clone(),
        provisioning_link_tx,
    )
    .await
    {
        tokio::task::spawn_local(start_channel_send(manager.clone(), rx));
        tokio::task::spawn_local(start_channel_recv(
            id,
            attachments_dir,
            db.clone(),
            manager.clone(),
            tx,
        ));
    } else {
        warn!("Skipping startup of just-linked channel");
    }
    Ok(())
}
