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
while `519A50` triggers fatigue (27) and breath (29). Tutorial state and its
server synchronization are being traced separately from the mirror-timer owner.
