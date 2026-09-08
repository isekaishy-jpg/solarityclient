# Unit aura ownership and resurrection admission

Native handler `7300A0` owns `SMSG_AURA_UPDATE_ALL` (`495`) and
`SMSG_AURA_UPDATE` (`496`). Each starts with a packed unit GUID and an ordered
stream of byte-indexed slots. `716510` reads spell ID, flags, caster level and
application count. A zero spell ID removes that slot with no remaining fields.
Flag 8 supplies the owning unit as caster; otherwise a packed caster GUID follows.
Flag `20` adds maximum and remaining duration words.

`72F5D0` clears the old bank for `495` and preserves it for `496`. Repeated
indices apply in wire order. Its slot-removal path clears only the spell word;
full replacement clears the complete records. `UnitAuras` retains these native
semantics and supports all 256 indices. The expiration timestamp uses wrapping
receipt plus remaining milliseconds, replacing zero with one. The client does
not remove an authoritative aura merely because that timestamp has passed.
Unknown unit GUIDs do not create owners, and entity replacement drops the bank.

`727E70` constructs the active aura-type set from the three enabled effect bits
in each slot and its Spell record's aura types. `SpellEffectCatalog` retains the
effect triplets and resurrection attribute from the exact build-12340 layout.
Missing Spell records do not contribute an aura type. The native bit for type
314 drives `CannotBeResurrected`. `727860` admits release when the blocker is
absent or the replicated self-resurrection spell has word 11 bit `08000000`.
The latter bypass does not erase the blocker reported to Lua.

The session freezes these values in its ordered player UI notifications and
publishes them before the associated life callback. Local aura packets dispatch
`UNIT_AURA` after updating the resurrection projection. The native slot and
resurrection-admission oracle has 120 samples. Encrypted integration tests check
the retained records, exact valid prefix boundaries, restriction changes,
unknown spells, GUID replacement and wrapping deadlines.

On raw-health death, `6DC0F0` checks dynamic field 79 bit `20`, then initializes
the local release timer. Unit field 59 bit `100000` requests forced release
through `6D2950`, with byte zero, before the death combat record. The runtime
freezes this request's aura admission at that callback. Its queue shares the
existing bounded encrypted death-action writer. Ninety-six native callback
probes cover local/remote, signed health, ghost, field and restriction gates;
runtime tests apply the 64 cases that can enter through a health-death callback.

These owners establish replicated aura state and resurrection admission.
Aura visuals, spell casting, complete `UnitAura` filtering, and inventory item
fallback have separate consumers; they are not inferred from the wire flags.
