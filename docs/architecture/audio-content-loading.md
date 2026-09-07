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
the resident static liquid providers, including registered interior groups.
Vehicle/passenger policy and transport liquids still require their owners.

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
the local channel limit, and cancellation across disconnect. Remote ground
movement now publishes jump/land notifications through this same callback path.

SDL slot admission reports every replaced voice generation, including a voice
that finished naturally while the next payload was loading. The engine retires
that generation and releases its decoded reference before publishing the new
voice. A memory-output regression completes a sample between load reservation
and completion, then applies live settings; the old behavior retained a stale
backend handle and terminated the world event loop on the following update.

## Zone selection and replicated world-state audio

`ZoneSoundCatalog` loads the exact ZoneMusic, ZoneIntroMusicTable, and
SoundAmbience layouts. AreaTable and WMOAreaTable provide five independently
inherited relations. The common location projection retains separate zone,
subzone, WMO-root, and WMO-group IDs, including each placement's name set.
The native `0x0078E9A0` precedence is group, root, subzone, zone, with the area
branches suppressed for an exclusive interior WMO registration.

`ZoneSoundState` implements the eleven native priority layers, realm-clock
day/night columns, normal music delays, introduction cooldowns, and explicit
zero-entry overrides. `ZoneSoundService` owns asynchronous payload generations,
native music and ambience crossfades, revival of a fading previous generation,
and completion callbacks. The engine updates fades before selection and moves
ordinary positional voices against the current listener every world frame.
World FrameXML media actions now reach the runtime sound owner.

`SMSG_INIT_WORLD_STATES` (`0x2C2`, reader `0x0052693A`) carries map, zone, area,
a u16 count, and ordered u32 field/value pairs. `SMSG_UPDATE_WORLD_STATE`
(`0x2C3`, reader `0x005269E5`) replaces one field. `0x00548970` changes the UI
location filter; initialization does not erase omitted hash entries.
`WorldStateValues` preserves that merge behavior, duplicate-key wire order,
raw value bits, and `0x00548D10`'s zero for absent keys. Map replacement moves
this session-global owner into the destination world. Both setup and live
dispatch apply the packets before world audio samples their conditions.

`WorldStateZoneSounds` retains file order and its native eight-word, non-ID
schema. The selector follows `0x004CBE70`, including its distinct mixed-WMO
branch, and updates normal layer 1 and introduction layer 6 when the selected
relations change. WorldChunkSounds uses `0x004C6810`'s float stores and masked
chunk tuple. The original-executable fixture generator
`tools/ghidra/zone_sound_oracle.py` covers inheritance, time boundaries, fades,
chunk coordinates, and world-state rule precedence. Separate playback tests
exercise actual decoded PCM and cancellation; an encrypted-session test covers
state-dependent music conditions and transfer lifetime.

The effects provider remains selected metadata until the DSP owner is wired.
Underwater ambience receives the camera's submerged LiquidType ID each world
frame. `0x00795D40` first limits a 1760-unit downward ray by terrain and runs
`0x007D59B0`'s independent camera interior banks. Its selected WMO group is the
sole liquid provider in `0x00790920`; without one, the general query visits
resident WMO roots before terrain. Terrain uses bilinear MH2O heights, authored
holes, and `0x007AD3B0` floor rejection. WMO grids use row-major MLIQ addressing,
`0x007C8360`'s bilinear interpolation and LiquidType-dependent tolerance.
The audio owner compares IDs as `0x004C8630` does, retaining liquid-to-liquid
changes and selecting underwater ambience 4209. Liquid rendering and swimming
remain prerequisites for the broader water implementation.
`tools/ghidra/liquid_query_oracle.py` captures complete native WMO liquid and
camera root queries; external tests compare 512 liquid cases and 256 camera
selection cases with decoded archive fixtures. Runtime terrain tests cover
camera entry, exit, and rejection below the terrain floor.

## Output options and explicit restart

The game-output menu receives the actual SDL playback device catalog. Native
`0x008783B0` places a localized `SYSTEM_DEFAULT` entry at index zero, followed
by physical drivers; an unattached headless UI retains zero drivers. The
`Sound_OutputDriverIndex` Lua callback saves the corresponding driver name
(`0x004D0DD0`) without restarting playback. Startup and restart reconcile both
saved fields using `0x008790C0`: retain a matching pair, otherwise search by
name, then select the system default if the device disappeared.

`Sound_OutputQuality` defaults to one (`0x004D1050`). `0x0087C710` requests
22,050 Hz for zero, 44,100 Hz for one, and 48,000 Hz otherwise. Both UI
lifetimes queue `Sound_GameSystem_RestartSoundSystem` (`0x00985D30`) to apply
the live device, quality, and software channel settings. Native restart stops
voices and cancels pending loads (`0x0087DED0`), while retaining logical
channel metadata (`0x0087B490`) and the advanced/zone owners. The media owner
keeps old handles queryable as stopped until those owners retire them normally;
late archive completions cannot resurrect pre-restart voices.

The SDL adapter opens its replacement before destroying the old tracks and
output. It needs no native FMOD settling sleep. Tests verify all three actual
memory-mixer rates, silence and cancellation after restart, fresh audible
playback, localized Lua enumeration, saved-name reconciliation, and ordered
restart dispatch. Invalid Lua driver indices return an empty string instead
of reproducing native unchecked memory access. FMOD's high-quality resampler
algorithm and automatic physical-device hotplug restarts remain unimplemented.

## Model callback sound ownership

`0x0070C050` and `0x007BD5A0` retain one `$DSL` handle per game object or
doodad. Both explicitly force looping at options `+0x1C` while retaining the
default random variation selector at `+0x18`. Game objects fade in/out over
one second; doodads fade in over three seconds and stop immediately. `$DSE`
belongs to the doodad callback. `0x004CFE00` suppresses a matching playing or
paused entry strictly within squared distance 6, excluding pending resources.
The original callback capture is reproducible with
`tools/ghidra/model_sound_oracle.py`; its fixture distinguishes these option
offsets without relying on decompiler local-variable names.

Rendering placements lazily own their model sound lifetime. Callback events,
pending reads, and retained voices carry weak references, so an event queue
cannot preserve a removed or replaced model. Removal cancels pending work or
applies the callback's stop fade, leaving any fade tail with the engine.
`0x008793C0` copies the callback position; ordinary model loops retain that
world origin while the listener mix changes. Model payloads now use the same
ordered archive and decoder workers as other runtime sounds.

The SDL adapter supplies infinite looping through `MIX_PROP_PLAY_LOOPS_NUMBER`
when starting a track. Setting `MIX_SetTrackLoops` before `MIX_PlayTrack` loses
the requested count because playback initialization resets it. The backend
test generates beyond the input duration for both physical and virtual loops,
then reuses a slot for a one-shot. An archive integration test checks audible
model loops, duplicate admission, pending cancellation, replacement identity,
and both stop policies. See the upstream
[loop setter contract](https://wiki.libsdl.org/SDL3_mixer/MIX_SetTrackLoops).

`$CSD` follows `0x00746D60`: emote and pet gates, the player's health guard,
local priority 110 and gain multiplier 0.65, entry loop policy, and a retained
unit voice which replaces its predecessor. It starts from attachment 17
(`0x00831330`, without the attachment enable track) or origin plus world Z*2.
The GUID update callback (`0x00879F70` / `0x004C5D60`) subsequently uses the
unit's current origin. Generic model dispatch no longer plays a second copy.
Class-specific DSP filters and the player-only internal guard at unit +0x1944
still require their owning actor/effects state.

Focus loss applies `0x004C5DC0`'s background-sound CVar to the output bus.
`0x008794A0` and `0x0087A8E0` change gain while timelines continue; focus
muting therefore does not reject pending requests or pause individual voices.
Output restart preserves this independent gain. Minimized presentation keeps
music selection, fades, and emitter servicing active with the retained listener.

SoundEntries flags `0x800` and `0x400` enable volume and pitch variation.
`0x00982310` constructs its random fraction from the low 23 random bits.
Volume adds a value in [-0.15, 0.15) after the caller's source multiplier,
then `0x00879710` clamps the result to [0, 1]. Pitch selects [0.85, 1.15).
The executable fixture from `tools/ghidra/sound_parameters_oracle.py` covers
200 cases, matching result bits and random draw order. Mixer admission applies
frequency, gain, and loop options before playback and clears a reused track's
previous spatial panning.

Ordinary positioned loads retain their world origin before decoding. Admission
uses the current listener and applies distance gain and pan before the first
mixed sample; pending unit vocals update their bound origin while loading.
Memory-output regressions check the initial distance gain, pitch-dependent
playback duration, and pitch reset when a track is reused for direct-file audio.

## World-entry and distance regressions

Stock does not use FMOD's default inverse-distance curve. Initialization at
`0x0087DDF3` installs `0x00878320` as the system rolloff callback through
`0x008D1540`; the callback reads the channel's minimum and maximum distances
and calls `0x008782B0`. That kernel returns zero at or beyond the maximum,
one within the minimum, and otherwise
`minimum / ((distance - minimum) * 4 + minimum)`. Above 90% of the maximum,
it multiplies by a linear taper to silence. The 90% constant is stored as f32
and extended for the x87 calculation. Cutoff comparison precedes minimum
comparison, including equal and zero endpoints. The executable oracle in
`tools/ghidra/sound_distance_oracle.py` supplies 88 exact output-bit cases.
Both ordinary positioned voices and advanced emitters use this callback.

Character entry in `0x004DAB40`, state 10, calls `0x009860E0(3.0)` at
`0x004DB91F`. This removes the Glue music repeat callback and releases its
voice with a three-second fade. Glue ambience retains its default zero
fade-out. Runtime drains the final Glue actions before this handoff, cancels
pending music loads, and lets the engine own only an already-playing tail.

FrameXML initialization at `0x0052A980` and world-entry dispatch at
`0x00528010` bracket non-positional SoundEntries admission with
`0x004CFB80` and `0x004CFB90`. `0x004C6A40` checks the counter through
`0x004CFBA0` and rejects a null-position request while it is nonzero.
`PlaySound` therefore does not enqueue startup sounds for later replay.
Direct-file playback through `0x004C9110` bypasses this particular gate.
Scoped UI admission restores the enclosing state after nested calls and Lua
errors without modifying CVars or already-playing voices.

The adapter now queries playback once per rebalance and skips unchanged SDL
gains. Promoting only voices already inside the real set cannot change its
membership, so the second sort and playback scan were redundant. Engine
runtime and ducking updates also skip unchanged gains. The manual
`backend_many_voice_gain_benchmark` exercises 128 changing voices in the
512-slot pool; the debug memory-mixer measurement fell from 30.95 to 9.14 ms
per update on the development machine. This isolates mixer work and does not
measure end-to-end game frame time.
