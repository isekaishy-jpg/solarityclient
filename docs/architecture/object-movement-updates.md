# Movement-only object updates

Build-12340 `SMSG_UPDATE_OBJECT` operation 1 has a packed GUID followed directly
by the living movement block. It does not prefix `UpdateFlag`, even though
creation operations 2/3 do. Treating operation 1 as a creation movement block
shifted every subsequent read and could reject a valid packet or interpret its
movement flags as unrelated placement flags.

The native boundary is `0x004D6DA0`: packed GUID reader `0x0076DC20`, movement
initialization, and then `0x004F5090`. The latter calls MovementInfo reader
`0x004F4D40`, reads nine speed floats, and conditionally calls spline reader
`0x004F4B50` when movement bit `0x08000000` is set. Creation enters the same
reader through the living branch of `0x004D3890` after reading `UpdateFlag`.

`UpdateCursor::read_living_movement` now serves both paths. Creation retains its
own conditional tails and field mask. Movement-only snapshots report zero
creation flags and cannot manufacture non-living passenger offsets or packed
GameObject quaternions. The decoder preserves wire flags and optional field
presence; native clears INTERPOLATED after reading its second transport clock,
while the network snapshot retains that original bit and the optional clock.

`GameplaySession` consumes local-player operation-1 echoes without applying them,
matching the GUID guard in `0x004D6DA0`. Remote Unit/Player snapshots update
position, all speeds, and the full conditional context in packet order. Missing
conditional fields clear the previous snapshot's attachment, pitch, and fall
data. This operation does not replace the local input/control protocol.

Stock dispatches remote operation 1 directly to the Unit movement owner at
`0x0073C220`. The runtime explicitly rejects a non-unit target before mutating
it, preventing malformed living data from erasing a GameObject's packed rotation
or adding a living component. This category validation is a Rust admission
guard; it is not a claim that the native client gracefully rejects that malformed
target. Unknown GUIDs retain the existing lifecycle error.

## Verification and limits

`runtime/tests/fixtures/object-movement-native.txt` stores eight input payloads
and native movement structures produced by executing unmodified `0x004F5090`
and its readers. The capture used an initialized native data buffer and executed
without hooks or replacement routines. It checked exact byte consumption and
all nine speeds. Its header records the source executable SHA-256.

Encrypted loopback tests compare those snapshots with the Rust decoder. Other
cases cover creation/movement layout separation, following field operations,
local-player echoes, remote attachment removal, non-unit rejection without
mutation, conditional spline consumption, and every prefix truncation of a
populated movement packet followed by a valid encrypted frame. The deliberately
incorrect creation-prefix layout is rejected.

The native fixture corpus excludes spline-enabled blocks; the independently
authored encrypted fixture tests their consumption. Spline trajectories are still
consumed without a retained path owner. These checks establish this packet
boundary and ECS projection, not complete remote-motion interpolation, local
ground/fall integration, dynamic collision, or live visual parity.
