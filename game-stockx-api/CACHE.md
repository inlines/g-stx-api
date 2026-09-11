# Cache policy

Redis is a disposable cache, never a source of user-owned data. Reads distinguish hit, miss and error. Writes are best-effort. Each operation has a 250 ms deadline including checkout; a request can execute several operations. Multiplexed connections preserve protocol response ordering when a deadline cancels a future.

Catalogue keys use cache:v3:catalog:visibility; other keys use cache:v2, with a PostgreSQL catalog revision. Bulk IGDB imports increment catalog_cache_revision in the same transaction as changes. Name approvals increment catalog_name_revision (search) and products.cache_revision (that product) in their transaction. Read revisions before cache/DB data: in-flight requests using a previous revision cannot repopulate current keys. Serial approvals increment catalog_cache_revision too, refreshing the platform-specific has_serials catalogue flag. Old keys expire naturally.

TTLs: catalogue offset=0: 300s, other pages: 60s; basic game/company/franchise/platform data: 86400s. Releases/sellers/screenshots and personal/social data are not newly cached. Manual catalogue SQL must increment catalog_cache_revision in the same transaction. This policy requires the cache_revisions migration and matching IGDB export.cjs/cron.sh; do not deploy backend alone.

Metrics: app_cache_reads_total{cache,result}, app_cache_writes_total{cache,result}, app_cache_errors_total{cache,operation,reason}, app_cache_operation_duration_seconds. Labels are bounded and contain no user IDs/search strings. Hit ratio = hit/(hit+miss); errors are a separate share of all reads. Zero traffic has no ratio. Backend restart resets these counters; use rate/increase.

Run cargo test --locked and tests/admin_contract.py against the built executable for cache/administration regressions. That integration suite creates disposable PostgreSQL/Redis containers; it does not touch application data.

Manual features/load.sh import updates ratings, related IDs and per-game/per-platform multiplayer modes, then advances catalog_cache_revision atomically. Catalogue keys include both multiplayer filters and the visibility switches; basic details use a features namespace to avoid decoding old DTOs. Multiplayer and similar-game sections on details are read from PostgreSQL. The scheduled IGDB cron is unchanged.

## Catalogue visibility

`include_unreleased` defaults to false: `products.first_release_date IS NULL` is excluded regardless of search, platform or digital filter. True includes those games subject to the other filters. A future non-null date is not treated as missing; this switch follows the agreed missing-date definition.

With `ignore_digital=true`, the platform entry must not be marked `digital_only`, and browsing requires a non-blank serial on a non-digital release of that same platform. Unknown physical availability is allowed only by a non-empty literal name/alternative-name search. Percent, underscore and backslash are escaped in ILIKE. Bundle membership, parent relationships and IGDB types cannot disprove an independently issued physical edition; confirmed serials are not discarded by the old type exclusion. With the digital filter off, the legacy type exclusions remain for unknown editions when browsing; confirmed serials or text search allow those game types too.

`digital_only=false` alone is not evidence of a physical edition. Serials are existing curated evidence, not an independent authenticity check. No IGDB data or user contributions are rewritten. Serial approvals already advance the catalogue revision; the newly confirmed edition consequently appears without waiting for TTL. The date switch is included in cache keys. The `catalog_physical_lookup` migration indexes the non-digital release lookup and is applied by the usual deployment migrations.
