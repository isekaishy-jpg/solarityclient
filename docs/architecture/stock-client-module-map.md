# Stock client module map

## Decision

The initial folder modules are derived from the stock World of Warcraft
3.3.5a build 12340 executable, then adapted from its C++ object hierarchy into
the repository's Rust modular-monolith boundaries. The stock file names are
evidence for responsibilities, not names that Solarity must reproduce one for
one.

Every seeded module is private to its crate. Each crate root remains the future
facade, so consumers cannot couple to the implementation tree as APIs are
added.

## Evidence

Ghidra 12.1.3 completed its default x86 PE analysis of the fingerprinted stock
binary in 462 seconds. The deterministic exporter in `tools/ghidra` reported:

| Recovered evidence | Count |
| --- | ---: |
| Functions | 27,733 |
| Defined embedded source files | 395 |
| Source-string cross-references | 4,198 |
| MSVC RTTI type descriptors | 566 |
| Imported symbols | 470 |

The executable hash was
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.
The generated reports remain outside version control because they can be
recreated from a locally owned client with `tools/ghidra/README.md`.

Representative Ghidra cross-references demonstrate that the source strings are
connected to recovered code rather than being an unattached filename list:

| Responsibility | Embedded source path | String | Code xref | Function |
| --- | --- | ---: | ---: | ---: |
| archive access | `SFile2-Core.cpp` | `009e4f68` | `00421968` | `00421950` |
| client database | `DBClient.cpp` | `00a26e8c` | `006337d5` | `006337d0` |
| map chunks | `MapChunk.cpp` | `00a40378` | `007c586f` | `007c5690` |
| render device | `CGxDevice/CGxDevice.cpp` | `00a2dd6c` | `00685c6b` | `00685c60` |
| shader effects | `ShaderEffectManager.cpp` | `00a4d61c` | `00876dd8` | `00876d90` |
| audio | `SoundEngine.cpp` | `00a4f170` | `00878068` | `00878010` |
| movie widget | `CSimpleMovieFrame.cpp` | `00a9f010` | `0095df8c` | `0095dc80` |
| world connection | `WowConnection.cpp` | `009e8ad8` | `00467412` | `004673c0` |
| legacy login | `Grunt.cpp` | `00a94ac0` | `008cbb79` | `008cbb40` |
| Warden | `WardenClient.cpp` | `00a40774` | `007da20e` | `007da200` |
| unit state | `Unit_C.cpp` | `00a34b10` | `007166a9` | `00716650` |
| spell behavior | `SpellCast.cpp` | `00aa9818` | `009ab847` | `009ab810` |
| glue UI | `CGlueMgr.cpp` | `009f3d44` | `004d7a11` | `004d7940` |
| frame UI | `CSimpleFrame.cpp` | `009eb828` | `0048ffc8` | `0048fef0` |
| XML UI | `XMLTree.cpp` | `00a43dc8` | `00814619` | `008145d0` |
| composition root | `Client.cpp` | `009e0e94` | `004013d6` | `00401390` |
| event scheduler | `EvtSched.cpp` | `009ea034` | `0047dee9` | `0047dea0` |
| input | `InputControl.cpp` | `00a1de54` | `005f970b` | `005f96f0` |

## File-seed scope

This pass seeds navigation and the complete evidence-derived implementation
file layout, not implementation behavior. Every directory below contains a
`mod.rs` facade and is declared by its parent. Every implementation seed is
also declared by its owning facade. No placeholder behavior, guessed stock
state, or speculative public API is introduced merely to make a seed look
implemented.

The ownership generator maps all 395 recovered source artifacts, all 566 RTTI
descriptors, and all 470 imported symbols to 159 distinct leaf owners. Seven
additional cross-cutting modules split responsibilities that a
one-artifact-to-one-owner table cannot express. The `ui/feature` and
`ui/widget` parent facades bring the complete tree to 168 folder-backed
modules.

Inside those facades, the seed contains:

- 274 implementation files derived from distinct non-vendor stock source
  families; paired `.cpp` and `.h` artifacts collapse into one Rust file;
- 31 `types.rs` files for RTTI/import-only and cross-cutting owners that have no
  recoverable non-vendor source filename;
- five `dependency.rs` files marking archive, audio backend, audio codec,
  cinematic, and LCD vendor boundaries that must use approved dependencies or
  platform adapters instead of copied vendor implementations;
- 16 focused files for the seven cross-cutting CPU-pool, ECS world/view,
  player, camera, object-lifecycle, and UI-event responsibilities; and
- 166 external test leaves included by nine crate-level integration-test roots.

That is 326 production implementation seeds and 166 external test seeds in
addition to the 168 folder facades. The filenames are a traceability and
ownership skeleton; implementations may split further when actual stock
behavior proves that another concept changes independently.

## Crate ownership

| Crate | Count | Seeded folder modules |
| --- | ---: | --- |
| `asset` | 10 | `archive`, `cache`, `database`, `file_stack`, `model`, `storage`, `terrain`, `texture`, `world`, `world_model` |
| `cpu` | 3 | `job`, `pool`, `synchronization` |
| `ecs` | 17 | `corpse`, `creature`, `dynamic_object`, `effect`, `game_object`, `item`, `minigame`, `missile`, `movement`, `object`, `player`, `spell`, `trade`, `unit`, `vehicle`, `view`, `world` |
| `media` | 10 | `audio`, `audio/backend`, `audio/cache`, `audio/codec`, `audio/dsp`, `audio/emitter`, `audio/engine`, `audio/spatial`, `cinematic`, `voice` |
| `network` | 8 | `account_data`, `authentication`, `connection`, `integrity`, `protocol`, `realm`, `session`, `transport` |
| `rendering` | 16 | `camera`, `device`, `effect`, `geometry`, `lighting`, `liquid`, `math`, `minimap`, `model`, `particle`, `scene`, `shader`, `terrain`, `texture`, `weather`, `world_text` |
| `runtime` | 13 | `application`, `configuration`, `console`, `event`, `foundation`, `input`, `legal`, `loading`, `platform`, `platform/lcd`, `security`, `telemetry`, `time` |
| `systems` | 39 | `achievement`, `arena`, `auction`, `battlefield`, `calendar`, `camera`, `character`, `collision`, `combat`, `currency`, `dance`, `duel`, `effect`, `equipment`, `group_finder`, `guild`, `interaction`, `inventory`, `language`, `loot`, `mail`, `missile`, `movement`, `name_cache`, `object`, `pet`, `petition`, `player`, `quest`, `raid`, `reputation`, `skill`, `social`, `spell`, `stable`, `support`, `talent`, `vehicle`, `world` |
| `ui` | 52 | `addon`, `animation`, `binding`, `event`, `feature/*`, `font`, `frame`, `glue`, `glue/character`, `notification`, `region`, `render`, `script`, `widget/*`, `world`, `xml` |

The UI feature leaves are `action_bar`, `battlenet`, `chat`, `commentator`,
`container`, `dress_up`, `guild_bank`, `health_bar`, `item_text`, `loot`,
`merchant`, `minimap`, `name_plate`, `paper_doll`, `party`, `portrait`, `quest`,
`spell_book`, `taxi_map`, `tooltip`, `trade`, `trade_skill`, and `trainer`.
The concrete widget leaves are `button`, `check_box`, `color_select`, `edit_box`,
`html`, `hyperlink`, `message`, `model`, `movie`, `scroll_frame`, `slider`,
`status_bar`, and `texture`.

## Cross-cutting placement

- Player replication, identity, and durable fields belong to `ecs/player`;
  local-player behavior and orchestration belong to `systems/player`.
- Renderer-independent camera target, mode, zoom, and orientation belong to
  `ecs/view`; camera policy and transitions belong to `systems/camera`; view and
  projection submission belong to `rendering/camera`.
- World-object state is split into the concrete ECS object families. Shared
  object behavior belongs to `systems/object`, world rules to `systems/world`,
  and world presentation to `rendering/scene` and `ui/world`.
- Cinematic stream ownership remains in `media/cinematic`; the stock movie
  frame itself remains in `ui/widget/movie`.

`tools/architecture/TestStockSeedTopology.ps1` audits the generated ownership
tables against the tree. It asserts the build-specific evidence counts, rejects
unmapped rows, checks all 159 mapped owners, includes the seven additional
cross-cutting seeds, verifies every parent and child `mod` declaration, and
checks all 326 production plus 166 external test files.

## Boundary rules

- Asset modules decode and retain CPU-side stock data. Rendering owns Vulkan
  resources derived from that data.
- ECS owns state and entity identity. Systems own behavior applied to ECS state.
- Network owns build-12340 wire and session state. It publishes typed outcomes
  rather than mutating ECS, UI, or runtime internals.
- Media owns audio/video streams and timing. UI owns the movie frame widget;
  rendering owns presentation resources.
- UI owns FrameXML, GlueXML, Lua 5.1 bindings, frame state, and widget behavior.
  It emits renderer-facing presentation data through the rendering facade.
- Runtime is the composition root and sole owner of concrete subsystem wiring,
  startup, event-loop integration, loading transitions, and shutdown.
- The stock executable contains several renderer backends. Solarity's approved
  architecture intentionally implements only Vulkan 1.3 with SPIR-V 1.6; it
  does not select an automatic graphics fallback.
- The target AzerothCore environment uses the stock legacy Grunt/SRP path.
  Battlenet artifacts present in this particular binary do not justify adding
  a second authentication implementation to the initial architecture.
  The `BNGetMaxPlayersInConversation` registration at `00accca8` points to
  native function `00537a00`; its enabled path pushes the double at
  `00a09f58`, exactly `12.0`, while failed service predicates return no Lua
  values. The disabled runtime therefore registers the API but returns `nil`
  instead of inventing a zero-capacity Battle.net service.
