# Homing Framework v0.1 — how every repo and program gets a geographic home

Seeded 2026-11-18. Companion to `cute-city-geo.json` and
`cute-city-triples.json`. This is a framework + first queue, not a finished
homing of the estate: the github-map sibling agent owns the repo inventory,
and every home below the seed records is still unassigned.

Scope note: Zack vetoed a literal city-map visualization for DIOGENES on
Oct 12, 2026 ("don't make a literal map of city" — he asked for the RFC
instead). This framework honours that: it is a DATA layer (GeoJSON +
triples + home records), not a literal-map artifact. No map visualization
is built here. Visualization is his call, later.

## 1. The idea in one paragraph

Same city, infinite maps: one physical block mapping, unlimited graph
layers as different edge sets over the same blocks. A repo, a program, a
tricycle, a lease, a person — each is a Thing, and each Thing gets a HOME:
a geographic anchor (or a stated absence of one), a network anchor, a
ledger anchor, and an access tier. Navigation is following the
cryptocurrent — hash-chained receipts flowing through streets, meats, and
sheets (see §5). Deliberate gaps are part of the map, not holes in it:
some Things are unmapped because they are unmeasured, some because they
are unproven, and some because they are private by design.

## 2. Home record schema (v0.1)

One JSON record per Thing. Stored wherever the homed Thing's graph lives;
the merge key is `thing_id`.

```json
{
  "thing_id": "cute_city",
  "thing_type": "Operator",
  "home_state": "seeded",
  "geographic": {
    "district": "coal_harbour",
    "district_state": "claimed",
    "geometry": null,
    "place_id": "harbour_green",
    "place_state": "claimed",
    "precision": "district-level, no surveyed coordinate for the operator itself"
  },
  "network": {
    "host": "cutecity.cc",
    "route": "apex",
    "state": "proven"
  },
  "ledger": {
    "receipt": "ledger seq or receipt envelope id, or null",
    "state": "unknown"
  },
  "access": "OPERATOR",
  "valid_from": "2026-11-18",
  "valid_to": null,
  "source": "homing-seed 2026-11-18"
}
```

Field rules:

- `home_state`: `homed` (all layers assigned with sources) | `seeded`
  (first assignment, claimed) | `unhomed` (no layer assigned yet) |
  `hidden` (deliberately unmapped — see §4).
- `geographic.geometry`: a GeoJSON geometry, or `null`. Null is a real
  state. Never a guessed coordinate, never a geocoded address passed off
  as surveyed. An address string lives in `geographic.address_string`,
  never in `geometry`.
- `geographic.place_id` may be set while `geometry` is null: homed to a
  PLACE is not the same as homed to a POINT. Harbour Green is a place
  before it is a point.
- `network.host` / `route`: where the Thing lives on the network
  (domain, workers.dev subdomain, repo URL). Proven only by a live probe
  or a repo record, never by assumption.
- `ledger.receipt`: the receipt that proves the homing. No receipt, no
  claim — the home stays `seeded` or `unhomed`.
- Every layer carries its own `state` (proven / claimed / unknown) and
  the record carries `valid_from` / `valid_to`. A home without a time is
  not timeless — its time is unknown.

## 3. Homing rules (load-bearing)

1. **Never invent a coordinate.** A coordinate comes from a surveyed or
   published place result, copied verbatim with its precision note. An
   address string is not a coordinate. A district name is not a
   coordinate. Geocoding without a stated source + precision is invention.
2. **Address string ≠ point.** A Thing with an address and no coordinate
   is `geometry: null, address_string: "..."`. That is a complete home
   record, not a failed one.
3. **District assignment needs a source.** A place belongs to a district
   when a source says so (contains triple, closest-to triple, or the
   run framing that names it). Otherwise district is null and the state
   is unknown.
4. **One Thing, many layers.** Geographic, network, and ledger homes are
   independent layers over the same Thing. A program can be network-homed
   (proven) and geographically unhomed (unknown) at the same time — that
   is the honest state of all 5 cutecity workers today.
5. **Homing is versioned + receipted.** A home changes only by a new
   record with a new `valid_from` and a receipt. Old homes are superseded,
   never deleted or rewritten.
6. **Hidden is a first-class home.** `home_state: hidden` means the
   Thing is deliberately unmapped. It is not missing, not a backlog item,
   and not to be "completed" by a future pass. See §4.

## 4. Access tiers — navigational aids vs deliberate gaps

| Tier | Who may navigate | What it holds | Examples in v1 |
|---|---|---|---|
| PUBLIC | anyone | Parks, BIA offices, stations, seawall, published venues | harbour_green, bia_downtown, bia_westend, 15 stations, seawall_route |
| COMMUNITY | named community, no tracking | Venues, shelters — address public, no live position, no encounter counts | 6 Granville venues, 8 shelters |
| OPERATOR | Cute City ops only | Base, storage, routes in progress, funding stack | tricycle_base (claimed), shop/storage target (claimed) |
| HIDDEN | need-to-know only (Zack + Lucidota) | Personal residences, mailing addresses, unannounced sites, key custody | 2 personal address-things (values withheld from every file in this folder by design) |

### Navigational aids that EXIST

- This GeoJSON layer: 9 located features, 5 distinct points, 1 schematic
  seawall line. Park-level precision, honestly labelled.
- Station proxies: shelter + crime proxy per station (inference, not
  measurement — labelled as such in every triple).
- GTFS-realtime (TransLink, key required): publishes vehicle positions.
  A future aid — the graph records the capability, not a live feed.
- City of Vancouver open data: publishes traffic stills, public, no
  login, small pull. A real, usable, boring aid.
- BIA offices + community policing centre as waypoints: real addresses,
  real humans, real letters of support.
- The OSINT location-acquisition workflow (10 steps, run #1 Harbour
  Green): the procedure that homes the NEXT place. The workflow IS a
  navigational aid — it tells you how to arrive, not just where.

### Deliberate gaps (named, with reason class)

| Gap | Reason class | Detail |
|---|---|---|
| 29 of 38 features unlocated | UNMEASURED | No surveyed coordinate exists in any source. Fill only with a surveyed/published result. |
| No district polygons | UNMEASURED | No boundary source exists. Districts are names, not geometries. |
| No heights (38/38 null) | UNMEASURED | No height in any source. Extrusion waits for real heights. |
| No live trike / vehicle / person positions | UNMEASURED + PRIVATE | No live feed exists; live person positions would be HIDDEN anyway. |
| No per-station encounter counts | UNMEASURED | No public data exists. The graph records the absence (unknown triples), not a zero. |
| No vendor→venue links (Patronscan etc.) | UNPROVEN | Capability is not deployment. 5 venues unknown, 1 untested. Never force a link. |
| No lease execution proof (Pandey) | UNPROVEN | Authorised 2026-07-27; execution not proven in public record. |
| No Fry / Orr endorsement | UNPROVEN | Claimed, not in writing. |
| Personal addresses absent | PRIVATE-BY-DESIGN | HIDDEN tier. Withheld from every file here. Not a backlog item. |
| Key custody locations absent | PRIVATE-BY-DESIGN | Secrets live in Vault / Zack's custody. Never mapped. |
| Unannounced future sites absent | PRIVATE-BY-DESIGN | A site Zack has not named does not get a home record until he names it. |

Rule: a gap is closed only by the evidence its class names. An UNMEASURED
gap closes with a measurement. An UNPROVEN gap closes with proof. A
PRIVATE-BY-DESIGN gap does not close — it is the map working as intended.

## 5. The cryptocurrent journey — streets, meats, sheets

Three layers over the same blocks. Same city, infinite maps — these are
three of the maps, and the current runs through all of them.

- **STREETS** — the physical layer. Parks, seawall, stations, roads,
  the trike base, the routes. Currency: movement — rides, routes, feet,
  wheels. What the GeoJSON in this folder plots. Navigation here is
  literal: how do you get from the base to the venue, from the station
  to the shelter, from the seawall to the shop.
- **MEATS** — the living layer. People, venues, food, bodies, shelters,
  beds, meals, the fleshy city that streets serve. Currency: presence
  and care — who was there, who ate, who slept, who got helped.
  Navigation here is social: who do you know, who trusts you, who feeds
  you. This layer is why COMMUNITY tier exists: the meats are real
  people, and real people are not tracked.
- **SHEETS** — the paper layer. Ledgers, receipts, spreadsheets, leases,
  grants, the audit trail. Currency: proof — hash-chained receipts that
  say a ride happened, a meal was served, a lease was signed. Navigation
  here is evidentiary: can you prove you were there, can you prove it
  happened, can you prove it twice.

**The cryptocurrent** is the receipt flow that binds the three: every
ride (streets) touches a person or venue (meats) and lands a receipt
(sheets). A journey through the city is a journey along the current —
street → meat → sheet, then back again. A place with no current is a
place with no proof: unmapped, unproven, or private. The current does
not run everywhere on purpose: it stops at the HIDDEN tier, it stops
where nothing is measured, and it never runs on an invented number.

Concretely, for Cute City v1, the current is thin but real: the base is
claimed at Harbour Green, the workers are proven on the network, the
receipts live in the ledger and the proof-ledger envelopes. Every future
homing thickens the current by one receipt at a time.

## 6. How a repo / program gets homed (procedure)

1. **Identify.** Name the Thing, its type (Program / Worker / Crate /
   Script / Repo), and its source (repo URL, workspace path, or worker
   name). No Thing, no home.
2. **Classify the tier.** PUBLIC / COMMUNITY / OPERATOR / HIDDEN.
   Default for code is OPERATOR unless the repo is public (then PUBLIC
   for the repo record — never for its secrets, keys, or deploy targets,
   which are HIDDEN).
3. **Assign layers.** Geographic (place → district → geometry, each
   with state + source, nulls allowed), network (host + route, proven
   by probe or repo record), ledger (receipt id or null).
4. **Receipt it.** Write the home record, sign it, append the receipt.
   A home without a receipt is `seeded` at best.
5. **Publish or hide.** PUBLIC/COMMUNITY homes go in the shared GeoJSON
   + triples. OPERATOR homes go in the operator graph. HIDDEN homes are
   recorded as ids only — no geometry, no address, no network route.

Merge contract with the github-map lane: the github lane owns the repo
inventory (names, visibility, size, remotes, flags); this framework owns
the home schema + tiers. Merge key is the program/repo id. The github
lane MUST NOT invent geographic homes for repos — a repo's geographic
home is unknown until a homing record says otherwise. A repo's network
home is its remote URL (proven by the repo record).

## 7. First homing queue (26 programs, all geographically UNHOMED)

Seed state, 2026-11-18. Network home known only where a probe proved it.

| Program | Kind | Network home | Geographic home | Tier |
|---|---|---|---|---|
| cute_city_worker | worker | cutecity.cc (proven, 2026-10-07 probe) | unknown | OPERATOR |
| tricycles_worker | worker | workers.dev (proven) | unknown | OPERATOR |
| ctcc_router | worker | workers.dev (proven) | unknown | OPERATOR |
| ai_router | worker | workers.dev (proven) | unknown | OPERATOR |
| lucidota_exchange | worker | workers.dev (proven) | unknown | OPERATOR |
| cutecity-a11y-audit | organelle check | unknown | unknown | OPERATOR |
| cutecity-launch-check | organelle check | unknown | unknown | OPERATOR |
| cutecity-tipjar-check | organelle check | unknown (Solana RPC) | unknown | OPERATOR |
| cutecity-uptime-check | organelle check | unknown (probes cutecity.cc) | unknown | OPERATOR |
| cutecity-voice-pass | organelle check | unknown | unknown | OPERATOR |
| cute-backup | rust crate | unknown | unknown | OPERATOR |
| cute-cms | rust crate | unknown | unknown | OPERATOR |
| cute-feed | rust crate | unknown | unknown | OPERATOR |
| cute-guard | rust crate | unknown | unknown | OPERATOR |
| cute-pub | rust crate | unknown | unknown | OPERATOR |
| cute-routes | rust crate | unknown | unknown | OPERATOR |
| memory-conflict | rust crate (city) | unknown | unknown | OPERATOR |
| memory-filter | rust crate (city) | unknown | unknown | OPERATOR |
| memory-folders | rust crate (city) | unknown | unknown | OPERATOR |
| memory-verify | rust crate (city) | unknown | unknown | OPERATOR |
| acct-attach.py | script (city-1) | unknown | unknown | OPERATOR |
| acct-domains.py | script (city-1) | unknown | unknown | OPERATOR |
| add-tricycles-route.py | script (city-1) | unknown | unknown | OPERATOR |
| debug-put.py | script (city-1) | unknown | unknown | OPERATOR |
| route-cname.py | script (city-1) | unknown | unknown | OPERATOR |
| verify-route.py | script (city-1) | unknown | unknown | OPERATOR |

Plus the network-layer things already homed in triples: `cutecity_cc`
and `workers_dev` as NetworkHome things. The tentacle-city worker (1042
on 2026-10-07, route gone) is NOT in the queue — its home is unknown
because the worker itself is unaccounted for; that is a github/network
question for the sibling lane, not a homing question.

## 8. What is unknown (standing)

- Geographic home of every program in the queue (26/26 unknown).
- Whether the github estate's repos map 1:1 onto these programs — the
  sibling lane answers that; this framework does not guess.
- The second Cute City location: run #2 of the OSINT workflow is TBD
  by Zack. The framework is ready; the site is his call.
- Whether district boundaries can be sourced (City open data is the
  first place to look — unproven, not attempted here).
- Surveyed coordinates for the 29 unlocated features — the single
  highest-value measurement for v2.

## 9. Sources

- `cute-city-geo.json`, `cute-city-triples.json`,
  `districts-streets-inventory.md` (same folder, 2026-11-18)
- `~/workspace/coal-harbour-cute-city/` (typed-traits, geo, graph,
  algebra, workflow, rules-graph)
- `~/workspace/vancouver-surveillance/` (surveillance + housing graphs)
- `~/workspace/cutecity/` (DEV-MAP, NETWORK-MAP-2026-10-07, organelles)
- `~/workspace/city/` (build-graph, memory-rs)
- `~/workspace/city-1/` (route scripts)
- Ledger grep: 125 lines mention "cute city"; 0 mention "harbour green".
- Oct 12 veto: memory/2026-10-12.md — "don't make a literal map of city."
