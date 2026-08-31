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

The schema is checked against the public build range covering
3.1.0.9767 through 3.3.5.12340 in the
[WoWDBDefs SoundEntries definition](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SoundEntries.dbd).
