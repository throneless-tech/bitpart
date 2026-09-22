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
        libsignal_account_keys::AccountEntropyPool,
        prelude::{MasterKey, ProfileKey, Uuid},
        protocol::{IdentityKeyPair, SenderCertificate},
        zkgroup::GroupMasterKeyBytes,
    },
    manager::RegistrationData,
    model::identity::OnNewIdentity,
    store::{ContentsStore, StateStore, Store, Thread},
};
use protocol::BitpartProtocolStore;
use tracing::warn;

use deadpool_sqlite::Pool;
use sha2::{Digest, Sha256};
use std::str;
use std::str::FromStr;

mod content;
mod db;
mod error;
mod protobuf;
mod protocol;

pub use error::BitpartStoreError;

pub fn group_ref(master_key: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"bitpart:group:");
    hasher.update(master_key);
    hasher.finalize()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

const BITPART_KEY_REGISTRATION: &str = "registration";
const BITPART_KEY_SENDER_CERTIFICATE: &str = "sender_certificate";
const BITPART_KEY_MASTER: &str = "master";
const BITPART_KEY_ACCOUNT_ENTROPY_POOL: &str = "account_entropy_pool";

#[derive(Clone)]
pub struct BitpartStore {
    id: String, // database ID

    pool: Pool,

    /// Whether to trust new identities automatically (for instance, when a somebody's phone has changed)
    trust_new_identities: OnNewIdentity,

    #[cfg(test)]
    _temp_dir: Option<std::sync::Arc<tempfile::TempDir>>,
}

impl BitpartStore {
    pub async fn open(
        id: &str,
        pool: &Pool,
        trust_new_identities: OnNewIdentity,
    ) -> Result<Self, BitpartStoreError> {
        Ok(BitpartStore {
            id: id.to_owned(),
            pool: pool.clone(),
            trust_new_identities,
            #[cfg(test)]
            _temp_dir: None,
        })
    }

    pub async fn aci_sessions(&self) -> Result<Vec<(String, Vec<u8>)>, BitpartStoreError> {
        db::sessions::get_all_aci(&self.id, &self.pool).await
    }

    pub async fn group_by_ref(
        &self,
        group_ref: &str,
    ) -> Result<Option<GroupMasterKeyBytes>, BitpartStoreError> {
        Ok(
            db::groups::get_master_key_by_ref(&self.id, group_ref, &self.pool)
                .await?
                .and_then(|key| key.try_into().ok()),
        )
    }

    pub async fn purge_conversation(&self, user_id: &str) -> Vec<BitpartStoreError> {
        let mut errors = Vec::new();
        let address_pattern = format!("{user_id}%");

        if let Err(error) =
            db::sessions::remove_like_aci(&self.id, &address_pattern, &self.pool).await
        {
            warn!(%error, target = "aci_sessions", "failed to purge");
            errors.push(error);
        }
        if let Err(error) =
            db::sessions::remove_like_pni(&self.id, &address_pattern, &self.pool).await
        {
            warn!(%error, target = "pni_sessions", "failed to purge");
            errors.push(error);
        }
        if let Err(error) =
            db::identities::remove_like(&self.id, &address_pattern, &self.pool).await
        {
            warn!(%error, target = "identities", "failed to purge");
            errors.push(error);
        }
        if let Err(error) =
            db::sender_keys::remove_like_aci(&self.id, &address_pattern, &self.pool).await
        {
            warn!(%error, target = "aci_sender_keys", "failed to purge");
            errors.push(error);
        }
        if let Err(error) =
            db::sender_keys::remove_like_pni(&self.id, &address_pattern, &self.pool).await
        {
            warn!(%error, target = "pni_sender_keys", "failed to purge");
            errors.push(error);
        }

        if let Ok(uuid) = Uuid::parse_str(user_id) {
            match db::profiles::remove_profile_key(&self.id, uuid.as_bytes(), &self.pool).await {
                Ok(Some(key_bytes)) => {
                    if let Ok(key_bytes) = <[u8; 32]>::try_from(&key_bytes[..]) {
                        let profile_hash =
                            self.profile_key_for_uuid(uuid, ProfileKey::create(key_bytes));
                        if let Err(error) =
                            db::profiles::remove_profile(&self.id, &profile_hash, &self.pool).await
                        {
                            warn!(%error, target = "profiles", "failed to purge");
                            errors.push(error);
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    warn!(%error, target = "profile_keys", "failed to purge");
                    errors.push(error);
                }
            }

            let thread_id = crate::content::messages_thread_id(&Thread::Contact(
                presage::libsignal_service::protocol::ServiceId::Aci(uuid.into()),
            ));
            if let Err(error) = db::messages::clear_thread(&self.id, &thread_id, &self.pool).await {
                warn!(%error, target = "messages", "failed to purge");
                errors.push(error);
            }
            if let Err(error) = db::expire_timers::remove(&self.id, &thread_id, &self.pool).await {
                warn!(%error, target = "expire_timers", "failed to purge");
                errors.push(error);
            }
        }

        errors
    }

    #[cfg(test)]
    async fn temporary() -> Result<Self, BitpartStoreError> {
        use deadpool_sqlite::{Config, Hook, HookError, Runtime};

        // File-backed: deadpool's `:memory:` gives each connection its
        // own private DB.
        let dir =
            tempfile::tempdir().map_err(|e| BitpartStoreError::Pool(format!("tempdir: {e}")))?;
        let path = dir.path().join("presage-test.sqlite");

        // V2 schema DDL (from bitpart-common/src/db/schema_v2.sql)
        const TEMP_DDL: &str = "
            CREATE TABLE channel (
                id TEXT PRIMARY KEY,
                bot_id TEXT,
                channel_id TEXT,
                created_at TEXT,
                updated_at TEXT
            );
            INSERT INTO channel (id, bot_id, channel_id, created_at, updated_at)
                VALUES ('test', 'bot_id', 'signal', '1678295210', '1678295210');

            -- Protocol tables for ACI
            CREATE TABLE signal_identities (
                channel_id varchar NOT NULL,
                is_pni integer NOT NULL,
                address varchar NOT NULL,
                identity_key blob NOT NULL,
                PRIMARY KEY (channel_id, is_pni, address)
            );
            CREATE TABLE signal_sessions (
                channel_id varchar NOT NULL,
                address varchar NOT NULL,
                session_data blob NOT NULL,
                PRIMARY KEY (channel_id, address)
            );
            CREATE TABLE signal_pre_keys (
                channel_id varchar NOT NULL,
                key_id integer NOT NULL,
                record_data blob NOT NULL,
                PRIMARY KEY (channel_id, key_id)
            );
            CREATE TABLE signal_signed_pre_keys (
                channel_id varchar NOT NULL,
                key_id integer NOT NULL,
                record_data blob NOT NULL,
                PRIMARY KEY (channel_id, key_id)
            );
            CREATE TABLE signal_kyber_pre_keys (
                channel_id varchar NOT NULL,
                key_id integer NOT NULL,
                record_data blob NOT NULL,
                is_last_resort integer NOT NULL DEFAULT 0,
                PRIMARY KEY (channel_id, key_id)
            );
            CREATE TABLE signal_sender_keys (
                channel_id varchar NOT NULL,
                sender_key varchar NOT NULL,
                record_data blob NOT NULL,
                PRIMARY KEY (channel_id, sender_key)
            );
            CREATE TABLE signal_base_keys_seen (
                channel_id varchar NOT NULL,
                is_pni integer NOT NULL,
                kyber_pre_key_id integer NOT NULL,
                signed_pre_key_id integer NOT NULL,
                base_key blob NOT NULL,
                PRIMARY KEY (channel_id, is_pni, kyber_pre_key_id)
            );
            CREATE TABLE signal_state (
                channel_id varchar NOT NULL,
                key varchar NOT NULL,
                value blob NOT NULL,
                PRIMARY KEY (channel_id, key)
            );
            -- Protocol tables for PNI
            CREATE TABLE signal_pni_sessions (
                channel_id varchar NOT NULL,
                address varchar NOT NULL,
                session_data blob NOT NULL,
                PRIMARY KEY (channel_id, address)
            );
            CREATE TABLE signal_pni_pre_keys (
                channel_id varchar NOT NULL,
                key_id integer NOT NULL,
                record_data blob NOT NULL,
                PRIMARY KEY (channel_id, key_id)
            );
            CREATE TABLE signal_pni_signed_pre_keys (
                channel_id varchar NOT NULL,
                key_id integer NOT NULL,
                record_data blob NOT NULL,
                PRIMARY KEY (channel_id, key_id)
            );
            CREATE TABLE signal_pni_kyber_pre_keys (
                channel_id varchar NOT NULL,
                key_id integer NOT NULL,
                record_data blob NOT NULL,
                is_last_resort integer NOT NULL DEFAULT 0,
                PRIMARY KEY (channel_id, key_id)
            );
            CREATE TABLE signal_pni_sender_keys (
                channel_id varchar NOT NULL,
                sender_key varchar NOT NULL,
                record_data blob NOT NULL,
                PRIMARY KEY (channel_id, sender_key)
            );
            CREATE TABLE signal_pni_state (
                channel_id varchar NOT NULL,
                key varchar NOT NULL,
                value blob NOT NULL,
                PRIMARY KEY (channel_id, key)
            );
            -- Content tables
            CREATE TABLE signal_profiles (
                channel_id varchar NOT NULL,
                profile_hash varchar NOT NULL,
                profile_data blob NOT NULL,
                PRIMARY KEY (channel_id, profile_hash)
            );
            CREATE TABLE signal_profile_keys (
                channel_id varchar NOT NULL,
                uuid blob NOT NULL,
                profile_key blob NOT NULL,
                PRIMARY KEY (channel_id, uuid)
            );
            CREATE TABLE signal_profile_avatars (
                channel_id varchar NOT NULL,
                profile_hash varchar NOT NULL,
                avatar_data blob NOT NULL,
                PRIMARY KEY (channel_id, profile_hash)
            );
            CREATE TABLE signal_contacts (
                channel_id varchar NOT NULL,
                uuid blob NOT NULL,
                contact_data blob NOT NULL,
                PRIMARY KEY (channel_id, uuid)
            );
            CREATE TABLE signal_groups (
                channel_id varchar NOT NULL,
                master_key blob NOT NULL,
                group_data blob NOT NULL,
                group_ref varchar,
                PRIMARY KEY (channel_id, master_key)
            );
            CREATE TABLE signal_group_avatars (
                channel_id varchar NOT NULL,
                master_key blob NOT NULL,
                avatar_data blob NOT NULL,
                PRIMARY KEY (channel_id, master_key)
            );
            CREATE TABLE signal_sticker_packs (
                channel_id varchar NOT NULL,
                pack_id blob NOT NULL,
                pack_data blob NOT NULL,
                PRIMARY KEY (channel_id, pack_id)
            );
            CREATE TABLE signal_messages (
                channel_id varchar NOT NULL,
                thread_id varchar NOT NULL,
                timestamp integer NOT NULL,
                content_data blob NOT NULL,
                PRIMARY KEY (channel_id, thread_id, timestamp)
            );

            CREATE TABLE signal_expire_timers (
                channel_id TEXT NOT NULL,
                thread_id TEXT NOT NULL,
                timer INTEGER NOT NULL,
                version INTEGER NOT NULL,
                PRIMARY KEY (channel_id, thread_id)
            );
        ";

        let cfg = Config::new(&path);
        let ddl_once = std::sync::Arc::new(std::sync::Once::new());
        let ddl_once_for_hook = ddl_once.clone();
        let pool = cfg
            .builder(Runtime::Tokio1)
            .map_err(|e| BitpartStoreError::Pool(format!("builder: {e}")))?
            .max_size(4)
            .post_create(Hook::async_fn(move |obj, _metrics| {
                let once = ddl_once_for_hook.clone();
                Box::pin(async move {
                    obj.interact(move |c| -> rusqlite::Result<()> {
                        let mut result = Ok(());
                        once.call_once(|| {
                            result = c.execute_batch(TEMP_DDL);
                        });
                        result
                    })
                    .await
                    .map_err(|e| HookError::message(format!("interact: {e}")))?
                    .map_err(|e| HookError::message(format!("ddl: {e}")))?;
                    Ok(())
                })
            }))
            .build()
            .map_err(|e| BitpartStoreError::Pool(format!("build: {e}")))?;

        // Force the first connection so the DDL runs before any test
        // code grabs the pool.
        let _conn = pool
            .get()
            .await
            .map_err(|e| BitpartStoreError::Pool(format!("warm-up get: {e}")))?;

        Ok(Self {
            id: "test".to_owned(),
            pool,
            trust_new_identities: OnNewIdentity::Reject,
            _temp_dir: Some(std::sync::Arc::new(dir)),
        })
    }

    fn profile_key_for_uuid(&self, uuid: Uuid, key: ProfileKey) -> String {
        let key = uuid.into_bytes().into_iter().chain(key.get_bytes());

        let mut hasher = Sha256::new();
        hasher.update(key.collect::<Vec<_>>());
        format!("{:x}", hasher.finalize())
    }
}

impl StateStore for BitpartStore {
    type StateStoreError = BitpartStoreError;

    async fn load_registration_data(
        &self,
    ) -> Result<Option<RegistrationData>, Self::StateStoreError> {
        if let Some(data) =
            db::state::get_aci(&self.id, BITPART_KEY_REGISTRATION, &self.pool).await?
        {
            Ok(Some(serde_json::from_slice(&data)?))
        } else {
            Ok(None)
        }
    }

    async fn set_aci_identity_key_pair(
        &self,
        key_pair: IdentityKeyPair,
    ) -> Result<(), Self::StateStoreError> {
        let key_bytes = key_pair.serialize();
        let key_base64 = BASE64_STANDARD.encode(key_bytes);
        db::state::set_aci(
            &self.id,
            "aci_identity_key_pair",
            key_base64.as_bytes(),
            &self.pool,
        )
        .await?;
        Ok(())
    }

    async fn set_pni_identity_key_pair(
        &self,
        key_pair: IdentityKeyPair,
    ) -> Result<(), Self::StateStoreError> {
        let key_bytes = key_pair.serialize();
        let key_base64 = BASE64_STANDARD.encode(key_bytes);
        db::state::set_pni(
            &self.id,
            "pni_identity_key_pair",
            key_base64.as_bytes(),
            &self.pool,
        )
        .await?;
        Ok(())
    }

    async fn save_registration_data(
        &mut self,
        state: &RegistrationData,
    ) -> Result<(), Self::StateStoreError> {
        let data = serde_json::to_vec(state)?;
        db::state::set_aci(&self.id, BITPART_KEY_REGISTRATION, &data, &self.pool).await?;
        Ok(())
    }

    async fn is_registered(&self) -> bool {
        self.load_registration_data()
            .await
            .unwrap_or_default()
            .is_some()
    }

    async fn clear_registration(&mut self) -> Result<(), Self::StateStoreError> {
        // drop registration data (includes identity keys)
        db::state::remove_all_aci(&self.id, &self.pool).await?;
        db::state::remove_all_pni(&self.id, &self.pool).await?;
        // drop all saved profile (+avatards) and profile keys
        self.clear_profiles().await?;

        // drop all keys
        self.aci_protocol_store().clear(true).await?;
        self.pni_protocol_store().clear(true).await?;

        Ok(())
    }

    async fn sender_certificate(&self) -> Result<Option<SenderCertificate>, Self::StateStoreError> {
        if let Some(value) =
            db::state::get_aci(&self.id, BITPART_KEY_SENDER_CERTIFICATE, &self.pool).await?
        {
            Ok(Some(SenderCertificate::deserialize(&value)?))
        } else {
            Ok(None)
        }
    }

    async fn save_sender_certificate(
        &self,
        certificate: &SenderCertificate,
    ) -> Result<(), Self::StateStoreError> {
        db::state::set_aci(
            &self.id,
            BITPART_KEY_SENDER_CERTIFICATE,
            certificate.serialized()?,
            &self.pool,
        )
        .await?;
        Ok(())
    }

    async fn fetch_master_key(&self) -> Result<Option<MasterKey>, Self::StateStoreError> {
        if let Some(value) = db::state::get_aci(&self.id, BITPART_KEY_MASTER, &self.pool).await? {
            Ok(Some(MasterKey::from_slice(&value)?))
        } else {
            Ok(None)
        }
    }

    async fn store_master_key(
        &self,
        master_key: Option<&MasterKey>,
    ) -> Result<(), Self::StateStoreError> {
        if let Some(key) = master_key {
            db::state::set_aci(&self.id, BITPART_KEY_MASTER, &key.inner[..], &self.pool).await?;
        } else {
            db::state::remove_aci(&self.id, BITPART_KEY_MASTER, &self.pool).await?;
        }
        Ok(())
    }

    async fn fetch_account_entropy_pool(
        &self,
    ) -> Result<Option<AccountEntropyPool>, Self::StateStoreError> {
        if let Some(value) =
            db::state::get_aci(&self.id, BITPART_KEY_ACCOUNT_ENTROPY_POOL, &self.pool).await?
        {
            let pool = AccountEntropyPool::from_str(str::from_utf8(&value)?)
                .map_err(|e| BitpartStoreError::Store(e.to_string()))?;
            Ok(Some(pool))
        } else {
            Ok(None)
        }
    }

    async fn store_account_entropy_pool(
        &self,
        account_entropy_pool: Option<&AccountEntropyPool>,
    ) -> Result<(), Self::StateStoreError> {
        if let Some(pool) = account_entropy_pool {
            db::state::set_aci(
                &self.id,
                BITPART_KEY_ACCOUNT_ENTROPY_POOL,
                pool.to_string().as_bytes(),
                &self.pool,
            )
            .await?;
        } else {
            db::state::remove_aci(&self.id, BITPART_KEY_ACCOUNT_ENTROPY_POOL, &self.pool).await?;
        }
        Ok(())
    }
}

impl Store for BitpartStore {
    type Error = BitpartStoreError;
    type AciStore = BitpartProtocolStore;
    type PniStore = BitpartProtocolStore;

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
    use chrono::{TimeZone, Utc};
    use presage::libsignal_service::{
        content::{ContentBody, Metadata},
        prelude::Uuid,
        proto::DataMessage,
        protocol::ServiceId,
    };
    use presage::store::ContentsStore;
    use quickcheck::{Arbitrary, Gen};
    use quickcheck_macros::quickcheck;
    use rand::prelude::*;

    use super::*;

    #[derive(Debug, Clone)]
    struct Thread(presage::store::Thread);

    #[derive(Debug, Clone)]
    struct Content(presage::libsignal_service::content::Content);

    impl Arbitrary for Content {
        fn arbitrary(g: &mut Gen) -> Self {
            // Bound to a representable millisecond range: DateTime<Utc> cannot hold an
            // arbitrary u64, and Signal timestamps are epoch millis anyway.
            let timestamp: u64 = u64::arbitrary(g) % 253_402_300_799_000;
            let contacts = [
                Uuid::from_u128(Arbitrary::arbitrary(g)),
                Uuid::from_u128(Arbitrary::arbitrary(g)),
                Uuid::from_u128(Arbitrary::arbitrary(g)),
            ];
            let sender_uuid: Uuid = *g.choose(&contacts).unwrap();
            let destination_uuid: Uuid = *g.choose(&contacts).unwrap();
            let sender_device: u8 = rand::rng().random_range(1..127);
            let metadata = Metadata {
                sender: ServiceId::Aci(sender_uuid.into()),
                destination: ServiceId::Aci(destination_uuid.into()),
                sender_device: sender_device.try_into().unwrap(),
                server_guid: None,
                pni_verified: None,
                client_timestamp: Utc.timestamp_millis_opt(timestamp as i64).unwrap(),
                server_timestamp: Utc.timestamp_millis_opt(timestamp as i64).unwrap(),
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
            Self(presage::store::Thread::Contact(
                presage::libsignal_service::protocol::ServiceId::Aci(
                    Uuid::from_u128(Arbitrary::arbitrary(g)).into(),
                ),
            ))
        }
    }

    fn content_with_timestamp(
        content: &Content,
        ts: u64,
    ) -> presage::libsignal_service::content::Content {
        presage::libsignal_service::content::Content {
            metadata: Metadata {
                client_timestamp: Utc.timestamp_millis_opt(ts as i64).unwrap(),
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
        assert!(pre_key_id <= next_pre_key_id)
    }

    #[quickcheck_async::tokio]
    async fn test_messages_are_not_retained(
        thread: Thread,
        content: Content,
    ) -> anyhow::Result<()> {
        let db = BitpartStore::temporary().await?;
        let thread = thread.0;
        for ts in [1678295210, 1678295220, 1678295230, 1678295240, 1678280000] {
            db.save_message(&thread, content_with_timestamp(&content, ts))
                .await?;
        }

        assert_eq!(db.messages(&thread, ..).await?.count(), 0);
        assert_eq!(db.messages(&thread, 0..).await?.count(), 0);
        assert_eq!(db.messages(&thread, 1678280000..).await?.count(), 0);
        assert_eq!(db.messages(&thread, 0..=1678295240).await?.count(), 0);

        Ok(())
    }

    #[quickcheck_async::tokio]
    async fn test_thread_for_sender_and_timestamp_finds_nothing(
        content: Content,
    ) -> anyhow::Result<()> {
        let db = BitpartStore::temporary().await?;
        let ts = 1678295210;
        let content = content_with_timestamp(&content, ts);
        let sender = content.metadata.sender;

        if let Some(thread) = presage::store::Thread::try_from(&content).ok() {
            db.save_message(&thread, content.clone()).await?;
        }

        assert_eq!(db.thread_for_sender_and_timestamp(&sender, ts).await?, None);

        let other = ServiceId::Aci(Uuid::from_u128(0xDEAD_BEEF).into());
        assert_eq!(db.thread_for_sender_and_timestamp(&other, ts).await?, None);

        Ok(())
    }

    #[tokio::test]
    async fn test_profile_key_round_trip() -> anyhow::Result<()> {
        let mut store = BitpartStore::temporary().await?;

        let test_uuid = Uuid::from_u128(0x12345678_9ABC_DEF0_1234_567890ABCDEF);
        let test_key = ProfileKey::create([42u8; 32]);

        store.upsert_profile_key(&test_uuid, test_key).await?;

        let service_id = ServiceId::Aci(test_uuid.into());
        let retrieved_key = store.profile_key(&service_id).await?;

        assert!(retrieved_key.is_some());
        let retrieved_key = retrieved_key.unwrap();
        assert_eq!(retrieved_key.get_bytes(), test_key.get_bytes());

        Ok(())
    }

    #[tokio::test]
    async fn test_purge_conversation_is_scoped_to_one_user() -> anyhow::Result<()> {
        let store = BitpartStore::temporary().await?;
        let ch = &store.id;
        let pool = &store.pool;

        let target = Uuid::from_u128(0x1111);
        let other = Uuid::from_u128(0x2222);
        let target_id = target.to_string();
        let other_id = other.to_string();

        for (uuid, dev) in [(&target_id, "1"), (&target_id, "2"), (&other_id, "1")] {
            let addr = format!("{uuid}.{dev}");
            db::sessions::set_aci(ch, &addr, b"session", pool).await?;
            db::sessions::set_pni(ch, &addr, b"session", pool).await?;
            db::identities::set(ch, false, &addr, b"identity", pool).await?;
            let sk = format!("{addr}:dist");
            db::sender_keys::set_aci(ch, &sk, b"sender_key", pool).await?;
            db::sender_keys::set_pni(ch, &sk, b"sender_key", pool).await?;
        }

        let key_bytes = [7u8; 32];
        for uuid in [&target, &other] {
            db::profiles::set_profile_key(ch, uuid.as_bytes(), &key_bytes, pool).await?;
            let hash = store.profile_key_for_uuid(*uuid, ProfileKey::create(key_bytes));
            db::profiles::set_profile(ch, &hash, b"profile", pool).await?;
        }

        for uuid in [&target, &other] {
            let thread_id = crate::content::messages_thread_id(&presage::store::Thread::Contact(
                presage::libsignal_service::protocol::ServiceId::Aci((*uuid).into()),
            ));
            db::messages::set(ch, &thread_id, 1000, b"content", pool).await?;
            db::expire_timers::set_if_newer(ch, &thread_id, 3600, 1, pool).await?;
        }

        db::pre_keys::set_aci(ch, 1, b"pre_key", pool).await?;

        let errors = store.purge_conversation(&target_id).await;
        assert!(errors.is_empty(), "purge reported errors: {errors:?}");

        let target_thread = crate::content::messages_thread_id(&presage::store::Thread::Contact(
            presage::libsignal_service::protocol::ServiceId::Aci(target.into()),
        ));
        let other_thread = crate::content::messages_thread_id(&presage::store::Thread::Contact(
            presage::libsignal_service::protocol::ServiceId::Aci(other.into()),
        ));
        let other_addr = format!("{other_id}.1");
        let other_hash = store.profile_key_for_uuid(other, ProfileKey::create(key_bytes));

        for dev in ["1", "2"] {
            let addr = format!("{target_id}.{dev}");
            assert_eq!(db::sessions::get_aci(ch, &addr, pool).await?, None);
            assert_eq!(db::sessions::get_pni(ch, &addr, pool).await?, None);
            assert_eq!(db::identities::get(ch, false, &addr, pool).await?, None);
            let sk = format!("{addr}:dist");
            assert_eq!(db::sender_keys::get_aci(ch, &sk, pool).await?, None);
            assert_eq!(db::sender_keys::get_pni(ch, &sk, pool).await?, None);
        }
        assert_eq!(
            db::profiles::get_profile_key(ch, target.as_bytes(), pool).await?,
            None
        );
        assert_eq!(
            db::messages::get_all(ch, &target_thread, pool).await?.len(),
            0
        );
        assert_eq!(
            db::expire_timers::get(ch, &target_thread, pool).await?,
            None
        );

        assert!(
            db::sessions::get_aci(ch, &other_addr, pool)
                .await?
                .is_some()
        );
        assert!(
            db::sessions::get_pni(ch, &other_addr, pool)
                .await?
                .is_some()
        );
        assert!(
            db::identities::get(ch, false, &other_addr, pool)
                .await?
                .is_some()
        );
        let other_sk = format!("{other_addr}:dist");
        assert!(
            db::sender_keys::get_aci(ch, &other_sk, pool)
                .await?
                .is_some()
        );
        assert!(
            db::sender_keys::get_pni(ch, &other_sk, pool)
                .await?
                .is_some()
        );
        assert!(
            db::profiles::get_profile_key(ch, other.as_bytes(), pool)
                .await?
                .is_some()
        );
        assert!(
            db::profiles::get_profile(ch, &other_hash, pool)
                .await?
                .is_some()
        );
        assert_eq!(
            db::messages::get_all(ch, &other_thread, pool).await?.len(),
            1
        );
        assert_eq!(
            db::expire_timers::get(ch, &other_thread, pool).await?,
            Some((3600, 1))
        );

        assert!(db::pre_keys::get_aci(ch, 1, pool).await?.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_purge_conversation_allows_reinitiation() -> anyhow::Result<()> {
        let store = BitpartStore::temporary().await?;
        let ch = &store.id;
        let pool = &store.pool;

        let user = Uuid::from_u128(0x3333);
        let user_id = user.to_string();
        let addr = format!("{user_id}.1");

        db::sessions::set_aci(ch, &addr, b"session", pool).await?;
        assert!(store.purge_conversation(&user_id).await.is_empty());
        assert_eq!(db::sessions::get_aci(ch, &addr, pool).await?, None);

        db::sessions::set_aci(ch, &addr, b"new_session", pool).await?;
        assert_eq!(
            db::sessions::get_aci(ch, &addr, pool).await?,
            Some(b"new_session".to_vec())
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_purge_conversation_tolerates_non_uuid() -> anyhow::Result<()> {
        let store = BitpartStore::temporary().await?;
        let errors = store.purge_conversation("not-a-uuid").await;
        assert!(errors.is_empty(), "purge reported errors: {errors:?}");
        Ok(())
    }
}
