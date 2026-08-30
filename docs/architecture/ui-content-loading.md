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
fail at the asset boundary. Chunks are not executed yet. Execution requires the
stock globals and widget APIs to be registered first; installing permissive
no-op functions would conceal compatibility gaps and violate the repository's
no-fallback policy.

The bundle retains both the source files and its Lua state. This gives the next
stage a single owner for API registration and ordered execution without reading
the archives a second time.

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

Against the current local client, the complete 16-archive stack expands and
validates 59 Glue resources (31 XML and 28 external Lua) containing 76 global
fonts, 71 object templates, and 34 live roots. Frame expansion validates 265
resources (133 XML and 132 external Lua) containing 149 global fonts, 311 object
templates, and 276 live roots. Inline scripts remain ordered actions rather
than synthetic files. The same command opens the archive-backed
`FRIZQT__.TTF` face and rasterizes a validation glyph.
