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

`WorldModelBatchVisibilityQuery` retains scratch storage for this selection
and returns first-accepted indices. Inputs are validated local bounds and
ordered local frusta. Each call starts a fresh group/frame, and the decoded
asset's authored flags remain immutable.

## Evidence and limits

`tools/ghidra/world_model_batch_visibility_oracle.py` maps only the fingerprinted
build-12340 image. It executes full `790E20`, `78FB00` and `7A7630` routines.
It also executes the original `7AC730..7AC9DB` selection loop, skipping only
texture/shader/GPU submission after the accepted-marker store at `7AC76C`.
The checked-in fixtures cover 432 local frusta, with every corner and plane
matching bit for bit, and 72 six-window batch sequences. Each sequence has
48 signed-bounds batches, including enclosing and degenerate boxes, arbitrary
initial flag bytes, overlapping/disjoint windows, and repeated visits.
Tests compare every prefix's selection order, native final markers, empty
inputs, and reuse in the next frame.

This is the validated local selection boundary. Runtime WMO drawing still
uses `PlacedWorldModelDrawPlan::select_visible_draws` with world group/batch
frustum tests. `WorldModelCameraSceneQuery` currently retains group indices
while discarding their portal clips; unit admission further reduces those
callbacks to a set. Completing graphics integration requires retaining the
full callback regions, first-group order, native outdoor depth-list order,
and fog state. The unit-admission set cannot substitute for graphics visits.
Alternate renderer callbacks and material-specific early dispatch also need
bounded evidence before extending the normal-loop claim to those paths.
No runtime FPS improvement or complete WMO visibility parity is claimed here.
