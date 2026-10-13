-- Mes ventes et journal des gains.

-- Achats réels (depuis la liste de courses ou saisis à la main) : stock FIFO.
CREATE TABLE stock_lots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  item_id INTEGER NOT NULL,
  hq INTEGER NOT NULL,
  quantity INTEGER NOT NULL,
  remaining INTEGER NOT NULL,
  -- Prix payé par unité, taxe d'achat comprise.
  unit_cost REAL NOT NULL,
  bought_at INTEGER NOT NULL,
  source_world TEXT
);
CREATE INDEX idx_stock_item ON stock_lots(item_id, hq, bought_at);

-- Objets dont on surveille les annonces en plus de ceux en stock.
CREATE TABLE tracked_items (
  item_id INTEGER PRIMARY KEY,
  added_at INTEGER NOT NULL
);

-- Annonces de mes servants vues sur Universalis.
CREATE TABLE my_listings (
  listing_id TEXT PRIMARY KEY,
  item_id INTEGER NOT NULL,
  hq INTEGER NOT NULL,
  price INTEGER NOT NULL,
  quantity INTEGER NOT NULL,
  retainer TEXT NOT NULL,
  first_seen INTEGER NOT NULL,
  last_seen INTEGER NOT NULL,
  -- active | sold | removed | pending (disparue sans vente correspondante)
  status TEXT NOT NULL DEFAULT 'active',
  closed_at INTEGER,
  -- Nouveau prix saisi dans l'app après une modification en jeu.
  expected_price INTEGER,
  -- Prix concurrent le plus bas quand on est passé devant nous.
  undercut_price INTEGER,
  undercut_notified_price INTEGER
);
CREATE INDEX idx_my_listings_status ON my_listings(status, item_id);

-- Ventes (détectées ou saisies) et leur coût d'achat FIFO.
CREATE TABLE sales_journal (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  item_id INTEGER NOT NULL,
  hq INTEGER NOT NULL,
  quantity INTEGER NOT NULL,
  unit_price INTEGER NOT NULL,
  -- Taxe de vente appliquée (0.05 = 5 %).
  tax_rate REAL NOT NULL,
  sold_at INTEGER NOT NULL,
  -- Coût d'achat des unités couvertes par le stock, et leur nombre.
  cost REAL NOT NULL DEFAULT 0,
  cost_units INTEGER NOT NULL DEFAULT 0,
  listing_id TEXT,
  -- auto (détectée) | manual (confirmée ou saisie)
  source TEXT NOT NULL
);
CREATE INDEX idx_sales_time ON sales_journal(sold_at);
