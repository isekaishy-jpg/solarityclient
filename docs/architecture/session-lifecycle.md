# Logout and lost world connections

The behavioral reference is the build-12340 executable with SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.

## Logout admission and callbacks

`Logout` (`0x00510430`) and `Quit` (`0x00510450`) call `0x006B1930` with
different destinations. An active world and a clear pending flag admit the
normal empty `CMSG_LOGOUT_REQUEST` (`0x004B`). `ForceLogout`
(`0x005109F0 -> 0x006B21F0`) bypasses the pending guard, selects character
selection, and sends empty `CMSG_PLAYER_LOGOUT` (`0x004A`). It does not
close the socket. `ForceQuit` (`0x00510A00`) requests process termination.
Glue's separate `QuitGame` remains a process action.

`CancelLogout` (`0x0051AC90 -> 0x006B18C0`) sends empty `0x004E` and clears
pending immediately. `SMSG_LOGOUT_RESPONSE` (`0x004C`) contains the reason
DWORD and instant byte read by `0x004647E0`. `0x006B08B0` emits
`PLAYER_CAMPING` or the stock spelling `PLAYER_QUITING` for reason zero
and instant zero. A nonzero instant byte emits neither event. A nonzero
reason emits `ERR_LOGOUT_FAILED` through `UI_ERROR_MESSAGE` and then
clears pending. The existing FrameXML popups own their countdown display.
There is no client timer that fabricates successful logout.

`SMSG_LOGOUT_CANCEL_ACK` (`0x004F -> 0x006B0900`) only emits `LOGOUT_CANCEL`
while pending. It clears pending after Lua handlers return. A locally
cancelled request therefore does not emit another cancellation event when
its acknowledgment arrives. Keeping this ordering also prevents a normal
`Logout` called inside a failure/cancellation handler from bypassing the
still-set native pending flag.

`SMSG_LOGOUT_COMPLETE` (`0x004D -> 0x006B2180`) calls `0x006B1840` before
world retirement; that clears world admission and notifies pending camping
state. Normal completion returns to character selection through
`0x00406510(1, 1, 0)`. The quit destination then requests process termination.
FrameXML initialization `0x0052A980` sets the final-retirement flag;
`0x00528010`, reached when the local player is retired, emits
`PLAYER_LOGOUT` before `PLAYER_LEAVING_WORLD`. Ordinary map replacement
continues to use its existing world-leaving event without final UI logout.

## Connection ownership

Header encryption continues across logout. Authentication splits the cipher
into its independent send and receive halves. `WorldSessionDuplex` retains
the account, realm, entitlement and add-on manifest while the packet pump
borrows both I/O directions. The reader stops exactly after the complete
logout packet. The writer finishes accepted packets and consumes a queued
handoff command before the halves reunite. Enqueueing that command keeps
polling the writer, including when its bounded queue is full.

Packets and returned session ownership share an ordered channel. Earlier
logout callbacks reach Lua before final retirement, and bytes following
the completion packet remain available to the character-screen session.
No new authentication exchange or cipher initialization occurs. The existing
character-selection publication invokes the stock requests for account data,
character enumeration and realm split information.

## Lost connections

Native `0x004DA9D0` uses `0x00406510(0, 1, 0)` and then dispatches Glue's
`DISCONNECTED_FROM_SERVER`. `GlueParent.lua` returns to login and presents
its authored disconnect dialog. Both active-world I/O failure and closure
on the idle character screen now reach this lifecycle. Idle observation
polls cancellation-safe socket readiness for read closure or an error;
it does not consume encrypted headers, including buffered bytes preceding FIN.

Final retirement saves the available camera and CVar state, releases world
input and presentation resources, and drops the world UI and loading card.
The remembered realm label remains process/profile state. Logout retains the
authenticated transport; connection loss retires both login phases.

## Verification boundary

The Lua regression checks native admission, destinations, forced operations,
camping notifications, rejection and callback ordering. Encrypted loopback
tests cover all three client opcodes, bounded writer handoff, packet ordering,
metadata continuity, character-screen requests, a second world entry and
logout on the same connection, and active/idle peer loss. Additional socket
tests cover FIN with buffered unread bytes and an abortive RST.

`cargo run -p solarity-ui --example validate_logout -- <Data> enUS` loads
mounted stock FrameXML and clicks its actual Game Menu, countdown cancellation
and Quit Now buttons. It also dispatches final world-exit events and checks
for contained Lua callback failures, without a renderer or network login.

These checks establish the implemented lifecycle and wire continuity. They
do not establish unrelated AddOn persistence, secure-action provenance, or
every resource-specific native destructor.
