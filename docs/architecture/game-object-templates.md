# Game-object templates and transport clock admission

The behavioral source is the pinned Windows build-12340 `Wow.exe`, SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.

## Network representation

`0x006351B0` dispatches the game-object query response to `0x0067BEE0`.
The leading entry is a u32. Its high bit marks a missing entry and is removed
before notifying callers. A positive entry is followed by `0x0098D750`'s body:

| Field | Representation |
| --- | --- |
| Behavior type and display ID | Two u32 words |
| Names | Four NUL-terminated byte strings |
| Icon, cast-bar caption, unnamed text | Three NUL-terminated byte strings |
| Behavior properties | 24 u32 words |
| Scale | One f32 |
| Quest items | Six u32 words |

The pinned `wow_world_messages` response schema has only six property words and
cannot decode this packet. Solarity decodes the native body directly. Native
`0x0047B480` uses a 1024-byte destination including the terminator; each string
therefore admits at most 1023 bytes. Byte strings remain uninterpreted, including
non-UTF-8 bytes. Packet decoding rejects truncated fields and trailing bytes
without reading into another encrypted frame.

Requests use `CMSG_GAMEOBJECT_QUERY` (`0x005E`), a u32 entry followed by a full
u64 instance GUID, serialized through the existing encrypted writer.

## Ownership and admission

Native M2 admission `0x00712F30` and WMO admission `0x00713130` register the
template callback after model initialization. `0x0067BD40` coalesces callers by
entry and returns an already completed template immediately. `0x00712AE0`
resolves the callback's live object before `0x00712400` attaches the result.
The destructor `0x00712B80` unregisters the object's callback.

`RuntimeGameplayCoordinator` owns the session cache. Model admission registers
each `GameObjectInstance` once, after resource and animation initialization.
Pending delivery uses weak references to instance-owned registrations; removing
an object cancels its delivery even if its GUID is reused. Display replacement
retains an existing template binding. Completed entries share one `Rc` rather
than copying names and properties per frame. The cache survives map replacement
and clears on disconnection. Missing replies remove the cache entry and do not
retry an existing binding; a new admission can request that entry again.

Requests remain queued until the bounded world writer accepts them. The frame
thread does not wait on network I/O and does not discard requests under pressure.
The native disk WDB persistence layer is outside this implementation.

## Transport clock

The create movement block's transport-progress word is retained instead of
skipped. Native `0x00714250` anchors it as an unsigned difference from the client
clock at creation. `GameObjectMovement` stores the wrapping difference and adds
it to the current client clock when queried, matching `0x007134A0`/`0x00711F20`.
Repeated creates for an existing GUID retain its current movement lifetime.

The native property lookup `0x00746190` and table at `0x00A36F34` identify type
15 properties 0, 1, 2, 5, and 8 as path ID, speed, acceleration, transport physics,
and stopping permission. Type 11 is the other animated transport family; type 7
is a chair. These owners retain template and clock inputs. The separate
[transport route provider](transport-routes.md) implements native construction,
sampling, physics, and stopping; live placement and passenger integration remain.

## Verification

Encrypted network tests cover all property words, string bounds, non-UTF-8 text,
missing replies, every truncated prefix, trailing bytes, and the following frame.
The writer test checks consecutive full-GUID requests. Runtime tests exercise
setup and live responses, same-entry sharing, removed callbacks, map replacement,
missing-entry admission, disconnect, and a deliberately full writer queue.
The movement update regression checks zero and wrapping clocks and GUID refresh.
