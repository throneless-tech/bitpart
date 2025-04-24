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

const BITPART_TREE_PROFILE_AVATARS: &str = "profile_avatars";
const BITPART_TREE_PROFILE_KEYS: &str = "profile_keys";
const BITPART_TREE_STICKER_PACKS: &str = "sticker_packs";
const BITPART_TREE_CONTACTS: &str = "contacts";
const BITPART_TREE_GROUP_AVATARS: &str = "group_avatars";
const BITPART_TREE_GROUPS: &str = "groups";
const BITPART_TREE_PROFILES: &str = "profiles";
const BITPART_TREE_THREADS_PREFIX: &str = "threads";

impl ContentsStore for BitpartStore {
    type ContentsStoreError = BitpartStoreError;

    type ContactsIter = BitpartContactsIter;
    type GroupsIter = BitpartGroupsIter;
    type MessagesIter = BitpartMessagesIter;
    type StickerPacksIter = BitpartStickerPacksIter;

    async fn clear_profiles(&mut self) -> Result<(), Self::ContentsStoreError> {
        {
            self.remove_all(BITPART_TREE_PROFILES).await?;
            self.remove_all(BITPART_TREE_PROFILE_KEYS).await?;
            self.remove_all(BITPART_TREE_PROFILE_AVATARS).await?;
        }
        Ok(())
    }

    async fn clear_contents(&mut self) -> Result<(), Self::ContentsStoreError> {
        {
            self.remove_all(BITPART_TREE_CONTACTS).await?;
            self.remove_all(BITPART_TREE_GROUPS).await?;

            for tree in db::channel_state::get_trees(&self.id, &self.db)
                .await?
                .into_iter()
                .filter(|n| n.starts_with(BITPART_TREE_THREADS_PREFIX))
            {
                self.remove_all(&tree).await?;
            }
        }

        Ok(())
    }

    async fn clear_contacts(&mut self) -> Result<(), BitpartStoreError> {
        self.remove_all(BITPART_TREE_CONTACTS).await?;
        Ok(())
    }

    async fn save_contact(&mut self, contact: &Contact) -> Result<(), BitpartStoreError> {
        self.insert(BITPART_TREE_CONTACTS, contact.uuid, contact)
            .await?;
        debug!("saved contact");
        Ok(())
    }

    async fn contacts(&self) -> Result<Self::ContactsIter, BitpartStoreError> {
        Ok(BitpartContactsIter {
            data: db::channel_state::get_all(&self.id, BITPART_TREE_CONTACTS, &self.db)
                .await?
                .into_iter()
                .map(|(k, v)| (k.into_bytes(), v.into_bytes()))
                .collect(),
            index: 0,
        })
    }

    async fn contact_by_id(&self, id: &Uuid) -> Result<Option<Contact>, BitpartStoreError> {
        self.get(BITPART_TREE_CONTACTS, id).await
    }

    /// Groups

    async fn clear_groups(&mut self) -> Result<(), BitpartStoreError> {
        self.remove_all(BITPART_TREE_GROUPS).await?;
        Ok(())
    }

    async fn groups(&self) -> Result<Self::GroupsIter, BitpartStoreError> {
        Ok(BitpartGroupsIter {
            data: db::channel_state::get_all(&self.id, BITPART_TREE_GROUPS, &self.db)
                .await?
                .into_iter()
                .map(|(k, v)| (k.into_bytes(), v.into_bytes()))
                .collect(),
            index: 0,
        })
    }

    async fn group(
        &self,
        master_key_bytes: GroupMasterKeyBytes,
    ) -> Result<Option<Group>, BitpartStoreError> {
        self.get(BITPART_TREE_GROUPS, master_key_bytes).await
    }

    async fn save_group(
        &self,
        master_key: GroupMasterKeyBytes,
        group: impl Into<Group>,
    ) -> Result<(), BitpartStoreError> {
        self.insert(BITPART_TREE_GROUPS, master_key, group.into())
            .await?;
        Ok(())
    }

    async fn group_avatar(
        &self,
        master_key_bytes: GroupMasterKeyBytes,
    ) -> Result<Option<AvatarBytes>, BitpartStoreError> {
        self.get(BITPART_TREE_GROUP_AVATARS, master_key_bytes).await
    }

    async fn save_group_avatar(
        &self,
        master_key: GroupMasterKeyBytes,
        avatar: &AvatarBytes,
    ) -> Result<(), BitpartStoreError> {
        self.insert(BITPART_TREE_GROUP_AVATARS, master_key, avatar)
            .await?;
        Ok(())
    }

    /// Messages

    async fn clear_messages(&mut self) -> Result<(), BitpartStoreError> {
        for name in db::channel_state::get_trees(&self.id, &self.db).await? {
            if name.starts_with(BITPART_TREE_THREADS_PREFIX) {
                db::channel_state::remove_all(&self.id, &name, &self.db).await?;
            }
        }
        Ok(())
    }

    async fn clear_thread(&mut self, thread: &Thread) -> Result<(), BitpartStoreError> {
        trace!(%thread, "clearing thread");

        self.remove_all(&messages_thread_tree_name(thread)).await?;

        Ok(())
    }

    async fn save_message(
        &self,
        thread: &Thread,
        message: Content,
    ) -> Result<(), BitpartStoreError> {
        let ts = message.timestamp();
        trace!(%thread, ts, "storing a message with thread");

        let tree = messages_thread_tree_name(thread);
        let key = ts.to_be_bytes();

        let proto: ContentProto = message.into();
        let value = proto.encode_to_vec();

        self.insert(&tree, key, value).await?;

        Ok(())
    }

    async fn delete_message(
        &mut self,
        thread: &Thread,
        timestamp: u64,
    ) -> Result<bool, BitpartStoreError> {
        let tree = messages_thread_tree_name(thread);
        self.remove(&tree, timestamp.to_be_bytes()).await
    }

    async fn message(
        &self,
        thread: &Thread,
        timestamp: u64,
    ) -> Result<Option<Content>, BitpartStoreError> {
        let val: Option<Vec<u8>> = self
            .get(&messages_thread_tree_name(thread), timestamp.to_be_bytes())
            .await?;
        match val {
            Some(ref v) => {
                let proto = ContentProto::decode(v.as_slice())?;
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
        let mut tree_thread: Vec<(u64, Vec<u8>)> = self
            .get_all(&messages_thread_tree_name(thread))
            .await?
            .into_iter()
            .map(|(k, v)| (u64::from_be_bytes(k), v))
            .collect();
        tree_thread.sort();
        debug!(%thread, count = tree_thread.len(), "loading message tree");

        let range = match (range.start_bound(), range.end_bound()) {
            (Bound::Included(start), Bound::Unbounded) => {
                &tree_thread[tree_thread
                    .iter()
                    .position(|(k, _)| k >= start)
                    .unwrap_or(tree_thread.len())..]
            }
            (Bound::Included(start), Bound::Excluded(end)) => {
                &tree_thread[tree_thread
                    .iter()
                    .position(|(k, _)| k >= start)
                    .unwrap_or(tree_thread.len())
                    ..tree_thread.iter().rposition(|(k, _)| k <= end).unwrap_or(0)]
            }
            (Bound::Included(start), Bound::Included(end)) => {
                &tree_thread[tree_thread
                    .iter()
                    .position(|(k, _)| k >= start)
                    .unwrap_or(tree_thread.len())
                    ..=tree_thread.iter().rposition(|(k, _)| k <= end).unwrap_or(0)]
            }
            (Bound::Unbounded, Bound::Included(end)) => {
                &tree_thread[..=tree_thread.iter().rposition(|(k, _)| k <= end).unwrap_or(0)]
            }
            (Bound::Unbounded, Bound::Excluded(end)) => {
                &tree_thread[..tree_thread.iter().rposition(|(k, _)| k <= end).unwrap_or(0)]
            }
            (Bound::Unbounded, Bound::Unbounded) => &tree_thread,
            (Bound::Excluded(_), _) => {
                unreachable!("range that excludes the initial value")
            }
        };

        let iter = Vec::from_iter(
            range
                .iter()
                .map(|(k, v)| (k.clone().to_be_bytes().into(), v.clone())),
        );
        let end = if iter.len() > 0 { iter.len() - 1 } else { 0 };

        Ok(BitpartMessagesIter {
            start: 0,
            end,
            data: iter,
        })
    }

    async fn upsert_profile_key(
        &mut self,
        uuid: &Uuid,
        key: ProfileKey,
    ) -> Result<bool, BitpartStoreError> {
        db::channel_state::set(
            &self.id,
            BITPART_TREE_PROFILE_KEYS,
            &uuid.to_string(),
            String::from_utf8_lossy(&key.get_bytes()),
            &self.db,
        )
        .await
        .map(|_| true)
    }

    async fn profile_key(&self, uuid: &Uuid) -> Result<Option<ProfileKey>, BitpartStoreError> {
        self.get(BITPART_TREE_PROFILE_KEYS, uuid.as_bytes()).await
    }

    async fn save_profile(
        &mut self,
        uuid: Uuid,
        key: ProfileKey,
        profile: Profile,
    ) -> Result<(), BitpartStoreError> {
        let key = self.profile_key_for_uuid(uuid, key);
        self.insert(BITPART_TREE_PROFILES, key, profile).await?;
        Ok(())
    }

    async fn profile(
        &self,
        uuid: Uuid,
        key: ProfileKey,
    ) -> Result<Option<Profile>, BitpartStoreError> {
        let key = self.profile_key_for_uuid(uuid, key);
        self.get(BITPART_TREE_PROFILES, key).await
    }

    async fn save_profile_avatar(
        &mut self,
        uuid: Uuid,
        key: ProfileKey,
        avatar: &AvatarBytes,
    ) -> Result<(), BitpartStoreError> {
        let key = self.profile_key_for_uuid(uuid, key);
        self.insert(BITPART_TREE_PROFILE_AVATARS, key, avatar)
            .await?;
        Ok(())
    }

    async fn profile_avatar(
        &self,
        uuid: Uuid,
        key: ProfileKey,
    ) -> Result<Option<AvatarBytes>, BitpartStoreError> {
        let key = self.profile_key_for_uuid(uuid, key);
        self.get(BITPART_TREE_PROFILE_AVATARS, key).await
    }

    async fn add_sticker_pack(&mut self, pack: &StickerPack) -> Result<(), BitpartStoreError> {
        self.insert(BITPART_TREE_STICKER_PACKS, pack.id.clone(), pack)
            .await?;
        Ok(())
    }

    async fn remove_sticker_pack(&mut self, id: &[u8]) -> Result<bool, BitpartStoreError> {
        self.remove(BITPART_TREE_STICKER_PACKS, id).await
    }

    async fn sticker_pack(&self, id: &[u8]) -> Result<Option<StickerPack>, BitpartStoreError> {
        self.get(BITPART_TREE_STICKER_PACKS, id).await
    }

    async fn sticker_packs(&self) -> Result<Self::StickerPacksIter, BitpartStoreError> {
        Ok(BitpartStickerPacksIter {
            data: db::channel_state::get_all(&self.id, BITPART_TREE_STICKER_PACKS, &self.db)
                .await?
                .into_iter()
                .map(|(k, v)| (k.into_bytes(), v.into_bytes()))
                .collect(),
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
        self.decrypt_value(&value).into()
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
        let group = self.decrypt_value(&value).ok()?;
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
        self.decrypt_value(&value).into()
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
        elem.map_err(BitpartStoreError::from)
            .and_then(|(_, value)| {
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

fn messages_thread_tree_name(t: &Thread) -> String {
    use base64::prelude::*;
    let key = match t {
        Thread::Contact(uuid) => {
            format!("{BITPART_TREE_THREADS_PREFIX}:contact:{uuid}")
        }
        Thread::Group(group_id) => format!(
            "{BITPART_TREE_THREADS_PREFIX}:group:{}",
            BASE64_STANDARD.encode(group_id)
        ),
    };
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{BITPART_TREE_THREADS_PREFIX}:{:x}", hasher.finalize())
}
