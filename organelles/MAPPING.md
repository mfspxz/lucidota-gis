# gis — organelle mapping decisions

Decomposition reviewed 2026-10-10 under the corrected doctrine
(decompose ACTUAL capabilities; map-or-design per unit, justified).

## What the code actually does

`gis` is the CuteCity GIS backend: a geometry toolkit (`geo_core`), a
POI database (`poi`), a pedicab route planner (`route`), plus dataset and
packaging crates. The three compute crates are genuinely separable
capabilities: pure geodesy, a stateful POI store, and a stateful routing
graph. (`geo_core` even anticipated this: `organelle_shim.rs` pre-named
pure geo ops for a future host — this migration replaces the shim's
stand-in registry with real host registration.)

## Capability units → organelles

| Unit | Source | Organelle / ops | Decision |
|---|---|---|---|
| Pure geodesy: haversine, bearing, destination, Web-Mercator tile math, projection conversions | `geo_core` (coord.rs: LatLon/WebMercator) | **gis-geo** (`ge`): `haversine`, `bearing`, `destination`, `tile_xy`, `tile_bbox`, `to_meters`, `to_latlon`. All ReadOnly/Proven. | **Bespoke** (`ge`). Justification: `ref-geo-point` is a single-point primitive; this is a geodesy toolkit (tile pyramids, rhumb-line destination, projection roundtrips). Adopting the ref would cover one op and leave six reinvented. The unit's identity is the toolkit. |
| POI database: in-memory DB + JSON-file persistence, radius/kind queries, seeded canonical datasets | `poi` (PoiDb: add/remove/get/near/by_kind/save/load/seed_*) | **gis-poi** (`gi`): `add`, `remove` (Write, `gi.write`), `get`, `near`, `by_kind`, `user_added` (ReadOnly), `seed_saltspring`, `seed_coombs` (Write, `gi.write`), `save`, `load` (Write `gi.write` / ReadOnly). | **Bespoke** (`gi`). Justification: a geo POI store with radius queries and seeded canonical datasets is not `ref-kv-store` (no geo queries) and not `ref-geo-point` (a store, not a point). |
| Pedicab route planner: waypoint graph, Dijkstra with pedicab grade profile | `route` (Graph: add_node/add_leg/shortest/path_distance_m) | **gis-route** (`gj`): `add_node`, `add_leg` (Write, `gj.write`), `shortest`, `path_distance` (ReadOnly). | **Bespoke** (`gj`). Justification: no ref organelle routes. The pedicab profile (grade-penalized Dijkstra) is domain logic. |

## Deliberate exclusions

- `gis_api` (HTTP API server): a server, not an organelle — it would be a
  transport surface over these organelles, not a capability unit itself.
- `saltspring`, `saltspring_data`, `coombs`, `coombs_data`: canonical
  datasets (data, not capabilities) — surfaced via the poi seed ops.
- `ortho`, `palontour` (imagery/contour tilesets), `map_export`,
  `offline_packs`, `open_data`: dataset/packaging pipelines, not
  independently callable capabilities in this environment.
- `repomix_mine`, `client_template`, `gis_docs`, `gis_redteam`:
  docs/templates/redteam harness.
- `geo_core/src/organelle_shim.rs`: superseded by this migration — the shim
  itself says "when org-002 lands, this module is deleted". Left in place
  (archive-don't-delete); the real registration now lives here. Its op names
  (`geo.haversine_m` etc.) informed the `ge` op design.

## Reference-organelle check (all 12 considered)

- `ref-geo-point`: single-point primitive; `ge` is a six-op toolkit, `gi`
  is a store, `gj` routes. Adopting it would cover ~1 of 20 ops.
- `ref-kv-store`: blind KV; the POI store has radius queries, kinds, and
  seeded datasets. Rejected.
- `ref-file-cas`: POI persistence is JSON-file, not content-addressed.
  Rejected.
- Others: no overlap.
- Verdict: genuinely bespoke. Nothing reinvented that already existed.

## Execution facts

- gis-geo: Proven (pure geodesy). No fs, no network, no secrets.
- gis-poi: Candidate (stateful). Owns `Mutex<PoiDb>`; `save`/`load` touch
  the fs allowlist (one JSON file per call). Suspend = no-op, kill = drop
  (in-memory; `save` is the durability op).
- gis-route: Candidate (stateful graph). Owns `Mutex<Graph>`.
  Suspend = no-op, kill = drop.
- Envelopes: 1 cpu / 256 MB / 30 s each.
- Receipts: content-addressed sha256, chained; `verify_chain` green in tests.

## Codon collision check (2026-10-11)

`ge`, `gi` clear. `gj` (gis-route): renamed from `gr` — `gr` is owned by
park-send-wreck's `pw-grid` (committed 2026-10-10 21:44 UTC, earlier than
gis's 23:41 UTC); earlier commit wins. `gj` verified free workspace-wide.
No remaining collisions.
