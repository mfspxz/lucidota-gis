# gis_api REST (canon)

Base: `http://127.0.0.1:PORT/v1`. Auth: `X-API-Key: <key>` header, constant-
time compare. Missing/wrong key -> 401. No key, no data (privacy by default).

| method | path | description |
|---|---|---|
| GET | /v1/health | {"ok":true} (no auth) |
| GET | /v1/layers | layer names |
| GET | /v1/layers/{name} | GeoJSON |
| GET | /v1/poi | all POIs as JSON |
| GET | /v1/poi/near?lat=&lon=&r= | POIs within r meters |
| GET | /v1/route?from=&to= | from/to are POI ids; pedicab profile JSON |
| GET | /v1/tile/{z}/{x}/{y} | PNG tile bytes (from pack or synth) |

Rate posture: single-threaded std TcpListener; behind the Exchange governor
in production. Not internet-facing in v1.
