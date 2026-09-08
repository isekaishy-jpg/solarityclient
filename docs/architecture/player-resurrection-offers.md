# Resurrection offers and source names

Build 12340 registers `6DBD00` for server opcode `15B` (`6E86AC`). Its body
contains a full eight-byte source GUID, a four-byte name-region length, that
region, and unsigned sickness/timer bytes. `47B6B0` advances by the supplied
length; the event reads a C string at the start. The decoder preserves that
distinction, including embedded terminators and an empty region, while rejecting
out-of-bounds or invalid UTF-8 strings. Extra packet bytes are not interpreted.

The GUID and flags are retained before looking up the player. A nonempty name
emits `RESURRECT_REQUEST` (`512500`, event `106`) only when virtual predicate
`6DAC10` reports nonpositive signed raw health or the player ghost flag. An
empty name goes through the session name cache even when the player is alive.
The named event and its preceding offer snapshot retain packet order.

The original Lua APIs `5159C0` and `515A00` return numeric one or nil. The latter
suppresses the timer in battlefield kind four. `51AAC0` and `51AAF0` require a
resident player, then call `6D1D30` without another health or restriction check.
A nonzero GUID produces `15C`, full GUID, and an accept/decline byte. Only the
GUID is consumed; flags survive. The UI consumes it before returning to Lua,
and the next packet batch observes consumption even when the writer is full.
Unpublished offer snapshots cannot be erased by the previous UI image.

`RetrieveCorpse` (`51B800`) first declines an offered resurrection when a player
is resident, then always queues `1D2` with the retained corpse GUID, including
zero. This API does not enforce range, health, or delay itself. Those inputs
belong to FrameXML and the corpse provider.

## Deferred source names

`9CB770` initializes the native name cache with request opcode `50`; `668CE0`
writes its full source GUID. `635BA4` registers `6357D0` for response `51`.
Responses use a packed GUID and status byte. Status zero carries bounded
name/realm C strings (48/256 bytes including terminators), race, gender, class,
and an optional five declined forms, each bounded by 64 bytes. Status two
requeues an existing request without completing its callbacks. Status three
stores `?` with empty identity data. Other nonzero statuses complete pending
callbacks and discard the entry. The native flagged-record bit for status three
and `1FD` GUIDs is retained with the record.

The session cache shares one wire request across repeated unresolved offers,
but retains each callback: `67D770` receives zero for callback deduplication.
`6DBC60` always looks up the **current** offer GUID. A miss clears both GUID
halves and both flags; a ready, nonempty name emits only for a resident dead or
ghost player. This matters when an older request finishes after a newer offer
or after the user has consumed the GUID. Name records survive map replacement;
offers and their UI state are cleared at world exit. This implementation does
not persist name records to `namecache.wdb`.

## Recovery clock and decline dialog

The switch tables at `526EF0`/`526F24` route server opcode `269` to `5267D8`.
It reads milliseconds and calls `513A80`, storing `now + delay` with wrapping
arithmetic and replacing a zero deadline with one. `516280` returns positive
signed remaining time divided by 1000, otherwise zero. This clock is independent
of the release timer. Restarting it repeats the current corpse-range event;
map equality chooses event `184` or `185`.

The stock decline callback also calls `UnitIsControlling`. `613C90` returns one
when either of the requested unit's charm or summon GUIDs is nonzero. The local
player projection reads absolute replicated words 6–9 before associated life
events. Its Lua API follows the existing local-player unit projection boundary.

## Evidence and validation

`player_resurrection_offer_oracle.py` executes the pinned original offer reader,
player virtual predicate, Lua queries/actions, recovery owner and control-GUID
predicate for 221 cases. Object/name lookup, Lua stack, event dispatch, clock
and outgoing writes are supplied boundaries. `player_name_query_oracle.py`
executes the original name response and query writer for 21 cases; its supplied
boundaries expose cache operations and bounded record copies.

Encrypted tests compare all offer and name-response records, malformed prefixes,
and response/query/reclaim bodies. Runtime tests cover native deferred callback
decisions, retries, duplicate callbacks and consumed/new offer ordering. UI tests
cover numeric-one/nil results, one-shot response admission, control GUID halves,
and recovery wraparound. The complete archive-backed FrameXML test clicks all
three resurrection popup variants, waits out their authored countdowns, and
checks that decline restores the death dialog without replacing stock scripts.

Resident corpse selection, query-position/range publication, transport corpse
queries, battlefield activation, and inventory self-resurrection selection are
separate owners still being connected in the water/player-UI audit.
