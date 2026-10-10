# AUDIT-APPLICATION — gis

**Slice:** brutal-audit rollout 3/8. **Date:** 2026-10-10. **Repo:** ~/workspace/gis.
**Audited at:** `5840a9d`. **This pass:** `cc69b0f`.

## What this is

Rust workspace: `geo_core` (coordinates, tracks, ingest validators), `gis_redteam`
(adversarial battery against the ingest boundary), `offline_packs` (.gispack
offline map packs with blake2b manifests), plus coombs/saltspring/route/
map_export/ortho/poi and others. Threat model (REDTEAM.md): spoofed GPS tracks,
forged map features, tampered offline packs, poisoned mirror data.

## Per-finding application

- **R-1 (instance-only fix claims, one book):** APPLIED. Rigor-scope section
  appended to `crates/gis_docs/docs/REDTEAM.md`: the battery replays 10 fixed
  attack shapes against the same validators production ingest uses; every
  "attack rejected" result is `class_scope: instance-only`; no dedicated
  adversarial re-attack wave has signed off class closure; bypasses are
  mirrored in REDTEAM.md as `SUPERSEDED-BY-BYPASS` (one book).
- **R-2 (honest guardrail bounds):** APPLIED. Catch/can't-catch table added to
  REDTEAM.md for all 7 defenses: NaN/inf rejection (malformed input only);
  region containment (can't catch in-region lies); teleport detection at the
  25 m/s pedicab default (can't catch sub-threshold drift; the threshold is a
  tuning constant, not a measured adversary bound); timestamp monotonicity
  (can't catch consistent fabricated timelines); pack manifest digests (can't
  catch whole-pack rewrite — manifest unsigned); CAS verify (can't catch
  wrong-from-the-start digests); mirror log (can't catch wrong-but-well-formed
  mirror data).
- **R-3 (out-of-band head copy):** N/A. No hash-chained log in-repo; CAS
  digests are self-verifying (the digest IS the address).
- **R-4 (backlog triage):** N/A. No defect backlog in-repo; no standing HIGHs.
- **R-5 (signature games):** N/A. No signature/denylist/strip security games.
- **R-6 (detector + consumer + gate):** satisfied, no gap. The validator is
  consumed: `coombs::validate_all()` gates place data (Reject verdicts become
  errors); the `gis_redteam` battery runs the production validators
  (`geo_core::validate`). Detector, consumer, and gate live in the same system.
- **R-7 (property vs change-detection):** APPLIED. `gis_redteam` tests labeled
  PROPERTY (adversarial invariants: all attacks rejected, honest inputs
  accepted); `geo_core::validate` tests labeled PROPERTY (ingest-boundary
  invariants); `battery_covers_ten_cases` labeled a change-detection guard
  (prevents silent battery shrinkage). Policy: property expectations are never
  rewritten to match a behavior change without adversarial sign-off.
- **R-8 (unsigned provenance):** APPLIED. Fixed an overclaim in
  `crates/offline_packs/src/lib.rs`: the docstring said "Tamper-evident via
  the manifest" — now "INTEGRITY, NOT TAMPER-EVIDENCE". Digests detect
  accidental corruption and tampering by a party who can't rewrite the
  manifest; the manifest is UNSIGNED, so a whole-pack rewrite is silent; pack
  publisher authenticity is UNKNOWN; verification happens only on the read path.
- **R-9 (frozen claim schema):** N/A. No claim/falsification log in-repo.
- **R-10 (record-keeping in the same motion):** APPLIED. REDTEAM.md rigor
  section and the `offline_packs` docstring fix written in this motion,
  2026-10-10.
- **R-11:** explicit UNKNOWN list below.
- **R-12 (no unsourced footnotes):** APPLIED. None found.

## Commits

This pass (`TBD`): crates/gis_docs/docs/REDTEAM.md,
crates/gis_redteam/src/lib.rs, crates/geo_core/src/validate.rs,
crates/offline_packs/src/lib.rs, this file.

## Test results

Measured 2026-10-10, all green: `geo_core` 26/26, `gis_redteam` 3/3,
`offline_packs` 4/4, e2e 1/1.

## Wont-fix list (R-4)

None — no backlog, no standing HIGHs.

## Explicit UNKNOWN list (R-11)

1. Teleport threshold 25 m/s is a tuning constant, not a measured adversary
   bound.
2. Pack publisher authenticity — manifests are unsigned.
3. Mirror data correctness beyond well-formedness.
4. Untracked dirs `crates/gis-organelles/` and `organelles/` — contents not
   reviewed this pass.

## Top critical issues

1. **Unsigned pack manifests** — no publisher authenticity; a whole-pack
   rewrite (tiles + manifest) is silent. Integrity against the manifest only.
2. **Teleport threshold unmeasured** — 25 m/s is a tuning constant, not an
   adversary-derived bound; sub-threshold drift is uncatchable by design.
3. **In-region lies uncatchable** — wrong-but-plausible coordinates inside the
   bbox pass every defense.
4. **Untracked organelle dirs unreviewed** — `crates/gis-organelles/` and
   `organelles/` were not examined this pass.
