# Realm selection and login continuity

Change Realm previously opened `REALM_LIST_IN_PROGRESS` after world login had
consumed `RuntimeAuthenticatedLogin` and dropped the realmd stream. The subsequent
`NotAuthenticated` result was ignored, leaving the Cancel dialog indefinitely.
World login now receives a copied authenticated identity while the login
coordinator retains realmd. Choosing another realm disconnects the old world
session before authenticating the selected endpoint. Failed list requests publish
their localized error instead of silently keeping the progress dialog open.

The last received realm directory remains available while a refresh worker owns
the socket. Suggest Realm selects from those rows without requiring ownership of
the socket on the presentation thread. An unmatched preference opens the realm
list. The native selection routine at `0x004DE960` likewise uses retained realm
tables, not a transport-ownership test.

Before character enumeration, `GetSelectBackgroundModel` must obtain its fallback
from ChrRaces row 2. An empty default could make `SetBackgroundModel` call
`PlayGlueAmbience(nil)`, aborting the original `SUGGEST_REALM` handler before
`ChangeRealm`. Production Glue now uses its loaded race catalog for this query.
`selection_background_oracle.py` executes the original `0x004E3620` bounds and
row lookup with Lua adapters stubbed; six cases confirm the authored row-2 string
for an empty character list.

`realm_refresh_survives_world_login_and_realm_switch` performs real loopback SRP
and encrypted world authentication, refreshes the list after character selection,
then reconnects to a realm. `validate_glue_realm_selection` uses installed GlueXML
and DBCs to click both realm buttons, validate matching/nonmatching wizard
callbacks, and enter character selection without a supplied character fixture.

The separate black failed-connection text report was not reproduced. A real
refused TCP connection produced complete gold text, including its first presented
failure frame at 2560x1440. `capture_glue_dialog --connection-failure` retains that
diagnostic route. No text-color change was made for that report.
