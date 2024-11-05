use std::{
    ops::{Bound, RangeBounds, RangeFull},
    sync::Arc,
};

use presage::{
    libsignal_service::{
        content::Content,
        prelude::Uuid,
        zkgroup::{profiles::ProfileKey, GroupMasterKeyBytes},
        Profile,
    },
    model::{contacts::Contact, groups::Group},
    store::{ContentExt, ContentsStore, StickerPack, Thread},
    AvatarBytes,
};
use prost::Message;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use sled::IVec;
use tracing::{debug, trace};

use crate::{protobuf::ContentProto, BitpartStore, BitpartStoreError};

const SLED_TREE_PROFILE_AVATARS: &str = "profile_avatars";
const SLED_TREE_PROFILE_KEYS: &str = "profile_keys";
const SLED_TREE_STICKER_PACKS: &str = "sticker_packs";
const SLED_TREE_CONTACTS: &str = "contacts";
const SLED_TREE_GROUP_AVATARS: &str = "group_avatars";
const SLED_TREE_GROUPS: &str = "groups";
const SLED_TREE_PROFILES: &str = "profiles";
const SLED_TREE_THREADS_PREFIX: &str = "threads";

impl ContentsStore for BitpartStore {
    type ContentsStoreError = BitpartStoreError;

    type ContactsIter = BitpartContactsIter;
    type GroupsIter = BitpartGroupsIter;
    type MessagesIter = BitpartMessagesIter;
    type StickerPacksIter = BitpartStickerPacksIter;

    async fn clear_profiles(&mut self) -> Result<(), Self::ContentsStoreError> {
        let db = self.write();
        db.drop_tree(SLED_TREE_PROFILES)?;
        db.drop_tree(SLED_TREE_PROFILE_KEYS)?;
        db.drop_tree(SLED_TREE_PROFILE_AVATARS)?;
        db.flush()?;
        Ok(())
    }

    async fn clear_contents(&mut self) -> Result<(), Self::ContentsStoreError> {
        let db = self.write();
        db.drop_tree(SLED_TREE_CONTACTS)?;
        db.drop_tree(SLED_TREE_GROUPS)?;

        for tree in db
            .tree_names()
            .into_iter()
            .filter(|n| n.starts_with(SLED_TREE_THREADS_PREFIX.as_bytes()))
        {
            db.drop_tree(tree)?;
        }

        db.flush()?;
        Ok(())
    }

    async fn clear_contacts(&mut self) -> Result<(), BitpartStoreError> {
        self.write().drop_tree(SLED_TREE_CONTACTS)?;
        Ok(())
    }

    async fn save_contact(&mut self, contact: &Contact) -> Result<(), BitpartStoreError> {
        self.insert(SLED_TREE_CONTACTS, contact.uuid, contact)?;
        debug!("saved contact");
        Ok(())
    }

    async fn contacts(&self) -> Result<Self::ContactsIter, BitpartStoreError> {
        Ok(BitpartContactsIter {
            iter: self.read().open_tree(SLED_TREE_CONTACTS)?.iter(),
            #[cfg(feature = "encryption")]
            cipher: self.cipher.clone(),
        })
    }

    async fn contact_by_id(&self, id: &Uuid) -> Result<Option<Contact>, BitpartStoreError> {
        self.get(SLED_TREE_CONTACTS, id)
    }

    /// Groups

    async fn clear_groups(&mut self) -> Result<(), BitpartStoreError> {
        let db = self.write();
        db.drop_tree(SLED_TREE_GROUPS)?;
        db.flush()?;
        Ok(())
    }

    async fn groups(&self) -> Result<Self::GroupsIter, BitpartStoreError> {
        Ok(BitpartGroupsIter {
            iter: self.read().open_tree(SLED_TREE_GROUPS)?.iter(),
            #[cfg(feature = "encryption")]
            cipher: self.cipher.clone(),
        })
    }

    async fn group(
        &self,
        master_key_bytes: GroupMasterKeyBytes,
    ) -> Result<Option<Group>, BitpartStoreError> {
        self.get(SLED_TREE_GROUPS, master_key_bytes)
    }

    async fn save_group(
        &self,
        master_key: GroupMasterKeyBytes,
        group: impl Into<Group>,
    ) -> Result<(), BitpartStoreError> {
        self.insert(SLED_TREE_GROUPS, master_key, group.into())?;
        Ok(())
    }

    async fn group_avatar(
        &self,
        master_key_bytes: GroupMasterKeyBytes,
    ) -> Result<Option<AvatarBytes>, BitpartStoreError> {
        self.get(SLED_TREE_GROUP_AVATARS, master_key_bytes)
    }

    async fn save_group_avatar(
        &self,
        master_key: GroupMasterKeyBytes,
        avatar: &AvatarBytes,
    ) -> Result<(), BitpartStoreError> {
        self.insert(SLED_TREE_GROUP_AVATARS, master_key, avatar)?;
        Ok(())
    }

    /// Messages

    async fn clear_messages(&mut self) -> Result<(), BitpartStoreError> {
        let db = self.write();
        for name in db.tree_names() {
            if name
                .as_ref()
                .starts_with(SLED_TREE_THREADS_PREFIX.as_bytes())
            {
                db.drop_tree(&name)?;
            }
        }
        db.flush()?;
        Ok(())
    }

    async fn clear_thread(&mut self, thread: &Thread) -> Result<(), BitpartStoreError> {
        trace!(%thread, "clearing thread");

        let db = self.write();
        db.drop_tree(messages_thread_tree_name(thread))?;
        db.flush()?;

        Ok(())
    }

    async fn save_message(&self, thread: &Thread, message: Content) -> Result<(), BitpartStoreError> {
        let ts = message.timestamp();
        trace!(%thread, ts, "storing a message with thread");

        let tree = messages_thread_tree_name(thread);
        let key = ts.to_be_bytes();

        let proto: ContentProto = message.into();
        let value = proto.encode_to_vec();

        self.insert(&tree, key, value)?;

        Ok(())
    }

    async fn delete_message(
        &mut self,
        thread: &Thread,
        timestamp: u64,
    ) -> Result<bool, BitpartStoreError> {
        let tree = messages_thread_tree_name(thread);
        self.remove(&tree, timestamp.to_be_bytes())
    }

    async fn message(
        &self,
        thread: &Thread,
        timestamp: u64,
    ) -> Result<Option<Content>, BitpartStoreError> {
        // Big-Endian needed, otherwise wrong ordering in sled.
        let val: Option<Vec<u8>> =
            self.get(&messages_thread_tree_name(thread), timestamp.to_be_bytes())?;
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
        let tree_thread = self.read().open_tree(messages_thread_tree_name(thread))?;
        debug!(%thread, count = tree_thread.len(), "loading message tree");

        let iter = match (range.start_bound(), range.end_bound()) {
            (Bound::Included(start), Bound::Unbounded) => tree_thread.range(start.to_be_bytes()..),
            (Bound::Included(start), Bound::Excluded(end)) => {
                tree_thread.range(start.to_be_bytes()..end.to_be_bytes())
            }
            (Bound::Included(start), Bound::Included(end)) => {
                tree_thread.range(start.to_be_bytes()..=end.to_be_bytes())
            }
            (Bound::Unbounded, Bound::Included(end)) => tree_thread.range(..=end.to_be_bytes()),
            (Bound::Unbounded, Bound::Excluded(end)) => tree_thread.range(..end.to_be_bytes()),
            (Bound::Unbounded, Bound::Unbounded) => tree_thread.range::<[u8; 8], RangeFull>(..),
            (Bound::Excluded(_), _) => {
                unreachable!("range that excludes the initial value")
            }
        };

        Ok(BitpartMessagesIter {
            #[cfg(feature = "encryption")]
            cipher: self.cipher.clone(),
            iter,
        })
    }

    async fn upsert_profile_key(
        &mut self,
        uuid: &Uuid,
        key: ProfileKey,
    ) -> Result<bool, BitpartStoreError> {
        self.insert(SLED_TREE_PROFILE_KEYS, uuid.as_bytes(), key)
    }

    async fn profile_key(&self, uuid: &Uuid) -> Result<Option<ProfileKey>, BitpartStoreError> {
        self.get(SLED_TREE_PROFILE_KEYS, uuid.as_bytes())
    }

    async fn save_profile(
        &mut self,
        uuid: Uuid,
        key: ProfileKey,
        profile: Profile,
    ) -> Result<(), BitpartStoreError> {
        let key = self.profile_key_for_uuid(uuid, key);
        self.insert(SLED_TREE_PROFILES, key, profile)?;
        Ok(())
    }

    async fn profile(
        &self,
        uuid: Uuid,
        key: ProfileKey,
    ) -> Result<Option<Profile>, BitpartStoreError> {
        let key = self.profile_key_for_uuid(uuid, key);
        self.get(SLED_TREE_PROFILES, key)
    }

    async fn save_profile_avatar(
        &mut self,
        uuid: Uuid,
        key: ProfileKey,
        avatar: &AvatarBytes,
    ) -> Result<(), BitpartStoreError> {
        let key = self.profile_key_for_uuid(uuid, key);
        self.insert(SLED_TREE_PROFILE_AVATARS, key, avatar)?;
        Ok(())
    }

    async fn profile_avatar(
        &self,
        uuid: Uuid,
        key: ProfileKey,
    ) -> Result<Option<AvatarBytes>, BitpartStoreError> {
        let key = self.profile_key_for_uuid(uuid, key);
        self.get(SLED_TREE_PROFILE_AVATARS, key)
    }

    async fn add_sticker_pack(&mut self, pack: &StickerPack) -> Result<(), BitpartStoreError> {
        self.insert(SLED_TREE_STICKER_PACKS, pack.id.clone(), pack)?;
        Ok(())
    }

    async fn remove_sticker_pack(&mut self, id: &[u8]) -> Result<bool, BitpartStoreError> {
        self.remove(SLED_TREE_STICKER_PACKS, id)
    }

    async fn sticker_pack(&self, id: &[u8]) -> Result<Option<StickerPack>, BitpartStoreError> {
        self.get(SLED_TREE_STICKER_PACKS, id)
    }

    async fn sticker_packs(&self) -> Result<Self::StickerPacksIter, BitpartStoreError> {
        Ok(BitpartStickerPacksIter {
            cipher: self.cipher.clone(),
            iter: self.read().open_tree(SLED_TREE_STICKER_PACKS)?.iter(),
        })
    }
}

pub struct BitpartContactsIter {
    #[cfg(feature = "encryption")]
    cipher: Option<Arc<presage_store_cipher::StoreCipher>>,
    iter: sled::Iter,
}

impl BitpartContactsIter {
    #[cfg(feature = "encryption")]
    fn decrypt_value<T: DeserializeOwned>(&self, value: &[u8]) -> Result<T, BitpartStoreError> {
        if let Some(cipher) = self.cipher.as_ref() {
            Ok(cipher.decrypt_value(value)?)
        } else {
            Ok(serde_json::from_slice(value)?)
        }
    }

    #[cfg(not(feature = "encryption"))]
    fn decrypt_value<T: DeserializeOwned>(&self, value: &[u8]) -> Result<T, BitpartStoreError> {
        Ok(serde_json::from_slice(value)?)
    }
}

impl Iterator for BitpartContactsIter {
    type Item = Result<Contact, BitpartStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .next()?
            .map_err(BitpartStoreError::from)
            .and_then(|(_key, value)| self.decrypt_value(&value))
            .into()
    }
}

pub struct BitpartGroupsIter {
    #[cfg(feature = "encryption")]
    cipher: Option<Arc<presage_store_cipher::StoreCipher>>,
    iter: sled::Iter,
}

impl BitpartGroupsIter {
    #[cfg(feature = "encryption")]
    fn decrypt_value<T: DeserializeOwned>(&self, value: &[u8]) -> Result<T, BitpartStoreError> {
        if let Some(cipher) = self.cipher.as_ref() {
            Ok(cipher.decrypt_value(value)?)
        } else {
            Ok(serde_json::from_slice(value)?)
        }
    }

    #[cfg(not(feature = "encryption"))]
    fn decrypt_value<T: DeserializeOwned>(&self, value: &[u8]) -> Result<T, BitpartStoreError> {
        Ok(serde_json::from_slice(value)?)
    }
}

impl Iterator for BitpartGroupsIter {
    type Item = Result<(GroupMasterKeyBytes, Group), BitpartStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.iter.next()?.map_err(BitpartStoreError::from).and_then(
            |(group_master_key_bytes, value)| {
                let group = self.decrypt_value(&value)?;
                Ok((
                    group_master_key_bytes
                        .as_ref()
                        .try_into()
                        .map_err(|_| BitpartStoreError::GroupDecryption)?,
                    group,
                ))
            },
        ))
    }
}

pub struct BitpartStickerPacksIter {
    #[cfg(feature = "encryption")]
    cipher: Option<Arc<presage_store_cipher::StoreCipher>>,
    iter: sled::Iter,
}

impl Iterator for BitpartStickerPacksIter {
    type Item = Result<StickerPack, BitpartStoreError>;

    #[cfg(feature = "encryption")]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .next()?
            .map_err(BitpartStoreError::from)
            .and_then(|(_key, value)| {
                if let Some(cipher) = self.cipher.as_ref() {
                    cipher.decrypt_value(&value).map_err(BitpartStoreError::from)
                } else {
                    serde_json::from_slice(&value).map_err(BitpartStoreError::from)
                }
            })
            .into()
    }

    #[cfg(not(feature = "encryption"))]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .next()?
            .map_err(BitpartStoreError::from)
            .and_then(|(_key, value)| serde_json::from_slice(&value).map_err(BitpartStoreError::from))
            .into()
    }
}

pub struct BitpartMessagesIter {
    #[cfg(feature = "encryption")]
    cipher: Option<Arc<presage_store_cipher::StoreCipher>>,
    iter: sled::Iter,
}

impl BitpartMessagesIter {
    #[cfg(feature = "encryption")]
    fn decrypt_value<T: DeserializeOwned>(&self, value: &[u8]) -> Result<T, BitpartStoreError> {
        if let Some(cipher) = self.cipher.as_ref() {
            Ok(cipher.decrypt_value(value)?)
        } else {
            Ok(serde_json::from_slice(value)?)
        }
    }

    #[cfg(not(feature = "encryption"))]
    fn decrypt_value<T: DeserializeOwned>(&self, value: &[u8]) -> Result<T, BitpartStoreError> {
        Ok(serde_json::from_slice(value)?)
    }
}

impl BitpartMessagesIter {
    fn decode(
        &self,
        elem: Result<(IVec, IVec), sled::Error>,
    ) -> Option<Result<Content, BitpartStoreError>> {
        elem.map_err(BitpartStoreError::from)
            .and_then(|(_, value)| self.decrypt_value(&value).map_err(BitpartStoreError::from))
            .and_then(|data: Vec<u8>| ContentProto::decode(&data[..]).map_err(BitpartStoreError::from))
            .map_or_else(|e| Some(Err(e)), |p| Some(p.try_into()))
    }
}

impl Iterator for BitpartMessagesIter {
    type Item = Result<Content, BitpartStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let elem = self.iter.next()?;
        self.decode(elem)
    }
}

impl DoubleEndedIterator for BitpartMessagesIter {
    fn next_back(&mut self) -> Option<Self::Item> {
        let elem = self.iter.next_back()?;
        self.decode(elem)
    }
}

fn messages_thread_tree_name(t: &Thread) -> String {
    use base64::prelude::*;
    let key = match t {
        Thread::Contact(uuid) => {
            format!("{SLED_TREE_THREADS_PREFIX}:contact:{uuid}")
        }
        Thread::Group(group_id) => format!(
            "{SLED_TREE_THREADS_PREFIX}:group:{}",
            BASE64_STANDARD.encode(group_id)
        ),
    };
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{SLED_TREE_THREADS_PREFIX}:{:x}", hasher.finalize())
}
