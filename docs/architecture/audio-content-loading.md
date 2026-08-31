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
reports its authored sample rate and channel count. The caller explicitly
chooses streaming or complete predecode residency; the normalized path and that
mode form the decoder identity.

The pinned safe SDL wrapper copies the original encoded file into its `Audio`
resource during admission. This is the one adapter-boundary copy: the temporary
`IOStream` retires immediately, and the encoded cache can collect its source
once other owners release it. Unsupported or corrupt bytes fail admission and
do not trigger another codec, filesystem search, or extension substitution.

Output and track ownership are separate safe Rust values. `SoundOutput` owns
one explicitly selected default-device or memory mixer; `SoundBackend` borrows
that output and preallocates exactly the caller-provided track count. Runtime
policy must pass the authoritative `Sound_NumChannels` value, whose build-12340
default is 64. Exhaustion is reported instead of allocating another track or
stealing an active voice without an evidenced priority rule.

Each voice handle carries the backend, slot, and slot generation. Stopping a
voice permits reuse, and the next generation invalidates the earlier handle.
Playback accepts finite nonnegative gain without clamping amplification and
retains explicit one-shot versus infinite-loop behavior. Pause, resume, stop,
gain changes, memory mixing, and state queries remain backend primitives;
category buses, spatialization, DSP, fades, and voice priority belong to the
stock-facing sound-engine layers above this adapter.

`SoundEngine` is the first stock-facing orchestration layer. It owns the exact
`SoundEntries.dbc` catalog, encoded cache, decoder registry, backend track pool,
and retained voice policy. A request supplies its entry identifier, calling
category, already bounded variation ticket, decode residency, and loop intent.
The engine therefore does not infer category from an unevidenced `SoundType`
mapping, advance another RNG, reinterpret flags, or guess whether a payload is
music.

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
