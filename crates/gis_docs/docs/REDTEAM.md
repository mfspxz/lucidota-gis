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

## Rigor scope (R-1/R-2/R-7 — 2026-10-10)

**R-1 (instance-only):** the battery replays 10 fixed attack shapes against
the same validators production ingest uses. Every "attack rejected" result is
`class_scope: instance-only` — it proves the validator rejects that exact
shape, not that the class is closed. No dedicated adversarial re-attack wave
has signed off class closure. A bypass of a battery case is mirrored here as
`SUPERSEDED-BY-BYPASS` against the original claim (one book), never filed only
elsewhere.

**R-2 (honest bounds):** what the defenses catch vs what they admit they can't.

| Defense | Catches | Admits it can't catch |
|---|---|---|
| NaN/inf rejection | malformed coordinates at parse boundary | nothing beyond malformed input |
| Region containment | points outside the declared region | an in-region lie (wrong-but-plausible coordinates inside the bbox) |
| Teleport detection (25 m/s pedicab default) | gross spoofs, replays with impossible speed | sub-threshold drift (a spoofed track that stays under the speed cap); the threshold is a tuning constant, not a measured adversary bound |
| Timestamp monotonicity | replays, time-travel | a consistent fabricated timeline (monotonic but false) |
| Pack manifest digests | accidental corruption; tile tampering without manifest rewrite | whole-pack rewrite (manifest is unsigned — see offline_packs) |
| CAS verify | bytes ≠ claimed digest on ingest | a digest that was wrong from the start (garbage in, addressed garbage) |
| Mirror log | silent zero-fills (failed fetches recorded as pending) | a mirror that serves wrong-but-well-formed data |

**R-7:** `gis_redteam` tests and `geo_core::validate` tests are PROPERTY
tests (adversarial invariants). `battery_covers_ten_cases` is a
change-detection guard. Property expectations are never rewritten to match a
behavior change without adversarial sign-off.
