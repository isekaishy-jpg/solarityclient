# Process service ownership at startup

The combined cutover validation failed in
`application::application_starts_foundations_and_shuts_down_cleanly` with a stack
overflow. The preceding full compile and Clippy run passed, so those checks did
not establish usable startup.

Inspection of the actual MSVC debug object code found these stack reservations:

| Function | Pre-fix stack reservation |
| --- | ---: |
| ClientApplication::start_with_visibility | 0x96230 bytes |
| ClientServices::start | 0x7ee58 bytes |

Both frames coexist, with the caller's application storage and further startup
calls. `ClientApplication` previously embedded the full process service owner;
startup returned it through nested results by value.

`ClientServices::start` now returns `Box<ClientServices>`, and `ClientApplication`
retains that box. The long-lived process owner no longer moves through outer
startup results/caller storage by value. Subsystem construction order, shutdown
order, camera processing and frame execution remain unchanged. The allocation
occurs at startup, not per frame. This does not remove all startup-local storage
or establish a runtime FPS change.

The exact previously failing startup/shutdown test now passes (1 passed, 0 failed)
without changing Rust's test stack limit or the PE stack reserve. Formatting and
full workspace Clippy with warnings denied also pass. The full workspace suite
passes 1,692 tests with 0 failures and 33 existing ignored tests, including doc
tests, with no compiler or linker warnings. Evidence logs: ignored `target/admission-startup.stdout.log`,
`target/admission-startup.stderr.log`, and `target/admission-clippy.stderr.log`.
