# Collection copies and paged libraries

The owned record (`users_have_releases`, unique user/release pair) now stores
`selected_serial TEXT NULL` and `cib BOOLEAN NULL`. These fields never modify a
catalogue release. Null CIB means not specified; false means explicitly incomplete.
Migration `2026-09-23-140000-0000_collection_copy_details` preserves legacy positive
sale CIB flags. Legacy false flags were defaults and are not evidence of incompleteness.
Existing purchase prices and catalogue serial arrays are preserved.

`POST /api/collection-copy` accepts `{release_id, selected_serial, cib}`. This replaces
both metadata fields, including explicit null resets. Authentication determines the
owner; no login can be supplied. A serial must match a code on the exact release
(case and formatting separators may differ). Unknown/unowned copies return 404,
invalid serials return 400 without changing CIB or sale data. Updating copy CIB
synchronizes a current sale flag. The legacy sale mutation also updates copy CIB
when the request explicitly specifies it.

`GET /api/library/{collection|wishlist|wts}` accepts `cat`, `regions` (europe,
america, japan, other), `query`, `search_mode` (name/serial), `sort`
(name/date/price/rating), `limit` (1..1000), `offset`. Filters and ordering apply
before LIMIT/OFFSET, with release ID as a stable final tie-breaker. Dates belong to
the owned/wanted release. Region selection includes worldwide records, as before.
The response includes `items`, `total_count`, `unfiltered_total`, `platform_ids`,
and `owned_regions` independent of the selected page. Multiplayer is platform-scoped.

An optional `login` selects someone else's collection or sales; their wishlist is
not exposed. Purchase prices stay private (`price` and `purchase_price` are null
in a public collection; sale prices remain public in WTS). All metadata mutations
are restricted to the authenticated owner's copy. Existing list endpoints remain
available for older clients.

## Deployment

Back up the database first using the existing backup procedure. Deploy the API
with its migrations **before** the new frontend: the frontend uses `/api/library/*`
and `/api/collection-copy`. The standard API migration runner must include the new
migration directory. Then deploy the frontend normally. No IGDB cron changes.
Rolling back only the frontend is safe; rolling back the migration drops new copy
metadata, so prefer leaving the additive columns in place if reverting API code.

## Validation

`cargo check --offline` / `cargo build --offline`.
`python3 tests/library_contract.py` starts disposable PostgreSQL/Redis containers,
runs all migrations and exercises the real API. It checks migration backfill,
server pages and global filtering, copy ownership and exact release membership,
null/false/true CIB, separate prices, and private purchase prices. It removes only
its own test containers and never reads the project's .env file.
