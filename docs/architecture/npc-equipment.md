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

## Virtual held items

The three Wrath `UNIT_VIRTUAL_ITEM_SLOT_ID` fields (56 through 58) are **item
entries**, unlike the eleven Extra-row armor display identifiers. Native update
`725010` reads the fields at `UnitFields +C8 + 4*slot` (the unit field base follows
the six object fields), joins `Item.dbc`, retains its display ID and compact item
metadata, and updates the held component. Initialization also calls `72DBC0`
for slots zero, one, and two.

`UnitVirtualItems` now projects these three entries and retains the other hands
during sparse updates. NPC residency joins Item.dbc and ItemDisplayInfo, with
missing entries/displays omitted as in `725010`. The selected models reach the
same loader, item visuals, particle colors, GPU attachments, environment light
bank and opacity owner as NPC armor. This also applies to ordinary creatures
without a `CreatureDisplayInfoExtra` row.

`NpcWeaponState` keeps the native consumed flags and the current AnimationData
behavior classes. `71F440`/`718FC0` distinguish main-hand disarm, off-hand disarm,
and ranged suppression. `72DBC0` suppresses an NPC off-hand beside the native
two-handed weapon subclasses, removes an unreadied ranged component, and uses
inventory types 25/26 to select the right ranged hand. `721ED0`/`715D00` adjust
readiness for the current body behavior. `4EACD0` selects the shield folder/link
only for the off-hand slot and checks the parent's authored link before loading
the child. The runtime likewise omits unsupported weapon links before requesting
assets. GPU preparation repeats that admission defensively.

The creature residency key holds entries and the small normalized weapon-state
value. Unchanged units do not reconstruct weapon paths or repeat item/display
joins each frame. Ordinary body updates retain unchanged virtual components and
their attached visuals; changing an entry resets that component even when its
display path matches. Finger-pose requests now include creature bodies.
The immediate relocation path in `7310A0` preserves the actual component while
moving it between hand and sheath links. GPU publication updates both the item
owner and its visual children's parent point, retaining their sources, clocks,
particles and ribbons without new initialization rolls.

The native capture executes 1,536 settled equipment cases through the actual
entry join, disarm/readiness predicates, component selection, and attachment-link
selection prefix. The rendering regression compares every captured slot/link
and checks authored paths, textures, visuals, and particle-color metadata. Scene
coverage exercises armored and ordinary NPCs together, sparse entry/flag/sheath
updates, unrelated component retention, entry replacement, opacity, and a body
without weapon links whose requested weapon asset deliberately does not exist.

The stock archive check passed 522 NPC displays with real virtual weapons and
48 additional armored-Orc sheath/disarm scenes. The selected stock entries are
sword 727, shield 143, bow 2504 and gun 1046.

Ordinary replicated sheath reconciliation in `738180` calls `736D30` with its
immediate flag, which dispatches the component relocation path through `731F40`.
The separate non-immediate branch starts upper-body sheath animations. That
branch and spell/combat-driven ranged model behavior are outside the settled
component-selection capture. AnimationData weapon-flag overrides and their
stateful interaction with effective sheath state still need connection to the
unit animation owner; the raw replicated byte alone does not cover them. The
combined Durotar comparison remains required before closing the user's report.
