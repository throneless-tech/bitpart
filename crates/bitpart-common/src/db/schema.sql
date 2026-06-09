-- Bitpart schema, version 1. Do not edit in place; add a new migration
-- in `bitpart_common::db::migration` and bump the version.

CREATE TABLE "bot" (
    "id" uuid_text NOT NULL PRIMARY KEY,
    "bot_id" varchar NOT NULL,
    "bot" varchar NOT NULL,
    "engine_version" varchar NOT NULL,
    "updated_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "created_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE "channel" (
    "id" uuid_text NOT NULL PRIMARY KEY,
    "bot_id" varchar NOT NULL,
    "channel_id" varchar NOT NULL,
    "created_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "updated_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE "channel_state" (
    "id" uuid_text NOT NULL PRIMARY KEY,
    "channel_id" varchar NOT NULL,
    "tree" varchar NOT NULL,
    "key" varchar NOT NULL,
    "value" varchar NOT NULL,
    "created_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "updated_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE "conversation" (
    "id" uuid_text NOT NULL PRIMARY KEY,
    "bot_id" varchar NOT NULL,
    "channel_id" varchar NOT NULL,
    "user_id" varchar NOT NULL,
    "flow_id" varchar NOT NULL,
    "step_id" varchar NOT NULL,
    "status" varchar NOT NULL,
    "last_interaction_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "updated_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "created_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "expires_at" datetime_text
);

CREATE TABLE "memory" (
    "id" uuid_text NOT NULL PRIMARY KEY,
    "bot_id" varchar NOT NULL,
    "channel_id" varchar NOT NULL,
    "user_id" varchar NOT NULL,
    "key" varchar NOT NULL,
    "value" varchar NOT NULL,
    "created_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "updated_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "expires_at" datetime_text
);

CREATE TABLE "message" (
    "id" uuid_text NOT NULL PRIMARY KEY,
    "conversation_id" uuid_text NOT NULL,
    "flow_id" varchar NOT NULL,
    "step_id" varchar NOT NULL,
    "direction" varchar NOT NULL,
    "payload" varchar NOT NULL,
    "content_type" varchar NOT NULL,
    "message_order" integer NOT NULL,
    "interaction_order" integer NOT NULL,
    "created_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "updated_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "expires_at" datetime_text,
    FOREIGN KEY ("conversation_id") REFERENCES "conversation" ("id")
);

CREATE TABLE "state" (
    "id" uuid_text NOT NULL PRIMARY KEY,
    "bot_id" varchar NOT NULL,
    "channel_id" varchar NOT NULL,
    "user_id" varchar NOT NULL,
    "type" varchar NOT NULL,
    "key" varchar NOT NULL,
    "value" varchar NOT NULL,
    "created_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "updated_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "expires_at" datetime_text
);

CREATE TRIGGER bot_updated_at
            AFTER UPDATE ON bot
            FOR EACH ROW
            BEGIN
                UPDATE bot
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;

CREATE TRIGGER channel_state_updated_at
            AFTER UPDATE ON channel_state
            FOR EACH ROW
            BEGIN
                UPDATE channel_state
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;

CREATE TRIGGER channel_updated_at
            AFTER UPDATE ON channel
            FOR EACH ROW
            BEGIN
                UPDATE channel
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;

CREATE TRIGGER conversation_updated_at
            AFTER UPDATE ON conversation
            FOR EACH ROW
            BEGIN
                UPDATE conversation
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;

CREATE TRIGGER memory_updated_at
            AFTER UPDATE ON memory
            FOR EACH ROW
            BEGIN
                UPDATE memory
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;

CREATE TRIGGER message_updated_at
            AFTER UPDATE ON message
            FOR EACH ROW
            BEGIN
                UPDATE message
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;

CREATE TRIGGER state_updated_at
            AFTER UPDATE ON state
            FOR EACH ROW
            BEGIN
                UPDATE state
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;
