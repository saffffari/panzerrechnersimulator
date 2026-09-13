# Compass and equipment review fixtures

Prepared 2026-09-09. These source fixtures use the same actual Playtest constructor, loader
position, six practice targets and world manifest as `catalog/shader_reviews/loader.json`.
Their status is **pending runtime review**. Authoring and JSON checks do not demonstrate that
the intended pickup, camera framing or physical reach succeeds in the running game.

| Fixture under `catalog/shader_reviews/` | Duration | Review |
|---|---:|---|
| `compass_held.v1.json` | 3.5 s | Ordinary C raise, then three seconds of static head view; compass remains equipped. |
| `equipment_rack.v1.json` | 3 s | Turn toward the six interior tubes without changing inventory. |
| `toolkit_pickup_putdown.v1.json` | 6 s | Look at the box, LMB pickup, E put-down away from the mount, look at its actual resting position, LMB repick, E put-down. |

`compass` calls `Game::toggle_compass` and consumes the primary press, as the keyboard C
handler does. `look_equipment` accepts only `toolbox` or `panzerfaust_rack` and turns the existing
Loader head toward current geometry. It does not move the body, grant inventory, choose a
station or change equipment state. `equip` uses the public E equipment handler; `mg_coax`
is the recorder's existing LMB input field. Pickup remains subject to ordinary reach and
occlusion checks. Read-only state assertions cover compass/tool carry, persistent put-down
state, rack count, station, tank survival and zero main-gun shots.

Once a combined executable is available, validate each file before capturing:

```bash
target/release/panzerrechnersystem mechanic-validate catalog/shader_reviews/compass_held.v1.json
```

When GPU review is authorized, an example capture command is:

```bash
KESSEL_ASSET_ROOT=/home/alex/panzer/tank_simulator KESSEL_INTERNAL_RES=640x360 target/release/panzerrechnersystem mechanic-review captures/astra/compass_held catalog/shader_reviews/compass_held.v1.json
```

Use corresponding distinct output prefixes for the other fixtures. `mechanic-review` records
the actual renderer and writes its outcome receipt. `mechanic-evaluate` also initializes a GPU
device even though it does not record media; only `mechanic-validate` is a parsing-only path.
No build, simulation or capture was run while preparing this batch after host recovery.

Inspect the result for a readable compass, full-size interior stowage, a toolbox in the left
hand and hammer in the right, and the same tools remaining visible after put-down. The toolkit
fixture intentionally uses an undamaged tank: it does not validate repairs or exterior carry.
Current player bindings are documented in [CONTROLS.md](CONTROLS.md).
