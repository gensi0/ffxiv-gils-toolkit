-- Détail des annonces à acheter pour chaque opportunité (prix max et lots en JSON).
ALTER TABLE opportunities ADD COLUMN max_unit_price INTEGER;
ALTER TABLE opportunities ADD COLUMN lots TEXT;
