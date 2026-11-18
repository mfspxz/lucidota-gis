# Cute City — Districts & Streets Inventory v1

Generated 2026-11-18. Companion to `cute-city-geo.json` (38 features) and
`cute-city-triples.json` (97 triples, 83 things). Every row traces to a source
file; state is ternary only (proven / claimed / unknown). No coordinate is
invented anywhere in this inventory — addresses are strings, districts are
named areas with no surveyed boundary.

## Districts (10)

| District | Places / things | District assignment state | GeoJSON features | Source |
|---|---|---|---|---|
| `coal_harbour` | harbour_green, coal_harbour_park, seawall_route, chcc, restaurant_unit_1700sqft, splash_pad, washrooms | claimed (run #1 framing) | 6 located | harbour-green-geo.json + graph |
| `west_end` | bia_westend | claimed | 1 located | harbour-green-geo.json |
| `central_business_district` | belkin_house, catholic_charities_hostel, bia_downtown, granville_station | proven for shelters + station; claimed for bia_downtown | 1 located, 2 unlocated, 1 station | housing graph + harbour-green-geo.json |
| `granville_strip` | 6 venues (910 / 932 / 1050 / 1100 / 1117 / 1202 Granville) | claimed | 6 unlocated | surveillance graph |
| `granville_yaletown` | — (district-thing, no places assigned yet) | unknown | 0 | housing graph things list |
| `dtes_strathcona` | osborn_shelter, new_fountain_shelter, ugm_heatley, sro_buildings_vancouver | proven | 3 unlocated | housing graph |
| `east_hastings` | — (district-thing, no places assigned yet) | unknown | 0 | housing graph things list |
| `mount_pleasant` | — (district-thing, no places assigned yet) | unknown | 0 | housing graph things list |
| `new_west_downtown` | purpose_society_shelter_new_west, russell_residence_new_west, agnes_602_new_west, new_westminster_station, columbia_station | proven except agnes (claimed) | 3 unlocated, 2 stations | housing graph |
| `new_west_sapperton` | sapperton_station | proven (closest-to) | 1 station | housing graph |

District notes:

- No district has a surveyed boundary. Districts are named areas, not
  geometries, until a boundary source is found. Do not draw polygons from
  these rows.
- `harbour_cafe_nicola` (683 Nicola St) has NO district assigned — address
  only. It sits in the Coal Harbour / West End edge and no source assigns it.
- `granville_strip` (surveillance graph) vs `granville_yaletown` (housing
  graph) are two names over overlapping ground: same city, infinite maps.
  The strip is the venue row; Yaletown is the wider housing-graph area.
  They are kept as separate district-things until a source relates them.
- 10 of 15 SkyTrain stations have no district: only granville_station
  (central_business_district), sapperton_station (new_west_sapperton),
  new_westminster_station + columbia_station (new_west_downtown) have proven
  closest-to districts. Waterfront has proxy links only, not a district.

## Streets (15, from sourced addresses only)

| Street | Addresses on file | What is there | State |
|---|---|---|---|
| W Cordova St | 1199 | Harbour Green Park (base site) | proven |
| Bute St | foot of Bute | grand staircase above restaurant unit | proven |
| Jervis St | 323 | Coal Harbour Park | proven |
| Melville St | 1100 #1380 | Downtown Vancouver BIA | proven |
| Davie St | 1033 #411; 1295 | West End BIA; West End–Coal Harbour Community Policing Centre | proven (BIA); claimed (CPC, graph nodes) |
| Nicola St | 683 | Harbour Cafe (distinct from Harbour Green Cafe lease) | proven |
| Granville St | 910, 932, 1050, 1100, 1117, 1202 | Roxy Burger, Roxy, strip venue, Edge, pawn shop, Two Parrots | 1 proven (1050), 5 claimed |
| W Hastings St | 27 | Osborn shelter, 80 spaces | proven |
| E Hastings St | 356, 601 | New Fountain shelter, 60; UGM Heatley, 82 | proven |
| Homer St | 555 | Belkin House, 81 spaces | proven |
| Comox St | 1056 | Catholic Charities hostel, 108 spaces | proven |
| Columbia St (New West) | 502 | Purpose Society shelter, 50 beds | proven |
| Carnarvon St (New West) | 740 | Russell Residence, 40 SRO + 15 shelter | proven |
| Agnes St (New West) | 602 | Agnes, 52 supportive — occupancy UNCONFIRMED | claimed |
| Beach Ave | 2099 | Park Board office | claimed (people file) |

## SkyTrain stations (15, names only — no addresses, no coordinates)

Waterfront, Granville, Vancouver City Centre, Stadium–Chinatown, Main
Street–Science World, Oakridge–41st, Langara–49th, Marine Drive, Sapperton,
Braid, King Edward, Nanaimo, New Westminster, Columbia, 22nd Street.
TransLink list verified 2026-10-30. Shelter/crime proxies are inferences,
not published ratings; a low proxy score is NOT a safety claim at a point
in time. No per-station count of homeless-person encounters or police
encounters exists in public data.

## Shelters / SRO (8)

Osborn (27 W Hastings, 80), New Fountain (356 E Hastings, 60), UGM Heatley
(601 E Hastings, 82), Belkin House (555 Homer, 81), Catholic Charities
hostel (1056 Comox, 108), Purpose Society (502 Columbia St NW, 50 beds),
Russell Residence (740 Carnarvon St NW, 40 SRO + 15 shelter), Agnes
(602 Agnes St NW, 52 supportive, claimed, occupancy unconfirmed).
Bed counts are variable daily (BC Housing daily / bc211 twice daily).
SRO citywide: 146 buildings, ~6,500 rooms, most in DTES (proven, housing graph).

## Not in this inventory (deliberate gaps)

- Personal residences / mailing addresses (2 known address-things in
  USER.md): tier HIDDEN. They are not listed here by design. See
  `homing-framework.md` — a hidden home is a first-class home, not a
  missing one.
- 502 20th St New Westminster tiny homes: announced Feb 2026, opening
  unconfirmed — not a place-thing until confirmed.
- `cafe afrigsto`: unmatched tenant name from run #1. No match in public
  sources. Closest distinct matches only (Harbour Cafe 683 Nicola;
  Harbour Green Cafe lease). Never force-fit a phonetic guess.
- Real-time positions: no live trike, vehicle, or person positions exist
  in any source. GTFS-realtime publishes vehicle positions (TransLink,
  key required) — a future navigational aid, not a current one.

## Counts (reconciled against the two JSON files)

- GeoJSON features: 38 (9 located, 29 null-geometry unlocated)
- Distinct coordinate points: 5 (all verbatim from harbour-green-geo.json)
- Triples: 97 (36 proven, 48 claimed, 13 unknown)
- Things: 83 (3 district-things with no triples yet: east_hastings,
  granville_yaletown, mount_pleasant)
- Districts: 10 (3 with no places assigned yet)
- Streets: 15 (from sourced addresses only)
- Stations: 15 (4 with a proven district, 11 without)
- Shelters/SRO: 8 (7 proven, 1 claimed)

## Sources

- `~/workspace/coal-harbour-cute-city/harbour-green-geo.json` (2026-10-26)
- `~/workspace/coal-harbour-cute-city/harbour-green-graph.json` (2026-10-26)
- `~/workspace/coal-harbour-cute-city/typed-traits.json` (2026-10-26)
- `~/workspace/coal-harbour-cute-city/algebra-primitives.json` (2026-10-26)
- `~/workspace/vancouver-surveillance/vancouver-surveillance-graph.json` (2026-10-27)
- `~/workspace/vancouver-surveillance/housing-crime-skytrain-graph.json` (2026-10-30)
- `~/workspace/cutecity/NETWORK-MAP-2026-10-07.md` (live probe 2026-10-07)
- `~/workspace/cutecity/DEV-MAP.md`, `~/workspace/cutecity/organelles/MAPPING.md`
- `~/workspace/city/build-graph-2026-10-12.json` (session/build graph, no geo)
- Ledger: 125 lines mention "cute city"; 0 mention "harbour green" — the
  Harbour Green work lives in workspace files, not the ledger.
