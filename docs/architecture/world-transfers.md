# Active-world transfers

An accepted world connection now survives ordinary map transfers. The main
thread retires old replicated objects, terrain and collision instances,
player and transport presentation, and world sound before publishing the
destination. FrameXML, the realm clock, action-button state, the encrypted
socket, and the saved camera view remain owned across that boundary.

This is separate from initial character login. Transfer plumbing alone does
not establish completion of presentation or movement slices; their accepted
scope is recorded in the [release milestones](../releasing.md).

## Original executable evidence

The authority is build 12340 `Wow.exe`, SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.

| Packet/path | Stock address | Behavior retained |
| --- | --- | --- |
| `SMSG_TRANSFER_PENDING` (`0x03F`) | `0x00401480` | Opens a card; retains the current world and the optional transport entry/source-map pair. |
| `SMSG_NEW_WORLD` (`0x03E`) | `0x00403D10` | Reads map/XYZ/orientation, validates Map.dbc membership, and schedules a zero-delay callback even for the current map ID. |
| `SMSG_LOGIN_VERIFY_WORLD` (`0x236`) | `0x00403DE0` | Same-map packets do nothing. A different map invokes replacement immediately without a worldport acknowledgement. |
| Replacement callback | `0x00403B70` | Tears down old ownership, loads the map, then sends empty opcode `0x0DC` when called by NEW_WORLD. Does not create a new card itself. |
| Object-manager lifetime | `0x004D6750`, `0x004D7750` | Destroys the old replicated manager and creates the destination manager. |
| Camera anchor | `0x006066E0`, `0x00607BD0`, `0x00604B90` | Detaches the old target and changes the camera base transform without recreating the persistent camera/view owner. |
| `SMSG_TRANSFER_ABORTED` (`0x040`) | `0x00403910` | Selects localized text, emits system chat, and dismisses the card without replacing the world or acknowledging a port. |
| Difficulty denial text | `0x00634950` | Searches the contiguous map run for the exact difficulty; no difficulty substitution. |
| Chat event payload | `0x004FDBC0`, `0x0081AC90`, `0x0081AA00` | Sends thirteen positional arguments and forwards the complete callback argument sequence. Nine legacy argument globals are not a callback-size limit. |

The original `MapDifficulty.dbc` has 23 fields and 92-byte records; its
localized denial text begins at field 3. The pinned enUS `GlobalStrings.lua`
transfer formats contain plain text or one map-name `%s`. Missing or empty
tokens produce no message, as in the native handler. `Languages.dbc` has no
ID-zero row, so these system messages carry an empty language name.

## Ownership and completion

### Client entry notification

The client loads all 1,220 stock `AreaTrigger.dbc` rows (ten fields and
forty-byte records). Native `6D2F70` selects the current map's record run;
`6DBE30` polls the living player's world position every 100 milliseconds.
`6CE140` includes a sphere's radius boundary and uses strict faces for an
oriented box. The box retains the native rounded inverse translation before
testing the transformed point.

The first contained row remains active until the player leaves it, so
overlapping volumes do not repeatedly send notifications. A map replacement
resets that ownership, including same-map replacements. Entry freezes an
active-mover heartbeat (`724E70(0xEE)`) and then sends `CMSG_AREATRIGGER`
(`0xB4`) with the row identifier. Both writes follow earlier movement output
and share the existing encrypted writer; queue backpressure retains the pair.
The server then decides whether to initiate a transfer through the existing
pending/new-world handlers. There are no instance-specific portal IDs.

Containment is checked against 720 executions of the original predicate and
its matrix callees. State tests cover overlap, re-entry, dead-player admission,
missing movers, replacement and clock wraparound. An encrypted loopback test
fills the actual writer queue and verifies heartbeat/trigger ordering after
retry. These checks do not replace live rendered entry verification.

The async pump owns one reader and one continuously owned encrypted writer.
Application acknowledgements, time-sync replies, and latency probes share
the writer queue. Queue backpressure retains the application's completion
obligation; it never silently drops an acknowledgement.

NEW_WORLD stores one latest destination but schedules every callback.
Two packets drained together therefore load the latest destination twice
and send two acknowledgements. The runtime preserves this behavior instead
of cancelling the older callback. A synchronous verify-world replacement
pauses packet dispatch at that packet; deferred callbacks resume after it.

Map loading is asynchronous in Solarity. Main-thread packet dispatch pauses
during the callback while the network task retains its connection. Once the
destination terrain/GPU scene is ready, the runtime admits the required ACK.
It then resumes object updates and waits for the new player, transport,
environment, and UI readiness before revealing the world. This ordering
prevents a deadlock in which player creation waits for an ACK that itself
waits for player creation.

Pending cards cannot complete against the old world's already-ready player
and scene. If no packet-owned card exists, the runtime retains the last
presented frame through replacement rather than inventing a card. World
input remains suspended until the replacement player is ready.

Retired terrain and transport jobs may return their worker/archive handles,
but cannot publish instances into a replacement generation merely because
their map, tile, or transport keys match. Immutable caches remain reusable.

## Validation and remaining scope

The completed change passes `cargo fmt --all -- --check`, warning-free
workspace Clippy across all targets/features, and all 591 workspace tests
with all features enabled.

Encrypted loopback tests exercise the actual gameplay pump, consecutive
worldport acknowledgements, time sync during transfer handling, latest-slot
callback ordering, old-object removal, retained camera view, same-map verify
suppression, immediate different-map verification, aborts, unknown maps,
and malformed NEW_WORLD packets followed by valid traffic.

A worker-barrier regression retires an in-flight terrain job, lets it finish,
then requests the identical map/tile. The retired job cannot become the new
resident scene. A Lua regression receives all thirteen event arguments while
preserving and restoring the nine legacy globals. Archive-backed tests cover
MapDifficulty's exact lookup and empty-message semantics.

The matching-transport dynamic loading-card branch at `0x0040AD50` remains
explicitly unsupported. A missing or nonmatching admitted transport follows
the native ordinary-card branch; a matching transport reports the missing
dynamic-card capability. Ship/spline map animation needs its own completion
work. Live rendered teleports and instance entry also need verification;
the encrypted-session tests do not certify those visuals. Player locomotion
and camera input are the next in-world slice.
