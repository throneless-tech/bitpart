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

#[allow(clippy::derive_partial_eq_without_eq)]

mod textsecure {
    include!(concat!(env!("OUT_DIR"), "/textsecure.rs"));
}

use std::str::FromStr;

use presage::libsignal_service::content::Content;
use presage::libsignal_service::content::ContentBody;
use presage::libsignal_service::content::Metadata;
use presage::libsignal_service::prelude::Uuid;
use presage::libsignal_service::proto;
use presage::libsignal_service::protocol::ServiceId;

use crate::BitpartStoreError;

use self::textsecure::AddressProto;
use self::textsecure::MetadataProto;

impl From<ServiceId> for AddressProto {
    fn from(s: ServiceId) -> Self {
        AddressProto {
            uuid: Some(s.raw_uuid().as_bytes().to_vec()),
        }
    }
}

impl TryFrom<AddressProto> for ServiceId {
    type Error = BitpartStoreError;

    fn try_from(address: AddressProto) -> Result<Self, Self::Error> {
        address
            .uuid
            .and_then(|bytes| Some(Uuid::from_bytes(bytes.try_into().ok()?)))
            .ok_or_else(|| BitpartStoreError::NoUuid)
            .map(|u| ServiceId::Aci(u.into()))
    }
}

impl From<Metadata> for MetadataProto {
    fn from(m: Metadata) -> Self {
        MetadataProto {
            address: Some(m.sender.into()),
            sender_device: m.sender_device.try_into().ok(),
            timestamp: m.timestamp.try_into().ok(),
            server_received_timestamp: None,
            server_delivered_timestamp: None,
            needs_receipt: Some(m.needs_receipt),
            server_guid: None,
            group_id: None,
            destination_uuid: Some(m.destination.raw_uuid().to_string()),
        }
    }
}

impl TryFrom<MetadataProto> for Metadata {
    type Error = BitpartStoreError;

    fn try_from(metadata: MetadataProto) -> Result<Self, Self::Error> {
        Ok(Metadata {
            sender: metadata
                .address
                .ok_or(BitpartStoreError::NoUuid)?
                .try_into()?,
            destination: ServiceId::Aci(
                match metadata.destination_uuid.as_deref() {
                    Some(value) => value.parse().map_err(|_| BitpartStoreError::NoUuid),
                    None => Ok(Uuid::nil()),
                }?
                .into(),
            ),
            sender_device: metadata
                .sender_device
                .and_then(|m| m.try_into().ok())
                .unwrap_or_default(),
            server_guid: metadata
                .server_guid
                .and_then(|u| crate::Uuid::from_str(&u).ok()),
            timestamp: metadata
                .timestamp
                .and_then(|m| m.try_into().ok())
                .unwrap_or_default(),
            needs_receipt: metadata.needs_receipt.unwrap_or_default(),
            unidentified_sender: false,
            was_plaintext: false,
        })
    }
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContentProto {
    #[prost(message, required, tag = "1")]
    metadata: MetadataProto,
    #[prost(message, required, tag = "2")]
    content: proto::Content,
}

impl From<Content> for ContentProto {
    fn from(c: Content) -> Self {
        (c.metadata, c.body).into()
    }
}

impl From<(Metadata, ContentBody)> for ContentProto {
    fn from((metadata, content_body): (Metadata, ContentBody)) -> Self {
        ContentProto {
            metadata: metadata.into(),
            content: content_body.into_proto(),
        }
    }
}

impl TryInto<Content> for ContentProto {
    type Error = BitpartStoreError;

    fn try_into(self) -> Result<Content, Self::Error> {
        let metadata = self.metadata.try_into()?;
        Content::from_proto(self.content, metadata)
            .map_err(|_| BitpartStoreError::UnsupportedContent)
    }
}
