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

Against the current local client, the complete 16-archive stack resolves and
validates 31 Glue sources (30 XML and 1 Lua) and 139 Frame sources (126 XML and
13 Lua).
