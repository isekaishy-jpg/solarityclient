# Binding declaration ownership

Build 12340 loads `Interface\FrameXML\Bindings.xml` as a special UI document;
it is not an ordinary `FrameXML.toc` entry. The local stock document contains
273 active `<Binding>` declarations and 16 `<ModifiedClick>` declarations. Two
additional binding-shaped elements inside an XML comment are correctly absent
from the parsed vocabulary. Binding
bodies are Lua 5.1 source, and `runOnUp="true"` causes the same body to receive
both `keystate = "down"` and `keystate = "up"` transitions.

`solarity-ui` owns this authored command vocabulary. `UiBindingCatalog` loads
the built-in document through ordinary MPQ precedence, validates its exact XML
shape, and compiles every Lua body before input can reach it. It retains source
order, grouping headers, release behavior, hidden/debug status, the exact
`windows`/`mac` platform gate, and modified-click defaults without converting
any of them into hard-coded Rust gameplay commands.

## AddOn boundary

An AddOn may provide the special path
`Interface\AddOns\<name>\Bindings.xml` independently of its TOC entry list.
The catalog accepts enabled AddOns one at a time in the caller's resolved load
order and uses the existing loose-first, MPQ-second AddOn file stack. Absence of
that optional special file is normal. A present malformed document fails at
its exact AddOn path rather than falling back to another source.

The stock executable contains separate diagnostics for a binding name, binding
header, or modified-click action being defined more than once in the current
source. The catalog therefore rejects each duplicate across the built-in and
appended AddOn documents. It does not invent first- or last-wins replacement.

## Runtime boundary

Physical key state and chord routing remain owned by `runtime/input`.
`InputBindingRouter` joins physical keyboard, pointer-button, and vertical
wheel transitions to these assignments, then returns the selected declaration
and stock `down`/`up` phase for the active FrameXML environment. Player
movement and camera systems will therefore receive authored Lua API calls such
as `MoveForwardStart` and `CameraZoomIn`; SDL key values do not become gameplay
policy directly.

The active-world owner now retains the catalog and the exact assignment image
shared with Lua. It compiles each command once in its retained FrameXML state,
with the local `keystate`, `pressure`, `angle`, and `precision` parameters
authored by native `0x00564470`. Debug and foreign-platform declarations are
not executable. The keyboard/mouse caller `0x00563150` and invocation at
`0x0055F860` supply pressure 1 on down or 0 on up, angle -1, and precision 0.
These remain function locals and do not overwrite globals with the same names.
Platform events reach that state through
`InputBindingRouter::route_to_frame` after focused UI input has been delivered.
Claimed presses do not start bindings; releases and focus loss still drain
previously admitted commands, including while the developer console or loading
card captures input. World departure drains outstanding releases before its
FrameXML event. Lua executes after the assignment borrow ends.

Binding mutations use the existing object and visual journals, including
mutations made before an authored error. One failing release does not prevent
the rest of the release batch from executing. Dynamic secure-action execution
and native movement/camera command consumers remain separate unfinished owners;
successful dispatch does not establish locomotion or camera-input parity.

## Assignment streams

The same archive stack exposes `WTF\DefaultBindings.wtf`. The installed
build-12340 member contains 153 unique key chords selecting 144 actions. Its
grammar is the executable's exact line-oriented `bind KEY ACTION` command,
while saved state may also contain `modifiedclick ACTION CHORD` and
`BINDINGMODE 0|1|2` records for default, account, and character modes.

`UiBindingAssignments` parses those records sequentially. Repeating a key or
modified-click action replaces its earlier value without rebuilding the map,
matching command execution order. Named declarations and the dynamic `SPELL`,
`ITEM`, `MACRO`, and `CLICK button:mouseButton` action forms remain distinct
types. Modified-click actions must have been declared by `Bindings.xml`.

Key chords remain validated stock tokens rather than being split at every
hyphen: both the bare minus key `-` and the modified chord `CTRL--` occur in
the real default file. Runtime input builds the complete chord in a fixed
stack buffer and performs a borrowed lookup, so minus is never misinterpreted
as an empty component.

Validate the full built-in vocabulary against a locally owned client without
copying its assets into the repository:

```powershell
cargo run -p solarity-ui --example validate_bindings -- `
    'C:\path\to\World of Warcraft\Data' `
    enUS
```

Validate that every active Windows default can be reached from the physical
input vocabulary:

```powershell
cargo run -p solarity-runtime --example validate_input_bindings -- `
    'C:\path\to\World of Warcraft\Data' `
    enUS
```
