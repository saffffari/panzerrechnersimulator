# Build Act One by hand

Choose **Editor** on the shader game's main menu, then **Act One world**. The chooser also offers **Tiger I garage**, **Tiger II garage**, **IS-2 garage**, **T-34/76 garage** and **Panzer IV garage**. The selected editor opens in another window and the menu remains available. A second launch from that game does not open a duplicate editor.

Direct commands, from any directory:

```sh
/home/alex/panzer/shader-graphics/tools/launch_shader_playtest.sh --editor
/home/alex/panzer/shader-graphics/tools/launch_shader_playtest.sh --garage tiger_i
```

Use `--menu` for the chooser. Add `--build` after source changes. The other garage IDs are `tiger_ii`, `is2`, `t34_76` and `panzer_iv`. These routes use this worktree's executable and the shared game asset warehouse; they do not launch the older ray-query build. Close the current launcher session before starting another one.

The default world is `/home/alex/panzer/shader-graphics/assets/worldgen/revisions/act1-winter-approach/20260913-v1/manifest.v1.json`. Set `KESSEL_PLAYTEST_MANIFEST` explicitly to edit another revision. The menu's world entry requires an Act One manifest; other acts remain available via the binary's `edit <manifest>` command.

The world starts in orbit 22 metres from the spawn, with compact selectable spawn/goal stakes. Actual Act One snow and fog remain visible. A tank garage starts as an isolated tank inspection scene, without loading a campaign map.

## Navigate and place props

The toolbar provides **Orbit**, **Walk**, **Inside**, **Assets**, **Lighting**, **Save**, **Play** and **Home view**. **Walk** gives a first-person crosshair for selection and placement in both the world and garages. Open **Assets** or press **Tab**, choose a model, close the browser, aim at a surface and press **V**. LMB selects the aimed object; G/R/T then move, rotate or scale it. Search `enigma` for the real **Enigma machine** under **Tank equipment**. The upper-right **High** or **Low** label reports graphics quality and the following numbers show render resolution.

| Action | Control |
| --- | --- |
| Walk / orbit camera | F or toolbar |
| Orbit | Hold MMB and drag; RMB also works |
| Pan orbit | Shift+MMB |
| Zoom orbit | Mouse wheel |
| World Walk movement | WASD; Shift faster |
| Garage Walk movement | WASD; Q/E down/up; Ctrl crouch; Shift faster |
| Look in first person | Mouse; Esc releases pointer |
| Inspect tank interior / Tiger crew position | Inside or F8 |
| Asset library | Assets or Tab; `/` searches; V places the selected asset after closing the browser |
| Select / multi-select | LMB / Shift+LMB |
| Move / rotate / scale selection | G / R / T; LMB confirms, Esc cancels |
| Tank-prop rotation axis | R, then X, Y or Z; mouse left/right rotates; default axis is Y |
| Duplicate / delete selection | C / X or Delete |
| Scatter vegetation or selected prop mixture | B; hold LMB; Shift+wheel changes density |
| Sculpt world terrain | H; 1–5 selects tool; wheel changes radius |
| Environment lighting | Lighting toolbar |
| Selected lamp range / intensity / temperature | T or [/] / − and = / comma and period |
| Selected lamp on/off / flicker | L / K |
| Save prop/light/sound edits | Save or F5 |
| Save sculpted world ground with selected props as a scene block | F7 |
| Reload saved document | F9 |
| Save and test | Play or F10 |
| Reset camera | Home view |

In the Tiger I garage, Inside starts at the loader camera: 1–5 select commander, gunner, loader, driver and radio positions. WASD drives the tank and arrow keys move the gun; switch to Walk for free inspection. Tiger II, IS-2 and Panzer IV use measured interior inspection positions with free camera movement. T-34/76 offers exterior inspection because it has no verified crew interior. While rotating a tank prop, X selects its rotation axis instead of deleting it; LMB commits and Esc restores the previous transform.

## Save and test the world

Start with one small site: place a building shell, inspect its approach at walking and tank height, then dress the interior. Save before moving to the next site. [The historical scene list](ACT_ONE_HISTORICAL_SCENES.md) provides twelve candidates and their date/location constraints.

World F5 writes `manifest_edits.json` beside the selected manifest. Uncaptured terrain needs F7 first; the editor reports this rather than discarding the sculpt. F7 needs selected scene props as well as the terrain edit. A changed procedural base requires rebasing the overlay; use the revision workflow before regenerating over hand edits.

World F10 saves and carries the loaded manifest's act ID and **exact revision path** into a separate game at the editor camera's map position. It clears inherited auto-Playtest flags. The editor stays open; close the test game to return. This is local map inspection. Start **Act One Playtest** from the menu to test the real mission route and progression.

## Save and test a tank garage

Garage F5 writes `assets/tanks/editor/<vehicle-id>/scene.v1.json` in the shader checkout, preserving attached prop transforms, local lamps and lighting. The default Tiger I scene also synchronizes its editable dressing file. Saves do not write into the separate canonical asset warehouse.

Tiger I F10 opens actual isolated Tiger gameplay using the saved scene; close that test window to return to editing. Other vehicles' Play saves and enters a source walkthrough in the editor; **Stop** or **F10** returns to editing. These source walkthroughs do not pretend to provide a missing gameplay rig. The complete Tiger II, IS-2 and Panzer IV source meshes currently use provisional neutral materials because their supplied OBJ archives contain no painted materials or textures.

See [the tank garage source inventory](TANK_GARAGE_EDITOR.md) for exact asset paths, rebuild commands and verification limits. Source conversion, the compiled Rust asset-import check and all five final v2 garage GPU checks passed. The GPU checks use actual editor methods and rendering with temporary documents and the production asset browser; they do not replay native mouse/keyboard events or launch full Tiger gameplay. The separate authoring MCP writes versioned world data and does not remotely control the live editor viewport.
