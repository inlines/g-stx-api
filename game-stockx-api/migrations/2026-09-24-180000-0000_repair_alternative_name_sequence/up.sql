SELECT pg_advisory_xact_lock(724891003);
LOCK TABLE alternative_names IN SHARE ROW EXCLUSIVE MODE;
-- Never move the descending sequence backwards or overwrite existing aliases.
SELECT setval('local_alternative_name_id', LEAST(
 (SELECT last_value FROM local_alternative_name_id),
 COALESCE((SELECT min(id)::bigint FROM alternative_names WHERE id<0), -1)
), true);
