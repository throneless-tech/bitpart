-- Bitpart schema, version 5. Do not edit in place; add a new migration
-- in `bitpart_common::db::migration` and bump the version.

ALTER TABLE "signal_groups" ADD COLUMN "group_ref" varchar;
CREATE INDEX "signal_groups_group_ref" ON "signal_groups" ("channel_id", "group_ref");
