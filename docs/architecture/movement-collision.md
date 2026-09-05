# Player movement collision

The systems crate now owns a pure player-volume sweep in `collision/collide`.
It accepts a foot origin, admitted radius and effective height, a displacement,
and an ordered slice of oriented collision triangles in the same coordinate
space. It returns permitted travel and the body planes near first contact.
It does not yet drive the player or collect triangles from the resident world.

## Native evidence

The implementation follows build-12340 `Collide.cpp` in the fingerprinted
`Wow.exe`, SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.

| Address | Responsibility |
| --- | --- |
| `0x0075C8F0` | Four side planes, the top, and four sloped foot planes |
| `0x0075CA80` | Nine vertices and the five quad/four triangle face indices |
| `0x0075F9D0` | Sweep entry, minimum extrusion, contact ordering, travel cutoff |
| `0x0075CD70` | Sweeps the four triangular foot faces before the five quads |
| `0x0075BC50`, `0x0075BE80` | Extruded face side planes |
| `0x0075C5A0` | Approaching-triangle filtering and nearest contact per face |
| `0x0075B710` | Convex polygon clipping with whole-polygon tolerance |
| `0x0075C0B0` | Face distance and initial-penetration admission |
| `0x0075B560` | Swaps a newly earlier contact plane into the first slot |

The foot rises by radius times the exact float at `0x00A32830`, approximately
1.849399. Its four normals use the components at `0x00A37F28` and
`0x00A37F2C`. The origin is their common tip. The supplied top must reach the
upper foot edge for the public volume's geometric invariant; the caller owns
stock unit-dimension resolution before admission.

The sweep uses the original binary thresholds, including the contact tolerance
at `0x00A37F14`, minimum extrusion length at `0x00A3F854`, triangle approach
threshold at `0x00A34EEC`, and separate shallow/deep penetration tests. A zero
or tiny request returns zero travel. Larger requests below the final contact
tolerance also return zero travel, including when the triangle slice is empty.
Back-facing and tangential triangles do not become blocking surfaces.

The result's planes are outward **body** planes, not surface normals. Contacts
within the native distance tolerance accumulate in face iteration order;
a newly earlier contact swaps into the first position. The triangle output
preserves the last face query's selected triangle, which is not necessarily
the globally earliest triangle. Equal contacts within one face select the later
triangle in the supplied ordering.

## Storage and arithmetic

The query uses fixed arrays: nine body vertices and planes, up to nine contact
planes, and fifteen polygon vertices. Clipping a triangle against four
extrusion sides and eight remaining body planes needs at most fifteen vertices.
The sweep allocates no memory and owns no persistent world cache.

Native geometry uses x87 intermediates with explicit single-precision stores.
The Rust path uses wider intermediates through plane arithmetic and clipping,
then stores the native float fields. In particular, the quad-side cross product
spills edge differences before its Y/Z products while X retains the wider
differences. Preserving that boundary fixes observable contact ordering in a
diagonal sweep. This is supported by assembly at `0x0075BF15` onward and by
execution of the original instructions. It is not a general promise of bitwise
x87 equivalence for arbitrary inputs.

## Independent validation

`crates/systems/tests/fixtures/movement-sweep-native.txt` contains 69 cases
captured by running the original `0x0075F9D0` and its native callees under
Unicorn 2.1.4 in x86-32 mode, with x87 control word `0x037F`. The PE sections
were mapped at their authored virtual addresses. Each call supplied an explicit
origin pointer and triangle list, bypassing resident-world lookup without
replacing any narrow-phase function. Radius/height occupied movement offsets
`0xC8`/`0xCC`; triangle records held their plane followed by three vertices.

The checked-in fixture contains input and output scalar bits, not executable
code. Ordinary Rust tests require neither the emulator nor the original game
installation. They compare travel, exact contact count/order, plane equations,
and the selected triangle. Scalar comparison allows 0.0001 world units for
intermediate arithmetic differences, below the native contact tolerance.

Cases cover empty geometry; both windings; tangent, endpoint, and beyond-endpoint
contact; walls, floor, ceiling, slope, corner, shallow and deeper penetration;
low/narrow obstacles; geometry above the body; all six axis directions across
six gap distances; oblique motion; and large translated world coordinates.
Separate admission tests reject non-finite and degenerate geometry.

## Remaining movement ownership

This query implements the narrow phase after candidate collection. The movement
owner still needs resident terrain/WMO/M2 triangle selection, transport-space
conversion, sliding and step-up, support/slope decisions, gravity and landing,
and timestamped input integration. Native `0x0075FF90` and `0x0075F0A0` own
candidate collection and transport conversion; `0x007620F0`, `0x00761B00`, and
related `Collide.cpp` callers own movement response. Those operations must not
be inferred from the camera ray API or from successful contact tests.
