-- This file should undo anything in `up.sql`
DROP TABLE IF EXISTS covers CASCADE;
DROP TABLE IF EXISTS products CASCADE;
DROP TABLE IF EXISTS releases CASCADE;
DROP TABLE IF EXISTS platforms CASCADE;
DROP TABLE IF EXISTS regions CASCADE;
DROP TABLE IF EXISTS users_have_releases;
DROP TABLE IF EXISTS screenshots;
DROP TABLE IF EXISTS product_platforms;
DROP TABLE IF EXISTS users_have_wishes;
DROP TABLE IF EXISTS users_have_bids;
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS users;