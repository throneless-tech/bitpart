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
use presage::libsignal_service::profile_name::ProfileName;
use presage::libsignal_service::proto::AttachmentPointer;
use presage::libsignal_service::proto::BodyRange;
use presage::libsignal_service::proto::body_range::AssociatedValue;
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
use presage_store_bitpart::{BitpartStore, group_ref};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
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
use crate::limits::{ChannelLimiters, RateLimits};
use crate::metrics::{ChannelMetrics, ConnectionStatus, MetricsRegistry, SendOutcome};
use chrono::Utc;
use governor::DefaultKeyedRateLimiter;
use std::sync::Arc;

// === manager + dispatch ===

#[derive(Serialize, Deserialize)]
pub enum ChannelMessageContents {
    LinkChannel {
        id: String,
        device_name: String,
        profile_name: Option<String>,
    },
    StartChannel {
        id: String,
    },
    ResetSessions {
        id: String,
    },
    SubmitChallenge {
        id: String,
        token: Option<String>,
        captcha: String,
    },
}

pub struct ChannelMessage {
    pub msg: ChannelMessageContents,
    pub pool: bitpart_common::db::Pool,
    pub token: CancellationToken,
    pub tracker: TaskTracker,
    pub sender: tokio_oneshot::Sender<String>,
    pub metrics: MetricsRegistry,
    pub limits: RateLimits,
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
    metrics: Arc<ChannelMetrics>,
    limiters: Arc<ChannelLimiters>,
    pending_purges: RefCell<BTreeSet<String>>,
}

// === device linking ===

async fn start_channel_recv(
    id: String,
    pool: bitpart_common::db::Pool,
    metrics: MetricsRegistry,
    limits: RateLimits,
    manager: &mut Cell<Manager<BitpartStore, Registered>>,
) -> Result<()> {
    let channel = crate::db::channel::get_by_id(&id, &pool)
        .await?
        .ok_or_else(|| BitpartErrorKind::Signal("No such channel.".to_owned()))?;
    let channel_metrics = metrics.get_or_create(&id, &channel.bot_id);
    let state = ChannelState {
        id: channel.bot_id,
        channel_id: id,
        pool,
        metrics: channel_metrics,
        limiters: Arc::new(ChannelLimiters::new(&limits)),
        pending_purges: RefCell::new(BTreeSet::new()),
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
        limits,
    } = msg;
    match msg {
        ChannelMessageContents::LinkChannel {
            id,
            device_name,
            profile_name,
        } => {
            let config_store = BitpartStore::open(&id, &pool, OnNewIdentity::Trust).await?;
            if let Some(channel) = crate::db::channel::get_by_id(&id, &pool).await? {
                metrics
                    .get_or_create(&id, &channel.bot_id)
                    .set_status(ConnectionStatus::Unlinked);
            }
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
                                if let Some(name) = profile_name.filter(|n| !n.trim().is_empty()) {
                                    let name = ProfileName {
                                        given_name: name.trim().to_owned(),
                                        family_name: None,
                                    };
                                    if let Err(err) = manager.update_profile(name, None, None).await {
                                        error!("Failed to set profile name after linking device: {}", err);
                                    }
                                }
                                let mut manager_ref = Cell::new(manager);
                                let res = start_channel_recv(
                                    id,
                                    pool.clone(),
                                    metrics.clone(),
                                    limits,
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
                                    start_channel_recv(id, pool.clone(), metrics.clone(), limits, &mut manager_ref).await;

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
        ChannelMessageContents::SubmitChallenge { id, token, captcha } => {
            let result = submit_challenge(&id, &pool, &metrics, token, &captcha).await;
            let reply = match result {
                Ok(()) => "".to_owned(),
                Err(error) => {
                    warn!(%error, "captcha challenge not accepted");
                    format!("error: {error}")
                }
            };
            Ok(sender.send(reply).map_err(BitpartErrorKind::Signal)?)
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

const MAX_INLINE_RETRY: Duration = Duration::from_secs(2);
const MIN_RETRY_WAIT: Duration = Duration::from_secs(1);

enum SendFailure {
    RateLimited(Option<Duration>),
    ChallengeRequired(String),
    Other,
}

fn classify<E: std::error::Error>(error: &presage::Error<E>) -> SendFailure {
    use presage::libsignal_service::prelude::{MessageSenderError, ServiceError};
    let rate_limited = |service: &ServiceError| match service {
        ServiceError::RateLimitExceeded { retry_after } => Some(SendFailure::RateLimited(
            retry_after.and_then(|d| d.to_std().ok()),
        )),
        ServiceError::ProofRequiredError(proof) => {
            Some(SendFailure::ChallengeRequired(proof.token.clone()))
        }
        _ => None,
    };
    match error {
        presage::Error::ServiceError(service) => rate_limited(service),
        presage::Error::MessageSenderError(sender) => match sender.as_ref() {
            MessageSenderError::ProofRequired { token, .. } => {
                Some(SendFailure::ChallengeRequired(token.clone()))
            }
            MessageSenderError::ServiceError(service) => rate_limited(service),
            _ => None,
        },
        _ => None,
    }
    .unwrap_or(SendFailure::Other)
}

fn inline_retry_wait(failure: &SendFailure) -> Option<Duration> {
    match failure {
        SendFailure::RateLimited(Some(wait)) if *wait <= MAX_INLINE_RETRY => {
            Some((*wait).max(MIN_RETRY_WAIT))
        }
        _ => None,
    }
}

fn outcome_of(failure: SendFailure) -> SendOutcome {
    match failure {
        SendFailure::RateLimited(wait) => SendOutcome::RateLimited {
            retry_at: wait
                .and_then(|w| chrono::Duration::from_std(w).ok())
                .map(|w| Utc::now() + w),
        },
        SendFailure::ChallengeRequired(token) => SendOutcome::ChallengeRequired {
            challenge_token: token,
        },
        SendFailure::Other => SendOutcome::Failing,
    }
}

enum Outgoing {
    Contact(ServiceId),
    Group(GroupMasterKeyBytes),
}

async fn deliver<S: Store>(
    manager: &mut Manager<S, Registered>,
    metrics: &ChannelMetrics,
    to: Outgoing,
    body: ContentBody,
    timestamp: u64,
) -> Result<()> {
    let mut retried = false;
    loop {
        let result = match &to {
            Outgoing::Contact(service_id) => {
                manager
                    .send_message(*service_id, body.clone(), timestamp)
                    .await
            }
            Outgoing::Group(master_key) => {
                manager
                    .send_message_to_group(master_key, body.clone(), timestamp)
                    .await
            }
        };
        let error = match result {
            Ok(()) => {
                metrics.record_send(SendOutcome::Ok);
                return Ok(());
            }
            Err(error) => error,
        };
        let failure = classify(&error);
        if !retried
            && matches!(to, Outgoing::Contact(_))
            && let Some(wait) = inline_retry_wait(&failure)
        {
            debug!(?wait, "rate limited by Signal, retrying once");
            retried = true;
            sleep(wait).await;
            continue;
        }
        warn!(%error, "message dropped");
        metrics.record_send(outcome_of(failure));
        return Err(BitpartErrorKind::PresageStore(error.to_string()).into());
    }
}

async fn throttle(limiter: &DefaultKeyedRateLimiter<Uuid>, recipient: Uuid) {
    limiter.until_key_ready(&recipient).await;
}

async fn send<S: Store>(
    manager: &mut Manager<S, Registered>,
    limiter: &DefaultKeyedRateLimiter<Uuid>,
    metrics: &ChannelMetrics,
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
            throttle(limiter, uuid).await;
            deliver(
                manager,
                metrics,
                Outgoing::Contact(ServiceId::Aci(uuid.into())),
                data_message,
                timestamp,
            )
            .await?;
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
            let me = manager.registration_data().service_ids.aci;
            if let Ok(Some(group)) = manager.store().group(master_key).await {
                for member in group
                    .members
                    .iter()
                    .map(|m| m.aci.into())
                    .filter(|a: &Uuid| *a != me)
                {
                    throttle(limiter, member).await;
                }
            }
            deliver(
                manager,
                metrics,
                Outgoing::Group(master_key),
                data_message,
                timestamp,
            )
            .await?;
        }
    }

    Ok(())
}

async fn send_delivery_receipt<S: Store>(
    manager: &mut Manager<S, Registered>,
    limiter: &DefaultKeyedRateLimiter<Uuid>,
    metrics: &ChannelMetrics,
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

    throttle(limiter, content.metadata.sender.raw_uuid()).await;
    deliver(
        manager,
        metrics,
        Outgoing::Contact(content.metadata.sender),
        receipt,
        now,
    )
    .await?;

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
                let bot = manager.registration_data().service_ids.aci;
                if let ContentBody::DataMessage(data_message) = &content.body
                    && let Some(text) = addressed_text(data_message, bot)
                    && let Err(err) = reply(
                        group_user_id(key, &content.metadata.sender.raw_uuid()),
                        text,
                        attachments.clone(),
                        state,
                        manager,
                    )
                    .await
                {
                    warn!("Problem with replying to message: {:?}", err);
                }
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

fn captcha_response(input: &str) -> &str {
    let input = input.trim();
    input.strip_prefix("signalcaptcha://").unwrap_or(input)
}

async fn submit_challenge(
    id: &str,
    pool: &bitpart_common::db::Pool,
    metrics: &MetricsRegistry,
    token: Option<String>,
    captcha: &str,
) -> Result<()> {
    let channel = crate::db::channel::get_by_id(id, pool)
        .await?
        .ok_or_else(|| BitpartErrorKind::Signal("No such channel.".to_owned()))?;
    let channel_metrics = metrics.get_or_create(id, &channel.bot_id);
    let token = token
        .filter(|t| !t.trim().is_empty())
        .or_else(|| channel_metrics.pending_challenge())
        .ok_or_else(|| {
            BitpartErrorKind::Signal(
                "No pending challenge for this channel; pass a token.".to_owned(),
            )
        })?;
    let store = BitpartStore::open(id, pool, OnNewIdentity::Trust).await?;
    let manager = Manager::load_registered(store)
        .await
        .map_err(|e| BitpartErrorKind::Signal(e.to_string()))?;

    manager
        .submit_recaptcha_challenge(token.trim(), captcha_response(captcha))
        .await
        .map_err(|e| BitpartErrorKind::Signal(e.to_string()))?;

    channel_metrics.clear_challenge();
    info!("captcha challenge accepted");
    Ok(())
}

fn take_due_purges(
    pending: &RefCell<BTreeSet<String>>,
    limiter: &DefaultKeyedRateLimiter<String>,
) -> Vec<String> {
    let mut pending = pending.borrow_mut();
    let due: Vec<String> = pending
        .iter()
        .filter(|user| limiter.check_key(user).is_ok())
        .cloned()
        .collect();
    for user in &due {
        pending.remove(user);
    }
    due
}

async fn purge_signal_state(channel_id: &str, pool: &bitpart_common::db::Pool, user_id: &str) {
    let store = match BitpartStore::open(channel_id, pool, OnNewIdentity::Trust).await {
        Ok(store) => store,
        Err(error) => {
            warn!(%error, "could not open store to purge signal state");
            return;
        }
    };
    let errors = store.purge_conversation(user_id).await;
    if errors.is_empty() {
        info!("purged signal state for deleted conversation");
    } else {
        warn!(
            count = errors.len(),
            "purged signal state with errors, some data may remain"
        );
    }
}

async fn send_expire_timer<S: Store>(
    manager: &mut Manager<S, Registered>,
    limiter: &DefaultKeyedRateLimiter<Uuid>,
    metrics: &ChannelMetrics,
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
    throttle(limiter, uuid).await;
    deliver(
        manager,
        metrics,
        Outgoing::Contact(ServiceId::Aci(uuid.into())),
        data_message.into(),
        timestamp,
    )
    .await?;
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
        channel_id: state.channel_id.clone(),
        user_id: user_id.clone(),
    };

    let first_contact = db::conversation::get_by_client(&client, Some(1), None, &state.pool)
        .await?
        .is_empty();
    if first_contact
        && let Ok(Recipient::Contact(uuid)) = try_user_id_to_recipient(&user_id)
        && let Some(bot) = db::bot::get_latest_by_bot_id(&state.id, &state.pool).await?
        && let Some(timer) = bot.expire_timer.filter(|t| *t > 0)
        && manager
            .store()
            .expire_timer(&Thread::Contact(ServiceId::Aci(uuid.into())))
            .await
            .ok()
            .flatten()
            .is_none_or(|(t, _)| t == 0)
    {
        send_expire_timer(manager, &state.limiters.send, &state.metrics, uuid, timer).await?;
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
                &state.limiters.send,
                &state.metrics,
                resolve_recipient(state, &reply_get_user_id(i, &user_id)).await?,
                reply_get_text(i),
                reply_get_attachments(i),
            )
            .await?;
        }
    }

    if res.get("deleted").and_then(Value::as_bool).unwrap_or(false)
        && Uuid::try_parse(&user_id).is_ok()
    {
        if state.limiters.purge.check_key(&user_id).is_ok() {
            state.pending_purges.borrow_mut().remove(&user_id);
            purge_signal_state(&state.channel_id, &state.pool, &user_id).await;
        } else {
            info!("repeated deletion, deferring signal state purge");
            state.pending_purges.borrow_mut().insert(user_id);
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

fn group_user_id(key: &GroupMasterKeyBytes, sender: &Uuid) -> String {
    format!("group:{}:{sender}", group_ref(key))
}

fn parse_group_ref(user_id: &str) -> Option<&str> {
    let rest = user_id.strip_prefix("group:")?;
    Some(rest.split_once(':').map_or(rest, |(r, _)| r))
}

async fn resolve_recipient(state: &ChannelState, user_id: &str) -> Result<Recipient> {
    let Some(group_ref) = parse_group_ref(user_id) else {
        return try_user_id_to_recipient(user_id);
    };
    let store = BitpartStore::open(&state.channel_id, &state.pool, OnNewIdentity::Trust).await?;
    store
        .group_by_ref(group_ref)
        .await?
        .map(Recipient::Group)
        .ok_or_else(|| BitpartErrorKind::Signal("unknown group".to_owned()).into())
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

fn mentions_bot(range: &BodyRange, bot: Uuid) -> bool {
    match &range.associated_value {
        Some(AssociatedValue::MentionAci(aci)) => Uuid::try_parse(aci).is_ok_and(|u| u == bot),
        Some(AssociatedValue::MentionAciBinary(bytes)) => {
            Uuid::from_slice(bytes).is_ok_and(|u| u == bot)
        }
        _ => false,
    }
}

fn addressed_text(data_message: &DataMessage, bot: Uuid) -> Option<String> {
    let mut ranges: Vec<(usize, usize)> = data_message
        .body_ranges
        .iter()
        .filter(|range| mentions_bot(range, bot))
        .filter_map(|range| {
            let start = range.start? as usize;
            Some((start, start.checked_add(range.length? as usize)?))
        })
        .collect();
    if ranges.is_empty() {
        return None;
    }
    ranges.sort_unstable();
    let body: Vec<u16> = data_message.body.as_deref()?.encode_utf16().collect();
    let mut kept = Vec::with_capacity(body.len());
    let mut cursor = 0;
    for (start, end) in ranges {
        let start = start.min(body.len());
        if start > cursor {
            kept.extend_from_slice(&body[cursor..start]);
        }
        cursor = cursor.max(end.min(body.len()));
    }
    kept.extend_from_slice(&body[cursor..]);
    Some(
        String::from_utf16_lossy(&kept)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
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
                        for user_id in take_due_purges(&state.pending_purges, &state.limiters.purge)
                        {
                            purge_signal_state(&state.channel_id, &state.pool, &user_id).await;
                        }
                        match content {
                            Received::QueueEmpty => debug!("done with synchronization"),
                            Received::Contacts => debug!("got contacts synchronization"),
                            Received::Content(content) => {
                                if matches!(content.body, ContentBody::DataMessage(_)) {
                                    state.metrics.incr_received();
                                }
                                if let Err(err) = send_delivery_receipt(
                                    manager,
                                    &state.limiters.send,
                                    &state.metrics,
                                    &content,
                                )
                                .await
                                {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn mention(start: u32, length: u32, aci: Uuid) -> BodyRange {
        BodyRange {
            start: Some(start),
            length: Some(length),
            associated_value: Some(AssociatedValue::MentionAci(aci.to_string())),
        }
    }

    fn message(body: &str, body_ranges: Vec<BodyRange>) -> DataMessage {
        DataMessage {
            body: Some(body.to_owned()),
            body_ranges,
            ..Default::default()
        }
    }

    const BOT: Uuid = Uuid::from_u128(0xB07);
    const OTHER: Uuid = Uuid::from_u128(0x07E);

    type TestError = presage::Error<std::io::Error>;

    fn rate_limited(seconds: Option<i64>) -> TestError {
        presage::Error::ServiceError(
            presage::libsignal_service::prelude::ServiceError::RateLimitExceeded {
                retry_after: seconds.map(chrono::Duration::seconds),
            },
        )
    }

    #[test]
    fn rate_limits_with_short_waits_retry_inline() {
        for (seconds, expected) in [
            (Some(0), Some(Duration::from_secs(1))),
            (Some(1), Some(Duration::from_secs(1))),
            (Some(2), Some(Duration::from_secs(2))),
            (Some(3), None),
            (Some(600), None),
            (None, None),
        ] {
            let failure = classify(&rate_limited(seconds));
            assert!(matches!(failure, SendFailure::RateLimited(_)));
            assert_eq!(
                inline_retry_wait(&failure),
                expected,
                "retry_after {seconds:?}"
            );
        }
    }

    #[test]
    fn proof_required_is_a_challenge_and_not_retried() {
        let sender_error: TestError =
            presage::libsignal_service::prelude::MessageSenderError::ProofRequired {
                token: "tok".to_owned(),
                options: vec!["captcha".to_owned()],
            }
            .into();
        let service_error: TestError = presage::Error::ServiceError(
            presage::libsignal_service::prelude::ServiceError::ProofRequiredError(
                presage::libsignal_service::push_service::ProofRequired {
                    token: "tok".to_owned(),
                    options: vec!["captcha".to_owned()],
                },
            ),
        );
        for error in [sender_error, service_error] {
            let failure = classify(&error);
            assert!(matches!(&failure, SendFailure::ChallengeRequired(t) if t == "tok"));
            assert_eq!(inline_retry_wait(&failure), None);
            assert_eq!(
                outcome_of(failure),
                SendOutcome::ChallengeRequired {
                    challenge_token: "tok".to_owned()
                }
            );
        }
    }

    #[test]
    fn other_errors_are_reported_as_failing() {
        let error: TestError = presage::Error::ServiceError(
            presage::libsignal_service::prelude::ServiceError::Unauthorized,
        );
        assert_eq!(outcome_of(classify(&error)), SendOutcome::Failing);
    }

    #[test]
    fn dropped_rate_limit_reports_when_to_retry() {
        let before = Utc::now();
        let outcome = outcome_of(classify(&rate_limited(Some(600))));
        let SendOutcome::RateLimited { retry_at: Some(at) } = outcome else {
            panic!("expected a retry time, got {outcome:?}");
        };
        assert!(at >= before + chrono::Duration::seconds(600));
        assert_eq!(
            outcome_of(classify(&rate_limited(None))),
            SendOutcome::RateLimited { retry_at: None }
        );
    }

    #[test]
    fn captcha_response_accepts_link_or_bare_value() {
        assert_eq!(captcha_response("signalcaptcha://abc.def"), "abc.def");
        assert_eq!(captcha_response("  signalcaptcha://abc.def\n"), "abc.def");
        assert_eq!(captcha_response("abc.def"), "abc.def");
    }

    #[test]
    fn due_purges_respect_the_per_user_quota() {
        let quota = governor::Quota::with_period(Duration::from_secs(3600))
            .unwrap()
            .allow_burst(std::num::NonZeroU32::new(1).unwrap());
        let limiter = DefaultKeyedRateLimiter::keyed(quota);
        let pending = RefCell::new(BTreeSet::from(["a".to_owned(), "b".to_owned()]));

        assert!(limiter.check_key(&"a".to_owned()).is_ok());

        assert_eq!(take_due_purges(&pending, &limiter), vec!["b".to_owned()]);
        assert_eq!(*pending.borrow(), BTreeSet::from(["a".to_owned()]));
        assert!(take_due_purges(&pending, &limiter).is_empty());
    }

    #[tokio::test]
    async fn throttle_delays_only_the_busy_recipient() {
        let quota = governor::Quota::with_period(Duration::from_millis(200))
            .unwrap()
            .allow_burst(std::num::NonZeroU32::new(2).unwrap());
        let limiter = DefaultKeyedRateLimiter::keyed(quota);

        let start = std::time::Instant::now();
        throttle(&limiter, BOT).await;
        throttle(&limiter, BOT).await;
        assert!(start.elapsed() < Duration::from_millis(100));

        throttle(&limiter, BOT).await;
        assert!(start.elapsed() >= Duration::from_millis(150));

        let other = std::time::Instant::now();
        throttle(&limiter, OTHER).await;
        assert!(other.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn unmentioned_group_message_is_ignored() {
        assert_eq!(addressed_text(&message("help", vec![]), BOT), None);
    }

    #[test]
    fn mention_of_someone_else_is_ignored() {
        let msg = message("\u{fffc} help", vec![mention(0, 1, OTHER)]);
        assert_eq!(addressed_text(&msg, BOT), None);
    }

    #[test]
    fn bot_mention_is_stripped() {
        let msg = message("\u{fffc} help", vec![mention(0, 1, BOT)]);
        assert_eq!(addressed_text(&msg, BOT).as_deref(), Some("help"));
    }

    #[test]
    fn other_mentions_are_kept() {
        let msg = message(
            "ask \u{fffc} about \u{fffc}",
            vec![mention(4, 1, BOT), mention(12, 1, OTHER)],
        );
        assert_eq!(
            addressed_text(&msg, BOT).as_deref(),
            Some("ask about \u{fffc}")
        );
    }

    #[test]
    fn binary_mention_is_recognised() {
        let msg = message(
            "hi \u{fffc}",
            vec![BodyRange {
                start: Some(3),
                length: Some(1),
                associated_value: Some(AssociatedValue::MentionAciBinary(BOT.as_bytes().to_vec())),
            }],
        );
        assert_eq!(addressed_text(&msg, BOT).as_deref(), Some("hi"));
    }

    #[test]
    fn offsets_are_utf16() {
        let msg = message("\u{1f600} \u{fffc} go", vec![mention(3, 1, BOT)]);
        assert_eq!(addressed_text(&msg, BOT).as_deref(), Some("\u{1f600} go"));
    }

    #[test]
    fn out_of_range_mention_does_not_panic() {
        let msg = message("hi", vec![mention(10, 5, BOT)]);
        assert_eq!(addressed_text(&msg, BOT).as_deref(), Some("hi"));
    }

    #[test]
    fn group_user_id_carries_its_ref() {
        let key = [7u8; 32];
        let user_id = group_user_id(&key, &OTHER);
        assert_eq!(parse_group_ref(&user_id), Some(group_ref(&key).as_str()));
        assert!(Uuid::try_parse(&user_id).is_err());
    }

    #[test]
    fn group_user_id_does_not_contain_the_key() {
        let key = [7u8; 32];
        let user_id = group_user_id(&key, &OTHER);
        assert!(!user_id.contains(&hex::encode(key)));
        assert!(!user_id.contains(&BASE64_STANDARD.encode(key)));
    }

    #[test]
    fn contact_user_id_has_no_group_ref() {
        assert_eq!(parse_group_ref(&OTHER.to_string()), None);
    }

    #[test]
    fn distinct_groups_get_distinct_refs() {
        assert_ne!(group_ref(&[1u8; 32]), group_ref(&[2u8; 32]));
    }

    #[test]
    fn group_members_get_distinct_conversations() {
        let key = [7u8; 32];
        assert_ne!(group_user_id(&key, &BOT), group_user_id(&key, &OTHER));
    }

    #[test]
    fn contact_user_id_is_a_contact() {
        assert!(matches!(
            try_user_id_to_recipient(&OTHER.to_string()),
            Ok(Recipient::Contact(u)) if u == OTHER
        ));
    }
}
