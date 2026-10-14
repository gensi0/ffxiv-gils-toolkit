-- Rang de mes annonces face à la concurrence (même qualité).
ALTER TABLE my_listings ADD COLUMN rank INTEGER;
ALTER TABLE my_listings ADD COLUMN listing_count INTEGER;
-- Prix le plus bas des autres vendeurs, même qualité.
ALTER TABLE my_listings ADD COLUMN cheapest_other INTEGER;
