use std::{collections::BTreeMap, ops::Range, sync::Arc};

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
    de::DeserializeOwned,
    ser::{SerializeMap, Serializer},
    Deserialize, Deserializer, Serialize,
};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, MutexGuard};

mod content;
mod db;
mod error;
mod protobuf;
mod protocol;

pub use error::BitpartStoreError;

const SLED_TREE_STATE: &str = "state";

const SLED_KEY_REGISTRATION: &str = "registration";
const SLED_KEY_SCHEMA_VERSION: &str = "schema_version";

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

#[derive(PartialEq, Eq, Clone, Debug, Default, Serialize, Deserialize)]
pub enum SchemaVersion {
    /// prior to any versioning of the schema
    #[default]
    V0 = 0,
    V1 = 1,
    V2 = 2,
    V3 = 3,
    // Introduction of avatars, requires dropping all profiles from the cache
    V4 = 4,
    /// ACI and PNI identity key pairs are moved into dedicated storage keys from registration data
    V5 = 5,
    /// Reset pre-keys after fixing persistence
    V6 = 6,
}

impl SchemaVersion {
    fn current() -> SchemaVersion {
        Self::V6
    }

    /// return an iterator on all the necessary migration steps from another version
    fn steps(self) -> impl Iterator<Item = SchemaVersion> {
        Range {
            start: self as u8 + 1,
            end: Self::current() as u8 + 1,
        }
        .map(|i| match i {
            1 => SchemaVersion::V1,
            2 => SchemaVersion::V2,
            3 => SchemaVersion::V3,
            4 => SchemaVersion::V4,
            5 => SchemaVersion::V5,
            6 => SchemaVersion::V6,
            _ => unreachable!("oops, this not supposed to happen!"),
        })
    }
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
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
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

    async fn schema_version(&self) -> SchemaVersion {
        self.get(SLED_TREE_STATE, SLED_KEY_SCHEMA_VERSION)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    fn decrypt_value<T: DeserializeOwned>(&self, value: Vec<u8>) -> Result<T, BitpartStoreError> {
        Ok(serde_json::from_slice(&value)?)
    }

    fn encrypt_value(&self, value: &impl Serialize) -> Result<Vec<u8>, BitpartStoreError> {
        Ok(serde_json::to_vec(value)?)
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
        // .map(|p| self.decrypt_value(p))
        // .transpose()
        // .map_err(BitpartStoreError::from)
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
            .map(move |(_, value)| self.decrypt_value::<V>(value.clone())))
    }

    async fn insert<K, V>(&self, tree: &str, key: K, value: V) -> Result<bool, BitpartStoreError>
    where
        K: AsRef<[u8]>,
        V: Serialize,
    {
        // let value = self.encrypt_value(&value)?;
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
    id: &str,
    database: &DatabaseConnection,
    migration_conflict_strategy: MigrationConflictStrategy,
) -> Result<(), BitpartStoreError> {
    // let run_migrations = {
    //     let mut store = BitpartStore::new(id, database, OnNewIdentity::Reject).await?;
    //     let schema_version = store.schema_version();
    //     for step in schema_version.steps() {
    //         match &step {
    //             SchemaVersion::V1 => {
    //                 debug!("migrating from v0, nothing to do")
    //             }
    //             SchemaVersion::V2 => {
    //                 debug!("migrating from schema v1 to v2: encrypting state if cipher is enabled");

    //                 // load registration data the old school way
    //                 let registration = store
    //                     .read()
    //                     .open_tree("default")?
    //                     .get(SLED_KEY_REGISTRATION.as_bytes());
    //                 if let Some(data) = registration {
    //                     let state =
    //                         serde_json::from_slice(&data).map_err(BitpartStoreError::from)?;

    //                     // save it the new school way
    //                     store.save_registration_data(&state).await?;

    //                     // remove old data
    //                     let db = store.write();
    //                     db.open_tree("default")?
    //                         .remove(SLED_KEY_REGISTRATION.as_bytes());
    //                     store.flush().await?;
    //                 }
    //             }
    //             SchemaVersion::V3 => {
    //                 debug!("migrating from schema v2 to v3: dropping encrypted group cache");
    //                 store.clear_groups().await?;
    //             }
    //             SchemaVersion::V4 => {
    //                 debug!("migrating from schema v3 to v4: dropping profile cache");
    //                 store.clear_profiles().await?;
    //             }
    //             SchemaVersion::V5 => {
    //                 debug!("migrating from schema v4 to v5: moving identity key pairs");

    //                 #[derive(Deserialize)]
    //                 struct RegistrationDataV4Keys {
    //                     #[serde(with = "serde_private_key", rename = "private_key")]
    //                     pub(crate) aci_private_key: PrivateKey,
    //                     #[serde(with = "serde_identity_key", rename = "public_key")]
    //                     pub(crate) aci_public_key: IdentityKey,
    //                     #[serde(with = "serde_optional_private_key", default)]
    //                     pub(crate) pni_private_key: Option<PrivateKey>,
    //                     #[serde(with = "serde_optional_identity_key", default)]
    //                     pub(crate) pni_public_key: Option<IdentityKey>,
    //                 }

    //                 let run_step: Result<(), BitpartStoreError> = {
    //                     let registration_data: Option<RegistrationDataV4Keys> =
    //                         store.get(SLED_TREE_STATE, SLED_KEY_REGISTRATION)?;
    //                     if let Some(data) = registration_data {
    //                         store
    //                             .set_aci_identity_key_pair(IdentityKeyPair::new(
    //                                 data.aci_public_key,
    //                                 data.aci_private_key,
    //                             ))
    //                             .await?;
    //                         if let Some((public_key, private_key)) =
    //                             data.pni_public_key.zip(data.pni_private_key)
    //                         {
    //                             store
    //                                 .set_pni_identity_key_pair(IdentityKeyPair::new(
    //                                     public_key,
    //                                     private_key,
    //                                 ))
    //                                 .await?;
    //                         }
    //                     }
    //                     Ok(())
    //                 };

    //                 if let Err(error) = run_step {
    //                     error!("failed to run v4 -> v5 migration: {error}");
    //                 }
    //             }
    //             SchemaVersion::V6 => {
    //                 debug!("migrating from schema v5 to v6: new keys encoding in ACI and PNI protocol stores");
    //                 let db = store.read();

    //                 let trees = [
    //                     AciBitpartStore::signed_pre_keys(),
    //                     AciBitpartStore::pre_keys(),
    //                     AciBitpartStore::kyber_pre_keys(),
    //                     AciBitpartStore::kyber_pre_keys_last_resort(),
    //                     PniBitpartStore::signed_pre_keys(),
    //                     PniBitpartStore::pre_keys(),
    //                     PniBitpartStore::kyber_pre_keys(),
    //                     PniBitpartStore::kyber_pre_keys_last_resort(),
    //                 ];

    //                 for tree_name in trees {
    //                     let tree = db.open_tree(tree_name)?;
    //                     let num_keys_before = tree.len();
    //                     let mut data = Vec::new();
    //                     for (k, v) in tree.iter() {
    //                         if let Some(key) = std::str::from_utf8(&k)
    //                             .ok()
    //                             .and_then(|s| s.parse::<u32>().ok())
    //                         {
    //                             data.push((key, v));
    //                         }
    //                     }
    //                     tree.clear();
    //                     for (k, v) in data {
    //                         let _ = tree.insert(Vec::from(k.to_be_bytes()), v.to_owned());
    //                     }
    //                     let num_keys_after = tree.len();
    //                     debug!(tree_name, num_keys_before, num_keys_after, "migrated keys");
    //                 }
    //             }
    //             _ => return Err(BitpartStoreError::MigrationConflict),
    //         }

    //         store
    //             .insert(SLED_TREE_STATE, SLED_KEY_SCHEMA_VERSION, step)
    //             .await?;
    //     }

    //     Ok(())
    // };

    // // let db_path = database
    // //     .get_sqlite_connection_pool()
    // //     .connect_options()
    // //     .get_filename();

    // if let Err(BitpartStoreError::MigrationConflict) = run_migrations {
    //     match migration_conflict_strategy {
    //         // MigrationConflictStrategy::BackupAndDrop => {
    //         //     let mut new_db_path = db_path.clone();
    //         //     new_db_path.set_extension(format!(
    //         //         "{}.backup",
    //         //         SystemTime::now()
    //         //             .duration_since(UNIX_EPOCH)
    //         //             .expect("time doesn't go backwards")
    //         //             .as_secs()
    //         //     ));
    //         //     fs_extra::dir::create_all(&new_db_path, false)?;
    //         //     fs_extra::dir::copy(db_path, new_db_path, &fs_extra::dir::CopyOptions::new())?;
    //         //     fs_extra::dir::remove(db_path)?;
    //         // }
    //         // MigrationConflictStrategy::Drop => {
    //         //     fs_extra::dir::remove(db_path)?;
    //         // }
    //         MigrationConflictStrategy::Raise => return Err(BitpartStoreError::MigrationConflict),
    //     }
    // }

    Ok(())
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

    use crate::SchemaVersion;

    use super::*;

    #[test]
    fn test_migration_steps() {
        let steps: Vec<_> = SchemaVersion::steps(SchemaVersion::V0).collect();
        assert_eq!(
            steps,
            [
                SchemaVersion::V1,
                SchemaVersion::V2,
                SchemaVersion::V3,
                SchemaVersion::V4,
                SchemaVersion::V5,
                SchemaVersion::V6,
            ]
        )
    }
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
