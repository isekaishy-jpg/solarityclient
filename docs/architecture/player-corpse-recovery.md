# Corpse recovery

Corpse query location and resident corpse identity are independent native
owners. A location response does not contain the GUID sent by `RetrieveCorpse`.
The runtime retains both through the original packet, object and UI paths.

## Original build-12340 behavior

`524A30` clears the original/display maps to -1, clears the position and
transport counter, and leaves recovery time untouched. It clears the range
latch, announcing `CORPSE_OUT_OF_RANGE` outside an active arena. A resident
ghost outside an arena also sends empty `CMSG_CORPSE_QUERY` (216). This runs
on world entry and ghost-bit transitions. `6E0FD0` emits
`PLAYER_FLAGS_CHANGED` first; on unghost, `6DF710` emits `PLAYER_UNGHOST`
before clearing the corpse location.

The common receiver `526530` handles inbound 216. A zero first byte clears
the location even without a player. Any nonzero byte reads original map,
three float coordinates, display map and transport low counter. Those fields
apply only to a resident ghost. Other players still consume the packet.

`51F5C0` checks range every world frame. The corpse map must be nonnegative
as a signed word and the local player must exist. The squared XYZ distance
must be at most 1600, including the boundary; NaN fails. There is no health,
ghost or current-map equality gate in this calculation. `512C40` only emits
an event when the latch changes. Entry emits `CORPSE_IN_RANGE` when the two
corpse maps agree, otherwise `CORPSE_IN_INSTANCE`. Arena suppression still
updates the latch. Recovery-delay packets use their already implemented
`513A80` path, which repeats the current entry event without arena suppression.

`523DB0` forms the transport GUID from the signed low counter and high family
1FC00000. `51F430` uses a resident object's current placement matrix. A
resident object without a usable matrix leaves the raw position. An absent
transport uses the retained fallback matrix and requests its pose with 4B6
and the low counter, at most once per 30 seconds on the signed wrapping
wall-clock comparison. Successful resident resolution clears this deadline.
Neither changing the corpse location nor changing its transport resets it.
Inbound 4B7 contains XYZ and orientation; it replaces the fallback matrix
without a GUID, health or residency gate. Matrix construction rounds the
original sine/cosine outputs before transforming the point.

`705B20` is the corpse create constructor called by `4D6577`; its tail and
the world-add function `705FA0` set the resident GUID through `512C20` when
owner fields 6/7 equal the local player and corpse flags field 33 lacks bit 0.
`705F30` clears it when removing any such corpse, even an older corpse after
a newer one became current. Values updates and duplicate remote create
blocks do not rerun that constructor. Becoming bones does not clear the
GUID, and removing bones does not clear it. World replacement carries the
result of these removal predicates into the destination object owner.

The recovery deadline is reset by FrameXML initialization (`52A980`), not
map replacement. The retained query clock and fallback matrix also survive
map replacement. Entry clears the location after the life events. Ordered
notifications publish GUID changes before subsequent packet callbacks, and
the bounded encrypted writer preserves every admitted query under backpressure.

## UI and validation

The complete stock FrameXML test now releases the player, opens both corpse
prompts, waits for recovery, clicks the authored reclaim button, checks its
full GUID, and verifies that the prompt remains until a range-exit or missing
location response. No replacement popup script is used.

The first full ghost flags event also exposed missing `ShowingHelm`,
`ShowingCloak`, `UnitIsAFK` and `UnitIsDND` queries in the stock options/friends
handlers. Their resident local-player field queries now read the replicated
PLAYER_FLAGS snapshot before that event. Remote party snapshots and the
automatic idle-AFK owner remain separate providers.

`tools/ghidra/player_corpse_oracle.py` executes the fingerprinted original
instructions for 560 location, range, transport, ownership, event-order and
Lua-query cases. Object lookup/placement, time, UI dispatch, map-marker
notification and byte output are supplied boundaries. Runtime tests compare
the fixtures, including encrypted response decoding, every truncated prefix,
outgoing queries, live transport placement and map replacement. UI tests
compare the native query results directly.

The location owner retains the `7F4990` marker position for the separate
minimap presentation audit. Minimap marker drawing, the world-map coordinate
selector, and active battlefield-state ingestion are not established by
this recovery stage. Corpse model rendering is also a separate owner.
