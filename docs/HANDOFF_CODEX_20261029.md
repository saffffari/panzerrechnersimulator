# Handoff to Codex — 29 October 2026

Panzerrechnersystem: a first-person Tiger I simulator in Rust + wgpu 27 (Vulkan) + winit, with a
"ray-traced PS1" look (low internal resolution, real lighting, Bayer dither, palette crush). The
owner is moving development to Codex. This note covers what a new agent needs that the code and
`CLAUDE.md` don't already say. Read `CLAUDE.md` and `AGENTS.md` first, then this, then the design map.

## 1. Where things are

- **GitHub:** `saffffari/panzerrechnersimulator`, branch `main` at **1681d66** at handoff, plus this
  documentation commit. `main` is the only source of truth; every feature branch below is merged.
- **Main checkout:** `C:\shop\panzerrechnersystem`. It holds the **git-ignored, owner-private asset
  library** (`/assets/*`, about 3.4 GB of purchased kits: Pizza Doggy, Rust & Blood, Echoes, ROT, and
  others). It is currently checked out on branch `raymarching` (the old renderer working branch), not `main`.
  Switch it to `main` (or a branch off main) before working there.
- **Worktrees** `C:\shop\panzerrechnersystem-<name>` exist for every merged feature branch
  (crew-solo, t34ai, vehdyn, lan, scatter, terrain, armor-scars, soundworld, rmmerge, integration, …).
  All are merged into `main`; treat them as stale. Worktrees have no assets: set
  `KESSEL_ASSET_ROOT=C:\shop\panzerrechnersystem` (the launcher does this automatically).
- **Owner's play build:** `C:\shop\panzerrechnersystem-gallery`, detached at 1681d66, built to
  `target\release\panzerrechnersystem.exe`. The window title shows `main <hash> (<date>)`.
- **Laptop package:** `C:\shop\panzerrechnersystem-gallery\dist\Panzerrechnersystem-win64-1681d66`
  (1,050 files, 2.1 GB, verified to build all six worlds from its own folder). The `-1b8f797` package
  beside it is superseded.
- **Raw sound libraries** the owner picks from: `C:\shop\tank_simulator\pizza_assets\psx_audio_assets\`.
  Chosen files are copied into the git-ignored `assets\sounds` and listed in
  `tools\windows_payload_traces\` so the package picks them up.

## 2. Hard rules (owner instructions)

- **Never commit, push, upload or attach assets or packaged builds.** Before every push, check
  `git diff --name-only origin/main main` for `assets/`, `dist/`, media and VDB files, and anything over 2 MB.
- **Screenshots and videos go flat into** `C:\shop\panzerrechnersystem\captures\screenshots\` and
  `...\captures\videos\` — no subfolders, prefix files from worktrees with the worktree name. After a
  capture batch run `powershell -ExecutionPolicy Bypass -File tools\sync_captures.ps1` (copies every
  checkout's captures/artifacts media there, keeps timestamps, never moves originals). `captures/` is git-ignored.
- **Don't show the owner test builds.** Test runs are headless. Only point them at a build built
  from pushed `main`, and say which fixes it contains and which are pending.
- **Kill game processes by PID, never by image name**, and run benchmarks from a renamed copy of the
  exe (`bench_*.exe`) so other cleanup can't kill them. One GPU process at a time; the owner also uses
  the 4090 for other games — if they say the GPU is busy, run nothing on it.
- **`ANTHROPIC_API_KEY`** is set at user level (platoon radio uses Claude Haiku via curl). Never print it.
- **Line endings: LF.** On Windows, Python `write_text` produces CRLF; use `newline='\n'`. A CRLF commit
  shows every line of a file as changed.
- **Push only verified, fast-forward `main`.** The workflow was: feature branch → build + `cargo test` +
  headless smoke → fast-forward `main` → push.

## 3. Build, run, measure

- Build: `cargo build --release --locked` (the 64 MiB main-thread stack is in `.cargo/config.toml`).
- Play: `tools\launch_windows.ps1` — `-Renderer raymarch` (the owner's renderer; the launcher's
  default is `shader`), `-Graphics Low|High|Ultra`, `-Windowed`, `-Logged` (session telemetry JSONL),
  `-Autopilot -Seed N -MaxSeconds N`, `-KesselEnv NAME=VALUE`, `-Host Canyon -Bind 127.0.0.1` / `-Join`.
  The launcher clears all inherited `KESSEL_*` variables; pass overrides with `-KesselEnv`.
- Presets live in `src/graphics.rs` (`GraphicsPreset`, `RaymarchQuality`); High is the reference.
- Headless harnesses (exe subcommands):
  - `gfxbench tag <frames> <out.png> <low|high|ultra>`: the preset is the 5th argument, not an env
    var. Add `KESSEL_BENCH_STATION=commander|driver|loader|gunner` for interiors.
  - `combat-scene-suite <prefix> <scene> <secs>`: video with audio. Scenes include winter_canyon,
    winter_canyon_inside, drive_day, urban_shadows, night_firelight and the small-arms variants. Its
    "missed observations" exit 1 is a scripted-acceptance check; the video is still usable.
  - `blizzard-battle`, `duel`, `brawl`, `crew-battle` (run from the main checkout's directory: it
    resolves terrain from the working tree), `canyon-ai-run` (data only, no video), `asset-trace`.
  - `ai-showreel` currently fails its own "camera mutated the shipping atmosphere" check.
- Perf tooling: `KESSEL_SESSION_LOG` + `KESSEL_GPU_PROFILE=1`, `tools/analyze_session_log.py`,
  `tools/sim_segments.py`, `tools/sim_calls.py`, `tools/summarize_gfxbench.py`.
- **Test baseline:** about 89–92 unit tests fail in a worktree without assets (missing asset files and
  stale source-text checks). Compare failure **name lists** against `main`, not counts.
- Latest headless Low smoke (Canyon seed 137, 300 s, 4090): frame p99 28.5 ms; sparks pass ≤1.2 ms.

## 4. Owner decisions made at handoff (supersede older docs)

1. **LAN co-op is tabled.** It works on Windows (host/join, discovery, firewall script, AI crew in
   empty seats) but has never been tested between two real machines, and many systems don't replicate
   (wounds, crew voice, casings, mission orders, scatter, wingmen, enemy infantry fire, weather).
   Don't spend effort on it unless asked.
2. **Written words are now allowed; spoken dialogue is not.** `CLAUDE.md`'s first principle still says
   the story is told "without a single spoken or written word". The owner said: "we're gonna need to
   bend that rule on words to tell the story. no spoken dialogue i think still feels right." **First
   task: amend that principle in `CLAUDE.md`** (and any docs that repeat it) so written orders,
   message logs, maps and the repair manual are legitimate, while spoken dialogue, voice-over and
   narration stay excluded. Show the owner the wording.
3. **The repair manual is the game's identity.** It's a physical book with symptom-led diagnosis and
   drawings of real parts (see the design map).
4. **What's missing is the dynamic planning side.** The owner described it like this: a much more
   detailed map, possibly several pages covering different zones. You receive an order such as
   "attack and hold a village". The village is marked on the map, but you work out how to get
   there, where to attack from, and which position to defend from.
5. **A large variety of mission types, and a war that reacts.** Each mission's outcome shifts the
   frontline, and the campaign adapts, so every playthrough plays out a bit differently.

## 5. Design direction

`docs/TIGER_NEXT_STAGE_DEVELOPMENT_MAP_20261028.md` was developed with Astra (another agent) and is
the current design direction, together with its research files in `docs/research/*_20261028.md`.
Items 2, 4 and 5 above extend or settle parts of it (written orders are no longer in conflict; map
planning and the adaptive frontline are the owner's chosen emphasis). Its staged plan still applies:
**human baseline → contrasting missions → one complete repair chain → a short persistent chain →
selective expansion**, each gated on observed player behaviour, not on automated runs.

Recommendations from the outgoing agent (not owner decisions):

- **Fix before any human playtest:**
  - walking inside zeroes the driver's orders (`crew_solo.rs`, driver order eligibility)
  - bypassed enemies get reported as "cleared"
  - the opening text still describes a lone crewman, though solo play has five living crew
- **First repair chain: tracks.** Physical track fitting already exists, track damage happens in
  ordinary combat, and it's the most common real field repair. Turret traverse would be second.
  Cooling needs engine internals the model doesn't have.
- **The map/orders slice is the natural Stage 1.** Use an existing compact area (Canyon v2 or an Act
  map): one multi-page map, three contrasting orders (attack and hold, hold a crossing, withdraw or
  recover), with route and position choice, and outcomes recorded truthfully (held, bypassed,
  withdrawn, destroyed).

## 6. Known open issues

- **Visual:**
  - a burning cabin is readable now, but still strongly orange
  - the lit lamp cubes added about 0.1–1.4 ms to `rm-point-face`
  - the SU-100 has no model files, so a box placeholder is used
- **Performance spikes:**
  - one 28 ms `tank.enemy_ai_contacts` step
  - the hull-strike sound synthesis takes about 9 ms
- **Audio:**
  - enemy guns heard while buttoned up are almost pure bass, with peaks near full scale (from the
    hull calibration, not the recordings)
  - the SU-100, KV-2 and IS guns still use the old sounds
  - sounds are ready but have no event to trigger them: headphones, crates, smoke dischargers,
    running over bodies and glass, 12.7–25 mm rounds, misfires
  - left out on purpose as non-diegetic: new_objective, level_up, craft, upgrade
- **Gameplay:** the canyon autopilot is slow, about 15–20% of the course in 300 s. It's only a test
  driver, but it's the smoke-test workhorse.

## 7. Context that's easy to miss

- **Physical-truth principle (`CLAUDE.md`, second principle):** no artistic fakes in the renderer or
  the simulation. Backends may not change an emitter's power. Presets change sampling and
  resolution, never the scene or the simulation.
- **The ray queries matter.** Without them the raymarch cabin used to render like open sky. That's
  fixed, but keep ray queries on wherever the adapter supports them.
- **Rendering pitfalls to remember:**
  - **sky visibility:** the static atlas must not retain meshes that move every tick
  - **fireflies:** stochastic sky-sharing samples need clamping
  - **glare:** flashes must not produce screen-space glare discs
- **Integration log:** `docs/INTEGRATION_LOG.md` has the full history, rounds 0–22.
- **The outgoing agent's notes** are in `C:\Users\saffari\.claude\projects\C--shop-tank-simulator\memory\`.
  `panzer-*.md` there covers the branch histories, gotchas and recipes. They're plain Markdown and
  worth skimming.
