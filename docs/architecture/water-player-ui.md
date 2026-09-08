# Water and player UI

The server owns breath and fatigue countdowns. Registered unit liquid contact
publishes `IsSwimming`; it does not invent timer durations, pause messages, or
damage. Stock FrameXML owns timer placement, color, labels, and bar presentation.

## Mirror timers

Build 12340's `519A50` receives `SMSG_START_MIRROR_TIMER` (`1D9`, 21 bytes),
`SMSG_PAUSE_MIRROR_TIMER` (`1DA`, five bytes), and `SMSG_STOP_MIRROR_TIMER`
(`1DB`, four bytes). The start packet contains a u32 timer identifier, three
signed 32-bit values (current, maximum, scale), one pause byte, and a spell ID.
The native slots at `BD0B80` have a 28-byte stride. Kinds zero, one, and two map
to `EXHAUSTION`, `BREATH`, and `FEIGNDEATH`; all other kinds map to `UNKNOWN`.

The runtime retains packets through UI construction and publishes the complete
anchor image before FrameXML executes. Once the UI exists it delivers every
notification in order, including repeated stops. Start and stop callbacks see
the preceding record; storage changes after callback dispatch, even if Lua fails.
Timer notifications preceding a transfer are delivered before world exit.
`528C30 -> 513EA0` then emits three stops and clears all slots. Destination-world
entry cannot inherit a preceding world's countdown.

`GetMirrorTimerInfo` (`51CC10`) returns six values for slots one through three,
including the original anchor value. `GetMirrorTimerProgress` (`517AA0`) performs
signed, wrapping 32-bit arithmetic:

```
value + (client_milliseconds - start_milliseconds) * scale
```

The progress query accepts case-insensitive timer tokens, does not clamp to the
maximum or zero, and does not special-case the start packet's pause byte.
FrameXML's status bar owns range clamping and its update callback owns pausing.
The pause packet itself only emits `(token, pause_byte)`; it changes no native
slot field. The archived `MirrorTimer.lua` has a separate pause-event quirk: it
compares the first, string argument with zero. Native event arguments are
preserved; the client-authored Lua is not rewritten.

`5199A0` resolves a spell record first and returns its selected-locale name,
including an authored empty name. A missing spell resolves `<TOKEN>_LABEL` in
Lua. The build-12340 `Spell.dbc` name bank validates the 234-word schema and
physical locale column 136. It interprets no unrelated spell behavior.

## Evidence and verification

`tools/ghidra/mirror_timer_oracle.py` executes the fingerprinted original receiver,
its CDataStore readers, resident spell lookup, stop/reset owner, and progress
query in Unicorn. Hooks supply the monotonic clock, Lua argument/result/event
bridges, localized-label formatting/lookup, and tutorial dispatch. No client
entry point or operating-system code runs.

The committed fixture contains 40 ordered receiver cases and 600 native Lua
progress captures. An encrypted loopback regression sends those same bodies
through network decoding, the runtime timer owner, FrameXML event dispatch,
and real Lua queries. It compares event arguments, record visibility during
callbacks, signed values, spell/global labels, fractional slot arguments,
case-insensitive tokens, unknown IDs, pause behavior, refill rates, repeated
stops, and clock/arithmetic wraparound.

The archive-dependent timer-frame test loads the user's original
`MirrorTimer.lua`, `MirrorTimer.xml`, font, and bar assets. It checks simultaneous
breath/fatigue visibility, elapsed countdown, refill, a paused start, and stop
cleanup without launching the client or opening a window.

## Related water UI audit

`6124A0` exposes unit `A30` bit `200000` through `IsSwimming`; this is already
published from immediate unit immersion independently of deferred movement flags.
The native water path also reaches tutorials: `721210` triggers swimming (28),
while `519A50` triggers fatigue (27) and breath (29).

## Tutorial discovery and completion

The shared tutorial owner preserves the two native bit banks (seen and
completed), 60-entry local completion history, and outstanding acknowledgements.
`530920` replaces both banks with the entire `SMSG_TUTORIAL_FLAGS` (`FD`)
payload; it does not clear history. The runtime queues flags and timer packets
in receive order, so a later flags packet cannot alter an earlier timer trigger.
Initial flags enter the UI before synchronous FrameXML construction. World
replacement retains tutorial state while resetting mirror timers.

`530840` gates discovery on the received bank and the seen bit. With
`showTutorials` enabled it requests `TutorialPopup`, dispatches
`TUTORIAL_TRIGGER` with the one-based ID, then marks discovery. With the CVar
disabled it marks discovery and completion without a sound or Lua event.
`FlagTutorial`, `ClearTutorials`, and `ResetTutorials` queue `FE` (zero-based
u32), `FF` (empty), and `100` (empty) respectively through the sole encrypted
writer; queue backpressure retains the pending action.

The Lua globals preserve the original numeric coercions, nil versus zero
return counts, usage text, and misspelled `GetNextCompleatedTutorial` and
`GetPrevCompleatedTutorial` names. Traversal also preserves native boundary
behavior: next ID 60 is the nil sentinel, and previous traversal includes the
adjacent seen-bank bit count slot. Clear fills flags before checking history;
it does not invent completion history for already flagged entries.

Swimming discovery follows the immediate entry transition for the local-player
GUID. Mirror-start discovery follows `MIRROR_TIMER_START` and precedes the
new timer record, including on repeated starts; the shared seen bank deduplicates
the tutorial itself. A controlled non-player unit does not discover swimming.

The stock tutorial checks `InCombatLockdown`. Native `728F70` responds to
changes in player `UNIT_FIELD_FLAGS` bit `80000` through `524600`:
`PLAYER_REGEN_DISABLED` precedes setting lockdown, while clearing lockdown
precedes `PLAYER_REGEN_ENABLED`. `511CC0` returns numeric one or nil. The
runtime queues these changes alongside timer/tutorial notifications and releases
combat before world exit. This implements the query/event dependency; it does
not establish the separate secure-frame mutation enforcement system.

`tutorial_state_oracle.py` captures 122 native receiver, discovery, completion,
history and Lua-result cases. `player_combat_lockdown_oracle.py` executes the
original flag callback, event owner and Lua query for seven state transitions.
Regression tests compare these fixtures, encrypted acknowledgement bodies,
receive order, and writer backpressure. The archive-dependent tutorial test
loads the complete original FrameXML manifest and checks combat deferral,
opening/completion of swimming, and pending breath/fatigue prompts. The badge
remains hidden because build 12340 disables it in `TutorialFrame_CheckBadge`.

## Environmental damage and death

`SMSG_ENVIRONMENTALDAMAGELOG` (`1FC`) now drives signed health prediction,
combat-log and floating-text delivery, stock hit feedback, visual kits, unit
animation and sound. Replicated health remains distinct from prediction; see
[player health presentation](player-health-presentation.md). The full FrameXML
test covers drowning, fatigue, lava, slime, absorption and resistance feedback.

Native unit callbacks read the final packet image and the last old-field mirror
captured for the unit. Health callback `73F330` reconciles prediction before the
per-unit health UI observer. Positive-to-nonpositive raw health produces
`PLAYER_DEAD`; the inverse produces `PLAYER_ALIVE`. `6E0FD0` produces
`PLAYER_FLAGS_CHANGED` with `player`, and local ghost-bit clearing subsequently
produces `PLAYER_UNGHOST`. Packet `37A` refreshes the release timer and emits the
current life event even without a raw-health transition. Runtime notifications
retain the health, ghost and timer image seen by each callback.

The raw-health death callback also produces the eight-argument `UNIT_DIED`
combat record before the life event. `718A90` admits players, rejects creatures
without a resolved template, and suppresses template flag `400`. Creature type
13 selects `UNIT_DISSIPATES`. The record has an empty source GUID, nil source
name, source flags `80000000`, and no damage/spell tail. Both filtered and
unfiltered combat events run with the updated player health and retained release
timer already visible: `729220` calls `6DC0F0` before `7561E0` constructs the log.
Template bindings require the current unit generation and entry. A late or
missing template does not retroactively produce a death record.

`UNIT_DESTROYED` has an unusual native branch: `752ED0` reads the unmodified
stack output after a failed Spell lookup. The oracle supplies an empty Spell
bank and zeroed stack, and does not claim arbitrary stack-residue reproduction.
The runtime preserves the deterministic ordinary-death and type-13 branches.
Remote-player names still depend on the separate name-query provider.

After `PLAYER_DEAD`, `519280` clears the cursor and emits `CURSOR_UPDATE`.
The UI item-cursor projection clears at that boundary; `CursorHasItem` returns
numeric one for the item predicate and nil otherwise, matching `515100`.
The stock UI then dismisses the pending equip-bind popups. The native cursor's
inventory, spell, money and auction pickup/cancellation operations are separate
providers; this establishes the death callback and item predicate, not all cursor
mode side effects.

`6DC070` initializes the retained release timer using the low byte of absolute
private field 1197 and `PLAYER_FLAGS`. `GetReleaseTimeRemaining` (`516210`)
preserves the native signed countdown, clock wrapping, inactive zero and
no-timer `-1`. It is not derived from the breath/fatigue timer or predicted
health. The 216-case original-code timer fixture covers these distinctions.

The original `StaticPopup.lua` owns the death dialog, labels, falling gate and
button visibility. The Lua dependencies have explicit state:

- `IsFalling` uses movement `1000` without `800`; `IsOutOfBounds` uses player
  flag `4000`.
- `HasSoulstone` requires signed raw health at or below zero. Replicated
  self-resurrection spell field 1199 supplies its localized label. An unknown
  nonzero ID returns the executable's literal `UNKNOWN` string.
- `CannotBeResurrected` reads the resurrection restriction projection.
- `IsActiveBattlefieldArena` returns independent numeric-one/nil results for
  native battlefield kind four and the current slot's registered-match flag.
- `InCinematic` reads the in-world cinematic flag; it is separate from Glue
  movie playback.

`RepopMe` invokes `6D2950`. The player constructor installs vtable `A326C8`;
slot `128` points to `6DAC10`, which admits signed nonpositive health **or a
ghost**, regardless of stand animation. After restriction admission the explicit
Lua request sends `15A` with one zero byte. `UseSoulstone`'s replicated-spell
branch (`51ADD0`) checks the restriction but has no health/ghost gate; it sends
`2B3` with an empty body. Both actions enter the sole encrypted writer in order,
retaining pending requests under backpressure and clearing them on world exit.

World entry invokes `528010`'s automatic release for signed nonpositive raw
health, using byte one, before `6E7F50`'s life event. Positive-health ghosts do
not trigger that automatic request. Initial and replacement worlds retain the
release timer, seed resurrection inputs, and announce life after
`PLAYER_ENTERING_WORLD`. The native life fixture covers health boundaries and
the following cursor event; encrypted tests distinguish explicit and automatic
release bytes.

Character selection uses `SMSG_CHAR_ENUM` flag `2000` for its ghost preview,
independently of current-world health. The refreshed directory passes that flag
through metadata and Glue to the existing ghost model and lighting path. The
archive-dependent character refresh test covers repeated living/ghost changes
for the same GUID, including the independent rename flag.

`player_death_dialog_oracle.py` executes the original Lua wrappers, release
function and real player virtual predicate for 190 cases. Lookup, inventory
name, restriction-provider and datastore boundaries are documented in the
script. The archive-dependent complete FrameXML test opens the death dialog,
checks falling disables release, clicks each real button and verifies both
encrypted packet bodies. It does not replace authored popup scripts.

### Remaining life-system work

The popup/query and explicit spell-button path is implemented; this is not a
claim of complete resurrection support. Native inventory fallback (`6D6640`,
`6D6560`) finds usable item spells with effect 94. Aura type 314 builds the
restriction used by `727860`, and spell attribute `08000000` can bypass it.
Live inventory-item selection, aura restriction construction, battlefield
activation and world cinematic ownership still need their respective session
providers. Their current empty state has no item, restriction, active arena or
world cinematic. Provider inputs are not established by the death-dialog oracle.

Corpse recovery, resurrection offers, other cursor-mode side effects, and the
unit's forced-release flag remain in the life-system audit. Lighting and sky
remain the final water-slice work.
