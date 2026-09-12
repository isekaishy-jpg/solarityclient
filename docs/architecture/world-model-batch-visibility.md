# WMO portal batch visibility

The ordinary stock graphics path consumes each group's stored scene frusta,
not only a group visibility bit. `799310` appends a copy of the current scene
frustum on every group callback. `7964A0` processes the collected groups and
`7ABF50` restores each stored frustum through `791100`, transforms it into the
owner's local coordinates through `78FB00`, and calls the selected renderer.

`78FB00 -> 983F40` transforms the eight corners using `4C2300` and rebuilds
all six planes through `983E70`. The corner transform stores each component
after extended Z/Y products, the X product, and translation. An ordinary f32
matrix multiply or a world-space oriented-box test is not an equivalent
boundary. `WorldSceneFrustum::transformed` preserves those stores.

The pinned image's ordinary selector value is 5 (`ADFE44`, used by `7AD020`).
Its normal callback `7AC6A0` clears each MOBA flags byte's high nibble on
frustum zero. For every frustum, it skips batches already marked, calls
`7A7630` on each remaining batch, and marks accepted batches before texture
and GPU work. `7A7630` widens the six signed-i16 MOBA bounds and uses
`78FB20 -> 9839E0` to test the six local planes with native tolerance.
Later frusta append newly visible batches in authored order. The complete
result need not be in increasing batch-index order.

The callback also controls the number of candidate batches. `7D8379` copies
MOGP's exterior count into group offset `60`; ordinary `7AC6A0` uses this as
a prefix length, without adding the transition/interior counts as an offset.
`7D7CE1` sets the MOCV pointer only for group flag `4`. With that pointer,
`7ABF50` selects `7AC9F0`, whose loop uses the full MOBA count at `16C`.
Root flag `2` redirects either material callback to `7A9380`, also using the
full count. The untextured `7A9BF0` loop uses the same full-count boundary.
`WorldModelMeshPlan` retains complete geometry but limits each group's draw
range to the candidates used by its normal material callback.

`WorldModelBatchVisibilityQuery` retains scratch storage for this selection
and returns first-accepted indices. Inputs are validated local bounds and
ordered local frusta. Each call starts a fresh group/frame, and the decoded
asset's authored flags remain immutable.

## Evidence and limits

`tools/ghidra/world_model_batch_visibility_oracle.py` maps only the fingerprinted
build-12340 image. It executes full `790E20`, `78FB00` and `7A7630` routines.
It also executes the original selection loops in `7AC6A0`, `7AC9F0`,
`7A9380`, and `7A9BF0`, skipping only texture/shader/GPU submission after
each accepted-marker store. All four loops are checked at candidate counts
0, 1, 17, and 48, with the unused count field deliberately set differently.
The checked-in fixtures cover 432 local frusta, with every corner and plane
matching bit for bit, and 72 six-window batch sequences. Each sequence has
48 signed-bounds batches, including enclosing and degenerate boxes, arbitrary
initial flag bytes, overlapping/disjoint windows, and repeated visits.
Tests compare every prefix's selection order, native final markers, empty
inputs, and reuse in the next frame.

`world_model_scene_clip_oracle.py` captures another 360 original callback
frusta from camera-root and exterior-window recursion. Tests compare every
corner and plane bit. Initial callbacks retain the exact inherited frustum;
recursive windows reproduce the add/spill/divide stores before recropping.
The public camera query retains every callback, including duplicate groups
and direct callbacks that inherit the preceding fog bank.

Runtime collection now retains first-group order and every local portal
frustum independently of the narrower unit-admission set. Static entries
and converted moving entries join the same 64 depth bins before traversal.
`world_model_outdoor_order_oracle.py` executes original insertion, moving-list
conversion, and consumption for 24 mixed-root scenes. A runtime test replays
these sequences through the production queue and group collector, including
ties, masked groups, and the moving list's early return beyond bin 63.

`WorldModelFrame` resolves each collected owner to its GPU placement and
submits first-accepted batches through `WorldModelBatchVisibilityQuery`.
The hidden Vulkan runtime test loads two groups and two replicated owners
through decoded archive resources. It checks combined group index ranges,
right-then-left portal order, repeated-window deduplication, shared mesh
identity, and each owner's transform. Captured pixels show the two selected
surfaces with the center surface absent; the following frame checks that
the previous regions and accepted markers do not survive.
Group fog bits accumulate over callbacks and the final submitted group
restores the bank carried to the next frame. Material texture residency and
shader execution are outside the native selection oracle's boundary.

The installed Orgrimmar check on 2026-09-10 covers the benchmark's settled
camera at `(1075.3798, -4500, 156.24147)`, facing approximately
`(0.9848077, 0, -0.17364818)`, at 1280 x 720 and far clip 777. Its MODF
165042 has 144 groups, 157 portals, and 314 references. Original
`7B3A10`/`7AD350`/`7AC060` and `7A9090` select groups `5, 10, 9, 114`;
all 192 local corner/plane float stores and all 42 selected batches match
the public Systems queries bit for bit. The changed city silhouette in
the runtime capture agrees with these native decisions. The older image,
which drew additional interior groups, is not the reference for visibility.
That installed check preceded integration of the separate attached-doodad path.

Reproduce this bounded installed-data check with:

```powershell
cargo run -p solarity-systems --example capture_ogrimmar_scene -- <Data> target/ogrimmar-scene
py -3.11 tools/ghidra/ogrimmar_scene_oracle.py <Wow.exe> target/ogrimmar-scene
```

The Python tool requires Unicorn and `tools/ghidra` on `PYTHONPATH`.
Copied archives and captures remain local. The oracle takes the placement
matrices as inputs, supplies resident lookup and graphics-state setup, and
disables optional occlusion/exclusion output. Original camera arithmetic,
portal projection/recursion, clip operations, and batch selection execute
against the extracted data. This is not a stock-client screenshot or proof
of camera registration, terrain occlusion, doodad visibility, or shaders.

WMO liquids now consume these admitted groups and their fog flags; see
[liquid admission and fog](liquid-rendering.md#group-admission-and-fog).
Attached doodads now use their separate [native admission paths](world-model-doodad-visibility.md).
No runtime FPS improvement or complete WMO visibility parity is claimed by
these selection fixtures.

## Per-group surface fog

`799310` clears the scene group's fog bit on its first visit and accumulates
`0x8000` for any indoor callback. `7966E7` reads that bit before the group is
drawn. The actual group constructor `7B3DE0` installs `7B3F30` at vtable slot
one; this callback reads the ordinary DayNight bank at `8C` or the indoor bank
at `A0`, then writes the query through `834990`. The final `7F16F0` composition
gives the two model banks identical range and exponent with separate colors.

Runtime now passes both retained colors into WMO surface preparation and
selects the material's fog color from the group's accumulated flag. Previously
every visible group received the camera bank, even when traversal selected
ordinary exterior fog. The subsequent [physical-pass fog policy](world-fog.md#wmo-physical-pass-fog-policy)
now applies callback-specific exterior overrides and fog enable, including
different banks for transition contributions. Programmable WMO surfaces do not
use blend-specific black/white substitutions. Scene range/exponent and the
native final-group carry remain independent of that pass policy.

`world_model_group_fog_oracle.py` executes the constructor and its actual virtual
callback with only the current DayNight provider substituted. Its 64 captures
include both banks, unrelated group flags, distinct packed colors, two common
fog ranges, and repeated use of the same query storage. The runtime Vulkan
regression submits adjacent groups through decoded WMO resources, compares
each material's selected color bits, and checks all 64 resulting frames while
the groups switch fog banks. It also retains the existing owner/portal ordering,
hidden-region, and next-frame invalidation checks. These controlled fixtures
establish the surface connection; they do not replace combined live-world
comparisons. The separate [M2 doodad consumer](world-model-doodad-visibility.md)
has its own native and GPU coverage.
