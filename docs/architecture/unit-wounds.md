# Environmental wound animation

`73B140` classifies the visual kit's animation by `AnimationData` behavior.
Behaviors 8–10 enter `736640`. An ordinary hit chooses Wound (8), or
WoundCombat (9) when the unit's retained attack GUID is nonzero. An explicit
critical request chooses WoundCritical (10). The kit's literal animation 9
therefore does not by itself select WoundCombat.

This request leaves the primary animation running. `735820` passes a zero
final argument to `832AB0`, which installs the secondary timer through
`826DD0`. Its envelope starts at 0.75 and decays with native smoothstep over
one authored clip duration. The secondary clock uses the secondary clamp bit,
the before-scene one-tick start offset, and wrapping unsigned arithmetic.
It does not own primary completion callbacks or authored event dispatch.

`73E840` chooses semantic bone 4, then 6, then the root. `736640` chooses
between that upper-body group and the root using movement, posture, mount,
primary behavior and control state. A semantic key mapped to bone zero still
uses the root timer. Descendants inherit the nearest bone timer. Held-item
finger poses retain their primary override and inherit the wound contribution.
Swimming consequently retains lower-body motion while the upper body reacts.
The `7385C0` death branch clears the upper-body secondary; its root secondary
can continue through the primary death transition.

The animation owner retains these timers across presentation rebuilds.
The retained model now owns each explicit bone's primary and previous-pose
timers together. A wound on a seated upper body blends over that upper primary,
and the same slot reaches event-position and attachment sampling. Death removes
the wound; an outgoing upper primary can separately retain its native clear fade.
The systems resolver contains the native admission and group selection rules;
the runtime resolves authored tiers and variations using the shared CRT stream.
Environmental packet snapshots retain attack GUID and creature-template flags
at receipt, so a later attack-stop packet cannot rewrite a queued reaction.

Attack state comes from the real `143` start and `144`/`261` stop packet
layouts. Local packets also dispatch the original empty-argument
`PLAYER_ENTER_COMBAT` and `PLAYER_LEAVE_COMBAT` events. Creation reader
`4D3890` reads the packed target under update flag 4 and writes zero when
absent; `98E560` transfers that record into the unit owner. Ordinary movement
and duplicate remote creates preserve the live attack target. A raw-health
death callback clears it before subsequent notifications. GUID replacement
creates a fresh owner.

`tools/ghidra/unit_wound_oracle.py` executes the fingerprinted original code
for 68 routes, 20 timer starts, 112 inherited bone-clock samples, seven attack
packets, five creation records and 40 death-layer clears. Provider boundaries
are stated in the script. Portable tests compare route, clock, matrix and
encrypted packet results. Archive-dependent tests exercise both genders of
all ten playable races while standing and swimming, including a wound followed
by death and eventual resurrection into current movement.

The native active-spell-kit flag and vehicle-control admission inputs are
represented in the resolver. The current environmental effect owner has no
active spell-kit or vehicle-control provider. These inputs remain false until
those systems establish their owners. Attack swing scheduling, attack-stop
spell retirement, and the native action-bar refresh are separate combat work.
