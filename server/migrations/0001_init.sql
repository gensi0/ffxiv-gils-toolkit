CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE items (
  item_id INTEGER PRIMARY KEY,
  name_en TEXT NOT NULL,
  name_fr TEXT,
  name_de TEXT,
  name_ja TEXT,
  icon_path TEXT,
  category TEXT,
  item_kind TEXT,
  stack_size INTEGER NOT NULL DEFAULT 1,
  level_item INTEGER,
  can_be_hq INTEGER NOT NULL DEFAULT 0,
  is_marketable INTEGER NOT NULL DEFAULT 1,
  fetched_at INTEGER NOT NULL
);
CREATE INDEX idx_items_name_en ON items(name_en);
CREATE INDEX idx_items_name_fr ON items(name_fr);

CREATE TABLE worlds (
  world_id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  dc_name TEXT NOT NULL,
  region TEXT NOT NULL
);

CREATE TABLE price_snapshot (
  item_id INTEGER NOT NULL,
  world_id INTEGER NOT NULL,
  quality TEXT NOT NULL CHECK(quality IN ('NQ','HQ')),
  min_price INTEGER,
  listings_count INTEGER,
  units_listed INTEGER,
  sale_velocity REAL,
  last_upload_unix INTEGER,
  fetched_at INTEGER NOT NULL,
  PRIMARY KEY (item_id, world_id, quality)
);
CREATE INDEX idx_snap_item ON price_snapshot(item_id);

CREATE TABLE sales_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  item_id INTEGER NOT NULL,
  world_id INTEGER NOT NULL,
  sold_at_unix INTEGER NOT NULL,
  quality TEXT NOT NULL CHECK(quality IN ('NQ','HQ')),
  price_per_unit INTEGER NOT NULL,
  quantity INTEGER NOT NULL,
  buyer_hash TEXT,           -- hash of buyerName, for dedup only
  UNIQUE (item_id, world_id, sold_at_unix, quality, price_per_unit,
          quantity, buyer_hash)
);
CREATE INDEX idx_history_item_world ON sales_history(item_id, world_id, sold_at_unix);

CREATE TABLE opportunities (
  scan_id TEXT NOT NULL,
  item_id INTEGER NOT NULL,
  quality TEXT NOT NULL CHECK(quality IN ('NQ','HQ')),
  source_world_id INTEGER NOT NULL,
  source_unit_price INTEGER NOT NULL,
  units_to_buy INTEGER NOT NULL,
  source_cost INTEGER NOT NULL,
  expected_sell_price INTEGER NOT NULL,
  home_median_price INTEGER NOT NULL,
  home_units_per_day REAL NOT NULL,
  days_of_supply REAL,
  unit_profit INTEGER NOT NULL,
  profit_per_trip INTEGER NOT NULL,
  daily_profit INTEGER NOT NULL,
  confidence TEXT NOT NULL,
  source_upload_unix INTEGER,
  computed_at INTEGER NOT NULL,
  PRIMARY KEY (scan_id, item_id, quality, source_world_id)
);
CREATE INDEX idx_opp_scan_daily ON opportunities(scan_id, daily_profit DESC);

CREATE TABLE watchlist (
  item_id INTEGER PRIMARY KEY,
  added_at INTEGER NOT NULL,
  notify_above_profit INTEGER
);

CREATE TABLE scan_runs (
  scan_id TEXT PRIMARY KEY,
  started_at INTEGER NOT NULL,
  completed_at INTEGER,
  items_scanned INTEGER,
  items_failed INTEGER,
  opportunities_found INTEGER,
  status TEXT NOT NULL CHECK(status IN ('running','done','failed','cancelled'))
);
