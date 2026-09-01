# Test client

The user-facing test contract is a persistent build installed outside Cargo's
disposable `target` directory. Install or refresh it from the repository root:

```powershell
./scripts/install-test-client.ps1 -DataRoot "C:\wow\ChromieCraft_3.3.5a\Data"
```

The installer builds the `test-client` profile, copies the executable to
`%LOCALAPPDATA%\SolarityClient\testing`, and creates **Solarity Client
(Testing)** on the current user's Desktop. The shortcut always launches the
installed binary, records stdout/stderr under the installation's `logs`
directory, and retains a failing console until acknowledged.

The generated launcher contains explicit client-data, locale, realm, worker,
window, and GPU arguments. Re-run the installer after code changes; it updates
the stable binary and records the installed Git revision in `build-info.txt`.

## Vertical-slice gates

User testing advances through complete screen-to-screen behavior rather than
isolated rendering demonstrations:

1. Startup movie policy enters the correct next Glue screen.
2. Login has its stock `ModelFFX` scene, text, focus, editing, pointer input,
   audio, authentication status, and errors.
3. Realm and character selection render and respond to server state.
4. Character creation renders customization and submits valid choices.
5. Loading cards cover world admission and map transitions.
6. The initial in-world slice presents terrain, the controlled character,
   FrameXML, input, audio, and orderly disconnect/shutdown.

A window that only proves SDL/Vulkan startup is a developer smoke test and is
not considered a passing vertical slice.
