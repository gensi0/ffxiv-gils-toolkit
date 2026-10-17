-- Localized market-board category names (items.category stays English).
ALTER TABLE items ADD COLUMN category_fr TEXT;
ALTER TABLE items ADD COLUMN category_de TEXT;
ALTER TABLE items ADD COLUMN category_ja TEXT;

-- Raw API responses with their fetch time; TTL is decided by the reader.
CREATE TABLE api_cache (
  url TEXT PRIMARY KEY,
  body TEXT NOT NULL,
  fetched_at INTEGER NOT NULL
);

-- Scan prefilter output (F4 step 5), one row per item x source quality x source DC.
CREATE TABLE scan_candidates (
  scan_id TEXT NOT NULL,
  item_id INTEGER NOT NULL,
  quality TEXT NOT NULL CHECK(quality IN ('NQ','HQ')),
  sold_as TEXT NOT NULL CHECK(sold_as IN ('NQ','HQ')),
  source_dc TEXT NOT NULL,
  source_world_id INTEGER NOT NULL,
  source_min_price INTEGER NOT NULL,
  est_sell_price INTEGER NOT NULL,
  est_profit INTEGER NOT NULL,
  est_score REAL NOT NULL,
  home_units_per_day REAL NOT NULL,
  source_upload_unix INTEGER,
  PRIMARY KEY (scan_id, item_id, quality, source_dc)
);
CREATE INDEX idx_cand_scan_score ON scan_candidates(scan_id, est_score DESC);

-- HQ units can be sold into NQ demand (F5); record which demand was used.
ALTER TABLE opportunities ADD COLUMN sold_as TEXT NOT NULL DEFAULT 'NQ';
ALTER TABLE opportunities ADD COLUMN is_stale INTEGER NOT NULL DEFAULT 0;

ALTER TABLE scan_runs ADD COLUMN error TEXT;
