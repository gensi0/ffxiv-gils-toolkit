-- Données du client gardées sur le serveur (liste de courses, préférences…),
-- pour les retrouver sur tous les appareils.
CREATE TABLE kv (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
