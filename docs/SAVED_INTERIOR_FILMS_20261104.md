# Saved-camera interior films — 2026-11-04

The saved range cameras and settings are snapshotted into `target/interior-films-20261104-182651/`. The final plan renders ten clips: 76 mm BR-354P APCR, 85 mm BR-365 APHE and 88 mm PzGr.39 APCBC against the hull front, then 122 mm BR-471 and 152 mm BR-540 against the turret front. Each uses two saved interior cameras. All shots come directly from ahead at 100 m. Camera pose, focal length, lamp gain, materials and display look come from the owner's settings; the tank is closed, the sky held, and per-camera automatic exposure is metered then held during each shot.

The renderer uses its native 960 × 540 grid, two render samples per output frame, full transport and `KESSEL_REAL_STEPS=16,16,8`. Nearest-neighbor 2× output produces 1920 × 1080 video at 60 fps, H.264 4:4:4 CRF 12. Videos are silent. A half-second opening hold precedes 0.14 simulation seconds at 128× slow motion and 0.45 simulation seconds at 16×. Every output frame advances two substeps: approximately 65.1 microseconds in the first span and 520.8 microseconds in the aftermath. No real-time catch-up, skipped simulation steps or frame dropping is used.

`KESSEL_TISSUE_SOLVE=reference` selects the original 10 mm anatomy, 6,000-particle armour budget and eight cells across the projectile. This does not create the requested future separate uniform/skin layers or a 5 mm anatomy source.

A timing diagnostic exposed the interactive solver's 0.35 s penetration commit delay: it is a worker scheduling allowance, not physical flight through armour. For reference runs with `KESSEL_FEA_RANGE_FILM` set, stopped/penetrating solves commit at the hit and the update waits for completion. Ordinary interactive runs retain their existing scheduling. The first diagnostic clips, prefixed `interior-20261104-182131`, retain the old delay and are not the final film set.

Implementation: `src/game_fea_range/offline.rs`, dispatched by `KESSEL_FEA_RANGE_FILM` through `fea-range --shots`. Plan fields include exact saved cameras/settings, job shell/camera/aim/range, destinations and encoding settings. Metadata includes the actual first-surface contact, shell velocity, exposure, solved damage chain, frame count and wall time. Completed clips are atomically renamed from `.partial.mp4`; reports update after each completed clip.

Run an existing plan with the editor closed throughout:

```sh
python3 tools/render_saved_interiors.py /absolute/path/to/run-directory render --keep-closed
python3 tools/review_interior_films.py /absolute/path/to/run-directory
```

While a batch is running, `review_interior_films.py ... --partial` validates completed clips and publishes a clearly marked partial gallery. Later reviews reuse successful checks only when the video size and modification time still match, then check newly completed videos. The final review requires a complete render report.

The render binary is `/home/alex/panzer/fea-range/target/release/panzerrechnersystem`; it is separate from the installed interactive editor. The script requires an already prepared `plan.json`. Preview mode additionally requires `preview-plan.json` and leaves the editor closed after a successful preview so the final render can follow. All settings/camera snapshots and original files remain local and unchanged.

Validation reads every video with ffprobe, checks exact frame count/60 fps/dimensions, fully decodes with ffmpeg, exports review images and creates a local HTML gallery. Videos go directly to `/windows/shop/panzerrechnersystem/captures/videos/`; stills and contact sheets go directly to the sibling flat `screenshots/` folder. The gallery presents only the final batch, with exact camera names and round labels.


## Collision and motion revision

The replacement plan is `target/interior-feedback-final-20261104/plan.json`. It includes ten views of the five approved rounds, a separate BR-365 saved-camera-04 regression, and two 88 mm PzGr.40 longitudinal cutaways. Bow framing was reconstructed from the supplied reference; the turret uses saved camera 07. Existing camera files and prior videos are preserved. The output prefix is `voxel-runtime-interior-fixed-20261104`.

Blood and cabin sparks query the nearest actual tank or crew surface along each simulation step. Living figures use posed triangles; retained tissue and detached pieces use surviving voxel cells. Crew blood marks remain attached to their contacted surfaces. A blood contact seeds its first wall cell immediately, without the former minimum flight delay. Cabin sparks stop at the first surface, display one larger orange contact frame, and disappear on the next rendered frame; the physical damaging steel fragments retain their own dynamics. Shader spark emission is reduced, and fragment trails preserve a visible segment at tiny simulation steps. APHE compartment bursts now create their blast/smoke visuals.

Living wounded crew ease into the authored posture on simulation time. Retained bodies begin from the complete posed source skeleton and use those initial joint anchors. Tissue cuts account for the finite projectile cross section; joint detachment follows surviving tissue bonds. These changes correct demonstrated game behavior; they do not constitute a validated wound-ballistics model. The unchanged 10 mm anatomy and material assumptions remain limitations.

The section plane contains the incoming round's axis. The local armour patch is replaced with intermediate states recorded from the actual lattice solve, including temperature and projectile position. A 0.8 ms interval at 4096× makes the sub-millisecond deformation visible; it starts after 9.5 ms at 128×, followed by 50 ms at 128× and 250 ms at 16×. This avoids a long static lead-in at the slowest speed. Detached final-state lattice cells are removed after the local solve because the moving fragment pool owns them. These two inspection clips use steady scene lighting and omit the opaque exterior flash and final-state rim overlay so the plate remains visible. `KESSEL_RM_SKIP=16` disables volumetric glow in these two views; physical sparks and fragments remain. During the brief recorded lattice interval, terminal-state physical fragment drawing waits for the lattice to finish, avoiding duplicate steel. Interior clips keep event lighting and effects. The solver uses prescribed projectile motion and authored material laws; the cutaway is a diagnostic view of that model.

Focused validation is recorded in `target/feedback-final-tests.json`: 84 passing tests, with 8 existing ignored asset/diagnostic tests. Crew animation tests were rerun successfully with the real asset root after the first invocation lacked it. The final video receipt and full-decode checks live alongside the final plan. A successful render is not a locked-60 interactive performance measurement.


The final plan is split into three bounded render workers under the same run directory; their jobs have disjoint media filenames. Merge their receipts with `python3 tools/merge_interior_film_reports.py /absolute/path/to/run-directory` before generating the review gallery. The interior and section binaries have separate SHA-256 entries in `build-provenance.json`; their only difference is the offline lattice/fragment display handoff and added inspection metadata. Both use the same collision, crew and simulation changes. Source snapshots accompany the run. All renders remain local, and the editor remains closed.
