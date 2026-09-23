-- Bitpart schema, version 6. Do not edit in place; add a new migration
-- in `bitpart_common::db::migration` and bump the version.

UPDATE "conversation" SET "channel_id" = (
    SELECT c."id" FROM "channel" c
    WHERE c."bot_id" = "conversation"."bot_id"
    ORDER BY c."channel_id" = 'signal' DESC
    LIMIT 1
)
WHERE "channel_id" = 'signal'
  AND (
    EXISTS (SELECT 1 FROM "channel" c WHERE c."bot_id" = "conversation"."bot_id" AND c."channel_id" = 'signal')
    OR (SELECT COUNT(*) FROM "channel" c WHERE c."bot_id" = "conversation"."bot_id") = 1
  );

UPDATE "memory" SET "channel_id" = (
    SELECT c."id" FROM "channel" c
    WHERE c."bot_id" = "memory"."bot_id"
    ORDER BY c."channel_id" = 'signal' DESC
    LIMIT 1
)
WHERE "channel_id" = 'signal'
  AND (
    EXISTS (SELECT 1 FROM "channel" c WHERE c."bot_id" = "memory"."bot_id" AND c."channel_id" = 'signal')
    OR (SELECT COUNT(*) FROM "channel" c WHERE c."bot_id" = "memory"."bot_id") = 1
  );

UPDATE "state" SET "channel_id" = (
    SELECT c."id" FROM "channel" c
    WHERE c."bot_id" = "state"."bot_id"
    ORDER BY c."channel_id" = 'signal' DESC
    LIMIT 1
)
WHERE "channel_id" = 'signal'
  AND (
    EXISTS (SELECT 1 FROM "channel" c WHERE c."bot_id" = "state"."bot_id" AND c."channel_id" = 'signal')
    OR (SELECT COUNT(*) FROM "channel" c WHERE c."bot_id" = "state"."bot_id") = 1
  );
