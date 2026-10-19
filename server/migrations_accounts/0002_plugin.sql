-- Jetons du plugin Dalamud (seule l'empreinte SHA-256 est gardée).
CREATE TABLE plugin_tokens (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  token_hash TEXT NOT NULL UNIQUE,
  user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  label TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  last_used_at INTEGER
);
CREATE INDEX idx_plugin_tokens_user ON plugin_tokens(user_id);
