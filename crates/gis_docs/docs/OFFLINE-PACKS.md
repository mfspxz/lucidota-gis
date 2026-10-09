# Offline pack spec: .gispack v1 (canon)

A `.gispack` is a tar archive (readable by system tar):

```text
manifest.json
tiles/{z}/{x}/{y}.png
layers/{name}.geojson
```

`manifest.json`:
```json
{"name":"saltspring-z10","format":"gispack-1","region":{...},
 "tiles":[{"z":10,"x":301,"y":372,"digest":"<blake2b-256 hex>","size":1234}],
 "layers":["places"]}
```

- The manifest digest (blake2b-256 of manifest.json) is the pack identity.
- `offline_packs::PackReader::verify` recomputes every tile digest; any
  mismatch is a tamper verdict.
- Build from ortho pyramid tiles + layer GeoJSON via `PackBuilder`.
