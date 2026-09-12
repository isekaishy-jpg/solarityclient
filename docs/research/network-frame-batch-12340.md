# Build 12340 network service batch

Executable: `Wow.exe`, SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.
Inspection used Ghidra 12.1.2 decompilation and the corresponding x86 instructions.

`NetClient` source references identify the receive/dispatch family around
`00631D30`–`00633730`. The ordinary receive path in `00633330` enqueues event
`0x18` through `00633650`. That producer acquires the queue object's lock at
offset `+4` before allocating/linking the event:

```text
00633655  MOV EBX,ECX
00633658  LEA ECX,[EBX + 0x4]
0063365F  CALL 00774640
```

`006321A0` reaches the queue through `NetClient+2E34` and runs the dispatch body
also recovered at `006334F0`. The body acquires the same queue lock:

```text
006334F8  MOV ESI,ECX
006334FB  LEA EDI,[ESI + 0x4]
006334FE  MOV ECX,EDI
00633500  CALL 00774640
00633519  MOV EBX,[ESI + 0x34]
```

It walks the event list, dispatches `0x18` through the NetClient virtual slot
`+38` (`00632460` in the recovered connection table), handles the other connection
events, performs queue cleanup through `00633470`, and releases the lock through
`00774650`. There is no unlock around individual packet callbacks and no recovered
per-packet time budget in this loop. The producer therefore cannot refill this
pass from the network thread while dispatch holds the lock.

The runtime channel does not share a producer mutex, so its service snapshots
the currently queued count and retains arrivals beyond that boundary for the
next service. This preserves the finite batch boundary, packet order, existing
logout/transfer stops, and complete object-update transactions. It introduces
neither packet dropping nor a guessed millisecond quota. A packet can still
contain a large indivisible object update; this evidence does not justify
splitting that transaction.
