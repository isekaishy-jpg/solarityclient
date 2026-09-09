# Stock world-model format

The asset loader validates WMO chunks against build 12340 before passing them
to the dependency parser. Unknown later-format chunks remain errors.

## Transport convex volumes

MCVP is present in build-12340 transport roots. The pinned stock executable
(`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`)
loads this optional chunk in `0x007D7470`, after MFOG. The decoder stores its
payload pointer at root offset `0x15C` and its byte length shifted right by four
at offset `0x198`. Each record therefore contains four floats, with no trailing
flags field. The dependency's Cataclysm-era annotation is not authoritative for
this chunk.

`DecodedWorldModel::convex_volume_planes` retains the authored `[A, B, C, D]`
coefficients and their order independently of group BSP geometry. The loader
rejects duplicate chunks, incomplete 16-byte records, and non-finite values.
Retaining the planes does not implement transport boarding or a runtime convex
volume query.

Rejecting MCVP previously terminated world entry when the client admitted
`WORLD/WMO/TRANSPORTS/TRANSPORT_SHIP_NE/TRANSPORTSHIP_NE.WMO`. Installed-data
validation now loads this root, its group, and all 28 planes. The transport
catalog scan loads all 17 roots present in the inspected data; a separate DBC
entry for `WORLD/WMO/TRANSPORTS/ZEPPELIN/TRANSPORT_ZEPPELIN.WMO` references a
missing file and remains an explicit validation failure.

Run the asset example to check GameObjectDisplayInfo WMO paths and all groups
under an archive prefix:

```text
cargo run -p solarity-asset --example validate_world_models -- <Data> enUS WORLD/WMO/TRANSPORTS/TRANSPORT_SHIP_NE/
```

This checks asset decoding, not live network entry, rendering, or boarding.

## Missing vertex-color streams

`0x007C8560` uploads `0xFF7F7F7F` when a group has no MOCV and root flag
`0x02` is clear. With that flag set, the default is `0xFF000000`. This applies
to both stock vertex formats (4 and 13). These defaults bypass the authored
MOCV correction in `0x007D7380`; treating absent colors as white overlights
exterior zeppelin groups using MapObjU.

`wmo_vertex_color_oracle.py` executes the original uploader for 72 cases,
covering both formats, D3D/OpenGL packing, six root-flag combinations and
present/absent streams. Asset tests compare all 24 absent-stream cases. A GPU
regression renders the six root flags under three ambient levels, verifying
the distinct ordinary and unified defaults through their actual shaders.
