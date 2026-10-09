# GIS domain architecture (canon)

One Rust workspace (`~/workspace/gis`), one crate per scaffold item, std-only
(no external crates — crates.io is unreachable from the build VM, so every
algorithm is implemented against std; builds are fully deterministic).

```text
geo_core        shared primitives: coord, tile, raster(PNG), geojson,
                track, validate, organelle_shim
saltspring      gis-001 Saltspring Slasher core: dataset + Leaflet site gen
saltspring_data gis-002 ingest -> change-detect -> versioned snapshots
coombs          gis-003 Coombs/Parksville core + offline manifest
coombs_data     gis-004 parcel fabric (synthetic sample) + imagery manifest
palontour       gis-005 blake2b CAS + tileset mapping engine
repomix_mine    gis-006 repomix pack miner -> restored PALANTOUR inventory
ortho           gis-007 drone frames (PPM) -> mosaic -> tile pyramid (PNG)
offline_packs   gis-009 .gispack tar packs with blake2b tile manifests
poi             gis-011 POI database (JSON persistence, user-addable)
route           gis-012 pedicab route planner (Dijkstra over waypoint graph)
map_export      gis-013 SVG renderer + PDF writer (FlateDecode, xref-valid)
gis_redteam     gis-014 spoof-rejection battery (reuses geo_core::validate)
client_template gis-015 reusable client GIS crate generator
open_data       gis-016 public-dataset mirror manifest + curl fetcher
gis_api         gis-017 keyed REST over std TcpListener (JSON)
```

Data flow: `open_data` mirrors -> `saltspring_data`/`coombs_data` ingest ->
`ortho` tiles -> `palontour` CAS -> `offline_packs` -> `gis_api` serves.
`gis_redteam` guards every ingest boundary. `route` reads `poi`.

Blocked by other domains (recorded, not worked around):
- gis-008 needs org-002's organelle host interface. `geo_core::organelle_shim`
  defines the exact op surface so registration is mechanical later.
- gis-010 needs beat-006's lifelog GPS feed. `geo_core::track` already
  accepts any TrackPoint source; overlay pipeline is built and tested.
