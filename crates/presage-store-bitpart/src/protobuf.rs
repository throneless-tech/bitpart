// presage-store-bitpart
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

use std::str::FromStr;

use chrono::{TimeZone, Utc};
use presage::libsignal_service::content::{Content, ContentBody, Metadata};
use presage::libsignal_service::prelude::Uuid;
use presage::libsignal_service::proto;
use presage::libsignal_service::protocol::{DeviceId, Pni, ServiceId};

use crate::BitpartStoreError;

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContentProto {
    #[prost(bytes = "vec", optional, tag = "1")]
    sender_uuid: Option<Vec<u8>>,
    #[prost(uint32, optional, tag = "2")]
    sender_device: Option<u32>,
    #[prost(int64, optional, tag = "3")]
    client_timestamp: Option<i64>,
    #[prost(int64, optional, tag = "4")]
    server_timestamp: Option<i64>,
    #[prost(bool, optional, tag = "5")]
    needs_receipt: Option<bool>,
    #[prost(string, optional, tag = "6")]
    server_guid: Option<String>,
    #[prost(string, optional, tag = "7")]
    destination_uuid: Option<String>,
    #[prost(string, optional, tag = "8")]
    pni_verified: Option<String>,
    #[prost(bool, optional, tag = "9")]
    unidentified_sender: Option<bool>,
    #[prost(bool, optional, tag = "10")]
    was_plaintext: Option<bool>,
    #[prost(message, required, tag = "11")]
    content: proto::Content,
}

impl From<Content> for ContentProto {
    fn from(c: Content) -> Self {
        (c.metadata, c.body).into()
    }
}

impl From<(Metadata, ContentBody)> for ContentProto {
    #[allow(clippy::unnecessary_fallible_conversions)]
    fn from((metadata, content_body): (Metadata, ContentBody)) -> Self {
        ContentProto {
            sender_uuid: Some(metadata.sender.raw_uuid().as_bytes().to_vec()),
            sender_device: metadata.sender_device.try_into().ok(),
            client_timestamp: Some(metadata.client_timestamp.timestamp_millis()),
            server_timestamp: Some(metadata.server_timestamp.timestamp_millis()),
            needs_receipt: Some(metadata.needs_receipt),
            server_guid: metadata.server_guid.map(|u| u.to_string()),
            destination_uuid: Some(metadata.destination.raw_uuid().to_string()),
            pni_verified: metadata
                .pni_verified
                .as_ref()
                .map(|p| p.service_id_string()),
            unidentified_sender: Some(metadata.unidentified_sender),
            was_plaintext: Some(metadata.was_plaintext),
            content: content_body.into_proto(),
        }
    }
}

impl TryInto<Content> for ContentProto {
    type Error = BitpartStoreError;

    fn try_into(self) -> Result<Content, Self::Error> {
        let sender = self
            .sender_uuid
            .and_then(|bytes| Some(Uuid::from_bytes(bytes.try_into().ok()?)))
            .map(|u| ServiceId::Aci(u.into()))
            .ok_or(BitpartStoreError::NoUuid)?;

        let destination = ServiceId::Aci(
            match self.destination_uuid.as_deref() {
                Some(value) => value.parse().map_err(|_| BitpartStoreError::NoUuid),
                None => Ok(Uuid::nil()),
            }?
            .into(),
        );

        let metadata = Metadata {
            sender,
            destination,
            sender_device: self
                .sender_device
                .and_then(|d| d.try_into().ok())
                .unwrap_or(DeviceId::new(1)?),
            client_timestamp: self
                .client_timestamp
                .and_then(|t| Utc.timestamp_millis_opt(t).single())
                .unwrap_or_default(),
            server_timestamp: self
                .server_timestamp
                .or(self.client_timestamp)
                .and_then(|t| Utc.timestamp_millis_opt(t).single())
                .unwrap_or_default(),
            needs_receipt: self.needs_receipt.unwrap_or_default(),
            unidentified_sender: self.unidentified_sender.unwrap_or_default(),
            was_plaintext: self.was_plaintext.unwrap_or_default(),
            server_guid: self.server_guid.and_then(|u| Uuid::from_str(&u).ok()),
            pni_verified: self
                .pni_verified
                .as_deref()
                .map(|p| {
                    Pni::parse_from_service_id_string(p).ok_or(BitpartStoreError::NoUuid)
                })
                .transpose()?,
        };

        Content::from_proto(self.content, metadata)
            .map_err(|_| BitpartStoreError::UnsupportedContent)
    }
}
