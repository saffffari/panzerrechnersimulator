# Normal enemy report and independent KV-2 weapon reviews

Status: source and recipes prepared; no Cargo, GPU, simulation, recording, or audition has been run by this worker. Root owns the coordinated gate. The v18 nearby duplicate source fix remains intact.

## Normal enemy report

The older `incoming_impact` films inject an explicit terminal armor contact. They do not call normal enemy AI discharge and cannot prove the muzzle-report duplicate was removed.

The two new cases use the actual first-person Commander head-out view. A T-34-76 is staged on existing Arena terrain 80 m or 180 m ahead of the Tiger at (0,-180). Initial readiness is one second. Its detection meter starts empty. Normal stealth acquisition, scripted flanking (or installed enemy MLP), turret/gun slew, reload, `enemy_fire`, source muzzle, launch dispersion, authoritative swept projectile flight, and terminal contact all remain active. No action or terminal outcome is injected. The fixed initial terrain/tree/cover sightline must pass the actual Game LOS query; otherwise setup aborts without clearing or moving obstructions. Root must evaluate those fixed lanes before recording.

The unrelated opponent slots are moved to remote posts and held at their initial reload for this disclosed isolated review. Warmup sounds are discarded before the fixture is staged. The recorder rejects any discharge that occurred before its recording origin rather than silently resetting an event counter. The one pre-recording scene-submission tick still performs ordinary detection accrual.

`src/enemy_discharge_review.rs` observes bounded event records (maximum 128, with a visible overflow counter). Game tags only the actual calibrated report with the discharge identity and projectile ID. Every `EnemyGun` queued by a discharge is counted, so reintroducing the old generic close-range duplicate creates a queue surplus. Queue arrival observes both tagged and untagged `EnemyGun` sounds. Missing Audio, generic arrivals, duplicate identity arrivals, or unmatched arrivals remain failures.

Each event reports actual caliber, muzzle world position, projectile ID, discharge time, muzzle-to-threat-center distance, requested distance/343 m/s delay, expected arrival, actual queue release and error. Queue time is quantized to the normal 120 Hz update; a maximum 8.4 ms error is allowed in the recipes. Audio submission means the normal Audio path was called, not that an acceptable sound was heard. Hear-time bearing and occlusion still use the live camera. This preserves the current propagation distance convention instead of quietly changing acoustics while checking the duplicate.

The recipes require at least one discharge, calibrated arrival and Audio submission; exactly zero report surplus, pending reports, generic reports, duplicate/unmatched arrivals, missing Audio and event overflow; no player main-gun discharge; and actual shot ranges strictly below or above 120 m throughout the case. Initial setup distance alone cannot make that check pass. The later physical impact sound remains separate and may precede the muzzle report because the projectile travels faster than sound. Damage and observer death remain possible.

Recipes:

- `assets/review/first_person/fp_enemy_report_near_v19_20260905.json`
- `assets/review/first_person/fp_enemy_report_far_v19_20260905.json`

## KV-2 MG and HE are separate causal checks

Root's actual `captures/astra/kv2_commander_v18_eval_20260905_manifest.json` reported two MG rounds and zero MG damage, while one HE impact dealt 264 damage and killed all four German infantry by one second. The combined check failed. That receipt and its recipe have not been changed. Earlier on-foot MG damage cannot stand in for tank-to-infantry MG proof because the remaining on-foot player was a target after the infantry died.

The new optional `setup.soft_target_tank.he_ammo` is a whole number from zero through eight, defaulting to eight when absent. It initializes the real auxiliary inventory. It does not force an action, hit, target, or weapon-selection result. The existing policy observation reads that inventory fraction, and the existing firing gate consumes it. Zero HE rounds therefore presents a legitimate empty HE inventory to the same candidate policy while keeping 300 MG rounds.

- `assets/review/first_person/fp_kv2_mg_only_commander_v19_20260905.json`: same prior geometry and candidate, zero HE rounds; requires MG shots and actual damage by eight seconds, with zero HE launches, impacts and damage.
- `assets/review/first_person/fp_kv2_he_commander_v19_20260905.json`: same prior geometry and eight HE rounds; requires native HE launch, actual impact and damage, without claiming MG damage.

Both use the ordinary Commander observer, retain live player/tank damage, require explicit candidate weights, and preserve the initial eight-infantry roster. The full per-tank auxiliary inventory/target/aim/readiness is now included in capture telemetry as `tank_aux_inventory`, and the initial requested loadout is disclosed under `setup_results.soft_target_tank`. These source-ready scenarios may still fail the real candidate. No training or promotion was performed.

## Coordinated validation

`artifacts/astra-enemy-report-review-20260905-v19/queue.json` lists all four recipes with hashes and pending status. It deliberately has no binary or invented media path receipt.

Four new pure ReportAudit tests cover near/far arrival identity and timing, detection of a generic duplicate and missing Audio, bounded storage with visible overflow, and absence of enemy aiming/firing/outcome overrides in the fixture. Two preflight tests cover the enemy recipes' rejection of injected impacts and optional HE inventory validation including legacy omitted-field acceptance. Rustfmt parse and JSON parsing succeeded locally; compilation and those tests remain pending root's gate. Final video, visual action framing and combined sound quality remain separate required proof.
