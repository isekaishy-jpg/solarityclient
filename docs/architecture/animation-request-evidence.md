# Native sequence-request and completion boundaries

`tools/ghidra/animation_request_oracle.py` executes the fingerprinted build-12340
instructions at `00827190`, `00831C30` and `0083D840`. The local executable hash
was rechecked as `aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.
LLVM disassembly of these entries and `0083DA10` was inspected alongside the
existing timer oracle. The script passed 39 deterministic cases with Unicorn
2.1.4; the generated report is ignored at `target/animation-request-native.json`.

The prefetch fixture supplies an already normalized animation ID and executes
stock's linear sequence lookup and variation-chain traversal. A sequence with
neither `0x10` nor `0x20` requests a source; loading and resident entries are
skipped. A separate alias entry retains its own caller index at the source
boundary. An unrelated animation is not requested. An unready model instead
queues opcode 14 with its scene tick and requested animation; it does not
attempt a source read before model readiness.

The pending-consumer fixture executes the existing-request path in `00831C30`.
It finds a shared pending sequence directly or through the sequence link, then
updates that model's existing waiter without allocating another consumer or
requesting another read. Repeated requests replace the saved bone, timer offset,
speed, mode and flags in that waiter. Sharing the source does not merge model
playback ownership.

The completion fixture executes the control flow in `0083D840`. Source payload
publication precedes consumer dispatch. A withdrawn waiter (bit 8) is removed;
a consumer rejected by the bone admission boundary is not applied. The primary
and secondary paths retain the saved sequence/bone/mode/offset/speed arguments;
they preserve the independent low flag bits, and clear the applied model pointer.
Changing the scene tick does not rewrite these arguments before dispatch. The
word at waiter `+0x10` is a timer offset, not a captured wall-clock timestamp.

These are bounded native-instruction observations, not a complete pending-pose
oracle. Allocation, animation-ID normalization, source I/O admission, payload
relocation, bone admission, final channel application and cleanup are declared
provider boundaries. The fixtures do not prove frame sampling while data is
pending, blend timing after late completion, missing-companion behavior,
outstanding-read destruction, or full retention policy. Those remain required
before the runtime can replace eager companion loading without guessing a pose
fallback. No Rust animation-loading behavior changed in this evidence step.
