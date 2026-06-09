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

use async_trait::async_trait;
use base64::Engine;
use presage::{
    libsignal_service::{
        pre_keys::{KyberPreKeyStoreExt, PreKeysStore},
        prelude::Uuid,
        protocol::{
            DeviceId, Direction, GenericSignedPreKey, IdentityChange, IdentityKey, IdentityKeyPair,
            IdentityKeyStore, KyberPreKeyId, KyberPreKeyRecord, KyberPreKeyStore, PreKeyId,
            PreKeyRecord, PreKeyStore, ProtocolAddress, ProtocolStore, SenderKeyRecord,
            SenderKeyStore, ServiceId, SessionRecord, SessionStore, SignalProtocolError,
            SignedPreKeyId, SignedPreKeyRecord, SignedPreKeyStore,
        },
        push_service::DEFAULT_DEVICE_ID,
        session_store::SessionStoreExt,
    },
    proto::verified,
    store::{StateStore, save_trusted_identity_message},
};
use tracing::{debug, error, trace, warn};

use crate::{BitpartStore, BitpartStoreError, OnNewIdentity, db};

#[derive(Clone)]
pub struct BitpartProtocolStore {
    pub(crate) store: BitpartStore,
    is_pni: bool,
}

impl BitpartProtocolStore {
    pub(crate) fn aci_protocol_store(store: BitpartStore) -> Self {
        Self {
            store,
            is_pni: false,
        }
    }

    pub(crate) fn pni_protocol_store(store: BitpartStore) -> Self {
        Self {
            store,
            is_pni: true,
        }
    }

    pub(crate) async fn clear(&self, clear_sessions: bool) -> Result<(), BitpartStoreError> {
        if self.is_pni {
            db::pre_keys::remove_all_pni(&self.store.id, &self.store.pool).await?;
            db::signed_pre_keys::remove_all_pni(&self.store.id, &self.store.pool).await?;
            db::kyber_pre_keys::remove_all_pni(&self.store.id, &self.store.pool).await?;
            db::sender_keys::remove_all_pni(&self.store.id, &self.store.pool).await?;
            if clear_sessions {
                db::sessions::remove_all_pni(&self.store.id, &self.store.pool).await?;
            }
        } else {
            db::pre_keys::remove_all_aci(&self.store.id, &self.store.pool).await?;
            db::signed_pre_keys::remove_all_aci(&self.store.id, &self.store.pool).await?;
            db::kyber_pre_keys::remove_all_aci(&self.store.id, &self.store.pool).await?;
            db::sender_keys::remove_all_aci(&self.store.id, &self.store.pool).await?;
            if clear_sessions {
                db::sessions::remove_all_aci(&self.store.id, &self.store.pool).await?;
            }
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl PreKeyStore for BitpartProtocolStore {
    async fn get_pre_key(&self, prekey_id: PreKeyId) -> Result<PreKeyRecord, SignalProtocolError> {
        let buf = if self.is_pni {
            db::pre_keys::get_pni(&self.store.id, prekey_id.into(), &self.store.pool).await
        } else {
            db::pre_keys::get_aci(&self.store.id, prekey_id.into(), &self.store.pool).await
        }?
        .ok_or(SignalProtocolError::InvalidPreKeyId)?;

        PreKeyRecord::deserialize(&buf)
    }

    async fn save_pre_key(
        &mut self,
        prekey_id: PreKeyId,
        record: &PreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        let record_data = record.serialize()?;
        if self.is_pni {
            db::pre_keys::set_pni(
                &self.store.id,
                prekey_id.into(),
                &record_data,
                &self.store.pool,
            )
            .await
        } else {
            db::pre_keys::set_aci(
                &self.store.id,
                prekey_id.into(),
                &record_data,
                &self.store.pool,
            )
            .await
        }
        .map_err(|error| {
            error!(%error, "store error");
            SignalProtocolError::InvalidState("save_pre_key", "store error".into())
        })?;
        Ok(())
    }

    async fn remove_pre_key(&mut self, prekey_id: PreKeyId) -> Result<(), SignalProtocolError> {
        let removed = if self.is_pni {
            db::pre_keys::remove_pni(&self.store.id, prekey_id.into(), &self.store.pool).await
        } else {
            db::pre_keys::remove_aci(&self.store.id, prekey_id.into(), &self.store.pool).await
        }
        .map_err(|error| {
            error!(%error, "store error");
            SignalProtocolError::InvalidState("remove_pre_key", "store error".into())
        })?;

        if removed.is_none() {
            return Err(SignalProtocolError::InvalidState(
                "remove_pre_key",
                "key not found".into(),
            ));
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl SignedPreKeyStore for BitpartProtocolStore {
    async fn get_signed_pre_key(
        &self,
        signed_prekey_id: SignedPreKeyId,
    ) -> Result<SignedPreKeyRecord, SignalProtocolError> {
        let buf = if self.is_pni {
            db::signed_pre_keys::get_pni(&self.store.id, signed_prekey_id.into(), &self.store.pool)
                .await
        } else {
            db::signed_pre_keys::get_aci(&self.store.id, signed_prekey_id.into(), &self.store.pool)
                .await
        }?
        .ok_or(SignalProtocolError::InvalidSignedPreKeyId)?;

        SignedPreKeyRecord::deserialize(&buf)
    }

    async fn save_signed_pre_key(
        &mut self,
        signed_prekey_id: SignedPreKeyId,
        record: &SignedPreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        let record_data = record.serialize()?;
        if self.is_pni {
            db::signed_pre_keys::set_pni(
                &self.store.id,
                signed_prekey_id.into(),
                &record_data,
                &self.store.pool,
            )
            .await
        } else {
            db::signed_pre_keys::set_aci(
                &self.store.id,
                signed_prekey_id.into(),
                &record_data,
                &self.store.pool,
            )
            .await
        }
        .map_err(|error| {
            error!(%error, "store error");
            SignalProtocolError::InvalidState("save_signed_pre_key", "store error".into())
        })?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl KyberPreKeyStore for BitpartProtocolStore {
    async fn get_kyber_pre_key(
        &self,
        kyber_prekey_id: KyberPreKeyId,
    ) -> Result<KyberPreKeyRecord, SignalProtocolError> {
        debug!("get_kyber_pre_key");

        if let Some(buf) = if self.is_pni {
            db::kyber_pre_keys::get_pni(&self.store.id, kyber_prekey_id.into(), &self.store.pool)
                .await
        } else {
            db::kyber_pre_keys::get_aci(&self.store.id, kyber_prekey_id.into(), &self.store.pool)
                .await
        }? {
            return KyberPreKeyRecord::deserialize(&buf);
        }

        Err(SignalProtocolError::InvalidKyberPreKeyId)
    }

    async fn save_kyber_pre_key(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
        record: &KyberPreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        debug!("save_kyber_pre_key");
        let record_data = record.serialize()?;
        if self.is_pni {
            db::kyber_pre_keys::set_pni(
                &self.store.id,
                kyber_prekey_id.into(),
                &record_data,
                false,
                &self.store.pool,
            )
            .await
        } else {
            db::kyber_pre_keys::set_aci(
                &self.store.id,
                kyber_prekey_id.into(),
                &record_data,
                false,
                &self.store.pool,
            )
            .await
        }
        .map_err(|error| {
            error!(%error, "store error");
            SignalProtocolError::InvalidState("save_kyber_pre_key", "store error".into())
        })?;
        Ok(())
    }

    async fn mark_kyber_pre_key_used(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
        ec_prekey_id: SignedPreKeyId,
        base_key: &presage::libsignal_service::protocol::PublicKey,
    ) -> Result<(), SignalProtocolError> {
        debug!("mark_kyber_pre_key_used");

        // Check if this is a last resort key by looking it up with is_last_resort filter
        let last_resort_keys = if self.is_pni {
            db::kyber_pre_keys::get_last_resort_pni(&self.store.id, &self.store.pool).await
        } else {
            db::kyber_pre_keys::get_last_resort_aci(&self.store.id, &self.store.pool).await
        }?;

        let is_last_resort = last_resort_keys.iter().any(|(key_id, _)| {
            let kyber_id: u32 = kyber_prekey_id.into();
            *key_id == kyber_id
        });

        if is_last_resort {
            trace!(%kyber_prekey_id, "removed kyber pre-key");
            let base_key_bytes: [u8; 32] =
                base_key.public_key_bytes().try_into().map_err(|_| {
                    SignalProtocolError::InvalidState(
                        "mark_kyber_pre_key_used",
                        "invalid base key size".into(),
                    )
                })?;

            db::base_keys_seen::set(
                &self.store.id,
                self.is_pni,
                kyber_prekey_id.into(),
                ec_prekey_id.into(),
                &base_key_bytes,
                &self.store.pool,
            )
            .await?;
        } else {
            let _removed = if self.is_pni {
                db::kyber_pre_keys::remove_pni(
                    &self.store.id,
                    kyber_prekey_id.into(),
                    &self.store.pool,
                )
                .await
            } else {
                db::kyber_pre_keys::remove_aci(
                    &self.store.id,
                    kyber_prekey_id.into(),
                    &self.store.pool,
                )
                .await
            }
            .map_err(|error| {
                error!(%error, "store error");
                SignalProtocolError::InvalidState("mark_kyber_pre_key_used", "store error".into())
            })?;
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl SessionStore for BitpartProtocolStore {
    async fn load_session(
        &self,
        address: &ProtocolAddress,
    ) -> Result<Option<SessionRecord>, SignalProtocolError> {
        let session_data = if self.is_pni {
            db::sessions::get_pni(&self.store.id, &address.to_string(), &self.store.pool).await
        } else {
            db::sessions::get_aci(&self.store.id, &address.to_string(), &self.store.pool).await
        }?;

        trace!(
            %address,
            session_exists = session_data.is_some(),
            "loading session",
        );

        session_data
            .map(|b| SessionRecord::deserialize(&b))
            .transpose()
    }

    async fn store_session(
        &mut self,
        address: &ProtocolAddress,
        record: &SessionRecord,
    ) -> Result<(), SignalProtocolError> {
        trace!(%address, "storing session");
        let session_data = record.serialize()?;
        if self.is_pni {
            db::sessions::set_pni(
                &self.store.id,
                &address.to_string(),
                &session_data,
                &self.store.pool,
            )
            .await
        } else {
            db::sessions::set_aci(
                &self.store.id,
                &address.to_string(),
                &session_data,
                &self.store.pool,
            )
            .await
        }?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl IdentityKeyStore for BitpartProtocolStore {
    async fn get_identity_key_pair(&self) -> Result<IdentityKeyPair, SignalProtocolError> {
        trace!("getting identity_key_pair");
        let key_data = if self.is_pni {
            db::state::get_pni(&self.store.id, "pni_identity_key_pair", &self.store.pool).await
        } else {
            db::state::get_aci(&self.store.id, "aci_identity_key_pair", &self.store.pool).await
        }?
        .ok_or_else(|| {
            SignalProtocolError::InvalidState(
                "get_identity_key_pair",
                "no identity key pair found".to_owned(),
            )
        })?;

        let key_base64 = String::from_utf8(key_data).map_err(|e| {
            SignalProtocolError::InvalidState("get_identity_key_pair", e.to_string())
        })?;
        let key_bytes = base64::prelude::BASE64_STANDARD
            .decode(key_base64)
            .map_err(|e| {
                SignalProtocolError::InvalidState("get_identity_key_pair", e.to_string())
            })?;
        IdentityKeyPair::try_from(&*key_bytes)
            .map_err(|e| SignalProtocolError::InvalidState("get_identity_key_pair", e.to_string()))
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
    ) -> Result<IdentityChange, SignalProtocolError> {
        trace!("saving identity");

        let existed_before = db::identities::get(
            &self.store.id,
            self.is_pni,
            &address.to_string(),
            &self.store.pool,
        )
        .await
        .map_err(|error| {
            error!(%error, %address, "failed to check existing identity");
            error
        })?
        .is_some();

        db::identities::set(
            &self.store.id,
            self.is_pni,
            &address.to_string(),
            &identity_key.serialize(),
            &self.store.pool,
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

        Ok(if existed_before {
            IdentityChange::ReplacedExisting
        } else {
            IdentityChange::NewOrUnchanged
        })
    }

    async fn is_trusted_identity(
        &self,
        address: &ProtocolAddress,
        right_identity_key: &IdentityKey,
        _direction: Direction,
    ) -> Result<bool, SignalProtocolError> {
        match db::identities::get(
            &self.store.id,
            self.is_pni,
            &address.to_string(),
            &self.store.pool,
        )
        .await?
        .map(|b| IdentityKey::decode(&b))
        .transpose()?
        {
            None => {
                warn!(%address, "trusting new identity");
                Ok(true)
            }
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
        db::identities::get(
            &self.store.id,
            self.is_pni,
            &address.to_string(),
            &self.store.pool,
        )
        .await?
        .map(|b| IdentityKey::decode(&b))
        .transpose()
    }
}

#[async_trait(?Send)]
impl SenderKeyStore for BitpartProtocolStore {
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
        let record_data = record.serialize()?;
        if self.is_pni {
            db::sender_keys::set_pni(&self.store.id, &key, &record_data, &self.store.pool).await
        } else {
            db::sender_keys::set_aci(&self.store.id, &key, &record_data, &self.store.pool).await
        }?;
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
        let record_data = if self.is_pni {
            db::sender_keys::get_pni(&self.store.id, &key, &self.store.pool).await
        } else {
            db::sender_keys::get_aci(&self.store.id, &key, &self.store.pool).await
        }?;

        record_data
            .map(|b| SenderKeyRecord::deserialize(&b))
            .transpose()
    }
}

impl ProtocolStore for BitpartProtocolStore {}

#[async_trait(?Send)]
impl PreKeysStore for BitpartProtocolStore {
    async fn next_pre_key_id(&self) -> Result<u32, SignalProtocolError> {
        let max_id = if self.is_pni {
            db::pre_keys::max_key_id_pni(&self.store.id, &self.store.pool).await
        } else {
            db::pre_keys::max_key_id_aci(&self.store.id, &self.store.pool).await
        }?;
        Ok(max_id.map_or(0, |id| id + 1))
    }

    async fn next_signed_pre_key_id(&self) -> Result<u32, SignalProtocolError> {
        let max_id = if self.is_pni {
            db::signed_pre_keys::max_key_id_pni(&self.store.id, &self.store.pool).await
        } else {
            db::signed_pre_keys::max_key_id_aci(&self.store.id, &self.store.pool).await
        }?;
        Ok(max_id.map_or(0, |id| id + 1))
    }

    async fn next_pq_pre_key_id(&self) -> Result<u32, SignalProtocolError> {
        let max_id = if self.is_pni {
            db::kyber_pre_keys::max_key_id_pni(&self.store.id, &self.store.pool).await
        } else {
            db::kyber_pre_keys::max_key_id_aci(&self.store.id, &self.store.pool).await
        }?;
        Ok(max_id.map_or(0, |id| id + 1))
    }

    async fn signed_pre_keys_count(&self) -> Result<usize, SignalProtocolError> {
        let all_keys = if self.is_pni {
            db::signed_pre_keys::get_all_pni(&self.store.id, &self.store.pool).await
        } else {
            db::signed_pre_keys::get_all_aci(&self.store.id, &self.store.pool).await
        }
        .map_err(|error| {
            error!(%error, "store error");
            SignalProtocolError::InvalidState("signed_pre_keys_count", "store error".into())
        })?;
        Ok(all_keys.len())
    }

    async fn kyber_pre_keys_count(&self, _last_resort: bool) -> Result<usize, SignalProtocolError> {
        debug!("kyber_pre_keys_count");
        let all_keys = if self.is_pni {
            db::kyber_pre_keys::get_all_pni(&self.store.id, &self.store.pool).await
        } else {
            db::kyber_pre_keys::get_all_aci(&self.store.id, &self.store.pool).await
        }
        .map_err(|error| {
            error!(%error, "store error");
            SignalProtocolError::InvalidState("kyber_pre_keys_count", "store error".into())
        })?;
        Ok(all_keys.len())
    }

    async fn signed_prekey_id(&self) -> Result<Option<SignedPreKeyId>, SignalProtocolError> {
        let max_id = if self.is_pni {
            db::signed_pre_keys::max_key_id_pni(&self.store.id, &self.store.pool).await
        } else {
            db::signed_pre_keys::max_key_id_aci(&self.store.id, &self.store.pool).await
        }?;
        Ok(max_id.map(From::from))
    }

    async fn last_resort_kyber_prekey_id(
        &self,
    ) -> Result<Option<KyberPreKeyId>, SignalProtocolError> {
        debug!("last_resort_kyber_prekey_id");
        let all_keys = if self.is_pni {
            db::kyber_pre_keys::get_last_resort_pni(&self.store.id, &self.store.pool).await
        } else {
            db::kyber_pre_keys::get_last_resort_aci(&self.store.id, &self.store.pool).await
        }?;

        let mut keys: Vec<u32> = all_keys.iter().map(|(key_id, _)| *key_id).collect();
        keys.sort();
        Ok(keys.last().copied().map(From::from))
    }
}

#[async_trait(?Send)]
impl KyberPreKeyStoreExt for BitpartProtocolStore {
    async fn store_last_resort_kyber_pre_key(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
        record: &KyberPreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        debug!("store_last_resort_kyber_pre_key");
        let record_data = record.serialize()?;
        if self.is_pni {
            db::kyber_pre_keys::set_pni(
                &self.store.id,
                kyber_prekey_id.into(),
                &record_data,
                true,
                &self.store.pool,
            )
            .await
        } else {
            db::kyber_pre_keys::set_aci(
                &self.store.id,
                kyber_prekey_id.into(),
                &record_data,
                true,
                &self.store.pool,
            )
            .await
        }
        .map_err(|error| {
            error!(%error, "store error");
            SignalProtocolError::InvalidState(
                "store_last_resort_kyber_pre_key",
                "store error".into(),
            )
        })?;
        Ok(())
    }

    async fn load_last_resort_kyber_pre_keys(
        &self,
    ) -> Result<Vec<KyberPreKeyRecord>, SignalProtocolError> {
        debug!("load_last_resort_kyber_pre_keys");
        let last_resort_keys = if self.is_pni {
            db::kyber_pre_keys::get_last_resort_pni(&self.store.id, &self.store.pool).await
        } else {
            db::kyber_pre_keys::get_last_resort_aci(&self.store.id, &self.store.pool).await
        }?;

        let mut records = Vec::new();
        for (_, record_data) in last_resort_keys {
            let record = KyberPreKeyRecord::deserialize(&record_data)?;
            records.push(record);
        }
        Ok(records)
    }

    async fn remove_kyber_pre_key(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
    ) -> Result<(), SignalProtocolError> {
        let _removed = if self.is_pni {
            db::kyber_pre_keys::remove_pni(&self.store.id, kyber_prekey_id.into(), &self.store.pool)
                .await
        } else {
            db::kyber_pre_keys::remove_aci(&self.store.id, kyber_prekey_id.into(), &self.store.pool)
                .await
        }?;
        Ok(())
    }

    async fn mark_all_one_time_kyber_pre_keys_stale_if_necessary(
        &mut self,
        _stale_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), SignalProtocolError> {
        unimplemented!("should not be used yet")
    }

    async fn delete_all_stale_one_time_kyber_pre_keys(
        &mut self,
        _threshold: chrono::DateTime<chrono::Utc>,
        _min_count: usize,
    ) -> Result<(), SignalProtocolError> {
        unimplemented!("should not be used yet")
    }
}

#[async_trait(?Send)]
impl SessionStoreExt for BitpartProtocolStore {
    async fn get_sub_device_sessions(
        &self,
        address: &ServiceId,
    ) -> Result<Vec<DeviceId>, SignalProtocolError> {
        let session_prefix = format!("{}.", address.raw_uuid());
        trace!(session_prefix, "get_sub_device_sessions");
        let device_id: u32 = (*DEFAULT_DEVICE_ID).into();

        let all_sessions = if self.is_pni {
            db::sessions::get_all_pni(&self.store.id, &self.store.pool).await
        } else {
            db::sessions::get_all_aci(&self.store.id, &self.store.pool).await
        }?;

        let session_ids: Vec<DeviceId> = all_sessions
            .iter()
            .filter_map(|(address_str, _session_data): &(String, Vec<u8>)| {
                if !address_str.starts_with(&session_prefix) {
                    return None;
                };
                if let Ok(did) = address_str.strip_prefix(&session_prefix)?.parse::<u32>()
                    && did != device_id
                {
                    return did.try_into().ok();
                }
                None
            })
            .collect();
        Ok(session_ids)
    }

    async fn delete_session(&self, address: &ProtocolAddress) -> Result<(), SignalProtocolError> {
        trace!(%address, "deleting session");
        let _removed = if self.is_pni {
            db::sessions::remove_pni(&self.store.id, &address.to_string(), &self.store.pool).await
        } else {
            db::sessions::remove_aci(&self.store.id, &address.to_string(), &self.store.pool).await
        }?;
        Ok(())
    }

    async fn delete_all_sessions(&self, address: &ServiceId) -> Result<usize, SignalProtocolError> {
        let pattern = format!("{}%", address.raw_uuid());
        let removed = if self.is_pni {
            db::sessions::remove_like_pni(&self.store.id, &pattern, &self.store.pool).await
        } else {
            db::sessions::remove_like_aci(&self.store.id, &pattern, &self.store.pool).await
        }?;
        Ok(removed as usize)
    }
}

#[cfg(test)]
mod tests {
    use core::fmt;

    use base64::prelude::*;
    use presage::{
        libsignal_service::{
            pre_keys::{KyberPreKeyStoreExt, PreKeysStore},
            protocol::{
                self, Direction, GenericSignedPreKey, IdentityKeyStore, KyberPreKeyStore, PreKeyId,
                PreKeyRecord, PreKeyStore, SessionRecord, SessionStore, SignedPreKeyId,
                SignedPreKeyRecord, SignedPreKeyStore, Timestamp, kem,
            },
        },
        store::Store,
    };
    use quickcheck::{Arbitrary, Gen, TestResult};

    use super::BitpartStore;
    use rand::prelude::*;

    #[derive(Debug, Clone)]
    struct ProtocolAddress(protocol::ProtocolAddress);

    #[derive(Clone)]
    struct KeyPair(protocol::KeyPair);

    #[derive(Clone, Debug)]
    struct KyberPreKeyId(protocol::KyberPreKeyId);

    #[derive(Clone, Debug)]
    struct KyberPreKeyRecord(protocol::KyberPreKeyRecord);

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
            let device_id: u8 = rand::rng().random_range(1..127);
            ProtocolAddress(protocol::ProtocolAddress::new(
                name,
                device_id.try_into().unwrap(),
            ))
        }
    }

    impl Arbitrary for KeyPair {
        fn arbitrary(_g: &mut Gen) -> Self {
            KeyPair(protocol::KeyPair::generate(&mut rand::rng()))
        }
    }

    impl Arbitrary for KyberPreKeyId {
        fn arbitrary(_g: &mut Gen) -> Self {
            let id: u32 = rand::rng().random();
            KyberPreKeyId(id.into())
        }
    }

    impl Arbitrary for KyberPreKeyRecord {
        fn arbitrary(g: &mut Gen) -> Self {
            let random: [u8; 32] = rand::rng().random();
            let pkey = protocol::PrivateKey::deserialize(&random).unwrap();
            KyberPreKeyRecord(
                protocol::KyberPreKeyRecord::generate(
                    kem::KeyType::Kyber1024,
                    KyberPreKeyId::arbitrary(g).0,
                    &pkey,
                )
                .unwrap(),
            )
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
    async fn test_save_get_kyber_prekey(id: KyberPreKeyId, record: KyberPreKeyRecord) -> bool {
        let mut db = BitpartStore::temporary()
            .await
            .unwrap()
            .aci_protocol_store();
        db.save_kyber_pre_key(id.0, &record.0).await.unwrap();
        let key = db.get_kyber_pre_key(id.0).await.unwrap();
        key.public_key().unwrap() == record.0.public_key().unwrap()
    }

    #[quickcheck_async::tokio]
    async fn test_save_get_last_resort_kyber_prekey(
        id: KyberPreKeyId,
        record: KyberPreKeyRecord,
    ) -> bool {
        let mut db = BitpartStore::temporary()
            .await
            .unwrap()
            .aci_protocol_store();
        db.store_last_resort_kyber_pre_key(id.0, &record.0)
            .await
            .unwrap();
        let key = db.get_kyber_pre_key(id.0).await.unwrap();
        key.public_key().unwrap() == record.0.public_key().unwrap()
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
