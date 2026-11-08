# Temporal impact and field films — 2026-11-04

The local review gallery is `C:\shop\panzerrechnersystem\captures\videos\voxel-runtime-film-release-20261104-gallery.html`. Original exports remain untouched. This working-tree release is not a published build or a performance certification.

## Impact films

Six views have slow-inspection and separate seeded real-time replays: HEAT, HESH and APFSDS through the front plate; HEAT through the mantlet; HEAT toward the driver/radio compartment; and APCR with mantlet left, gunner and commander right. Crew are excluded from the visual section plane. Slow films have 1,443 frames at 60 fps (24.05 s), including the real-time opening, eased slowdown, steady inspection, return to real time and 5.25 s aftermath. Real-time companions have 399 frames at 60 fps.

`*-deep.mp4` copies preserve the native encoded picture stream. `tools/finish_cinematic_audio.py` stretches and lowers the main impact under the slow inspection, retaining quieter contact-driven fragment whizzes and ricochets. Ordinary-speed sound returns at the end. The native originals and receipts are retained.

The incoming body uses the FEA shot geometry. Its tracer keeps its swept path after contact, with the tail travelling toward the stopped front. The local plate and projectile advance only by the elapsed clock; interior sweeps and fragment/blast contacts cannot apply their damage before arrival. The mantlet is separated from the recoiling gun source part. Chemical impact flashes are brief enough to expose the cut plate during inspection.

The live local solver still has a calibrated prescribed penetration motion, separate shot contact mechanics, and a contact-triggered anatomy solve. It is not a fully coupled, experimentally validated material or injury model. HEAT is a simplified jet proxy rather than a computed charge collapse. HESH uses a coarse front-surface impulse. Modern profile dimensions and performance are authored game assumptions. The published nominal M829A2 velocity of 1,680 m/s is the only corresponding sourced specification; the authored 5 kg is not the complete cartridge mass.

The real-time APFSDS replay records 13 projectile fragment IDs crossing the hull's rear witness plane, including the main body, totalling 3.58568 kg. This is a result of this game setup, not a real ammunition prediction. HEAT sends only small fragments across that plane; HESH does not in this run. Receipts are in `target/temporal-impact-20261104/{cinematic-release,realtime-release}`.

## Color

The native display saturation default is 1.15. The stronger 1.25 option remains available. Earlier films have separate 1.25 saturation copies with original audio, preserving their original files and their older simulation provenance.

The bridge-field still tests preserve the map's authored May atmosphere: fog 0.00045/m, aerosol optical depth 0.2 at 550 nm, Angstrom exponent 1.6, scale height 1,500 m and overcast 0.06. `real=[1.35,0.85,15,7]` with blue-noise mode 1 is the outdoor reference preset. The editor's coarser `7,7,4` option remains available.

Two rendering defects were corrected: non-finite HDR history is rejected as a whole RGB sample, and the secondary bounce attachment uses the configured HDR format. The deltaP quantizer also now honors explicit blue-noise mode 1 instead of always forcing Bayer8. Paired frozen-frame tests preserve mean RGB to within 0.011 of one eight-bit code while removing the coherent rainbow waves. Normal gameplay keeps its adapting eye; the accelerated army timelapse locks its measured daylight exposure and smoothly adds up to six stops of night exposure from solar elevation, independent of flashes. The original fixed-exposure export is retained for comparison.

Wide battlefield review exposed another native rendering bug: the distant grass canopy had a negative depth bias (`constant=-16`, `slope_scale=-4`) that projected it metres in front of its real height and over painted tanks. The canopy now uses unbiased physical depth and yields where persistent tread/body contacts have laid the blades flat. Prone contact footprints use the same saved grass memory. Before/after native previews and batch-visibility logs are retained under `target/temporal-impact-20261104/army/`; this fix changes occlusion, not actor positions or casualty counts.

Daylight, dusk and frozen-grade/dither comparisons: `captures/screenshots/voxel-runtime-daylight-openfield-20261104-gallery.html` in the owner's main checkout. Color receipts and provenance are in `target/temporal-impact-20261104/{color-grade,daylight-openfield,daylight-native-grade,dither-fix}`.

## Large battle

`army-timelapse PREFIX 90 render` fields a finite roster of 96 tanks and 1,536 infantry in four opposing waves at 0, 180, 360 and 540 simulation seconds. It runs 900 s of combat at a 20 Hz vehicle/round clock and 10 Hz infantry clock, with three astronomical days compressed independently into 90 s and a 35 m forward camera dolly. The final frame grid is native 1920 × 1080 at 30 fps, encoded 4:4:4. Wrecks and bodies persist; casualties are never recycled or filled to a quota.

This distant-battle adapter shares the native Tank drivetrain, infantry micro-expert/ray-body combat and ballistic tables. Vehicle hit boxes and penetration knockouts are coarse; it does not run the close-up per-crew FEA for every unit. Infantry use source standing silhouettes and a small movement bob, with prone retained bodies. Gun effects follow real simulated discharge/contact events accumulated over each shutter interval. The film is silent to avoid compressing hundreds of reports into an audio wall.

The completed army simulation records 50 wrecked tanks, 1,056 downed infantry, 205 tank round contacts and 5,322 rifle rounds. The final export is `voxel-runtime-army-timelapse-night-balanced-20261104.mp4`: 2,700 frames, 90 s, with measured daylight exposure 5.372142 and a six-stop solar night bracket. Every recorded casualty ID is unique and occurs after its wave arrives. The original fixed-exposure export is retained.

The native AI interior films use the normal neural Tiger pilot and embodied crew against native T-34 opponents, with actual movement, firing and incoming hits. Their battle receipts record the outcomes. Saved range cameras are expressed in the hull frame, including turret-attached cameras; turret attachment rotates them around the pivot without adding its height twice.

## Validation and outstanding boundaries

The release build and 65 focused temporal, fragment, tracer, save/restore, effect, geometry, section, profile and shader checks passed; one pre-existing GPU review test is ignored. `tools/review_temporal_release.py` checked expected frame counts and fully decoded all 15 delivered films. The owner-folder receipt is `captures/videos/voxel-runtime-film-release-20261104-validation.json`. Visual checks remain separate from tests and frame counts.

No locked-60-FPS claim is made. Independent half-size uniform/skin anatomy layers remain outside this change, as recorded in `EDITOR_WEB_PARITY_20261104.md`. The web version remains a visual port with bounded gameplay approximations, not full native solver parity. Stable installation preserves the user's cameras and presets and leaves the editor closed.

## Local installation

The tested binary SHA256 is `b19f4a17146da35ea629a149cd00314e7ffa7b1a24afcdecca6d34dc69fcfb8e`. The stable editor receives this binary and the corresponding source changes. Its saved look changes only saturation from 1.0 to 1.15 and the dither selector from 0 to explicit blue-noise mode 1. Saved cameras, presets and tuning remain byte-identical; other state fields and launch quality remain unchanged. Installation hashes and backups are recorded in `target/temporal-impact-20261104/stable-install.json` in the working checkout.
