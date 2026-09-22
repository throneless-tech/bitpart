-- Bitpart schema, version 3. Do not edit in place; add a new migration
-- in `bitpart_common::db::migration` and bump the version.

CREATE TABLE "signal_expire_timers" (
    "channel_id" varchar NOT NULL,
    "thread_id" varchar NOT NULL,
    "timer" integer NOT NULL,
    "version" integer NOT NULL,
    PRIMARY KEY ("channel_id", "thread_id")
);

DELETE FROM "signal_messages";

CREATE TABLE "conversation_new" (
    "id" uuid_text NOT NULL PRIMARY KEY,
    "bot_id" varchar NOT NULL,
    "channel_id" varchar NOT NULL,
    "user_id" varchar NOT NULL,
    "flow_id" varchar NOT NULL,
    "step_id" varchar NOT NULL,
    "status" varchar NOT NULL,
    "last_interaction_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "updated_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "created_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL
);
INSERT INTO "conversation_new"
    SELECT id, bot_id, channel_id, user_id, flow_id, step_id, status,
           last_interaction_at, updated_at, created_at
    FROM "conversation";
DROP TABLE "conversation";
ALTER TABLE "conversation_new" RENAME TO "conversation";

CREATE TABLE "memory_new" (
    "id" uuid_text NOT NULL PRIMARY KEY,
    "bot_id" varchar NOT NULL,
    "channel_id" varchar NOT NULL,
    "user_id" varchar NOT NULL,
    "key" varchar NOT NULL,
    "value" varchar NOT NULL,
    "created_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "updated_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL
);
INSERT INTO "memory_new"
    SELECT id, bot_id, channel_id, user_id, key, value, created_at, updated_at
    FROM "memory";
DROP TABLE "memory";
ALTER TABLE "memory_new" RENAME TO "memory";

CREATE TABLE "message_new" (
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
    FOREIGN KEY ("conversation_id") REFERENCES "conversation" ("id")
);
INSERT INTO "message_new"
    SELECT id, conversation_id, flow_id, step_id, direction, payload,
           content_type, message_order, interaction_order, created_at, updated_at
    FROM "message";
DROP TABLE "message";
ALTER TABLE "message_new" RENAME TO "message";

CREATE TABLE "state_new" (
    "id" uuid_text NOT NULL PRIMARY KEY,
    "bot_id" varchar NOT NULL,
    "channel_id" varchar NOT NULL,
    "user_id" varchar NOT NULL,
    "type" varchar NOT NULL,
    "key" varchar NOT NULL,
    "value" varchar NOT NULL,
    "created_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "updated_at" datetime_text DEFAULT CURRENT_TIMESTAMP NOT NULL
);
INSERT INTO "state_new"
    SELECT id, bot_id, channel_id, user_id, type, key, value, created_at, updated_at
    FROM "state";
DROP TABLE "state";
ALTER TABLE "state_new" RENAME TO "state";

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
