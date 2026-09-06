# Player movement collision

The systems crate now owns a pure player-volume sweep in `collision/collide`.
It accepts a foot origin, admitted radius and effective height, a displacement,
and an ordered slice of oriented collision triangles in the same coordinate
space. It returns permitted travel and the body planes near first contact.
The triangle API also tests support at a foot point after swept movement.
`MovementFallContactQuery` combines these queries with native fall contact-time selection,
landing/ceiling classification, and the remaining horizontal correction.
These queries do not yet drive the player. The separate
[world geometry collectors](movement-world-geometry.md) now supply ordered
terrain, WMO, and M2 faces from admitted resident generations.
The [fall interval owner](movement-fall-integration.md) now repeats contact
queries and returns updated airborne state or a ground transition.
The [ground interval owner](movement-ground-integration.md) now handles surface
following, wall response, step probes, and speculative falls.

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
| `0x0075D340` | Landing support after applying the swept displacement |
| `0x0075D0A0` | Tests the foot point against the triangle's vertical footprint |

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

### Landing support

`MovementCollisionTriangle::supports_at` takes the foot point after swept
displacement and a resolved `MovementSupportProfile`. The slope test uses
strict greater-than against the native normal-Z threshold: `0x00A37F0C`
(approximately 0.642788) for the player-control profile and `0x00A37F10`
(approximately 0.173648) for the other profile. Selection belongs to the native
unit-control policy at `0x00716710`, which involves unit flags, type, and
controlling-unit identity; this query does not infer it from the selected GUID.

For an admitted slope, the triangle is extruded vertically and the foot point
is tested against its three edge planes with the exact 1/12-unit tolerance at
`0x00A37F38`. Height separation is not retested: the preceding sweep owns
contact distance. `0x0075D0A0` also preserves initially supplied +Z planes when
the edge builder stops on a degenerate projected edge. The implementation keeps
that behavior, including partial edge-plane construction.

An additional 158 captured calls to original `0x0075D340` exercise both profiles,
slopes around both thresholds, front/back winding, footprint boundaries, large
world coordinates, and small projected edges. The capture executes the real
`0x00716710` predicate using explicit player/ordinary-unit object images;
it does not replace the predicate with a constant. The standalone fixture
`movement-support-native.txt` and Rust tests compare each support decision.

## Fall contact response

`MovementFallContactQuery::resolve` implements one fall interval's contact
response at `0x00760FC0`, followed by the contact-time adjustment at `0x0075EB00`.
Its query supplies the existing analytic fall curve, launch height, elapsed and
available seconds, horizontal speed, and resolved support profile. The result
contains allowed displacement/distance, consumed seconds, the selected triangle,
an XY correction for remaining travel, and a clear/slide/land/ceiling decision.
It allocates no memory and mutates no movement or world state. Movement owns
the curve and time calculation; collision supplies geometry and response using
the resolved time remaining to the apex. Collision does not depend on movement.

Support is tested first. An unsupported upward contact requests a ceiling reset
only when a top body plane is present and allowed travel fits the remaining time
to the apex times horizontal speed. Other contacts use `0x0075E9C0`: steep faces
select a response plane; flatter surfaces choose the nearest infinite triangle
edge line, preserving ordered ties. The XY correction preserves the remaining
vertical motion and adds the native 0.001-unit bias.

The two-body-contact branch (`0x0075E760`) can intersect the world plane with
both body planes, select an edge using `0x0075D4B0`'s unnormalized cross-product
metric, and orient the resulting normal against the first body contact. Its
parallel-edge branch recognizes +1 alignment only. The one-body-contact helper
`0x0075D890` multiplies absolute plane distances before testing for a negative
product; that branch cannot select the alternative normal for finite geometry.
The implementation preserves this observed behavior.

`0x0075E040` selects the height crossing from the launch, apex, interval, and
horizontal travel. The inverse root retains its wider intermediate through time
subtraction. A root at/before elapsed time clears displacement and consumes zero
seconds; a root after the interval consumes the interval. Earlier contacts can
shorten horizontal travel and recompute distance. With no selected triangle,
the original displacement survives even when the narrow phase cuts its reported
distance to zero. A request below 2^-22 returns before that narrow-phase cutoff.

The fixture `movement-fall-contact-native.txt` contains 807 executions of the
original contact and time functions with all native callees. Explicit cached
candidate bounds exercise `0x0075F0A0`'s existing-cache branch without replacing
the collector, support predicate, or geometry helpers. Tests compare all four
decisions exactly, displacement/distance/correction within 0.0001 world units,
and time within one microsecond. Cases include the 69 sweep geometries under
three launch/time states and 600 deterministic ballistic samples across both
terminal modes and support profiles. Native branch observation recorded 41
three-plane edge resolutions, 31 flat-surface edge selections, 56 horizontal
time shortenings, 172 support decisions, and 23 ceiling resets.

These comparisons exposed an early-rounding error in the shared sweep length:
native callers store length only after taking the square root of wider products.
Computing the products in f32 could reverse simultaneous body-plane ordering
and change the slide bias. The shared sweep now retains that native boundary;
the existing 69 contact/order comparisons still pass.

## Remaining movement ownership

This query implements the narrow phase after candidate collection. The movement
owner still needs residency admission and ordered assembly of the implemented
terrain/WMO/M2 face collectors, transport-space
conversion, application of fall/ground transitions,
and timestamped input integration. Native `0x0075FF90` and `0x0075F0A0` own
candidate collection and transport conversion; `0x007620F0`, `0x00761B00`, and
related `Collide.cpp` callers own movement response. Ground/step and fall
intervals now implement their respective response cores. Runtime operations must not
be inferred from the camera ray API or from successful contact tests.

The [analytic fall curves](movement-trajectories.md) used by collision response
and step trials are now implemented and checked against native x86 execution.
The fall interval now owns repeated collisions and fall-specific state updates;
the ground interval now owns surface response and the surrounding step trials.
The [runtime geometry cache](movement-world-geometry.md#per-sweep-cache-refresh)
now implements native endpoint coverage and union recollection within one
borrowed world context. Ground/fall response still needs its refresh/failure
interface before replacing the fixed candidate slices used by those cores.
