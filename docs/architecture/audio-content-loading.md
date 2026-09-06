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

The per-frame cache pass at executable `0x0087b010` reads
`Sound_MaxCacheSizeInBytes`, whose default is 16 MiB. Values below 4 MiB use a
4 MiB floor; values above 100 MiB select 128 MiB; intermediate values remain
unchanged. Unreferenced predecoded samples are evicted until encoded-byte
accounting meets that effective budget. Playing samples remain resident even
when they alone exceed it.

The pinned safe SDL wrapper copies the original encoded file into its `Audio`
resource during admission. This is the one adapter-boundary copy: the temporary
`IOStream` retires immediately, and the encoded cache can collect its source
once other owners release it. Unsupported or corrupt bytes fail admission and
do not trigger another codec, filesystem search, or extension substitution.

Stock admission is nonblocking: `SoundEngine.cpp` at `0x0087ee60` ORs
`0x82010000` into the mode passed to both FMOD creation branches. This includes
`FMOD_NONBLOCKING` (`0x00010000`); its completion callback is `0x0087b180`.
FMOD documents background opening and readiness polling in its
[creation API](https://www.fmod.com/docs/2.03/api/core-api-system.html).
`SoundEngine::begin_load` therefore separates ordered definition validation,
channel/exclusive admission, and variation selection from payload extraction.
The pending reservation counts toward channel and same-entry limits. Completion
consumes that reservation exactly once, applies current gain settings, and
admits the exact selected path without another random draw. Category stop and
explicit cancellation retire pending reservations as well as playing voices.
Late or foreign completions cannot allocate a decoder or start a track.

Glue music and ambience use these reservations with the existing bounded CPU
executor. A worker-private archive stack mounts the already discovered catalog
once, retains its original precedence, and serializes selected reads. Capacity
pressure retains the selected request for later admission. Replaced queued
requests are skipped; an already running read remains owned and its result is
observed even after cancellation. Application shutdown cancels queued requests
and joins the active read before shutting down the pool. SDL resources remain
registered on the output-owning thread. The same bounded CPU executor then
prepares a new decoder resource, including MP3 seek tables and any complete
sample predecode. `SoundEngine::poll_load` keeps the selected reservation and
payload while the pool is full or decoding is unfinished, then publishes the
completed resource and starts its track with current gain settings. Reusable
samples already in the registry complete without another worker submission;
concurrent sample completions recheck that registry, while streamed voices keep
their independent resources. Ordinary UI and world sound callers retain the
immediate API, which uses the same selection, reservation, and completion rules.

SDL documents audio loading, inspection, and destruction as safe from any
thread; `MIX_Init` and `MIX_Quit` are not thread-safe. The pinned Rust wrapper
marks its general `Mixer` and `Audio` types as non-transferable. Media therefore
owns a narrow private transfer boundary: a worker receives only shared ownership
of a memory mixer for `MIX_LoadAudio_IO`, and returns one exclusively owned,
validated audio resource. The original mixer owner retains every task and joins
them before releasing its own reference, keeping the final mixer/library
destructor on the owning thread. No track or general mixer API crosses this
boundary. Registry audio objects also drop before that initialization reference.
Cancelled jobs remain owned until their results can be discarded; normal frame
collection polls them, while explicit shutdown joins them. These lifetime and
thread contracts are specified by SDL's
[loading API](https://wiki.libsdl.org/SDL3_mixer/MIX_LoadAudio_IO),
[audio destruction](https://wiki.libsdl.org/SDL3_mixer/MIX_DestroyAudio), and
[library shutdown](https://wiki.libsdl.org/SDL3_mixer/MIX_Quit).

External tests hold the worker queue at known capacity, cancel queued decodes,
verify actual muted/unmuted PCM after asynchronous completion, check sample
deduplication versus per-play streams, reject corrupt bytes, and drop an engine
with outstanding work before reinitializing SDL. No elapsed-time assumption is
needed to establish those ownership and admission outcomes.

Output and track ownership remain separate at the adapter boundary.
`SoundOutput` owns one explicitly selected default-device or memory mixer;
`SoundBackend` borrows that output and preallocates build 12340's hard 512
virtual voices. `OwnedSoundEngine` contains their self-reference behind one
tested safe boundary: a stable shared output allocation outlives the engine
field and Rust's field destruction order drops every track first. Its read and
write callbacks must accept any output lifetime, and their result cannot carry
that lifetime out of the callback. The owner exposes no `Deref` or `DerefMut`
access to its internally retained engine. The earlier mutable exposure allowed
safe callers to swap engines between output owners and then destroy an output
still borrowed by the surviving engine. Compile-fail regression tests now
reject that exchange, scoped exchanges, escaping references, and insertion of
an engine borrowing a shorter-lived output. Runtime policy separately passes
the authoritative `Sound_NumChannels` real software
mix count, whose build-12340 default is 64 and whose executable clamp is
12 through 128. The CVar is read only during sound initialization; a later
change has no effect until the next initialization.

SDL has no FMOD-equivalent virtual channel. Every logical voice therefore
retains a timeline on an SDL track, while voices outside the real software set
advance at zero adapter gain. FMOD priority buckets select that real set: zero
is most important, 256 least important, and the untouched signed option word
`-1` retains FMOD's default 128. Within one bucket the voice with greater
current audibility remains real. Only exhaustion of the hard 512-voice pool
reuses the weakest generation and reports that stolen handle to the engine for
immediate decoder-reference retirement.

The per-frame stock pass also distinguishes default one-shots from loops. A
non-looping voice whose authored priority word is negative or exactly 128 is
promoted from 128 to 127 only after FMOD reports it playing and non-virtual.
Virtual and looping voices do not take this promotion, and positive words above
256 retain FMOD's default without entering that branch.

Each voice handle carries the backend, slot, and slot generation. Stopping a
voice permits reuse, and the next generation invalidates the earlier handle.
Playback accepts finite nonnegative gain without clamping amplification and
retains explicit one-shot versus infinite-loop behavior. Pause, resume, stop,
gain changes, memory mixing, and state queries remain backend primitives;
category buses, spatialization, DSP, and fades belong to the stock-facing
sound-engine layers above this adapter.

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

The same path retains the exact `VolumeSliderCategory`/call-site channel index
instead of collapsing every SFX-labelled entry. Channels zero through five use
an effectively unlimited channel-layer cap. Channels six through seventeen use
fixed simultaneous-instance caps of `1, 1, 2, 1, 2, 2, 1, 6, 4, 1, 2, 4`.
That check runs before variation selection or payload admission. The default
same-entry concurrency rule comes from `SoundEntries.Flags & 0x20`; explicit
play options can force or suppress exclusivity.

`AdvancedSoundService` now owns usage-zero continuous, usage-one periodic, and
usage-two terminal instances; shared variation state; scheduled gain; camera-
relative position and cone mixing; and the global category-duck list. Completed
service-owned generations are released before a later instance can reuse the
same backend track. Whole-world and ADT replacement boundaries stop and destroy
all earlier instances explicitly.

The runtime composition root opens the default SDL output after Glue CVar
registration, reads the live master/SFX/music/ambience snapshot, allocates the
fixed 512 logical voices, and applies `Sound_NumChannels` as the startup real
software count. A live software-count change requires a restart instead of
rewiring the backend during an active session. Resident MCSE records are staged in
authored chunk order and updated from the rendered `WorldCameraFrame` and the
server-anchored `RealmClock`; neither local wall time nor another listener is
substituted. MCSE payloads use the same recovered extension/size residency rule
as every other stock sound. A larger HD archive override can therefore cross
the normal stream threshold without introducing an HD-specific path or mode.

## Retained live sound policy

An unchanged CVar snapshot no longer reapplies every active voice's gain.
Each backend gain update rebalances the 512-slot logical pool, so the previous
per-frame settings application repeatedly scanned and sorted the pool and
entered SDL mixer operations during otherwise idle frames. Voice admission and
runtime gain changes already apply the current policy at their own boundaries.

Settings snapshots become current only after all changed gains are applied,
keeping a partial backend failure retryable. Unchanged snapshots still collect
naturally stopped unmanaged voices and enforce the decoded-cache budget.
Memory-output tests cover retirement through an unchanged snapshot and
preservation of independent runtime muting.

With identical frame diagnostics enabled, a 4,000-following-frame Glue replay
measured warm Human selection at 3,574 FPS versus 2,395 before this change.
Normal creation/customization scenes ran around 3,400–3,500 FPS, confirmed in
a subsequent 6,000-following-frame replay. These are local GTX 1070, 1280x720
measurements; transition frames and heavier scenes are reported separately in
[Glue completion](glue-completion.md).

## Unit movement audio

World presentation now dispatches authored `$FSD` callbacks and frozen local
jump/landing notifications to UnitSound_C's movement routes. Display sound
overrides precede model sound rows; mounted sound rows and authored child rows
retain their precedence. `CreatureSoundData` supplies the footstep selector and
optional jump/land entries. A zero entry remains silent: the ordinary Blood Elf
player row has no jump/land vocal, while other authored displays do.

Footsteps join `TerrainType` and `FootstepTerrainLookup`, including the original
terrain-zero retry and first-authored-row precedence. ADT sound cells consume
all sixteen packed selection bytes and MCLY's ground-effect key. The original
`0x007A0530` instructions supply 1,320 boundary, hole, and packed-layer fixtures.
WMO ground types use the existing native registration and fallback-face banks
and the selected MOMT material. Wet entries consume liquid behavior flags,
area/parent substitutions, foot height, and the water-walking exclusion through
the resident static liquid providers. This does not complete registered interior
group/area liquid selection, vehicle/passenger policy, or transport liquids.

Armor foley joins the player's chest Item.dbc material or the creature model's
foley material to `Material.dbc`'s third column. The original `0x004CFC10` table
owner at `0x00AD41A8` resolves through its vtable and loader to `Material.dbc`;
`ItemGroupSounds.dbc` supplies unrelated inventory pickup/putdown cues. Authored
zero foley entries remain silent. Server item-cache material overrides remain
outside this path.
Hover, ghost, and the movement flight flag suppress ordinary footsteps/foley.
`FootstepSounds` and both armor-foley CVars apply live alongside the world UI's
master/category sound settings. Local footsteps use channel 17 and priority 115;
other footsteps use channel 13. Local vocals/foley retain priority 110 and the
0.65 gain when `Sound_ListenerAtCharacter` selects nonpositional playback.

Movement requests retain native random mode 2 on the separate Blizzard stream.
Selection and voice reservation occur in event order, then the existing audio
worker extracts and decodes the selected resource without holding the render
thread. World frames poll completions, and disconnect cancels pending movement
loads and stops their admitted voices. Channel/exclusivity refusals and payload
failures are contained at the unit callback boundary. The memory-output archive
test checks decoded samples, live footstep suppression, hover/ghost admission,
the local channel limit, and cancellation across disconnect. Remote jump/land
notifications still depend on the remaining remote movement packet integration.

SDL slot admission reports every replaced voice generation, including a voice
that finished naturally while the next payload was loading. The engine retires
that generation and releases its decoded reference before publishing the new
voice. A memory-output regression completes a sample between load reservation
and completion, then applies live settings; the old behavior retained a stale
backend handle and terminated the world event loop on the following update.
