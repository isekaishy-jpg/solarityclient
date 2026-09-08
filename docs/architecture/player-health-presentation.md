# Player health presentation

Build 12340 keeps replicated `UNIT_FIELD_HEALTH` separately from the signed
prediction at `CUnit + FB0`. `UnitHealth` (`60EB60`, through `71C2C0`) selects
the prediction when `predictedHealth` is enabled. Its registration at `51F0F7`
uses a default of `1`. `UnitHealthMax` (`60EC60`) always returns the signed
replicated maximum.

`UnitHealthPrediction` retains this second value in ECS. Object projection
reconciles it when the health word changes, matching `73F330`; sparse power,
maximum-health and unchanged-health writes preserve it. `predict_unit_health`
implements `71C260`: signed wrapping addition, a minimum of one checked before
the signed maximum, and secondary unit flag `100` suppressing negative deltas.
This prediction does not change server-owned health or decide death.

The gameplay owner queues local-player health snapshots in packet order with
the other water/player notifications. Publication preserves the other UI
resources, then emits `UNIT_HEALTH` and `UNIT_MAXHEALTH` for changed replicated
values. The native field callback `60C240`, through `60BF10`, maps offsets
`48` and `68` to event indices `18` and `26`. Prediction-only publication does
not manufacture a field event: stock `UnitFrameHealthBar_OnUpdate` polls it.
Initial FrameXML publication includes the current prediction and ghost flag.

Local `UnitIsDead` follows signed replicated health, and `UnitIsGhost` follows
`PLAYER_FLAGS` bit `10`. The native local-player group-membership result makes
feign-death visibility retain actual health. These queries do not substitute
for the distinct native death/resurrection event and animation owners.

## Verification

`tools/ghidra/player_health_oracle.py` executes the original health Lua entries,
prediction function and field-event callbacks in the pinned executable. The
resident-unit lookup, local group membership, Lua I/O and final event delivery
are explicit provider boundaries; no client process or OS entry point runs.
Committed fixtures cover 40 Lua-query cases and 560 signed prediction cases.

Runtime tests compare Lua results against those fixtures, verify sparse ECS
reconciliation and queued snapshot ordering, and exercise the installed
`UnitFrame.lua` and `TextStatusBar.lua` through a real status bar. The archive
test checks prediction, reconciliation, maximum changes, zero health, displayed
text, and disabling prediction. Set `SOLARITY_STOCK_DATA_ROOT` to locally owned
build-12340 data and run `cargo test -p solarity-runtime --lib player_health_
-- --include-ignored` for the complete health check.
