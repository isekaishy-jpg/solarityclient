# Unit water notifications

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
