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

use std::ops::{Bound, RangeBounds};

use presage::{
    AvatarBytes,
    libsignal_service::{
        Profile,
        content::Content,
        prelude::Uuid,
        protocol::ServiceId,
        zkgroup::{GroupMasterKeyBytes, profiles::ProfileKey},
    },
    model::{contacts::Contact, groups::Group},
    store::{ContentExt, ContentsStore, StickerPack, Thread},
};
use prost::Message;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use tracing::{debug, trace};

use crate::{BitpartStore, BitpartStoreError, db, protobuf::ContentProto};

impl ContentsStore for BitpartStore {
    type ContentsStoreError = BitpartStoreError;

    type ContactsIter = BitpartContactsIter;
    type GroupsIter = BitpartGroupsIter;
    type MessagesIter = BitpartMessagesIter;
    type StickerPacksIter = BitpartStickerPacksIter;

    async fn clear_profiles(&mut self) -> Result<(), Self::ContentsStoreError> {
        db::profiles::remove_all_profiles(&self.id, &self.pool).await?;
        Ok(())
    }

    async fn clear_contents(&mut self) -> Result<(), Self::ContentsStoreError> {
        db::contacts::remove_all(&self.id, &self.pool).await?;
        db::groups::remove_all_groups(&self.id, &self.pool).await?;
        db::messages::clear_all_messages(&self.id, &self.pool).await?;
        Ok(())
    }

    async fn clear_contacts(&mut self) -> Result<(), BitpartStoreError> {
        db::contacts::remove_all(&self.id, &self.pool).await?;
        Ok(())
    }

    async fn save_contact(&mut self, contact: &Contact) -> Result<(), BitpartStoreError> {
        let contact_data = serde_json::to_vec(contact)?;
        db::contacts::set(&self.id, contact.uuid.as_bytes(), &contact_data, &self.pool).await?;
        debug!("saved contact");
        Ok(())
    }

    async fn contacts(&self) -> Result<Self::ContactsIter, BitpartStoreError> {
        Ok(BitpartContactsIter {
            data: db::contacts::get_all(&self.id, &self.pool).await?,
            index: 0,
        })
    }

    async fn contact_by_id(&self, id: &ServiceId) -> Result<Option<Contact>, BitpartStoreError> {
        let contact_data =
            db::contacts::get(&self.id, id.raw_uuid().as_bytes(), &self.pool).await?;
        match contact_data {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    async fn clear_groups(&mut self) -> Result<(), BitpartStoreError> {
        db::groups::remove_all_groups(&self.id, &self.pool).await?;
        Ok(())
    }

    async fn groups(&self) -> Result<Self::GroupsIter, BitpartStoreError> {
        Ok(BitpartGroupsIter {
            data: db::groups::get_all_groups(&self.id, &self.pool).await?,
            index: 0,
        })
    }

    async fn group(
        &self,
        master_key_bytes: GroupMasterKeyBytes,
    ) -> Result<Option<Group>, BitpartStoreError> {
        let group_data = db::groups::get_group(&self.id, &master_key_bytes, &self.pool).await?;
        match group_data {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    async fn save_group(
        &self,
        master_key: GroupMasterKeyBytes,
        group: impl Into<Group>,
    ) -> Result<(), BitpartStoreError> {
        let group_data = serde_json::to_vec(&group.into())?;
        db::groups::set_group(&self.id, &master_key, &group_data, &self.pool).await?;
        Ok(())
    }

    async fn group_avatar(
        &self,
        master_key_bytes: GroupMasterKeyBytes,
    ) -> Result<Option<AvatarBytes>, BitpartStoreError> {
        let avatar_data =
            db::groups::get_group_avatar(&self.id, &master_key_bytes, &self.pool).await?;
        match avatar_data {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    async fn save_group_avatar(
        &self,
        master_key: GroupMasterKeyBytes,
        avatar: &AvatarBytes,
    ) -> Result<(), BitpartStoreError> {
        let avatar_data = serde_json::to_vec(avatar)?;
        db::groups::set_group_avatar(&self.id, &master_key, &avatar_data, &self.pool).await?;
        Ok(())
    }

    async fn clear_messages(&mut self) -> Result<(), BitpartStoreError> {
        db::messages::clear_all_messages(&self.id, &self.pool).await?;
        Ok(())
    }

    async fn clear_thread(&mut self, thread: &Thread) -> Result<(), BitpartStoreError> {
        trace!(%thread, "clearing thread");
        let thread_id = messages_thread_id(thread);
        db::messages::clear_thread(&self.id, &thread_id, &self.pool).await?;
        Ok(())
    }

    async fn save_message(
        &self,
        thread: &Thread,
        message: Content,
    ) -> Result<(), BitpartStoreError> {
        let ts = message.timestamp();
        trace!(%thread, ts, "storing a message with thread");

        let thread_id = messages_thread_id(thread);
        let proto: ContentProto = message.into();
        let content_data = proto.encode_to_vec();

        db::messages::set(&self.id, &thread_id, ts as i64, &content_data, &self.pool).await?;
        Ok(())
    }

    async fn delete_message(
        &mut self,
        thread: &Thread,
        timestamp: u64,
    ) -> Result<bool, BitpartStoreError> {
        let thread_id = messages_thread_id(thread);
        db::messages::remove(&self.id, &thread_id, timestamp as i64, &self.pool)
            .await
            .map(|existing| existing.is_some())
    }

    async fn message(
        &self,
        thread: &Thread,
        timestamp: u64,
    ) -> Result<Option<Content>, BitpartStoreError> {
        let thread_id = messages_thread_id(thread);
        let val = db::messages::get(&self.id, &thread_id, timestamp as i64, &self.pool).await?;
        match val {
            Some(data) => {
                let proto = ContentProto::decode(data.as_slice())?;
                let content = proto.try_into()?;
                Ok(Some(content))
            }
            None => Ok(None),
        }
    }

    async fn messages(
        &self,
        thread: &Thread,
        range: impl RangeBounds<u64>,
    ) -> Result<Self::MessagesIter, BitpartStoreError> {
        let thread_id = messages_thread_id(thread);

        let (start_ts, end_ts) = match (range.start_bound(), range.end_bound()) {
            (Bound::Included(start), Bound::Unbounded) => (*start as i64, i64::MAX),
            (Bound::Included(start), Bound::Excluded(end)) => (*start as i64, (*end - 1) as i64),
            (Bound::Included(start), Bound::Included(end)) => (*start as i64, *end as i64),
            (Bound::Unbounded, Bound::Included(end)) => (i64::MIN, *end as i64),
            (Bound::Unbounded, Bound::Excluded(end)) => (i64::MIN, (*end - 1) as i64),
            (Bound::Unbounded, Bound::Unbounded) => (i64::MIN, i64::MAX),
            (Bound::Excluded(_), _) => {
                unreachable!("range that excludes the initial value")
            }
        };

        let messages_data = if start_ts == i64::MIN && end_ts == i64::MAX {
            db::messages::get_all(&self.id, &thread_id, &self.pool).await?
        } else {
            db::messages::get_range(&self.id, &thread_id, start_ts, end_ts, &self.pool).await?
        };

        debug!(%thread, count = messages_data.len(), "loading message thread");

        let iter: Vec<(Vec<u8>, Vec<u8>)> = messages_data
            .into_iter()
            .map(|(ts, data)| ((ts as u64).to_be_bytes().into(), data))
            .collect();

        Ok(BitpartMessagesIter {
            start: 0,
            end: if iter.is_empty() { 0 } else { iter.len() - 1 },
            data: iter,
        })
    }

    async fn upsert_profile_key(
        &mut self,
        uuid: &Uuid,
        key: ProfileKey,
    ) -> Result<bool, BitpartStoreError> {
        db::profiles::set_profile_key(&self.id, uuid.as_bytes(), &key.get_bytes(), &self.pool)
            .await
            .map(|_| true)
    }

    async fn profile_key(
        &self,
        service_id: &ServiceId,
    ) -> Result<Option<ProfileKey>, BitpartStoreError> {
        let uuid = service_id.raw_uuid();
        let key_data = db::profiles::get_profile_key(&self.id, uuid.as_bytes(), &self.pool).await?;
        match key_data {
            Some(data) => {
                let key_bytes: [u8; 32] = data
                    .try_into()
                    .map_err(|_| BitpartStoreError::Store("Invalid profile key length".into()))?;
                Ok(Some(ProfileKey::create(key_bytes)))
            }
            None => Ok(None),
        }
    }

    async fn save_profile(
        &mut self,
        uuid: Uuid,
        key: ProfileKey,
        profile: Profile,
    ) -> Result<(), BitpartStoreError> {
        let profile_hash = self.profile_key_for_uuid(uuid, key);
        let profile_data = serde_json::to_vec(&profile)?;
        db::profiles::set_profile(&self.id, &profile_hash, &profile_data, &self.pool).await?;
        Ok(())
    }

    async fn profile(
        &self,
        uuid: Uuid,
        key: ProfileKey,
    ) -> Result<Option<Profile>, BitpartStoreError> {
        let profile_hash = self.profile_key_for_uuid(uuid, key);
        let profile_data = db::profiles::get_profile(&self.id, &profile_hash, &self.pool).await?;
        match profile_data {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    async fn save_profile_avatar(
        &mut self,
        uuid: Uuid,
        key: ProfileKey,
        avatar: &AvatarBytes,
    ) -> Result<(), BitpartStoreError> {
        let profile_hash = self.profile_key_for_uuid(uuid, key);
        let avatar_data = serde_json::to_vec(avatar)?;
        db::profiles::set_profile_avatar(&self.id, &profile_hash, &avatar_data, &self.pool).await?;
        Ok(())
    }

    async fn profile_avatar(
        &self,
        uuid: Uuid,
        key: ProfileKey,
    ) -> Result<Option<AvatarBytes>, BitpartStoreError> {
        let profile_hash = self.profile_key_for_uuid(uuid, key);
        let avatar_data =
            db::profiles::get_profile_avatar(&self.id, &profile_hash, &self.pool).await?;
        match avatar_data {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    async fn add_sticker_pack(&mut self, pack: &StickerPack) -> Result<(), BitpartStoreError> {
        let pack_data = serde_json::to_vec(pack)?;
        db::sticker_packs::set(&self.id, &pack.id, &pack_data, &self.pool).await?;
        Ok(())
    }

    async fn remove_sticker_pack(&mut self, id: &[u8]) -> Result<bool, BitpartStoreError> {
        db::sticker_packs::remove(&self.id, id, &self.pool)
            .await
            .map(|existing| existing.is_some())
    }

    async fn sticker_pack(&self, id: &[u8]) -> Result<Option<StickerPack>, BitpartStoreError> {
        let pack_data = db::sticker_packs::get(&self.id, id, &self.pool).await?;
        match pack_data {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    async fn sticker_packs(&self) -> Result<Self::StickerPacksIter, BitpartStoreError> {
        Ok(BitpartStickerPacksIter {
            data: db::sticker_packs::get_all(&self.id, &self.pool).await?,
            index: 0,
        })
    }
}

pub struct BitpartContactsIter {
    data: Vec<(Vec<u8>, Vec<u8>)>,
    index: usize,
}

impl BitpartContactsIter {
    fn decrypt_value<T: DeserializeOwned>(&self, value: &[u8]) -> Result<T, BitpartStoreError> {
        Ok(serde_json::from_slice(value)?)
    }
}

impl Iterator for BitpartContactsIter {
    type Item = Result<Contact, BitpartStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (_, value) = self.data.get(self.index)?;
        self.index += 1;
        self.decrypt_value(value).into()
    }
}

pub struct BitpartGroupsIter {
    data: Vec<(Vec<u8>, Vec<u8>)>,
    index: usize,
}

impl BitpartGroupsIter {
    fn decrypt_value<T: DeserializeOwned>(&self, value: &[u8]) -> Result<T, BitpartStoreError> {
        Ok(serde_json::from_slice(value)?)
    }
}

impl Iterator for BitpartGroupsIter {
    type Item = Result<(GroupMasterKeyBytes, Group), BitpartStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (key, value) = self.data.get(self.index)?;
        self.index += 1;
        let group = self.decrypt_value(value).ok()?;
        let group_master_key_bytes: Result<[u8; 32], _> = key
            .to_owned()
            .try_into()
            .map_err(|_| BitpartStoreError::GroupDecryption);
        Some(group_master_key_bytes.map(|v| (v, group)))
    }
}

pub struct BitpartStickerPacksIter {
    data: Vec<(Vec<u8>, Vec<u8>)>,
    index: usize,
}

impl BitpartStickerPacksIter {
    fn decrypt_value<T: DeserializeOwned>(&self, value: &[u8]) -> Result<T, BitpartStoreError> {
        Ok(serde_json::from_slice(value)?)
    }
}

impl Iterator for BitpartStickerPacksIter {
    type Item = Result<StickerPack, BitpartStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (_, value) = self.data.get(self.index)?;
        self.index += 1;
        self.decrypt_value(value).into()
    }
}

pub struct BitpartMessagesIter {
    data: Vec<(Vec<u8>, Vec<u8>)>,
    start: usize,
    end: usize,
}

impl BitpartMessagesIter {
    fn decode(
        &self,
        elem: Result<(&Vec<u8>, &Vec<u8>), BitpartStoreError>,
    ) -> Option<Result<Content, BitpartStoreError>> {
        elem.and_then(|(_, value)| {
            ContentProto::decode(&value[..]).map_err(BitpartStoreError::from)
        })
        .map_or_else(|e| Some(Err(e)), |p| Some(p.try_into()))
    }
}

impl Iterator for BitpartMessagesIter {
    type Item = Result<Content, BitpartStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (key, value) = self.data.get(self.start)?;
        self.start += 1;
        self.decode(Ok((key, value)))
    }
}

impl DoubleEndedIterator for BitpartMessagesIter {
    fn next_back(&mut self) -> Option<Self::Item> {
        let (key, value) = self.data.get(self.end)?;
        if self.end > 0 {
            self.end -= 1;
        }
        self.decode(Ok((key, value)))
    }
}

fn messages_thread_id(t: &Thread) -> String {
    use base64::prelude::*;
    let key = match t {
        Thread::Contact(service_id) => {
            format!("threads:contact:{}", service_id.raw_uuid())
        }
        Thread::Group(group_id) => format!("threads:group:{}", BASE64_STANDARD.encode(group_id)),
    };
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}
