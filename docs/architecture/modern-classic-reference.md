# Modern Classic comparison binary

On 2026-09-20 the user offered the locally installed WoW Forever/modern Classic
binary as an additional architecture reference, and confirmed its directory:

`C:\Program Files (x86)\World of Warcraft\_classic_beta_`

Read-only identification of `WowB.exe`:

| Field | Value |
| --- | --- |
| Product | World of Warcraft |
| File version | `1.60.1.69913` |
| Original filename resource | `WoW.exe` |
| Size | 102,693,584 bytes |
| PE machine / optional header | AMD64 (`0x8664`), PE32+ (`0x20b`) |
| Preferred image base | `0x140000000` |
| SHA-256 | `FA792B922A8306D46640ED696E812D62F0E9DF9C797E094F9D307281963E3B3F` |

The user identifies this as a DX12 client. Static inspection finds
`D3D12CreateDevice` and `d3d12.dll` strings. This confirms relevant binary
content, not the renderer selected by a live session.

Use the binary to investigate modern worker scheduling, independent model/draw
preparation, cache ownership and draw batching where useful to the CPU cutover.
The 3.3.5 stock executable and its existing disassembly remain the gameplay and
visual-behavior authority. Modern graphics API choices do not directly specify
the correct Vulkan implementation or prove applicability to 3.3.5 data.

Initial static search leads are raw file offsets, **not virtual addresses**:
`D3D12CreateDevice` at `0x5188118`, `0x518814a`, `0x51b2980`;
`ThreadPool` occurrences around `0x4cdc2cd`, `0x4ce4ca0`;
`WorkerThread` occurrences around `0x50f3289`, `0x50f32c8`.
These strings can belong to middleware; no scheduler design is inferred from
their names. Follow actual cross-references and callers before treating one as
evidence of game-loop behavior. Recheck the hash after launcher updates and use
a separate analysis project from the stock 3.3.5 project.

Only identity and static string presence were inspected at this checkpoint.
No game process was launched, no binary was modified, and no modern scheduler
or instancing implementation has yet been established from disassembly.
