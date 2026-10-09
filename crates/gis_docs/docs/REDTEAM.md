# GIS red-team notes (canon, gis-014)

Threat model: spoofed GPS tracks, forged map features, tampered offline packs,
poisoned mirror data.

Defenses (all tested in `gis_redteam` + `geo_core::validate`):
1. NaN/inf coordinates rejected at parse boundary.
2. Region containment: points outside the declared region rejected.
3. Teleport detection: segment speed > profile max (pedicab 25 m/s default)
   rejected as spoof.
4. Timestamp monotonicity: replays/time-travel rejected.
5. Pack tamper: blake2b-256 manifest digests recomputed on read.
6. CAS verify: bytes must hash to claimed digest on ingest.
7. Mirror log: every mirrored file logged with digest + timestamp; failed
   fetches recorded as pending, never silently zero-filled.

Red-team battery asserts each attack is REJECTED and each honest input is
ACCEPTED (no over-correction).
