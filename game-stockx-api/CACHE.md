# Cache policy

Redis is a disposable cache, never a source of user-owned data. Reads distinguish hit, miss and error. Writes are best-effort. Each operation has a 250 ms deadline including checkout; a request can execute several operations. Multiplexed connections preserve protocol response ordering when a deadline cancels a future.

Keys use cache:v2 with a PostgreSQL catalog revision. Bulk IGDB imports increment catalog_cache_revision in the same transaction as changes. Name approvals increment catalog_name_revision (search) and products.cache_revision (that product) in their transaction. Read revisions before cache/DB data: in-flight requests using a previous revision cannot repopulate current keys. Serial approvals increment catalog_cache_revision too, refreshing the platform-specific has_serials catalogue flag. Old keys expire naturally.

TTLs: catalogue offset=0: 300s, other pages: 60s; basic game/company/franchise/platform data: 86400s. Releases/sellers/screenshots and personal/social data are not newly cached. Manual catalogue SQL must increment catalog_cache_revision in the same transaction. This policy requires the cache_revisions migration and matching IGDB export.cjs/cron.sh; do not deploy backend alone.

Metrics: app_cache_reads_total{cache,result}, app_cache_writes_total{cache,result}, app_cache_errors_total{cache,operation,reason}, app_cache_operation_duration_seconds. Labels are bounded and contain no user IDs/search strings. Hit ratio = hit/(hit+miss); errors are a separate share of all reads. Zero traffic has no ratio. Backend restart resets these counters; use rate/increase.

Run cargo test --locked and tests/admin_contract.py against the built executable for cache/administration regressions. That integration suite creates disposable PostgreSQL/Redis containers; it does not touch application data.
