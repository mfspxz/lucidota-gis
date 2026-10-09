# GIS data dictionary (canon)

## saltspring.places
| field | type | meaning |
|---|---|---|
| name | string | canonical place name |
| kind | enum | village\|ferry\|park\|beach\|lake\|airstrip |
| lat, lon | f64 | WGS-84, validated by `geo_core::validate::Region` |
| note | string | human note; coordinates of named features are real-world |

## ferry_routes
| field | type | meaning |
|---|---|---|
| name/from/to/operator | string | BC Ferries routes; endpoints real |
| waypoints | LatLon[] | first waypoint is island-side (validated); offshore points approximate |

## coombs.parcels (synthetic sample fabric)
| field | type | meaning |
|---|---|---|
| id | string | COOMB-{ix:02}-{iy:02} |
| nw/se | LatLon | parcel corners |
| land_use | enum | residential\|agricultural\|commercial\|park\|forest (deterministic hash) |
| area_m2 | f64 | equirectangular approx |
| fabric | const | "synthetic-sample" — replaced by DataBC parcel mirror (gis-016) |

## poi.Poi
id, name, kind, lat, lon, notes, user_added, created_unix. JSON persistence.

## ortho.Frame
path (PPM P6), nw (LatLon), pixel_size_m (ground sample distance).

## offline_packs (.gispack tar)
manifest.json {name, format:"gispack-1", region, tiles[{z,x,y,digest,size}], layers[]},
tiles/{z}/{x}/{y}.png, layers/*.geojson. Tile digests are blake2b-256 hex.

## palontour CAS
objects/<xx>/<rest> keyed by blake2b-256 hex; write-then-rename; Tileset root
digest commits to the full (z,x,y)->digest index.
