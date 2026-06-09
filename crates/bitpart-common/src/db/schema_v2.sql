-- Bitpart schema, version 2. Do not edit in place; add a new migration
-- in `bitpart_common::db::migration` and bump the version.

-- Drop the generic channel_state KV table and its trigger
DROP TRIGGER IF EXISTS channel_state_updated_at;
DROP TABLE IF EXISTS channel_state;

-- Protocol tables for ACI (one set per Signal instance)
CREATE TABLE "signal_identities" (
    "channel_id" varchar NOT NULL,
    "is_pni" integer NOT NULL,
    "address" varchar NOT NULL,
    "identity_key" blob NOT NULL,
    PRIMARY KEY ("channel_id", "is_pni", "address")
);

CREATE TABLE "signal_sessions" (
    "channel_id" varchar NOT NULL,
    "address" varchar NOT NULL,
    "session_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "address")
);

CREATE TABLE "signal_pre_keys" (
    "channel_id" varchar NOT NULL,
    "key_id" integer NOT NULL,
    "record_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "key_id")
);

CREATE TABLE "signal_signed_pre_keys" (
    "channel_id" varchar NOT NULL,
    "key_id" integer NOT NULL,
    "record_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "key_id")
);

CREATE TABLE "signal_kyber_pre_keys" (
    "channel_id" varchar NOT NULL,
    "key_id" integer NOT NULL,
    "record_data" blob NOT NULL,
    "is_last_resort" integer NOT NULL DEFAULT 0,
    PRIMARY KEY ("channel_id", "key_id")
);

CREATE TABLE "signal_sender_keys" (
    "channel_id" varchar NOT NULL,
    "sender_key" varchar NOT NULL,
    "record_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "sender_key")
);

CREATE TABLE "signal_base_keys_seen" (
    "channel_id" varchar NOT NULL,
    "is_pni" integer NOT NULL,
    "kyber_pre_key_id" integer NOT NULL,
    "signed_pre_key_id" integer NOT NULL,
    "base_key" blob NOT NULL,
    PRIMARY KEY ("channel_id", "is_pni", "kyber_pre_key_id")
);

CREATE TABLE "signal_state" (
    "channel_id" varchar NOT NULL,
    "key" varchar NOT NULL,
    "value" blob NOT NULL,
    PRIMARY KEY ("channel_id", "key")
);

-- Protocol tables for PNI (one set per Signal instance)
CREATE TABLE "signal_pni_sessions" (
    "channel_id" varchar NOT NULL,
    "address" varchar NOT NULL,
    "session_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "address")
);

CREATE TABLE "signal_pni_pre_keys" (
    "channel_id" varchar NOT NULL,
    "key_id" integer NOT NULL,
    "record_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "key_id")
);

CREATE TABLE "signal_pni_signed_pre_keys" (
    "channel_id" varchar NOT NULL,
    "key_id" integer NOT NULL,
    "record_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "key_id")
);

CREATE TABLE "signal_pni_kyber_pre_keys" (
    "channel_id" varchar NOT NULL,
    "key_id" integer NOT NULL,
    "record_data" blob NOT NULL,
    "is_last_resort" integer NOT NULL DEFAULT 0,
    PRIMARY KEY ("channel_id", "key_id")
);

CREATE TABLE "signal_pni_sender_keys" (
    "channel_id" varchar NOT NULL,
    "sender_key" varchar NOT NULL,
    "record_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "sender_key")
);

CREATE TABLE "signal_pni_state" (
    "channel_id" varchar NOT NULL,
    "key" varchar NOT NULL,
    "value" blob NOT NULL,
    PRIMARY KEY ("channel_id", "key")
);

-- Content tables (shared across instances)
CREATE TABLE "signal_profiles" (
    "channel_id" varchar NOT NULL,
    "profile_hash" varchar NOT NULL,
    "profile_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "profile_hash")
);

CREATE TABLE "signal_profile_keys" (
    "channel_id" varchar NOT NULL,
    "uuid" blob NOT NULL,
    "profile_key" blob NOT NULL,
    PRIMARY KEY ("channel_id", "uuid")
);

CREATE TABLE "signal_profile_avatars" (
    "channel_id" varchar NOT NULL,
    "profile_hash" varchar NOT NULL,
    "avatar_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "profile_hash")
);

CREATE TABLE "signal_contacts" (
    "channel_id" varchar NOT NULL,
    "uuid" blob NOT NULL,
    "contact_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "uuid")
);

CREATE TABLE "signal_groups" (
    "channel_id" varchar NOT NULL,
    "master_key" blob NOT NULL,
    "group_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "master_key")
);

CREATE TABLE "signal_group_avatars" (
    "channel_id" varchar NOT NULL,
    "master_key" blob NOT NULL,
    "avatar_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "master_key")
);

CREATE TABLE "signal_sticker_packs" (
    "channel_id" varchar NOT NULL,
    "pack_id" blob NOT NULL,
    "pack_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "pack_id")
);

CREATE TABLE "signal_messages" (
    "channel_id" varchar NOT NULL,
    "thread_id" varchar NOT NULL,
    "timestamp" integer NOT NULL,
    "content_data" blob NOT NULL,
    PRIMARY KEY ("channel_id", "thread_id", "timestamp")
);