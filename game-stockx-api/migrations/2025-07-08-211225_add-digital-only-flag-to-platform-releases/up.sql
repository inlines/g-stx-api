ALTER TABLE product_platforms ADD COLUMN digital_only BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE releases ADD COLUMN serial text[] DEFAULT NULL;
