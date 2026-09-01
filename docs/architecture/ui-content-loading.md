# Build-12340 UI content loading

The initial `solarity-ui` content boundary follows the built-in manifests from
the local 3.3.5a client. It does not generalize addon discovery or construct UI
objects yet; those layers build on this ordered source representation.

## Manifest behavior

The built-in entry points are exact archive paths:

| Client state | Manifest |
| --- | --- |
| Login, realm, character selection | `Interface\GlueXML\GlueXML.toc` |
| In world | `Interface\FrameXML\FrameXML.toc` |

Both inspected manifests use the same small grammar. Blank lines and lines
beginning with `#` are metadata or comments. Every other line names an XML or
Lua source relative to the manifest directory. Entry order is load order.

The TOC is only the first level of that order. While visiting each XML root,
the loader expands `<Include file="...">`, external `<Script file="...">`, and
inline `<Script>` at the exact point encountered. Relative directives may
lexically traverse to a sibling stock interface directory, as
`VideoOptionsPanels.xml` does for `..\FrameXML\GraphicsQualityLevels.lua`, but
the normalized result cannot escape the archive root. Recursive includes,
invalid extensions, and missing files are explicit errors. Nested `On*` event
handlers are compiled for Lua 5.1 syntax and retained in their XML object; they
are not mistaken for global scripts.

The loader therefore rejects unsupported extensions, absolute paths, and path
traversal. It does not search loose directories or try a second base path when
an entry is missing. All files pass through `AssetStore`, so ordinary patch and
HD archive precedence applies to UI files without a separate override system.

## XML ownership

`quick-xml` performs UTF-8 tokenization and XML well-formedness checks. The UI
crate copies the result into a compact arena whose child references are stable
indices. Names, normalized attributes, and ordered element/text content remain
available for the later template-inheritance and `CSimple*` construction
stages. Comments, declarations, processing instructions, and the document type
do not become runtime nodes.

The parser accepts XML 1.0 character and predefined entity references and
rejects unknown references. Schema interpretation remains a stock UI concern;
the tokenizer is not allowed to invent missing elements or attributes.

## Lua ownership

The workspace pins vendored Lua 5.1 through `mlua`. Every manifest Lua source
is compiled immediately in load order so syntax and bytecode compatibility
fail at the asset boundary. `UiScriptRuntime` then advances across the expanded
manifest one action at a time. XML actions expose only the object batch created
at that point; external and inline Lua actions execute in their original slots.
A Lua failure leaves the cursor on the failing action for a reproducible
diagnostic. Missing globals and methods remain errors rather than permissive
no-op functions.

The bundle retains both the source files and its Lua state, so execution never
reads the archives a second time. Bootstrap first installs the stock
compatibility aliases for Lua's table, degree-based math, and string libraries,
the error-handler pair, and aspect-compensated screen dimensions. The logical
SDL extent is required input; the stock UI coordinate space remains 768 units
high and derives its width from that real aspect ratio.

Object metatables are concrete per widget type rather than one permissive table.
The implemented surface includes identity, resolved dimensions, mutable sizes
and points, frame visibility and backdrop colors, all 41 Glue events,
button/slider enabled state, slider ranges, and scroll-frame offsets. Each root
batch registers its structural children in construction order and invokes their
`OnLoad` callbacks postorder before the root callback. Named callbacks resolve
their Lua global only when construction reaches that object. Like
`FrameScript_Object::RegisterScriptObject`, object registration does not
overwrite a non-nil global with the same name.

Virtual declarations are also expanded ahead of execution into owned runtime
prototypes. The local Glue corpus contains 71 templates expanding to 422
prototype objects. `CreateFrame` resolves one of those prototypes, allocates its
typed root and structural children, expands `$parent` names against the supplied
runtime name, preserves first-global ownership, and invokes nested `OnLoad`
handlers in construction postorder. This keeps dynamic dropdown and dialog
objects on the same metatable and callback path as archive-authored objects.

## Typed XML callbacks

Object construction converts each `<Scripts>` child into a callback slot from
the concrete build-12340 widget table. Common frame callbacks, button clicks,
edit-box input, scrolling, models, sliders, status bars, hyperlinks, movies,
color selection, and tooltip notifications each retain their exact stock
parameter names. A callback belonging to another widget type is an error; an
unknown `On*` element never becomes a permissive generic hook.

Every inline body is compiled as `return function(...) ... end` with its real
callback signature. This catches context errors that compiling the text as a
top-level Lua chunk cannot detect, such as using varargs in `OnLoad`; `OnEvent`
deliberately remains variadic. A `function="Name"` declaration is retained for
global lookup when ordered execution reaches the object. An empty handler
clears the inherited slot, matching the stock loader's explicit registry
unreference behavior.

The recovered client stores the compiled function on the XML handler node and
copies a registry reference into every instance. Solarity applies the same
ownership more compactly: one Lua registry function is compiled per unique XML
element, while flat per-object bindings reference it by `u32` index. This
avoids recompiling callback bodies across thousands of template instances
without sharing mutable callback slots. Real-client validation processes 1,443
Glue declarations into 1,363 active bindings and 482 unique inline functions;
FrameXML processes 14,506 declarations into 13,590 bindings and 1,883 unique
functions.

## Font boundary

Root-level `<Font>` objects are constructed in XML load order. Their face,
height, outline, monochrome mode, spacing, color, shadow, and justification
properties inherit only from global fonts already constructed. A missing or
forward parent is an error; the catalog does not search later documents to make
an invalid order appear to work. Values absent after inheritance remain absent
until the stock schema-default stage rather than receiving guessed defaults.

Font faces such as `Fonts\FRIZQT__.TTF` are resolved through `AssetStore` and
retained by the UI-thread FreeType owner. Each face is read once while pixel
height remains a glyph-level input, matching the stock XML's reuse of one face
at many sizes. Rasterization returns tightly packed 8-bit coverage plus exact
bearing and 26.6 advance metrics. Grayscale and one-bit monochrome bitmaps are
handled explicitly; other formats, corrupt fonts, and missing glyph assets are
errors instead of requests for an operating-system substitute.

## Object declarations

Every expanded root XML action is registered as either a virtual template or a
live root object. The type vocabulary is limited to concrete names observed in
the stock GlueXML and FrameXML data, including frame, button, texture, model,
tooltip, message, status, scrolling, movie, and world-root families. Unknown
root tags fail registration rather than being treated as generic frames.

Names are global and case-sensitive. `inherits` targets must already exist in
load order and object targets must be virtual; the catalog never searches
forward. Root `FontString` declarations may also inherit a previously
registered global font. Definitions retain references to their original XML
subtrees instead of duplicating hundreds of layout nodes before construction.
This keeps startup allocations bounded while preserving every unconsumed stock
attribute and child for the subsequent frame, region, and widget stages.

Live construction expands `$parent` against the nearest named ownership
context, applies each pre-linearized template layer once, and stores all frames,
regions, and widgets in one indexed arena. Explicit parents may bind after the
child declaration: stock `GameTimeFrame` names the nested `Minimap` before the
owning minimap XML has been constructed. Parent fixups occur once after the
load pass rather than adding a runtime name search.

The arena also retains one construction batch for every live root. A batch
links the root and its contiguous node range to the exact expanded
`UiLoadAction` that created it. Template declarations and intervening Lua
actions therefore consume action positions without creating fake batches.
Each node separately retains its structural XML owner and its final layout
parent: deferred `parent` fixups may change the latter but never rewrite the
former. The Lua executor can consequently expose one root batch at the correct
manifest boundary without making later globals visible early, while post-load
callbacks can still walk the original nested construction topology.

Nested global names are not required to be unique. Stock declares
`QuestInfoRequiredMoneyText` under two distinct live parents; both instances
remain owned while Lua retains the first non-nil global binding. Objects with
the same name, parent, type, and role are instead merged as inherited/concrete
layers. The constructor keeps XML layers as references into the bundle,
avoiding a second copy of the expanded property trees.

## Validation

Generated MPQs exercise manifest order, relative resolution, XML ownership,
Lua compilation, and explicit failure behavior:

```powershell
cargo test -p solarity-ui --test stock_seed
```

An installed client can be checked without copying its data into the
repository:

```powershell
cargo run -p solarity-ui --example validate_ui_bundle -- `
    'C:\path\to\World of Warcraft\Data' `
    enUS `
    glue

cargo run -p solarity-ui --example validate_ui_bundle -- `
    'C:\path\to\World of Warcraft\Data' `
    enUS `
    frame
```

Appending `execute <logical-width> <logical-height>` runs Glue's ordered
bootstrap until completion or the first unimplemented stock API boundary.
Frame execution additionally requires `<player-money-copper> <player-xp>
<player-next-level-xp>` because `GetMoney`, `UnitXP`, and `UnitXPMax` read
authoritative active-player state and deliberately have no offline zero
fallback:

```powershell
cargo run -p solarity-ui --example validate_ui_bundle -- `
    'C:\path\to\World of Warcraft\Data' `
    enUS `
    frame `
    execute 1920 1080 0 0 400
```

Requiring an extent keeps `GetScreenWidth` and `GetScreenHeight` tied to real
window facts instead of a guessed resolution. The explicit player values are
validation fixtures, not production defaults. This mode is intentionally
strict and is the incremental compatibility audit for global and widget
bindings.

The Frame environment also owns the two build-12340 battleground queue slots
and single world-PvP battlefield-manager slot. Before any corresponding world
session packet, these slots are authoritatively inactive and return `none`;
they are not synthesized from map state. Active slots retain the localized map
name, instance or battle ID, level bracket, arena team size, rated flag, and
invitation expiration used by `GetBattlefieldStatus` and
`GetWorldPVPQueueStatus`. Area hearth-and-resurrection availability is a
separate world-state fact and resets when the active world ends.

Minimap tracking is likewise projected from the local player's actual spell
and area capabilities. The state is a compact ordered list of label, texture,
and stock `spell` or `area` category values plus one selected index. Before
those capabilities arrive the list is empty; `GetNumTrackingTypes` therefore
returns zero rather than exposing fabricated defaults. `SetTracking(nil)`
clears the selection represented by the dropdown's authored “None” row.

Against the current local client, the complete 16-archive stack expands and
validates 59 Glue resources (31 XML and 28 external Lua) containing 76 global
fonts, 71 object templates, and 34 live roots. Frame expansion validates 265
resources (133 XML and 132 external Lua) containing 149 global fonts, 311 object
templates, and 276 live roots. Inline scripts remain ordered actions rather
than synthetic files. The same command opens the archive-backed
`FRIZQT__.TTF` face and rasterizes a validation glyph.

Nested construction currently produces 2,552 Glue objects (1,410 globally
named) and 22,015 Frame objects (15,769 globally named), with 12 and 66
top-level owners respectively. These counts are emitted by the validator so
future template or merge changes cannot hide an unexpected fan-out during
local-client review.

## Persistent Glue ownership

`GlueManager` turns the validation pipeline into the pre-world runtime owner.
It loads and executes `GlueXML.toc` once, releases the temporary XML-backed
catalog and construction tree, and retains the lifetime-independent object
hierarchy, resolved frame and region states, texture plan, font catalog,
compiled handlers, dynamic templates, and Lua state. Direct-child indices use
one flat arena rather than one allocation per object. The same mounted
`AssetStore` is retained behind the main-thread Lua boundary for synchronous
stock model and font methods; runtime startup does not mount every MPQ twice.

## Typed layout plan

Region geometry is decoded after object construction into three flat arrays:
one node range per object, a shared inheritance-layer arena, and a shared anchor
arena. XML layers that contain no geometry consume no layout entry. This avoids
allocating separate vectors for every one of the tens of thousands of object
instances while retaining exact application order.

The plan accepts both stock dimension spellings observed in the local client:
`<Size><AbsDimension x="..." y="..."/></Size>` and direct
`<Size x="..." y="..."/>`. Anchors retain their explicit point,
`relativeTo`, `relativePoint`, and absolute offset. `$parent` references are
expanded from the constructed ownership tree. Missing values remain absent;
the parser does not manufacture size, anchor, alpha, scale, visibility, or
`setAllPoints` defaults.

Real-client validation retains 1,815 Glue layout layers with 1,441 anchors and
18,168 Frame layout layers with 14,692 anchors.

The resolved startup pass then applies those flat layers to every live object.
Dimensions begin at the stock zero state and replace only the authored axis.
Anchor declarations replace an earlier constraint only when they address the
same one of the nine region points; an absent `relativePoint` uses the local
point, while an absent `relativeTo` uses the layout parent or the `CSimpleTop`
screen root. Empty stock attributes have the same absent meaning. Offsets begin
at zero. If a layer contains `<Anchors>`, those declarations win even when the
same layer also sets `setAllPoints`; otherwise `setAllPoints="true"` clears the
current constraints and binds `TOPLEFT` plus `BOTTOMRIGHT` to the layout
parent. Missing named targets and self-anchors are startup errors rather than
requests for a nearby global or screen fallback.

Visibility, alpha, and scale start at shown, 1.0, and 1.0. Ordered XML layers
replace their local values, after which visibility is intersected and alpha
and scale are multiplied through the final ownership graph. This graph pass is
independent of construction order, so it includes deferred stock parents and
rejects cycles. The installed client resolves all 2,552 Glue objects to 1,545
final anchors and all 22,015 FrameXML objects to 17,020 final anchors.

Nested textures and font strings also retain the `<Layer>` draw band from
their declaration wrapper. Back-to-front order is typed as `BACKGROUND`,
`BORDER`, `ARTWORK`, `OVERLAY`, and `HIGHLIGHT`. A missing `level` becomes
`ARTWORK` because the archived build-12340 `UI.xsd` declares that exact
default. `BNet.xml` ships one `OVERLAY\`` typo around `$parentGlow`; only that
exact legacy spelling is normalized, while other unknown bands are errors.
The instantiated local trees retain 668 Glue and 9,472 Frame declarations with
an explicit or schema-defaulted draw band.

Frame-derived objects keep a second flat plan for explicit strata, frame level,
numeric ID, top-level behavior, movement and resize policy, screen clamping,
keyboard and mouse input, protection, and saved-position opt-out. Strata are a
closed back-to-front vocabulary from `BACKGROUND` through `TOOLTIP`; unknown
values are not silently mapped into an adjacent render band. Defaults remain a
later stock schema/application concern, so an absent XML property consumes no
per-object state here.

Real-client validation retains 218 Glue and 1,604 Frame property-bearing
layers in this plan.

Resolved frame state follows the recovered `CSimpleFrame::SetParent` behavior:
an unparented frame starts in `MEDIUM` at level zero, while a child copies its
parent's stratum and starts one level higher. Ordered template and concrete XML
layers then replace only explicitly authored values. Resolution follows the
final ownership graph rather than construction order, which covers stock's
deferred explicit parents without a second runtime name search. Cycles,
non-frame parents, and level overflow are explicit startup failures.
The installed client resolves 580 Glue and 4,938 FrameXML frame-derived
objects through this pass.

## Typed animation plan

Animation groups form a frame-owned construction tree, not frame or render
regions. They are registered after their owner table and before that owner's
`OnLoad`, so XML `parentKey` fields and expanded global names are available to
stock Lua at the same boundary as the original client. An unnamed group keeps
its frame's name context; consequently an `Alpha` named `$parentPulser` inside
`TutorialFrameCallOut` becomes the global `TutorialFrameCallOutPulser`.

The installed build-12340 FrameXML corpus contains 13 `AnimationGroup`
declarations, one timing-only `Animation`, 20 `Alpha` primitives, and two
`Translation` primitives. Their authored order, start and end delays,
duration, smoothing, looping mode, additive values, `OnLoad`, and `OnFinished`
targets are retained. Equal order values execute as a parallel band, while
higher orders follow sequentially. Later-client primitives such as `Scale`,
`Rotation`, and path animation are rejected rather than assigned invented
behavior. GlueXML contains no animation declarations in the installed corpus.

Lua animation objects have their own metatables and retained playback state.
They do not enter the frame layout arena, renderer batches, or frame event
subscriber count. A renderer-clock update stage will advance their progress;
construction-time `Play`, `Pause`, `Stop`, and `Finish` calls already preserve
the stock script-visible lifecycle and group callback ownership.

## Typed texture plan

Texture declarations are decoded into a flat plan parallel to the object tree.
Each node owns a range of inheritance layers containing only explicitly stated
file, blend-mode, texture-coordinate, tiling, load, uniform-color, and
vertical-gradient values. Missing properties stay missing, while `file=""` is
retained as a dynamic runtime assignment rather than being mistaken for a
missing archive asset. The accepted `ADD` and `BLEND` modes and gradient shape
come directly from the installed GlueXML and FrameXML corpus.

Stock XML uses extensionless names and a handful of legacy `.tga` spellings,
but the installed 3.3.5a archives contain the corresponding `.blp` path in
every observed case. The plan therefore performs the single stock naming
conversion to `.BLP` before lookup. It does not probe multiple extensions or
loose files. The same canonical path also means an HD patch archive replaces
the payload solely through normal archive priority; the UI layer has no
separate HD path or quality branch.

The instantiated local Glue tree retains 1,671 texture layers referencing 97
unique concrete paths; Frame retains 10,913 layers referencing 494. The
validator reads every unique canonical path through the mounted archive stack
so a naming-rule or precedence regression fails before rendering begins.
