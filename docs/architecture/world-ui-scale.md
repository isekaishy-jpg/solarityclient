# World UI root scale

FrameXML now binds the constructed UIParent and applies the native initial
scale before publishing its first layout. GlueXML keeps its existing canvas.
World GetScreenWidth and GetScreenHeight divide the base UI extent by the
retained root's effective scale, including after a script replaces the global
UIParent name. Child anchors, text, model viewports and input use the existing
effective-scale layout path.

The pinned client initializes this state in `5240E0`. Automatic scaling uses
the viewport height and configured aspect, with a 0.9 floor. Thus the reported
2560 by 1440 scene with default settings uses 0.9, whereas Solarity previously
left UIParent at 1. Manual initial settings below the native minimum fall back
to automatic scaling. The `useUiScale` and `uiScale` callbacks (`5237E0` and
`518B60`) instead clamp manual requests to 0.64 after the aspect cap. Changing
uiScale while manual scaling is disabled only changes the saved value.

`50F7C0` obtains the aspect from widescreen and gxResolution. `51FB80` and
`513240` propagate the scale and synchronously emit DISPLAY_SIZE_CHANGED.
The CVar setter `7668C0` invokes callbacks before committing the cached value,
so display handlers observe the new geometry with the previous CVar value.
Nested event dispatch restores the enclosing event and argument globals.
World screen queries follow `51AEF0`/`51AF50`; the Glue queries remain separate.

## Verification and boundary

`tools/ghidra/ui_scale_oracle.py` executes 504 original-code cases across
initialization and both callbacks, seven viewport sizes, widescreen on/off,
manual scaling on/off and six scale requests. The Rust policy matches the
captured float bits. The capture intercepts viewport, CVar and root/event
services; it does not emulate the complete native layout engine.

FrameXML scene tests cover saved settings, live callback ordering, nested
events, scaled screen queries, anchored control geometry, and pointer clicks
inside and outside resized controls. A GlueXML scene verifies its separate
scale behavior. The installed-archive FrameXML lifecycle validator also passes
startup, health/mana updates, scaled chat geometry, Escape-menu toggles and
pointer recovery. A 2560 by 1440 Durotar capture with two equipped NPCs verifies
the smaller root layout in both stationary and orbit views.
General window resizing and graphics-setting application
remain with their owning runtime/settings work; these checks do not claim
that those controls are implemented.

The real paper-doll DISPLAY_SIZE_CHANGED handler also requires RefreshUnit
(`597B00` -> `5977E0`). It now retains the existing unit selection and marks
selected model state for refresh; an unselected hidden widget has no unit work.
The FrameXML PlayerModel compositor remains part of the core UI stage, so this
startup dependency does not establish rendered paper-doll support.
