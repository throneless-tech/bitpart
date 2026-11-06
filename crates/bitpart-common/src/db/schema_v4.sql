-- Bitpart schema, version 4. Do not edit in place; add a new migration
-- in `bitpart_common::db::migration` and bump the version.

ALTER TABLE "bot" ADD COLUMN "expire_timer" integer;
