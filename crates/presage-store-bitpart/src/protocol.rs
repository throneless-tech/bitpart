use std::marker::PhantomData;

use async_trait::async_trait;
use presage::{
    libsignal_service::{
        pre_keys::{KyberPreKeyStoreExt, PreKeysStore},
        prelude::Uuid,
        protocol::{
            Direction, GenericSignedPreKey, IdentityKey, IdentityKeyPair, IdentityKeyStore,
            KyberPreKeyId, KyberPreKeyRecord, KyberPreKeyStore, PreKeyId, PreKeyRecord,
            PreKeyStore, ProtocolAddress, ProtocolStore, SenderKeyRecord, SenderKeyStore,
            ServiceId, SessionRecord, SessionStore, SignalProtocolError, SignedPreKeyId,
            SignedPreKeyRecord, SignedPreKeyStore,
        },
        push_service::DEFAULT_DEVICE_ID,
        session_store::SessionStoreExt,
    },
    proto::verified,
    store::{save_trusted_identity_message, StateStore},
};
use tracing::{error, trace, warn};

use crate::{BitpartStore, BitpartStoreError, OnNewIdentity};

#[derive(Clone)]
pub struct BitpartProtocolStore<T: BitpartTrees> {
    pub(crate) store: BitpartStore,
    _trees: PhantomData<T>,
}

impl BitpartProtocolStore<AciBitpartStore> {
    pub(crate) fn aci_protocol_store(store: BitpartStore) -> Self {
        Self {
            store,
            _trees: Default::default(),
        }
    }
}

impl BitpartProtocolStore<PniBitpartStore> {
    pub(crate) fn pni_protocol_store(store: BitpartStore) -> Self {
        Self {
            store,
            _trees: Default::default(),
        }
    }
}

impl<T: BitpartTrees> BitpartProtocolStore<T> {
    async fn next_key_id(&self, tree: &str) -> Result<u32, SignalProtocolError> {
        Ok(self
            .store
            .read()
            .await
            .open_tree(tree)?
            .keys()
            .last()
            .and_then(|data| Some(u32::from_be_bytes(data[..4].try_into().ok()?)))
            .map_or(0, |id| id + 1))
    }
}

pub trait BitpartTrees: Clone + Send + Sync {
    fn identities() -> &'static str;
    fn state() -> &'static str;
    fn pre_keys() -> &'static str;
    fn signed_pre_keys() -> &'static str;
    fn kyber_pre_keys() -> &'static str;
    fn kyber_pre_keys_last_resort() -> &'static str;
    fn sender_keys() -> &'static str;
    fn sessions() -> &'static str;
    fn identity_keypair() -> &'static str;
}

#[derive(Clone)]
pub struct AciBitpartStore;

impl BitpartTrees for AciBitpartStore {
    fn identities() -> &'static str {
        "identities"
    }

    fn state() -> &'static str {
        "state"
    }

    fn pre_keys() -> &'static str {
        "pre_keys"
    }

    fn signed_pre_keys() -> &'static str {
        "sender_keys"
    }

    fn kyber_pre_keys() -> &'static str {
        "signed_pre_keys"
    }

    fn kyber_pre_keys_last_resort() -> &'static str {
        "kyber_pre_keys_last_resort"
    }

    fn sender_keys() -> &'static str {
        "kyber_pre_keys"
    }

    fn sessions() -> &'static str {
        "sessions"
    }

    fn identity_keypair() -> &'static str {
        "aci_identity_key_pair"
    }
}

#[derive(Clone)]
pub struct PniBitpartStore;

impl BitpartTrees for PniBitpartStore {
    fn identities() -> &'static str {
        "identities"
    }

    fn state() -> &'static str {
        "pni_state"
    }

    fn pre_keys() -> &'static str {
        "pni_pre_keys"
    }

    fn signed_pre_keys() -> &'static str {
        "pni_sender_keys"
    }

    fn kyber_pre_keys() -> &'static str {
        "pni_signed_pre_keys"
    }

    fn kyber_pre_keys_last_resort() -> &'static str {
        "pni_kyber_pre_keys_last_resort"
    }

    fn sender_keys() -> &'static str {
        "pni_kyber_pre_keys"
    }

    fn sessions() -> &'static str {
        "pni_sessions"
    }

    fn identity_keypair() -> &'static str {
        "pni_identity_key_pair"
    }
}

pub(crate) trait BitpartPreKeyId: Into<u32> {
    fn sled_key(self) -> [u8; 4] {
        let idx: u32 = self.into();
        idx.to_be_bytes()
    }
}

impl BitpartPreKeyId for PreKeyId {}
impl BitpartPreKeyId for SignedPreKeyId {}
impl BitpartPreKeyId for KyberPreKeyId {}

impl<T: BitpartTrees> BitpartProtocolStore<T> {
    pub(crate) async fn clear(&self, clear_sessions: bool) -> Result<(), BitpartStoreError> {
        let mut db = self.store.write().await;
        db.drop_tree(T::pre_keys())?;
        db.drop_tree(T::sender_keys())?;
        db.drop_tree(T::signed_pre_keys())?;
        db.drop_tree(T::kyber_pre_keys())?;
        if clear_sessions {
            db.drop_tree(T::sessions())?;
        }
        Ok(())
    }
}

impl<T: BitpartTrees> ProtocolStore for BitpartProtocolStore<T> {}

#[async_trait(?Send)]
impl<T: BitpartTrees + Send + Sync> PreKeyStore for BitpartProtocolStore<T> {
    async fn get_pre_key(&self, prekey_id: PreKeyId) -> Result<PreKeyRecord, SignalProtocolError> {
        let buf: Vec<u8> = self
            .store
            .get(T::pre_keys(), prekey_id.sled_key())
            .await
            .ok()
            .flatten()
            .ok_or(SignalProtocolError::InvalidPreKeyId)?;

        PreKeyRecord::deserialize(&buf)
    }

    async fn save_pre_key(
        &mut self,
        prekey_id: PreKeyId,
        record: &PreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        self.store
            .insert(T::pre_keys(), prekey_id.sled_key(), record.serialize()?)
            .await
            .map_err(|error| {
                error!(%error, "sled error");
                SignalProtocolError::InvalidState("save_pre_key", "sled error".into())
            })?;
        Ok(())
    }

    async fn remove_pre_key(&mut self, prekey_id: PreKeyId) -> Result<(), SignalProtocolError> {
        self.store
            .remove(T::pre_keys(), prekey_id.sled_key())
            .await
            .map_err(|error| {
                error!(%error, "sled error");
                SignalProtocolError::InvalidState("remove_pre_key", "sled error".into())
            })?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl<T: BitpartTrees + Send + Sync> PreKeysStore for BitpartProtocolStore<T> {
    async fn next_pre_key_id(&self) -> Result<u32, SignalProtocolError> {
        self.next_key_id(T::pre_keys()).await
    }

    async fn next_signed_pre_key_id(&self) -> Result<u32, SignalProtocolError> {
        self.next_key_id(T::signed_pre_keys()).await
    }

    async fn next_pq_pre_key_id(&self) -> Result<u32, SignalProtocolError> {
        self.next_key_id(T::kyber_pre_keys()).await
    }

    async fn signed_pre_keys_count(&self) -> Result<usize, SignalProtocolError> {
        Ok(self
            .store
            .read()
            .await
            .open_tree(T::signed_pre_keys())
            .map_err(|error| {
                error!(%error, "sled error");
                SignalProtocolError::InvalidState("signed_pre_keys_count", "sled error".into())
            })?
            .keys()
            .count())
    }

    /// number of kyber pre-keys we currently have in store
    async fn kyber_pre_keys_count(&self, last_resort: bool) -> Result<usize, SignalProtocolError> {
        Ok(self
            .store
            .read()
            .await
            .open_tree(if last_resort {
                T::kyber_pre_keys_last_resort()
            } else {
                T::kyber_pre_keys()
            })
            .map_err(|error| {
                error!(%error, "sled error");
                SignalProtocolError::InvalidState("save_signed_pre_key", "sled error".into())
            })?
            .keys()
            .count())
    }
}

#[async_trait(?Send)]
impl<T: BitpartTrees> SignedPreKeyStore for BitpartProtocolStore<T> {
    async fn get_signed_pre_key(
        &self,
        signed_prekey_id: SignedPreKeyId,
    ) -> Result<SignedPreKeyRecord, SignalProtocolError> {
        let buf: Vec<u8> = self
            .store
            .get(T::signed_pre_keys(), signed_prekey_id.sled_key())
            .await
            .ok()
            .flatten()
            .ok_or(SignalProtocolError::InvalidSignedPreKeyId)?;
        SignedPreKeyRecord::deserialize(&buf)
    }

    async fn save_signed_pre_key(
        &mut self,
        signed_prekey_id: SignedPreKeyId,
        record: &SignedPreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        self.store
            .insert(
                T::signed_pre_keys(),
                signed_prekey_id.sled_key(),
                record.serialize()?,
            )
            .await
            .map_err(|error| {
                error!(%error, "sled error");
                SignalProtocolError::InvalidState("save_signed_pre_key", "sled error".into())
            })?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl<T: BitpartTrees> KyberPreKeyStore for BitpartProtocolStore<T> {
    async fn get_kyber_pre_key(
        &self,
        kyber_prekey_id: KyberPreKeyId,
    ) -> Result<KyberPreKeyRecord, SignalProtocolError> {
        let buf: Vec<u8> = self
            .store
            .get(T::kyber_pre_keys(), kyber_prekey_id.sled_key())
            .await
            .ok()
            .flatten()
            .ok_or(SignalProtocolError::InvalidKyberPreKeyId)?;
        KyberPreKeyRecord::deserialize(&buf)
    }

    async fn save_kyber_pre_key(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
        record: &KyberPreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        self.store
            .insert(
                T::kyber_pre_keys(),
                kyber_prekey_id.sled_key(),
                record.serialize()?,
            )
            .await
            .map_err(|error| {
                error!(%error, "sled error");
                SignalProtocolError::InvalidState("save_kyber_pre_key", "sled error".into())
            })?;
        Ok(())
    }

    async fn mark_kyber_pre_key_used(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
    ) -> Result<(), SignalProtocolError> {
        let removed = self
            .store
            .remove(T::kyber_pre_keys(), kyber_prekey_id.sled_key())
            .await
            .map_err(|error| {
                error!(%error, "sled error");
                SignalProtocolError::InvalidState("mark_kyber_pre_key_used", "sled error".into())
            })?;
        if removed {
            trace!(%kyber_prekey_id, "removed kyber pre-key");
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl<T: BitpartTrees> KyberPreKeyStoreExt for BitpartProtocolStore<T> {
    async fn store_last_resort_kyber_pre_key(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
        record: &KyberPreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        trace!(%kyber_prekey_id, "store_last_resort_kyber_pre_key");
        self.store
            .insert(
                T::kyber_pre_keys_last_resort(),
                kyber_prekey_id.sled_key(),
                record.serialize()?,
            )
            .await
            .map_err(|error| {
                error!(%error, "sled error");
                SignalProtocolError::InvalidState(
                    "store_last_resort_kyber_pre_key",
                    "sled error".into(),
                )
            })?;
        Ok(())
    }

    async fn load_last_resort_kyber_pre_keys(
        &self,
    ) -> Result<Vec<KyberPreKeyRecord>, SignalProtocolError> {
        trace!("load_last_resort_kyber_pre_keys");
        self.store
            .iter(T::kyber_pre_keys_last_resort())
            .await?
            .filter_map(|data: Result<Vec<u8>, BitpartStoreError>| data.ok())
            .map(|data| KyberPreKeyRecord::deserialize(&data))
            .collect()
    }

    async fn remove_kyber_pre_key(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
    ) -> Result<(), SignalProtocolError> {
        self.store
            .remove(T::kyber_pre_keys_last_resort(), kyber_prekey_id.sled_key())
            .await?;
        self.store
            .remove(T::kyber_pre_keys_last_resort(), kyber_prekey_id.sled_key())
            .await?;
        Ok(())
    }

    /// Analogous to markAllOneTimeKyberPreKeysStaleIfNecessary
    async fn mark_all_one_time_kyber_pre_keys_stale_if_necessary(
        &mut self,
        _stale_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), SignalProtocolError> {
        unimplemented!("should not be used yet")
    }

    /// Analogue of deleteAllStaleOneTimeKyberPreKeys
    async fn delete_all_stale_one_time_kyber_pre_keys(
        &mut self,
        _threshold: chrono::DateTime<chrono::Utc>,
        _min_count: usize,
    ) -> Result<(), SignalProtocolError> {
        unimplemented!("should not be used yet")
    }
}

#[async_trait(?Send)]
impl<T: BitpartTrees> SessionStore for BitpartProtocolStore<T> {
    async fn load_session(
        &self,
        address: &ProtocolAddress,
    ) -> Result<Option<SessionRecord>, SignalProtocolError> {
        let session = self.store.get(T::sessions(), address.to_string()).await?;
        trace!(
            %address,
            session_exists = session.is_some(),
            "loading session",
        );
        session
            .map(|b: Vec<u8>| SessionRecord::deserialize(&b))
            .transpose()
    }

    async fn store_session(
        &mut self,
        address: &ProtocolAddress,
        record: &SessionRecord,
    ) -> Result<(), SignalProtocolError> {
        trace!(%address, "storing session");
        self.store
            .insert(T::sessions(), address.to_string(), record.serialize()?)
            .await?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl<T: BitpartTrees> SessionStoreExt for BitpartProtocolStore<T> {
    async fn get_sub_device_sessions(
        &self,
        address: &ServiceId,
    ) -> Result<Vec<u32>, SignalProtocolError> {
        let session_prefix = format!("{}.", address.raw_uuid());
        trace!(session_prefix, "get_sub_device_sessions");
        let session_ids: Vec<u32> = self
            .store
            .read()
            .await
            .open_tree(T::sessions())?
            .iter()
            .filter_map(|(key, _)| {
                let key_str = String::from_utf8_lossy(&key);
                if !key_str.starts_with(&session_prefix) {
                    return None;
                };
                let device_id = key_str.strip_prefix(&session_prefix)?;
                device_id.parse().ok()
            })
            .filter(|d| *d != DEFAULT_DEVICE_ID)
            .collect();
        Ok(session_ids)
    }

    async fn delete_session(&self, address: &ProtocolAddress) -> Result<(), SignalProtocolError> {
        trace!(%address, "deleting session");
        self.store
            .write()
            .await
            .open_tree(T::sessions())?
            .remove(&Vec::from(address.to_string()))
            .ok_or_else(|| SignalProtocolError::SessionNotFound(address.clone()))?;
        Ok(())
    }

    async fn delete_all_sessions(&self, address: &ServiceId) -> Result<usize, SignalProtocolError> {
        let mut db = self.store.write().await;
        let sessions_tree = db.open_tree(T::sessions())?;

        sessions_tree.retain(|key, _| {
            let key_str = String::from_utf8_lossy(&key);
            !key_str.starts_with(&address.raw_uuid().to_string())
        });

        let len = sessions_tree.len();
        sessions_tree.clear();
        Ok(len)
    }
}

#[async_trait(?Send)]
impl<T: BitpartTrees> IdentityKeyStore for BitpartProtocolStore<T> {
    async fn get_identity_key_pair(&self) -> Result<IdentityKeyPair, SignalProtocolError> {
        trace!("getting identity_key_pair");
        self.store
            .get_identity_key_pair::<T>()
            .await?
            .ok_or_else(|| {
                SignalProtocolError::InvalidState(
                    "get_identity_key_pair",
                    "no identity key pair found".to_owned(),
                )
            })
    }

    async fn get_local_registration_id(&self) -> Result<u32, SignalProtocolError> {
        let data =
            self.store
                .load_registration_data()
                .await?
                .ok_or(SignalProtocolError::InvalidState(
                    "failed to load registration ID",
                    "no registration data".into(),
                ))?;
        Ok(data.registration_id)
    }

    async fn save_identity(
        &mut self,
        address: &ProtocolAddress,
        identity_key: &IdentityKey,
    ) -> Result<bool, SignalProtocolError> {
        trace!("saving identity");
        let existed_before = self
            .store
            .insert(
                T::identities(),
                address.to_string(),
                identity_key.serialize(),
            )
            .await
            .map_err(|error| {
                error!(%error, %address, "failed to save identity");
                error
            })?;

        save_trusted_identity_message(
            &self.store,
            address,
            *identity_key,
            if existed_before {
                verified::State::Unverified
            } else {
                verified::State::Default
            },
        )
        .await?;

        Ok(true)
    }

    async fn is_trusted_identity(
        &self,
        address: &ProtocolAddress,
        right_identity_key: &IdentityKey,
        _direction: Direction,
    ) -> Result<bool, SignalProtocolError> {
        match self
            .store
            .get(T::identities(), address.to_string())
            .await?
            .map(|b: Vec<u8>| IdentityKey::decode(&b))
            .transpose()?
        {
            None => {
                // when we encounter a new identity, we trust it by default
                warn!(%address, "trusting new identity");
                Ok(true)
            }
            // when we encounter some identity we know, we need to decide whether we trust it or not
            Some(left_identity_key) => {
                if left_identity_key == *right_identity_key {
                    Ok(true)
                } else {
                    match self.store.trust_new_identities {
                        OnNewIdentity::Trust => Ok(true),
                        OnNewIdentity::Reject => Ok(false),
                    }
                }
            }
        }
    }

    async fn get_identity(
        &self,
        address: &ProtocolAddress,
    ) -> Result<Option<IdentityKey>, SignalProtocolError> {
        self.store
            .get(T::identities(), address.to_string())
            .await?
            .map(|b: Vec<u8>| IdentityKey::decode(&b))
            .transpose()
    }
}

#[async_trait(?Send)]
impl<T: BitpartTrees> SenderKeyStore for BitpartProtocolStore<T> {
    async fn store_sender_key(
        &mut self,
        sender: &ProtocolAddress,
        distribution_id: Uuid,
        record: &SenderKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        let key = format!(
            "{}.{}/{}",
            sender.name(),
            sender.device_id(),
            distribution_id
        );
        self.store
            .insert(T::sender_keys(), key, record.serialize()?)
            .await?;
        Ok(())
    }

    async fn load_sender_key(
        &mut self,
        sender: &ProtocolAddress,
        distribution_id: Uuid,
    ) -> Result<Option<SenderKeyRecord>, SignalProtocolError> {
        let key = format!(
            "{}.{}/{}",
            sender.name(),
            sender.device_id(),
            distribution_id
        );
        self.store
            .get(T::sender_keys(), key)
            .await?
            .map(|b: Vec<u8>| SenderKeyRecord::deserialize(&b))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use core::fmt;

    use base64::prelude::*;
    use presage::{
        libsignal_service::{
            pre_keys::PreKeysStore,
            protocol::{
                self, Direction, GenericSignedPreKey, IdentityKeyStore, PreKeyId, PreKeyRecord,
                PreKeyStore, SessionRecord, SessionStore, SignedPreKeyId, SignedPreKeyRecord,
                SignedPreKeyStore, Timestamp,
            },
        },
        store::Store,
    };
    use quickcheck::{Arbitrary, Gen, TestResult};

    use super::BitpartStore;

    #[derive(Debug, Clone)]
    struct ProtocolAddress(protocol::ProtocolAddress);

    #[derive(Clone)]
    struct KeyPair(protocol::KeyPair);

    impl fmt::Debug for KeyPair {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            writeln!(
                f,
                "{}",
                BASE64_STANDARD.encode(self.0.public_key.serialize())
            )
        }
    }

    impl Arbitrary for ProtocolAddress {
        fn arbitrary(g: &mut Gen) -> Self {
            let name: String = Arbitrary::arbitrary(g);
            let device_id: u32 = Arbitrary::arbitrary(g);
            ProtocolAddress(protocol::ProtocolAddress::new(name, device_id.into()))
        }
    }

    impl Arbitrary for KeyPair {
        fn arbitrary(_g: &mut Gen) -> Self {
            // Gen is not rand::CryptoRng here, see https://github.com/BurntSushi/quickcheck/issues/241
            KeyPair(protocol::KeyPair::generate(&mut rand::thread_rng()))
        }
    }

    #[quickcheck_async::tokio]
    async fn test_save_get_trust_identity(addr: ProtocolAddress, key_pair: KeyPair) -> bool {
        let mut db = BitpartStore::temporary()
            .await
            .unwrap()
            .aci_protocol_store();
        let identity_key = protocol::IdentityKey::new(key_pair.0.public_key);
        db.save_identity(&addr.0, &identity_key).await.unwrap();
        let id = db.get_identity(&addr.0).await.unwrap().unwrap();
        if id != identity_key {
            return false;
        }
        db.is_trusted_identity(&addr.0, &id, Direction::Receiving)
            .await
            .unwrap()
    }

    #[quickcheck_async::tokio]
    async fn test_store_load_session(addr: ProtocolAddress) -> bool {
        let session = SessionRecord::new_fresh();

        let mut db = BitpartStore::temporary()
            .await
            .unwrap()
            .aci_protocol_store();
        db.store_session(&addr.0, &session).await.unwrap();
        if db.load_session(&addr.0).await.unwrap().is_none() {
            return false;
        }
        let loaded_session = db.load_session(&addr.0).await.unwrap().unwrap();
        session.serialize().unwrap() == loaded_session.serialize().unwrap()
    }

    #[quickcheck_async::tokio]
    async fn test_prekey_store(id: u32, key_pair: KeyPair) -> bool {
        let id = id.into();
        let mut db = BitpartStore::temporary()
            .await
            .unwrap()
            .aci_protocol_store();
        let pre_key_record = PreKeyRecord::new(id, &key_pair.0);
        db.save_pre_key(id, &pre_key_record).await.unwrap();
        if db.get_pre_key(id).await.unwrap().serialize().unwrap()
            != pre_key_record.serialize().unwrap()
        {
            return false;
        }

        db.remove_pre_key(id).await.unwrap();
        db.get_pre_key(id).await.is_err()
    }

    #[quickcheck_async::tokio]
    async fn test_signed_prekey_store(
        id: u32,
        timestamp: u64,
        key_pair: KeyPair,
        signature: Vec<u8>,
    ) -> bool {
        let mut db = BitpartStore::temporary()
            .await
            .unwrap()
            .aci_protocol_store();
        let id = id.into();
        let signed_pre_key_record = SignedPreKeyRecord::new(
            id,
            Timestamp::from_epoch_millis(timestamp),
            &key_pair.0,
            &signature,
        );
        db.save_signed_pre_key(id, &signed_pre_key_record)
            .await
            .unwrap();

        db.get_signed_pre_key(id)
            .await
            .unwrap()
            .serialize()
            .unwrap()
            == signed_pre_key_record.serialize().unwrap()
    }

    #[derive(Debug, Clone)]
    struct ArbPreKeyRecord(protocol::PreKeyRecord);

    impl Arbitrary for ArbPreKeyRecord {
        fn arbitrary(g: &mut Gen) -> Self {
            let id = u32::arbitrary(g);
            let key_pair = KeyPair::arbitrary(g);
            Self(protocol::PreKeyRecord::new(id.into(), &key_pair.0))
        }
    }

    #[derive(Debug, Clone)]
    struct ArbSignedPreKeyRecord(protocol::SignedPreKeyRecord);

    impl Arbitrary for ArbSignedPreKeyRecord {
        fn arbitrary(g: &mut Gen) -> Self {
            let id = u32::arbitrary(g);
            let timestamp = Arbitrary::arbitrary(g);
            let key_pair = KeyPair::arbitrary(g);
            let signature: Vec<u8> = Arbitrary::arbitrary(g);
            Self(protocol::SignedPreKeyRecord::new(
                id.into(),
                protocol::Timestamp::from_epoch_millis(timestamp),
                &key_pair.0,
                &signature,
            ))
        }
    }

    #[quickcheck_async::tokio]
    async fn get_next_pre_key_ids(
        key1: ArbPreKeyRecord,
        key2: ArbPreKeyRecord,
        signed_key: ArbSignedPreKeyRecord,
    ) {
        let db = BitpartStore::temporary().await.unwrap();
        let mut store = db.aci_protocol_store();

        assert_eq!(store.next_pre_key_id().await.unwrap(), 0);
        assert_eq!(store.next_pq_pre_key_id().await.unwrap(), 0);
        assert_eq!(store.next_signed_pre_key_id().await.unwrap(), 0);

        store
            .save_pre_key(PreKeyId::from(0), &key1.0)
            .await
            .unwrap();
        store
            .save_pre_key(PreKeyId::from(1), &key2.0)
            .await
            .unwrap();
        store
            .save_signed_pre_key(SignedPreKeyId::from(0), &signed_key.0)
            .await
            .unwrap();

        assert_eq!(store.next_pre_key_id().await.unwrap(), 2);
        assert_eq!(store.next_pq_pre_key_id().await.unwrap(), 0);
        assert_eq!(store.next_signed_pre_key_id().await.unwrap(), 1);
    }

    #[quickcheck_async::tokio]
    async fn test_next_key_id_is_max(keys: Vec<u32>, record: ArbPreKeyRecord) -> TestResult {
        let db = BitpartStore::temporary().await.unwrap();
        let mut store = db.aci_protocol_store();

        for &key in &keys {
            store.save_pre_key(key.into(), &record.0).await.unwrap();
            if key == u32::MAX {
                return TestResult::discard();
            }
        }
        if keys.iter().copied().max().map(|id| id + 1).unwrap_or(0)
            != store.next_pre_key_id().await.unwrap()
        {
            return TestResult::failed();
        }
        TestResult::passed()
    }
}
