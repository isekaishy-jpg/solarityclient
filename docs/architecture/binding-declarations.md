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
order, grouping headers, release behavior, hidden/debug status, the stock Mac
platform gate, and modified-click defaults without converting any of them into
hard-coded Rust gameplay commands.

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

Physical key state remains owned by `runtime/input`. A later assignment layer
will join saved key chords to these authored command names, then execute the
selected Lua body in the active FrameXML environment. Player movement and
camera systems therefore receive stock Lua API calls such as
`MoveForwardStart` and `CameraZoomIn`; SDL key values do not become gameplay
policy directly.

Validate the full built-in vocabulary against a locally owned client without
copying its assets into the repository:

```powershell
cargo run -p solarity-ui --example validate_bindings -- `
    'C:\path\to\World of Warcraft\Data' `
    enUS
```
