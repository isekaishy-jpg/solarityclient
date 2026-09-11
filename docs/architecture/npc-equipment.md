# Build-12340 NPC equipment

Ordinary Unit_C armor uses the same component machinery as player armor.
Initialization at `730100` walks the eleven display identifiers at
`CreatureDisplayInfoExtra +20..48`, in component order, and passes each nonzero
identifier through `4F2830`. That routine reads `ItemDisplayInfo` and dispatches
to `4F2640`. Component zero creates a helmet on link 11; component one creates
the shoulder pair on links 6 and 5. The other armor components supply body
textures, geoset decisions, and the cape replacement rather than separate M2s.

The runtime NPC appearance path now prepares a `CharacterAttachmentPlan` from
those display-only items, using the Extra row's race and gender for the helmet
filename. The existing model/texture/effect loader supplies resident children.
The race-prefix lookup runs only for a nonempty helmet model; an NPC without a
helmet does not acquire a new dependency on `ChrRaces.dbc`.
NPC GPU publication calls the shared equipment preparation routine after the
body; `UnitItem` and `UnitItemVisual` identify children of either player or
creature bodies. Attachment transforms sample the current body and item bones.
The children share the owning unit's opacity and use the environment light
bank. Removal selects children by their actual body's GUID, so replacing
creatures cannot remove player equipment, or vice versa.

Unchanged armor display identifiers preserve their existing components during
unrelated body updates, including deliberately one-sided shoulders. Changed
armor inputs follow `4EF020` and `4EF710`: helmets compare their model path, and
shoulder reuse requires both members of the pair to match. NPC armor retains its
actual display identity separately from public player item entries. Retained components preserve their
source resources, animation timers, particles, ribbons, and attached visuals.
A new unit lifetime with the same GUID receives new components; removed visible
hierarchies can finish their existing disappearance interpolation independently.

The scene regression covers a remote player and two armored NPCs, bone-parent
selection, shared opacity, lighting-bank selection, material/body-scale
replacement, equipment removal, and GUID reuse. The opt-in stock archive test
passed all 522 selected displays: male Goblin appearances and four armored male
Orcs (1836, 1906, 4386 and 4515), including one-sided armor and complete shoulder
pairs, through model loading and visible GPU draw preparation. These checks do not
replace the user's combined Durotar screenshot comparison.

## Remaining held-item consumer

The three Wrath `UNIT_VIRTUAL_ITEM_SLOT_ID` fields (56 through 58) are **item
entries**, unlike the eleven Extra-row armor display identifiers. Native update
`725010` reads the fields at `UnitFields +C8 + 4*slot` (the unit field base follows
the six object fields), joins `Item.dbc`, retains its display ID and compact item
metadata, and updates the held component. Initialization also calls `72DBC0`
for slots zero, one, and two.

NPC virtual items are not yet connected to runtime residency. That work must
preserve the native unit-specific selection rules, including readiness,
off-hand suppression, ranged replacement, missing attachment admission, and
the item update path. Reusing the player held-item plan alone does not prove
those behaviors. The shared child renderer is ready for their resolved inputs;
the missing weapon reported in the Durotar comparison remains open.
