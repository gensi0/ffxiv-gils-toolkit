-- Données envoyées par le plugin Dalamud.

-- Dernier état connu : personnage, inventaire, servants, niveaux…
CREATE TABLE plugin_state (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

-- Événements déjà traités (ventes, achats) : un renvoi n'est pas compté deux fois.
CREATE TABLE plugin_events (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  at INTEGER NOT NULL,
  payload TEXT NOT NULL,
  received_at INTEGER NOT NULL
);
