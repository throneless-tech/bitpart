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

use std::{collections::BTreeMap, sync::Arc};

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
use serde::{
    Deserialize, Deserializer, Serialize,
    de::DeserializeOwned,
    ser::{SerializeMap, Serializer},
};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, MutexGuard};

mod content;
mod db;
mod error;
mod protobuf;
mod protocol;

pub use error::BitpartStoreError;

#[cfg(test)]
use sea_orm::ConnectionTrait;

const SLED_TREE_STATE: &str = "state";

const SLED_KEY_REGISTRATION: &str = "registration";

// In-memory stand-in for Sled
#[derive(Debug, Deserialize)]
struct DoubleMap {
    #[serde(flatten, deserialize_with = "from_string_keys")]
    pub trees: BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, Vec<u8>>>,
}

fn from_string_keys<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, Vec<u8>>>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut new_map = DoubleMap::new();
    let m: BTreeMap<String, Vec<u8>> = Deserialize::deserialize(deserializer)?;

    for (k, v) in m {
        let s = k.split(":").collect::<Vec<&str>>();
        new_map
            .open_tree(s[0])
            .unwrap()
            .insert(s[1].as_bytes().to_vec(), v);
    }

    Ok(new_map.trees)
}

impl Serialize for DoubleMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.trees.len()))?;
        for (k, v) in self.trees.iter() {
            let outer_key = String::from_utf8_lossy(k.as_ref());
            for (k, v) in v.iter() {
                let inner_key = String::from_utf8_lossy(k.as_ref());
                let key = format!("{outer_key}:{inner_key}");
                map.serialize_entry(&key, &v)?;
            }
        }
        map.end()
    }
}

impl DoubleMap {
    fn new() -> Self {
        DoubleMap {
            trees: BTreeMap::new(),
        }
    }

    fn open_tree<V: AsRef<[u8]>>(
        &mut self,
        tree: V,
    ) -> Result<&mut BTreeMap<Vec<u8>, Vec<u8>>, BitpartStoreError> {
        Ok(self
            .trees
            .entry(tree.as_ref().to_vec())
            .or_insert(BTreeMap::new()))
    }

    fn drop_tree<V: AsRef<[u8]>>(&mut self, tree: V) -> Result<bool, BitpartStoreError> {
        Ok(self.trees.remove(tree.as_ref()).is_some())
    }

    fn tree_names(&self) -> Vec<Vec<u8>> {
        self.trees.keys().map(|k| k.clone()).collect()
    }
}

#[derive(Clone)]
pub struct BitpartStore {
    id: String, // database ID

    db_handle: DatabaseConnection,
    db: Arc<Mutex<DoubleMap>>,

    /// Whether to trust new identities automatically (for instance, when a somebody's phone has changed)
    trust_new_identities: OnNewIdentity,
}

/// Sometimes Migrations can't proceed without having to drop existing
/// data. This allows you to configure, how these cases should be handled.
#[derive(Default, PartialEq, Eq, Clone, Debug)]
pub enum MigrationConflictStrategy {
    /// Just drop the data, we don't care that we have to register or link again
    // Drop,
    /// Raise a `Error::MigrationConflict` error with the path to the
    /// DB in question. The caller then has to take care about what they want
    /// to do and try again after.
    #[default]
    Raise,
    // / _Default_: The _entire_ database is backed up under, before the databases are dropped.
    // BackupAndDrop,
}

impl BitpartStore {
    #[allow(unused_variables)]
    async fn new(
        id: &str,
        database: &DatabaseConnection,
        trust_new_identities: OnNewIdentity,
    ) -> Result<Self, BitpartStoreError> {
        let store = db::channel::get_by_id(id, database).await?;
        let state: DoubleMap = match serde_json::from_str(&store.state) {
            Ok(state) => state,
            Err(_) => DoubleMap::new(),
        };
        Ok(BitpartStore {
            id: id.to_owned(),
            db_handle: database.clone(),
            db: Arc::new(Mutex::new(state)),
            trust_new_identities,
        })
    }

    pub async fn flush(&self) -> Result<usize, BitpartStoreError> {
        let state = serde_json::to_string(&*self.read().await).unwrap();
        db::channel::set_by_id(&self.id, &state, &self.db_handle).await?;
        Ok(0)
    }

    pub async fn open(
        id: &str,
        database: &DatabaseConnection,
        migration_conflict_strategy: MigrationConflictStrategy,
        trust_new_identities: OnNewIdentity,
    ) -> Result<Self, BitpartStoreError> {
        Self::open_with_passphrase(
            id,
            database,
            migration_conflict_strategy,
            trust_new_identities,
        )
        .await
    }

    pub async fn open_with_passphrase(
        id: &str,
        database: &DatabaseConnection,
        migration_conflict_strategy: MigrationConflictStrategy,
        trust_new_identities: OnNewIdentity,
    ) -> Result<Self, BitpartStoreError> {
        migrate(id, database, migration_conflict_strategy).await?;
        Self::new(id, database, trust_new_identities).await
    }

    #[cfg(test)]
    async fn temporary() -> Result<Self, BitpartStoreError> {
        let db_handle = sea_orm::Database::connect("sqlite::memory:").await?;
        db_handle
            .execute_unprepared(
                "CREATE TABLE channel (
                    id TEXT PRIMARY KEY,
                    bot_id TEXT,
                    channel_id TEXT,
                    state TEXT,
                    created_at TEXT,
                    updated_at TEXT
                );
                INSERT INTO channel (
                    id,
                    bot_id,
                    channel_id,
                    state,
                    created_at,
                    updated_at
                ) VALUES(
                    'test',
                    'bot_id',
                    'signal',
                    '{}',
                    '1678295210',
                    '1678295210'
                );",
            )
            .await
            .unwrap();
        Ok(Self {
            id: "test".to_owned(),
            db_handle,
            db: Arc::new(Mutex::new(DoubleMap::new())),
            trust_new_identities: OnNewIdentity::Reject,
        })
    }

    async fn read(&self) -> MutexGuard<DoubleMap> {
        self.db.lock().await
    }

    async fn write(&self) -> MutexGuard<DoubleMap> {
        self.db.lock().await
    }

    pub async fn get<K, V>(&self, tree: &str, key: K) -> Result<Option<V>, BitpartStoreError>
    where
        K: AsRef<[u8]>,
        V: DeserializeOwned,
    {
        if let Some(value) = self.read().await.open_tree(tree)?.get(key.as_ref()) {
            Ok(Some(serde_json::from_slice(value)?))
        } else {
            Ok(None)
        }
    }

    pub async fn iter<'a, V: DeserializeOwned + 'a>(
        &'a self,
        tree: &str,
    ) -> Result<impl Iterator<Item = Result<V, BitpartStoreError>> + 'a, BitpartStoreError> {
        Ok(self
            .read()
            .await
            .open_tree(tree)?
            .clone()
            .into_iter()
            .map(move |(_, value)| Ok(serde_json::from_slice::<V>(&value)?)))
    }

    async fn insert<K, V>(&self, tree: &str, key: K, value: V) -> Result<bool, BitpartStoreError>
    where
        K: AsRef<[u8]>,
        V: Serialize,
    {
        let replaced = self
            .write()
            .await
            .open_tree(tree)?
            .insert(key.as_ref().to_owned(), serde_json::to_vec(&value)?);
        self.flush().await?;
        Ok(replaced.is_some())
    }

    async fn remove<K>(&self, tree: &str, key: K) -> Result<bool, BitpartStoreError>
    where
        K: AsRef<[u8]>,
    {
        let removed = self.write().await.open_tree(tree)?.remove(key.as_ref());
        self.flush().await?;
        Ok(removed.is_some())
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
        let key_base64: Option<String> = self.get(SLED_TREE_STATE, T::identity_keypair()).await?;
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
        self.insert(SLED_TREE_STATE, T::identity_keypair(), key_base64)
            .await?;
        Ok(())
    }
}

async fn migrate(
    _id: &str,
    _database: &DatabaseConnection,
    _migration_conflict_strategy: MigrationConflictStrategy,
) -> Result<(), BitpartStoreError> {
    todo!("No migrations yet!");
}

impl StateStore for BitpartStore {
    type StateStoreError = BitpartStoreError;

    async fn load_registration_data(&self) -> Result<Option<RegistrationData>, BitpartStoreError> {
        self.get(SLED_TREE_STATE, SLED_KEY_REGISTRATION).await
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
        self.insert(SLED_TREE_STATE, SLED_KEY_REGISTRATION, state)
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
        {
            let mut db = self.write().await;
            db.open_tree("default")?
                .remove(SLED_KEY_REGISTRATION.as_bytes());
            db.drop_tree(SLED_TREE_STATE)?;
        }

        self.flush().await?;
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
        assert!(PreKeyId::from(pre_key_id).sled_key() <= PreKeyId::from(next_pre_key_id).sled_key())
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
