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

use base64::prelude::*;
use presage::{
    libsignal_service::{
        prelude::{ProfileKey, Uuid},
        protocol::IdentityKeyPair,
    },
    manager::RegistrationData,
    model::identity::OnNewIdentity,
    store::{ContentsStore, StateStore, Store},
};
use protocol::{AciBitpartStore, BitpartProtocolStore, BitpartTrees, PniBitpartStore};

use sea_orm::DatabaseConnection;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::str;

mod content;
mod db;
mod error;
mod protobuf;
mod protocol;

pub use error::BitpartStoreError;

#[cfg(test)]
use sea_orm::ConnectionTrait;

const BITPART_TREE_STATE: &str = "state";

const BITPART_KEY_REGISTRATION: &str = "registration";

#[derive(Clone)]
pub struct BitpartStore {
    id: String, // database ID

    db: DatabaseConnection,

    /// Whether to trust new identities automatically (for instance, when a somebody's phone has changed)
    trust_new_identities: OnNewIdentity,
}

impl BitpartStore {
    pub async fn open(
        id: &str,
        database: &DatabaseConnection,
        trust_new_identities: OnNewIdentity,
    ) -> Result<Self, BitpartStoreError> {
        Ok(BitpartStore {
            id: id.to_owned(),
            db: database.clone(),
            trust_new_identities,
        })
    }

    #[cfg(test)]
    async fn temporary() -> Result<Self, BitpartStoreError> {
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        db.execute_unprepared(
            "CREATE TABLE channel (
                    id TEXT PRIMARY KEY,
                    bot_id TEXT,
                    channel_id TEXT,
                    created_at TEXT,
                    updated_at TEXT
                );
                INSERT INTO channel (
                    id,
                    bot_id,
                    channel_id,
                    created_at,
                    updated_at
                ) VALUES(
                    'test',
                    'bot_id',
                    'signal',
                    '1678295210',
                    '1678295210'
                );
                CREATE TABLE channel_state (
                    id TEXT PRIMARY KEY,
                    channel_id TEXT,
                    tree TEXT,
                    key TEXT,
                    value TEXT,
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TRIGGER channel_state_updated_at
                AFTER UPDATE ON channel_state
                FOR EACH ROW
                BEGIN
                    UPDATE channel_state
                    SET updated_at = (datetime('now','localtime'))
                    WHERE id = NEW.id;
                END;",
        )
        .await?;
        Ok(Self {
            id: "test".to_owned(),
            db,
            trust_new_identities: OnNewIdentity::Reject,
        })
    }

    pub async fn get<K, V>(&self, tree: &str, key: K) -> Result<Option<V>, BitpartStoreError>
    where
        K: AsRef<[u8]>,
        V: DeserializeOwned,
    {
        let key = serde_json::to_string(key.as_ref())?;
        if let Some(value) = db::channel_state::get(&self.id, tree, &key, &self.db).await? {
            Ok(Some(serde_json::from_str(&value)?))
        } else {
            Ok(None)
        }
    }

    pub async fn get_all<K, V>(&self, tree: &str) -> Result<Vec<(K, V)>, BitpartStoreError>
    where
        K: AsRef<[u8]> + DeserializeOwned + std::fmt::Debug,
        V: DeserializeOwned + std::fmt::Debug,
    {
        Ok(db::channel_state::get_all(&self.id, tree, &self.db)
            .await?
            .into_iter()
            .flat_map(move |(key, value)| {
                Ok::<(K, V), serde_json::Error>((
                    serde_json::from_str::<K>(&key)?,
                    serde_json::from_str::<V>(&value)?,
                ))
            })
            .collect())
    }

    pub async fn iter<'a, V: DeserializeOwned + 'a>(
        &'a self,
        tree: &str,
    ) -> Result<impl Iterator<Item = Result<V, BitpartStoreError>> + 'a, BitpartStoreError> {
        Ok(db::channel_state::get_all(&self.id, tree, &self.db)
            .await?
            .into_iter()
            .map(move |(_, value)| Ok(serde_json::from_str::<V>(&value)?)))
    }

    async fn insert<K, V>(&self, tree: &str, key: K, value: V) -> Result<bool, BitpartStoreError>
    where
        K: AsRef<[u8]>,
        V: Serialize,
    {
        let key = serde_json::to_string(key.as_ref())?;
        let replaced = db::channel_state::set(
            &self.id,
            tree,
            &key,
            serde_json::to_string(&value)?,
            &self.db,
        )
        .await?;

        Ok(replaced)
    }

    async fn remove<K>(&self, tree: &str, key: K) -> Result<bool, BitpartStoreError>
    where
        K: AsRef<[u8]>,
    {
        let key = serde_json::to_string(key.as_ref())?;
        let removed = db::channel_state::remove(&self.id, tree, &key, &self.db).await?;
        Ok(removed > 0)
    }

    async fn remove_all(&self, tree: &str) -> Result<bool, BitpartStoreError> {
        let removed = db::channel_state::remove_all(&self.id, tree, &self.db).await?;
        Ok(removed > 0)
    }

    fn profile_key_for_uuid(&self, uuid: Uuid, key: ProfileKey) -> String {
        let key = uuid.into_bytes().into_iter().chain(key.get_bytes());

        let mut hasher = Sha256::new();
        hasher.update(key.collect::<Vec<_>>());
        format!("{:x}", hasher.finalize())
    }

    async fn get_identity_key_pair<T: BitpartTrees>(
        &self,
    ) -> Result<Option<IdentityKeyPair>, BitpartStoreError> {
        let key_base64: Option<String> =
            self.get(BITPART_TREE_STATE, T::identity_keypair()).await?;
        let Some(key_base64) = key_base64 else {
            return Ok(None);
        };
        let key_bytes = BASE64_STANDARD.decode(key_base64)?;
        IdentityKeyPair::try_from(&*key_bytes)
            .map(Some)
            .map_err(|e| BitpartStoreError::ProtobufDecode(prost::DecodeError::new(e.to_string())))
    }

    async fn set_identity_key_pair<T: BitpartTrees>(
        &self,
        key_pair: IdentityKeyPair,
    ) -> Result<(), BitpartStoreError> {
        let key_bytes = key_pair.serialize();
        let key_base64 = BASE64_STANDARD.encode(key_bytes);
        self.insert(BITPART_TREE_STATE, T::identity_keypair(), key_base64)
            .await?;
        Ok(())
    }
}

impl StateStore for BitpartStore {
    type StateStoreError = BitpartStoreError;

    async fn load_registration_data(&self) -> Result<Option<RegistrationData>, BitpartStoreError> {
        self.get(BITPART_TREE_STATE, BITPART_KEY_REGISTRATION).await
    }

    async fn set_aci_identity_key_pair(
        &self,
        key_pair: IdentityKeyPair,
    ) -> Result<(), Self::StateStoreError> {
        self.set_identity_key_pair::<AciBitpartStore>(key_pair)
            .await
    }

    async fn set_pni_identity_key_pair(
        &self,
        key_pair: IdentityKeyPair,
    ) -> Result<(), Self::StateStoreError> {
        self.set_identity_key_pair::<PniBitpartStore>(key_pair)
            .await
    }

    async fn save_registration_data(
        &mut self,
        state: &RegistrationData,
    ) -> Result<(), BitpartStoreError> {
        self.insert(BITPART_TREE_STATE, BITPART_KEY_REGISTRATION, state)
            .await?;
        Ok(())
    }

    async fn is_registered(&self) -> bool {
        self.load_registration_data()
            .await
            .unwrap_or_default()
            .is_some()
    }

    async fn clear_registration(&mut self) -> Result<(), BitpartStoreError> {
        // drop registration data (includes identity keys)
        db::channel_state::remove_all(&self.id, BITPART_TREE_STATE, &self.db).await?;
        // drop all saved profile (+avatards) and profile keys
        self.clear_profiles().await?;

        // drop all keys
        self.aci_protocol_store().clear(true).await?;
        self.pni_protocol_store().clear(true).await?;

        Ok(())
    }
}

impl Store for BitpartStore {
    type Error = BitpartStoreError;
    type AciStore = BitpartProtocolStore<AciBitpartStore>;
    type PniStore = BitpartProtocolStore<PniBitpartStore>;

    async fn clear(&mut self) -> Result<(), BitpartStoreError> {
        self.clear_registration().await?;
        self.clear_contents().await?;

        Ok(())
    }

    fn aci_protocol_store(&self) -> Self::AciStore {
        BitpartProtocolStore::aci_protocol_store(self.clone())
    }

    fn pni_protocol_store(&self) -> Self::PniStore {
        BitpartProtocolStore::pni_protocol_store(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use presage::libsignal_service::{
        content::{ContentBody, Metadata},
        prelude::Uuid,
        proto::DataMessage,
        protocol::{PreKeyId, ServiceId},
    };
    use presage::store::ContentsStore;
    use protocol::BitpartPreKeyId;
    use quickcheck::{Arbitrary, Gen};
    use quickcheck_macros::quickcheck;

    use super::*;

    #[derive(Debug, Clone)]
    struct Thread(presage::store::Thread);

    #[derive(Debug, Clone)]
    struct Content(presage::libsignal_service::content::Content);

    impl Arbitrary for Content {
        fn arbitrary(g: &mut Gen) -> Self {
            let timestamp: u64 = Arbitrary::arbitrary(g);
            let contacts = [
                Uuid::from_u128(Arbitrary::arbitrary(g)),
                Uuid::from_u128(Arbitrary::arbitrary(g)),
                Uuid::from_u128(Arbitrary::arbitrary(g)),
            ];
            let sender_uuid: Uuid = *g.choose(&contacts).unwrap();
            let destination_uuid: Uuid = *g.choose(&contacts).unwrap();
            let metadata = Metadata {
                sender: ServiceId::Aci(sender_uuid.into()),
                destination: ServiceId::Aci(destination_uuid.into()),
                sender_device: Arbitrary::arbitrary(g),
                server_guid: None,
                timestamp,
                needs_receipt: Arbitrary::arbitrary(g),
                unidentified_sender: Arbitrary::arbitrary(g),
                was_plaintext: false,
            };
            let content_body = ContentBody::DataMessage(DataMessage {
                body: Arbitrary::arbitrary(g),
                timestamp: Some(timestamp),
                ..Default::default()
            });
            Self(presage::libsignal_service::content::Content::from_body(
                content_body,
                metadata,
            ))
        }
    }

    impl Arbitrary for Thread {
        fn arbitrary(g: &mut Gen) -> Self {
            Self(presage::store::Thread::Contact(Uuid::from_u128(
                Arbitrary::arbitrary(g),
            )))
        }
    }

    fn content_with_timestamp(
        content: &Content,
        ts: u64,
    ) -> presage::libsignal_service::content::Content {
        presage::libsignal_service::content::Content {
            metadata: Metadata {
                timestamp: ts,
                ..content.0.metadata.clone()
            },
            body: content.0.body.clone(),
        }
    }

    #[quickcheck]
    fn compare_pre_keys(mut pre_key_id: u32, mut next_pre_key_id: u32) {
        if pre_key_id > next_pre_key_id {
            std::mem::swap(&mut pre_key_id, &mut next_pre_key_id);
        }
        assert!(
            PreKeyId::from(pre_key_id).store_key() <= PreKeyId::from(next_pre_key_id).store_key()
        )
    }

    #[quickcheck_async::tokio]
    async fn test_store_messages(thread: Thread, content: Content) -> anyhow::Result<()> {
        let db = BitpartStore::temporary().await?;
        let thread = thread.0;
        db.save_message(&thread, content_with_timestamp(&content, 1678295210))
            .await?;
        db.save_message(&thread, content_with_timestamp(&content, 1678295220))
            .await?;
        db.save_message(&thread, content_with_timestamp(&content, 1678295230))
            .await?;
        db.save_message(&thread, content_with_timestamp(&content, 1678295240))
            .await?;
        db.save_message(&thread, content_with_timestamp(&content, 1678280000))
            .await?;

        assert_eq!(db.messages(&thread, ..).await.unwrap().count(), 5);
        assert_eq!(db.messages(&thread, 0..).await.unwrap().count(), 5);
        assert_eq!(db.messages(&thread, 1678280000..).await.unwrap().count(), 5);

        assert_eq!(db.messages(&thread, 0..1678280000).await?.count(), 0);
        assert_eq!(db.messages(&thread, 0..1678295210).await?.count(), 1);
        assert_eq!(
            db.messages(&thread, 1678295210..1678295240).await?.count(),
            3
        );
        assert_eq!(
            db.messages(&thread, 1678295210..=1678295240).await?.count(),
            4
        );

        assert_eq!(
            db.messages(&thread, 0..=1678295240)
                .await?
                .next()
                .unwrap()?
                .metadata
                .timestamp,
            1678280000
        );
        assert_eq!(
            db.messages(&thread, 0..=1678295240)
                .await?
                .next_back()
                .unwrap()?
                .metadata
                .timestamp,
            1678295240
        );

        Ok(())
    }
}
