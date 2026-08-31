# Audio content loading

Audio starts at the client database boundary rather than at the decoder. Build
12340's `SoundEntries.dbc` has exactly 30 four-byte fields: an identifier and
type, an internal name, ten file names, ten corresponding frequencies, a base
directory, volume and distance policy, flags, an EAX definition, and a
`SoundEntriesAdvanced` identifier.

The asset crate owns this schema because the same identifiers feed UI sounds,
world ambience, creature vocalizations, footsteps, spells, and music. It joins
each nonempty file slot to the row's base directory and validates the result as
an ordinary `AssetPath`. It does not read the encoded payload while building
the catalog.

File slots remain in authored zero-through-nine order and retain their exact
frequency values. Empty slots stay absent. The catalog does not substitute a
neighboring slot, basename, loose file, or alternate codec when a selected path
is invalid or missing.

Some shipped build-12340 rows place one root separator before `DirectoryBase`.
The DBC boundary removes that archive-root marker before constructing the
normalized `AssetPath`; parent traversal and every other invalid path remain an
error. This is the same single logical MPQ identity, not a second lookup.

The media crate will consume these typed definitions, apply stock variation
and channel policy, and request the chosen encoded payload through
`AssetStore`. The codec/backend boundary receives those selected bytes; it does
not search archives independently. Replacement sound packs therefore use the
same mechanism as higher-resolution texture packs: normal MPQ precedence
selects one payload for one logical path without a pack-specific resource type.

Variation selection treats each positive `Freq` field as the size of its file
slot's contiguous ticket range. Zero-frequency slots are disabled and are not
silently made playable. The media selector accepts an already bounded ticket;
the runtime remains responsible for advancing and scaling the process-wide
stock CRT random stream in the evidenced call order.

Selected encoded payloads are cached by normalized `AssetPath`. One
`Arc<EncodedSound>` owns the path, selected archive descriptor, and original
encoded bytes for all concurrent decoder voices. The cache reads a path only
once, retains no parallel stock/pack version, and can collect entries after the
cache becomes their sole owner. No unevidenced byte limit or age-based eviction
policy is introduced.

SDL3_mixer admission is device-independent. A memory mixer uses build 12340's
44.1 kHz output default and signed-16 stereo minimum, while every resource still
reports its authored sample rate and channel count. `SoundEngine.cpp` at
executable `0x0087ee60` selects residency after the exact archive payload has
been opened. An `.mp3` path always streams. Every other supported payload is
predecoded when its archive-reported logical size is less than or equal to
`Sound_MaxCacheableSizeInBytes`, and streams when larger. The CVar defaults to
1 MiB and the executable caps its effective value at 2 MiB. The normalized path
forms reusable sample identity. Streamed playback creates a distinct decoder
resource per voice and releases it when that voice clears its backend track;
the noncacheable stock branch does not reuse a stream object.

The pinned safe SDL wrapper copies the original encoded file into its `Audio`
resource during admission. This is the one adapter-boundary copy: the temporary
`IOStream` retires immediately, and the encoded cache can collect its source
once other owners release it. Unsupported or corrupt bytes fail admission and
do not trigger another codec, filesystem search, or extension substitution.

Output and track ownership remain separate at the adapter boundary.
`SoundOutput` owns one explicitly selected default-device or memory mixer;
`SoundBackend` borrows that output and preallocates exactly the caller-provided
track count. `OwnedSoundEngine` contains their self-reference behind one tested
safe boundary: a stable boxed mixer allocation outlives the engine field and
Rust's field destruction order drops every track first. Runtime policy passes
the authoritative `Sound_NumChannels` value, whose build-12340 default is 64.
Exhaustion is reported instead of allocating another track or stealing an
active voice without an evidenced priority rule.

Each voice handle carries the backend, slot, and slot generation. Stopping a
voice permits reuse, and the next generation invalidates the earlier handle.
Playback accepts finite nonnegative gain without clamping amplification and
retains explicit one-shot versus infinite-loop behavior. Pause, resume, stop,
gain changes, memory mixing, and state queries remain backend primitives;
category buses, spatialization, DSP, fades, and voice priority belong to the
stock-facing sound-engine layers above this adapter.

`SoundEngine` is the first stock-facing orchestration layer. It owns one joined
catalog containing the exact `SoundEntries.dbc` and
`SoundEntriesAdvanced.dbc` tables, plus the encoded cache, decoder registry,
backend track pool, and retained voice policy. A request supplies its entry
identifier, calling category, already bounded variation ticket, and loop mode.
The engine therefore does not infer category from an unevidenced `SoundType`
mapping, advance another RNG, or guess whether a payload is music.

The live settings snapshot mirrors `Sound_EnableAllSound`, the SFX/music/
ambience enables, and their master/category gains. Those CVar gains validate in
the stock zero-to-one range; the authored entry volume remains a separate
nonnegative multiplier. Disabling a category suppresses new playback before
file selection or archive access and changes retained voices to zero gain. A
later enable restores gain without restarting or replacing the voice.

Missing entries, zero-weight definitions, out-of-range variation tickets,
invalid authored volume, exact archive failures, decoder failures, and full
voice pools remain distinct errors. The engine does not search a neighbor,
clamp a ticket, choose a zero-weight file, try another extension, or steal an
active voice. Natural-stop and encoded-cache collection are explicit service
operations so timing and memory policy can be driven by the eventual stock
main-loop integration rather than a guessed timer.

The schema is checked against the public build range covering
3.1.0.9767 through 3.3.5.12340 in the
[WoWDBDefs SoundEntries definition](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SoundEntries.dbd).

Advanced emitter and ducking policy begins in a separate exact asset catalog.
For builds 3.0.1.8770 through 3.3.5.12340,
`SoundEntriesAdvanced.dbc` contains 24 four-byte fields: its identifier and
SoundEntries relation; inner radius; four time fields; random offset, usage,
and interval fields; volume-slider category; three category-duck gains; two
influence radii; duck and unduck times; inside/outside angles and outside gain;
outer radius; and name. The asset crate preserves every value and rejects a
later layout or non-finite float before media policy consumes it. Enum meanings,
time units, and behavioral validation remain uninterpreted until executable or
call-site evidence establishes them. The layout is checked against the
[WoWDBDefs SoundEntriesAdvanced definition](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SoundEntriesAdvanced.dbd).

WotLK's 28-byte terrain `MCSE` record stores a
`SoundEntriesAdvanced.ID` followed by an authored position and a three-component
directional-cone orientation. The stock map-chunk path passes that final vector
through `SE2__PlaySoundKit`; the sound engine normalizes it, reverses Z for its
FMOD coordinate boundary, and installs it with `set3DConeOrientation`. The asset
API therefore exposes `cone_orientation`, not a guessed attenuation size. Media
must resolve `MCSE → SoundEntriesAdvanced → SoundEntries` before selecting a
file. It must not send the MCSE key directly to `SoundEntryCatalog`, even when
IDs happen to overlap in a particular data set.

Media's `SpatialSoundCatalog` owns those tables once and performs the exact
two-step lookup. `SoundEngine::resolve_spatial_sound` exposes the advanced and
base rows as one borrowed result, so the eventual zone service can retain the
terrain emitter's position and cone orientation separately. An absent key is a
typed failure and never falls through to another row.

`AdvancedSoundProperties` applies the corrections recovered from build 12340's
`SoundInterface2AdvancedKitProperties.cpp` constructor. Decreasing `TimeA`
through `TimeD` values roll into the next 86,400,000-millisecond day. Duck gains
outside zero through one become the neutral `1.0`; an inverted influence pair
moves the inner radius down to the outer radius; and an inverted cone pair moves
the inside angle down to the outside angle. These are stock fallbacks, not new
compatibility behavior.

The advanced update path uses full three-dimensional listener distance for the
fields named `InnerRadius2D` and `OuterRadius2D`. At and inside the inner radius
it sets FMOD's 3D pan level to zero, beyond the outer radius it sets the level to
one, and between them it interpolates linearly. This blends a positioned voice
from two-dimensional to three-dimensional panning; it does not select a lower-
or higher-resolution asset.

The same update evaluates `TimeA` through `TimeD` against game-clock
milliseconds within the realm day. A sound is absent outside a nonempty window,
ramps from zero to one between A and B, remains at one until C, and ramps back to
zero at D. Midnight-crossing windows move early-day samples into the following
integer day. The constructor selects one signed offset from
`[-RandomOffsetRange, RandomOffsetRange)` using the process-wide Blizzard table
generator at executable `0x00464580`. Runtime retains that exact 61-word table,
four prime-length cursors, accumulator, and timer seed separately from the CRT
`rand` stream. Stock shifts the window comparisons by that offset but retains
the unshifted time points in interpolation numerators; `scheduled_gain`
preserves that executable behavior.

`SoundInterface2.cpp` at executable `0x004c6a40` resolves the ordinary default
loop state from `SoundEntries.Flags & 0x200`. Request state can explicitly force
looping or one-shot playback; advanced usage-zero continuous objects force the
former, while periodic and terminal objects force the latter.

`AdvancedSoundService` now owns usage-zero continuous, usage-one periodic, and
usage-two terminal instances; shared variation state; scheduled gain; camera-
relative position and cone mixing; and the global category-duck list. Completed
service-owned generations are released before a later instance can reuse the
same backend track. Whole-world and ADT replacement boundaries stop and destroy
all earlier instances explicitly.

The runtime composition root opens the default SDL output after Glue CVar
registration, reads the live master/SFX/music/ambience snapshot, and allocates
the fixed startup `Sound_NumChannels` pool. A live capacity change requires a
restart instead of resizing the backend. Resident MCSE records are staged in
authored chunk order and updated from the rendered `WorldCameraFrame` and the
server-anchored `RealmClock`; neither local wall time nor another listener is
substituted. MCSE payloads use the same recovered extension/size residency rule
as every other stock sound. A larger HD archive override can therefore cross
the normal stream threshold without introducing an HD-specific path or mode.
