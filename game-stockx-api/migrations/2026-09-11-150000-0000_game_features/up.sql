ALTER TABLE products ADD COLUMN IF NOT EXISTS total_rating_count INTEGER;
ALTER TABLE products ADD COLUMN IF NOT EXISTS similar_game_ids INTEGER[] NOT NULL DEFAULT '{}';
CREATE TABLE IF NOT EXISTS product_multiplayer_modes (
 id INTEGER PRIMARY KEY,
 game INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
 platform INTEGER,
 offlinemax INTEGER,
 onlinemax INTEGER,
 offlinecoopmax INTEGER,
 onlinecoopmax INTEGER,
 offlinecoop BOOLEAN,
 onlinecoop BOOLEAN,
 splitscreen BOOLEAN,
 splitscreenonline BOOLEAN,
 lancoop BOOLEAN,
 campaigncoop BOOLEAN,
 dropin BOOLEAN
);
CREATE INDEX IF NOT EXISTS idx_multiplayer_game_platform ON product_multiplayer_modes(game,platform);
