# October 2 footage and sound reference study

Status: reference review and source audit; no gameplay or renderer implementation in this pass.
Baseline inspected: `e11ec0e9027da7d37b7dfe08fa77b31de26df5c6`.

## Evidence and boundaries

Ten original screen recordings remain in `/home/alex/Downloads/`, named
`ScreenRecording_10-02-2026 HH-MM-SS_1.MP4`. The supplied museum recording remains
`/home/alex/Recordings/tiger1_tank_museum_sound.wav`.
`artifacts/runtime/instagram_references_20261002.json` indexes durations, paths and
six-frame contact sheets. Additional one-second strips exist for 17-51-07,
17-38-55 and 17-37-51 in `captures/`, with suffix `_20261002_seconds.jpg`.
All visual conclusions below come from these extracted frames; sparse sampling
cannot establish the absence of a short muzzle flash or measure an impulse.

These are edited Instagram montages, not calibrated recordings. Their captions
do not establish the identity/date/location of every inserted shot. Color and
sound provenance have not been established. The tall explosion at the end of
17-51-07 does not establish a measured turret height or vehicle identity.
17-55-14 shares winter sequences with 17-38-55; do not count them as independent
examples. Original audio has not been admitted as a shipping asset.

The museum WAV is 86.101 seconds, 48 kHz stereo PCM16, peak -1.067 dBFS, with no
full-scale samples. Signal analysis and an extracted audio preview do not
constitute a completed subjective listening comparison.

## Additional visual cues

- Winter composition (17-38-55, 0-5 s): broad open snow fields bounded by dense
  dark stands, foreground scrub and a distant woodland band. Vegetation density
  varies at field scale as well as individual clump scale.
- Ground history (17-38-55, 17-24 and 37-39 s): dirty wheel/track routes and
  exposed ground organize the white landscape. Snow cover is not uniformly deep.
  17-49-50 presents a thinner snow/stubble look than the snowy village sequence.
- White finish (17-38-55, 21-26 and 31-39 s): irregular light coverage, dirt and
  surviving dark regions retain panel and wheel detail. This suggests a winter
  paint layer in addition to track snow, not a uniformly white vehicle tint.
- Large-scale vegetation (17-37-51, 10-14 s; 17-40-58): taller stems partly hide
  the lower hull. Green, full vegetation here belongs to non-snow sequences;
  retain a dead-stem palette for the winter map.
- Discharge and ground interaction (17-51-07, 11-19 s): a pale airborne cloud and
  low dust around the vehicle occupy different regions. Distinguish directional
  propellant smoke, disturbed terrain and exhaust rather than one orange puff.
- Persistent occlusion (17-38-55, 12-20 s): white smoke persists after a shot and
  obscures parts of the vehicle/road while drifting and thinning. It changes what
  can be seen, rather than only decorating a hit point.
- Fire has stages (17-51-07, 20-24 s): an initially compact event expands rapidly,
  develops a rising column, then a rolling cap with darker material. Persistent
  surface burning, vented ammunition burnout and sudden blast need distinct
  behavior and shared fuel/energy bookkeeping.
- Environment also burns (17-38-55, 6-11 s): building fires and separate smoke
  columns give the entire settlement an evolving atmosphere.
- Human scale (17-38-55, 6-11 and 27-28 s; 17-37-51, 24-27 s): infantry appears in
  groups beside vehicles, across fields and among buildings. This supports the
  user's wish for more coordinated infantry presence, not just more tank targets.
- Surface contrast: bright snow/sky, dark running gear/openings and restrained
  intermediate materials make shapes readable. Preserve that hierarchy before
  increasing global saturation, bloom or fine detail.

## Confirmed implementation gaps

Read-only audits by AI agents Athena, Hermes and Demeter:

1. `weapon_vfx.rs` already supplies a pale 3.4 s propellant volume, but every
   cannon also gets 110 ms amber grains. `game.rs` independently supplies a
   90 ms legacy-calibrated muzzle light. A shared brief discharge envelope should
   drive visible emission and illumination, including actual weapon caliber.
2. `tank.rs::fired` animates the barrel; no hull recoil momentum enters the rigid
   ground-support solver. Whole-vehicle response needs supported body dynamics,
   not another independent camera shake.
3. Manual ram progress can jump to seated in one fast gesture. Actual movement
   needs a time-limited response distinct from the player's commanded progress.
4. `audio_feedback.rs` maps all four Tiger exterior gun variants to one generic
   explosion recording, and both interior variants to another. Preserve live
   propagation/interior filtering while establishing distinct dry report sources.
5. Ram/extraction scraping fires after the movement completes. Case drops use a
   hatch-stop recording and only the first bounce sounds with fixed parameters.
   Scrape should follow movement, with separate seating/release/contact events.
6. Healthy player tracks mostly use link thumps. Enemy T-34 track clatter is baked
   into an engine loop and remains present at rest. Separate travel cadence,
   engine RPM and load/friction character, including enemy source positions.
7. Some distant terrain/armor impacts discard the available physical contact
   distinctions and select generic events. Extend material/energy/outcome-aware
   source audio without breaking wavefront arrival timing.
8. Winter grass already has coherent patches, but is only 0.06-0.185 m high before
   0.025 m burial. Worldgen reduces wheat props from 1,423 to 10. Vegetation-mask
   coverage is 6.94% within 100 m of spawn and 2.62% within 250 m (whole-map
   coverage alone is misleading). Improve authored stand distribution and height
   variation without filling every field or reviving green summer grass.
9. The map already has 1,645 bare-tree placements, 160 pines and 249 buildings.
   Composition and species/stand diversity matter more than a blanket count bump.
10. Tiger exterior overrides currently force uniform charcoal grey. Winter-only
    patchy whitewash needs a source-local material mask that protects running
    gear, cables, exhaust and interior; constant whole-mesh overrides are inadequate.
11. Enemy catastrophe selection is seeded presentation rather than ammunition
    inventory/pressure/ring failure. Existing turret energy coupling deliberately
    yields a small toss. Large events require explicit energy/detachment modeling;
    the reference is not permission to assign an arbitrary launch velocity.

## Infantry incendiaries

The Tula Defence Museum documents Soviet infantry incendiary-bottle use in
1941-42: https://museum-tula.ru/muzei/muzej-oborony-tuly/3d/kokteyl-molotova/ .
The archival catalog records the July 7, 1941 decree on anti-tank incendiary
bottles: https://www.prlib.ru/item/1333819 . These establish historical use, not
universal squad equipment or effectiveness against this exact vehicle.

Game proposal: a limited infantry incendiary role, visible preparation/throw,
physical projectile contact, localized finite surface fire, smoke obscuration
and damage through the existing heat/fire model. Keep armor separation and
interior atmosphere meaningful. No guaranteed tank explosion on contact.

## Suggested order for implementation and review

1. Mechanical synchronization and audio identity: movement-linked loading,
   real casing contacts, independent engine/track sources and gun variants.
2. Shared muzzle emission envelope and terrain-interacting discharge volume;
   compare stationary exterior and cabin shots at the same exposure/weather.
3. Authored winter vegetation/ground composition and source-local whitewash;
   compare native 800x450 moving clips to catch thin-object shimmer.
4. Supported hull recoil, ammunition burnout and energy-based detachment;
   verify actual geometry motion as well as camera response.
5. Infantry incendiaries using the same surface-fire and occlusion systems.

Each stage requires game captures and listening review. This document records
the findings and proposed work; it does not claim those changes are implemented.

## Additional Tiger references supplied October 2

Originals: `/home/alex/Downloads/tiger1 sound 3.mp4` (33.45 s),
`tiger1 clip2.mp4` (17.83 s), and the three GIFs `xd7171hgyrnb1.gif`,
`b5p8vwbgyrnb1.gif`, `1ckdg18gyrnb1.gif`. SHA256/stream metadata and derived
artifact paths are in `artifacts/runtime/tiger_additional_references_20261002.json`.
The two MP4s contain stereo 48 kHz AAC. The GIFs contain no audio.

All 125 GIF frames were composited and examined by the visual audit agent;
root inspected their first 24-frame sheets as well. The frame/time markers below
refer only to these encoded GIFs. Original camera speed, exposure and retiming
are unknown, so these are shape/sequence references, not calibrated timing.

- `xd7171hgyrnb1`: first visible discharge at frame 17 (0.68 s), a large clipped
  white region near/beyond the muzzle. Pale haze subsequently spreads and
  obscures the building behind the gun. The saturated white cannot distinguish
  luminous flash from overexposed smoke by itself.
- `b5p8vwbgyrnb1`: first visible discharge at frame 18 (0.90 s), already a broad
  medium-grey cloud extending around the barrel and back over part of the tank.
  The near-ground haze is distinguishable from the upper cloud. No separately
  resolved bright flash is visible in the inspected sequence; this does not
  establish that no short flash occurred between exposures.
- `1ckdg18gyrnb1`: frame 3 (0.15 s) shows a compact dark cloud forward of the
  muzzle, subsequently broadening and becoming translucent. This is direct
  evidence against treating every shot's displayed smoke as the same white.
  Film/exposure/lighting prevent inferring a different propellant from its tone.
- Across the GIFs, fast initial expansion, lateral extent and sustained
  obscuration are more consistent cues than a particular shade. Implement
  neutral scattering/absorption with lighting and self-shadowing, not a fixed
  white or black billboard. Independent camera/film movement prevents taking
  apparent whole-frame displacement as a measured hull recoil impulse.
- `tiger1 clip2.mp4` is an edited montage and repeats sequences in earlier
  supplied clips, including the large terminal explosion. At approximately
  11.4 s it contains a brief bright muzzle region, followed by a pale spreading
  plume. The subsequent shot beginning around 12.1 s contains a separate large
  explosion. Do not use that explosion as the gun's normal discharge effect or
  treat this edit as a new independent catastrophic event.
- `tiger1 sound 3.mp4` shows a modern outdoor drive-by in several views: frontal
  approach, side, and close running gear. It offers movement/track-contact
  context for sound analysis. Vehicle identity, original recording provenance
  and exact engine configuration are not established by the filename alone.

Direct audio audition is unavailable through the current interface. Local
signal measurements can index frequency content and changes but cannot certify
the exact perceived engine/track timbre or distinguish a dubbed source by ear.
No reference audio has been copied into the game and no runtime code was changed.

The bounded signal analysis is saved in
`artifacts/runtime/tiger_audio_reference_analysis_20261002.json`, with a
spectrogram in `captures/tiger_audio_reference_spectrogram_20261002.png`.
In `sound 3`, approximately 84.46% of measured 20 Hz-12 kHz energy lies below
250 Hz, with independently changing upper-band detail. This supports preserving
a strong low-frequency body while varying mechanical detail, rather than a
constant bright noise layer. It does not identify individual sound sources by
ear or establish a target acoustic sound-pressure level. Separate the edited
camera shots when comparing; a large level drop around 31.1 s makes the final
tail unsuitable for a continuous machinery reference. The recording is nearly
mono, so it is not a reference for spatial imaging.

## Interior cutaway palette reference

User supplied `/tmp/codex-clipboard-e0e7b2fc-3480-4b1f-bae2-5d1db9550da8.png`.
A checksum-verified unchanged copy is preserved at
`artifacts/reference/tiger_cutaway_palette_20261002.png`, with provenance in
`artifacts/runtime/tiger_cutaway_palette_reference_20261002.json`.

The image appears to show a painted scale-model cutaway. It is useful for
material hierarchy but does not establish exact wartime paint codes or the
identity/function of every colored control. The removed roof also gives it
lighting unlike a closed playable tank interior.

Observed palette and proposed use:

- Warm ivory/light paint on much of the interior structure and turret ring.
- Dark desaturated green/blue-grey on selected mechanisms and equipment.
- Near-black fittings and recesses; preserve physical shadowing separately from
  material albedo rather than painting every dark area black.
- Restrained red on small controls, including the wheel the user highlighted.
- Different exposed-metal finishes on breech/rails and warm metallic cartridge
  cases, with localized edge/contact wear.
- Darker padded surfaces and a reddish lower compartment panel add separation
  without making the whole cabin a single uniformly colored surface.

Implementation direction: classify source components and give them distinct
paint, metal and padded-material properties. Keep roughness and localized wear
consistent with each material. Verify uncertain historical colors separately.
Do not reproduce this roofless photograph's brightness by adding unoccluded
interior light or weakening the existing armor shadows.

## Green-map color and composition target

User supplied `/home/alex/Downloads/myzrv4w4n3w31.jpg` and explicitly selected its
mix of gold, green and purple for the green maps. The image visibly credits
colorization to Julius Jääskeläinen. This is an art-direction target; its
colorization does not need to be original color film to serve that purpose.

The key relationships are golden sunlit grass and dry stems against olive/sage
green vegetation, small localized violet-magenta flower patches, red-brown brick
and worn pale plaster, cream/brown-grey smoke, and deep shaded foregrounds
contrasting with bright open distance. Purple belongs in selected plants and
small accents. Gold comes from vegetation/material variation and warm light,
while shaded areas retain depth and differentiated surfaces.

Preserved unchanged copy:
`artifacts/reference/green_map_gold_olive_violet_20261002.jpg`.
Provenance/checksum: `artifacts/runtime/green_map_palette_reference_20261002.json`.

The complete consolidated acceptance list, including this image, is
[REFERENCE_IMPLEMENTATION_CHECKLIST_20261002.md](/home/alex/panzer/canyon-v2-20260930/docs/REFERENCE_IMPLEMENTATION_CHECKLIST_20261002.md).
