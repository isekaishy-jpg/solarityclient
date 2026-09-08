# Unit water effects

Build 12340's `0x00730D10` evaluates registered surface depth after unit position
publication. Its immediate unit state and splash notifications are independent
of deferred movement events 0x15/0x16 and M2 movement animation callbacks.

`Unit_C +0xA30` bit `0x200000` is the value read by `IsSwimming` at `0x006124A0`.
The Lua wrapper returns one numeric `1` or one `nil`. While the movement flag
is clear, the unit bit uses the eligible unit's depth above 75% model height.
While the movement flag is set, the unit bit stays set until a later unit
update observes the completed leave command. No-collision secondary bit 4
preserves this state. Runtime movement publishes the immediate bit to the
shared UI projection, and world exit clears it. This bit was previously named
`water_animation`; the original callback at `0x0053CF10` updates spell action
usability rather than selecting an M2 animation.

When the movement swimming flag is clear, crossing either direction over 40%
model height calls splash audio `0x00746720`. The retained depth lane is updated
only in that branch and survives corrections within the same unit lifetime.
Local and remote runtime owners emit a frozen unit identity and world position;
the sound owner drains each notification once and rejects retired identities.

The constructor zeros unit offset `0x8F0` at `0x0073F718`; body initialization
copies `ChrRaces.dbc` field 10 (`SplashSoundID`) when the race row exists.
The audio callback uses that SoundEntries identifier directly,
without a CreatureSoundData or model-event dependency. Its play record uses
SFX channel 8, one-shot playback, and normal entry concurrency. Local-player
priority is 110. With `Sound_ListenerAtCharacter` enabled, the local splash is
unpositioned with gain 0.65; otherwise it uses the actual unit position without
the vocal path's two-yard Z offset. Remote splashes remain positioned.

The subsequent `0x0071CBA0(0xC9)` call requests ripple kind 3; 0xC9 is not a
SoundEntries identifier. Continuous ripple generation and surface-clipped
ripple rendering have their own native lifecycle and are separate from this
audio/UI boundary.

Tests retain the original immersion instruction fixtures, check the immediate
bit and single entry/exit notifications around deferred movement dispatch,
exercise Lua's numeric/nil result and world-exit reset, and decode field 10
through a real WDBC table. The opt-in archive audio test decodes and mixes the
race-authored splash in both character and camera listener modes, checking its
entry and exact position alongside the existing jump, land, foley, and footstep
callbacks.

## Authored breath and foot-contact models

`SpellVisualEffectCatalog` loads the seven fields of
`SpellVisualEffectName.dbc`: identifier, name, model, area-effect size, scale,
minimum scale, and maximum scale. CEffect initialization at `0x006F7520`
matches names case-sensitively and lets the last physical matching row win.
The identifier is data, not a hardcoded model choice. The installed table
also contains unused absolute developer export paths; the catalog retains
their names and validates archive-relative paths only when requested.

The relevant native slots are:

| Slot | Exact name after `HARDCODED ` | Attachment |
| --- | --- | --- |
| 0 | Footstep Water Run Spray | Positioned |
| 1 | Footstep Water Walk Spray | Positioned |
| 2 | Breath Underwater | 17, falling back to 19 if missing |
| 3 | Breath Cold | 17, falling back to 19 if missing |
| 7 | Inebriated Bubbles | 17, falling back to 19 if missing |

`UnitBreathState` implements `0x0071FA90` and the `$BTH` branch of
`0x00732650`. It refreshes on unit registration and on the signed, wrapping
ten-second deadline checked by `0x0073DAB0`, before position/model updates.
The scaled model-height product first spills to float. Underwater breath
requires **scaled height + 5.0 < liquid surface - unit origin Z**; 5.0 is
the actual constant at `0x009EBF34`. Otherwise the registration's cold-area
flag selects cold breath. Camera immersion and movement swimming flags do
not replace this retained unit state. An authored `$BTH` event emits only
with a model row whose flags bit 1 is clear and object-manager context mode other than
one. For a player, inebriation at least 0.5 takes precedence over either
environment effect. `0x004F7290` compares actual and fake inebriation as
signed values, caps the percentage at 100, and multiplies by the original
0.01 float constant without spilling its return. Consequently a percentage
of 50 is slightly below 0.5; bubble selection begins at 51. The helper
`unit_player_inebriation` retains that precision.

`UnitWaterSprayInput` implements the liquid branch of `0x00723A50`.
Mounted, transported, hovering, ghost, or non-colliding flight units are
rejected, as are backward movement, model flags bit 0, and disabled
`showfootprintparticles`. The authored foot must be within 25 units of the
camera. The liquid identifier must be nonzero and registered depth must
be strictly below half the retained model height. The position is foot
X/Y and **foot Z + registered depth**, preserving the animation offset.
`0x00716FA0` selects slot zero at or below twice walk speed and slot one
above it, despite the apparently reversed authored Run/Walk names.

`UnitEffectScale` preserves the separate placement paths. Positioned
effects (`0x006F8AE0`) use `0x006F7950`'s horizontal model extent, the
CreatureModelData world-effect scale, and owner scale before the authored
clamp. A nonpositive final positioned scale falls back to one. Attached
effects (`0x006F8C50`) first spill the product of CreatureModelData's
attached-effect scale and the authored multiplier, then clamp its product
with the animated attachment scale by correcting the *local* multiplier.
Near-zero attached products skip division; that path does not introduce
the positioned fallback. Area-effect size is independent of these scales.

The original-instruction fixtures contain 1,380 breath/spray decisions and
384 scale cases. They execute the fingerprinted build-12340 routines with
controlled registration, camera, model, and factory providers, including
depth/distance boundaries, clock wraparound, suppression, and scale clamps.
The opt-in archive test resolves and decodes all five effect models and
their authored texture dependencies.

The particle simulator now supports CEffect's model-owned emission switch:
native completion clears emitter runtime bit 2 through `0x008279F0`, while
already-live particles continue moving and aging. It retains the fractional
birth count and advances the rate-variation random stream on positive-time
updates. Zero-length updates leave the live pool and random stream unchanged.

The opt-in runtime test prepares all five models through the shared resident
M2 loader and shader compiler, samples camera-aware bones, builds particle
geometry with the shared twinkle table, and drains each emitter after disabling
births. Inebriated bubbles also contain two mesh draws, so they require the full
M2 scene path. This test runs without a window; it does not exercise GPU
submission, unit event timing, attachment placement, or completion callbacks.

Runtime consumption of the unit requests, mouth/root attachment fallback,
and CEffect model ownership through retirement are still pending. The decision,
scale, and particle simulation checks do not establish visible runtime or full
lifecycle parity. The native constructor also starts global-sequence clocks
at each model's creation tick (`+0x74`), separately from the particle-update
timestamp (`+0x8C`). Unit effect integration must preserve that origin so a
new spray starts at its authored burst even after the world has been running.
