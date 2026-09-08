# Build-12340 character creation

These contracts come from the fingerprinted executable described in
[the stock evidence instructions](../../tools/ghidra/README.md). SolCL's
implementation is not the authority for these behaviors.

## Random selection

Creation uses the shared Blizzard random stream at `0x00464580`, distinct from
the CRT stream used by model animation. The selectors multiply a 32-bit word by
the number of choices and keep the upper 32 bits (`MUL` and a 32-bit right shift,
for example at `0x004EB416`). An empty selector consumes no random word. Skin,
face, hair color, and hair style retain their previous values in that case;
facial features still map ordinal zero through the branch described below.

The entry points consume values in different orders:

| Entry | Recovered behavior |
| --- | --- |
| `ResetCharCustomize`, `0x004E1FD0` | Roll sex, then race; initialize an ordinary class-zero appearance; roll class; validate the appearance for that class |
| Fresh race/sex model, `0x004E13A0` | Start at zero; roll skin, hair color, hair style, face, facial feature |
| Randomize button, `0x004E17F0` | Roll skin, face, hair color, hair style, facial feature |

Race selection at `0x004DFF10` draws from the whole race list and retries an
expansion-locked result. Filtering that list before rolling would change both
the selected race and subsequent random state. Class selection at `0x004E0F50`
uses the expansion-admitted classes in `CharBaseInfo.dbc` order, established by
`0x004E1ED0`; `ChrClasses.dbc` order remains the separate Lua class-list order.

Hair color is selected for the current hair style. Hair style is then selected
from the styles supporting that new color. Face selection uses the new skin.
Facial-feature selection uses the new hair color.

## Appearance persistence and validation

The race and sex setters (`0x004E20B0`, `0x004E1540`) cache one appearance per
race/sex pair. Class is not part of that cache key. A restored appearance keeps
the current selected class and passes through validation.

The class setter at `0x004E1740` copies the current appearance and calls
`0x004E9D50`. Valid values remain unchanged. Invalid values use deterministic
ordinal/remainder mapping through the new class's available choices; this
validation does not consume random words. It does not roll a fresh appearance
for every class.

`CycleCharCustomization` dispatch at `0x004E01F0` uses the sign of its delta.
A zero delta does nothing; a larger magnitude still selects one neighboring
choice.

Skin cycling at `0x004EB150`/`0x004EB290` preserves the current face and skips
skins missing that face or underwear. Hair-style cycling at
`0x004F0490`/`0x004F0630` preserves color when the neighboring style supports
it; otherwise it selects that style's first eligible color and corresponding
first facial feature. These operations consume no random values.

Face cycling at `0x004EB710`/`0x004EB990` scans all authored face variations,
including those confined to another skin range. It retains the current skin
when the candidate supports it; otherwise it searches that face's allocated
skin range using the last explicit skin choice (`0x00B6B18C`). The stock loop
adds this origin before each remainder operation. The search origin is reset
by skin cycling, randomization, and model setup, but not by face cycling. This
allows the Gnome Death Knight selector to cross between faces `7..13` on
ordinary skins and faces `0,2,3` on exclusive skins in the installed data.

## Rendering selected customization

Creation eligibility does not filter the component rendering bank. Stock
`0x004F3DD0` includes all `CharSections` rows and overwrites duplicate keys in
physical order; `0x004F3BA0` looks up race, gender, section, variation, and color
without a class argument. Selection previews and server-supplied players use
those exact bytes even when the appearance would not be offered by creation.
Class-specific equipment and geoset decisions remain independent.

## Eligible section data

The creation filter is selected by `0x004F3A40` and evaluated by `0x004F39A0`:

- ordinary classes require player bit `0x01` and reject bits `0x04` and `0x08`;
- death knights require player bit `0x01`, either `0x04` or shared bit `0x10`,
  and reject bit `0x08`.

Skin counting at `0x004E7B80` examines skin rows; the presence of underwear is
not a prerequisite for a selectable skin. Facial-feature counting at
`0x004E7DF0` uses eligible texture variations for the current hair color when
the race/sex has a facial texture array. When that entire array is absent, it
uses the authored geometry-feature count. Missing one color does not trigger
the geometry-only branch.

Facial ordinal mapping at `0x004E80E0` separately checks the current feature's
allocated color range (`0x004F3BA0`). Outside that range it uses a geometry
ordinal. Inside it, the texture scan is bounded by the filtered count, so holes
are not compacted into a new index space. Facial cycling at
`0x004EBCA0`/`0x004EBE80` also branches on this range: geometry cycling retains
color, while textured cycling can choose the next feature's first eligible
color. Randomization and hair-style cycling retain the feature when this
ordinal mapper returns a negative result. Validation instead stores that
negative result; renderer handling of that sparse-array case remains a research
gap, and Solarity reports a missing-choice error at that boundary.

Deterministic UI tests verify selected fields and the next shared random word,
including rejection of locked race rolls, distinct class orders, class changes,
and race/sex cache ownership. Data-layer tests cover the exact flag masks and
the distinction between missing facial colors and geometry-only features.

The installed-data representation validator passes 124 race/class/sex outfits,
906 customization representations, the Gnome bald/facial-hair regression, and
the complete selection equipment/pet case, including selection-to-world reuse.
These checks establish composition coverage rather than visual or timing
parity. Ordinary and death-knight choices are cached once per race/sex, and
customization borrows those choices without cloning their nested vectors.

## Native choice buttons

The XML name `CheckButton` resolves through factory `0x00812630` to
`CSimpleCheckbox`, whose constructor is `0x009620E0` and vtable is
`0x00A9F988`. Its click override at `0x009623C0` toggles only its own checked
field at offset `0x2F8` through `0x00962340`, then calls Button's dispatch at
`0x0096FD70`. There is no native name-based group selection. The exact
`Interface/GlueXML/CharacterCreate.lua` handlers enforce race/class/gender
selection with `SetChecked` calls.

Script `Click` at `0x00978260` enters this same virtual override. Checkbox
toggling precedes the base enabled/recursion guards, so a disabled scripted
checkbox click changes its checked state without running handlers. A recursive
click on the same checkbox toggles again while the active script dispatch
remains guarded. The guarded base callback at `0x0096F090` invokes PreClick,
OnClick, and PostClick in order. Pointer and script clicks share this path in
Solarity; deterministic tests cover ordering, disabled clicks, recursion,
explicit Lua group selection, and guard cleanup after a handler error.

`CharacterCreateIconButtonTemplate` in the exact `CharacterCreate.xml` moves
its bevel by `(2, -2)` and resizes its shadow from 58 to 52 units on press,
then restores both on release. These mutations now retain their texture slots
and update dependent texture anchors. A non-texture dependency that cannot
use its retained text slots still requires complete publication. Geometry
comparison uses the renderer's `f32` coordinates to avoid treating `f64`
dimension-writeback rounding as movement. Replacing an absolute source mesh
also retires its prior object translation, preserving separate scroll offsets
and any decorations whose vertices were not replaced.
