# Act One Playtest routing

Policy module: `src/playtest_mode.rs`. The front end now integrates this policy with campaign construction, retry, completion and checkpoint routing. Runtime validation is recorded separately.

## Intended behavior

Playtest is the ordinary Act One campaign simulation entered directly into gameplay, with its one singleplayer crewman. It preserves the route, encounters, supplies, physical searches, mission objectives, damage, weather and ending requirements. It skips introductory presentation and stops after Act One completion. Campaign retains its existing multi-act behavior.

The old Playtest range remains available to explicit capture/development callers of `Game::build_playtest_act` and `playtest_setup`; those helpers must not be invoked by the user-facing Act One Playtest route. This preserves frozen mechanic fixtures without disguising a target sandbox as the real mission.

## Helper API

```rust
playtest_mode::uses_campaign(mode: PlayMode) -> bool
playtest_mode::skips_act_card(mode: PlayMode) -> bool
playtest_mode::completion_enters_next_act(mode: PlayMode) -> bool
playtest_mode::validate_act(mode: PlayMode, act: u8) -> anyhow::Result<()>
playtest_mode::checkpoint_path(mode: PlayMode) -> anyhow::Result<PathBuf>
```

`PLAYTEST_ACT` is 1. Campaign and Playtest use campaign simulation; only Playtest skips the act card. Only Campaign advances into another act. `validate_act` rejects non-Act-One manifests/checkpoints for Playtest before installation. Checkpoint paths retain the existing state-directory policy but use different filenames.

## Exact construction sequence

1. Choose `PlayMode::Playtest`, clear stale run/input/annotations as for a new campaign, and create `CampaignRun::new(seed)` using the existing reproducible seed policy. Select `act_manifest::playtest_manifest_path()` and require `Some(1)` through `validate_act`. Preserve that selected manifest throughout this session. The shader launcher currently selects the authored `act1-winter-approach/20260913-v1/manifest.v1.json` revision.
2. Resolve `world::act(1)` with the selected manifest, then apply the existing per-run seed mix. Prepare the attempt with `run.prepare_act_attempt(&map)` **before scene construction**, so route-bound buildings, start and exit positions use the same route.
3. Call `Game::build_campaign_act(device, queue, night, &map, &start)`, followed by `g.configure_campaign(&start, &mut renderer)`. That restores typed supplies and damage, places map cases, initializes the mission director, and benches encounter rigs until their world triggers activate. Do not call `playtest_setup` afterward: it erases campaign runtime and reinstalls the practice swarm.
4. Commit the prepared attempt only after build and restoration succeed: `run.commit_prepared_attempt(&start)`. Install the new Game/Renderer transactionally, configure the separately implemented single-player crew, then enter `AppState::Playing` directly. Ordinary Campaign retains ActCard.
5. Keep ordinary campaign signal ingestion, director updates, map HUD and physical interactions. Do not set `god`, `targets_only`, `debug_force_triggers`, return-fire OFF, or staging/heal helpers for the new mode. Existing explicit developer invincibility is a distinct setting, not a Playtest default.

## Mission and contact behavior that must survive

- Campaign construction (`game.rs`, `configure_campaign`) uses an attempt route rather than the practice spawn. It restores the same Tiger and typed inventory, and prevents replacing it with an abandoned hull to erase damage.
- Campaign enemies are triggered from the route. The first three guarded map sites require clearing their guards and physically recovering the corresponding folio while dismounted. A kill is not automatically a recovered sheet.
- Act One requires route selection, two city-layer observations, the blocking-force objective, the KV-2 objective and the exit objective (`campaign.rs`, `ACT1_OBJECTIVES`). The live exit also requires three map sheets, no current live wave, no remaining mandatory encounter, and the Tiger inside the extraction gate (`game.rs`, `update_campaign_director`).
- Do not replace those checks with `act.reached_goal()` alone, spawn six practice T-34s, or grant diagnostic progress. The earlier T-34 perception audit is separate and is not corrected by routing changes.

## Completion, death, restart and resume

**Success:** export live state into a candidate cloned from `run.persistent()`, then use ordinary `try_complete_act_with_snapshot(candidate)` so the same campaign objectives decide success. `Blocked` rearms the gate and keeps playing. For Playtest, consume `Advance` as an explicit Act One completion result and return to the menu/completion presentation; do not build Act Two or offer resume into an active Act Two. The normal Campaign route still follows `next_act`.

**Death:** call the ordinary `resolve_tiger_loss`/front-end loss path. Act One restores its entrance snapshot while retaining story knowledge and increments the next attempt. After the normal death presentation, rebuild Act One through prepare/build/configure/commit and return directly to Playing; do not route Playtest back to the old practice setup. A fresh New Playtest creates a new run. A retry keeps the existing run and checkpoint semantics.

**Pause/resume:** resume the same live simulation and selected manifest. It must not create a new run, reset damage/ammunition, or change the crew's identities. Preserve settings and controls overlays; remove misleading `PRACTICE / ENEMY FIRE OFF` wording and disable/remove the practice return-fire toggle for this route. The current Rust source has no separate tutorial state machine to bypass; ActCard is the identifiable introductory transition, and controls/map overlays remain player-invoked.

**Save/load:** use the isolated `playtest-act1-checkpoint.v1.json` sibling of `campaign-checkpoint.v1.json`. Both retain existing **act-entrance, same-attempt** save semantics; neither is an arbitrary mid-battle snapshot. Playtest loads must require Act One and restore Playtest mode plus its selected manifest. Ordinary title-menu campaign Load must continue reading the ordinary campaign path. Failure must restore the previous mode, run, selected manifest and time of day without replacing the live session.

The shader launcher already places state inside this worktree via `XDG_STATE_HOME`, but that only separates this worktree from the canonical game; a different filename is still needed to separate Campaign and Playtest within this worktree.

**Compatibility:** `campaign_save::current_compatibility` hashes `act_manifest::path_for_act`, including the selected runtime revision, its terrain and edits. The front end selects the manifest before decoding a save and restores the previous selection on failure. A changed winter revision therefore fails compatibility instead of silently loading canonical Act One.

## Crew boundary

Every singleplayer mode (Singleplayer, Act One Playtest and the other non-co-op, non-LAN entries) puts exactly one crewman aboard after world construction, including checkpoint retries (owner decision, 28 Sep 2026; this replaced the five-person roster). He works every post; the other seats are empty rather than dead, and his death ends the attempt. Do not use `set_lan_role` as a visual shortcut: it changes local station authority and bypasses some solo hatch/clamber paths. Verify the empty seats, hidden local body, the crewman's anatomy moving with the post he works, his physical casualty and no duplicate input ownership.

## Regression checks

1. New Playtest selects the winter Act One revision, has a live CampaignRuntime and CampaignRun attempt, and enters Playing without ActCard or practice setup. Verify the lone crewman's identity and local-body visibility separately.
2. No preplaced practice swarm, no return-fire OFF default, no automatic heal/refill. First contact activates only through the ordinary director trigger. Capture helpers retain their explicit practice behavior.
3. Playtest death restores an Act One checkpoint and rebuilds the crewman without advancing to Act Two. Failed build does not consume an attempt or destroy the previous live state.
4. Unmet objectives keep the exit closed; completing the real route ends Playtest as Act One. Campaign still advances Act One → Act Two and retains its final-act loss rules.
5. Save files differ; title campaign Load cannot resume the Playtest file. Playtest rejects Act Two/Three saves. Corrupt/incompatible saves preserve the current session. Selected winter manifest survives pause, retry and supported load.
6. Existing Campaign, Garage, Arena, raw map tests, LAN/couch and frozen mechanic captures preserve their original authority and entry routes. No unrelated T-34 visibility fixes are included.

The policy unit tests cover mode boundaries, Act-One validation and save-path isolation. They do not substitute for the integration and gameplay checks above.
