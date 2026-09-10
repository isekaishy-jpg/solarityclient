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
Breath height is the raw M2 bounding-box Z extent registered at `Unit_C +0xAC`,
multiplied by the raw object scale at `+0x98`. It is separate from the movement
height at `+0x854` used by the water-contact depth test. Cold-area selection
(`0x0078F1F0`) uses the interior group AreaTable relation, then the root relation
when the group has no valid area. The WMO join must contain both records.
Area flag 2 retains local climate; otherwise only the immediate parent's
flags replace the area flags. Flag 1 supplies the cold result.
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
The caller dispatches all forty `$BL0`-style markers: category B/F/R/S/W,
left/right L/R, and ordinal 0 through 3. `$FSD` is the separate audio callback.
The CVar is registered by `0x00715330` with default `1`. Spline speed retains
`0x00987570`'s unspilled return for this comparison; rounding it to float first
can change the selected spray at the twice-walk-speed boundary.

`UnitEffectScale` preserves the separate placement paths. Positioned
effects (`0x006F8AE0`) use `0x006F7950`'s horizontal model extent, the
CreatureModelData world-effect scale, and owner scale before the authored
clamp. A nonpositive final positioned scale falls back to one. Attached
effects (`0x006F8C50`) first spill the product of CreatureModelData's
attached-effect scale and the authored multiplier, then clamp its product
with the animated attachment scale by correcting the *local* multiplier.
Near-zero attached products skip division; that path does not introduce
the positioned fallback. Area-effect size is independent of these scales.

The original-instruction fixtures contain 1,380 breath/spray decisions,
384 scale cases, 185 marker dispatch cases, 126 spline-speed boundaries,
and 480 cold-area selections. They execute the fingerprinted build-12340 routines with
controlled registration, camera, model, and factory providers, including
depth/distance boundaries, clock wraparound, suppression, and scale clamps.
The opt-in archive test resolves and decodes all five effect models and
their authored texture dependencies.

The particle simulator now supports CEffect's model-owned emission switch:
native completion clears emitter runtime bit 2 through `0x008279F0`, while
already-live particles continue moving and aging. It retains the fractional
birth count and advances the rate-variation random stream on positive-time
updates. Zero-length updates leave the live pool and random stream unchanged.

The runtime retains one registration clock per ECS unit lifetime and shares
movement's existing liquid samples. Model admission initializes breath state;
ordinary registration updates leave it cached until its ten-second deadline.
The five named models decode on the CPU executor during login. GPU preparation
warms at most one driver pipeline per service tick before publishing the shared
source bank. Authored callbacks construct placement-local playback and particle
state without decoding or compiling shaders.

The M2 scene invokes the unit callback while dispatching each authored event,
including expired variation tails. It appends new effects after unit poses in
the same frame. Attached effects use the animated mouth matrix (17), falling
back to root attachment 19. Duplicate retirement compares the old *resolved*
attachment with the new *requested* attachment, as `0x006F8A60` does; an old
root fallback is not replaced by another request for attachment 17.

Resident effect creation performs both the ordinary default request and the
`0x008251B0 -> 0x006F7680` load callback's blended Stand request. The two
weighted/cycle pairs consume four CRT draws for a boned model. Sixteen original
load probes cover before/during-scene timing, fallback modes, and absent bones.
Requests waiting for source publication retain their creation tick and position;
their primary starts in the load-completion phase. The global-track origin
(`+0x74`) and particle timestamp (`+0x8C`) remain tied to creation.

Completion `0x00744870` requests authored Despawn (159) when present, with
`0x00743580` as its next completion. Otherwise it seeks the current primary
range's final millisecond and pauses before retirement. The seek retains the
stored reciprocal speed and repeat count; 2,880 original-instruction probes
cover zero, tiny, negative, and accelerated speeds, large offsets, and wrapping
scene clocks. Retirement removes mesh/ribbon submission and disables new
particles while retaining live particles, matching `0x006F87C0` and the special
particle submission in `0x00828A00`. `0x006FA450` waits on model particle-live
bit `0x400`; ribbon histories do not keep a retired effect alive. The root loop
at `0x00821BEE` advances registered effects through `0x00828A00`, including its
attached children. Runtime effects continue simulation and particle preparation
outside their authored model bounds, which need not contain the live particles.
Moving the camera cannot strand a retired particle pool. Destruction of the ECS unit retires positioned spray
as well as attached breath; an ordinary body material rebuild preserves the
unit lifetime.

Runtime retirement uses the current placement metadata's first-effect boundary
to exclude ordinary models from the drain scan. After a topology change it
scans the complete current list until the metadata is rebuilt. Removal keeps
surviving placements in order and uses the same retirement phase and live
particle predicate; attachment cleanup searches only the surviving candidate
range. Effect-free settled frames do not scan terrain M2 placement records.

The opt-in runtime tests prepare all five archived models through the shared
resident loader and shader compiler, advance the completion state, sample
camera-aware bones, build particle geometry with the shared twinkle table,
and drains each emitter. Inebriated bubbles also contain two mesh draws and use
the complete M2 scene path. None of the five models contains authored events,
so their effects introduce no additional sound callbacks. A hidden SDL/Vulkan
surface test warms the complete bank and prepares particle/mesh draw packets
through the real M2 frame until all five placements and their frame-local
source references drain.

A portable scene fixture dispatches authored breath markers through the live
callback, verifies same-frame publication and attachment position, checks the
different duplicate rules for attachments 17 and 19, and drains live particles
after unit destruction with the camera facing away. It also verifies delayed
bank publication, before-scene primary start, preserved creation/global clocks,
and rejection of requests whose unit was destroyed while loading. These tests
create no visible window and do not launch the client. Final GPU command
submission and visible placement during gameplay remain outside these checks.
