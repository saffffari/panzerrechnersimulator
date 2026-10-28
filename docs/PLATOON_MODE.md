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
enemy slot, stable for the run), `marker <name>` (`Wegpunkt N`, N the route knot index: the next
three knots after the route edge nearest the leader, at least 30 m away), or `left` / `right` /
`front` / `rear`. Every movement goal (advance, flank, fall back) is snapped to the nearest cell
the course measured clear for a tank within 80 m (`StreetNav::nearest_clear`), so an order into a
rock face becomes an order to the nearest drivable spot. Anything the context did not list is
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
the player. Movement orders halt for a contact inside 500 m (the Tiger cannot fire on the move);
`follow` halts within 60 m of its slot for a contact inside 900 m. There is no path planning along
the street grid for the wingmen yet (open item): goals are snapped to clear cells, the drive to
them is a straight line with avoidance.

## Prompt and context design

System prompt (fixed text, `radio_llm::system_prompt()`): the net, the leader's callsign, the
grammar table above with one-line meanings, `none` for non-orders, the target conventions, the
addressee rule, and the reply rules: one entry per addressed living tank, period tank radio
procedure, 3-14 words, the leader's language, acknowledge in the tank's own words, only facts
from that tank's context, no invented sightings, no chatter or modern slang, `Out.` / `Over.`,
personality colours word choice not length. Request: `max_tokens` 400, `output_config.effort`
`low`, `output_config.format` a JSON schema (`addressee`, `order` enum, `target`, `replies[]`),
one user message = the context JSON plus the transmission. Thinking is disabled (`thinking:
{type: "disabled"}`, accepted on Haiku 5.5 at effort `high` or below): a radio call is a small
classification plus one line, and thinking tokens both slowed the reply and ate the output
budget in the first live run. `max_tokens` is 800. No prefill, no tools.

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
(Haiku 5.5: $0.10 / $0.50 per MTok). Measured: 1,760-2,000 input tokens and 30-130 output tokens
per transmission, about $0.0002 on Haiku 5.5 (Sonnet 5.5 would be about $0.004, Opus 5.5 about
$0.008), median latency 1.8 s. Numbers from the live proof are in the verification section.

## What was verified

See the receipts in `captures/platoon_proof_*` (gitignored) and the section below, filled from
the proof run. Build: `cargo build --release --locked --bin panzerrechnersystem` on Windows
(`.cargo/config.toml` 64 MiB stack unchanged).

Proof run 2026-10-27 (`platoon-proof platoon_proof_20261027 29 200`, Windows 11 / RTX 4090,
assets from the main checkout, `KESSEL_NO_RT=1` for the stills, offline radio because this box has
no `ANTHROPIC_API_KEY`):

| Item | Result |
|---|---|
| Build | `cargo build --release --locked --bin panzerrechnersystem` passes; `.cargo/config.toml` unchanged |
| Unit tests | `cargo test --bin panzerrechnersystem -- radio_llm platoon`: grammar, addressee/target parsing, model-output validation, order application, navigation: all pass |
| Transmissions | 14 keyed by the script, 14 answered (`path=fallback reason=no_key`, 0-1 ms each) |
| Orders exercised | report_status, spot, follow, flank_right, cover_me, hold, engage, cease_fire, fall_back, advance, flank_left (the whole grammar) |
| Formation | both wingmen formed up and followed at 27-65 m while the leader drove the first 230 m at up to 8.4 m/s |
| Fighting | Anton engaged T-34 #7 at 480 m on his own initiative and was credited the kill; the leader's own kill count was 1 |
| fall_back | both tanks reversed/drove to 111-117 m behind the leader (goals snapped to the clear floor) |
| advance | Anton drove from 111 m behind to the next route knot (`Wegpunkt` ahead of the leader), arriving 7 m off the leader |
| flank_left / flank_right | Berta moved out toward the snapped flank goal; a contact inside 500 m halted her to fight, as designed |
| cease_fire / engage | weapons hold set and cleared; `engage` with nothing in sight answered "No target in sight. Over." |
| report_status / spot | replies carried the real round counts, damage, crew and the contacts in the tank's own line of sight (`T-34 #9 bearing 179, 550 metres, stationary; T-34 #12 bearing 165, 950 metres, moving`) |
| Captures | `captures/platoon_proof_20261027_t{002,030,070,110,160}.png` and `_final.png` (exterior chase view with the radio log drawn on the still), `_radio.jsonl`, `_trace.jsonl`, `_result.json` |
| Wall time | 131 s for 200 simulated seconds |

Caveat of this run: the rule-based leader pilot stalled after its first engagement (from t=30 s), so
formation following under movement is only evidenced for the first 30 s and in the recoveries
after orders.

### Live model run (same script, `ANTHROPIC_API_KEY` set, `claude-haiku-5-5`)

`platoon-proof platoon_proof_live_20261027 29 200`, 2026-10-27, after the request was tuned
(thinking disabled, `max_tokens` 800):

| Item | Measured |
|---|---|
| Transmissions / answered by the model | 14 / 14 (no fallback) |
| Latency per call | median 1.77 s, min 1.16 s, max 3.94 s |
| Tokens per call | 1,760-2,000 in, 29-127 out |
| Cost per call | $0.00019-0.00025 (Haiku 5.5 list price); $0.0031 for the whole run |
| Orders recognised | all 11; `advance to the next waypoint` resolved to `marker Wegpunkt 49` from the context, `Hartmann, how are you?` was classed `none` (the offline parser calls it `report_status`) |
| Replies | period register in the leader's language, personality visible (Berta: "flanking right, moving out", "eager for target"; Anton: terse counts), facts only from the context |

Three request-shape lessons, all now in the code: (1) with adaptive thinking left on, Haiku's
thinking tokens counted against a 400-token `max_tokens` and roughly a third of the replies came
back truncated (`bad_output_json: EOF ...`) or as a thinking block with no text; thinking is now
disabled for the radio and `max_tokens` is 800, after which 14/14 calls succeeded. (2) The model
sometimes returns a correct order with an empty `replies` array (4 of 14 calls, 29-33 output
tokens); the validator now fills a missing acknowledgement for every addressed living tank from
the offline templates and logs `filled n missing acknowledgement(s)`. (3) The API error message is
part of the fallback reason in the log, so a rejected request shape is diagnosable from the log
alone.

The first two live attempts were also useful as fallback proofs: a placeholder key gave HTTP 401
and a key pasted with masked bullet characters gave an empty HTTP 400 from the edge; in both
cases all 14 transmissions were answered by the offline parser in about 200 ms each.

Headless note: the proof's exterior stills run raster-only because the renderer's RT surface
pack panics on a degenerate instanced transform (`RT batch 4798 instance 703 ... transform is
non-invertible`) at t≈2 s on Canyon v2 in this headless capture path. Not a platoon issue; left
for the renderer owner. The stills also inherit the player's view exposure state, so dusk frames
read dark.


## Open items

- No live playtest with a human at the keyboard on this branch yet; the proof's leader is the
  rule-based `CanyonPilot`.
- Wingmen have no path planning along the street grid; goals are snapped to clear cells but the
  drive is a straight line, so a goal around a bend is reached only as far as the wall veto and
  the stuck watchdog get them. `StreetNav::path` exists and is the obvious next step.
- The headless proof's stills need `KESSEL_NO_RT=1` (renderer RT pack panic on a degenerate
  instanced transform in this path; see above). The leader pilot stalls after its first fight on
  seed 29, so a longer following run needs another seed or a human leader.
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
