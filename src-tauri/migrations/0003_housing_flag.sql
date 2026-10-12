-- Housing items (market board "Housing" tab, ItemSearchCategory.Category = 4)
-- are out of scope. Cached rows predate the flag, so drop them; they are
-- re-fetched from XIVAPI on demand.
ALTER TABLE items ADD COLUMN is_housing INTEGER NOT NULL DEFAULT 0;
DELETE FROM items;
