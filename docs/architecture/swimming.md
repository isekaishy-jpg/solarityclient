# Swimming movement

The specification is the locally owned build-12340 executable, SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.
The runtime owns deferred immersion commands, movement flags, analytic anchors,
collision residency, transport changes, camera steering and frozen wire snapshots.
The systems crate owns the native arithmetic and liquid geometry projection.

## Immersion and commands

After position registration, `73AB20` calls `730D10`. Unit registration selects
the first primary interior WMO group, otherwise the ordinary WMO/terrain liquid
query. This is independent of camera registration and camera submersion.
Swimming begins when liquid depth exceeds three quarters of the unit height,
subject to the original unit flags, parent GUID and rising-jump admission.
The exit threshold subtracts `0.02777777798473835` from that height product.
No-collision secondary flags bypass the update. The resulting same-time events
enter the movement command queue before producing StartSwim/StopSwim packets.

`989660` clears falling and switches to the swim speed/basis. `98BFF0` clears
swimming and ascent/descent, clears pitch unless secondary flag `20` retains it,
and conditionally starts a zero-launch fall. That fall keeps the former swim
speed while rebuilding its direction after clearing swimming. Surface jumps
use `9883F0`/`988370` and the original downward launch bits `C1118C48`.

The local controller resolves held input after an immersion transition.
Ascent/descent and pitch commands reanchor the swimming curve. Camera steering
follows `6023D0` -> `5FBE70` -> `989BC0`: negate the camera pitch, clear keyboard
pitch, reanchor, then emit SetPitch. Release/acquire retains water mode.
Remote movement uses the same 3D curve and collision path with received flags,
pitch and command clocks.

## Motion and collision

`987570`, `987EF0` and `987B50` select swim speed, pitched and flat direction
bases, and the translation/yaw/pitch/ascent trajectory. Forward movement uses
the pitched basis; strafing retains a flat basis. Combined axes preserve the
original diagonal factors, float stores, signed pitch remainder and analytic
arc anchors. Reanchoring and transport rebasing are separate operations.

`760B40` checks parent retention before moving, then sweeps ordinary collision
geometry and a separate water surface bank. Water sweeps use three quarters of
the unit height. The first ordinary contact disables later water-bank sweeps
within the interval. Ascending surface contact requests the original jump;
other contacts project the remaining movement against the collision normal.
Partial progress, tiny-contact limits, geometry failure and remaining time
follow the original solver.

The water collection flag `20000` admits all authored liquids without a DBC
type filter. Terrain follows `7CE960`/`7CE5D0`; WMO follows `7C94B0`, including
its authored grid, masks and transformed instance geometry. The runtime
negates plane normals after collection, preserving vertex order. The two
collision banks share residency, bounds refresh and passenger localization.

## Validation

Checked-in executable fixtures cover 7,290 trajectory cases, 348 collision
cases, 1,328 immersion decisions, 128 liquid geometry collections and 108
command transitions. The arithmetic/state fixtures compare every output bit.
Geometry compares vertex bits and normal direction with the established
hardware reciprocal-square-root tolerance.

Runtime regressions cover deferred fall-to-swim and swim-to-fall transitions,
camera-directed 3D movement, native surface-height jump contact, release/acquire,
remote pitch prediction and resident terrain liquid selection. Oracle scripts
live under `tools/ghidra`; they map the pinned executable into an emulator and
never start the client entry point or a game window.
