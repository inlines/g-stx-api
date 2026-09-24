-- Manual overrides survive IGDB refreshes of products and product_platforms.
CREATE TABLE IF NOT EXISTS product_platform_type_overrides (
 product_id integer NOT NULL REFERENCES products(id) ON DELETE CASCADE,
 platform_id integer NOT NULL REFERENCES platforms(id) ON DELETE CASCADE,
 game_type integer NOT NULL CHECK(game_type BETWEEN 0 AND 14),
 reason text NOT NULL,
 PRIMARY KEY(product_id,platform_id)
);
CREATE OR REPLACE FUNCTION effective_game_type(game integer, platform integer, fallback integer)
RETURNS integer LANGUAGE sql STABLE AS $$
 SELECT coalesce((SELECT o.game_type FROM public.product_platform_type_overrides o
 WHERE o.product_id=game AND o.platform_id=platform),fallback)
$$;
