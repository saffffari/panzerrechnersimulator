# Platoon mode (`KESSEL_PLATOON=2`)

Branch `platoon` (off `raymarching`), 2026-10-27. Two AI-driven Tigers under the player's command
on the Canyon v2 winter run, ordered and answered over the radio. The game otherwise carries no
words; this mode is the one deliberate exception, and it keeps to radio procedure, not
conversation.

## How to run

```powershell
powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Platoon            # shader renderer
powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Platoon -Renderer raymarch
```

or the title menu entry **Platoon** (Canyon v2, three Tigers). Environment:

| Variable | Meaning |
|---|---|
| `KESSEL_PLATOON=2` | field two wingmen (the only supported size). Read at world build (ally rigs) and at run start. |
| `ANTHROPIC_API_KEY` | radio goes through the Claude model. Unset: the offline keyword parser answers. |
| `KESSEL_RADIO_MODEL` | model id, default `claude-haiku-5-5` (fastest current model; a radio call is a small classification plus one line). |
| `KESSEL_RADIO_TIMEOUT_MS` | per-call wall limit, default 6000. A timeout falls back to the offline parser for that message. |
| `KESSEL_RADIO_OFFLINE=1` | force the offline parser even with a key (deterministic proofs). |
| `KESSEL_RADIO_CURL` | path to `curl` if it is not on `PATH`. |

In play: **T** opens the typed radio line (display-resolution `ab_glyph` text, outside the PS1
crush, the menus' register). Type, **Enter** sends, **Escape** cancels, **Backspace** edits. The
driver's keys are released while the line is open so the hull does not creep. Replies appear
under your line, prefixed by callsign, and fade after 14 s. Keying the set and a reply breaking
squelch each play a short synthesized squelch burst (no radio clip exists in the audio library;
the burst is a plain stereo buffer, not a modelled receiver).

## The platoon

| Callsign | Commander | Personality (one line, given to the model) |
|---|---|---|
| Zugführer | you | – |
| Anton | Feldwebel Jupp Hartmann | laconic Eastern Front veteran; counts rounds, never raises his voice |
| Berta | Unteroffizier Karl Weiss | young, eager, reports more than asked, keen to be first on target |

Each wingman is a full Tiger: the baked `assets/tanks/tiger/baked/tiger_{hull,turret,gun}.glb`
rig (`tank::build_tiger`), `ArmorScheme::TigerI` in the shared swept armour contact, the
`ballistics` plate solver, the authoritative projectile path (`ProjectileShooter::Ally`) for its
own PzGr 39 rounds, 52 AP + 40 HE, a five-man crew and six modules (gun, turret traverse, engine,
tracks, radio, optics). Losses have consequences: no driver, engine or tracks and the tank sits;
no gunner, loader or gun and it cannot shoot; no turret traverse and it lays by hull like a
casemate; no commander or radio and it stops answering (the silence is the report). A crew
wiped out, or an ammunition/fuel fire, destroys the tank; it stays on the map as a burning hulk,
an occluder and a sound source like a Halbe ally.

## Architecture

```
player types "Anton, engage T-34 #2"          (main.rs: KeyT opens the line; Enter -> Game::platoon_transmit)
        |
        v
Game::platoon_radio_context()                 (platoon.rs: truth from the sim, per wingman)
        |  RadioContext {leader heading/speed/contacts, markers, wingmen[callsign, order, ammo,
        |                damage, crew_dead, contacts(bearing, range, state)]}
        v
RadioLlm::submit(text, ctx)  ----------------> worker thread "radio-llm" (radio_llm.rs)
                                                 |  online: curl -> POST /v1/messages (structured JSON)
                                                 |  else / error / timeout / refusal / off-grammar:
                                                 |     parse_offline(text, ctx)   (keyword parser)
                                                 v
                                               RadioResult {order?, replies[], path}
        ^
        |  Game::platoon_update() polls once per frame and applies at that frame boundary
        v
Wingman.standing / engage / weapons_hold      (platoon.rs: the deterministic AI reads these every frame)
HUD log lines + squelch                       (main.rs render_game draws them)
```

* `src/radio_llm.rs` (new): grammar types, context, worker, request/validation, offline parser.
  No sim access. The model never steers a tank; its output is `{addressee, order, target,
  replies[]}` and nothing else crosses.
* `src/platoon.rs` (new, child module of `game.rs` like `canyon_pilot.rs`): wingman state, order
  execution, driver, gunnery, damage model, radio-line input, HUD lines, proof telemetry.
* `src/platoon_proof.rs` (new): the headless proof.
* Hooks (small, in the sibling-edited files): `game.rs` declares the module and the
  `Game.platoon` field, builds the two ally rigs when `KESSEL_PLATOON=2` (the act 3 branch
  relaxed), calls `platoon_update` in place of `update_allied_tigers`, routes ally penetrations
  through the wingman damage model, adds the wingmen to the enemies' avoidance bodies, and makes
  `enemy_threat_center(i)` (nearest Tiger to enemy `i`) the target the T-34s track, lay on and
  accrue detection against. `live_playtest.rs` hands the winter encounter the same target.
  `main.rs`: module declarations, radio keys, HUD lines, menu action, `start_canyon` enables the
  platoon, the `platoon-proof` subcommand. `ui.rs`: the menu entry. `tools/launch_windows.ps1`:
  `-Platoon`.

Determinism: the sim is stepped at 60 Hz as before. Radio results are applied on the frame the
worker finished, so the only nondeterminism is *when* an order lands, and the proof logs the
frame (`applied_t`) for every result. With `KESSEL_RADIO_OFFLINE=1` the whole run is reproducible
from the seed and the script.

## Order grammar

| Order id | Meaning (as given to the model) | What the wingman does |
|---|---|---|
| `follow` | resume formation behind the leader | drives to its echelon slot (22/30 m back, 16 m out), stops at 60 m when a contact is inside 900 m |
| `hold` | stop where you are, keep weapons ready | halts; gun still works |
| `advance` | move to the target (marker, bearing, or forward along the route) | drives to the named `Wegpunkt`, 150 m along a bearing/sector, or the next route knot |
| `flank_left` / `flank_right` | swing wide to the leader's left/right and push forward | goal 90 m out and 120 m ahead of where the leader stood when ordered |
| `engage` | fire at the target contact or sector; if none given, nearest threat | clears weapons hold; prefers the named contact / bearing / sector, else nearest visible |
| `cover_me` | halt in a firing position and overwatch the leader's heading | halts, lays 400 m down the leader's heading, fires at anything it sees |
| `cease_fire` | stop shooting, keep tracking | weapons hold until `engage` or `cover_me` |
| `fall_back` | reverse out of contact toward the leader's rear | reverses toward a point 120 m behind the leader while in contact, then turns and drives |
| `report_status` | report ammunition, damage, crew | no movement change; the reply carries rounds, damage, dead crew, contact count |
| `spot` | report what you can see from your position | the reply names the contacts in its own line of sight (bearing, range, state) |

Targets: `bearing N` (compass degrees, clock positions converted against the leader's heading),
`contact <label>` where the label must match one the context listed (`T-34 #3`; the index is the
enemy slot, stable for the run), `marker <name>` (`Wegpunkt N`, the next three route knots ahead
of the leader), or `left` / `right` / `front` / `rear`. Anything the context did not list is
dropped to "no target" rather than invented. Addressee: `all`, or one callsign (the commander's
surname also works in the offline parser).

Engagement rules shared by every order: the gun fires only when laid (`Tank::ai_aim`, the same
tolerance the enemies use), stationary (no stabiliser), reloaded (9-11 s, deterministic jitter),
with AP left, with an optical line to the target, and with no friendly hull within 6 m of the
shot line out to the target. Dispersion is the same gun-relative jitter as `enemy_fire`, with a
speed term.

Driving: `navigate` (pivot above 0.7 rad error, else drive), the shared
`apply_vehicle_avoidance` against the leader, the other wingman and every active enemy, the
course's measured clear-cell test (`winter_patrol_point_clear`) as a wall veto, a stuck watchdog
(2.5 s of wanting to move without moving -> 1.6 s of reverse and swing), and hull-to-hull contact
with the leader and the enemies through the same inverse-mass pair solver the enemies use against
the player. There is no navmesh path planning for the wingmen yet (open item).

## Prompt and context design

System prompt (fixed text, `radio_llm::system_prompt()`): the net, the leader's callsign, the
grammar table above with one-line meanings, `none` for non-orders, the target conventions, the
addressee rule, and the reply rules: one entry per addressed living tank, period tank radio
procedure, 3-14 words, the leader's language, acknowledge in the tank's own words, only facts
from that tank's context, no invented sightings, no chatter or modern slang, `Out.` / `Over.`,
personality colours word choice not length. Request: `max_tokens` 400, `output_config.effort`
`low`, `output_config.format` a JSON schema (`addressee`, `order` enum, `target`, `replies[]`),
one user message = the context JSON plus the transmission. Thinking is left at the model's
default (adaptive); no prefill, no tools.

Context JSON (`RadioContext::to_json`), all read from the simulation at the moment of keying:

```json
{"leader":{"callsign":"Zugführer","heading":271,"speed_kph":18,"contacts":[...]},
 "clock":"day","weather":"winter overcast","markers":["Wegpunkt 5","Wegpunkt 6","Wegpunkt 7"],
 "wingmen":[{"callsign":"Anton","commander":"Feldwebel Jupp Hartmann","personality":"...",
   "alive":true,"bearing_from_leader":104,"range_from_leader_m":28,"heading":270,"speed_kph":17,
   "current_order":"follow","ammo":{"ap":51,"he":40},"hull_hits_taken":1,
   "damage":["radio out"],"crew_dead":["loader"],
   "contacts":[{"label":"T-34 #3","bearing":296,"range_m":612,"state":"moving"}]}, ...]}
```

Contacts are only enemies the tank can actually see (`optical_los_clear` from its own eye, within
1500 m), so a reply cannot report what the crew could not know. `alive` is false for a tank that
cannot answer (dead, or commander/radio gone); the validator drops any reply attributed to it.

Validation (`validate_output`): addressee must be `all` or a listed callsign; order must be in the
grammar or `none`; target is re-parsed against the context; replies from tanks not addressed or
not alive are dropped; reply text is capped at 160 characters. Any failure, HTTP error, timeout,
refusal (`stop_reason: refusal`) or bad JSON routes that transmission to the offline parser, and
the log line says so (`[radio] #n path=fallback reason=...`).

Offline parser (`parse_offline`): phrase lists per order with precedence (`hold fire` beats `hold`,
`cover` beats `engage`), English and German, addressee by callsign or surname, targets by bearing,
clock position, contact label / "tank two", marker name, or sector word. Replies are templates
reading the same context (status: rounds, damage, crew, contact count; spot: nearest two contacts
with bearing and range). Unit tests cover the grammar and the validator (`cargo test --bin
panzerrechnersystem radio_llm`).

## Transport and cost

The call is the system `curl` (Windows ships one; the mingw one also works) driven through a
config file on stdin (`-K -`), so the key never appears on a command line and no HTTP crate was
added (Cargo.lock untouched; four sibling branches are in flight). Each call spawns one process;
spawn cost is tens of milliseconds against a network round trip of hundreds. Per-call log line:

```
[radio] #3 path=llm model=claude-haiku-5-5 latency=<ms> in=<tokens> out=<tokens> cost=$0.000xxx order=engage to=Anton target=contact T-34 #2 replies=1
```

Cost is computed from the response's `usage` at the price table in `radio_llm.rs`
(Haiku 5.5: $0.10 / $0.50 per MTok). A context of this size is roughly 900-1300 input tokens and
40-120 output tokens, i.e. about $0.0002 per transmission on Haiku 5.5; Sonnet 5.5 would be
about $0.004, Opus 5.5 about $0.008. Measured numbers from the proof are in the next section.

## What was verified

See the receipts in `captures/platoon_proof_*` (gitignored) and the section below, filled from
the proof run. Build: `cargo build --release --locked --bin panzerrechnersystem` on Windows
(`.cargo/config.toml` 64 MiB stack unchanged).

PROOF_RESULTS_PLACEHOLDER

## Open items

- No live playtest with a human at the keyboard on this branch yet; the proof's leader is the
  rule-based `CanyonPilot`.
- Wingmen have no navmesh path planning; `advance`/`flank` goals that lie behind a rock face are
  reached only as far as the wall veto and the stuck watchdog get them.
- The wingman damage model behind a penetrated plate is a simplified deterministic seat/module
  risk model, not the player's fragment shower (ledger entry in `docs/PHYSICAL_TRUTH_AUDIT.md`).
  Their HE rounds are counted but never fired (no soft targets are chosen for the wingmen yet).
- Enemies track the nearest Tiger, but stealth signatures (engine noise, hatches, silent watch)
  are still the player's; a wingman's own engine does not raise the enemies' detection rate.
- A destroyed wingman is not converted into a `Wreck` (it keeps its ally rig pose; fire and
  occlusion come from the existing dead-ally paths).
- Campaign acts: the ally rigs build on any act with `KESSEL_PLATOON=2`, but only the Canyon run
  calls `enable_platoon`; wiring it into `start_play` for the acts is a one-line follow-up once
  the run-loop branch lands.
- Voice input and spoken replies (TTS) are out of scope; the line is typed.
- The curl transport should become a proper HTTP client once the lock-file churn is acceptable.
