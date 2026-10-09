# Ingest spec (canon)

## Drone frame ingest (ortho)
- Format: binary PPM P6, maxval 255 (`geo_core::raster::Raster::read_ppm`).
- Convert camera output with any ppm tool, e.g. `convert in.jpg out.ppm`.
- Each frame needs: NW corner (WGS-84) + ground sample distance (m/px).
- Pipeline: `ortho::mosaic_frames(frames, z)` -> canvas at zoom z ->
  `ortho::build_pyramid(mosaic, z_min, z_max)` -> PNG tiles ->
  sealed into `palontour::ContentStore`, indexed by `palontour::Tileset`.

## Vector ingest (saltspring_data)
- Sources implement `Source::fetch() -> Vec<IngestRecord>` (canonical regen,
  directory drops of `*.geojson`).
- Change detection: FNV-1a fingerprint per record (non-crypto; tamper-evidence
  is the CAS layer's job). Only new/changed records are snapshotted.
- Snapshots: `data/v{n}/{id}.geojson` + `manifest.json`; `state.json` holds
  last-seen fingerprints.

## Validation (all ingest)
Every record passes `geo_core::validate::Region::check_point` /
`check_track`: NaN/inf rejected, outside-region rejected, impossible segment
speed rejected (teleport spoof), non-monotonic timestamps rejected
(replay spoof). See REDTEAM.md.
