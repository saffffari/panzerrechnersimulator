# Panzerrechnersystem physical-truth ledger

Updated: 2026-11-03 (vision blocks: the relay panes' finite stops, glass on the periscopes, side slits and visor); 2026-11-02 (the player's own body: the Panzertruppe seen from its own eyes, head hidden for the owner alone; night lights: the rods retreat where a place is adapted to its own light, so fire-lit ground keeps its colour; the aerosol spectral by the Angstrom law and the raymarch surfaces crossing the clear air; the spring meadow: GPU blades over every green ground, flowers, butterflies, fireflies as modeled emitters; the Tiger's 1.112 visual-size debt named, and the damage physics held to true size; the layered damage model: CAD-measured components, the layered chain, lodged rounds drawn, saved and removable, the voxel lattice solver, persistent steel fragments taken in hand, compartment pressure on the hatches; fire, sparks, strike smoke and the effects pass under the deltaP look); 2026-11-01 (the deltaP display look; The March orders and the staged start fixture); 2026-10-28 (tank-hit recordings; library recordings for handling, breech, wounds, materials, footsteps; own-gun voice candidates; vehicle dynamics: per-vehicle spec, sprung hulls, AI drivetrain, head on the neck; enemy tank crews: fix and flank, armour solution, flash and engine cues; traced room acoustics, foley catalogue, engine direction; LAN on Windows: the living crew in empty co-op seats, laptop tier); 2026-10-27 (solo crew, wounds and first aid; armor scars; strewn-field scatter entry; terrain detail; platoon wingman model; missions: signal stars, smoke columns, alerts; raymarch backend ledger entry; plate-solve obliquity ceiling, Canyon run loop; binaural rendering row); 2026-10-01 (graphics, weather and impact integration). This ledger is an implementation receipt, not a vision document. A feature is
not "physical," "path traced," or "authoritative" until the shipping code computes it and the
listed gate passes. `CLAUDE.md` owns the acceptance rule.

Updated: 2026-11-03 (the radio sets' and driver's Braun-style panels: eleven finite panel lamps on the cabin light list, drawn-only slab fronts, glass panes; the radio operator seated on his cushion; the bow MG eyepiece relayed over its whole lens; vision blocks: the relay panes' finite stops, glass on the periscopes, side slits and visor); 2026-11-02 (the player's own body: the Panzertruppe seen from its own eyes, head hidden for the owner alone; night lights: the rods retreat where a place is adapted to its own light, so fire-lit ground keeps its colour; the aerosol spectral by the Angstrom law and the raymarch surfaces crossing the clear air; the spring meadow: GPU blades over every green ground, flowers, butterflies, fireflies as modeled emitters; the Tiger's 1.112 visual-size debt named, and the damage physics held to true size; the layered damage model: CAD-measured components, the layered chain, lodged rounds drawn, saved and removable, the voxel lattice solver, persistent steel fragments taken in hand, compartment pressure on the hatches; fire, sparks, strike smoke and the effects pass under the deltaP look); 2026-11-01 (the deltaP display look; The March orders and the staged start fixture); 2026-10-28 (tank-hit recordings; library recordings for handling, breech, wounds, materials, footsteps; own-gun voice candidates; vehicle dynamics: per-vehicle spec, sprung hulls, AI drivetrain, head on the neck; enemy tank crews: fix and flank, armour solution, flash and engine cues; traced room acoustics, foley catalogue, engine direction; LAN on Windows: the living crew in empty co-op seats, laptop tier); 2026-10-27 (solo crew, wounds and first aid; armor scars; strewn-field scatter entry; terrain detail; platoon wingman model; missions: signal stars, smoke columns, alerts; raymarch backend ledger entry; plate-solve obliquity ceiling, Canyon run loop; binaural rendering row); 2026-10-01 (graphics, weather and impact integration). This ledger is an implementation receipt, not a vision document. A feature is
not "physical," "path traced," or "authoritative" until the shipping code computes it and the
listed gate passes. `CLAUDE.md` owns the acceptance rule.

## The radio sets' and the driver's panels, 2026-11-03 (`src/cabin_panels.rs`, `cabin-panels`)

- **What it is:** the owner's design (a 1960s Braun audio set): brushed-silver fronts, black knobs,
  glass panes over the radio's tuning scales and the driver's two dials and fuel counter, amber
  pilot lamps and 8 mm red, green and turquoise indicators. Not a reconstruction of the FuG 5 sets
  or the Tiger's driver's instruments.
- **Light:** eleven finite cube lenses (`cabin_panels::LAMPS`: four amber pilot lamps, seven
  indicators) are cabin light registrations 22-32 (`CABIN_LIGHT_REGISTRATIONS` 22 to 33), sampled
  through `FiniteCubeLamp` like every cabin fixture (visible radiance and transported power from
  the same lens area; isotropic point approximation conserving power), switched by
  `set_cabin_lights` (`practical_gain`), shadowed by the scene (raymarch: one any-hit ray per
  lamp; shader backend: the packed cube maps with the lens excluded as its own emitter). Power is
  an appearance calibration in the cabin's scale, not a measured bulb: an amber lens emits
  `PANEL_AMBER` (85, 14 mm; the driver's 12 mm) against the dome lamp's 42 at 28 mm, about half the
  dome lamp's power each. The indicators' states come from the simulation (engine speed for the
  driver's oil and charging lamps, coolant at 100 C for the oil lamp); the radio transmitter's red
  lamp burns at 0.45 on standby with no transmit state wired yet.
- **Drawn-only geometry (named bound):** each front is a closed slab standing on its source box
  (`Hull_0355` radio rack, `Hull_0240` driver's panel), 20 mm (supply box), 24 mm (sets) and 32 mm
  (driver) deep, with knobs, keys and lamps up to a further 20 mm; collision, the damage model and
  the crew's reach still use the source boxes, so a fragment or ray crosses at most 52 mm of drawn
  panel before it meets the source face. The source's own relief (knob stubs to 20 mm) lies inside
  the slabs.
- **Instruments:** the driver's tachometer (0-3000 rpm) and coolant needle (40-120 C) read
  `player_powertrain.rpm` and `workshop_coolant_temperature`; the three amber drums read
  `tank_fuel_liters` as an odometer (each drum carries through the last unit of the one below).
  The radio's tuning needle and frequency knob are groundwork: `CabinPanels::set_radio_tuning`
  sets them; nothing in play tunes yet. The digital FUEL/L readout (`driver_panel.rs`, an
  own-emissive raster-only print with no light registration) is removed.
- **The radio operator's seat:** `crew_live_model` placed the AI radio operator at the pre-fit
  (-0.66, 0.38, 1.42): his boots through the belly plate, his seat 0.24 m inside the pedestal,
  the backrest through his neck. Now (-0.635, 0.635, 1.58), pelvis on the cushion, back on the
  backrest, eye 1.59 (the player's at that post 1.58). His shins still pass the pedestal's box at
  this scale, as the driver's do (open).
- **Bow MG eyepiece:** its relay painted the first two of the lens's 24 triangles (0.9 % of its
  area). It now paints the whole planar lens. The eyepiece is still a vision-block relay (its
  field visible from anywhere in front of it), not an `Eyepiece` with an exit pupil as the
  gunner's sight; as for the TZF, a telescope's eyepiece is no cabin aperture, and no daylight
  enters through it (named, open).


## Vision blocks: the relay panes' finite stops, and glass on every vision block, 2026-11-03 (`shaders/raster_optics.wgsl` `relay_trace`, `src/tiger_periscopes.rs` `relay_stops`, `src/tiger_side_ports.rs` `slab_thickness`, `shaders/rm_glass.wgsl`, `shaders/glass_scratches.wgsl`)

The owner, in the FEA range: the periscopes "shift like a real periscope would", but read as
screens: full frame from any angle, no glass. Before this change the raymarch backend (and the
shader backend) drew a cupola block's relay as an opaque image on the prism-side face of its
closed inner window, admitting every incident direction; past `OFF_AXIS_ACCEPTANCE` the pane
showed the acceptance boundary's radiance (named vision-block debt), and no glass was drawn
over it.

Now, for the seven cupola blocks and the loader's periscope (the eight ports with registered
stop chains):
- Finite stops. `relay_stops` reads each block's chain off its registered parts by tracing the
  close-view chief ray: the inner window's two faces, the lower mirror, the four relay lenses,
  the upper mirror, the objective window's two faces, each with its finite quad. The relay pane
  carries the eye's ray (and three more over the pixel's footprint) through them in order: Snell
  at each glass face, the mirrors and ideal thin lenses as the ray backend's `periscope_path`
  applies them (0.90 per mirror, 0.96 per lens, exact Fresnel and Beer absorption at the
  objective window's faces). A ray that leaves a stop's outline has met the housing: the pane
  shows the housing interior there. The cupola image is still sampled along the incident
  direction (the chain preserves it: exit directions within 4e-5), so the parallax is unchanged;
  the loader keeps its registered virtual-pupil mapping and takes only the chain's admission.
  Chained ports' captures are no longer clamped to the acceptance boundary: the capture keeps
  every incident direction and the chain decides.
- Named approximation: the housing walls are not traced; a ray inside every stop is taken as
  clear. Measured against the complete block with its housing triangles
  (`relay_stop_chain_matches_the_complete_housing`, eyes 0-75 degrees off axis at 3, 12 and
  45 cm, 225,792 rays): 211 disagree (0.093%), bound 0.15%.
- Named limitation: the housing interior's radiance is not computed; the relay draws it black.
  Light reaching the walls through the objective and the cabin is missing (a dark
  underestimate, never a brightening). The ray backend shades that hit as a surface.
- Result of the registered geometry, not a choice: the inner window is 4.7 cm tall, the relay
  lenses and objective pass a 3.5 cm bundle, so even on axis part of the pane shows the housing
  (51% of the pane admitted from 45 cm, 37% from 12 cm, 12% from 3 cm), falling to under 1% past
  50 degrees. The relay prescription itself remains the reconstructed one (29 September entry).
- The inner window's glass. The raymarch glass pass draws the closed 3.892 mm slab (the
  registered window) over its relay: each face as its own interface with half the slab's path,
  so the slab's Beer absorption counts once and both faces' Fresnel loss twice, and the second
  face's reflection appears as a ghost offset by the glass thickness. Its reflection, glints and
  grime are `rm_glass.wgsl`'s existing law with that pass's named approximations (screen-marched
  reflection with a ray-query miss, Schlick Fresnel, scalar transmittance in the blend). The
  relay takes neither face's loss again.
- Wear on the eye-side face (`glass_scratches.wgsl`, authored, not measured): straight grit
  grooves 4-40 um wide and 1-12 mm long, about 0.4% of the face. In the glass pass the grooves
  reflect the sun and the cabin lamps (modeled emitters, with their visibility) into a fan across
  each groove, and scatter back 20% of the light reaching them. In the relay a groove takes its
  share of the image out and passes 80% of the exterior light arriving across it, integrated by
  eight stratified taps over +-0.30 rad, each through the stop chain, from the port's own
  capture. Named limitation: directions outside the captured field add nothing (an
  underestimate), and the sun's disk is only found when a tap lands on it in the capture.
- The pane's sky: a vision block's glass asks the sky-cover map from inside the compartment
  (`rm_cabin_sky_visibility`), not the lifted open-ground test. That test reached above the
  cupola roof and lit the glass's reflection and grime with the whole sky; the housing interior
  read mid-grey by day (`periscope_optics_20261103a_day_*` against `..._b_noglass_*`).
- Direct-view vision blocks (owner, same day: "lets add that same glass to the side ports as
  well"): the turret's two side slit blocks and the driver's visor block take the same slab
  treatment in the glass pass (each face half the slab's path, the second face's ghost, the
  cabin sky test, the grooves on the faces toward the eye) with their measured thickness
  (`side_ports::slab_thickness`: slits 9.7 and 11.2 mm, visor 50 mm). Before, the pass took the
  bounding box's thinnest extent for the slit mesh (both slits in one mesh, across the turret)
  and every face carried the whole slab's absorption. Named: one mesh carries both slits, so one
  block's measure stands for both (1.5 mm apart, under 1% of the slab's absorption). What is
  seen through a direct block is the rendered scene behind it, so its slit walls darken steep
  views by their own geometry; the image through it is not laterally displaced by refraction
  (`rm_glass.wgsl`'s existing named approximation; a parallel slab keeps directions, the shift
  is 3-4 mm through a slit block at 45 degrees and 1.7 cm through the visor), and light turned
  back by total reflection at the blocks' edges is not traced.
- Not changed: radio and gunner optics, the driver's roof periscope (no registered chain; prior
  mapping). The shader backend's pane takes the same chain and grooves but has no glass pass
  over it.
- Gates: `relay_stop_chain_matches_the_complete_housing`,
  `direct_vision_glass_is_a_measured_closed_slab`, the raster optic and raymarch shader
  validation tests, `periscope-optics PREFIX` stills (held, seat, 30 and 60 degrees off the
  front block; the right side slit held and 50 degrees off; by day and at night with the cabin
  lamps on).

Updated: 2026-11-03 (the radio sets' and driver's Braun-style panels: eleven finite panel lamps on the cabin light list, drawn-only slab fronts, glass panes; the radio operator seated on his cushion; the bow MG eyepiece relayed over its whole lens); 2026-11-02 (the player's own body: the Panzertruppe seen from its own eyes, head hidden for the owner alone; night lights: the rods retreat where a place is adapted to its own light, so fire-lit ground keeps its colour; the aerosol spectral by the Angstrom law and the raymarch surfaces crossing the clear air; the spring meadow: GPU blades over every green ground, flowers, butterflies, fireflies as modeled emitters; the Tiger's 1.112 visual-size debt named, and the damage physics held to true size; the layered damage model: CAD-measured components, the layered chain, lodged rounds drawn, saved and removable, the voxel lattice solver, persistent steel fragments taken in hand, compartment pressure on the hatches; fire, sparks, strike smoke and the effects pass under the deltaP look); 2026-11-01 (the deltaP display look; The March orders and the staged start fixture); 2026-10-28 (tank-hit recordings; library recordings for handling, breech, wounds, materials, footsteps; own-gun voice candidates; vehicle dynamics: per-vehicle spec, sprung hulls, AI drivetrain, head on the neck; enemy tank crews: fix and flank, armour solution, flash and engine cues; traced room acoustics, foley catalogue, engine direction; LAN on Windows: the living crew in empty co-op seats, laptop tier); 2026-10-27 (solo crew, wounds and first aid; armor scars; strewn-field scatter entry; terrain detail; platoon wingman model; missions: signal stars, smoke columns, alerts; raymarch backend ledger entry; plate-solve obliquity ceiling, Canyon run loop; binaural rendering row); 2026-10-01 (graphics, weather and impact integration). This ledger is an implementation receipt, not a vision document. A feature is
not "physical," "path traced," or "authoritative" until the shipping code computes it and the
listed gate passes. `CLAUDE.md` owns the acceptance rule.

## The player's own body seen from his own eyes, 2026-11-02 (`src/first_person_arms.rs`, `fp-body-review`)

The owner's report: looking down, his body was "orange pointy things". Cause, measured: the
first-person body (the nisu CC0 infantry figure) hung from its neck joint with the camera 8 cm above
it and 4.5 cm forward, so the eye sat inside the collar and shoulders. Its nearest drawn vertex was
7 cm from the eye and 207 of 800 lower-hemisphere rays met body surfaces within 15 cm; drawn
double-sided at a 2.5 cm near plane, those were the screen-filling shards. Skinning was sound
(every joint index inside the 49-bone skeleton, every weight sum 1, longest posed arm edge 0.112 m).

Now the player's body is the reviewed AdrielCZ Panzertruppe (CC-BY-4.0, black Panzer uniform,
the crew's own source, face01 = the commander identity), standing at the cupola, loader and
dismounted posts and seated at the gunner, driver and radio posts. The camera is at its `Eyes_N1`
joint. Named rules and approximations:

- OWNER_HEAD_HIDDEN: the one source object that is the head (`PanzerTruppe_Head_N1`: skull, face,
  neck skin; 116 triangles, every vertex weighted wholly to Neck/Head/Eyes) is not drawn for the
  camera that owns the body. Nothing is shrunk or moved. Every other camera draws the whole man (the
  commander witness is the same source with its head). The headless first-person pages are still a
  raster-only shadow caster, so the owner's own shadow lacks a head (an existing limitation).
- NECK_FLEXION_EYE_TRAVEL: looking down, the neck takes 0.6 of the view's depression, at most
  0.80 rad, about `Neck_N1`; the eyes' forward travel (4.4 cm at most) moves the body back under the
  fixed game camera. Its drop is not modelled (the game owns the eye height).
- Walker stature: out of the tank the same source is scaled uniformly by `foot::EYE` / its eye
  height (1.70 / 1.415 = 1.20), so its eyes are the walker's simulated eyes and its feet his ground
  contact. Crew posts keep the reviewed 1.53 m Tiger-interior size. The two statures differ, as the
  walker eye and the crew bake already did.
- A crouching walker lowers the whole standing source (no crouch pose); the standing loader's feet
  stand 12.5 cm under the turret floor at full height (his eye is 1.29 m above it).
- The arm page now skins every triangle with any arm-joint weight. The former 0.35 single-influence
  rule left spread-weight palm triangles on the static page, hanging at the thighs.

Gate: `cargo test --bin panzerrechnersystem first_person_arms` (skin joints within the skeleton,
unit weights, no arm-moved vertex on the static page, head rule equals the head object, eye clear of
the drawn body by more than 10 cm at 0 to 1.45 rad down, posed arm edges within 1.6x of the source).
Review: `fp-body-review <prefix>`.

## Spectral aerosol and the surfaces' clear air, 2026-11-02 (`src/physical_environment.rs`, `shaders/physical_environment.wgsl`, `shaders/rm_medium.wgsl`, `shaders/rm_shade.wgsl` `fog_to`)

The owner: "more blue haze in the atmosphere its just white scatter right now". Two causes, both
transport, not colour: the aerosol was one wavelength-independent coefficient (grey), and on the
raymarch backend a surface's aerial perspective crossed only the map's weather layer
(`fog_density`): no Rayleigh and no atmospheric aerosol reached a hill, only the sky had them.

What the code computes now:

- **The aerosol is spectral** by the Angstrom law, beta(lambda) = beta_550 (lambda / 550 nm)^-alpha,
  for scattering and extinction alike, in the CPU reference, the shared WGSL atmosphere (sky, sky
  cache, the shader/physical backends' aerial perspective and media, the solar transmittance, the
  upper-atmosphere diffuse flux) and the raymarch backend's clear air. Default winter background
  alpha 1.0 at AOD 0.05 (550 nm; the 550 nm band is unchanged); the spring campaign profiles
  (acts 2, 3) alpha 1.5. Sources: AERONET fine-mode continental/urban alpha(440-870) about 1.2-2.0
  (Eck et al. 1999, JGR 104 D24; Dubovik et al. 2002, JAS 59); Moscow MSU monthly AOD(500 nm) 0.08
  in December-January to 0.3 in August (Chubarova et al. 2016, AMT 9, 313). The winter alpha is an
  estimate at the low end (humid winter growth enlarges the particles), not a measurement.
- **A map's aerosol** (`environment_lighting.aerosol: {"aod_550", "angstrom", "scale_height_m"}`,
  `environment_authoring::AerosolTuning`; absent: the campaign's) replaces the atmosphere's column
  (extinction = AOD / scale height at the local planet surface, y = -256 m; the albedo 0.9 and g 0.80
  kept) for every consumer above, and gives the weather layer's aerosol its exponent.
- **Raymarch surfaces cross the clear air** (`fog_to(..., surface = true)`): molecules (Rayleigh
  phase) and the atmosphere's aerosol (Henyey-Greenstein) at the frame's altitude profile, per band,
  lit by the isotropic sky and the key beam through the cloud shadow, with per-band transmittance.
  Sky pixels add none (the sky cache holds that whole path already).
- **The weather layer (`fog_density`) is the near-ground haze/snow extinction at 550 nm**: its snow
  share (the layer's snow fraction) is grey, the rest is aerosol with the map's Angstrom factor, in
  the froxel grid's scattering and the far march's extinction and scattering. The painted tint it
  carried, (0.94, 0.97, 1), is gone from the weather layer.

- **The air's higher scattering orders** (fixed the same day, after the merge): with the spring
  bridge's proposed block (AOD 0.2, alpha 1.6) the sky away from the afternoon Sun went slate grey
  and darker at the horizon than at the zenith. The atmosphere was single-scattering only: away
  from the Sun the aerosol's Henyey-Greenstein backscatter lobe is weak (0.006 /sr at g 0.8) while
  the horizontal path's optical depth saturates, so a hazier sky went darker where a real one
  whitens. Every view sample now also scatters the diffuse field, taken isotropic in each
  hemisphere: (F_down_diffuse) / (2 pi), the downward diffuse flux being the delta-Eddington
  two-stream transmission of the Rayleigh/aerosol/ozone column above the point for the key and
  secondary body, or under a cloud the diffuse flux leaving the measured cloud column
  (`multiple_scattering_radiance`, WGSL `physical_multiple_scattering_radiance`). CPU reference,
  AOD 0.2 alpha 1.6, Sun 22 degrees up, looking away: horizon (2 deg) / zenith green radiance 1.5
  before, 2.7 after, and the horizon whiter than the zenith; the default winter sky is 11 % (green)
  to 24 % (blue) brighter at the zenith and up to 1.3x/1.65x at the horizon. Gate:
  `physical_environment::tests::a_hazy_sky_whitens_toward_the_horizon_away_from_the_sun`.

Named approximations (limits):

- The diffuse field is isotropic in each hemisphere and only the downward flux from the column
  above enters it: the upward flux (the air below the point, the ground's reflection, snow's 0.8
  included) is left out, so hazy skies over bright ground, snow most, are still too dark low down;
  the cloud's column is evaluated at the sample's own footprint; no diffuse field once the bodies
  are below the horizon (twilight multiple scattering stays open).
- The three bands are point samples at 680, 550 and 440 nm (as the Rayleigh coefficients); the
  aerosol's single-scattering albedo and asymmetry are the same in every band.
- The froxel grid stores one extinction: inside its 320 m the medium's transmittance is the 550 nm
  band's (grey) while its scattering is spectral; the far march and the sky are spectral.
- The clear air inside the grid's reach is two steps composited behind the grid's media (smoke,
  weather, banks dim its in-scatter whole, second order in the two optical depths); beyond the grid
  six steps of weather plus air, then four steps of air alone to 40 km. No geometric occlusion of
  the beam in the clear air, no secondary body (the Moon while the Sun keys) in it, single
  scattering only, altitude as y above the local planet surface (flat over the map).
- The weather layer's albedo (0.70 aerosol, 0.92 snow) and asymmetry are the values it had,
  unsourced; the cabin haze and the plumes kept the (0.94, 0.97, 1) scattering tint until 2 November
  2026, when it was removed as unsourced (it cast the cabin blue): they scatter neutrally. The
  weather layer's values remain open.

Measured (spring-bridge-v2 copy, 15:28 local solar, looking east across the valley to the hills
1.8 km away, the locked look; sRGB means over the hill band): before (128, 131, 134 → B-R +6.7);
after, the map unchanged: (127, 138, 153 → B-R +25.7); with the proposed block (AOD 0.2, alpha 1.6,
1,500 m): (137, 148, 158 → B-R +20.8, hazier). Winter Canyon v2 at its authored noon: frame mean
red -2.2, blue +2.2 sRGB codes (the layer is mostly snow there). Cost: `rm-shade` 0.739 → 0.762 ms
(4090, 1024x576, `gfxbench day 240` on the bridge copy). Gates:
`physical_environment::tests::the_aerosol_is_spectral_by_the_angstrom_law_in_every_consumer`,
`a_bluer_aerosol_makes_a_distant_view_bluer_and_its_transmittance_redder`,
`environment_authoring::tests::a_map_aerosol_round_trips_and_absent_means_the_campaign`.

## Owner-directed effects of the cigarette, and the walking bob, 2026-11-02 (`src/cigarette.rs`, `src/hand_items.rs`, `src/foot.rs`, `src/renderer.rs`)

These are game effects the owner chose, not physiological models, named so they are never read as
simulation:

- While a smoked boost lasts (`cigarette::State`, seconds per toke) the man on foot moves 1.5 times as
  fast (`cigarette::BOOST_FOOT_SPEED`, `FootSoldier::speed_scale`); the tank is unaffected.
- The same boost already raised the display's exposure (x1.9) and saturation (x1.18)
  (`boosted_display_grade`); it now also doubles the rods' gain (`VISION_BOOST_NIGHT`), so it is felt as
  better night vision. The rod signal stays capped at the display's key in `crush.wgsl`.
- Walking and running move the camera and the held Luger with a gait bob (about 1 cm walking, 2.5 cm
  running, a sway once per stride pair, a third of it when aiming), from a gait clock advanced by the
  distance actually walked (`FootSoldier::gait_bob`). Presentation only: the man's eye for sight, the
  stealth model and collision use the unbobbed eye.

## Spring meadow, butterflies and fireflies, 2026-11-02 (`src/meadow.rs`, `src/meadow_fauna.rs`, `src/renderer_meadow.rs`, `shaders/rm_meadow.wgsl`)

On a map whose terrain block says `grass_season: "spring"` and on the raymarch backend, the three
legacy grass layers (`grass.rs`) are replaced by a GPU-generated meadow; winter maps, maps without
a season and the other backends keep the legacy grass unchanged (`Grass::enable_meadow` returns
before touching anything; `KESSEL_MEADOW=0` restores the legacy grass on a spring map for
comparison). What each part computes and what it does not:

| Part | What the code computes | Named approximation and source |
| --- | --- | --- |
| Ground cover | Grass presence is the map's grass mask, the ground class's red channel (3 x 3 smoothed), since 2 November 2026 (the owner paints the earth under the grass as earth); where a map has no ground class, the authored ground material's green (green over blue, not redder than green: `meadow::greenness`) stands in. Zero where the ground lies below a river's water surface (manifest `river` centreline, 5 cm margin). Stand height (lushness) from world-fixed 23 m and 7 m noise. | The mask is authored, not a vegetation survey. Test: `green_material_is_grass_and_soil_road_and_water_are_not`. |
| Blade geometry | Every blade is a closed-shading ribbon of 1-3 quadratic segments (5, 3 or 1 triangles), 0.30-0.60 m tall, 7 mm mean width tapering to a point, leaning 6-30 deg, rounded shading normal across its width. Roots sit on the drawn terrain grid's triangles (the same split as `Heightfield::grade_at`). | Not a leaf model: no midrib fold, no twist, no seed heads; the rounded normal is a shading device. Optics in the next row. |
| Leaf shading (kind 5) | A bi-Lambertian sheet with cuticle specular: the beam lights the face it falls on with the blade's diffuse reflectance rho and passes to the other face with its diffuse transmittance tau, both through the sun's visibility; a GGX cuticle lobe at n = 1.47 (f0 0.036, roughness 0.4) on the lit face; lamps (and firefly lanterns) the same way; diffuse sky on both faces, (rho + tau) x the environment. rho and tau are PROSPECT-D (Feret et al. 2017 after Jacquemoud & Baret 1990; its published coefficients at 680/550/440 nm, `leaf_optics.rs`) for three leaf states: fresh (N 1.5, Cab 40, Car 9 ug/cm^2, Cw 0.010 cm, Cm 0.004 g/cm^2: rho 0.036/0.153/0.041, tau 0.005/0.153/0.001), pale (Cab 20) and senescent (Cab 2, brown 0.8, Cw 0.002), mixed in world-fixed patches, on thin ground and toward old tips, every live blade at least 0.45 pale; 30 % of the blades are last year's standing dead (senescent straw, 0.6 of the live height, lodged further), as a May steppe sward carries its litter (since 2 November 2026, the owner: "a bit too saturated, a bit dryer"; the share is an estimate); each blade's vigour scales rho by 0.85-1.15. Canopy visibility: a uniform turbid canopy (spherical leaf angles, G = 0.5) with leaf area index density x width x mean height (conserved by the LOD), leaf area above a point L = LAI (1 - h/H); sky gap fraction 2 E3(G L) replaces the screen gather and the former 70 % lift; past the near cascade (24 m) the sun is also attenuated exp(-G L / cos theta_sun). | The sheet is Lambertian on both faces (no anisotropic sheen along the blade, no subsurface spread); the leaf parameters are estimates inside Jacquemoud & Ustin's (2019) ranges for grasses, not a measured species; flower petals take tau = 0.4 rho (an estimate). Beer-Lambert canopy visibility, not blade-resolved beyond the cascade; within it the drawn blades at the cascade's texel LOD shadow each other. Shading model after Baranoski & Rokne's survey (1998). The winter straw (kind 4) keeps its named wrap and lift unchanged. Tests: `leaf_optics::tests`. |
| Density and placement | 1200 blades/m^2 near the eye (since 2 November 2026, the owner: thick enough not to see the earth beneath; leaf area index ~3, gap fraction ~0.2 straight down, none at the oblique views from eye and commander height); a stand is full once the grass mask passes one half. Each 2 m world tile is seeded by an integer hash; blade j sits at the j-th point of an R2 low-discrepancy sequence, so every prefix of a tile is evenly spread and thinning never moves a root. | 900/m^2 is an estimate for a lush spring sward, not a field count. Placement is not ecological (no tussock structure). Tests: `meadow::tests::placement_is_world_fixed_and_counter_seeded`, `every_prefix_of_a_tiles_blades_is_evenly_spread`. |
| Level of detail | Footprint LOD: a blade is drawn at least 1.5 pixels (camera) or 1.5 texels (shadow maps) wide; where that is wider than the blade, the blade is widened and the density divided by the same factor, conserving blade area per ground area (hence the canopy's coverage). Tiles take the coarsest of 9 levels whose capacity holds their need; blades past the local count sink smoothly into the ground (no pop). 2 m tiles to 100 m, then 8 m tiles with their own seeds (cross-faded over 88-100 m) fading out by 140 m, where the canopy slab (next row) has risen over 90-140 m. | A geometry LOD, not a canopy BRDF: a widened far blade (0.3 m at 140 m) stands for many real ones; far tiles draw no flowers and cast no shadow. Tests: `lod_conserves_blade_area_and_never_exceeds_near_density`, `a_tiles_level_always_holds_the_density_it_needs`. |
| Far field: canopy slab | Past the blades the meadow is a 1-D homogeneous turbid canopy slab (SAIL-type: Verhoef 1984/1998; spherical leaf angles, G = 0.5, extinction G LAI / H) drawn as a shell at the canopy top (32 m tiles of 4 m quads to 600 m, 128 m tiles of 8 m quads to 3 km, no overlap), G-buffer kind 6 carrying the stand's PROSPECT rho, tau and LAI in the blades' mean mix (litter share, pale share, dry tips) with the flowers' petals mixed in by their area fraction (heads per m^2 x a head's area); it rises from the ground over 90-140 m while the blades fade. Shading: single scattering of the beam by the area scattering phase function Gamma of bi-Lambertian spherical leaves (Ross 1981; tabulated at 17 angles, `leaf_optics::gamma_tables`, energy-checked) integrated over the leaf area in eight steps with Kuusk's hotspot joint gap probability (hotspot parameter 0.02, leaf width over height), the soil (albedo 0.12) through both gaps, and multiple scattering as the Kubelka-Munk deep-canopy reflectance (two-stream, Sellers 1985) less a first-order share of omega/4; diffuse sky by the same two-stream reflectance. Bare ground (cover LAI < 0.1) discards the shell so roads and soil show the terrain. | Single-scatter plus a two-stream estimate, not SAIL's full four-stream solution; no cuticle specular in the far field; no lamps on the slab; the shell is the canopy top only where the terrain is its pixel (its 0.3-0.6 m rise above a distant ridge line is drawn as geometry, its side faces are not); T-junction cracks of centimetres where 4 m and 8 m quads meet at 600 m. The shell is drawn with a depth bias toward the eye (constant -16, slope -4): a 0.3-0.6 m rise above terrain hundreds of metres off is below the depth buffer's resolution there, and lost to the terrain without it. Tests: `the_canopy_phase_function_conserves_energy_and_backscatters_the_reflection`, `the_far_canopy_reads_as_a_green_meadow_with_a_hotspot`. |
| Sun exposure | Per 5-10 m cell (512 x 512 over the map, computed at install from the drawn terrain and the manifest's tree placements), the growing season's direct-sun dose on the slope over the dose on open flat ground: mid-May hourly sun directions at the act's site from the ephemeris (`ephemeris::CelestialState`), each through the slope's cosine, the beam thinned by exp(-0.2 x air mass), and blocked where the terrain or a tree crown (12 m deciduous / 14 m pine, 3.5 m radius, times its placement scale) stands above the line to the sun within 240 m; weighted 0.8, plus the sky view factor (mean cos^2 of the horizon over eight azimuths) weighted 0.2. Exposure drives the leaf state: below 0.6 (shade, far slopes) live blades stay green, toward 1.1 (open ground in full sun, sun-facing slopes) up to 0.7 of a live blade is senescent straw (drought and radiation load dry exposed steppe swards first, sun-facing slopes most: Holland & Steyn 1975, J. Biogeogr. 2:179). The far slab takes the same mix. | One representative day, no clouds, no soil water model; tree heights and crowns are estimates (the placements carry no crown size); buildings and wrecks are not occluders; the dose-to-dryness mapping's numbers are estimates chosen with the owner ("high sun exposure regions lean towards tan and shaded regions green"). Test: `exposure_is_high_on_sun_slopes_and_low_on_far_slopes_and_under_trees`. |
| Tussocks | A world-fixed Voronoi tessellation of jittered seeds 0.9 m apart (after the clump attributes of Ghost of Tsushima's grass, Wohllaib, GDC 2021): each blade takes its clump's height (0.80-1.15, lower toward the clump's rim), fans out from the clump's centre with a lean growing toward the rim, and shifts its leaf state with the clump; its R2 root is unchanged. Flowers favour the gaps between clumps (0.45 of the rate inside one, 1.25 on a gap), on the CPU (`meadow::flower_at`, where butterflies land) and the GPU alike. The canopy's mean blade height is taken as 0.75 of the stand height for its leaf area index. | A visual clump structure, not a tiller or rhizome model; tussock size is one constant. Test: `tussocks_are_world_fixed_and_flowers_favour_their_gaps`. |
| Wind | A 64 x 64 grid of 4 m cells around the eye integrates a spring per cell (1.4 Hz, damping ratio 0.3) toward a drag deflection proportional to u^2 (capped at 0.34 m for a 0.48 m reference blade), driven by the act's wind times a gust factor: world-fixed noise of 28 m and 9 m carried downwind at the mean wind (Taylor's frozen turbulence; the finer octave at 0.8 of it), streamwise intensity 0.28. Pressure fronts kick the cells as before. Each blade adds a flutter at a hashed 2.5-4.5 Hz, amplitude proportional to wind speed. | Frequencies and damping are estimates in the range of cereal and meadow canopies (Py, de Langre & Moulia 2006, J. Fluid Mech. 568, honami); the gust field is a frozen-turbulence model, not a solved boundary layer; the flutter is a harmonic per blade, not an oscillator; blade length is kept by renormalising the tip. Test: `the_wind_grid_bends_downwind_rings_down_and_keeps_its_state_when_recentred`. |
| Treads | The grass contact memory (`grass_filament::TrackMemory`, 25 cm cells with the tread's heading) is mirrored page by page into a GPU page table and atlas; a blade on a contacted cell lies along the heading (spread +-26 deg), its tip 3-6 cm above the ground; flowers there are crushed (not drawn). | Flattening is permanent (no recovery); tread contact is the existing two-strip footprint; 4096 atlas pages (262 000 m^2 of tread) at most, further pages counted and not drawn flattened. Test: `tread_pages_get_stable_atlas_slots`. |
| Hulls and walkers | Every hull in the meadow (the player's, enemies', allies', moving or standing, by its `hull_half_extents`) and every man on foot press a grid of 0.5 m cells, 80 m across round the eye: blades under a hull and within 0.35 m of its sides lie away from its nearest side (tip displaced up to 0.40 m on the 0.48 m reference blade), those within 0.45 m of a walker away from him; once free they spring back by Hooke's law, critically damped at 1.5 rad/s (about 3 s), as Jahrmann & Wimmer (2017, I3D, "Responsive real-time grass rendering for general 3D scenes") recover bent blades; treads lodge them for good (the row above). | The recovery rate is an estimate for unbroken stems; no blade breaks; the hull is its footprint box, not its mesh; walkers are a 0.45 m disc; cells beyond 40 m of the eye are not pressed. Test: `a_hull_lays_the_grass_aside_and_it_springs_back`. |
| Shadows and secondary rays | Blades cast into the near sun cascade and the sky-cover map within 24 m of the eye, at those maps' own texel LOD. The ray queries see the meadow as the same turbid medium: a grid of ground height, canopy height H and leaf area index per terrain node (`meadow::canopy_grid`) is bound to the shade and reflection passes; an occlusion (ambient) ray that escapes the scene, a lamp's shadow ray, and an off-screen reflection ray are multiplied by exp(-G LAI / H x the path inside the canopy) over their first 12 m (six samples), the reflection taking the deep canopy's two-stream radiance under the sun and sky for the share it hides. | The blades are not in the TLAS: the meadow is statistically present to the rays (Beer-Lambert in a homogeneous slab, no blade-resolved occlusion, the slab's stand averaged over a 4.9 m node); only the first 12 m of a ray are tested; the point-light faces take no meadow; flowers cast no shadow. Test: `the_canopy_grid_carries_the_stand_and_nothing_on_bare_ground`. |
| Flowers | Five species (poppy, cornflower, chamomile, buttercup, meadow sage), each with its own clump field (world-fixed two-octave noise through a band); a slot takes species s with probability = its clump density / 1 per m^2, so species clump; since 2 November 2026 (the owner: "too many flowers ... a spec here and there") the clumps are rare (the noise band 0.71-0.88) and sparse (0.3-0.7 heads per m^2 at a clump's heart, at most four per 2 m tile): about 0.05 heads per m^2 over the meadow, most of it none; a two-segment stem and a six-triangle head (centre colour to petal colour) tilted 30-55 deg, swaying with the wind grid. Beyond the footprint the head is widened to 1.5 px and the slots thinned by the square of the widening (coloured area conserved); drawn to 60 m. | Display-equivalent pigment albedos, not measured petal spectra; geometry is a fan, not petals; no phenology. Test: `flowers_clump_by_species_and_respect_bare_ground`. |
| Butterflies | Four species, two real wing meshes each (raster only, no shadow, no body), flapping at 9-11 Hz (small pierids and nymphalids beat at ~8-12 Hz; Dudley 2000, The Biomechanics of Insect Flight); flight bouts of 2-10 s at 1.4-2.6 m/s with a heading that turns by a new hashed rate every 0.25 s, staying within 15 m of home; then the nearest real flower head of the meadow within 6 m, approached and perched on for 3-15 s. Spawned per 12 m world cell (0-6, from the cell's flower density) within 40 m of the eye; wings drawn double-sided; only with the Sun above 3 deg, no rain; a wind over 7 m/s keeps them perched. | A behavioural model, not flight aerodynamics; no collision with vehicles or people; outside 40 m and at night they are not simulated. Test: `butterflies_fly_and_land_on_flowers`. |
| Fireflies: spectrum | A Gaussian peaking at 560 nm with 22 / 24 nm half-widths (~54 nm FWHM; firefly emission maxima span 552-582 nm, Photinus pyralis in vivo near 560-562 nm, Seliger & McElroy 1964, PNAS 52:75). It enters the three lanes as its display-equivalent triplet (CIE 1931 colour matching, Wyman, Sloan & Shirley 2013 fit, to linear sRGB primaries, negative blue clipped) scaled to the spectrum's photopic luminance: (0.56, 1.00, 0.00), chromaticity x 0.403, y 0.587. | Point samples at 680/550/440 nm would put the 560 nm band on the 550 lane alone (pure green). With the triplet the rods see about 1.8 times (estimated) the true scotopic share of such a band (named; the lane model cannot match photopic, scotopic and chromaticity at once). The curve's shape is an estimate. Test: `the_lantern_is_yellow_green_and_its_power_is_silvers_count`. |
| Fireflies: power | 1e11 photons per flash, the top of the 1e8-1e11 of Silver 2026 ("How bright is a firefly? Resolving a century of overestimation", Am. J. Phys., doi:10.1119/5.0325834, which corrects Coblentz's 1912 1/50-1/400 candle; the top is his ceiling of every luciferase turning over once with unit yield), a 0.12 s sin^2 pulse, isotropic: about 3e-5 cd at the peak. | The photon count is Silver's upper bound, not a per-species measurement; isotropy is assumed (the lantern is ventral). |
| Fireflies: transport | Each lit lantern is a one-pixel emitter in the HDR frame before bloom and the crush (`rm_points.wgsl` kind 3): pixel radiance = I / d^2 x focal^2 (its irradiance at the eye over the pixel's solid angle), times the froxel medium's transmittance, hidden by any nearer depth (a blade in front hides it). The 32 nearest lit lanterns are also point lights in the clustered light list with the shared range cut-off. | Lanterns take no point-shadow cube (six scene depth faces for tens of micro-candela; named cost bound) and so their light is unoccluded; at 3e-5 cd (5e-10 lane units) their light reaches the shared 2e-7 cut-off at 5 cm, the minimum range; that is the physical scale (3e-5 lx at 1 m, against ~1e-3 lx of starlight on the grass), so their light on the meadow does not show. The eye (see the rod pooling row): the display shows a lantern's pixel-averaged luminance (about 1.5 cd/m^2 at 3 m, 0.03 at 20 m at 1280x720); before the pooling the observer added a full rod signal to that pixel and a lantern read pale at night. `KESSEL_FIREFLY_PROBE=1` (debug only) holds one lantern lit 3 m before the eye, `KESSEL_FIREFLY_PROBE_GAIN` scales it, for this check; `KESSEL_BUTTERFLY_PROBE=1` (debug only) holds a 3x wing pair 1.5 m before the eye. |
| The eye: rod pooling of sub-pixel emitters | The observer (`observer.rs`, `crush.wgsl` `eye_response`) takes a sub-pixel emitter's rod signal as its pixel's scotopic luminance times the pixel's share of the rods' summation disc, half a degree across (complete spatial summation of dark-adapted rods, Ricco's area; Barlow 1958, J. Physiol. 141:337): 3 % at 1280x720 and 55 deg; the cones take the pixel whole (foveal summation is a few arcminutes, under the 4.6 arcmin pixel). The emitters' own radiance is written to a point layer (`rm_points.wgsl` `fs_layer`, the firefly lanterns only) that the display stage reads; transport buffers are unchanged. | A reproduction transform, not transport. The pooled rod signal is dropped at the pixel rather than spread over the disc (the faint rod halo round a lantern is not drawn). Only the lanterns use the layer: the stars, embers, sparks and tracers are unchanged, so the stars keep their 3x display-white ceiling and stay calm. Seen on spring-bridge-v2 (2 November 2026): the layer carries the lanterns (debug view `KESSEL_POINT_LAYER_VIEW=1`, debug only), yet at night a lantern still reads as a pale blue-white point, not yellow-green: even 5 % of its scotopic luminance, under the night eye's rod gain (doubled by the owner's cigarette boost), outweighs its cone signal at a pixel-averaged 0.03 cd/m^2 (20 m). Open: a rod-free fovea for fixated points, or a point-source cone response, is the eye model's to decide; the lanterns' power and spectrum are not raised. Tests: `observer::rod_pooling_tests`, `renderer_meadow::tests::the_crush_with_rod_pooling_validates`. |
| The eye: the rod-free fovea for points | Near the line of sight (the screen's centre, where the eye fixates) a sub-pixel emitter falls on the fovea, which has no rods: its pooled rod share is scaled by the rods' presence, 0 within 0.6 deg of the centre rising (smoothstep) to full by 5 deg (the rod-free foveola is about 1.25 deg across; rod density climbs steeply beyond it: Curcio, Sloan, Kalina & Hendrickson 1990, J. Comp. Neurol. 292:497), so a lantern looked at shows its cones' yellow-green and one in the periphery stays pale, as a dark-adapted eye sees them. Where a point falls in that rod-free region its pixel takes no rod signal at all (not even from the glare round it), and its foveal cones see it concentrated in the eye's point spread, about 2 arcminutes at a night pupil (Campbell & Gubisch 1966, J. Physiol. 186:558): the cone stimulus is the pixel's luminance times (pixel width / spread)^2, 8x at 1024x576, fading with the rods' return. Seen on spring-bridge-v2 at night in the locked look: a lantern fixated at 6 m is a yellow-green point (19,55,11); the same lantern 20 deg off the line of sight a faint grey (10,12,9); at 10 m fixated (probe) (42,78,14). A lantern beyond about 15 m is faint even fixated: 1e11 photons per flash is about a fourth-magnitude star at 20 m, whose colour an eye does not see either. | Point layer only (the lanterns): extended fields keep the whole-field rod model, so the eye's central night blind spot is not drawn; fixation is assumed at the screen's centre (no eye tracking); the transition's shape is a smoothstep, not the measured density curve. Stars are not in the layer and keep their 3x display-white ceiling. |
| Fireflies: flash pattern and swarms | Photinus carolinus, the measured clustered flasher: groups of more than 15 flash synchronously every 0.55 s in bursts repeating every 12 s, each member taking 3-8 consecutive slots with onsets within +-30 ms, dark between bursts; smaller groups and lone males burst 1-4 flashes at irregular times (a 50 % chance per 6 s bin) (Sarfati, Hayes & Peleg 2020 J. R. Soc. Interface 17:20200179; 2021 Sci. Adv. 7:eabg9259). Swarms (one per 32 m world cell with probability 0.8 x grass cover, 8-79 males, radius 3.5-7.5 m) wander slowly (< 0.4 m/s) about their home; members orbit within the swarm radius; altitude rises from 0.45 m to 1.75 m as the Sun goes from -3 to -9 deg, members emerging one by one through that dusk. | P. carolinus is North American: its measured clustered behaviour stands in for the lower Volga's Luciola and Lampyris (named). Orbits are smooth Lissajous paths, not flight dynamics. Tests: `a_large_swarm_flashes_in_synchronous_bursts`, `a_small_group_flashes_irregularly_and_each_flash_is_a_short_pulse`, `a_swarm_moves_together`, `no_fireflies_by_day_and_a_dark_meadow_has_some`. |

Debug gate: `KESSEL_MEADOW_SKIP=1` skips the meadow's draws (profiling only); `KESSEL_MEADOW=0` keeps the
legacy grass. Neither is set by any preset.
## Berlin 1945 map: authored landmark colliders and standing water, 2026-11-02 (`docs/BERLIN_REICHSTAG_1945.md`)

| Item | Live truth | Named approximation / limit |
| --- | --- | --- |
| Authored landmark colliders (`src/authored_colliders.rs`, `tools/berlin1945/bl_kit.py`) | A custom landmark GLB (the Reichstag, the Brandenburg Gate, the Kroll, the ministry, the city shells, the bridge) collides through a sidecar `<model>.colliders.json` written by the same Blender generator that builds the mesh: each box is a modelled solid part (a wall pier, a sill, a lintel, a column, a plinth, a pier of the bridge), so the shells' window openings, the gate's passages and the courtyards stay open to tanks, men, shells and sight. A model without a sidecar keeps the old path vocabulary (`worldgen::manifest_structure_proxies`). Unit tests: `authored_colliders::tests`. | (1) A curved or sloped part (a column drum, a pediment, the portico stair, a mansard) is boxed by its own model-frame bounds; each sidecar records the parts so boxed and the worst excess (the Reichstag's portico stair 3.2 m in two steps, a column 0.36 m, a mansard 1.04 m). (2) Collision boxes are world axis-aligned: a model turned off a quarter turn (the Dorotheenstadt's 6.3 deg grid, the Moltke Bridge's 137 deg) has each box split until no piece's world box reaches more than 0.4 m (`MAX_ROTATION_EXCESS_M`) beyond the turned piece; a large solid keeps its rim pieces and one inner box only when that box's world bounds are checked to stay inside the same bound. (3) The Reichstag's and the roofed shells' interiors are not modelled: their collision is the solid mass behind the walls (the Reichstag's windows were bricked up in 1939; the roofed shells' glazed and boarded windows are closed to fire and sight). (4) The landmarks are static: no part of them is brought down by the push or by shells (the grammar houses on In den Zelten keep the building runtime's damage). |
| Standing water in the Koenigsplatz cut and the Humboldthafen (`ditch_water_1945.glb`) | A flat slab at the water level, the same opaque smooth finish as the river's surface (the river water row). | The river water row's limits (no transmission, refraction, flow or wading depth); the slab is a scaled box, so its ends overlap the cut's banks under the ground. |
| The Moltke Bridge | The deck's top is a `floor_ceiling_` walk surface for people on foot; the barricades at both ends (Beevor 2002) are its collision against a tank; the blown arch is a gap with the upstream third still spanning. | A tank cannot drive onto the deck: the engine's tank-bearing bridges (`src/river_crossings.rs`) run along world X only, and this bridge runs at 137 deg. The barricades stand where a tank would meet that limit; they are static (not shot away). |

## Tiger visual size: an open debt, 2026-11-02 (the owner's decision pending)

The player's hero Tiger is drawn 1.112 times the real vehicle's size in every dimension (hull
7.13 m long against the real 6.32 m; 1.37 times the volume). It is the owner's RM-5010 kit CAD at
one uniform scale (0.038919827 m per kit mm against the kit's 1:35, 0.035 m): registering 446 CAD
solids to the game meshes needs no rotation and one uniform scale, with 0.17 mm RMS residual
(`docs/research/TIGER_CAD_COMPONENTS_20261102.md`). Nothing is rescaled until the owner decides.

- **Drawn at 1.112:** the hero mesh and its hatches, the interior rig and stations, the crew fit
  and crew hit volumes, the cabin collision proxies and eye points, the CAD component envelopes
  as registered (`catalog/tiger_cad_component_voxels.v1.json`), the engine bay's firewall plane
  used to place bursts (z = -1.15 m), and every distance between plates and components along a
  shot line. A crewman therefore has 11 % more room, and a fragment crosses 11 % more cabin, than
  in the real tank.
- **True size:** armour thicknesses and classes (the armour scheme), every ballistic resolution,
  the voxel lattice's plate thickness and every solve's dimensions (a test pins the lattice to the
  scheme's millimetres), calibres, rounds and their drawn bodies, fragment sizes and masses,
  wound sizes, casing thicknesses, the crew compartments' 10 m³ of free air.
- **Compensated in the physics:** along a shot line the layered chain meets each homogenised
  interior at its density divided by 1.112 and each liquid, vapour or air layer over its length
  divided by 1.112, so the mass a round meets is the real component's; the burst model loads and
  vents the hatches at their real areas (the drawn apertures divided by 1.112; a lid's real area
  is 0.81 of the drawn one).
- **Visible consequences:** a wound is drawn at true size on a plate drawn 11 % large, and a
  lodged round's exposed length is true against a plate that looks 11 % thicker.
- **Not compensated:** distances inside the cabin (fragment flight, crew reach, the penetrator's
  path to crew hit volumes) are the drawn ones.

## Expedition frame cost, 2026-11-01 (`expedition-bench`, `artifacts/perf/expedition_*_20261101.log`)

The first expedition playtest at 1024x576 under the deltaP look ran at "3 fps at times". The
new `expedition-bench FROM_S RENDER_S [preset] [site]` (`KESSEL_BENCH_CLOSE_M` stages the Tiger
that close to the site, gunner on fire at will) measured a standing frame of 35 ms, of which
16 ms were the commander's raster optic relays (the relay sky pass alone 10 ms: sky.wgsl's
per-pixel atmosphere and cloud march for every uncovered aperture pixel of every port, with the
port buckets grown by the 1024x576 grid), and, once the Tiger was penetrated and the cabin
burned, a CPU update of 46 ms (p50) and a fenced frame of 226 ms (p50): the interior fire's
49,152 flame and 24,576 soot grains regenerated every update, and every relayed port walking all
of them again on the CPU each frame. Changes, none of which alters an emitter, a material, a
medium or a simulation value:

- Relay sky from the sky cache (`shaders/relay_sky_cached.wgsl`): an optical capture on the
  raymarch backend samples the frame's equirectangular sky cache by direction instead of
  marching sky.wgsl again. Same transport, same cloud march, one evaluation per cache texel per
  refresh phase instead of one per port pixel. Named approximation: the solar and secondary discs
  are not in the cache (the main frame draws them analytically), so a port looking straight at
  the Sun shows the cloud-attenuated sky without the disc.
- Relay extents (`aperture_extent`): the "two source samples per native aperture pixel" rule is
  taken against a 640x360 frame at most, so the port buckets stop growing with the internal grid.
  A resolution choice (the ports keep the resolution they had at 640x360), not a transport change.
- Grain views (`prepare_fine_vfx_view`): a batch whose bounds lie outside a view's frustum is
  skipped before any per-grain work (the cabin's grains lie behind a port's near plane). No grain
  that a view could see is dropped; the receipt counts `culled_batches`.

- Fire presentation cadence (`drive_interior_fire`, `FIRE_PRESENTATION_PERIOD` 2): the fire's
  grains, their wall-clearance sweeps (`clear_source_paths`, 14 ms of the 26 ms) and their upload
  regenerate every second update, 30 Hz at the 60 Hz step; a skipped update keeps the last grains
  and the aggregate lights derived from the same cells. The fire's cells, emission and heat are
  unchanged; only the grains' flicker rate halves. A grain can therefore stand up to one update
  (17 ms) past a wall it has since drifted into.
- Playback range: the demo launcher now runs the reference tone curve (`KESSEL_AUDIO_RANGE=
  reference`, `src/reproduction.rs`): the balanced range's lift of up to 12 dB below 110 dB(A)
  made distant strikes and MG impacts read as near ones (owner, 2026-11-01). A reproduction
  choice; physical source levels and the propagation are untouched.

Still open after these: the interior fire presentation's own cost per update (probed as
`fire_smoke_clearance`, `fire_patch_surfaces`, `fire_world_sources`, `surface_blood_update`), the
raymarch main view's per-frame grain merge (`collect_parcels`), and the `native-pixel-hdr-effects`
pass (the effects session's item).

## The March orders, 2026-11-01 (`catalog/expedition_missions.v1.json`, `docs/MISSIONS.md` "The March")

Authored three-beat orders for the expedition theatre's twelve sites, on the Canyon mission model.
The beats observe the theatre's own records (`site_guards_down`: the site's finite guard tanks all
dead; `site_secured`: the theatre's own credit) and say so on the net; no beat releases a platoon,
alerts an enemy, lights a smoke column or changes a hold, a guard or a counterattack. The one cue
is the stranded crewman's signal star at the two rescue sites, a modeled emitter of the existing
flare kind fired at the site. The approach point is derived geometry (700 m short of the site on
the road network), not a trigger volume: the beat completes by the actor's distance to it, or to
the site.

Disclosed fixtures, not mechanics: `Game::debug_expedition_start_order(site, forward)` (the demo
launcher's `--order N --forward`) accepts an order and stands the crewed Tiger on the approach road
with the engine running, printing `STAGED forward start` to the log; the theatre, its guards and
its clocks are untouched. `debug_expedition_kill_site_garrison` marks a site's garrison dead for
the `expedition-orders-review` harness and is named in its receipt. Neither is reachable from the
shipping menu path.

## The deltaP display look, 2026-11-01 (`KESSEL_LOOK=deltap`, `docs/DELTAP_LOOK_20261101.md`)

A display-stage variant of the raymarch backend ported from deltaP's `crush.wgsl` and
`bloom.wgsl` (the owner's cockpit sim). It reads `KESSEL_LOOK` in the crush uniform's appended
`look` lane and in the bloom chain only; no emitter, material, medium, preset budget or
simulation value reads it. Named presentation choices while the look is on:

- Quantizer: 8 steps per channel on a square-root curve over display gamma with a full-range
  Bayer-8 threshold on the internal texel (`deltap`), or the PICO-8 32-colour palette by
  exhaustive OKLab best pair with the ratio solved in linear light (`deltap-pico8`). Both discard
  tonal precision after the eye and the tonemap; neither adds light. The standard crush's
  far-field dither attenuation does not apply under the look.
- Glare: deltaP's pyramid bloom (13-tap Jimenez downsample, soft-knee bright pass, Karis
  average, 3x3 tent upsamples summed into the HDR frame before the tonemap) replaces the compact
  halo *and* the CIE veil. deltaP's authored values (knee 0.55 +- 0.35 exposed, cap 1.0, 0.9 per
  octave, strength 0.8) put 3.75 exposed units of halo on every texel at the cap, which turned a
  fire's 1-3 exposed texels into a wide plateau and flooded the cabin from a vision port
  (2026-11-01 playtest; the effects session's measurement). Since 2026-11-01 the bright pass
  starts at 1.0 +- 0.5 exposed, caps at 0.5 per texel, falls at 0.65 per octave and enters at
  0.4 (about 0.5 exposed units of halo per capped texel). These remain authored values, not a
  measured point-spread function; the veil's omission is a departure from the eye model's
  intraocular scatter, taken for the look.
- Grid: 1024x576 by the demo launcher (5x on the 5120x2880 panel), a resolution preset only.

Gate: `tests/dither_motion.rs` and `tests/display_quantization.rs` validate the complete crush
shader and the Crush lanes (span 144, `look` at 128, earlier lanes unmoved);
`src/renderer.rs` unit tests check the uniform's offsets.

## Fire, sparks and the effects pass under the deltaP look, 2026-11-01 (branch `fx-fire-sparks`)

The owner's Canyon playtest under the look (2026-11-01) found the fire glowing over too wide a
radius, armour and blast sparks reading as one effect stacked on itself, and a T-34 kill dropping
the frame rate. Receipts: `captures/fx_before2_20261101_deltap_*` / `fx_after_20261101_deltap_*`,
`fx_strikes_20261102_deltap_*` and `fx_kill_20261102_deltap.json` (the `fx-review` harness, `src/fx_review.rs`: the burning wreck at dusk and night, three strikes on
its hull from the real projectile path, the gunner's frame across a catastrophic kill with
`KESSEL_GPU_PROFILE=1`).

- The fire's glow is the pyramid bloom, not the grains: embers and fire parcels are already one
  frame pixel / one froxel in size at 1024x576. Measured against the standard look on the same
  frame and handed to the main worktree, which owns the bloom constants (the deltaP section above).
- Relayed ports' fire and soot grains: a port's native extent is twice its displayed span and a
  flame parcel covers thousands of a port's pixels, so the spark budget (16 a pixel on High) let
  the gunner's sight pane draw 3.7 million grains for one burning wreck (44 ms worst frame,
  `native-pixel-hdr-effects`). Ports now take `relay_grain_samples_per_pixel` (Low 1, High 2,
  Ultra 4) and no optional quadrature density; over budget every parcel is thinned towards one
  sample per covered pixel, and in a port on to one sample per eight covered pixels (a sample's
  weight at most 8: the sight pane magnifies a fire whose thousand parcels overlap every pixel,
  and at weight 1 whole omission dropped 1,297 of 1,654 parcels), before any whole far parcel is
  omitted and counted. Each parcel's emission and optical depth hold in expectation; the port's
  fire is a sparser, brighter speckle (per-pixel noise rises as 1 / sqrt(samples)); the main
  view's own fire is the raymarch backend's parcels and embers and is untouched.
  Measured on the 120-frame kill at 1280x720 (`fx_kill_20261102_deltap.json`, RTX 4090, the GPU
  otherwise idle): `native-pixel-hdr-effects` p50 0.43 / p95 3.6 / max 5.5 ms (before: 3.3 /
  29.2 / 43.8); on the worst frame the sight pane draws 460,800 of 7.75 million requested grains
  for 1,398 parcels at a weight of at most 7.98, none omitted; the sparks pass p50 1.3 / p95 2.2
  / max 3.3 ms; `rm-points` 0.72 ms with the six embers a grain (10,044 embers that frame).
  The grain sampler's radical inverses are per-base functions (bit reversal for base 2), the same
  sequence a few floating-point ulps off; no sample identity changes.
- Embers (the main view's own fire, `renderer_raymarch.rs` `collect_parcels`, `rm_points.wgsl`):
  each flame grain within 90 m is drawn as `EMBERS_PER_GRAIN` (6) one-pixel incandescent points
  scattered through the grain's own cube by a fixed hash of its identity, each carrying a sixth
  of the grain's intensity (packed in 1/128 steps up to 2, the former 1/32 over 0..8 rounded the
  dimmest stage by a quarter). The same light as the one ember per grain before, as more, sharper
  points (the owner, 2026-11-01, on the dusk wreck: "tons of these sharp smaller ones instead of
  a bunch of clusters that look like isolated fires"). The merged volumetric parcels are
  unchanged; the ember is still the quadrature of a hot-gas cube as 1 cm grains, not a measured
  soot-particle distribution.
- Strike sparks (`RICOCHET`, `BULLET_ARMOR`, `BLAST`, `ARMOR_IMPACT`; `weapon_particle_math.rs`
  is the reference, `rm_sparks.wgsl` and `weapon_particles.wgsl` follow it): a ricochet's fan
  follows the outgoing ray, lifting off the plate and spreading with the ray's elevation; its
  speed law is slow-heavy (`q^1.5`) and scaled by the round's retained kinetic energy (the
  solver's `residual_v`); each spark burns for a time set by its size and cools through white and
  yellow as a fraction of that time, hotter strikes whiter longer; the sparks leave over a few
  milliseconds; they bounce once off the terrain under the strike (a fifth to two fifths of the
  vertical speed, just over half of the tangential) and lie on it. A blast's sparks scale with
  its charge. Each burst's fan width, speed share, lift and spread of leaving times are its own
  (a hash of the burst's seed), so two strikes of one kind no longer coincide. Authored: the
  speed laws, the restitution, the size-to-lifetime coupling and the cooling fractions are set by
  eye against the previous presentation, not fragmentation or thermal measurements; the bounce
  plane is the terrain height under the strike, not the hull or any object.
- Spark samples (2026-11-02): a record carries `SPARK_MULTIPLIER` = 8 samples a spark instead of
  the thousandfold quadrature, and both spark shaders draw every spark kind (blast, plate strike,
  ricochet, bullet, pressure vent, cook-off) as the 2026-10-29 dotted streak: the spark's path
  sampled 8 times over 14 ms, its whole light shared among the samples. The thousandfold split
  drew a blast's and a plate strike's 384,000 dim points as a sheet or a cone (the owner's
  "clusters" on the 2026-11-01 stills); the same 384 sparks now read as streaks, at an eighth of
  a percent of the vertex work. The ground's ejecta keep their quadrature (`GROUND_MULTIPLIER`
  1000: a mass distributed over grains, not sparks) and the hot rim its ring. The 14 ms streak is
  an exposure-length presentation of the spark's path, not a measured trail; the pass is now
  `analytic-streaked-sparks` (was `analytic-thousandfold-sparks`). Later the same day the streak
  itself was withdrawn: eight samples along the 14 ms path drew as bars of bloomed dots and 24 as
  solid lines (the owner: "thick laser beams"). A spark is now one sample where it is at the
  frame's instant, as every other body in this renderer (nothing is motion-blurred), covering
  its cross-section only; its tail is the support test's, no longer a smear of its light into
  the pixel. `SPARK_MULTIPLIER` and both shaders' `SPARK_TRAIL` are 1 and the `streaked`
  machinery stays at that length. The port shader caps a sample's projected area at 6 px as the
  main view's does, and a spark's colour is `spark_glow`: the three Planck-band
  ratios at the spark's temperature, cooling from its own start (2100-3400 K by kind, hash and
  the ricochet's retained energy; a blast's hottest) to 1100 K over its burn, on the palette's
  authored display scale (5.3 at 1500 K to 9.9 from 2800 K). The temperatures are authored
  incandescent-steel ranges, not measurements; the magnitude is not Planck's (which would rise a
  hundredfold over that range). The small-arms tracer strip and head light are the 3400 K
  white-yellow of a burning magnesium and strontium nitrate composition (bands 1, 0.705, 0.348;
  before 1, 0.462, 0.030) with the intensity rescaled (0.00165 to 0.001167) so the pellet's
  photopic estimate stays about 61 cd. `KESSEL_RM_LIGHTS_DEBUG=1` also prints which lights hold
  the two point-shadow cubes: on the strike fixture the burst at 6 m and either the second burst
  or the burning wreck 41 m off hold them, so one or two lights (the weaker burst, or the wreck's
  0.13 W/sr when two bursts outrank it) pass through the struck hull unshadowed. Kill at 1024x576
  after all of it: sparks 0.14 / 0.14 / 0.15 ms, rm-points 0.03, port grains unchanged.
- Exterior flame pages (`game.rs` `FIRE_FLAME_COLORS`, the second row; `exterior_flame_parcel_sample`
  stage regions): the base and the tongues' tips and outer sheath are the deep red of cooling
  soot ([168,52,9] and [148,28,6] sRGB, from the 2026-10-01 ambers [187,87,18] and [192,88,18])
  and the sheath takes the outer quarter of the spread and the top third of the rise (was the
  outer sixth and the top fifth); the yellow-white core and amber body are unchanged. Still an
  authored radiance calibration, not a soot temperature model (the owner, 2026-11-02: the fire
  "too yellow"; the embers inherit the pages, so the fire's outer points go deep red with it).
- The fire's light as points (`renderer_raymarch.rs` `FIRE_EMBER_SHARE` 0.8, `EMBERS_PER_GRAIN`
  12): a flame grain within 90 m draws 80 % of its authored emission as twelve one-pixel embers
  scattered through its cube and 20 % as its merged volumetric parcel (was half and half with
  six embers). The parcels, merged per 4 px volume cell, drew as cell-sized glowing blobs over
  the embers under the AgX and pastel looks (the owner: "diffuse glowing cubes"); the extinction
  and scattering of the parcels are unchanged, so the fire's haze and its light on the scene
  stay, and the total emission is the same authored calibration redistributed.
- Strike smoke in the raymarch backend (`renderer_raymarch.rs` `event_parcels`): a detonation's
  five soot puffs used to appear 1.5 m above the burst and spread about it, so a shell on a plate
  made a cloud in the air beside the hull (the owner: "exploding out of thin air"). They now leave
  the struck surface along its normal (a plate's, or up off the ground), born at the surface the
  size of the burst, slowing as they mix (2.6 m x (1 - e^(-t / 0.7 s))) and rising on their
  buoyancy from zero. A heavy round through the plate is its own event (`EVENT_PENETRATION`,
  from the perforation's `BULLET_ARMOR` dust record or an `ARMOR_IMPACT`, flagged heavy; a heavy
  ricochet stays `EVENT_ARMOUR`; before this no tank-round perforation made any event, so the
  molten spray only ever followed ricochets) and vents through the hole: six
  puffs born at the hole 0.45 s apart over 2.3 s, about the hole's size, leaving along the
  normal (1.6 m x (1 - e^(-age / 0.6 s))) and rising, fading over 6 s. Both are authored plumes
  on the event's origin and normal, not a vented-gas or detonation-products model; the hole's
  glow is the existing hot rim. Inside the hull the same event leaves its own smoke at the hole
  (2026-11-02, the owner: rapid like an explosion's aftermath, light and scattering, more for
  heavier hits, emanating from the impact rather than filling the compartment): eight puffs
  born over the first tenth of a second, blown 0.12-0.57 m inward along the normal and held near
  the hole, single-scattering albedo about 0.8, gone in five seconds; their reach, size and
  density scale with (plate work / 1 MJ)^(1/3) bounded to 0.5-1.6 (a 45 mm round about 0.7, a
  76 mm about 1, a 152 mm about 1.4); the exterior vent's puffs scale the same way and are lighter
  too (albedo about 0.6). An authored puff envelope for the round's smoke, the spall's dust and
  the burnt paint behind the plate, not a vented-gas model. The `fx-review` part
  `cabin_hit` (an observer inside behind the radio operator, the crew-solo fixture's wounding
  lane through the hull side) is its evidence. The spall bodies' trails (`HOT_FRAGMENT_TAIL_MAX_LENGTH`
  0.04 m, was 0.145 m) are capped at about one frame of the slow end of their motion: on the
  fixture every trail sat on the old cap a hand's breadth from the eye and drew as rods
  spanning the compartment for a second; a trail is still a bounded suffix of the body's own
  measured history, never a beam.
- The detonation cloud of a shell on a plate (`event_parcels`, `EVENT_DETONATION`; `blast()` now
  carries the struck surface's normal in the burst's direction lane and the event takes it):
  against a plate or wall (normal below 0.7 up) the baked ground-burst volumes are not drawn;
  the white-hot core grows out of the contact along the normal (0.3 m + 30 m/s x t for 45 ms)
  and five lobes born at the contact expand along the normal (2.2 m, 1 - e^(-t / 0.25 s)) and
  sideways within tenths of a second, then rise on their buoyancy for 14 s. Each lobe is fire
  while the products afterburn and the same soot after, so the fire and the smoke are one body
  born together with the fire dying first. Before, the core stood 0.6 m above the contact and
  three lobes were born 0.8 m above it at 2.7 m radius (the owner: the explosion "appearing
  outside of where it physically should be", a gap between the fire and the smoke). The GPU
  blast sparks keep the charge's speed law (10-38 m/s x charge) instead of 4.6x it: the exterior
  volume multiplier remains the raster path's flash presentation only. The detonation event is
  now by charge (above a quarter of an 88 mm HE round's), not by that multiplier. Authored
  expansion and rise rates for an 88 mm-class burst, not detonation-products gas dynamics; the
  lobes leave the plate at 3 m within a quarter second and lift at once, so the plate and its
  rim are clear of the cloud by about half a second (the first cut sat on the plate for a second
  and hid the ring). The burst's own light (`detonation_flash_heat`) is the 60 ms flash with,
  under it, an afterburn tail at a fifth of the flash fading over about a second and a half
  (0.45 s), so the cloud glows on in its own smoke rather than snuffing out at a tenth of a
  second; the burst's synthesized light follows the same envelope, and the fireball parcels'
  colour cools with it (white-hot at the flash, orange-red in the tail; a first cut at a third
  of the flash with the colour held hot drew a white ball for a second). Authored timing.
- Hot rim bound (`weapon_particle_math::rim_heat`, `RIM_KELVIN_MAX` 1800 K, was 2000 K): the
  rim's thermal reservoir still takes at most 1.5 % of the lost kinetic energy, but the rim that
  stays on the plate is held at steel's solidus, since hotter steel is molten and leaves with
  the perforation spray. The ring itself is drawn by the plate (`renderer_scars`, the scar's
  `rim_kelvin` from the same estimate, cooling over 2.5 s); the white-yellow blob a full 88 mm
  perforation drew was the rim's 512 incandescent grains piling their light onto a few pixels,
  so a rim grain's projected area is capped at one pixel in both spark shaders (the other kinds
  keep 6 px). A first cut held the bound at 1600 K and dimmed the ring the owner wants kept.
  The 1.5 % share and the rim's geometry remain estimates.
- (Retired 2026-11-02 by the owner's whole-load rule below: one round going inside sets off the
  rest and the turret goes near instantly. A catastrophic kill now launches on its next wreck
  update at the speed its own compartment solves, `game_cookoff::enemy_cookoff_throw`;
  `drive_cookoff_build` and its constants are deleted. The vent event remains for the breach's
  pressure vent.) The ammunition burnout before a turret goes (`game.rs` `drive_cookoff_build`,
  `COOKOFF_VENT_BUILD_S` 1.4 s): a catastrophic kill no longer launches the turret on the next
  step. The stowage burns first: for 1.4 s the compartment's pressure climbs and its gas and
  flame leave through every real opening, the terminal breach and each open hole in the hull
  and turret plates (a closed hatch is not invented as an opening), as pressure-vent bursts
  whose cadence quickens with the pressure (every 0.35 s at first, every 0.08 s at the end),
  and in the last fifth of a second from the turret race all round (eight points on a 0.71 m
  ring, outward and up) as it begins to lift; then the launch is armed and the existing ring
  plume follows the body. Since the same day's later change the build vents through ONE
  opening, the turret roof where the hatch is (the commander's eye sits under it; the roof
  height is the turret source's highest point), a jet re-issued every 0.3 s whose strength
  rises with the build (the breach, the open holes and the ring no longer vent; the owner
  asked for the one flame jet up through the roof). In the raymarch backend a heavy pressure
  vent is an event (`EVENT_VENT`): a six-parcel flame jet along the opening's normal, one to
  four metres long with the strength, fire while the gas burns and cooling along the jet and
  with age on the shared Planck law (`incandescence`), its smoke drifting off for four seconds,
  a torch light of about 0.3 W/sr at the jet, and 48 sparks (192 before) from the spark pass.
  The build time, the cadence, the jet's geometry and its lengths are authored estimates of a
  stowage's deflagration before the ring vents, not a gas-dynamics model; `fx-review` part
  `burnout` (a primed catastrophic kill from an observer 14 m off, five seconds) is the
  evidence.
- The `fx-review` harness films (`KESSEL_FX_REVIEW_FILM=1`: every frame of each strike and of
  the kill to a 60 fps H.264 clip, stills taken as frames of the clip with no history reset) and
  takes a clock (`KESSEL_FX_REVIEW_CLOCK=noon|dusk|blue|night|<UTC>`, blue hour 13:40 UTC on
  21 December 1942, the sun about 5 deg under; since 2026-11-02 every part defaults to `noon`,
  09:30 UTC, local noon at Stalingrad with the sun about 18 deg up, the owner having had enough
  of sunset renders). In film mode the fenced wall time includes the
  frame's readback and PNG write and is not a frame-rate figure; the pass timestamps are.
- Every lamp is occluded by every object in the air as on the surfaces (2026-11-02). The
  froxel pass (`rm_volume.wgsl`) gave a lamp without a shadow cube (all but the two strongest
  within 45 m: every fire, flash, torch and tracer head) no visibility test at all, only the
  medium's extinction, so a burning tank's fire lit the snow haze under its own belly and the
  air behind its hull through the plates. With ray queries the pass now binds the live scene
  (both TLASes) and asks one any-hit shadow ray per froxel cell and lamp toward the source,
  stopped short of the fixture, exactly the surface pass's rule. That exemption was four
  times the lamp's source radius, meant for a lamp in its housing; a fire's source radius is
  the whole flame, so the ray stopped four to eight metres short of a fire and nothing within
  that of a burning tank was shadowed at all: it is now within 0.15-0.3 m in both passes. A
  wreck's one fire light also stands in the flames' crown (the upper third of its cells'
  heights) instead of at the mean of cells spread over the deck and sides, which lay inside
  the hull and lit the snow under the belly. Without ray queries (the
  portable preset, `KESSEL_RM_RT=0`) the air stays unshadowed for those lamps: the named gap of
  that preset, not of the hero one. Cost is a ray per lit cell per lamp in the cell's bin;
  measured in the `fx-review` receipts (`rm-volume`).
- A burning wreck's flame and soot grains are regenerated every update (2026-11-02): the
  2026-11-01 half-rate (`FIRE_PRESENTATION_PERIOD` 2) was taken for the cabin fire's 73,728
  grains and applied to the wreck fire too, which then flickered at 30 Hz against the sparks'
  and embers' 60. The period now applies only while the cabin burns; a wreck fire alone is a
  few thousand grains.
- The exterior fire pages are the Planck law at the soot's temperatures (base 1500 K, body
  1750 K, core 2100 K, tips 1350 K; `incandescence::planck_srgb8`), the same band ratios
  `spark_glow` and `rim_color` compute in the spark shaders, so the fire, the embers, the vent
  jet, the detonation's parcels, the sparks and the hot rims glow on one law (the owner,
  2026-11-02: one visual language, and the fire into the deep red). Not calibrated soot
  emissivity; the display scale per page is unchanged. Twenty embers per grain (twelve before)
  carrying 0.6 of the grain's light (0.8), so the sharp points read as a continuous burning
  surface over the deck with the tongues' volume behind them.
- A wreck's smoke leaves from the flame's crown (2026-11-02): each soot lobe's root is the
  mean of this frame's upper third of flame cells, not a patch on the hull, so the smoke rises
  from where the tongues end (it rose beside them). A stove's or a non-particle frame's smoke
  keeps the patch root.
- Glowing metal on every heavy plate strike (2026-11-02): the hot rim (`rim_heat`, the plate's
  own steel at the strike heated by the round's lost work, 1.5 % of it, capped at the solidus)
  was drawn for perforations only; a shattered round's crater and a ricochet's gouge now draw
  the same annulus (named approximation: a gouge is drawn as the annulus a hole's rim is), and
  the annulus is 1.8 x wider (0.18 mm per shell millimetre, 3-30 mm). The owner: more glowing
  metal on the impacts.
- The tracer pellet's intensity is 180 cd (61 cd before): the 61 was a lower bound from the
  trace's required visibility, and the AgX display desaturates the hotter pellet toward white
  as a 3400 K source of that intensity should look; its chroma is unchanged.
- The airborne front of every surface burst reaches everything at the speed of sound
  (2026-11-02; `pressure_front`, `Game::emit_blast_front`, `deliver_pressure_fronts`). Before,
  only an HE burst and an inner-surface penetration emitted a front, received by the eye alone
  (a kick of an authored 0.18 m over distance squared). Now the first blast of a kill, the
  stowage's detonation at the turret's launch, a wreck's cook-off, the 88's and the enemy
  guns' muzzle blasts and the HE burst each leave one front, carrying the source's TNT
  equivalent (the sound levels' own: 0.71 kg, 3.67 kg, 0.71 kg, 0.36 kg per kg of
  propellant, the HE fill's equivalent), followed to where its incident overpressure falls
  under 0.2 kPa (at most 240 m). Every receiver's response is computed from that charge at its
  own distance and delivered at its own arrival, d / c, the same range and speed the report's
  scheduler uses, so the boom and the front arrive together: the eye's kick (a quarter metre
  at 20 kPa scaling with the root of the Kingery-Bulmash overpressure below it, decaying as the
  existing armour-hit kick; a presentation of the startle and the chest thump, the head's
  physical displacement being negligible, and never through the player's intact hull or for
  the own gun), the standing men (a flinch of 0.3 s at 140 dB peak to 1.2 s at 170 dB, nothing
  under 130; a presentation of a startle, since the drag impulse on a man at these distances
  is millimetres per second), the grass (each wind cell's blade spring takes the tip velocity
  the dynamic-pressure impulse gives a 5 mm, 0.7 g/m leaf at Cd 1.2: flattened within 15 m of
  3.67 kg, a whip at 40 m, a tremor at 100 m; the spring's displacement is now bounded at 0.85
  of the blade) and the standing trees (the crown's drag impulse on a cone a third as wide as
  tall at Cd 0.6 against 0.3 of the stem's mass, a damped spring at 5/height Hz with damping
  ratio 0.15, the pose a rotation about the base; centimetres at 15 m from 3.67 kg, as a 10 m
  pine should). The dynamic pressure is the weak-shock ½ρu² with u = Δp/(ρc), which overstates
  a front above ~50 kPa by up to a third: those distances are inside the burst's damage radius,
  where `blast` snaps the trees and resolves the men anatomically. Named approximations: a
  grass cell (8 m) receives its impulse at its centre's arrival; the muzzle blast is treated as
  a surface burst of its charge's equivalent (the directivity of the real muzzle blast is in the
  sound, not the front); the refractive cap is unchanged (a representative subframe radius,
  not a gas solver). `fx-review` part `shockwave` (a primed catastrophic kill 33 m from an
  observer in the open with three riflemen between; the receipt records the fronts, the
  arrivals, the kick, the men startled, the blade and crown impulses) is the evidence, with
  `pressure_front` tests of the charge's overpressure, impulse, reach and arrival windows.
- Lying snow with relief (2026-11-02; `rm_snow_lift.wgsl`). The snow on a vehicle was a share
  of the face's albedo mixed toward white, a flat colour. A covered face is now raised along
  its normal by the snow's thickness, 6 cm at full cover through the same lying field (slope,
  breakup, load), and the shading normal follows that field's slope across the face, so the
  deck's snow carries a lip at every plate edge and pillows between; the depth pre-pass lifts
  the face identically (a test holds its copy of the field equal to rm_common's). Named
  approximations: the scene's acceleration structure and the sky-cover map keep the bare
  geometry, so shadows, ray-query occlusion and the cover map ignore the lift (at most 6 cm);
  the thickness is a display estimate of a night's fall on a deck, not a deposition model.
  What rides aboard the player's tank (a part, a batch or a pane whose box is under 2 m across and
  whose box centre lies inside a vehicle-surface frame's oriented box with 5 cm grace;
  `is_aboard`, renderer_raymarch.rs) takes no lying snow from the world's load: the lift raised
  the cabin lamps' top faces off their bodies, green faces floating by the gunner (owner report,
  2 November 2026). The tank's own snow is its catch, of which none is kept. Named approximation:
  a box-centre test, so a small object under the hull between the tracks is counted aboard.
  A moving pool's bodies (hot fragments and their tails, a body's cells, lodged rounds, pieces in
  flight) take no lying snow at all (2026-11-03): the lift moves each vertex along its own face's
  normal, so a hard-edged box's faces parted by up to 6 cm round a 1-2 cm cube, drawn as offset
  triangles and loose planes (owner report: "the faces are like offset in this like accordion
  shape"). Open debt: any other hard-edged mesh out in the snow (a crate, a box prop) still has
  its faces parted by the lift where the field differs between them; a lift along a welded
  (shared-position) normal would close it.
- The winter running gear (2026-11-02; `snow_cover::step_pack`, `Renderer::set_running_gear_snow`,
  `snow_gear_crust`). Nothing packed the tracks before: the Tiger's belt carried only
  centimetre flecks (`track_snow`), the enemies' nothing, and lying snow stays off every
  vertical face by its slope rule, so no vehicle's gear read as winter's. Each hull now carries
  a pack load, gained with the distance driven through the lying snow (full after 80 m of a
  full field, in proportion to the world's load) and lost over 15 minutes standing, and the
  G-buffer draws a packed crust inside the hull frame's running-gear envelope (inner |x|, top
  y: the T-34's measured from its bake, the Tiger's from its track nodes and interleaved
  wheels, the heavy vehicles' from their track widths) whatever the face's slope, denser low
  where the pads run and the snow is thrown, with a 1.5 cm lift, a matte slightly grey snow.
  Named approximations: the envelope is a box in the hull frame (the lower hull side between
  the wheels takes crust with them); the pack rates are presentation estimates; a hulk stands
  at 0.6 of the world's load. `fx-review` part `gear` is the evidence.
- What the layered damage model's breaches do outside the hull (2026-11-02;
  `game_fea_effects`). The chain names the casings a round opened and where; before, that knocked
  the system out and lit the game's fire, and nothing showed. Now a holed fuel tank with petrol
  above the hole runs it out through the round's own hole in the plate at Torricelli's rate
  through the solved aperture (discharge coefficient 0.62, the head the tank's live level above
  the hole from the component voxels, falling as the tank drains; the two modelled tanks share the
  system's fill); the litres are taken from the live fuel; it falls as drawn beads (a fixed count
  per litre, presentation) and pools where it lands as a flat disc of 4 mm depth, and once the
  breach's fire burns (the game's existing rule) the pool burns as a world fire source. A holed
  radiator steams through the deck grilles over it for 120 s; a breached engine smokes its oil
  through the grilles for 240 s and, burning, stands its flames out of them (a world fire source
  at the grille); a holed gearbox, steering unit or final drive drips its oil (30/20/12 l,
  estimates, at 0.08 of petrol's rate through the same hole, named) under the hull and pools.
  Named approximations: the vents' durations stand for the coolant and oil volumes; the grille
  is the first source surface met from above the component's centre; a pool spreads on whatever
  it lands on without flowing; a pool's and the engine's fires are drawn with the world fire's
  tongues (two to three metres tall, their count with the strength), not a low, wide pool flame,
  and they stand at the game's fire strength from the first frame rather than growing over the
  seconds a petrol fire takes; nothing here is saved (a reload loses a running leak, not the fuel
  it took). `fx-review` part `breach` is the evidence; tests cover Torricelli's rate, a hole
  above the liquid, the drain and the pool's growth.
- Hot metal cools on its own thermal time (2026-11-02; Alex, playing: "glowing hot metal cools
  way too rapidly, it should glow for a good 10-20 sec slowly decaying"). The thin-ring rim
  estimate (`rim_heat`, every strike the lattice does not solve: enemies, ricochets, shatters)
  heated a 1-6 mm skin with 1.5 % of the lost plate work and cooled it on an authored 2.5 s; the
  heated zone is now a quarter calibre of worked steel (4-30 mm), 12 % of the lost work (the
  share calibrated on the lattice's solved rims, 1,350-1,400 K for an 88 on 45 mm, capped at the
  solidus), and it cools by conduction into the plate on depth²/α with steel's diffusivity
  1.2e-5 m²/s: 1 s for a bullet's mark, 10 s for a 45 mm hole, 36 s for an 88's. The scar
  renderer's empirical rims take the same time from the wound's calibre; the lattice's own
  cooling curves are unchanged. Every authored chip (the burst body's steel, the fragment
  shower) now leaves the plate at 1,250 K and cools as a lumped body by the solved chips' law
  (radiation and convection over its mass and surface): a 5 mm chip glows for some 13 s, a
  1.4 mm splinter a second, instead of fading on an authored life; such chips stay at least 20 s
  so the cooling is seen, then retire as before (only the solved steel persists). Named: the
  1,250 K leaving temperature and the 12 % share are estimates bounded by the lattice.
- The chips' aggregate light follows the Planck visible radiance of their temperature band
  (2026-11-02): 2.5 % at 1,100 K, 1.3e-4 at 900 K, 4e-7 at 750 K of the 1,300 K band's, so the
  pages' display scale (lifted and drawn hotter for legibility) no longer lights the cabin's dust
  orange from a floor of resting dull-red chips.
- The behind-armour flash (2026-11-02; `behind_armour_flash_peak`, the owner: a flash of light
  as the round bursts through, seen from inside). A perforation of the player's vehicle lights a
  modelled emitter 12 cm inside the hole: 1 % of the steel the round displaces (the calibre's
  disc over the oblique path) leaves as fine dust and burns in the compartment's air to magnetite
  (6.7 MJ/kg) over a 20 ms time constant, 30 % of the power radiated at a luminous efficacy of
  18 lm/W at about 2,800 K (its colour on the shared Planck law). A 76 mm round through 80 mm
  peaks near 4 Mcd, about five times the 88's muzzle flash light; a 14.5 mm bullet's is a
  fiftieth of a unit. Each frame shows the flash's mean over its own exposure, so it is equally
  bright at every frame rate. Named estimates: the 1 % fine share, the burn time, the radiant
  share and the efficacy; it lights through the scene's shadow rays like any lamp.
- Inside a perforation (2026-11-02, the owner): sparks thrown inward from the back face (the
  exterior strike's 0.6 % share of the plate's work, as incandescent dust along the shot line);
  an armour-piercing round with no filler that comes through above 60 m/s draws its base tracer
  across the compartment as the tracer strip, exposed over exactly the time the round takes to
  reach the first surface on its line (a few milliseconds, so one frame holds its whole path, as
  a camera records it; named: a tracer is assumed on every such round); the authored fragment
  sizes reach 1.2-1.8 cm instead of 7-9 mm (behind-armour debris has a centimetre tail; the drawn
  body is the physical one, so the larger pieces are heavier and hit harder), and a cooled chip's
  colour is a reddish-brown oxide (sRGB 104, 66, 52) instead of plain grey.
- Every wound glows with a radial temperature falloff (2026-11-02): where the lattice solved the
  strike, its own field; otherwise the rim estimate's temperature at the bore falling outward
  exponentially over a third of the hole's radius (0.3 R from a dent's centre; named), drawn,
  like the chips, at 1.7 times its temperature from 750 K up, so the bore reads white-yellow and
  the colour runs through orange, red and deep red to raw steel instead of one flat colour.
- A penetration's trapped interior smoke is a third as dense as its exterior vent (0.55 against
  1.6) and clears in six seconds instead of ten (2026-11-02): at the vent's density the six
  puffs were opaque across the turret, and an interior camera saw only sky-lit fog for ten
  seconds. A compartment does fill with smoke; the density is a named presentation value, not a
  vented-gas model.
- A penetration's interior smoke stays at the hole (above) and a fire's smoke is its own soot
  (2026-11-02, the owner: fires need time to build up smoke, much darker, absorbing the light).
  The soot is a uniform load in the cabin air (`Game::interior_soot`, 0-1), built at 0.38 per
  second times the burning surfaces' enclosure heat (`CabinFire::enclosure_diagnostics`) and
  drained linearly by the fan and an open hatch (`update_haze`); at a full load its extinction is
  1.5 /m (`SOOT_EXTINCTION_PER_M`: a lamp two metres off all but hidden), single-scattering albedo
  0.2 (fresh black carbon's order) and asymmetry 0.5, added to the cabin medium in
  `rm_medium.wgsl` inside the cabin domain only. The build rate, the extinction scale and the
  uniform spread are authored (named: no soot transport, no stratified hot layer, no deposition).
  The gun's fumes are their own two-zone model since 2026-11-03 (`cabin_fumes`, below).
- Spall and shrapnel carry tracers, and the solved chips glow by their temperature on the one
  law (2026-11-02; the owner). A flying fragment's tail is still a frame of its own measured
  motion (no rod drawn through a wall; a ricochet starts a new segment), now bounded at 0.45 m
  instead of 4 cm and 0.22 of the chip's size wide, so a 1/60 s exposure of a chip at tens of
  metres a second reads as a streak; the five heat pages' colours are the Planck law drawn at
  1.7 times their bands' lower temperatures (2200, 1870, 1530, 1275 K drawn for chips of 1300,
  1100, 900 and 750 K and up; cold steel below 750 K: white-yellow through red, the owner's call
  over the dull red the solved temperatures give, a named display choice) instead of an
  authored amber set, and
  the two coolest pages' display scale is lifted (1.05 to 1.4, 0.18 to 0.6; tails 0.86 to 1.1,
  0.12 to 0.5) so a chip at the Draper point reads red hot in the dark cabin. The cabin fire's
  four pages are the same Planck law at the exterior pages' temperatures (their display scale
  unchanged), so the Tiger's fire and a wreck's burn alike. Named: the pages' display scales
  are presentation, the chips' temperatures are the lattice's (plastic and friction work, cooling
  by radiation and convection).

Gate: `src/renderer_fine_vfx.rs` tests (thinning floor, no weight above the view's cap of 1 or 8,
whole omission below it), `src/graphics.rs` (grain rate per preset), `src/weapon_particle_math.rs` tests (bounce never
below ground, rest on it; two strikes differ by event and by seed; support bounds hold),
`src/weapon_vfx.rs` tests, and the shader validation tests of both spark shaders.

## Physical terrain wavelength and cold restore, 2026-10-31

Build 48's cold-cache GPU test passed all exact before/after height, opening-mask, resident-page,
upload and saved-state checks, then failed its separate CPU/GPU agreement gate by 2.667353 mm.
The cause was an inherited physical mismatch: CPU relief used a fixed 1.2207031 m base wavelength,
while WGSL used the native heightfield spacing (1 m in this fixture). Independent f32 evaluation
reproduced 2.667345 mm at `(0.625, 1.625)`, within rounding of the GPU result. That point is away
from the boot depression; the difference was not caused by R16 rut quantization or save/resume.

The source fix shares `terrain_relief::RELIEF_BASE_WAVELENGTH_M` with generated WGSL constants.
Both terrain geometry and normal-octave footprint filtering now use the same physical wavelength.
The documented 0.31–9.77 m geometric relief and existing CPU surface stay unchanged; native grid
spacing continues to govern heightfield sampling and lattice resolution. The cold-restore test
retains bit-exact GPU before/after checks and its original, separate 0.2 mm plus 0.1% rut-depth
fixture tolerance. A CPU regression covers several native spacings and the known failing point.
**The source correction still awaits the build 49 CPU/GPU rerun.**

Older terrain showcase receipts report a maximum CPU/GPU difference of about 0.250 mm over their
sampled lattice (`terrain_detail_after.json`, `terrain_ruts_nrm.json`, `terrain_lattice076.json`).
Those are measurements, not a universal accepted bound; their residual cause has not been
established here. This change does not silently widen tolerances or claim universal float parity.


## Physical workshop source ownership, 2026-10-30 — validation pending

`workshop_runtime` now preallocates 39 source banks: seven existing cupola vision assemblies,
the existing gunner sight, seven individually partitioned turret spare track shoes, and finite
replacement pools for these parts and an existing source torch. The seven shoes are removed
from the live turret by exact triangle-multiset subtraction; a changed source fails startup.
Removal, carrying, fitting and cargo use the same durable item ID and the same source meshes.
All 39 source frames follow the visible transforms in cabin contact, fire/foam and acoustic
queries; empty banks are disabled. The underlying turret retains its real armor triangles.
The shoe meshes are equipment contacts, not an additional nominal turret-armor layer. There is
currently **no layered shoe-plus-armor ballistic resistance calculation** and no claimed armor bonus.

The physical procedure groups fasteners into release / withdraw / insert / secure / test.
Retainer times and four grouped hammer contacts are authored interaction rules, not simulated
thread pitch, pin friction or torque. The hammer's measured +X head face reaches a confirmed
retainer triangle; its 80 mm square face and model remain visible. Component insertion follows
a rigid pose interpolation. This is constrained hand manipulation, not a full swept multi-body
assembly solver: component-to-cavity collisions throughout the extraction/insertion path still
need broader validation. Voluntary placement preserves orientation and requires actual source
support under a nine-point footprint, with a conservative envelope access check. An abandoned
held part settles to a source support when one is found; this is a placement abstraction, not
animated rigid-body falling. Failed support admission keeps the old owner's pose instead of
transferring the item to a successor's camera. Resting parts do not yet slide under acceleration.

Cupola/sight removal disables its optical authority. Replacement changes the actual source
optic registration and requires securing/testing. Generated optical relays retain the existing
reconstructed prescription; cross-mount optical quality still requires rendered evidence.
Direct, confirmed workshop source hits damage that item identity with a binary rule. Existing
interior fragment damage still uses the older module/compartment proxies and does not trace
loose-part fragment hits. Proxy backtraces cannot create hull scars on the removable banks.
Track repair in Expedition must fit an existing physical shoe before the ordinary four pin
blows can restore mobility; the same item then becomes the installed running-track shoe.

The optional work light uses the existing modeled torch on a source-supported turret bracket.
Its source lens supplies a finite, geometry-shadowed light. The 18 kJ battery, 3 W drain,
handling mass, bracket and intensity are authored estimates, not measured period hardware.
This is a functional visibility upgrade; engine, gearbox, radiator, gun and ammunition upgrades
are not implemented by this slice. Their source mapping and unresolved causal-system work are
recorded in `WORKSHOP_SOURCE_REGISTRATION_20261030.md`.

Whole-world owner state includes finite parts, jobs, socket progress, tool clocks/input latches
and all cached part poses. Restore presentation performs no repair, battery refill or work.
Gates added: exact source partition/atomic failure; source-supported lamp bracket; frame
enable/move/self-exclusion contact behavior; binary snapshot identity preservation. Root CPU
tests and the real-input workshop review are pending; source parsing alone is not gameplay proof.

## Living crew personal-space contact, 2026-10-30

- 2026-11-01: a walker whose locomotion capsule starts more than a touch inside another man's
  source triangles (deeper than 0.55 of the capsule radius: a seat left straight into the
  gunner's side) has that step released instead of clamped, so he can walk out; a touching capsule
  keeps the sweep's back-out rule. Counted in the receipt as `depenetrations`. This is a solver
  escape for an authored exit position inside a body, not a contact law; the proper fix is an exit
  position clear of the seated gunner (`place_interior_exit`).

Interior walking now queries the indexed standing/seated crew source meshes. Animated seated
positions come from the same last pose uploaded by `crew_arm_animation`; target transforms include
body displacement and the visible wounded slump. Seated wound masks use the renderer's source-weight
mask function. Cached per-pose triangle BVHs filter candidates before capsule/triangle contact;
there are no additional authored crew blocking boxes. The local player's identity is excluded.
The existing floor/breech walk clamp still applies separately.

Named approximation: the player locomotion shape is a vertical capsule, **not exact person-to-person
skin contact**. Its height is measured from the reviewed standing source; its radius is the maximum
horizontal distance of the source-classified torso/pelvis vertices. This encloses that sampled
cross-section but omits the player's arms and leg articulation. Conservative empty space within
that envelope is bounded by its reported radius; no claim of millimetre-accurate clothing contact
is made. A 1 mm numerical contact skin is added. The camera's existing 0.50 m crouch compression is
applied to capsule height. Capsule/triangle distance tests are geometric; continuous contact uses
at most 24 conservative-advancement iterations and four horizontal slide passes, failing closed
at the iteration limit. An initially touching player may back away instead of being trapped.

Scope limits: living cloth/body meshes only; separate helmets and retained ragdolls are not added
as new movement obstacles here. No shoulder-turn, yielding animation or forced displacement of
another person was introduced. Those need one shared render/damage/contact pose before they can
be admitted. Source-load failure logs the missing file and disables those contacts instead of
manufacturing invisible geometry.

Evidence hooks: `Game::crew_personal_space_receipt()` reports candidate triangles, bodies queried,
query count, measured capsule dimensions, skin and iteration bounds. CPU tests cover thin-surface
sweeps, diagonal sliding, backing out of overlap, frame partitioning, broadphase preservation and
empty-space movement. A live interior playtest is still required to assess passage width and
station access; passing geometry tests alone does not establish that the cramped interior is fun.

## Persistent people and retained bodies, 2026-10-30

The five living appearance slots no longer serve as unique people. Singleplayer binds stable
expedition PersonIds to monotonically assigned anatomical actor IDs, with a finite 16-actor
admission limit. Wounds, retired physiology, casualty history, corpse clearance and corpse render
pages keep the old actor ID when a slot receives a replacement. A fresh replacement cannot inherit
a predecessor's missing limb or be killed by a delayed contact addressed to that predecessor.
Existing field-person takeover changes control without replacing or healing the actor. LAN/couch
retain their existing peer identities and casualty authority.

Solo corpse rendering has sixteen independent actor allocations. Each uses the reviewed relaxed
source figure's eleven anatomical pages, its own atlas appearance, the existing reviewed bone
pages and source steel helmet. Prototypes share source geometry/material; bounded one-instance
pages preserve independent transforms and source IDs. Missing source geometry reports failure;
this path does not invent a replacement primitive. Legacy fresh-match resets reuse reserved
pages, rebinding the appearance source when necessary, rather than growing an unbounded pool.

Clearing a seat addresses the historical corpse actor at its physical death post, even after a
new commander uses that appearance slot. A successor can be admitted on foot while that corpse
still blocks the seat. Inside clearing remains an assisted translation of the articulated body
toward the existing compartment resting area. Outside, exact source flesh triangles select the
body and a line through a real, fully open source aperture admits extraction. The retained body
moves visibly at 0.35 m/s along a guided extraction path, then detaches into world coordinates.
The same actor and anatomical records remain. Moving the Tiger afterward does not move a body
left on the ground. Foot deaths start at the current feet/yaw/crouch transform, not a former seat.

Named limitations: extraction is an assisted whole-ragdoll translation, **not a musculoskeletal
lifting, fabric collision, or hatch-clearance solver for every limb**. The guided path may clip
limbs against the hatch; its initial source-ray/open-aperture check proves access, not clearance
of the entire posed body. On-foot death starts neutral anatomical limbs at the first-person body
transform, not a copy of the last hand IK pose. Outside settling uses the existing anatomical
segment bounds, sampled terrain height and bottom support sweeps against Tiger source triangles;
it does not provide full body-to-body or every-building collision. These are gameplay procedures
and retained physical state, not claims of an exact casualty-evacuation simulation.

Evidence hooks: `crew_corpse_render_receipt` identifies actor-owned source pages;
`crew_body_recovery_receipt` lists held and world-space actors and their retained PersonIds.
CPU tests cover actor binding/history isolation, finite admission, source-aperture crossing and
preservation of every segment transform through world detachment. Live extraction and repeated
succession visual checks are required before calling the full loop verified.

Living incapacitated people use that same extraction path without a KIA event: retained
anatomical actor and first-aid physiology stay alive and continue their ordinary field time course.
Their standing cabin mesh and station duty are excluded; inside hit queries use their actual
retained pose and outside small-arms queries use the world pose. A medical bed takes exclusive
ownership of the existing physiology only after physical admission. Its transform moves the
whole pose rigidly and zeros motion; it does not rebuild joints or restore tissue. Explicit
takeover of a capable treated person removes only that person's living detached pose, preserves
their actor ID and wounds, and leaves every historical dead body untouched. Bed placement has
a CPU regression for preserved segment distances, absent tissue, identity and zero velocities.

Evacuating a living incumbent now releases their runtime slot for relief without killing or
discarding them. Their physiological record transfers to actor-owned unbound storage; the
ordinary field clock advances it once unless the camp owns an admitted bed patient. Returning
survivors transfer that same record into a vacant controlled slot, reuse the old actor ID, and
retain lost blood and wounds. Original source appearance is recorded at first admission. Live
body/driver-arm slots copy its immutable source atlas and PBR channels, and corpse pages select
its original source prototype, independent of the slot used on return. The common local
first-person hand rig remains the existing source model. Sixteen unique actors is the finite
registry limit, rather than five lifetime people. Tests cover archive/restore without healing,
independent patient and replacement physiology, unique binding and preserved appearance.

This first evacuation slice does not include patient locomotion, full lifting animations,
detached-world cannon/blast/heat exposure, or free-world blood transport. Detached people are
excluded from the cabin's fire and blood emitters after extraction; field bleeding still advances.
Small-arms hits interrupt bedside care before adding their real wounds, preventing a stale
medical snapshot from undoing the injury. A living settled patient does not receive a fabricated
corpse gore pool. Those limitations remain explicit until the broader world-person solver exists.

## Camp actor source skinning, 2026-10-30

`camp_people_rig.rs` reads the original Panzertruppe GLB's 39 joints, inverse bind matrices,
both sets of original joint weights, and node transforms. It verifies the flattened loader order
against all 914 source positions and retains the 564 indexed triangles, UVs and texture/PBR
data. Source-world inverse-bind products must be identity within the stated numerical tolerance;
a different export convention fails admission rather than receiving an invented scale correction.
Neutral skinning preserves the source rest geometry. Ordinary idle lowers the original T-pose
arms using two-link IK about authored shoulder/elbow pivots.

Walking is an authored procedural cycle: 0.8 m travel per phase, 60% stance with a commanded
world-fixed foot at matched root travel, 40% swing with up to 9 cm lift. Pelvis drop makes room
for existing leg lengths; the IK never stretches them. Original ankle orientation keeps the
boots level. Salute/work are authored hand targets, not captured performances or guaranteed
contact with a tool. The caller must use actual travelled distance for phase and publish the
same posed vertices to both rendering and contact queries. Slope adaptation, planted-foot turn
compensation, learned locomotion, motion capture and a full human balance/contact solver are
not implemented by this module. CPU tests check stance algebra and the private source's
bind/weight/vertex/material/floor contracts; final movement appearance requires visual review.

## Vehicle dynamics: per-vehicle spec, sprung hulls, AI drivetrain, 2026-10-28 (branch `vehicle-dynamics`, docs/VEHICLE_DYNAMICS.md)

Every tank now has a sourced row (`src/vehicle_spec.rs`: Tiger I, T-34/76, SU-100, KV-2, IS-2,
ISU-152, IS-3), a sprung hull (`src/suspension.rs`) and, for AI vehicles, the player's force-based
drivetrain (`src/powertrain.rs`, `powertrain::ai_driver`). Sourced: masses, engine powers and rated
speeds where stated, gear counts, documented road speeds, wheel counts, the Tiger's 800 mm wheels
and 55/58 mm x 1.89 m torsion bars (tiger1.info), the Soviet wheel diameters, track widths,
clearances, ground pressures and obstacle limits (de/ru/en Wikipedia data tables, read 2026-10-28;
the row comments name each). Named estimates [E], each derived in the code comment beside it:

- Per-gear road speeds of the Soviet boxes (geometric spreads from a first gear to the documented
  top); idle speeds; the IS family's 2000 rpm rating; the yaw rates (0.45-0.6 rad/s).
- Track gauges other than the ISU-152's 2.42 m; contact lengths derived from mean ground pressure
  (`M / (2 b p)`); hull dimensions for inertia; CG heights at 0.45 of overall height (Tiger 1.30 m
  as before); unsprung shares 0.13-0.15.
- Spring rates. Tiger: from its bars, `G pi d^4 / (32 L) / a^2` with steel G 79.3 GPa and an
  estimated 0.41 m arm (224 kN/m, 275 at the ends; 1.37 Hz heave). Others: static deflection for a
  chosen heave frequency (Christie 1.25 Hz, torsion-bar heavies 1.4 Hz).
- Wheel travel (Tiger 0.15 m jounce / 0.10 m droop; Christie 0.14 / 0.09; KV/IS 0.12 / 0.08); bump
  stops at 8x the station rate with 0.3 damping; belly contact at 5 Hz, 0.5 damping.
- Damping: 0.06-0.08 of critical from friction on every station; the Tiger's end-arm hydraulic
  shock absorbers (not checked this session) at 0.5. No shock absorbers on the T-34/SU-100 (sourced)
  or the KV/IS family (IS-3 sourced, others assumed).
- The AI driver's pedal gains (full throttle 1.5 m/s under the wanted speed, brake to full at
  4 m/s over); the AI's speed command is a share of its own road speed (previously of one 12 m/s
  for every kind, reached at 6 m/s^2).
- The crewman's head on his neck: 1.8 Hz, 0.5 damping, 0.08 m travel.

Named simplifications: massless wheels and track (no unsprung bounce); stations move along the
hull's vertical; the track bridges whatever lies beyond a wheel radius of every wheel (no wrap or
tension); uniform-box inertias; frame-differenced accelerations bounded at 3 g for load transfer; a
station's support rises no faster than `2 v + 0.1` m/s (the old embedded rule, now per station); the
refusals (step, trench, gradient, tipping, belly hang-up) stay the rigid-rest rules of
`tank_support`, which the "Tank ground contact" row's simplification (1) and (3) described for the
pose and which now decide only what the hull may cross. AI stations sit on the footprint's
declared contact span (`vehicle_ground`, 0.65 of the half-length, 0.75 of the half-width) because
the drawn meshes are larger than the real vehicles; the player Tiger's stations are its 48 drawn
road wheels' measured centres. The owner-directed `CLIMB_GRADE_SHARE` stays player-only; AI
vehicles climb the full grade. Road wheels are not yet drawn moving with their stations
(`Ride::travel` is computed for them; see docs/VEHICLE_DYNAMICS.md).

Gate: `suspension::tests` (static pose, heave frequency within 8 % of the spec, pitch settling,
load transfer, a bump ridden with energy only decaying, the track nose on a step, droop and free
fall, belly crest, partition and determinism, stations against the baked wheels, head sway),
`vehicle_spec::tests` (row consistency, bar-derived Tiger rates, `ride_table`), the powertrain
tests unchanged, `tank::tests::ai_tanks_drive_with_their_own_mass_power_and_brakes` and
`ai_hull_rides_its_springs_over_a_bump_instead_of_snapping`, `tank_support::tests` unchanged (the
refusals), and the headless `vehdyn-trace` receipt on Canyon v2.

## LAN co-op on Windows, 2026-10-28 (branch `lan-windows`, `docs/LAN_WINDOWS.md`)

- **The living crew in empty LAN seats** (`crew_solo.rs`, LAN section). On the host the solo
  crew's AI gunner, driver and loader work every seat no human holds, through the same lay,
  trigger, lever and breech paths as solo and as a human (no new authority path). Debt, named:
  a man displaced by a human at his post is **not drawn** on any machine while displaced (solo
  stands him by under the cupola; LAN has no standby pose yet). His body is a render rig only in
  either mode; nothing collides with or is hit through the missing figure, and the host's crew
  damage still uses the existing LAN crew model. Wounds, first aid and bleeding stay solo-only in
  LAN (the crew alive mask is what replicates).
- **Laptop tier** (`tools/launch_windows.ps1 -Laptop`/`-Tier auto`): 480x272 grid, `laptop`
  preset, `gameplay` transport sampling, no MSAA, shader backend unless a ray-tracing GPU is
  present. Resolution and sampling only; emitters, occluders, materials, damage and dynamics are
  the shipping ones. Measured on an Intel UHD 770: about 3.0 s a frame at 480x272 and at
  320x180 (GPU-bound on shadow passes and draws, not fill): integrated graphics are below the
  floor, stated in the doc, no further shortcut taken.

## Solo crew, wounds and first aid, 2026-10-27 (branch `crew-solo`)

Singleplayer musters five living men (`src/singleplayer_crew.rs`, `src/crew_solo.rs`,
`src/first_aid.rs`, `docs/CREW_SOLO.md`). Wounds come only from the live penetration solver's
anatomical events and the live cabin fire; nothing here invents a hit. Named approximations,
none presented as physiology or a contact solve:

- The wound time course is a rule-of-thumb model: bleeding in litres per minute by body region
  and severity^1.5, clotting of minor wounds as an exponential over five minutes, performance
  as piecewise functions of blood loss, fracture, burn, pain, morphine and concussion, and
  death at 42 % loss. Severity is mapped from the solver's fragment work (`total_work_j / 350`,
  clamped 0.2..0.85) and tissue outcome (detachment and pulverization are 1.0). Unconsciousness
  from blast work above 150 J is `6 + blast_work_j / 60` seconds. Burn exposure is `fire *
  0.02` severity per second in the hull, 0.6 of that in the turret, 0.15 head out, with no
  position inside the compartment and no heat transport.
- Morphine ampoules stand in for the Wehrmacht's ampoules and syringe; the box contents (6
  dressings, 2 tourniquets, 3 morphine, 2 splints, 2 burn dressings, 3 triangular bandages, 2
  canteens, scissors) are a design count, not a reviewed Kfz-Verbandkasten inventory.
- Treatment is a stepwise hands-on procedure per wound kind (`first_aid::Procedure`): holds,
  presses, wraps as alternating mouse strokes in a 0.25-1.8 s cadence, windlass turns as
  presses half a second apart, a hands-off watch. The step tables, their seconds and counts,
  the quality penalties (0.08 a wrap off cadence, 0.03 a mashed turn, 0.05 a hold let go,
  0.15 an interruption, 0.2 a skipped cooling) and the quality-to-effect maps (a tourniquet
  leaks `0.5 x (1 - quality)` of the bleed, a dressing's factor rises by `0.6 x (1 - quality)`
  of the way back to untreated, a splint's performance factor is `0.45 + 0.25 x quality`,
  improvised work is capped at 0.6) are design values standing in for skill, not a model of
  tissue or of a bandage. No first-person hands are drawn doing them; the crosshair label and
  the man's state are the feedback. The airway rule (an unconscious man on his back dies of it
  at 240 s unless positioned) is a single timer.
- A bleeding man emits surface-blood grains every half second, `clamp(rate_l_per_min x 14,
  4, 48)` grains from a point chosen by the worst wound's region (lap, side, head, chest), and
  96 more as he dies of it: a design rate for the pool's growth, through the real surface
  transport, not a volume balance against his blood loss.
- Stowing spent cases upright in a rear-wall rack is a crew-habit choice for this Tiger, not a
  historical claim (practice varied by crew and unit; cases also went out of the hatch or onto
  the floor). The slot stands are generated from the turret's source triangles (a 100-degree
  ray fan from a point in the rear of the basket, the case stood a hand's width in from the
  wall on the surface a downward ray finds, if the upward ray finds headroom), so no slot is
  invented where the wall and floor are not; the margins (0.03 m from the wall, 0.01 m above
  the stand, 0.04 m headroom) are design values. The loader's carry to the rack is a pose
  interpolation over the stow time with no hands drawn; the stow time, the rushed and hurt
  thresholds are design values.
- Dumping the empties out of the side escape hatch with the commander's leave, and letting them
  collect on the floor otherwise, is a crew-habit choice, not a historical claim. The open hatch
  is the real lid state (sound enclosure, light and weather through the real opening, small-arms
  rays against the moved lid); the main-gun armour solve does not model an open hatch as a hole
  in the plate. The loader does not move to the hatch: his body stays at his post while the case
  is placed just outside the aperture and given the heave's velocity (CASING_THROW_SPEED outward,
  1.2 m/s up), so a rifleman can hit the aperture but not a man standing in it. The dump only
  happens between reloads with no contact in the gunner's sight; the 1.8 s per case is a design
  value. Floor clutter is counted, not collided: loose cases within 0.7 m of the loader's post
  stretch his stroke period (a quarter per case, at most double), cases within 0.32 m of the
  player's feet slow his step (0.18 per case, never below half) and are kicked with 1.4 times
  the foot's velocity; there is no foot-to-case contact solve and no trip or fall.
- The AI loader's working round is exempt from the off-station ram decay while he holds it
  (`loader_holding`); his stroke cadence (1.5 s per stroke at full performance) is set so the
  reload stays about six seconds.
- The commander's designation range is the hit distance of a ray from his eye to the terrain
  heightfield or a 2.4 m sphere round a contact's centre: a game shortcut, not a rangefinder or
  a stadiametric estimate, and it is the distance to whatever the eye ray meets first (a rise of
  ground in front of the intended point gives the rise's range). The gunner's response uses the
  ordinary lay and traverse drives at their real rates; his acknowledgement is immediate. The
  second pointer on the gunner's turret position indicator is the same source needle drawn at
  0.72 scale 4 mm in front of the first. That the Tiger I's indicator carried a commander's
  pointer is the brief's direction and is not verified here against a primary source; the
  turret's own 12-hour pointer is the existing instrument.
- Triage ordering is the model's own time-to-death estimate (bleed rate against the fatal
  loss, the airway timer), spoken once per change of the set of men needing care.
- The first-aid box has no mesh; it is a hotspot at a turret-local point on the rear right wall.
- The crew figures carry no animation clips. A wounded man's slump is a rotation about his seat
  by his performance; his anatomical hit volumes do not follow it (they stay at the seated
  pose). Dragging a body off a seat translates the settled articulated ragdoll as one toward a
  rest point over four seconds, with no contact against the interior; while the hands are on
  a man or a body the crosshair target is held (a neighbour cannot steal it mid-hold).
- Hands-on reach is 1.75 m from the eye to a man in the turret and 2.8 m down into the hull
  front (the driver and radio operator sit low and forward of the basket floor's edge, and a
  body there has slumped into the footwell); no leaning or kneeling pose is drawn for it.
- The headless proof's fixture: clear weather, a parked range target that does not fire, and
  a Tiger down to its last rounds, fired off before each strike so no live cartridge sits in
  the compartment. With a full stowage every wounding lane tried through a plate into a seated
  man (turret side, turret rear, hull side) found live rounds first and the Tiger went up, and
  a killing solid shot through the gunner's seat set off eight rounds left in the rear stowage.
  That is the model's own answer about the Tiger's stowage, not a defect of the crew; the
  proof states the fixture in its receipt.
- The driver's reaction (0.3 s / performance) and command ramp (2.5 / s × performance), the
  gunner's settle (0.55 s / performance), lay tolerance (0.010 rad / performance) and hand
  shake (a sinusoid scaled by 1 − performance on the desired lay), and the loader's stroke
  cadence (0.75 s / performance through the existing auto-crew stroke machine) are design
  values; the turret, gun and powertrain still move at their real rates through the existing
  drives.
- The gunner's target choice is nearest-first with an optical line from the eyepiece; no
  recognition, no priority by threat.
- Station entry refusal and the clamber cost reuse `switch_cost`; the clamber itself is the
  existing camera interpolation.

Gates: `crew-solo PREFIX` (eight stills and a JSON receipt, PASS/FAIL per check, nonzero exit
on failure) and the unit tests `first_aid`, `crew_solo`, `singleplayer_crew`.


## Layered damage model ("FEA damage"), 2026-11-02 (branch `claude/fea-damage-20261102`, `docs/FEA_DAMAGE_MODEL_20261102.md`)

What the code computes for a kinetic round striking the player's Tiger, and what it approximates.
The struck plate's outcome (perforated or not, residual speed) is still the calibrated
`ImpactEvent` from `ballistics.rs`; no law below changes it, and the calibration tests pass unchanged.

- **Geometry source.** The Tiger's internal components come from the owner's RM-5010 kit CAD,
  registered to the game hull on 446 solid↔mesh pairs (RMS 0.17 mm;
  `docs/research/TIGER_CAD_COMPONENTS_20261102.md`) and voxelised at 20 mm
  (`catalog/tiger_cad_component_voxels.v1.json`, `tools/tiger_cad_voxelize.py`): a voxel boundary
  lies within 17.3 mm of the true surface; voxel volumes are 0.94–1.06 of the exact solids.
  Surface normals for casing obliquity are occupancy-gradient staircase normals.
- **Inherited scale** (the "Tiger visual size" entry above): the game's Tiger is drawn 1.112
  times real. Not rescaled here. Interiors divide their density by 1.112 and fluid and air layers
  their length, so the mass met along a straight path is the real component's; plate and casing
  thicknesses are the armour scheme's and the authored true millimetres; the lattice's plate is
  the scheme's (a test pins it).
- **Casings and contents are authored estimates** (`fea_damage::components`): the kit CAD is solid
  plastic. Engine HL 210 P45 with an aluminium-alloy crankcase (12 mm, 1,050 kg estimate), gearbox
  and steering unit cast housings (12 mm, 700/600 kg), final drives (20 mm, 350 kg), sheet-steel
  fuel tanks holding the live fuel fraction of 534 l (the kit models two of the four tanks),
  radiators, fans, radio (identification inferred), MGs, breech and cradle, sight, traverse drive.
  Ammunition bins are their covers; the rounds stay the game's live-cartridge boxes.
- **Layered chain** (`fea_damage::chain`, wired in `game_fea.rs`): behind the struck plate each
  layer has a named law: further armour by the project's own DeMarre/Lambert–Jonas law (0.92);
  casings and solid parts as RHA-equivalent thickness (areal density × a named material resistance
  factor) under the same slope law, with the Recht–Ipson plug momentum share; homogenised
  interiors the same against their areal density without a plug; liquids and gas by
  hydrodynamic drag (Cd 0.5, named). A round stopped in the struck plate is taken to reach the
  depth where its calibrated capability is spent, spent uniformly along its path: a reduced rule,
  not validated against depth-of-penetration data. The projectile is rigid and keeps its mass (no
  erosion, no APCR carrier stripping, no yaw). A filled round that perforated bursts at the plate's
  back face as before (no fuze-delay model). The far wall is the first source surface met coming
  back along the line from outside, classified by the existing plate classes. Interior bulkheads
  that are not catalog components are not layers.
- **Consequences.** A casing the round perforated knocks its system's module out (fault `Spall`);
  a casing struck but not perforated leaves it working (recorded in the receipt). Radiators, fans,
  air filters and seats are recorded only (no live cooling circuit or seat state). The residual
  round's own straight path reaches crew through the existing anatomical boxes and live cartridges
  through their boxes, with its real mass and speed; injury outcomes and the existing 0.24 J and
  2.8 J rules are unchanged. Fire from a breached fuel tank or engine follows the existing
  fragment rule, and since 2026-11-02 it catches where the fuel or the engine was opened
  (`game_fea::fea_seed_fire_at_breach`): the first fuel or engine contact of the penetration, the
  round's own or a fragment's, swept on along its own path (5 cm before it to 75 cm past it, named)
  to the first source surface, which becomes the first burning patch. Until then every fuel or
  engine fire was lit at one standing spot in the turret basket (`INTERIOR_FIRE_SPOTS[0]`), so an
  engine-bay hit burned in the fighting compartment; that spot now takes a fire only when no
  surface lies on the breach path (logged). Spread, foam and the ammunition's exposure are the
  existing surface-fire rules: a fire behind the firewall reaches the fighting compartment's
  cartridges only where the source surfaces leave a clear line (no conduction or leakage through
  the firewall, named).
- **Fire on the crew** (`CabinFire::burning_surface_sees`): a man's burn exposure, the player's and
  each living crewman's, now needs a burning patch within the cabin's 2.5 m exposure range with a
  clear source line to his body (the ammunition's visibility rule). The rates are unchanged
  authored rules (the player's from the fire's strength, the crew's per post). Before 2026-11-02
  the fire's strength reached every man aboard wherever it burned.
- **Lodged rounds** (`fea_damage::lodged`): a round stopped in the struck plate is held only when
  its cylindrical body went past its nose taper (an ogive or cap is self-releasing when its
  half-angle exceeds the friction angle); a round stopped inside stays where it stopped. Pull-out
  force is Coulomb friction (0.3, named) on a residual grip pressure: until the lattice supplies its
  final contact forces, 5 % of the struck material's yield (named). Removal by one man's pull
  (400 N), the toolbox bar (×10), sledge blows (160 J at 50 %), or the workshop. A stopped round's
  lattice solve replaces the 5 % rule with its own contact grip when it commits.
- **Lodged rounds in play** (`game_fea::drive_lodged_rounds`, `equipment::RepairJob::LodgedRound`):
  each lodged round is drawn in an exact physical instance pool (casts, in the ray tracing scene),
  one prototype per kinetic specification, following its hull or turret. The drawn body is the
  chain's penetrator: the solve's diameter and ellipsoidal nose, with a cylinder that holds the
  round's mass as solid steel (named: no cavity, filler, cap, windshield or driving band; bare
  steel, the paint not modelled). An APCR round is its core alone, drawn 3 core diameters long
  (named, unsourced: its mass is mostly the carrier, which is not modelled; length enters neither
  the chain nor the solve). At most 6 rounds of one specification are drawn (named bound; further
  ones stay in the state and stay removable; logged). With the toolbox in hand a crewman works a
  field-removable round out where the tools reach it with a clear path (outside the plate for a
  round the struck plate holds): a hand pull takes 4 s and the bar 15 s (named tuning, like the
  other field repairs); sledge removal takes the counted blows at one per swing of the carried
  hammer. A round needing the workshop cannot be removed in the field. A removed round leaves the
  simulation (not kept as a loose object). A filled round that lodged unexploded is treated as
  inert: its fuze state is not modelled. Lodged rounds and deferred plate solves are saved
  (`vehicles.tanks` owner version 2; a pending solve is re-run from its saved problem on load and
  gives the identical answer). A round held by a part that elevates with the gun (breech,
  cradle, sight) moves with the gun. A remount into a spare Tiger leaves the lost hull's lodged
  rounds and fragments behind. LAN clients do not see lodged rounds (host state only), and the
  campaign checkpoint between acts does not carry them (named).
- **Voxel lattice plate solver** (`fea_damage::lattice`): an ordinary state-based peridynamic solid
  (linear peridynamic solid, 18-neighbour cubic lattice; not a finite-element method) with elastic
  bulk response, von Mises deviatoric plasticity by Mitchell's return mapping, linear hardening,
  logarithmic rate dependence lagged one step, linear thermal softening, bond failure at a ±8 %
  scattered critical plastic strain or by cumulative tensile spall damage of the Tuler–Butcher form
  (threshold 2 GPa, 1 µs scale, both named estimates: tension must last to open a spall), and
  compressive contact between broken original neighbours only. The projectile is rigid with an
  ellipsoidal nose; its motion is prescribed from the calibrated speed loss, starting clear of the
  plate. Yield and deviatoric stiffness are exact for an interior family in coordinate-plane shear
  and plane deviatoric strain; free surfaces keep their yield strain by weighted-volume scaling.
  Outer cells are clamped behind a damping ramp (the rest of the plate). Friction (0.1) softens to
  zero at melting. Measured: energy closes to 0.2–0.7 % on the reference cases (work in = kinetic
  + elastic + contact + plastic + friction + damping + fracture release); mass is exact; a
  budget-sliced solve and any thread count reproduce the reference bit for bit; the lattice
  absorbs 0.25–0.8 of the kinetic energy the calibrated law removes (reported, not forced). The
  fracture pattern (petal count, bulge) is deterministic per impact state but not converged in the
  time step. Cost on this i9 at the 6,000-particle cap, time step 0.4 of a cell's P-wave transit,
  12 threads (`fea-bench 5`, 65 impacts of the cross-section set, machine shared with other work at
  load ~18): lattice p50 215 ms, p95 447 ms, max 498 ms, energy closure within 1.8 %; chain p50
  46 µs, p95 0.8 ms. (At 12,000 particles and 8 threads, load ~12: p50 525 ms, p95 1,075 ms.) Rounds that stop very deep
  in the plate (85–92 % of the path) also release a small back-face scab in the lattice; the game
  spawns fragments only for perforations, so these appear only in the cross sections and receipts.
- **Film section view** (`Renderer::set_film_section`, film and review harnesses only): the
  meshes of one vehicle are hidden from the camera beyond a plane, in the raymarch G-buffer (and
  its glass panes and near-wound patches) only. The sun cascades, point-light faces, sky cover,
  ray queries and every simulation query keep the whole closed vehicle, so the opened view shows
  the closed tank's own light: a lamp-lit interior is dark beside daylit snow, as it is. It is a
  view, labelled on every frame, never a change to the scene; play never sets it (the plane is a
  private renderer field, set only by `set_film_section`, None in both shipping presets; the
  G-buffer's section bit is zero in shadow passes and without a plane: `section_bit`). One
  exception since 2026-11-02, the penetration death's replay (`Renderer::set_replay_cutaway`, only
  from `death_replay::render_replay_impact`, guarded by a source test): after the player is dead,
  the impact shot cuts the struck part (the turret with its gun, or the hull, by the fatal
  record's fragment frame) within a window round the shot's line (a cylinder 0.15 m growing to
  0.9 m) from the outer face in through the plate's thickness plus 3 cm, so the fragments inside
  are seen; the same camera-only rule (shadows, lights, queries and every simulation see the
  whole vehicle), unlabelled in play (it is the replay's view, not a claim about the vehicle).
  The plate's thickness is measured along the shot through the tank's own source triangles (0.1 m
  if they are not met; named).
- **FEA range** (`fea-range`, the owner's review editor, `src/game_fea_range.rs`, 2026-11-03): a
  window on the snow field (Canyon v2 winter workshop, act 1) with the player's Tiger and its solo
  crew, the shipping raymarch renderer and the game's own update. Its shots are real rounds through
  `Game::debug_launch_round` (9 m short of the first source surface on the chosen line, so the
  round meets every surface on it; impact velocity is the shell's own `v0·e^(−kx)` at the chosen
  range, a chosen velocity sets that range). Disclosed review actions, as in `fea-film`: the Tiger
  is placed and reset by state, enemies are banished and hold fire, the racks may be emptied, the
  cabin lamps switched, the sky clock is held at the chosen date and time, the eye's exposure may
  be held (a camera setting), and the section is the film section above (camera only, labelled on
  screen). Recorded frames are the renderer's own frames read back.
  **Rewind** (2026-11-03, `src/game_fea_range/rewind.rs`, review-only; no play path builds one): a
  backward seek puts back a checkpoint of the game's state kept at the end of each run frame
  instead of re-running the shot. A checkpoint is the world save's own lossless owner state
  (`game_world_state`, every field by name, encoded as the save encodes it) plus what the save does
  not keep: the voxel tissue and its pieces, the FEA chain receipts, the game clock (the renderer's
  `time`, which `Game::update` advances and reads) and the renderer's pressure fronts and wind. The
  frame is then stepped again from the checkpoint before it, so its picture is the frame's own
  update; a frame whose update was slow (the strike's solves, about 3 s) is put back, drawn from a
  1 µs update and put back again (named in the status line). Also kept, as copies, what a run
  changes that the world save does not hold (`rewind::Unsaved`, 2026-11-03, after a thrown turret
  outlived every reset): the thrown turret and the cook-off, the scars' solved rim cooling (the
  save skips it), queued and travelling blast fronts, the strike's effects and flashes, interior
  soot, the Tiger's ground contact and the armour's surface fires. Every other `Game` field is held
  by the save or named in `rewind::NOT_KEPT` with its reason (source data, caches, audio, UI,
  campaign/expedition owners); test `every_game_field_is_kept_or_named` fails on a field that is
  neither. Not kept: the renderer's draw state, sky and camera, and the grass about the camera. A
  frame that ends with a casualty's tissue solving on its worker is not kept (a seek steps it). Cause of the earlier non-repeat (measured with
  per-field checksums over three runs): `reset_match` leaves the game clock, the projectile serial,
  the armour clock, the renderer's pressure fronts, the infantry and a few presentation counters
  running on, and every range step began from whichever camera the previous frame left, while
  the game reads the renderer camera as its eye and ears. The range now keeps its settled fresh
  Tiger and puts it back for every run (and for the tape re-run), and poses the editor's camera
  before every step. With that, three runs agree in all 361 checked fields at every frame, and the
  tape re-run and a second re-run agree with the first run (`fea-range --shots`, 2026-11-03),
  except three listener-side fields named and left out of the comparison
  (`rewind::LISTENER_FIELDS`): the sound's room tracing (its import empties a per-frame job and
  cache), and the player's hearing and first-person arms, which follow the sound. The shipping
  `reset_match` is unchanged: a restarted match in play still starts from the running clock.
  Review-only display switches (EFFECTS, off by default in the range, the game unchanged): the
  crewman's concussion defocus and the eye's flash afterimage. With the
  section on, a body solved on its cells also draws its inside cells within 1.5 cells of the plane
  (`crew_corpse_tissue::set_review_section`, set by the range alone), so the cut shows cells, not
  the inside of the drawn shell; the solve and the cells present are unchanged.
  The section also cuts the moving pools' bodies (cells, fragments, rounds; camera only), each
  showing its own material where cut rather than the steel cut face (scars.w bit 25).
  **Tissue knobs** (`crew_tissue::TissueTuning`): a review-only override of the crew lattice's
  authored constants (tear energies, critical stretches, momentum shares, active reach, the piece
  landing-breakup factor), set only by `fea-range` while it differs from the authored values; with
  no override (both shipping presets, every play path) `tissue_tuning()` is exactly the constants
  (`crew_tissue` test `the_tissue_tuning_is_authored_unless_the_review_range_sets_it`, which also
  pins the only setter to `game_fea_range.rs`). Bonds keep the law `k = 2E/(ε_c·r)²`; a tuned class
  scales its bonds' stiffness and critical stretch at solve time instead of rebuilding the model.
  **Lamp power** (2026-11-03, the owner: a cut-open interior too dark to read): a review-only
  factor on the cabin lamps' radiant power (`game_fea_range::review_lamp_gain`, the dome, TPZ
  illuminator and ignition lamps' light and their glowing bulbs alike, through the one
  `practical_gain` the lamp switch sets), shown on the sidebar as "LAMP POWER xN * (REVIEW)". It is a
  brighter real emitter, not fill light: the lamps' light is still occluded and shadowed by the
  closed tank. 1 in every other path (test `the_lamp_power_is_one_unless_the_range_sets_it`). The
  range's LOOK dials are the display reproduction (look mode, exposure compensation, saturation,
  warmth, gamma, dither, palette, retro strength, white balance), not light; its fog dial sets the
  physical medium's density. Its focal length is a 35 mm-format field of view. Impact smoke (2026-11-03,
  the owner: it hid the strike): `weapon_vfx::impact_smoke_build`, an authored envelope for the
  `Impact`, `ArmorImpact` and `PenetrationSmoke` bursts in every path: none for 0.12 s, a smooth
  build over 0.5 s, 0.4 of the earlier density (earlier: full within 0.08 s). Authored, not
  combustion or dust modelling. The range's EFFECTS dials scale it (review-only; test pins the
  setter). Cannon rounds in flight (2026-11-03, the owner: the tracer was wider than the
  shell and hid it): drawn as their penetrator body (`drive_lodged_rounds`, the lodged rounds' own
  pools: APCR as its core alone, named), nose at the round's point; the tracer's glow a third of the
  calibre across (authored), its streak from the body's base. No depth of field:
  the renderer has no lens model, and a screen-space blur would not be one.
- **Cross sections** (`fea-cross-section PREFIX [CASE]`, CPU only, `game_fea_section.rs`): the
  game's own source-triangle contact, chain and lattice on the CPU-loaded Tiger meshes, drawn as
  SVG/PNG frames about 1/1000 of real time apart, with a contact sheet and a 12 fps clip per case.
  Section-view simplifications (named, not used by the game): fragments fly with drag until the
  first source triangle they enter (no bounce or embedding drawn; surfaces they leave are passed,
  since the struck plate's own source triangles are not perforated); all 64 hull cradles hold a
  round; fuel 80 %; turret forward, gun level. The 14.5 mm B-32 case round is a case-set projectile
  (no anti-tank rifle fires in the game yet) with estimated data.
- **Lattice in play** (`game_fea::fea_solve_impact`): every kinetic strike on the player's Tiger
  that perforates or stops in the plate is solved. A perforation is solved inside its tick before
  its scar, consequences and fragments are resolved (the tick waits and logs the wall time). A round
  the plate stopped changes only its scar and grip: its solve runs on a worker thread and commits a
  fixed 0.25 s of game time later (`FEA_STOP_COMMIT_S`, named), the update waiting for it if it is
  late (logged), so the switch happens at the same simulation moment on every machine; until then
  the scar shows the empirical geometry, and both the renderer and the next round's resistance see
  the change together. No consumer sees a half-solved plate, nothing is skipped, and both presets
  get the same result. Measured in the owner's demo (2026-11-02, Canyon run, this branch): three
  stopped 45 mm rounds solved on the worker in 139, 120 and 3,595 ms; the slow one, with the
  machine busy with other work, held the update 2.9 s at its commit. A deferred solve now runs on
  half the machine's threads (at most 6) beside the game, and the solver's barrier backs off from
  spinning to yielding to 50 µs sleeps, so a descheduled worker is not kept off the cores (both
  named tuning; results are bit-identical whatever the thread count). Not yet re-measured under
  contention. A perforation still holds its tick for its whole solve (about 0.2–0.45 s). Workers
  are now capped at one per 384 particles (was 1,024, which held a 6,000-particle solve to five):
  a 4,851-particle reference solve took 834 ms on 1 worker, 233 ms on 4 and 194 ms on 12
  (`lattice::diagnostics::thread_scaling`, debug build, machine lightly loaded); about a sixth of
  each step is serial (bond breaking and barriers).
  Ricochets, shatters, HE and HEAT keep their existing scars. Other vehicles keep the empirical
  scar rules. The sub-calibre core of an APCR round goes into the plate; its carrier is not modelled.
- **Scars from the lattice** (`ArmorField::record_measured`): aperture, back-face tear, petals,
  plug, cavity depth (a dent's resistance loss), worked radius and rim temperature are measured;
  a hole keeps the 94 % resistance rule and entrance funnel. The rim's cooling is the lattice's own
  (conduction through intact face bonds, radiation and 10 W/m² K natural convection from real
  surfaces only: broken bonds, the struck face, a back face the lattice reaches; the patch
  boundary adiabatic, named) as a ten-sample curve over 64 s, replacing the authored 2.5 s
  exponential for those scars (runtime only; a loaded scar is cold). The rim is the hottest
  surface particle still attached to the plate (bodies that broke free are the fragments). Such a
  wound glows at its local temperature: the lattice's surface field 0.5 s after the strike (struck
  face in rings of one cell round the shot's axis, bore wall in quarters of the depth; each ring
  the temperature whose red Planck radiance is its particles' mean, equal exposed area per
  particle named; the clamped edge and its damping ramp left out) is drawn as six face and four
  bore samples of each point's share of the rim's excess over ambient (5 bits each), scaled by
  the rim's solved cooling, with Planck radiance per pixel (rm_scars.wgsl `scar_local_kelvin`).
  Named: the field's own spreading by conduction after 0.5 s is not followed. Other scars keep
  the rim estimate over its authored ring. **Owner-directed departure (2026-11-02, "linear decay
  20 sec long"):** the drawn glow of every wound now falls linearly from its rim's peak to ambient
  over 20 s (`renderer_scars::WOUND_GLOW_S`), longer than the solved cooling, which takes the rim
  below red heat within seconds by conduction into the plate. Only the drawn emission changes; the
  solved curve stays the record. This is an authored emitter in both shipping presets, against the
  second principle's rule for new shortcuts, kept at the owner's explicit request;
  `KESSEL_SCAR_GLOW=solved` draws the solved cooling. Also on the owner's direction the same day:
  the authored red-brown primer flecks in the stripped band are gone (they read as rust), the
  authored grain and crater relief are a third of their former strength (the lattice gives the
  wound's scalars and heat field, not its texture), and the empirical 1/cos(obliquity) elongation
  is drawn at half its excess (the bore still runs down the shell's path). The near-patch renderer still draws a parametric
  profile from these scalars: the lattice's own displaced surface is not yet drawn. Not yet seen
  on the GPU.
- **Fragments from the lattice**: the clusters that broke free on the inside of the plate (mass,
  centre, velocity, spin and temperature as solved) are the penetration's fragment population;
  exterior ejecta are left to the cosmetic impact sparks. Material finer than a lattice cell is not
  resolved, so a penetration now yields tens of resolved chips rather than hundreds of authored
  ones. A filled round that burst adds its body's steel (mass minus filler) partitioned into the
  existing authored size shape and speeds, scaled to the exact mass (no fragmentation law); its
  temperature is not solved and keeps the authored heat pages. HE, HEAT and harness penetrations
  without a solve keep the authored population.
- **Every fragment** (lattice or authored) now flies with quadratic air drag (tumbling cube, mean
  area a quarter of the surface, Cd 1.05, 1.2 kg/m³; named) as well as gravity, and embeds in a
  surface when its normal energy can push its face a quarter of its size into 800 MPa steel (named
  cavity-work rule, applied to every surface whatever it is made of): it stays there, settled.
- **Fragments meet the vehicle's own surfaces** (2026-11-02, `game::FragmentWorld`,
  `integrate_hot_fragment_traced`): one swept segment per sub-step (at most 0.25 m of flight)
  against the retained source triangles the cabin's fire, foam and blood use (hull and turret
  with their interiors, gun, lids, ports, workshop parts; two-sided), posed as drawn that tick. At
  the first surface a body embeds, bounces (the existing authored restitution and tangential
  keep: 0.74/0.94 on a face turned up, 0.80/0.95 on one turned down, 0.82/0.96 on a wall) or comes
  to rest on a face turned up when slower than 0.42 m/s or past its bounce budget; one still
  moving on a wall or roof eight contacts past its budget is stopped where it is (named: no
  sliding or resting-contact solver). The rest of a sub-step after a contact is not flown (named:
  at most 2.1 ms per contact). Each segment's first 0.2 mm is not tested, so a burst born on the
  struck plate's back face leaves it. Since 2026-11-03 a fragment goes through a thin part it
  strikes (see "Every object inside stops or slows fragments" below); before, no surface could be
  perforated and thin sheet stopped it as armour would. A body 12 m from its frame's origin has left
  the vehicle and is dropped (it meets no ground; named). The damage pass follows the same bodies
  and meets modules where a body's path, ended at the first surface and carried 3 cm on (named:
  the drawn face and the voxel envelope differ by up to a cell), enters their CAD-measured
  component envelope, at the turret's and gun's pose; live cartridges where it crosses their
  oriented boxes; crew through the anatomical boxes along the same ended path. This replaced the
  cabin box (z 0.28–2.30 m), the turret cylinder and the breech block, with free flight outside
  them, and the module spheres (0.40–1.70 m radii round authored stations). Measured
  (`fea-film` probe, a fresh loaded Tiger with its crew aboard): before, every filled round that
  perforated into the engine bay (6 of 6, 85–152 mm) sent its fragments through the firewall and
  detonated the fighting compartment's ammunition, killing the crew; after, the same strikes
  open the fuel tank or engine behind the firewall and the fire catches there, all five men alive
  at 20 s. The pass took 60–143 ms for about 520 bodies (debug build), inside the penetration's
  tick. Solved bodies cool as lumped cubes
  (radiation, Ranz–Marshall convection; at rest by still air and radiation only: conduction into
  the steel a chip lies on or in is not modelled, named, so it cools more slowly than a real
  chip) and take the heat page of their temperature band (1,300 /
  1,100 / 900 / 750 K bounds, named; visible glow begins near the Draper point, 798 K); below
  750 K they draw on a cold, oxidised-steel page that emits nothing. A body the lattice solved is
  steel that stays: it never retires, and the fragment ring passes over resting solved steel, so
  only a pool full of it (768 bodies, named bound) gives up its oldest. The authored population
  still retires with its body's life. Resting steel draws in the fragment pages, which are
  raster-only: a cold chip casts no shadow in the ray tracing tier (named).
- **The dead as voxel bodies, solved on their cells** (2026-11-02, `game::crew_tissue`,
  `game::crew_corpse_tissue`, `game::crew_skeleton`). The skeleton is the owner's rigged
  low-poly PSX skeleton (its asset folder is named licence-unconfirmed: licence and author are not
  recorded, pending the owner), cut by its joints into the eleven body parts and fitted inside the
  relaxed figure (per part a principal-axis turn, 0.78 of the length ratio, shrinks to 70 % while a
  vertex lies outside by winding number, then each vertex still outside set 4–25 mm inside the
  skin; measured: none more than 3 mm outside, the most moved 43 mm). A fitted, not anatomically
  registered skeleton (named). The figure and the skeleton share one grid of 2 cm cells (built
  in about 100 ms at start): skin within a cell of the figure's surface (the clothing is this
  layer, named), muscle deeper (organs not told apart, named), bone where a skeleton triangle
  passes through a cell (the skeleton is a surface: its quads are voxelised, so a skull or ribcage
  is a shell). Only organic matter is cells (the owner, 2026-11-02): the steel helmet is a rigid
  body, below. A man a strike kills is drawn as his cells only (coloured from the figure's atlas
  by a 12-colour palette over the five faces, two authored muscle shades, the skeleton's mean
  albedo), never as the figure's triangles or the skeleton page; his helmet stays its own mesh.
  The strike is solved on the cells as a bond-based lattice (peridynamic in form, not a
  finite-element model): each cell a mass bonded to its 18 nearest neighbours by a spring that
  fails past a critical stretch in tension or compression (crushing, named). A fragment contact is
  a steel cube of its mass moving along its own path through the part's cells, slowed by the
  tissue's resistance and Newtonian drag (Poncelet's form: R·A + ρ·Cd·A·v²/2, Cd 1, R the tissue's
  tearing energy per volume; named); the momentum it loses goes to the cells within its half-size
  plus half a cell of its path, 0.95 along the path and 0.3 outward (authored). A burst's pressure
  work on a part (the existing crew model's) becomes kinetic energy of the part's surface cells
  facing it, shared by how squarely they face it (authored mapping). The lattice then runs 8 ms
  (symplectic Euler at a tenth of the stiffest moving cell's period, 0.1 % velocity damping per
  step: named numerical damping); only cells within 12 cm of a fragment's path, or of a part a
  burst works on, move, the rest of him holding them. Each struck part keeps its largest piece
  still bonded; every other piece comes away with its mass-weighted velocity and spin. Bond
  strengths are authored game-level values in the owner's order: tearing a cell free costs 0.4 J
  (skin), 1 J (muscle), 3.2 J (bone), at critical stretches 0.6 / 0.35 / 0.05; a bond takes its
  weaker cell's values (so a bone tears from the muscle before it breaks). The scale makes tearing all of a
  part's soft cells cost about its existing pulverizing work; not measured tissue strengths.
  Measured on the figure: a 3.14 kg 57 mm shot at 377 m/s through the torso tears 265 cells
  (a channel about 10 cm wide); a 20 g fragment at 700 m/s 27; a 2 g one at 400 m/s stops in him
  after 2; about 10 ms a solve (40–70 ms for a man several fragments crossed). A fragment's energy
  in the existing damage model (which decides death and the ragdoll's impulses) is unchanged. In
  solo play a pulverizing outcome removes a part from the articulated simulation only when less
  than 20 % of its soft cells remain; a part's mass is not reduced by the cells it loses, and the
  cells of a removed part ride the nearest retained segment, with no mass (named). The pieces fly
  under gravity and meet the vehicle's own surfaces (`FragmentWorld`) as a sphere of their
  bounding radius about their centre and, since 2026-11-03, by their outermost cells (the one
  furthest along each of 6 axes and 8 diagonals, swept with the piece's turn and its half edge
  ahead, so a chunk's side meets a wheel or seat; named: not every cell, and the pose is not
  pushed out of a face met sideways, only stopped), with no rotational response to a
  contact (named), restitution 0.25 and tangential keep 0.55 (authored: wet tissue), and rest on a
  face turned up below 0.35 m/s or after ten bounces; at most 4,096 cells in pieces (the oldest at
  rest go first). A piece that lands harder than its own bonds hold (half its mass times the
  square of its speed into the surface, against the breaking energy of its intact bonds) breaks
  into single grains, each leaving with the piece's motion at its place plus a hashed scatter of up
  to 0.3 m/s (authored); a falling piece also meets the grains already at rest (each a sphere of
  half a cell) and rests on them, so grains pile. Every visible cell is drawn as its own closed
  cube of one colour (cells of one colour blend; only cells with a face open to air or to a wound
  are submitted, about 3,600 for a whole man); cells within 3.5 cells of a joint are drawn
  between the two segments' poses (linear blend skinning of the drawn cell only; the lattice and
  the ragdoll do not see it; named), and only while that joint holds. All of it is ordinary moving
  instanced geometry (one page per colour, 8,192 cubes each, allocated at start so a save's draw
  layout keeps its shape): it casts shadows and is in the ray-traced scene, in both presets. The
  old under-body pool slabs are not drawn for a solved body (its blood is the droplets' film).
  The helmet (`game::crew_helmet_body`) rides the head segment while its strap holds. A fragment
  whose path crosses its shell first (two-sided ray against the helmet's own triangles) punches a
  plug out of it: shear plugging work ½·P·t²·τ over the fragment cube's perimeter P, with the
  shell 1.1 mm thick and 600 MPa in shear (authored game-level values, not a measured helmet
  steel), the residual speed by Recht and Ipson's momentum form with the plug flying on with the
  fragment (named); one below the plug's ballistic limit stops in the shell. The momentum the
  fragment loses goes to the helmet. A burst's work on the head reaches the shell in proportion
  to the shell's mean projected area (its mesh area over four, Cauchy's relation as the part boxes
  use it; named for an open shell) and drives it at its own speed; its push against the head is
  that momentum less the head's own. Past 1 N·s against the head (authored: a leather strap's
  breaking force, about a kilonewton, over about a millisecond), or once less than 20 % of the
  head's soft cells remain, the helmet comes away: a rigid body of its own mesh (1 kg authored,
  mass spread over its shell by area, the exact lamina inertia), under gravity, meeting the
  vehicle's surfaces (`FragmentWorld`) at 32 support points farthest-point sampled from its
  vertices (named: not its whole surface), with sequential contact impulses, restitution 0.3 above
  0.5 m/s closing and Coulomb friction 0.45 (authored, steel on painted steel), resting once quiet
  for 0.3 s. Limits (named): the shell's exit side and the hole a fragment leaves are not
  modelled or drawn; a loose helmet does not meet bodies, grains or other helmets; the parts are
  rigid segments, so at a bent joint two parts' cells interpenetrate at an angle; cells finer than 2 cm (a small fragment's
  track) are not resolved; only the strike that kills a man is solved on him; the living crew's
  wounds keep the existing rules; the couch co-op renderer draws a part whole or as bone as before;
  the solved state is not saved (a restored body is whole); dragging and picking a body still use
  its whole flesh triangles.
- **The compartment's heat, the ammunition's cook-off and the thrown turret** (2026-11-02,
  `cabin_heat`, `game::game_cookoff`; the owner: model the internal temperature; any live round,
  racked, chambered, loose or carried, ignites from the heat or from flames reaching it; one
  burning round sets off the rest and blows the turret off within a second or two; a direct hit
  on the racks goes at once). It replaces the earlier exposure fuse (a dimensionless flame
  exposure past 0.65 lit a 3 s fuse). One well-mixed gas zone in the burst model's 10 m³ of free
  air (ideal gas, γ 1.4): its mass and internal energy, heated by the burning surfaces (45 kW per
  surface patch at full intensity: about 0.12 m of burning oil, paint and stowage at 1 MW/m²,
  authored) and by burning propellant (2.5 kg a cartridge, `propellant_charge_kg`; heat of
  explosion 3.7 MJ/kg and the mass of its gas), losing heat to the hull's steel held at ambient by
  its mass (600 W/K: about 15 W/m²K over 40 m², authored), trading air through what is open
  (Kawagoe's ventilation factor 0.5·A·√H with H 0.5 m, authored for roof hatches), pushed out by its
  pressure through the openings and 0.01 m² of leaks (authored) by the burst model's orifice flow,
  never hotter than the propellant's flame (2,600 K, authored within 2,500-3,000 K); a closed hatch
  whose latch the gauge pressure beats is thrown open (the burst model's 20 kN). Ambient is the
  burst model's 293 K (the weather's air temperature is not used; named). No oxygen balance: a
  fire is not limited by its ventilation (named). Each case wall with the propellant heated behind
  it is one lumped areal heat capacity (9 kJ/m²K: 1.5 mm of brass and about a millimetre of
  propellant, authored), heated by convection from the gas (25 W/m²K, authored), the gas's
  radiation (emissivity 0.3 clean to 0.7 sooty, authored), the flames in sight of it (80 kW/m² at
  contact falling as (0.2/(0.2 + d))², from burning patches within 1.5 m with a clear line through
  the tank's own triangles, re-measured every 0.2 s; authored within impinging flames' 50-100
  kW/m²) and the flame of any burning neighbour (12 MW/m² at contact while it burns, falling as
  (0.05/(0.05 + gap))² with the gap between the cases: a case it touches ignites in about 0.2 s;
  authored). At 453 K (nitrocellulose's ignition, about 170-180 °C) its propellant ignites and
  burns in 0.3 s at atmospheric pressure, faster under pressure by Vieille's law with exponent 0.8
  (authored within 0.7-0.9). The turret (11 t, the Tiger I's turret with its gun; ring 1.85 m, real
  size) sits on its ring under its weight (no hold-down; named): the gauge pressure on the ring's
  area lifts it, the gap round the ring vents only once its skirt has risen 6 cm (authored), and
  once that gap is as large as the ring (a quarter of its diameter further up) it flies with the
  speed it has; while it is still over the open ring the leaving gas pushes it on, the gauge
  pressure on the ring's area falling as (D/(D + 2h))² with its height (authored). The thrown
  turret is the wreck turret's rigid body (`wreck_turret::Body::thrown`) built from the player's
  own Tiger at real size, its support samples kept one per 12 cm cube (3,485 of 233,288; the gap
  bound rises to 0.21 m; named), its thin-shell mass estimate (41 t) not the real 11 t (named: its
  flight is the same; contact impulses scale with it); everything authored in the turret's frame
  goes with it (`Tank::turret_thrown`), the dead in that frame are detached into the world first.
  The first round's bang kills the crew inside (the compartment at the propellant's flame: every
  man inside, not solved per body; the blast that throws them is the struck rounds' fillers and
  their propellant's TNT equivalent, 0.36 kg a kilogram); each later round is its own bang and
  flash and a flame from every opening. A round that ignites leaves the live inventory (its burst
  case and its shell are not drawn after; named). An HE shell's filler (the SprGr's 0.9 kg)
  detonates when a strike crosses it, when another HE shell within 0.2 m of it (centre to centre,
  about two shell diameters; authored) detonates, or when its steel body (36 kJ/m²K: about 10 mm,
  authored), heated like the case and by its own burning charge, reaches 570 K (TNT's cook-off,
  about 300 °C; authored): its energy (the burst model's TNT equivalence) and products go into
  the gas at once (the gas held at the flame's temperature at most; named) and its own charge is
  lit. An armour-piercing round's bursting charge (under 0.2 kg) is not counted (named); no shell
  fragments are thrown by a detonation in the chain (named). A strike that crosses a round sets
  off every round within 0.2 m of the crossings at once (authored reach of its flame and
  fragments). Measured (unit tests on the game's stowage at real
  size, two groups of 32 2.5 m apart): a round in a 60 kW/m² flame under a 300 kW fire ignites in
  about 20 s; its group pops over about 0.5 s; the turret goes 0.5-0.6 s after the first round at
  7-8 m/s (about 3 m up); a struck rack throws it within 0.6 s. In the game (fea-film,
  `cookoff_fire` and `cookoff_struck`): a flame staged on the rack's fitting ignites the first case
  in 12-13 s; the turret was thrown at 3.4-3.9 m/s (about 1 m) by the heated chain, at 2.8-6.9 m/s
  (0.5-3.6 m) by a 122 mm round into a rack of armour-piercing rounds, and at 17.6 m/s (about
  16 m) by one into the right sponson's twelve HE shells, which all detonated together (2.2 MPa).
  The death's outside shot frames the throw's whole height (the camera draws back on the cook-off's
  clock toward the turret's height plus v²/2g while it rises). Historical scale (2026-11-02 search): no measured heights; a
  T-34-76's full load set off in 1944 Soviet tests threw the turret roof about 30 m; Tiger wrecks
  show turrets beside the hull; a 122 mm hit in a Soviet trial moved a Tiger's turret 0.5 m. Not
  saved: the compartment's heat and the thrown turret (a restored tank is cold, its turret on its
  ring; named).
  - **The owner's whole-load rule** (2026-11-02, the owner: one shell going off inside sets off
    the rest; the toss near instant; a T-34's turret hundreds of feet up under a mushroom cloud
    taller than the throw). Once any round ignites or detonates, every other live round goes
    within 0.1 s (`cabin_heat::MASS_SPREAD_S`, each at its own hashed moment; authored), an HE
    shell by detonating (rounds going in the same update are presented as one volley since
    2026-11-03: one bang of their summed TNT at their TNT-weighted centre, one flash, one log line;
    each round's heat, fuel and stowage stay its own, and the crew die once, at the first); the heat and jet chain above decides only when the first goes. This is
    the owner's rule, not a solved propagation from round to round (named; the measured
    timelines above are with the rule off). The owner also asked how high the turret would go
    with the whole load's energy put into its rise: about 6 km (Tiger) and 12 km (T-34); not
    used, since the gas leaves round the ring once the skirt clears it and the turret only
    takes the pressure on the ring's area while it is over it (named). Measured with the rule
    (`cabin_heat` test `print_whole_load_throws`, every round struck at once): the Tiger's 64
    racked rounds (654 MJ with the HE fillers) peak at 11.5 MPa and throw its 11 t turret at
    41 m/s (about 87 m without drag); a T-34/76's 77 rounds (476 MJ) in 5 m³ of free air under a
    4 t turret on a 1.42 m ring, at 15.9 MPa and 62 m/s (about 194 m).
  - **The enemies' whole load** (`game_cookoff::enemy_cookoff_throw`): a turreted enemy's
    catastrophic kill (its ammunition struck) is solved once per kind in its own compartment
    (`cabin_heat`, one round struck, the rest under the rule, three closed 0.15 m² hatches) and
    kept: the launch speed, the burning mass for the fireball and the TNT equivalent for the
    front. The loads are authored estimates, named, not measured vehicles (full loads; the
    vehicle's own firing is not counted): T-34/76 77 rounds of 1.08 kg charges, two thirds
    OF-350 (0.71 kg), the rest 0.15 kg, a 4 t turret on a 1.42 m ring, 5 m³; KV-2 36 rounds of
    152 mm with about 2.5 kg charges, 80 % OF-530 (5.9 kg), 12 t on 1.535 m, 9 m³; IS-2 and IS-3
    28 rounds of 122 mm with 3.6 kg charges, 60 % OF-471 (3.6 kg), 9 t on 1.8 m, 8 m³; the
    casemates throw nothing. The thrown turret leaves along its own up axis leaning 0.18 of its
    speed away from the breach (authored) and spinning 0.9 rad/s; the rigid body flies it.
    Filmed (`fea-film` `cookoff_t34`, a T-34 killed through its ammunition by state, watched
    from 200 m): launched at 53 m/s, its centre rose 135 m (about 440 ft) and it came to rest
    130 m from the ring 15 s later. The first-shot blast and debris at the kill stay; the whole
    load's fireball, light and front follow on the next wreck update.
  - **The fireball and its cloud** (`game_cookoff::fireball`, `weapon_vfx::cookoff_fireball`,
    `renderer_raymarch` `EVENT_DETONATION`): the fireball of the load that went (propellant, and
    the fillers at their energy in propellant) by the published fuel-fireball correlations
    D = 5.8·M^(1/3), t = 0.45·M^(1/3) (Roberts 1982; CCPS), taken for a propellant fireball
    (named: they are fits to hydrocarbon fireballs): 129 kg, 29 m and 2.3 s for a T-34. The
    raymarch draws it as the 88 mm burst's authored envelope scaled: lengths by s = D / 1.5 m
    (the 88 mm's fireball taken as about 1.5 m; clamped 1-24), its clock slowed by s^(1/4), the
    emission and extinction per metre kept (the same burning gas and soot: an optically thick
    fireball's surface shines as the 88 mm's, its smoke's depth across grows with s), and above
    s = 4 a stem of five smoke parcels from the ground to the cloud. All of this is presentation
    scaling of an authored envelope, not a fireball or plume solve (named). Measured in the
    film: the glowing burst is some 50 m across in its first second, about twice the
    correlation's 29 m, because the 88 mm envelope's own glow is larger than the 1.5 m taken for
    it (named); the cloud stands about 100 m tall by four seconds and its cap rises past 130 m,
    as the owner asked (taller than the throw). The light is the blast light at half the
    diameter's radius for the fireball's life; the front is the load's TNT equivalent (71 kg for
    a T-34). The player's Tiger uses the same fireball for what actually went. Each round keeps
    its own report, but the compartment's flash and each opening's jet are issued at most every
    0.12 s (`game_cookoff::POP_INTERVAL_S`; authored): under the rule the load's pops come within
    a tenth of a second, and a flash and a vent per round per opening had filled the effect pool
    (48 bursts) and the raymarch's event list (96), crowding the fireball out. The fireball is
    kept in the pool like the plume and the vents, and the raymarch takes a scaled detonation
    first, in place of the oldest ordinary event when its list is full.
- **The crew's anatomical tissue** (2026-11-02; `crew_anatomy`, `crew_tissue`; the
  owner: build the crew's voxels from the character model with the skeleton and organs inside,
  each tissue with its physical properties). When an anatomical volume is present (the asset
  path, where the anatomy build now stands, or `KESSEL_CREW_ANATOMY`), the casualties' cells are its voxels:
  1 cm, in the relaxed figure's frame, labelled by the tissue table
  (`catalog/crew_anatomy_tissues.v1.json`: densities from the IT'IS Foundation database, blood
  fractions from ICRP 89, built by `tools/anatomy/build_anatomy.py`). Each cell's mass is its
  tissue's density. Its bonds come from its strength class:
  - skin, muscle and bone keep the existing authored energies, as densities (50, 125 and
    400 kJ/m³: at 2 cm the old 0.4, 1.0 and 3.2 J);
  - a bond takes the weaker cell's hold;
  - fat, brain, lung and the solid organs are skin-class, and hollow organs and cartilage are
    muscle-class (the table's authored mapping);
  - fluids (blood, CSF, gut contents, urine) are held by a quarter of skin's hold with a stretch
    of 1.0, so they stay until struck and then spill as pieces; the blood system's flow is not
    joined (named);
  - airway air is no cell.

  The outer shell takes the figure's colours at its nearest surface point, and every other
  tissue its table colour. The rig binding in the volume is not used: cells take the figure's
  part classification, and a joint's blend reaches 3.5 cells (3.5 cm at 1 cm; named). Without a
  volume the figure-and-skeleton build stands. Cost at 1 cm: 55,790 cells, a body built in about
  130 ms, and a killing strike's lattice solve in 0.4-0.65 s (about 25 ms at 2 cm). The solve
  holds the tick as the plate's lattice does; a worker solve is open. Since 2026-11-03 the step
  runs on up to 8 threads, each bond's force then each moving cell's gather in a fixed order, so
  its result is bit-identical on any thread count (test); a burst on a whole body (a cook-off's)
  solves in about 0.45 s instead of 2.3 s. When the strike is the player's own death (the tank's loss, or his own man
  in solo play) the casualties' solves run on workers behind the death's fade and black instead
  of holding the tick (the owner, 2026-11-02): the world is frozen through the death, finished
  solves are taken back only while the screen is fully black, and any still running is waited for
  before a view of the world returns or any update advances it, then the carve's consequences
  (pulverized parts, their coupled impulses, the pieces thrown) are applied to the unchanged
  ragdoll, so the outcome is the one the strike would have had. Other casualties still hold the
  tick. Not yet verified in a rendered death (the owner asked for no renders while he works).
  Since 2026-11-03 a fragment's path through him is its permanent cavity: every cell within the
  fragment's half-size plus a quarter cell of its path is crushed and drawn as gone before the
  lattice runs (a voxel discretisation of the track, named; the lattice still tears the walls).
  The part classification works point by point and left islands of a part bonded only to other
  parts (the relaxed left hand on the thigh: 121 forearm cells, 103 thigh cells), which came
  away whole and unhurt whenever their part was struck; each island now joins the part it touches
  most (`rehome_part_islands`; test: every part of the real figure is one bonded piece).
  `KESSEL_TISSUE_TUNE` scales the bonds' energy and stretch, the momentum shares and the landing
  break-up for the `fea-film` range review only: the harness opens its gate and nothing else
  does (test `the_tune_is_the_constants_unless_the_review_allows_it`), so play and both
  shipping presets keep the constants above.
- **Blood: droplets the anatomy's cell, a continuous coat, the owner's look** (2026-11-03,
  `surface_blood`, `surface_fire::SourceContacts::supports_cell`, `game::blood_material`).
  - Every droplet is a 1 cm cube, the crew anatomy's own cell (the owner: "blood voxels should be
    the same size as character voxels"), replacing the 1.5-6 mm distribution. The wound events'
    counts were set against that distribution; each count is converted to the 1 cm droplets that
    hold the same blood (its mean droplet volume, 0.031 cm³, against 1 cm³: about 32 to one), so a
    full burst is about a litre. A droplet wets its volume at a 1 mm coat (authored), about 3 cm
    across.
  - The coat is continuous across the facets of a curved surface: a film cell is covered by its
    own plane's triangles together with those of a facet turned by less than 25 degrees (the film's
    own facet carry) whose plane lies within sin 25 degrees of the cell's edge of every corner,
    their footprints projected onto the cell's plane. Before, a cell across a crease was covered by
    neither facet alone and left a bare strip a cell wide along every facet edge (the owner's pale
    lines through a pooled coat). A gap or a hole still has no cover (test), and the same rule
    applies to the fire's surface cells that use this test.
  - Look (owner-directed presentation): opaque, a bright saturated red base, roughness 0.12 and
    metallic 0.86, so it reads nearly black except in its reflections (the owner: "an opaque dark
    red, make sure it's shiny though and a bit metallic", "more red and brighter, but use the
    metallic knob to make it really dark like almost black"). Blood is a dielectric: its metallic
    response is the owner's art direction, not a measured property. The droplets, the coat and the
    pools under the couch dead share it.
  - Not yet seen rendered (no renders while the owner works).
- **The uniform tears and comes away with the flesh** (2026-11-03, `crew_tissue::Cell::cloth`,
  `CLOTH_BREAKING_STRAIN`; the owner: struck men left intact uniform shells round destroyed
  bodies). The body's outer shell of cells except the head is its uniform. Its bonds held as skin,
  whose authored stretch (0.6) the range's review tune multiplied four times, so the shell outlasted
  the muscle under it and stood on its own once the flesh inside was gone. Now a bond to a cloth
  cell lets go at a woollen fabric's stretch at break, 0.25 (wool fabric's elongation at break,
  roughly 25-35 %, the lower end: named), whatever the tune does to the flesh's, at the weaker
  cell's energy, so the cloth never outlasts the flesh. After each solve, a cloth cell with no
  intact bond to present flesh, or to cloth that has one, comes away as a single cell with its own
  motion (named: shredded cloth is cells, not a hanging rag). Not yet: the cloth's burning in a
  fire or under a hot fragment (the player's own clothing catches above 850 K,
  `ignition::CLOTHING_K`; the dead's cloth does not yet burn away: named).
- **The gun mantlet is armour on the gun's frame** (2026-11-03, `projectile_armor_source::plate_on`,
  `TIGER_MANTLET_NODE`; the owner: the mantlet appeared to be left out of the FEA). The armour
  contact surfaces held the hull, turret, hatches and visors but not the gun's frame, so a shell
  aimed at the mantlet passed through it and met the turret face behind. The mantlet's own node
  (Gun_0008, from the gun's baked mesh; not the barrel or breech) is now armour on the gun frame,
  posed with the gun's elevation; a contact on it is the scheme's 120 mm turret-front plate in
  cast armour (the materials table's cast mantlet), at its own face's angle, so the chain and the
  plate lattice solve it as cast steel. Named: a scar on the mantlet is kept in the turret's frame
  at the gun's pose when it was struck (scars are keyed by turret facing); if the gun elevates
  afterwards the mark stays where the mantlet was. A hit on the barrel is not an armour contact.
- **Refraction by blasts and hot air** (2026-11-03, `pressure_front` physical mode,
  `fea_damage::blast`, `HeatPlume`; behind `KESSEL_REFRACTION=physical`, off by default until the
  owner approves stills: the look is locked). One pass bends the camera's rays through two fields:
  - Blast fronts: each emitted front's shock radius from the blast model's Rankine-Hugoniot
    arrival (not the acoustic speed), the air behind it over its positive phase's length as the
    Friedlander shape, its refractivity jump by Gladstone-Dale on the Rankine-Hugoniot density. A
    ray is turned by the transverse gradient of n integrated along it (paraxial): the smooth profile
    sampled along its chord, the front's jump at each crossing with its grazing divergence cut at
    one pixel's footprint. Measured (1024 px over 60 degrees): 8.8 muzzle blast 2.4 px at 1 m,
    0.4 px at 2 m; 122 mm HE 17 px at 1 m, 3.5 px at 2 m, 0.8 px at 5 m; under a pixel beyond.
  - Hot-object plumes: Heskestad's point-source plume (centreline excess and width), excess capped
    at the source's own, heat into the air by free convection at 10 W/m²K (named); turbulence as
    hashed gradient noise at 0.38 of the local excess (George, Alpert & Tamanini 1977), half the
    plume's width in scale, carried up at the centreline rise (named: a statistical stand-in for the
    turbulent field). Measured: about 0.1 px at most (a glowing rim, 20 cm up), 0.04 px over a 20 kW
    fire at 1 m, under 0.01 px for chips and a 400 K barrel: invisible at the game's resolution,
    under a pixel even through the 2.5x sight. The physics gives this and no more.
  - The bent ray leaves its point of closest approach and is traced against the scene; what it
    meets is lit as the frame shows it there (named: radiance gathered from the lit frame at the
    true hit, not reshaded along the bent direction; a hit the frame does not show leaves the pixel
    as it was).
  - Not yet fed: the hot-object sources (fx-fire-sparks' `hot_surfaces`, pending its merge), and no
    barrel temperature exists yet. Not yet seen in a render (the owner's editor was open).
- **Every object inside stops or slows fragments** (2026-11-03, `FragmentWorld::perforation`,
  `game_fea::InteriorDamage`, `tools/tiger_game_part_components.py`; the owner: fragments passed
  straight through the traverse gearing, and every object inside should stop or slow them and be
  damaged by it).
  - What was wrong: the fragments' surfaces were the hull, turret and gun meshes and a list of
    hatches, periscopes, workshop parts and glass. The interior pieces drawn as meshes of their own
    were not in it, so fragments flew through them in the simulation, not only on screen: both
    handwheels (the gunner's elevation wheel and the commander's traverse flywheel over the
    traverse gearbox), the TPZ needle, the driver's yoke and button, the radio set and the order
    printer's head. They are now in it (the handwheels at rest about their hubs and the needle at
    zero bearing: their turning is not followed; the radio set at its base, a moved dressing prop
    not followed; named). The gearbox itself and the hydraulic drive are part of the turret mesh
    and always stopped fragments. The live rounds are still met only as cartridges (the rack), not
    as surfaces (named).
  - Through a part: at each surface a fragment strikes, the part's thickness along its path is the
    distance to the next surface ahead (its far face). It goes through when its kinetic energy
    exceeds 400 MPa (named: the nominal cast and structural steels) times its face area times that
    thickness, the embedding rule carried through the wall, and leaves the far face with the rest
    of its energy along the same line. A part with no far face within 20 mm (the cabin's 25 mm and
    thicker armour, solid parts, and open single-sided sheets: named) stops it as before, and so
    does a measured component with a solid interior, which keeps what enters it. One strength for
    every interior steel part (named: the sweep does not know which node it met).
  - New components from the game's own meshes (closed-mesh volumes, envelopes by the CAD
    voxeliser, casings and masses estimates): the commander's traverse gearbox (Turret_0137, kit
    05J27) and flywheel (wheel2), the hydraulic traverse pedal (Turret_0068), the elevation gearbox
    (Turret_0084, kit 06J56) and handwheel (wheel1), and the basket floor and its plates (TURRET
    FLOOR, Turret_0007, Turret_0204); the basket's drive is the existing hydraulic traverse motor.
    The basket floor's plates, about a cell thick, over-fill their 2 cm envelope by 18 % (named).
    Not identified: the gunner's own traverse handwheel (Turret_0086, 'HAND', is a 1.75 m rail).
  - Function: each strike's energy at a mechanism (a fragment's kinetic energy where it enters,
    the round's where it crosses) is summed per part. A traverse gearbox struck turns the turret
    stiffly in proportion, to a quarter of its free rate, and jams at 3 kJ like a jammed ring
    (powered and hand traverse alike); its flywheel counts half. The elevation gearbox does the
    same to the gun's elevation, jamming at 1.5 kJ, its handwheel counting half. The pedal is the
    hydraulic drive's control (the module, as before); the floor records its strikes. Thresholds
    authored (named: no damage-to-function data for these gears was found). Saved; not cleared by
    the field repair yet (named).
  - Not done: the struck parts do not yet change shape (no dents or bends drawn); the energy is
    recorded for it.
- **The round as a deformable body** (2026-11-03, `fea_damage::shot`, review film
  `fea-shot-film`; the owner: shells must squash, bend, ricochet and spin off the armour and break
  into shrapnel, each by its kind). Status: joined to play for perforations and ricochets. The round of a
  perforation is solved at 8 cells across (`game_fea::SHOT_CELLS_ACROSS`, every preset; 0.1-0.4 s
  on a worker beside the plate's lattice, the tick waiting for both) over one crossing time; the
  chain carries its residual mass past the plate (`chain::run_with_residual`: capability at the
  speed giving the lighter body the same DeMarre penetration, drag and plug shares at its mass, its
  shape the intact round's for the law: named), the residual's contacts with crew and modules take
  that mass, the pieces it shed through the plate join the interior's hot fragments at their solved
  motion, spin and heat (placed where each one's path crosses the back face, a piece still in the
  cavity going on unresisted; slower than 20 m/s it stays; filler pieces are the burst's), and a
  bursting round breaks only the steel it kept. A ricochet off any vehicle's armour is solved the
  same way on a worker and joined at that round's next contact (so the result never depends on when
  the worker ends): from then on the round carries its residual mass share into crew, infantry and
  gun contacts, and meets the next plate at the speed giving its lighter body the same DeMarre
  capability. The ricochet's outgoing velocity stays the authored `ricochet::response` (the solve's
  own exit speed is within a few per cent of it in the film's cases; named), the pieces it sheds
  outside the vehicle are not followed (named), a round saved mid-solve keeps its whole mass
  (named), and a second ricochet re-solves the intact round's build at its lighter share (named).
  Stops and lodged rounds still use the rigid round in play (drawn undeformed: named); flying
  rounds are drawn as tracer streaks, so the deformed shape shows in the review film and the range
  editor only. The review film solves at 12 across. Each kinetic round is a
  voxel lattice, 12 cells across its calibre, built from its kind: a hardened nose, a body, a soft
  base, a piercing cap (APCBC), a sheet-steel ballistic cap (2.5 mm, carried at its sheet's share of
  a cell), the filler in its cavity, or a tungsten-carbide core in a soft-steel carrier (APCR), its
  body length set so its mass is the round's. Soviet shot takes the hardness measured on captured
  rounds (Watertown Arsenal WAL 762/582 and 762/589: Rc ~47 at the nose, ~42 along the body, an
  air-cooled base near Rc 16-26, a V-notch below the bourrelet to fix where it breaks); German shot
  Rc 62 at the nose (secondary). Yield follows Banerjee's fit for 4340 (exp(0.0355 Rc + 5.5312)
  MPa, fitted over Rc 30-49, extrapolated beyond: named), hardening the secant of Johnson-Cook
  B = 0.634 sigma0, n = 0.26. Bonds are elastic-plastic springs (a bond-based solid: Poisson 1/4,
  named) yielding on their deviatoric part (a bond-level split, not a von Mises return: named),
  softening linearly toward melting with the plastic and friction heat they make; a bond fails in
  tension past its failure strain, or after 12 failure strains of accumulated flow (authored). The
  failure strain falls with hardness from 0.18 at Rc 20 to 0.02 at Rc 60 (authored: the sources
  give only the trend, harder noses shattering more readily); the tungsten carbide does not flow
  below 5.5 GPa and cracks at 0.2 % (authored). Pieces and loose cells that come together push
  apart; detached cells meet the plate along their own motion.
  The plate is a slab of voxels the round's size. A surface cell in a voxel still holding plate
  and advancing into it is pressed back along its own outward normal at Forrestal's
  cavity-expansion stress, static plus the plate's inertia (sigma_n = R + rho v_n^2), with kinetic
  friction 0.1; the voxel is gone once it has absorbed its volume times the calibrated mean
  pressure, so the nose cuts a cavity the shank passes through and a glancing nose a gouge. The
  calibrated mean pressure is the penetrating body's kinetic energy over the volume of its
  calibrated flat penetration (a sub-calibre round's core alone), times the plate's quality; the
  static part is that mean less half the plate's density times the striking speed squared, floored
  at a quarter of it (authored). Within a quarter calibre of either face the pressure and the
  voxels' energy ramp down (free-surface relief, floored at 5 %: authored). The voxels' cavity is
  this contact's only: the plate's own lattice is not coupled to the round's (named, one-way). The
  rifling spin is set from an authored 25-calibre twist (the guns' twists were not found).
  Measured (12 cells across, 3,100-3,700 cells, 2-9 s a solve on one thread): a 122 mm BR-471
  square on 100 mm at 780 m/s goes through keeping 24.8 of 25 kg (only its filler cracks); at 72
  degrees it gouges 80 mm deep and leaves the face at 530 m/s tumbling at 250 rad/s with 24.4 kg;
  a blunt 76 mm BR-350B stopped by 160 mm mushrooms (widest x1.5) and sheds 1.5 kg of nose; a
  76 mm BR-354P core stopped by 160 mm at 30 degrees cracks into pieces of a few cells; an 88 mm
  PzGr.39 through 100 mm at 30 degrees loses its piercing and ballistic caps and 1.5 kg of nose.
  Where the solve and the calibrated chain disagree (a round the chain puts through that the
  solve stops, or the reverse) the receipt reports both; the chain stays the outcome authority.
  Unsourced and named: critical ricochet angles, shatter-gap bands, perforation mass loss,
  driving-band stripping and the twists (docs/research/SHOT_DEFORMATION_SOURCES_20261103.md).
- **A fire lit by the strike** (2026-11-02, `fea_damage::ignition`, `SpallResult::ignite`; the
  owner: a spark or a fragment meeting what burns may start a fire inside the tank, each material
  at its own chance, some never, as often as fires started in the war). It replaces the rule that
  any round or fragment reaching the fuel or engine module lit a fire. The round and every
  fragment the strike throws (the lattice's clusters, a burst body's pieces) are traced through
  the measured component envelopes (the CAD voxels the chain uses). Each flammable component
  entered is one trial per strike, at the hottest body that entered it: the wartime rates are per
  shot, not per fragment, and a first cut that drew per fragment made a holed tank certain to
  light. The chances:
  - Petrol tanks: 0.5 a holing. NKV Laboratory 101-1 (1944) lit petrol tanks with 2 of 4 shots and
    full diesel tanks with 4 of 18; a small sample, named.
  - The engine: 0.3 (authored).
  - Mineral oils: 0.1 when a body above their hot-surface ignition entered: 623 K for lubricating
    and transmission oils, 713 K for hydraulic and recoil oil. The temperatures are from the US
    Bureau of Mines and NIOSH (Yuan 2006); the chance is authored.
  - Upholstery 0.02 and insulation 0.01, from a body above 700 K (authored; no wartime figures).
  - Coolant, steel, glass and the guns: never.

  A fragment must carry the energy to hole the component's casing: its module's resistance, else
  a thin sheet's 0.45 J. A body whose temperature the lattice did not solve (the round itself,
  the authored population) counts as cold: it lights petrol and the engine by its sparks only
  (named). The incandescent spark showers drawn at an impact light nothing (presentation, named).
  The draw is a counter-seeded hash of the penetration and the component. The fire starts where
  the lighting body entered, swept along its path to the first source surface. The first flame's
  strength is set by what lit (petrol and engine 0.16, oil 0.10, upholstery and insulation 0.06;
  authored). The ammunition is unchanged: a case crossed goes at once (`cabin_heat`), as
  dry-stowed charges "invariably" did in the 1943 US trials.

  A hot fragment above 850 K striking the player sets his clothing alight at 0.05 a strike,
  through `player_burn`; wool chars below about 570-600 °C (authored). The AI crewmen's clothing
  is not modelled (named). Flames behind the firewall heat the crew compartments' air at a quarter
  of their release (`game_cookoff::FIREWALL_SHARE`; authored; the bulkhead's temperature is not
  solved).

  Measured with `fea-fire-odds 1500` (CPU, 2026-11-02): 1,500 Soviet anti-tank rounds, 45-122 mm,
  100-800 m, with azimuths uniform round the circle, gave 498 penetrations. The section rig had
  every rack cradle full, fuel at 80 %, no crew, and hull and turret surfaces only. Fires started
  in 32 % of penetrations: ammunition 25.7 %, petrol 3.6 %, engine 1.6 %, oil 1.4 %. So 80 % of
  the fires came from the ammunition. By facing: hull front 26 %, sides 25 %, rear 24 %; turret
  front 74 %, side 57 %, rear 21 %. The wartime record:
  - ORS No. 2 (Normandy, 1944): 4 of 5 penetrated Tigers burnt out, with 3.25 penetrations each,
    about 0.31 a penetration.
  - ORS Report 33 (1945): 55 % of British tanks brewed up after one AP penetration.
  - The US 1943 AMRL trials: over 90 % of tank fires came from propellant.
  - The MRC survey (1946): of 107 fires seen, fuel 20, ammunition 19, other 3, unknown 65.

  The Tiger's 32 % stands with the Normandy figure, with no tuning.
- **Blood droplets and film** (2026-11-02, `surface_blood`; the owner: droplets too large, and
  blood should form a continuous film where enough lands). A droplet's edge comes from its seed,
  1.5–6 mm with most under 3 mm (an authored distribution, not a measured spatter one), and the
  film it leaves is its own volume over a fresh stain's thickness of 0.3 mm (authored): one 5 mm
  cell for the smallest, about nine (1.5 cm) for a 4 mm one, at most about 29. Before, every
  droplet grew a fixed 49-cell (3.5 cm) square. Film that lands on film joins it, so where enough
  collects it is continuous. On a face steeper than about 45° a pool spreads downhill first (an
  authored flow bias, not a fluid solve), so collected blood runs down a wall. The film now
  carries over an edge onto a neighbouring face turned by less than 25° (a curved wall is flat
  facets; the film used to stop at every facet edge, leaving lines of clean paint along the mesh):
  that one cell stands on its own centre's probe and may overhang the edge by up to half a cell
  (named). A flying droplet is still drawn at least 1.4 pixels across (the existing readable
  minimum, now from its own size).
- **Steel in hand** (`game_fea::fea_grab_fragment`): at the loader's station a resting fragment
  under the crosshair within 2.2 m can be taken with the left button when it is no hotter than
  343 K (a gloved hand, named) and, if embedded, when one man's 400 N pull beats its grip (the
  lodged-round friction rule on the cavity-work depth). It rides the same light spring as a spent
  case; releasing the button lets it fall under the ordinary fragment collision; R throws it at
  6 m/s (named). A throw whose line from the hand meets no source surface within 3 m goes out
  through an open aperture and leaves the simulation (named: steel outside is not kept). A chip
  the steel holds beyond one man's pull is freed with the toolbox (a field repair job: the bar,
  3 s named tuning, or counted sledge blows) and then rests loose for the hand. In the GPU review
  most solved chips embedded about their own size (5 mm) in the turret floor and needed the bar.
  The
  chip in hand is saved with its pool slot; fragments are saved with their solved temperature
  and embedding (`combat.projectiles` owner version 2).
- **Compartment pressure** (`fea_damage::compartment`, `game_fea::fea_compartment_burst`):
  quasi-static ideal-gas pressure from the filler's TNT-equivalent energy less the kinetic energy
  its own body's fragments carry, over a named 10 m³ of free air, vented through open apertures by
  isentropic orifice flow (Cd 0.6), hatches forced past a named 20 kN latch. Tested against the
  closed forms. In play: when a filled round bursts at the struck plate's back face inside the
  crew compartments' air (the turret, or the hull forward of the engine bay's firewall at
  z = -1.15 m, placed from the CAD envelopes between the fuel tanks and the sponson ammunition
  bins; sponsons included, machinery volume not subtracted: named), the openings are the four
  roof and escape lids and the cupola at their
  source apertures brought to real size (a lid counts open past half its travel, named) and the
  solved entry hole
  (its measured bore radii; the calibre's without a solve); a lid whose latch the pressure beats
  is opened in the game and then swings at its ordinary rate (named). A burst outside the crew
  compartments (the engine bay) gets no pressure history (named). Without a lattice solve the
  authored shower is not a partition of the round's body and nothing is taken from the filler for
  it (named). No shock or reflected-pressure history; the pressure acts on hatches only (crew
  injury stays with the existing crew model) and is not drawn. A 76 mm BR-350B's 65 g gives
  about 11 kPa, far below a latch (about 112 kPa on a driver's lid of 0.178 m² real area); a
  152 mm BR-540's 660 g gives 95 kPa, 17 kN on that lid, and every lid holds; a 152 mm OF-530's
  5.9 kg would throw every closed lid. (Until 2026-11-02 the lids were loaded at their drawn,
  1.24-times area, and the BR-540 threw three of them open in the GPU review.) The cross
  sections use the same rule with every lid closed.

## Armor scars, 2026-10-27 (branch `armor-scars`, `docs/ARMOR_SCARS.md`)

Permanent dents, holes, pocks and patches on every vehicle, one record per wound read by
ballistics (`plate_at`), saves (checkpoint v2, LAN protocol 8) and the raymarch renderer. What the
code computes and what it approximates:

- Terminal geometry (aperture, back-face scab, petals, plug) is an empirical rule from calibre,
  plate, obliquity and residual speed (`armor_damage::terminal_geometry`), not a plate solver.
  The record carries it as data so a voxel or particle plate solver can replace it.
- Crater depth of a non-perforating kinetic strike is 25 % (bounce, ricochet) or 15 % (shatter)
  of the round's penetration along the normal, scaled by pen / effective thickness, capped at
  45 % of the plate; the same depth is the resistance loss, so seen cavity and ballistic plate
  agree. HE keeps the energy rule (5 mm dish for a 76 mm OF-350). Not calibrated against a test
  plate series.
- Bullet and spall pocks are 1-3 mm splash craters sized from strike energy; they change no
  resistance (small arms stay inert against armor).
- Spall marks inside the Tiger: fragment strikes are resolved against the bounded cabin
  collision walls (pre-existing proxy); a mark is placed only where a trace from the wall along
  its normal meets a real cabin surface within 40 cm.
- Near the camera each wound is a dense patch generated from the record (local densification:
  the baked outer plates carry 0.27-2.2 m triangles, see ARMOR_SCARS.md), planar in the struck
  plate's tangent plane: on a curved casting the patch departs from the true surface by the
  sagitta (about 2 cm for a 0.2 m wound on a 1 m radius). Its footprint and the cut are clamped
  per sector to where a probe of the vehicle's source triangles finds the plate (16 sectors,
  0.1 R steps): the clamp error is one probe step.
- Mid and far: parallax occlusion of the analytic profile in the plate fragment (16 steps + 5
  bisections) with its depth written; the bore wall is an analytic oblique elliptical cylinder.
  Surfaces count as the struck plate when they face along its normal (|cos| > 0.6) within 3 cm.
- Holes are cut from the G-buffer and from every depth map by the same cylinder test, so the key
  beam and lamps enter exactly where the eye sees through. Daylight leak: one isotropic emitter
  inside each open hole of the player's Tiger, radiant intensity = outside sky band radiance
  along the plate normal x aperture area (a cosine-lobed aperture approximated as a point
  source; the sky as its three bands). No outward cabin-light emitter: cabin lamps are already
  unshadowed clustered lights in this backend.
- Behind a T-34 hole the inner faces of its exterior hull shell are seen: it has no modeled
  interior. The punched plug stops where its straight path from the bore exit first meets an inner
  surface of the same vehicle; no bounce or tumble.
- Rim glow: Planck radiance at the cooling rim temperature of the shared thin-ring heat estimate
  (`rim_heat`, 1.5 % of lost kinetic energy, 2.5 s time constant) over the crater floor, a hole's
  edge ring (0.2 calibre) and the bore wall. (2026-11-02: a wound on the player's Tiger that the
  voxel lattice solved glows at its own solved local temperature instead; see the layered damage
  model section.)
- Weathering: rust bloom (first order, 6 h), cavity grime (1 h), snow fill of sky-facing
  cavities at 4e-5 m/s at full precipitation (spindrift packing included). A loaded scar counts
  as a day old. Authored rates, not a corrosion or drift model.
- Wrecks: cannon rounds now strike dead enemies whose turret is seated (they used to pass
  through every dead tank). Still open: a popped turret (its contact frame is its old seat) and
  the dead player Tiger (shells still pass through it) take no further shell hits.
- Shader and physical backends: the 24 most significant player-Tiger wounds as rigid marks
  plus the track patch; enemy wounds are not drawn there.

Gate: `armor-scars PREFIX` (receipt + stills) and the unit tests in `armor_damage`,
`campaign_save` (512-scar round trip, v1 load), `lan` (window, worst-case packet).


## Strewn field (scatter), 2026-10-27 (branch `scatter`, `docs/SCATTER.md`)

A million-object litter layer placed deterministically from authored density and rule layers
and drawn by a GPU-culled indirect path on the raymarch backend (`src/scatter_field.rs`,
`src/renderer_scatter.rs`, `shaders/scatter_*.wgsl`). Implemented status and its limits, none
presented as a contact solve, a fluid or ballistic result, or exact occlusion:

- Contact: each object rests on the heightfield's drawn triangle surface at its centre, tilted
  to the local normal by its group's conformance, sunk by a group fraction of its height scaled
  by the snow cover; its own geometry is not solved against the ground. Bodies sink 6 %.
- Orientation by context (debris fans from a crater, cases in a gun's ejection sector, bodies
  along the authored direction of flight with 30 % reversed, drift tails along the wind) is the
  authored rule with jitter, not a fragmentation, gait or wind-transport result.
- Overlap avoidance covers medium/large pieces and bodies within one 32 m cell only.
- Interaction tiers by estimated mass and size (2026-10-28, after the "tank rocks from a small
  can" report): under 0.15 m or 5 kg nothing collides with the vehicle at all; an authored
  large piece under 400 kg or 0.45 m is sight cover only; above both it is static mass whose
  push resistance is sliding friction of its estimated mass (0.6·m·g over a 1 m shove), after
  which it slides 1 m along the push (its instance and cover box move). The mass is a
  bounding-box estimate with a material density by name, not a measured one. The medium tier
  is LOS cover only (`occluders`); its pushables move under the existing lightweight pushable
  model. Caps, nearest the route kept: 12 000 heavy, 3 000 medium occluders, 1 500 pushables,
  400 loot sockets. Everything else is visual, static after load, not burnable, not moved by
  shells, not kicked by crew on foot.
- Woody pieces (2026-10-28): anything named as a branch, twig, sapling, stick, pole, slat,
  plank, board, fence or log under 400 kg (per placement, its mass a green-wood cylinder of the
  piece's longest extent and 85 % of its smallest) has no vehicle collision; a heavier log
  resists by a rolling coefficient of 0.15 [I] over the 1 m shove. Not a rolling or snapping
  solve.
- Standing trees (`tank_push::standing_tree`, used by `trees.rs`): the bow meets the weaker of
  the stem's bending strength (modulus of rupture 40 MPa at 1.2 m) and the root plate's
  anchorage, 200 N·m per kg of stem [I] (tree-pulling regressions for conifers give about
  100-200, Nicoll et al. 2006; frozen ground at the top), the stem a green-wood cone (density
  800, form factor 0.5) of the measured trunk diameter and the placed height, tipping over 0.5
  m of bow travel. Before this only stem bending was modelled, so a 0.5 m trunk held the Tiger
  indefinitely; now only a tree of about 0.7 m and 12 m holds a crawling Tiger, and it goes over
  to one that arrives at 3 m/s. The anchorage is a regression constant, not a soil or root
  model.
- Draw: three LODs by projected size (the source mesh; a grid-clustered reduction; one native
  pixel carrying the mean albedo); no metallic-roughness or normal maps for the field's
  prototypes (the kit has none); shadows and sky cover from the large tier at the reduced LOD
  only, none into the point-light faces; the Hi-Z occlusion cull is the previous frame's depth
  under the previous camera (one frame of latency on disocclusion); nothing drawn beyond
  1 500 m (700 m for the shadow views) or below 0.8 projected pixels.
- Gates: `KESSEL_SCATTER=0` leaves the field out entirely; `KESSEL_SCATTER_SKIP=1` and
  `KESSEL_SCATTER_NO_HIZ=1` are measurement-only switches; `KESSEL_SCATTER_STATS=N` reads the
  visible counts back. Receipts: `captures/scatter_*` pairs and the `gfxbench` figures in
  `docs/SCATTER.md` §4.
- Not drawn by the shader backend.


## Terrain detail, 2026-10-27 (branch `terrain-detail`, docs/TERRAIN_DETAIL.md)

The Canyon v2 ground is the native 4097² Gaea export (1.2207 m; the 1025² grid it replaced was a
4x downsample) refined on a 0.076 m lattice that physics (`Heightfield::grade_at` ->
`terrain_relief::Detail::height_at`) and the raymarch backend's CDLOD terrain both evaluate; the
GPU surface matches the physics surface to 0.25 mm at 4096 probed lattice nodes (the rut atlas
is half-float on the GPU). Named
approximations:

- Nothing below 1.22 m exists in the source. Relief octaves 0.31-9.77 m (geometry at the finest
  level) and 0.15 m (shading normal only) are procedural value noise per ground class, amplitudes
  declared from field descriptions of snow waves, sastrugi, rubble and frozen ruts, not measured.
- Snow depth: 0.40 m declared settled cover redistributed by the Winstral maximum upwind slope
  (100 m reach, linear response, constants declared, not fitted to a 1942 survey); obstacles enter
  as box envelopes; no transport, sintering or melt over time.
- Ground class is derived per 1.22 m node from depth, slope, the course centreline and building
  footprints (rules in the cook), not surveyed.
- Ruts: tread bands from the hull's lowest 5 cm (as the track map), compaction 0.75 x local
  depth capped at 0.45 m, berm a quarter; boots 0.5 x depth capped 0.30 m every 0.75 m. A
  compaction rule, not a snow/soil constitutive solve; geometry and physics read the 12.5 cm map
  band-limited (3x3 binomial at 0.1 m); stamps are anti-aliased by cell coverage.
- Craters: cube-root scaling R = k W^(1/3), k 0.8 (frozen soil) to 1.3 (settled snow) m/kg^(1/3),
  depth 0.4 R, rim 0.12 of depth; filler mass taken as TNT. Not a cratering solve. The wreckage
  layer's authored scars stay meshes (not carved).
- Ground material (2026-10-28): the 2.56 m PSX detail tiles are hex-tiled with hashed rotation
  and offset, high-passed on upload and blended over two octaves; macro albedo/roughness drift
  follows cooked snow depth, wind exposure and relief with declared shares (+3 %, -4 %, +-1 %),
  and war dust/soot as value noise. A presentation of texture detail, not a measured BRDF field.
- LOD: away from the camera the drawn surface omits the finer relief octaves (morph-faded) and
  samples the lattice coarser; the physics surface keeps them. Bound: the omitted octaves' summed
  amplitude, <= 1.5 cm within 80 m, <= 5 cm within 160 m, <= 0.45 m (rubble; snow 0.3 m) beyond
  1.3 km where a pixel at 800x450 spans about 1.8 m. Rendered vertices are exact surface samples
  at every level.
- The raster/shader backends still draw the coarse 1025² mesh, now sampled from the detail
  surface at its nodes; between nodes it departs from the physics surface by the native residual
  (std 0.16 m, p99 0.41 m) plus relief. Open item; the raymarch backend is the one that draws
  the physics surface.
- The physics surface at a few fixed points (renderer track map on the raster path, the authored
  `crater_scar` decal rims, prop placements re-seated by the ground change under their anchor)
  is unchanged in kind.

Gates: `terrain-showcase PREFIX` (four viewpoints, day and night, GPU-vs-physics probe, ground
query cost; `KESSEL_TERRAIN_DETAIL=off` for the coarse grid) and `terrain-bench PREFIX FRAMES`
(GPU pass timings with/without the terrain, `KESSEL_RM_SKIP=262144` since integration, 8192 on the branch); unit tests
`terrain_relief::tests`, `terrain_lod::tests`.


## Enemy tank crews, 2026-10-28 (branch `t34-ai`, `docs/ENEMY_TANK_AI.md`)

Canyon T-34 platoons now fight an armoured target by fix and flank (`src/winter_encounter_ai.rs`,
`src/tank_doctrine.rs`); the crews still drive, lay, reload and fire through the same `Tank`
dynamics, turret traverse, reload clock, dispersion and authoritative shell solve as before. What is
decision-making only, and what is simplified:

- Armour solution: a crew's choice of post reads `kill_margin_mm`, the flat-plate budget of its
  BR-350B at the post's range minus `ballistics::effective_thickness` of the Tiger I hull face
  presented (front/side/rear by bearing from the hull's corner angle, plates from
  `ArmorScheme::TigerI`). It is crew knowledge of the enemy's armour, evaluated against the pose
  the crew last saw (or, unseen, a Tiger assumed to face it); no shell outcome is decided by it.
  It ignores turret faces, scars and the solver's per-shot variation.
- Hull-down: a candidate post counts as hull-down when the caller's optical sight test sees the
  target from 2.3 m above the post's ground and not from 1.1 m: two sight rays, not a visibility
  solve of the hull's silhouette.
- Gun-on-me: a crew in sight of the target reads its bore within about 6 degrees of its own hull
  as a gun laid on it (`GUN_ON_ME_COS`). A visual cue, not a model of what the eye resolves.
- Struck: a Tiger round striking the tank, or bursting on the ground within 9 m of it, counts as
  fire on it (`STRUCK_BESIDE_M`).
- Muzzle flash: when the Tiger fires its main gun, a crew looking at least side-on toward it,
  within 2 km, with a clear line (`los_clear`) through air whose `weather_view.transmittance`
  passes 3 % (`FLASH_MIN_TRANSMITTANCE`; silhouettes need 10 %), gets an exact fix of where the
  flash was at that moment, held 22 s like a sighting. A recognition knob, not a photometric flash
  model; the threshold is lower because the flash outshines a hull.
- Cued search: a crew whose fix (flash, sighting, radio or leader's signal) lies within 40 m of
  the Tiger accrues its detection meter 3 times faster (`CUED_SEARCH_*`); the envelope, arc,
  weather-contrast and line-of-sight gates are unchanged. A rule of thumb for cued detection, not
  a model of visual search.
- Engine noise: every 3 s each crew within an earshot radius (260 m idling to 700 m at full speed,
  minus up to 45 % in the storm, cut to 62 % through cover by `hear`) receives the existing
  uncertain sound report (rounded to a 32 m cell). The radius is tuning, not the acoustic
  propagation model the audio layer runs.
- Arena: `arena_setup` installs the evolved T-34 policy (`assets/ai/weights_academy.bin`) when it
  loads; it proposes manoeuvre and a fire hold only, as in the AI lab.

Gates: `cargo test --bin panzerrechnersystem winter_encounter_ai tank_doctrine enemy_ai_telemetry`
(CPU-only duels on open ground and a street district), `canyon-ai-run` receipts' `enemy_ai`
block, `ai-gauntlet`.

## Platoon wingmen, 2026-10-27 (`KESSEL_PLATOON=2`, branch `platoon`)

Two AI Tigers under the player's radio command (`src/platoon.rs`, `src/radio_llm.rs`,
`docs/PLATOON_MODE.md`). What is shared truth and what is simplified:

- Armour: a wingman is hit through the same swept `ArmorScheme::TigerI` contact and the same
  `ballistics` plate solver as the player's hull. No friendly-fire exemption exists: the player's
  rounds can hole a wingman and a wingman's rounds can hole the player; the gunnery only refuses
  to fire when a friendly hull lies within 6 m of the shot line.
- Behind the plate: a **simplified deterministic casualty model**, not the player's fragment
  shower. Five fixed seat points and six modules take a kill/loss probability from the solver's
  `behind_armor` lethality and the entry side (`ArmorPart`, `Facing`), drawn from a counter hash.
  Side hits pick a flank by hash because the plate contact does not report left from right. No
  fragment geometry, no interior occlusion, no fire spread timing. Named as such in the code
  (`platoon_on_penetration`) and the mode doc.
- Driving: the shared `apply_vehicle_avoidance` and the inverse-mass hull pair solver; the course's
  clear-cell test is a wall veto, not a path planner. A 1.6 s reverse-and-swing watchdog is a
  driver behaviour, not a physical recovery.
- Engagement: in contact the wingmen are driven by the evolved Tiger policy (`weights_tiger.bin`).
  It proposes drive, steer and a fire intent only; every physical gate (gun lay, travel lock,
  reload, ammunition, line of sight, avoidance, hull contact) is unchanged, as for the AI Tiger.
- Perception: enemies track, lay on and accrue detection against the nearest Tiger; the detection
  envelope (engine, hatches, silent watch) still comes from the player's hull state.
- Radio: the language model produces words and one validated order per transmission; it has no
  access to the simulation beyond the context JSON and can move nothing. The squelch burst is a
  synthesized stereo buffer, not a receiver model.

Gates: `platoon-proof PREFIX` (scripted orders over the grammar, replies with their path, exterior
captures, JSON receipt), `cargo test --bin panzerrechnersystem radio_llm platoon`. Off by default;
no shipping preset fields the platoon without `KESSEL_PLATOON=2`.


## Plate-solve obliquity ceiling and the Canyon run loop, 2026-10-27 (branch `run-loop`)

- `ballistics::MAX_OBLIQUITY_DEG` raises the angle the plate solve represents from 75 to 85
  degrees (`effective_thickness`, `resolve_at_velocity`). The ceiling predates source-triangle
  contacts, which report the true angle between the shot and the struck plate's normal: a round
  skimming a horizontal plate arrives at 85-90 degrees and was solved as a 75-degree hit on the
  thin plate. Canyon v2 seed 7 (2026-10-27, `captures/run_loop_baseline_s7_20261027.log`): a 76 mm
  BR-350B from 443 m, logged `HullFront 75.0 deg, penetration 92.3 mm / effective 71.6 mm,
  Penetration` through the 25 mm hull roof, nine modules lost, ammunition detonation at 58 s.
  With the ceiling at 85 degrees the same round is `85.0 deg, 62.1 / 120.1 mm, Ricochet`
  (`captures/run_loop_fixed_s7_20261027.log`); the slope law itself still saturates at 80
  degrees (`slope_multiplier`), so the steepest graze presents at most that LOS multiple. The
  legacy horizontal-only helper `horiz_obliquity_deg` (OBB proxy contacts) keeps its 75-degree
  limit. Regression: `ballistics::graze_tests`. This is a bound on an existing empirical slope
  law, not a measured ricochet-angle model; the ricochet response's 65/75-degree thresholds and
  restitution are unchanged and remain authored.
- The Canyon run's after-action card and opening brief (`src/canyon.rs` `RunReport`/`RunBrief`,
  `ui::draw_run_card`/`draw_run_brief`) are front-end presentation read from the game's own
  counters after the run has ended; they change nothing in the simulation. Play itself stays
  wordless. Per-attempt variation (`canyon::variation`) moves the manifest's authored start
  instant by whole hours on the same date (the ephemeris lights it) and phases the same authored
  winter fronts by the attempt seed; the medium's authored baseline, emitters, armour and damage
  laws are untouched. The headless `canyon-finish-check` and `canyon-foot-check` harnesses stage
  the Tiger with `debug_place_tiger` and are receipts of mechanism, not of balance.

## Raymarch backend, 2026-10-27 (`KESSEL_RENDERER=raymarch`, branch `raymarching`)

A selectable frame pipeline (`src/renderer_raymarch.rs`, `shaders/rm_*.wgsl`,
`docs/RAYMARCHING_ENGINE.md`). Surfaces are raster G-buffer visibility lit once; participating
media are a froxel grid (200x113x96 at 800x450) lit per cell by the key beam through two depth
cascades plus a marched medium transmittance, the sky through a top-down cover map, and clustered
lights (every offered light, not a 32 budget; two with depth faces). Named limits, none of which
are presented as ray tracing, GI or a combustion/solver result:

- Sky visibility is vertical only (one top-down depth map); horizontal openings leak nothing and
  covered points keep an 8 % residual sky term.
- Contact shadows, ambient occlusion and reflections are screen-space ray marches against the
  depth buffer; off-screen occluders/reflectors are missed, misses fall back to the sky bands.
- Lights beyond the two shadowed ones are occluded only by the screen-space contact test and the
  medium. A light's own housing mesh is excluded from its faces (the source sits inside it).
- The weather layer beyond the 320 m grid is homogeneous and unoccluded.
- Flame grains are merged into cells and drawn as a temperature field with authored tongue
  noise; fireballs and muzzle flashes are authored shapes driven by the burst records' age,
  energy class and position; spark/tracer medium glow is aggregated per burst.
- The eyepiece glass (refraction, dispersion, grime) is a presentation of the optic.
- Flakes, embers and sparks are one native pixel with projected-coverage energy.
- The froxel history reprojects by camera motion only.
- (2026-10-27, second pass) The physical sky is cached by direction: a 768x288 equirectangular
  map (0.47 deg per texel, the cloud layer marched with 20 view and 4 light steps instead of
  32/5) refreshed over 16 frames; the solar and secondary discs are drawn analytically from the
  same transmitted irradiance through the cache's cloud transmittance. Clouds and stars are that
  soft; below -20 deg elevation the bottom row stands in.
- Glass panes (`rm_glass.wgsl`): Fresnel at the entry face, absorption along the refracted path
  through the slab's thinnest extent, glints and screen-space reflection; no refraction offset of
  what is seen through the pane, no second-interface bounce, a scalar mean transmittance for the
  blend, grime from a fixed noise field. The cupola eyepieces are panes over the housing, not
  relayed views.
- Grass cards cast into the near cascade, the cover map and the point faces (not the far
  cascade); blades ask the cover map from 0.55 m above themselves, take no contact shadow, 70 %
  of their occlusion is lifted, and the key beam wraps round a blade (0.35 of the back-face
  cosine plus 0.15) as a stand-in for straw's translucency.
- Event sequels (`Event`): a detonation, muzzle report or heavy armour strike is kept for 24 s
  past its 1.85 s burst record; its fireball lobes, soot and dust, snow ring and column, fragment
  streaks, burning fragments, molten spray and the muzzle's snow cloud are authored motions
  seeded by the record, not fluid or fragmentation solvers. Grains the simulation throws from a
  detonation are read as its products (incandescent 0.25 s, then dark smoke). The muzzle brake's
  jet and side lobes shape the report's smoke, light and flash grains for 0.22 s.
- Spark samples' projected-area energy is capped (6 px, 3 px for event shards) so a shard
  passing the eye does not bloom into a disc; fragment and heavy-blast sparks trail 4-10
  samples per shard.
- Media phase is Henyey-Greenstein with an isotropic share standing in for multiple
  scattering (single scattering alone left the lit side of a thick smoke cloud black); the share
  is 35 % of (1 - exp(-2 sigma_t)), so thin weather aerosol keeps its single-scattering lobe
  (2026-10-28; it was a flat 35 %).
- Heat haze is a screen-space displacement of the G-buffer lookup by the heat integrated along
  the ray (fire and fireball cells), not refraction through an index field.
- Spindrift is a density field in the lowest 0.9 m over the cover map's surface, streaked along
  the wind, fading at the map's 48 m edge; not saltating snow. It lifts only above the 5 m/s
  threshold for loose snow (or in wind while snowing), at 0.012/m plus twice the weather layer
  (2026-10-28; it was 0.03/m from 2.5 m/s, a near-ground haze in the act's ordinary breeze).
- Snow glitter: one random ice facet per centimetre of terrain (a hash, not a crystal model)
  flashes the key beam at the eye through a narrow lobe; one native pixel each.
- Veiling glare (`stylized_bloom.wgsl` `fs_glare`, this chain only): see the "Light, sky and
  the eye" row; the display stage's rod signal takes the cool mesopic hue
  (`observer::ROD_DISPLAY_HUE`).
- (2026-10-28) Indirect light is a screen-space horizon gather (`rm_shade.wgsl` `screen_gather`):
  eight slices of eight samples out to 6 m of screen reach, no distance limit on the bounce; each
  sample that raises a slice's horizon owns the cosine-weighted band pi/8 (c^2 - h^2) and
  contributes the radiance the previous frame rendered there, reprojected through the previous
  view; the sky bands light the share the bands leave open. One bounce, visible surfaces only,
  one frame late, nothing from off-screen. A near-field horizon fading over 0.6-3 m is the
  contact occlusion. The GGX lobe's ambient is its directional albedo (Karis 2014 split-sum fit)
  times the same environment, less the reflection pass's gloss share. The upwelling-ground band
  (off-screen ground stand-in) is snow 0.80 in winter and earth 0.30 in spring (it was 0.30 in
  winter before, a bug), halved because most of the ground a surface sees is on screen. Gate:
  `KESSEL_RM_PROBE` on the noon Tiger still, hull plate 0.0039 -> 0.0068 (green, renderer
  units) against flat snow 0.171.
- Raymarch gunner view (2026-10-28; `renderer_raymarch.rs` `Temporal`, `sample_budget`,
  `rm_gbuffer.wgsl` `paint_source_metal`, `rm_reflect.wgsl`): each view slot keeps its own histories;
  the source model's default metal on the player's hull and turret is drawn as painted steel (the
  owner's gunmetal inside, the hull grey outside), bare steel only where wear scratches and worn edges
  expose it (a reclassification by the material's exact source values, not per-part source data);
  marched reflections fade out over roughness 0.2..0.5, the split-sum term carrying rough lobes; a
  sample budget scales the occlusion, gather and reflection rays below 800x450. The relays' sky-
  visibility atlas leaves the player's tank out (its own shadow is missing from the relayed views)
  and the relays do not rebuild the cabin lamps' shadow cubes: the relayed views take no light from
  point sources (lamps, fires, flashes; `CabinShadows::prepare_unmapped`). Gate: `raymarch-gunner`.
- Tiger exhaust plume (2026-10-28; `src/exhaust_plume.rs`, `rm_volume.wgsl` kind 7): puffs, not a
  fluid solver. Flow from the powertrain's speed and load; an engine temperature from a warm-up
  model (7 min at idle, faster under load, 30 min to cool); winter condensate always visible, densest
  cold at idle; black puffs on a hard throttle opening; effective jet speed a quarter of the bore's
  exit speed; buoyancy from the puff's decaying excess heat; drag toward the wind over 0.35 s;
  entrainment growth; extinction falling with the inverse of the radius (cold air keeps condensing
  the vapour). Densities, lifetimes and rates are presentation estimates sized from the engine's
  displacement and the stacks' bore. No shadow, no ground contact, raymarch backend only. The plume
  widens the enemy's detection radius by up to 25 % by day. Gate: `raymarch-exhaust`; unit tests in
  the module.
- Stars and discs at the pixel (2026-10-28; `rm_shade.wgsl` `rm_pixel_star`,
  `stylized_bloom.wgsl` `SKY_DISC_LIMIT`, `exposure.wgsl`): each lattice star's flux goes into the
  one pixel holding its direction (no seeing, no diffraction); air extinction from Kasten-Young
  airmass at an estimated zenith depth of 0.22; the sky discs feed bloom and the eye's veil at most at
  six times display white, an owner-directed presentation so their stepped edge shows (the real
  Sun's veil is far wider); the glare rings sample 1.5 texels apart; the eye's meter averages each
  cell's pixels rather than reading one.
- Explosion volumes (2026-10-28; `tools/bake_explosion_volumes.py`, `src/explosion_volumes.rs`,
  `rm_volume.wgsl` kind 8): shapes from simulated OpenVDB ground explosions (Cloudscapes V2 PRO,
  licensed to the owner) baked to 64^3 and normalised per slot; one snapshot per role, scaled,
  yawed, cooled and thinned over time rather than a simulated sequence; sizes and drift from the
  former parcel model for an 88 mm-class burst; the fireball a 60 ms afterburn decay; extinction from
  an estimated mean optical depth (burst 3.5 at 2.5 m, cloud 4 at 3 m, falling with the area); one
  albedo for soot and earth. Ground ejecta speeds follow M(>v) ~ v^(-1.2) between 0.15 and 1 of the
  apex-limited maximum (crater scaling, mu about 0.4), drawn apart from direction. Gate:
  `raymarch-showcase` he_impact stills; `explosion_volumes` and `weapon_particle_math` unit tests.
- Cabin or outside, per surface (2026-10-28; `rm_gbuffer.wgsl` `gb_cabin_air`): a surface is the
  cabin's when the point 3 cm off its visible side lies in the cabin air's columns (2 cm columns,
  `cabin_medium.rs`); the exterior grey swatch on a cabin-facing surface is read as the ivory there
  (a reclassification by exact swatch, not per-part source data).
- Crew compartment paint, grime, lamps (2026-10-28; `tank.rs` `tiger_paint_finish`,
  `surface_wear::CAMPAIGN_START`; `cabin_grime` and the `lamp_lens` profile were removed on 2
  November 2026, superseded by the vehicle surface model and unreached): ivory from a
  common RAL 1001 conversion, not a measured surviving interior; grime an authored mask (dust up,
  oil low, soot high) at one scalar load, 0.65 at the start (an estimate of weeks in the cauldron);
  lamp lenses an authored profile (core, rim, glint, cage) carrying the cube face's emission (the
  indicator lamps without a bezel since that evening, the ledger's "Indicator lamps" row); the
  raymarch camera halo 0.06 at half spread, a presentation choice.
- Spatio-temporal filter on the shade's stochastic terms (2026-10-28; `rm_shade.wgsl`
  `shade_history`): the ray queries' and the screen gather's open shares and the bounce blend into
  the mean of the 3 x 3 reprojected history texels of the same surface, not each pixel's own, and a
  new bounce estimate is capped at twice that mean: the indirect light and sky occlusion are
  smoothed over a few pixels (sharper contact detail stays in the near-field ao term).
- The cabin without ray queries (2026-10-28; `rm_visibility.wgsl` `rm_cabin_sky_visibility`,
  `rm_shade.wgsl`, `rm_reflect.wgsl`): skylight in the crew compartment is the top-down cover map
  asked 30 cm inside along the surface normal, all-or-nothing under an open hatch (no solid angle);
  no light through the vision blocks; lamps without a shadow cube unoccluded; an off-screen
  reflection is the surface's own lit radiance. Own-colour emitters leave the depth maps only while
  they glow.
- The cauldron's fire glow (2026-10-28; `physical_environment::KESSEL_FIRE_GLOW_FLUX`, both
  cloud-boundary solvers): an upward diffuse flux of 1.2 mlx photopic under the cloud base, a 1900 K
  Planck colour, returned to the ground by the cloud's delta-Eddington diffuse reflectance; an
  estimate set so the overcast moonless night reads dark but readable (about 1 mlx on the snow),
  uniform over the pocket, constant in time, campaign-wide; no second bounce between snow and cloud.
- The cloud deck from below takes at least the two-stream's exit radiance (2026-10-28; `sky.wgsl`
  `march_clouds`, `cloud_base_floor`): a floor on the march's octave estimate, not a transport solve
  inside the cloud, in compact form: six column steps, the secondary body's beam and the clear sky's
  diffuse light above the deck left out (the march's own skylight term carries the latter).
- Raymarch radiance stores (2026-10-28; `renderer_raymarch.rs` `radiance_format`): froxel
  in-scatter, the integrated column, the sky cache and the reflection history are 32-bit floats; on
  a device without float32 filtering they and the frame fall back to half floats, which hold no
  radiance below 6e-8 renderer units (night light vanishes there).
- Cabin lamps stay in the cabin (2026-10-28; `rm_shade.wgsl`, `rm_volume.wgsl`): any mesh enclosing
  a shadowed lamp's position is left out of that lamp's cube as its housing, which for the cabin lamps
  includes the whole turret and gun; a lamp inside the crew compartment's air domains therefore lights
  only surfaces facing that air and the air itself, the armour taken as opaque. Interior fittings of
  the turret and gun meshes cast no shadow for the cabin lamps.
- TZF reticle at the output's resolution (2026-10-28; `blit.wgsl`): the Strichbild is drawn after the
  upscale at its true angular size, four samples a pixel, rather than in the crushed frame; a
  presentation of the glass reticle, it changes no light.
- Small-arms tracers and flashes (2026-10-28; `rm_sparks.wgsl` `vs_tracer`,
  `renderer_raymarch.rs` `TRACER_INTENSITY`, `tank.rs` `TIGER_COAX_PORT`): the tracer pellet is
  about 60 cd, an estimate from the 7.62 mm trace requirement (visible by day to ~780 m) against a
  1e-4 lx detection threshold, not a measured candela; the strip conserves that light over the
  exposure's path (pixel-plane geometry, the depth along the view axis); only the eight nearest heads
  within 150 m light the scene; a hot impact's light is ~5 cd s spread over 40 ms so that it reaches
  two frames (the real flash lasts milliseconds). The player's coax fires from the source mantlet
  port; AI tanks' coax emitter stays out along the barrel. The eye's meter does not see the strips
  (drawn after it); their veiling glare reaches it computed on the host (CIE disability glare by the
  meter's windows, occlusion by the barrel ignored); in a sight the strips take the eyepiece's
  barrel distortion by a two-step inverse, not its lateral colour. Gate: `raymarch-tracers`.
- TPZ needle and instrument covers (2026-10-28; `game.rs` `TPZ_NEEDLE_WIDTH_M`,
  `Renderer::mark_instrument_cover`, `rm_glass.wgsl`): the TPZ needle is drawn 4.5 mm wide in the
  dial's plane (its source bar is 1 cm; 2.5 mm that morning); instrument covers carry an authored
  fixed dust (millimetre flecks at 0.06, a 1-2 % film), not a measured surface. The morning's second
  glossy lobe (roughness 0.3) is removed: added to the glint at the full Fresnel it doubled the
  cover's reflection.
- Sight lamps (2026-10-28; `Game::sight_lamp_states`, `interior_rig::SIGHT_LOAD_FIXTURES`,
  `SIGHT_ROUND_FIXTURES`, `KESSEL_SIGHT_LAMPS=0` removes them): a gameplay instrument the Tiger
  did not carry (its loader reported by voice or a tap): a loading lamp and a chambered-round lamp
  on the turret wall beside the TPZ and on the cupola ring beside the commander's front vision block
  (`lamp_mounts::SIGHT`), real emitters at the
  cabin lamps' power, driven by the reload machine's state, never by the player's selection. The
  APCR lens never lights because the reload machine chambers APCR as the armour-piercing round.
- Winter whitewash (2026-10-28; removed 2 November 2026 with its frame lane, its renderer field and
  `KESSEL_WHITEWASH`: the owner keeps the Tiger grey in every season, and the vehicle surface model
  has no whitewash layer; the description below is the former coat): in
  winter acts the player's Tiger's exterior paint (recognised by its exact swatch) takes a chalky
  white coat (linear 0.56-0.72), thin where the brush ran dry, worn through on the decks, streaked
  down the sides; an authored mask in the mesh frame, raymarch backend only. Gate: `raymarch-cabin`
  stills `tiger_front_quarter`, `tiger_rear_quarter`.
- Gunner sight view damping (2026-10-28; `Game::sight_view_model`, `KESSEL_SIGHT_DAMPING_S`): the
  gunner's view follows the hull's pitch and roll through a 0.25 s low-pass because the hull pose
  is fitted to the ground every frame without suspension dynamics; a comfort filter, not a head or
  brow-pad model. The sight mesh, the bore and the shell keep the true pose. Gate: `raymarch-cabin`
  receipt `sight_line_driving` (jolt 11.8 deg/s RMS raw, 1.1 as viewed, v2 canyon detail terrain).
- TZF eyepiece and glass (2026-10-28; `crush.wgsl` `tzf_field_distance`, `rm_lens.wgsl`): one
  eyepiece field, the monocular TZF 9c's (owner-directed; the Tiger's early TZF 9b was binocular,
  drawn as two fields until that evening); a 0.5 %
  coating-tinted ghost and wiping arcs scattering 3 % of the local field light (authored); the glass
  layer and the exit pupil's shading shift by the eye's offset from the sight's axis
  (`Renderer::optic_eye`), clamped to 5 % of the screen. Cabin dust motes take the hull's full pose.
- Raymarch relayed sights and cabin air (2026-10-28; `renderer_raymarch.rs` `relays`,
  `raster_optics.rs` `encode_raster_optic_relays`, `rm_visibility.wgsl` `rm_cabin_domain` and
  `rm_surface_motion`): the vision blocks show the shader backend's exterior raster capture per
  port (its lighting and weather, not this backend's transport), an interim until the ray
  queries carry the periscope image. The froxel medium and the one-pixel particles use the
  shader backend's cabin-air domains: no weather and no sky in-scatter inside them except
  straight down an open hatch, no snowflakes inside, no dust motes outside; light entering the
  air through the vision blocks is left out. Histories of points in (or 5 cm beneath the surface
  of) a cabin domain reproject through that rigid frame's motion. Gate: `raymarch-cabin`
  (`captures/rm_cabin_before_*` against `rm_cabin_after3_*`; consecutive-frame difference in
  motion 5.53 to 0.50 sRGB codes).
- Raymarch temporal filter and pane reflections (2026-10-28; `rm_shade.wgsl` `shade_history`,
  `rm_glass.wgsl` `pane_offscreen`): the screen gather's open share, contact term and bounce are
  averaged over frames (new estimates 0.2) with a disocclusion mask and a firefly clamp (eight times
  the history's luminance), which delays a genuine brightening by a few frames; the contact marches
  use a fixed per-pixel offset (a still pattern, not converged). A glass pane's reflected misses are
  traced through the scene instead of reflecting the sky under cover. Before-and-after captures are
  pending a GPU window.
- Raymarch cabin haze centre (2026-10-28; `rm_medium.wgsl` `rm_cabin_density`): the haze
  ellipsoid is centred on the crew compartment (0.9 m above the turret frame's origin) instead of
  the hull's ground-contact point, which had left the fumes below seated eye height. The ellipsoid
  itself remains an approximation of the compartment's volume. Gate: `KESSEL_SHOWCASE_SET=shafts`.
- Raymarch lamp shadow rays and periscope folding (2026-10-28; `rm_shade.wgsl` light loop,
  `rm_rt.wgsl` `rm_ray_open`): lamps without depth faces take one shadow ray per shaded pixel,
  stopped 0.15 m short of the source so its own housing does not shadow it; occlusion rays pass
  periscope windows and reflect at their mirrors, so the cabin takes skylight through the
  periscopes, as the hemisphere's sky band, not as the image the periscope carries. Gate:
  `KESSEL_SHOWCASE_SET=ao` and `=res` (`cabin_night`) with and without `KESSEL_RM_SKIP=32768`.
- Raymarch cloud shadows and fog banks (2026-10-28; `rm_cloud_shadow.wgsl`, `rm_medium.wgsl`
  `rm_fog_bank_density`, the far march in `rm_shade.wgsl` `fog_to`): the key beam is the clear-sky
  one and the cloud layer's transmittance toward the sun is applied per position from a 16 m map
  over 4 km (the same cloud field the sky draws, crossed from the eye's altitude); fog banks are a
  drifting footprint field below a floor altitude, an authored presentation of radiation fog, not a
  boundary-layer solve; beyond 320 m the medium is marched in six steps without geometric
  occlusion. The cloud does not shadow the skylight. Gate: `KESSEL_SHOWCASE_SET=clouds` with and
  without `KESSEL_RM_SKIP=131072`.
- Low cloud and mist banks inside the map (2026-11-02; `src/low_clouds.rs`, a map's
  `manifest_edits.json` `low_clouds` block, `rm_common.wgsl` `rm_low_*`, `rm_low_clouds.wgsl`,
  docs/RAYMARCHING_ENGINE.md "Low cloud and mist banks inside the map"). Computed: extinction from
  the authored liquid water content and effective drop radius, sigma = 3 W / (2 rho_w r_e)
  (extinction efficiency 2, Hansen and Travis 1974; Stephens 1978); Mie albedo 0.9999 and
  asymmetry 0.85 of water drops; the banks are a term of the one world medium, lit in the froxel
  grid, and every key-beam query takes their transmittance, so they shadow ground, vehicles, the
  medium and themselves. Named approximations: the shape is an authored ellipsoid with a noise
  edge (no condensation, growth or decay); multiple scattering is the two-stream transmission of
  a conservative layer, T = 1 / (1 + 3/4 (1 - g) tau) (Bohren 1987), added as isotropic sky and
  diffused key-beam radiance (no angular structure, no exchange between banks, lamps scatter once);
  optical depths and extinction come from a 176 x 64 x 176 grid of 4 m texels round the eye,
  trilinearly interpolated, half its slabs refreshed a frame while it holds still (update
  cadence); beyond 320 m six samples per bank crossing, the weather haze between the grid's edge
  and a bank applied behind it; the drift is the world wind times the gust, integrated per frame,
  wrapping in the authored area with a one-radius fade at its edge; the base follows the mean
  ground under the footprint. Open: the simulation (AI sight, perception) does not read the
  banks; the shader and physical backends draw none (the editor shows them with
  `KESSEL_RENDERER=raymarch`). Gate: `KESSEL_RM_SKIP=4194304` (debug A/B, removes the banks);
  `cargo test -- low_clouds` (extinction law, absent block, round trip, drift, optical depth).
- Surface wear on the player's Tiger (2026-10-28; `src/surface_wear.rs`; its `apply_wear` noise was
  removed on 2 November 2026 and the loads now set the vehicle surface model's amounts; the shade
  pass's worn edges and cavity grime serve the other vehicles; the glass film): four scalar loads,
  mechanical wear (+0.4 % of the remainder per round), interior grime (toward full over 40 min
  crewed, +3 % per round), exterior grime (toward full over 3 km driven), glass grime (+2 % per
  round, over 8 km, toward full over an hour of full snowfall; a wipe leaves 0.05), starting at
  0.55 / 0.35 / 0.30 / 0 for a Tiger some weeks into the cauldron with its optics wiped clean (the
  glass start was 0.25 under a fixed film until 2026-10-28; the gunner's sight carries no grime). Presentation estimates, not measurements; procedural masks, not painted
  or simulated wear; inside and outside told apart by facing the compartment's centre. Gate:
  `KESSEL_SHOWCASE_SET=wear` (clean against after a fight, and every load at 1).
- Raymarch ray queries (2026-10-28; `rm_rt.wgsl`, `rm_shade.wgsl` `rt_open_share`,
  `rm_reflect.wgsl` `offscreen_reflection`): occlusion traced with two cosine rays per pixel per
  frame to 4 m and averaged over frames (new estimate 0.15); cover beyond 4 m counts as open, and
  cover the frame does not show returns the surface's own last-frame radiance. Reflections trace
  screen-space misses to 80 m; an off-screen hit is a grey 0.3 surface facing the ray lit by the
  sun cascades and the sky cover map; the reflected radiance is averaged over frames (0.2) through
  the reflecting surface's reprojection. The TLAS is rebuilt each frame (no refit in wgpu 27).
  Gate: `KESSEL_SHOWCASE_SET=ao` with and without `KESSEL_RM_SKIP=32768` (the closed cabin by day
  78 to 39 mean luma: it was lit by skylight through walls the frame could not see).
- Lying snow on the raymarch backend (`src/snow_cover.rs`, `rm_common.wgsl` `rm_snow_lying`): a
  scalar load per vehicle (the player's Tiger, starting bare) and one for the world (0.7 at the
  start of a winter act, everything else including enemy vehicles). Snowfall fills a load at
  precipitation / 120 s x (1 - load); the Tiger sheds 0.5/s x ((speed - 2 m/s) / 10 m/s)^2 of it in
  the air stream, 0.4/s per rad/s of turret traverse, and a shot keeps 55 %. These are
  presentation estimates, not snow-physics measurements. The renderer covers faces flatter than
  about 63 degrees that the falling snow can reach (the sky-cover map), broken by a noise field in
  the mesh's frame, with a matte layer (albedo 0.86/0.89/0.93, roughness 0.85). Not modelled:
  per-face accumulation (leeward faces, melt on hot parts), snow depth and its shadow, contact
  puffs, falling clumps, enemy vehicles shedding snow, snow over the near-vertical optics. Gate:
  `KESSEL_SHOWCASE_SET=snow` time series (loads 0.00, 0.58, 0.20, 0.43, 0.25, 0.23).
- Tank ruts on the raymarch G-buffer: the track map darkens and polishes the compacted floor and
  tilts the shading normal by its gradient; the terrain triangles do not move.
- Exact RT surface pack (physical backend): an instanced transform with a singular or
  non-finite linear part is traced as a 1 mm point at its translation instead of aborting the
  frame (`rt_safe_instance_transform`), reported once per batch with the matrix. A zero-volume
  instance meets no ray; the departure is only that the frame continues.
- Winter snow sinkage (`vehicle_ground::WINTER_TRACK_SINKAGE_M`): 0.40 m of settled steppe
  snow (declared for the December 1942 cauldron, not a station measurement) compacts to a
  quarter of its depth under every tracked vehicle fielded here, so tracks rest 0.30 m below the
  drawn snow surface. The player's support profile and the AI heightfield rest use the same
  constant (the player's earlier authored 127 mm is replaced); cellar floors and the Act-II bridge
  stay hard. The snow is not displaced or banked around the hull, the belly does not plough, and
  no vehicle sinks differently by its ground pressure.

Gates: `raymarch-showcase PREFIX` (16 stills with a JSON receipt; `KESSEL_SHOWCASE_SET=palette`
for 18 more across the day and the weather) and `gfxbench` with `KESSEL_GPU_PROFILE=1` on the
raymarch backend; the shader and physical backends are unchanged and remain the shipping
defaults until this backend passes the same reviews.

## Missions: signal stars, smoke columns, alerts (2026-10-27, branch `missions`)

`src/mission.rs`, `src/mission_game.rs`, `docs/MISSIONS.md`. The mission's in-world cues are
things in the world, with these named limits:

| Cue | What it does | Where | Limit |
| --- | --- | --- | --- |
| Signal star (flare) | A 26.5 mm star: launched at 46 m/s, gravity to the apex (about 110 m), parachute descent at 3.2 m/s drifting with the ground wind, 24 s burn fading over 3.5 s. One point light with a 90 mm source radius and the star's colour, occluded and scattered like every other light; a 0.42 m emissive grain so the star is not sub-pixel at a kilometre; a launch-smoke thread and a thin thread above the star in the volumetric pool. | `mission::Flare`, `Game::mission_lights`, `mission_smoke`, `drive_mission_flares` | The radiant intensity (900, renderer units) is set by eye, not photometrically; the grain is larger than the star; the parachute is not a drag model (a fixed terminal rate and a bounded approach to the wind). |
| Smoke column | Three source-connected media supports from the base to the height, leaning downwind by the ratio of wind speed to a 2.2 m/s rise, growing over the first 20 s, fading over the duration. | `Game::mission_smoke` | No flame, no ember light, no heat: a burnt-out vehicle still smouldering. The pool is 12 supports shared with every other plume. |
| Alert | Unaware platoons within the radius enter the stealth model's own Searching state with the Tiger's current position as the last known area and the model's own 22 s timeout; guns within the radius (field-of-fire factor as `at_guns_hear`) have `heard`. | `Game::mission_alert`, `stealth::AlertState::Searching`, `at_guns_hear` | Grants knowledge the model could have given only by hearing; nothing is aimed, fired or moved by the cue itself. |
| Release | A platoon through the course director's own entry and siting (`canyon_platoon_site`, `canyon_reinforcement_entry`). | `Game::mission_release_platoon` | None beyond the director's. |
| Compass bezel | The pocket compass body turns to the ordered bearing; the needle keeps north. | `hand_map::compass_models`, `hand_compass::models` | The body carries the dial and the housing together, so the housing turns with the bezel (a real Marschkompass turns only the ring). |
| Position estimate | A pencil ring on the map: the true position displaced by an error of 4.5 cm per metre travelled since the last fix (a completed beat), 12 m to 110 m, in a direction fixed per fix. | `MissionState::estimate` | A design value standing in for dead-reckoning error, not a measurement. |

## Native geometry coverage, 2026-10-01 (consolidated shader renderer)

The shader backend now defaults to four hardware coverage samples per native geometry pixel.
Main and every raster optical exterior view use the same source meshes/material shader and
four-sample sky depth gating. Resolved color is an average in linear HDR before exposure,
bloom, tone mapping and the unchanged native Bayer8 grid. No temporal history or upscaling
filter is added. Centroid interpolation keeps the shaded material point on a covered primitive.
`KESSEL_GEOMETRY_AA=off` retains the one-sample diagnostic baseline; `msaa4` is the default.

The depth resolve deliberately retains the nearest of the four real depth samples. Existing
single-sample fog, particles and SSAO therefore cannot leak through a covered armor edge, but
can over-occlude the background within a mixed-coverage native pixel. This is a conservative
one-pixel spatial approximation, **not** per-sample volumetric integration. Transparent glass,
optical entrance panels and hover outlines still rasterize once per native pixel. Material
minification/specular aliasing and sub-four-sample thin features remain possible. No claim of
complete temporal stability follows from MSAA alone.

Gates: `tests/native_geometry_aa.rs` validates the actual resolve WGSL and includes an explicit
GPU coverage/depth regression; `stylized_raster` validates the unchanged composed material
shader with centroid interpolation. A deterministic native800 moving-camera Bayer8/continuous
pair, plus geometry-AA off/on captures, is required for subjective acceptance. Implementation
saved; CPU/GPU and native visual results are pending at this checkpoint.


## Cannon source and observer microphone revision, 2026-10-01

The shared procedural cannon voice now uses compact pressure-front and low/upper-bass
impulses plus a short crack and dark gas tail. These are authored timbres, not measured muzzle
pressure or a fluid/acoustic simulation. Live enemy reports and generated own-gun fallback
voices share the source; verified own-gun recordings still take priority where installed.

The blizzard recorder now includes actual armor-hit events, and uses the shared source voices,
clear-exterior atmospheric propagation, reproduction curve and one master limiter. Its wind
bed is the gameplay weather voice. It represents an external microphone, without player-ear
trauma, headset or tinnitus. Named capture limits: side-on gun source calibration rather than
measured muzzle directivity toward the camera; clear acoustic paths; incoming strike acoustic
origin at the tank center (within the vehicle envelope); looped engine timbre with observed
speed/range levels at 10 Hz. It is an offline mix, not a capture of the live player's audio sink.
Spectral/onset checks prove signal properties, not listener approval or real-world calibration.

## Graphics and atmosphere integration, 2026-10-01

- Graphics preferences are separate from world truth: internal/window size, shadows, transport
  samples, optic cadence and camera/display reproduction have a versioned local preference file.
  Time/Weather pages explicitly change world conditions; hardware presets do not do so. GPU
  target rebuilds retain scene slots, optics including damaged/blocked flags, lamp registrations,
  particle identities and clocks. The resize regression renders after two target changes.
- Handheld light uses a modeled lens, a warm directional emitter, a 16–28 degree cone and the
  shared geometry-shadow bank (512-pixel cubemap, 8 m range). Shipping Canyon shader backend
  only; unsupported RT views stow it. It is a compact emitter/shadow approximation, not a
  traced reflector or a global night-brightness lift. The night tone map remains desaturated.
- Removed fixed white viewport halo RGB. Bloom spreads the aperture's source RGB, bounded in
  exposed energy, then uses the same camera response. It remains a lens post effect, not GI.
- Cloud density now advects and rolls in world space. The existing delta-Eddington column diffuse
  closure now supplies a source function integrated through the actual 3D cloud density, rather
  than adding light at the cloud-base sphere (which looked planar). CPU/GPU integrate each
  piecewise-constant segment analytically. The column closure is still an approximation of
  multiple scattering, not full 3D multiple scattering; density evolution is prescribed, not CFD.
- Stars are a deterministic illustrative celestial field, transformed by latitude/sidereal time
  and attenuated by the shared atmosphere/cloud optical depth. They are not an astronomical
  catalogue. Dawn and dusk use the existing ephemeris and atmospheric scattering.
- Coupled storm override spans 0–8; gradual temporal fronts/gusts control suspension extinction,
  precipitation and wind. The same wind advects world-space snow banks and cloud density.
  World-medium view/source extinction samples a shared 3D density function. The raster fog's
  environment source is still a representative phase-weighted probe, not full volumetric GI.
  Discrete snow capacity rises to 65,536, preserving old flakes when adding density layers.
- Exterior soot and transient smoke move from opaque/faceted grains to twelve finite, overlapping
  density supports with soft edges, coherent erosion, wind drift and explicit scattering albedo
  (soot 0.08, propellant 0.55). Source light is shared with geometry and shadowed by it. Raster
  smoke integrates extinction at eight points/support and evaluates incident light at the density
  centroid; full per-sample source integration and soot self-shadowing remain limitations.
  Nearest three sustained emitters use nine supports, leaving three for transient smoke. Cabin
  soot retains its mesh-clipped sparse particles, with rounded silhouettes; cavity-constrained
  continuous density is not implemented by this change.
- Near-miss, blast-skirt and collapse dust now use the same soft participating-medium supports;
  their retained legacy box slots stay hidden. Radius, lift and dilution are prescribed envelopes,
  with advection from the shared weather wind, not a fluid/granular solver. Snow/dust albedos are
  0.9/0.65. Nearest dust uses supports left after fire and weapon smoke; the twelve-support bound
  can omit dust in a crowded event. Real solid blast/masonry debris remains geometry.
- Requested effect palette uses yellow/cream cores and orange/amber edges, with flame-source
  RGB updated from the same cells. Tracers read real projectile states. Wider spark trajectories
  are bounded cosmetic ejecta; they do not define penetration, spall damage or collision. The
  staged six-second review is an effects comparison beside the source Tiger, not combat evidence.
- Existing seasonal grove choices include both pine variants alongside bare/broken trees, retaining
  counts and placement exclusions. This changes authored scenery, identically for both presets.

Details and controls: [GRAPHICS_WEATHER_20261001.md](GRAPHICS_WEATHER_20261001.md).
AI recovery and evaluation boundary: [AI_RECOVERY_20261001.md](AI_RECOVERY_20261001.md).

## September 2026 audit corrections

The fresh implementation/evidence receipt is [ASTRA_NEXT_STAGE_2026-09-05.md](ASTRA_NEXT_STAGE_2026-09-05.md),
with linked acoustic, asset/story and gameplay audits. The rows below retain earlier debt descriptions;
use those fresh traces to distinguish already integrated work from still-open gaps.

- Required atlas placement was accidentally inside a debug-only assertion. Release builds omitted
  the first rectangle on each new regular page. Placement now executes in every build; the original
  seven failing surface/BVH tests and a new multipage uploaded-byte regression pass in release.
- GPU profiling identifies repeated physical snow/atmosphere work as the exterior bottleneck.
  Visibility-first zero-source exits and depth-gated sky preserve source/sample definitions. A
  conservative snow-cell bound passed parity checks but produced no meaningful speed gain, so it
  was removed; its experiment receipt is retained.
- Impact stereo now uses actual armor contact and the current listening camera, including turret
  rotation; geometry/channel-energy tests pass. This is not validation of Tiger pressure acoustics.
- The eleven preselected Pizza layers now load from verified owner-private sources. The authored
  spatial-emitter gameplay bridge and full Pizza sound foundation are still missing.
- Hit mix, 2026-09-28 ([IMPACT_SOUND_20260928.md](IMPACT_SOUND_20260928.md)): **REMOVED** the
  pre-hit contrast (the world bed dropping 10–26 dB in the 0.2 s before a predicted strike on the
  player's armor, with its read-only strike probe and the own-shot hold that suspended it). It had
  no physical cause — a supersonic round gives no acoustic warning — and the owner dropped it on
  28 Sep 2026; the calibrated ear model's loss after the hit replaces it. No pre-hit ducking
  remains in any live or offline listener path.

## Canyon v2 review, 2026-09-30

### Wreck fire correction, 2026-10-01

- The latest Canyon films and launcher explicitly use `KESSEL_RENDERER=shader`. This is a raster
  renderer with physically based material response and a physical environment probe, **not full
  path tracing or solved combustion**. Selecting the High/4090 preset does not select a backend.
- Removed the independently calibrated, unshadowed wreck source (`48 * 0.55 * flicker * strength`).
  Exterior flame cells now supply their own sRGB-decoded radiance, mesh face area, aggregate RGB
  intensity, centroid and support radius. Only the same eight admitted world-fire sources can
  provide light; the pool clears each frame. Smoke-only stovepipes do not acquire flame light.
- The engine-deck attachment is intersected against the loaded hull triangles, at an authored
  rearward longitudinal position, instead of starting 0.38 m above ground under the hull.
  Exterior flames use up to 768 candidates of the same 80–115 mm cubes, three rising/bending
  tongues and deterministic lattice occupancy. Duplicate coincident cells are removed before
  both rendering and energy aggregation. Cabin flame generation and practical lamps are unchanged.
- The existing raster shadow bank now reserves eight additional slots for these world fires,
  separate from the eight cabin fixture slots. The actual hull, terrain and nearby walls cast
  into 512-square cubemap faces within 8 m; surface and local-smoke shaders share that visibility.
  This prevents the old direct light through the wreck and nearby walls. Two cache banks use
  192 MiB rather than 96 MiB in the raster backend. Inactive/unadmitted lights do not render maps.
- Remaining approximation: authored flame radiance and motion, normalized fuel/damage law,
  sum of cube-face energy reduced to one isotropic source per fire, and centroid shadow sampling
  rather than flame-volume or finite-area transport. Cube-face self-occlusion and solved smoke
  extinction inside a flame are absent. Raster blockers beyond the 8 m source map remain outside
  its visibility estimate; there is no new artificial attenuation cutoff. Other non-fixture,
  non-wreck transient raster lights still retain their older unshadowed paths. This change does
  not establish global illumination or parity with the hardware ray-query backend.
- Evidence and review commands: [WRECK_FIRE_20261001.md](WRECK_FIRE_20261001.md).

- Runtime tread compaction (2026-10-01) no longer uses 2.10 m diameter circular stamps.
  Each rig's two oriented contact rectangles are measured from its rendered hull vertices within
  0.05 m of the lowest surface, preserving source width, spacing, and forward/rear extent. This is
  the envelope of the low tread surfaces, not per-shoe contact or simulated snow compaction.
  An R8 sparse atlas retains 12.5 cm cells on every act: 256 persistent 64 m-square pages in
  64 MiB, with a 256 KiB page table covering +/-8192 m. Pages never evict older trails; once full,
  previously untouched pages receive no further material history. Raster cell-center coverage has
  at most 6.25 cm edge error along a world axis (8.84 cm diagonally); nearest-cell lookup adds no
  out-of-band filter halo. This changes terrain material reflectance, not collision or terrain
  geometry. The shader renderer samples this same atlas as the legacy forward/deferred paths;
  those legacy paths retain their preexisting shading-normal perturbation. Source hull width and
  rotated/page-boundary area regressions are `renderer::track_ruts::tests`.

- The separate Gaea v2 uses source `SatMap_Out.png` for rock albedo and `snow.png` for coverage.
  The RGBA packing reserves alpha 32–223 for coverage, decoded separately from RGB sRGB. Snow
  also depends on surface slope; this is a material blend, not simulated snow deposition.
- Generator version 5 reworks prior-traffic and disturbed-yard albedo / snow coverage in a derived
  texture, while retaining both native Gaea inputs. At 4096 texels over 5000 m this has 1.22 m
  spacing; narrow track bands use fractional texel coverage. It adds no geometric ruts or runtime
  traffic simulation. The modern Bradley/T-90 reference guides appearance only, not WWII damage
  or historical-event claims; see `research/CANYON_FOOTAGE_DIRECTION_20260930.md`. Version 5
  adds broken material borders and slope-constrained exposed-bank patches. The shared terrain
  shader resolves the mixed cover using area-filtered source snow/soil textures; this is material
  variation, not a height map, fabricated normal, rut or snow-transport solver. Pure snow and rock
  retain their authored material identity. Native terrain geometry and collision are unchanged.
- The metre-wide nested red/pink aftermath contours are removed. Version 5 uses 6–32 mm-wide
  closed cuboids, 3–10 mm tall, slightly embedded in the shared terrain. 49,983 drops are grouped
  irregularly beside 753 existing anonymous figures. These static deposits follow the visual
  vocabulary of live cubic blood grains; they are not output from the wound or blood-fluid solver.
  No injury event, victim identity or measured blood volume is asserted. Fallen log orientation derives from the source mesh's principal axis and
  rests at its measured minimum; the original purchased mesh and materials are preserved.
- Requested ambient occlusion is the existing **screen-space, ambient-only** depth pass, now
  64 samples with 0.60 m interior / 2.20 m exterior radii. It is not GI and cannot see off-screen
  occluders. Sun visibility retains two geometry shadow maps: a fine nearby map and a 480 m
  far map (540 m crew union), with 1,400 m light-space depth for canyon occluders. Beyond the
  cascades there is no new terrain-shadow solution. The existing local sky-visibility atlas remains.
- Portal bloom is lens-space scatter from the aperture/radiance mask, now a contiguous normalized
  33-tap Gaussian per axis. It adds no world illumination. Practical cube emitters are smaller;
  visible radiance and emitted flux still derive from the same finite surface and spectrum.
  Visual acceptance remains incomplete: ordinary training-night views and the controlled sun
  probe clip practical/reflected highlights. Optical relay preparation did not resolve the probe.
  The Canyon reload frames are darker; they do not validate all eye-adaptation conditions.
- Canyon v2 winter ground cover now follows 91 authored surviving pockets, with road/yard
  exclusion. Only 0.1735% of the entire 25 km² source map permits grass above a 25% mask value;
  this is an authoring statistic, not a measured historical vegetation fraction. The authored mask
  is the clearing authority; fallback noise varies density within it without deleting entire
  stands. Worlds without such a mask retain the existing world-fixed 36 m fallback patches.
  Close stems are approximately 6 mm wide before their small instance variation; coarse LODs
  widen representative stems to 13 / 24 mm. Dry-straw albedo replaces the dark olive palette.
  Distant stems are a geometry LOD, not a measured botanical population. Grass remains primary
  raster geometry without secondary visibility / self shadow; worst-case budget is under 2 M
  triangles and is the same geometry rule across graphics presets.
  Explicit screenshot survey cameras now refresh this ordinary camera-local population after
  moving away from the Tiger; earlier free-camera stills did not reliably show local vegetation.
- Canyon rocks, breastworks and wrecked equipment use the existing source-triangle occupancy
  collision approximation (0.5 m local cells, capped adaptive grid). Cell dilation is bounded by
  the cell diagonal. V2 runs are limited to two cells; at unit-scale yaw placements every world proxy
  diagonal is below 1.62 m (regression over all six hard prop meshes and four rotations).
  This is a conservative error bound, not exact triangle contact. Vehicles still have longitudinal speed and XZ OBB contact,
  not a lateral/angular rigid-body solver. Contact now removes closing velocity with an inelastic
  mass-weighted impulse; it does not bounce, repeatedly damp both speeds or integrate gravity twice.
- Incoming shell pass timbre retains the computed N-wave and adds a synthesized receding wake.
  Arrival/miss distance/speed are the authoritative projectile's actual clipped segment; outgoing
  shots cannot sound ahead of causal arrival. The wake is an audible approximation, not a measured
  aeroacoustic spectrum. Its energy enters existing calibration and armor/headset/LOS filtering.
  Spatial bearing is sampled at arrival, not continuously moving along the tail.
- Every small-arms armor contact now uses the existing delayed exterior sound path or immediate
  own-hull structural path; the previous one-in-N exterior throttle is removed. Cannon spark
  presentation uses the struck armor normal. This does **not** yet continue the cannon after a
  ricochet: stopped/deflected terminal separation and dissipated outgoing flight remain pending.
- Casing extraction and cartridge ramming use distinct heavy-mechanism synthesis through the
  ordinary calibrated mixer; short lift/drop contacts retain their existing recordings. The
  assisted ram now emits its seating sound as well as the breech closure.
- Continuous burning audio is driven by actual cabin patches, burning wrecks, dead allied hulls
  and archived burning hulks. It reuses the positioned-loop engine (eight loudest fires, 60 ms
  fade, stereo pan and one-pole propagation filtering); source strength is an unsourced 74 dBA
  at 1 m per unit fire intensity. Cabin patches are aggregated at their intensity-weighted centroid.
  This is an authored combustion voice, not measured thermodynamic acoustics; ignition wavefront
  delay is not modeled for this continuous bed. Pause, KIA, extinguishing and mixer capture are tested.
- Environmental fixture radiance and source power retain their 0.25 scaling. The requested
  pre-Canyon generation-held cubic flames are restored with 6–8 mm cabin grains and 80–115 mm
  exterior grains; exterior count rises from 64 to 256 per selected source. The old flame radiance
  table is restored. Interior aggregate power still comes from the actual visible cube face area
  and radiance. The independent exterior 0.55 calibration described by that earlier review was
  removed by the 2026-10-01 correction above; source clustering remains an approximation.
- Heat shimmer is an explicit **debug-only screen-space prototype**, gated by
  `KESSEL_DEBUG_HEAT_HAZE=1` in a build with debug assertions. Both shipping presets keep it off,
  including when the environment variable is set. It samples up to 16 active fire-patch/wreck
  plumes, advects a prescribed curl field, bounds refraction to two native pixels, rejects visible
  foreground depth, and adds no radiance. Cabin plume height stops at sampled source-mesh
  contacts. This is not solved temperature, gas density or refractive light transport; it cannot
  reconstruct off-screen imagery. The fire A/B has identical authoritative final states.
- Terminal replay now uses the earlier rear-turret witness eye for **every visible phase**, including
  the initial fade. It never chooses an exterior, optic or driver camera from the player's last
  station. The turret-local eye is (-0.45, 2.34, -0.84), looking toward (0, 1.45, 0.18), at 80 degrees.
  The actual fatal crew, fragment/blast and blood snapshot supplies 3.2 seconds of presentation
  aftermath across eight screen seconds; no second hit or gameplay update is applied. The live
  audio tail fades, slow motion is silent, then black holds. Missing records show black rather
  than inventing a fatal hit. Window, mechanic recorder and Canyon recorder share this policy.
  The fixture's short explosion still clips highlights; later dark aftermath reveals the cabin.
- Isolated Gaea storm workshops compare fog 0.0046, cloud cover 0.98 and storm 0.58 against the
  default 0.0022 / 0.82 / 0.07. Morning and low-sun combat recordings use ordinary eye adaptation
  and existing emitters; no fixed exposure, global darkening grade or extra ambient fill is added.
  The 14:15 UTC test loses terrain visibility between shots and is rejected as a gameplay target.
  These weather comparisons are not the default v2 weather. See `CANYON_STORM_20260930.md`.
- Additional anonymous casualties and ruined field sites are an inferred fictional course under
  documented recurring winter battlefield conditions, not a named event or identified victims.
  The generator records provenance and source hashes. The original v1 workshop is preserved.

## Renderer

Current playability correction: stationary specular/glass accumulation now requires an observed
light change before a coherent sample fluctuation can reset its history. Changed eye/object motion
still allows view-dependent reflections to respond; ID/depth rejection remains unconditional.
This fixes a history-invalidation asymmetry without clipping samples or changing surface albedo.
The source and numeric regression are implemented; the current change still needs compilation and
same-camera GPU/video comparison before it can be called a visible sparkle fix.

The opt-in `gameplay` transport profile additionally uses four shared fixed sky quadrature samples,
Sun-center direct lighting and a fixed optical-medium quadrature pattern at traced optical
continuations. Refraction, reflection, terminal source materials and source visibility still use the
same scene; no optical image blur or brightness floor is introduced. This is an explicitly biased
finite lighting approximation for stable play, authorized by the current pixel-art direction. Full
transport remains the default and retains its stochastic estimators. Both composed profile paths
have regression coverage; compilation, visual stability, aperture coverage and performance still
need the current GPU comparison. The historical transport-law parity requirement below describes
Full, not this deliberately simplified opt-in gameplay profile.

| System | Live truth | Remaining debt / release gate |
|---|---|---|
| High surface lighting | High is raster-primary, then reconstructs exact committed secondary triangles, barycentrics, transforms, level-zero source texels and material state from the hardware TLAS. One diffuse/GGX secondary bounce receives shadowed source and shared-environment lighting. The exact path has compiled and rendered on Hal/RTX 4090. | This is not a full camera path tracer. P0: replace the fixed one-bounce branch with a shared bounded-depth path loop, robust geometric-normal ray origins, appropriate texture filtering and declared convergence/error gates. |
| Indirect GI and reflections | A material-driven stochastic secondary bounce and environment miss are live; there is no screen-space color gather, ambient floor, camera fill, campaign exposure lift or aperture light. A secondary ray that meets direct-view Schutzglas follows the same bounded entry/exit dielectric path as a camera ray. | P0: deeper interreflection and unbounded dielectric path depth remain incomplete. Presets may change samples/cadence/resolution, not transport law. |
| Volumetrics | High traces source visibility against the scene TLAS and integrates Beer–Lambert extinction plus normalized Henyey–Greenstein single scattering for compact local media. Visible sky, surface misses and volume source terms use the same physical environment query. The finite-disk/environment/emitter Monte Carlo estimate lands in a dedicated premultiplied target and is converged with per-view reverse reprojection plus a same-surface/opacity-aware spatial estimate; only effective source radiance is filtered, while current Beer–Lambert opacity (including tiny world-space cabin motes) is preserved exactly. Act-I blowing snow is a bounded -128..128 m effective bulk layer with neutral 0.995 albedo and 0.76 HG endpoint; its blue cast can come only from physical incident sky radiance. Horizontal `sigma_t` remains 0.0068 m^-1. High primary and exact-secondary surface endpoints retain directional Beer transmission and add only the scattered remainder from a constant-cost delta-Eddington hemispheric closure using the actual receiver column, albedo and asymmetry. The finite Sun remains a disjoint directional beam. High's world-volume estimator additionally samples orders 2–5 with exponential free flights, HG continuation, finite-Sun/environment/emitter NEE, exact slab exits, TLAS segment termination and Russian roulette; the uncollided camera term remains analytic Beer–Lambert. A ray refracted through direct-view glass continues through the same host-supplied slab. | The surface closure is plane-parallel flux transport, not full 3D angular transport; local-obstacle/ground coupling, positioned-emitter scattering and snow/atmosphere coupling remain unresolved. The spatiotemporal volume filter is a finite-resolution denoiser, not additional transport. P0: higher volume orders stop at five; binary volume TLAS hits terminate without evaluating their material, so ground/steel reflected or emitted radiance is absent. Overlapping local media and resolved aggregates are still composed as separate layers. Laptop contributes no higher orders until exact software-triangle visibility exists. Never replace missing transport with an authored tint, white sky, cone, halo or brightness floor. |
| Atmosphere / environment | One shared numerical spherical atmosphere integrates Rayleigh, aerosol/Mie, ozone, liquid-water extinction, a finite solar disk and Beer-Lambert direct/single scattering for visible sky, surface paths and volumes. Below a real cloud-base crossing, a measured structured liquid-water column now uses delta-Eddington direct and diffuse two-stream solves. True uncollided solar radiance remains separate; upper-atmosphere Rayleigh/Mie/ozone diffuse flux supplies the physical faint-blue component. Live cloud coverage is the campaign `overcast` state in both presets. | P0: the local plane-parallel hemispheric flux closure is not a solved atmospheric medium in exact 3D/multistream form; side leakage, broken-cloud angular structure, the angular structure of Rayleigh/aerosol multiple scattering (an isotropic downward two-stream field since 2026-11-02) and ground contribution remain. Validate against DISORT/high-sample references and Hal before widening the claim. No authored gradient, gray overcast, circumsolar curve or brightness floor is permitted. |
| Snow / spindrift | A declared bounded fine-grain suspension supplies whiteout extinction/scattering and receives the physically spectral cloud multiple-scattered environment through its directional estimator. At surface endpoints, exact directional Beer energy is supplemented only by the delta-Eddington scattered flux after subtracting a fixed quadrature of the analytic `2 E3(tau)` hemispheric uncollided term; the live `sigma_t`, slab geometry, neutral albedo and HG asymmetry drive the solve. High samples one temporally accumulated world-volume path per pixel: an exponential camera collision followed by HG sourceward continuation resolves snow-to-snow orders 2–5 with neutral albedo, NEE and roulette, while exact Beer–Lambert still owns uncollided opacity. High additionally resolves a 32 m large-aggregate population from immutable Lagrangian world cells: finite-shutter ray-cone filtering conserves each crystal's cross-section, scene depth clips the camera segment, and exact triangle rays gate its finite-Sun/environment sources. The premultiplied result enters HDR before bloom/exposure/crush. `snow.wgsl` contains no white-alpha particle, camera shell, veil, or depth-derived ground ribbon. | P0: the endpoint closure lacks exact 3D angular redistribution, geometry coupling and ground return. Volume orders above five, material response at opaque continuation hits, ground-reflected incident radiance, turbulent trajectories, accumulation/resuspension and coupling between resolved aggregates and the effective bulk remain unresolved. Laptop keeps the bulk suspension's extinction/order one but omits higher-order paths and resolved grains until exact software-triangle visibility exists; it may not restore a sprite/ribbon proxy. |
| Compact emitters | Surface and volume consumers share source position, physical source radius, RGB radiance scale, inverse-square falloff and visibility. Apertures are not emitters. Dynamic incandescent penetration chips contribute at most one shadowed aggregate per moving hull/turret frame: total RGB power is summed from every live cube's six-face area and current heat page, position is the emitted-power centroid, and the spherical source support encloses every contributing cube. | The fragment emitter preserves total authored RGB power and has an explicit source-support bound, but clustering loses the real near-field angular distribution. Replace it with direct dynamic emissive-triangle sampling; convert remaining hand-tuned source powers to one documented radiometric calibration. |
| Materials / emission | Base-color textures use sRGB decoding without runtime gamma/readability lifts. Crew, blood, loot, maps, foam, tools, brass, scars and wreck steel are not emitters. High/Vulkan direct-view Tiger Schutzglas now retains its real raster entry surface, finds the second interface in the exact triangle TLAS, applies Snell refraction at IOR 1.52, exact unpolarized Fresnel reflection/transmission and Beer–Lambert absorption over the traced thickness, then traces the exterior surface/environment and bounded world medium. It does not sample the screen or an RTT. | P0: archive-validate the provisional smooth-glass IOR/absorption coefficients; couple refracted glass paths into source-side illumination of the compartment; add rough dielectric scattering and internal multi-reflection; implement the full periscope optical path. Hot FX still use an authored orange mode instead of temperature/spectral emissive state. Current render-to-texture periscope is forbidden shipping debt. |
| RT scene completeness | Ordinary opaque meshes and admitted instanced pages enter the exact hit pack and triangle TLAS. TLAS capacity (131,072 instances since 2026-09-29, 65,536 before; the device must offer it, Vulkan reports 2^24) is explicit since 2026-09-28: the build prints the demand (scene-mesh slots plus live, visible instances) and refuses a scene that exceeds it; a frame whose layout exceeds it logs an error naming how many instances are missing from the ray scene and counts the frame (`Renderer::tlas_overflow_stats`); hidden static instances take no slot. | P0: view-local crew and several dynamic penetration/fire grains are currently marked raster-only, so they cannot occlude, reflect, receive GI, or return emission. Replace omission with per-view TLAS masks and current dynamic transforms before calling the transport scene-complete. |
| Laptop | Laptop allows no ray queries, so both backends take the forward raster path and, since 2026-09-29, the same shaders: surfaces and media are lit by the raster environment (next row), the key body's beam through the source-geometry sun shadow cascade, and the positioned emitters. The former forward `mesh.wgsl` lighting (a constant sun 1.74 and moon 0.21 on the physical backend without ray queries, research 7.8 item 4) no longer lights any frame. Measured headless on the urban intro (1943-01-15T12:27Z, camera 1846 7.9 262): the physical and shader backends on Laptop give the same frame; against High, HDR mean 193-197 against 165 cd/m^2, log-mean 161-163 against 127. | Integrate the exact software triangle BVH. Laptop may reduce resolution/rays/history length, never change emitters, occluders, materials or transport law; today Laptop and High still differ in transport (High traces surfaces and media, Laptop takes the raster environment's named approximations), measured above and in the backend-agreement regression. |
| Raster environment (shader backend; physical backend without ray queries), 2026-09-29 | No palette lights a raster frame. Every frame a compute pass (`environment_probe.rs`, `shaders/environment_probe.wgsl`) evaluates the physical atmosphere, the same `physical_environment_transport` the ray-traced path and the visible sky query, at the camera: the radiance of each of the scene-geometry visibility atlas's 32 directions (24 sky, 8 ground; mean of 16 samples over the direction's cone), the key body's beam through the atmosphere and the bounded world medium (combine.wgsl's directional Beer term and delta-Eddington scattered flux), and the diffuse horizontal irradiance. Ground directions carry the act terrain's hemispheric albedo (winter snow 0.91/0.93/0.95, spring canopy, as `terrain_material.wgsl`) times the downwelling irradiance over pi. Surfaces (`stylized_mesh.wgsl`): Lambert albedo/pi on the (1 - F) share, irradiance from a normalised cosine quadrature of the 32 directions through the atlas visibility (the since-2026-09-25 atlas: 384 px orthographic depth tiles of the real scene geometry), glossy reflection from the lobe-averaged radiance about the mirror direction with the Karis environment-BRDF fit, the key beam through the sun cascade, positioned emitters by I / max(d^2, r^2) with no cap, emission by gbuf.wgsl's formulas. Media (`stylized_smoke.wgsl`): the world medium's source is its single-scattering albedo times the Henyey-Greenstein-weighted probe environment plus the beam through the phase, with combine.wgsl's optics (tested identical); gun smoke, breech gas and cabin haze the same plus every positioned emitter, each hemisphere scaled by the atlas share that reaches the medium. The visible sky is `sky.wgsl` on every backend. Removed: `stylized_sky.wgsl`, `solar_daylight.wgsl` (CIE overcast fit, Kasten-Young air mass with Meinel transmittance, the 31 deg normalisation), the stylized sky/ground/sun and night palettes, the x12 point-light cap, and (2026-09-29, after the merge) the shader backend's x20 on the cabin's electrical fixtures (`interior_rig::SHADER_CABIN_PRACTICAL_GAIN`, a calibration for its old fixed exposure): with both backends in one unit system it multiplied the dome lamp, the TPZ lamp, the four annunciators and the breech ready lamp a second time, so the gunner's cabin blew out in amber and green and the ready lamp's unshadowed green reached the turret roof's edges and hinges from inside; the fixtures now emit the same power on both backends. Measured headless (urban intro, 1943-01-15T12:27Z, camera 1846 7.9 262, fog 0.0035): shader-backend HDR mean 193-197 cd/m^2 against High's 165 (log-mean 161-163 against 127, displayed mean 0.40 against 0.47); before the probe the two backends differed 68x on this view. By image region against High: near snow 1.00-1.07, sky and fog 1.04-1.21, distant buildings up to 1.95. Courtyard wall camera (-40 -3.5 33, noon): 754-771 against 632 cd/m^2. | Named approximations: (1) the environment and the beam are evaluated once at the camera, not at each receiver; (2) each direction is a 16-sample cone mean; (3) the ground directions are the local terrain's albedo times the irradiance over pi, without aerial perspective or the scene's own buildings; (4) the atlas footprint is bounded (+-4.5 m vehicle, +-12 m on foot, 10 m reach): beyond it the environment counts as open, which is why distant buildings read up to 1.95x High; (5) 32 directions, no interreflection; (6) positioned emitters are unshadowed except the eight registered cabin fixtures (512-square cube shadow maps, 5 mm near plane, 8 m reach, exact per-fixture self-emitter exclusion); local smoke shares their visibility at its optical-depth centroid. Other positioned lamps can still light through walls; (7) the world fog is not lit by positioned emitters (High's fog glows around lamps at night: moonlit urban camera 1942-12-24T23:00Z, HDR mean 3.6 High against 5.6-5.8 raster, the raster sky black and the lamp-lit snow brighter); (8) the medium's beam visibility is the cos^8 lobe of the sky directions, not the sun cascade. Replace with traced visibility when the backend gains ray queries; gate: `the backend-agreement regression` (below) and `tests/stylized_raster.rs`. |
| Winter snow on steep faces (2026-09-29, `shaders/terrain_material.wgsl`, both backends) | The terrain material's winter snow cover now thins on steep faces: from 45.6° (surface normal y 0.70) to 66.4° (0.40) the albedo moves to an exposed loess/clay reflectance (0.31, 0.27, 0.22 linear, 85 % at full slope), because dry snow sloughs off faces much steeper than its repose. It is one material rule in the shared shader, so both presets and both backends draw the same surface; the lighting of the face is unchanged. | Named simplifications: (1) snow cover is a function of the drawn triangle's slope, not a snow accumulation, wind or avalanche model; the 45-66° ramp and the earth reflectance are estimates [I] (snow repose, loess reflectance about 0.2-0.3), not measurements; (2) on the 3 m grid a cliff crossing the grid obliquely still alternates steeper and gentler triangles along its rim, drawn as a faint saw pattern (the canyon tool eases its walls over about a cell to reduce it); (3) `environment_probe`'s hemispheric terrain albedo still assumes full snow, which overstates the bounce inside the canyon by the walls' share of the view. Gate: the canyon captures (`captures/canyon_*_20260929.png`) and the unit suite's shader-inclusion checks. |
| **Post (reproduction transform, not physics)** | The crewman's eye (row in "Light, sky and the eye" below) sets the exposure; the ACES fit (Narkowicz) is the display's one tone curve; optional PS1 quantization/dither and the nearest upscale follow. They alter the recorded image after transport and add no light. The prior contrast S-curve and 2.2% black-point clip are removed, so dim transported radiance is preserved while zero stays black. | P0: optic masks remain debt until physical eye-box geometry replaces them. Camera response must not add light, clamp a readability floor or erase valid low radiance; all live WGSL camera code must parse/validate in tests. |

## Canyon Run, fire and tools (2026-09-30)

The existing Gaea canyon is a winter placeholder, not the unbuilt winter campaign map. The new
generator preserves the 16-bit source and its 5,000 m / 2,500 m scales; the runtime uses a
4.883 m grid with declared building pads/craters. Route finding checks both drawn triangles,
buffered for the tracks, and verifies the 8 m-wide travel corridor every 2 m. Stable terrain and
landmarks support seeded selection among physically probed gun sites, supported patrol knots
and building-side rifle posts. The generator receipt records the measured grades and source hash.

Winter T-34 movement now sweeps the **resolved** translation, including inertia/contact pushes,
through the existing source-derived static-cover queries at no more than 0.25 m intervals. Its
front/middle/rear footprint checks reuse the existing conservative hull/source boxes and terrain
clearance. This closes translation tunnelling; it is not the Tiger's full track-support, tipping
or deformable-rubble solver. Rotation-only contact and the proxy error bounds already recorded
below remain debt. Riflemen now use the existing static-cover, building and door queries after
both legacy and shared-combat walking steps, rejecting terrain height jumps above 0.45 m.
The rendered body and combat actor share the resolved position. Canyon omits the unrelated
allied practice squad at scene construction; Soviet shared-combat policies remain active. Tank escorts currently
walk beside their tank; no mounted-rider animation or suspension is claimed.

Fire presentation replaces cubic flames with deterministic, continuously advected, tapered
20-face parcels. Their triangular mesh area, current scale and heat page supply the existing
finite aggregate emitter's power and support. This is an improvement to an existing procedural
particle representation, **not** a new fluid, fuel-consumption or thermochemical simulation.
The existing authored heat colours, finite clustering and raster-only dynamic-particle omissions
remain explicit debt. No cabin fill, glow floor or preset-specific source multiplier was added.

Foam now uses 12 mm grains at 4,800/s (formerly 8 mm at 2,400/s), and 8,192 airborne slots.
Surface contacts, adhesion and coverage still determine suppression; grains remain non-emissive.
Fire growth/spread constants were increased at the owner's request. Grain size, foam delivery
and spread coefficients are gameplay discretization parameters, not calibrated discharge mass
flow or measured fuel chemistry. The aimed suppression fixture and unattended-fire fixture use
the same ignition source, preserving the distinction between coating a surface and cooling
already burning ammunition.

The hammer's hand/wrist arc now interpolates continuously through windup, contact, recoil and
rest, aligned with the existing 0.24 s contact event. Repair credit and sounds still require the
real eligible target. The animation does not model arm dynamics or tool impulse. The loader's
spawn moved 0.25 m inward to start inside the existing floor bound, avoiding the old first-step
snap. Interior walking still uses the existing floor/breech approximation; full body IK and
collision-constrained station transitions remain unimplemented.

Gates: `gaea_canyon_*` ignored GPU tests, the ordinary unit suite, the generator's travel-surface
validation, and player-view captures listed in `docs/CANYON_RUN_20260930.md`.

## Light, sky and the eye (2026-09-29)

Research: `docs/research/LIGHT_NIGHT_ADAPTATION_20260929.md` (the exposure-foundation brief; sections cited as
"research n.m"). The sky is lit from an ephemeris of the act's date, time and place; the eye that
looks at it is a reproduction transform, named as such, like the audio chain's tone map.

| System | Live truth | Remaining debt / release gate |
|---|---|---|
| Sky from an ephemeris (`ephemeris`, `celestial_sky`) | The Sun and the Moon come from Meeus: the Sun by chapter 25, the Moon by chapter 47 (the full 60 + 60 term series), four-term nutation, apparent sidereal time (ch. 12), topocentric parallax (ch. 40), the phase (ch. 48), Espenak-Meeus Delta T. Tests: Meeus examples 25.a and 47.a; Espenak's phases November 1942 - February 1943 within 2 min; 15:00 local apparent time on 21 Dec 1942 at Stalingrad (48.708 N, 44.513 E) puts the Sun at 7.41 deg, compass 220.9 deg. Each act has a place (Stalingrad; the Oderbruch and Halbe for the homeland acts) and a clock (`SkyClock`) that runs with simulated time; the named phases (Dawn, Day, Dusk, Night) are instants of it: noon, the Sun 3 deg rising or setting, and the act's historical night (1942-12-24T23:00Z, 1945-04-16T01:00Z, 1945-04-28T22:00Z). An authored lighting record (version 2) carries a UTC instant and the weather, never a sun direction or an exposure; a version-1 record's sun and exposure are ignored with a warning. The workshops were migrated by `tools/apply_ephemeris_lighting_20260929.py` to the instants their authored sun heights stood for (courtyard, "15:00 during Wintergewitter": 1942-12-21T12:00Z; urban intro 1943-01-15T12:27Z; staged encounter 1943-01-26T09:15Z; the ruined-house courtyard's 48 deg sun cannot stand over Stalingrad in winter, so noon on the solstice, 17.9 deg). The editor steps the clock by 10 min, 1 h and 1 day. Removed: the forced courtyard sun, the DayPhase sun and moon directions (93 deg from where the 15:00 Sun stood, research 3.6), the Night phase's Sun at 20 % of its irradiance (`night` 0.80, `solar_irradiance *= 1 - night`), the night fog multiplier (x1.10). | Named approximations: (1) one directional source carries the beam and the shadows: the Sun above -1 deg, otherwise the Moon when it is up; (2) while the Moon keys, the Sun lights only the air and the cloud (a secondary source without a beam) until it sinks below -21.5 deg, the altitude below which it lights no air seen from within 1 km of the datum; (3) while the Sun keys, the Moon is dropped: at most 1.5e-3 of the Sun's horizontal illuminance over the campaign window, tested every 10 min from 19 Nov 1942 to 5 Feb 1943; (4) the key changes discontinuously when the Sun crosses -1 deg; (5) twilight's higher orders are the two-stream field under Chapman's effective cosine (3 November 2026, "The sky through the day"): civil dusk (Sun -6 deg, clear) gives 5.2 lx against USNO 171's 2.99 (research 2.1); (6) no refraction of the bodies' directions, no eclipse. Gate: `ephemeris::tests`, `celestial_sky::tests`, `environment_authoring::tests`, `renderer::tests::named_phases_carry_no_light_and_the_night_has_no_floor`. |
| Moon, night sky and airglow (`physical_environment`) | The Moon is a second finite disc in the same atmosphere: its top-of-atmosphere illuminance is USNO 171's phase law P(E) scaled by the inverse square of its distance, spectrally the Sun's irradiance times the lunar reflectance ratio (1.22/1.00/0.80 at 680/550/440 nm), and it passes through the same Rayleigh, aerosol, ozone and cloud transport as the Sun. The night sky (airglow, zodiacal light and the unresolved stars) is an emitting layer at 90 km: zenith radiance (3.4, 2.7, 2.3)e-9 RU (research 2.4, Benn & Ellison 1998 at sunspot minimum) with van Rhijn brightening, attenuated along the view ray; the cloud boundary receives its hemispheric flux 2 pi L0 (1 - sqrt(1 - k^2)) / k^2. Measured (horizontal illuminance of the model): clear, Sun 7.41 deg: 10,187 lx against USNO 7,202 (cloudless, clean air); under the act's cloud deck 1,195 lx against USNO dark stratus ~720; the Moon on 24 Dec 1942 23:00 UT, clear: 0.102 lx against USNO 0.111; clear and moonless 0.90 mlx against the research's 0.5-0.8 mlx; overcast and moonless 0.17 mlx against 0.03-0.2. A night with no moon and no fires carries no other light: no floor, no night palette. Under cloud the cauldron's fires are a named source (2026-10-28, `KESSEL_FIRE_GLOW_FLUX`, the raymarch list above): about 1 mlx on the snow under the act's deck. | Named approximations: (1) the lunar reflectance ratio is one colour for every phase and libration; (2) the night-sky layer is one smooth emitter: no resolved stars, no Milky Way, zodiacal light folded into the zenith value, no solar-cycle variation; (3) the Moon's disc carries no limb darkening and no maria (its phase terminator and earthshine are drawn since 3 November 2026, "The sky through the day"); (4) earthshine lights the Moon's disc only, not the ground. Gate: `physical_environment::tests`, `celestial_sky::tests`. |
| **Reproduction transform (not physics): the crewman's eye** (`observer`, `shaders/exposure.wgsl`, `shaders/crush.wgsl`) | The fixed camera exposure is gone. On every presented frame of both backends and both presets, one compute pass meters the transported HDR (photopic and scotopic luminance from research 1.2's weights, a log-average of a 2 deg fovea, a 20 deg Gaussian and the field, 0.5/0.3/0.2) and advances the eye by the frame's simulated time: the pupil (Watson & Yellott 2012, age 25; constricting 0.3 s, dilating 2 s plus a 15 s tail), rate-limited pigment regeneration (Mahroo & Lamb 2004: cones 0.5/min, rods 0.045/min) with bleaching threshold rise, neural adaptation (cones 0.1 s up / 1 s down, rods 0.2 s / 3 s), Ferwerda 1996 threshold-versus-intensity curves, the CIE 191 mesopic weight, and flash blindness from the 1968 review's recovery fit (coverage-weighted energy, 180 s memory). The display shows the visibility-matching scale TVI(100 cd/m^2)/TVI(A) at the ACES key 0.18: the cones in colour, the rods as an achromatic signal weighted by (1 - m) that saturates at 1,700 scotopic trolands. The constants are generated from `observer.rs` into the shader, identical in both presets. The eye reads only the HDR and writes only its own state and the crush's exposure lanes. Measured with the CPU statement of the model: hatch closed from a 210 cd/m^2 dusk into a 0.5 cd/m^2 cabin shows the cabin at 1.3e-4 of white after 0.05 s, 0.0016 after 1 s, 0.0072 after 5 s, 0.0177 after 30 s (steady 0.0182); a muzzle flash over moonlit snow takes it from 0.0078 to 0.0007 half a second later and back after 30 s; a moonless scene maps under 2 % of display white. | This is a reproduction transform like ACES and the audio tone map, not physics: it models how an adapted observer would see the scene and cannot add light to it. Named limits: (1) the time course of dark adaptation is late: after a total bleach the rod-cone break comes at 29 min (classical 7-12 min) and the rods reach threshold at 39 min; the rod regeneration rate is chosen so the research's own statement (a full bleach to 10 % in about 30 min) holds; (2) flash blindness is global, not a retinal afterimage map; (3) no veiling glare (research 6.8: an owner decision; it would need its own gate); (4) no acuity loss, colour loss beyond the mesopic weight, or Purkinje shift beyond the scotopic weights; (5) a frame that is never metered (the LAN menu turntable) is shown by an eye adapted to 41 cd/m^2; (6) the meter reads the HDR after the camera's bloom halo. Debug gate: `KESSEL_CAMERA_EXPOSURE` (a fixed exposure without rods) opens only in a debug build on an explicit opt-in. Gate: `observer::tests` (the time courses, monotonic mapping, the moonless 2 % bound), `renderer::tests::the_eye_is_the_display_and_never_touches_transport`, `tests/exposure_clock_abi.rs`, `debug_gates::tests`. |
| HDR precision (2026-10-04) | The HDR frame and the radiance targets that feed it (light layers, accumulation, volume estimate, reconstruction, passthrough) are binary32 (Rgba32Float) wherever the adapter can blend, filter and four-sample-resolve that format (`renderer::choose_hdr_format`); the light, the shaders and both presets are otherwise unchanged. Measured on the steppe under a full Moon: the field read exactly 0 cd/m^2 in binary16 and reads 0.0006-0.0012 cd/m^2 now; a daylight frame's displayed mean is unchanged (0.24318 against 0.24317). | An adapter without those format features keeps binary16 and the old loss: its smallest subnormal is 6e-8 RU (about 0.004 cd/m^2), so a moonlit field stores as zero. `KESSEL_HDR_HALF=1` forces binary16 for comparison. The G-buffer albedo target, the texel-probe targets and the bloom chain stay binary16. The optic layer dump (`raster_optics`) still assumes 8 bytes a texel and needs `KESSEL_HDR_HALF=1`. |
| Twilight (2026-10-04) | `physical_first_planet_hit` in `shaders/physical_environment.wgsl` read the empty interval of a ray that misses the planet as a hit at infinity, so every sample whose ray toward the Sun cleared the planet was in the planet's shadow: all air above (R sin^2 h)/2 under a Sun at altitude h (3.9 km at 2 deg, 300 m at 0.5 deg) and all of it once the Sun set. The CPU reference never had the fault. Corrected: across sunset on the steppe the frame mean now falls 880, 529, 262, 77, 6.8, 0.21, 0.075 cd/m^2 for the Sun at +7.8, +4.8, +2.0, -0.8, -3.5, -6.0, -8.3 deg (before: 879, 487, 86, 0.0001, 0.005, 0.008, 0.07). | Low-Sun daylight is brighter than every earlier capture (the upper air is lit again). Gate: `celestial_sky::tests::the_light_falls_steadily_through_dusk` states the CPU reference's dusk; there is no GPU-against-CPU sky regression yet. |
| Night display gain (owner, 2026-10-04) | `observer::ROD_DISPLAY_GAIN` = 6: `crush.wgsl` shows the eye model's rod signal six times brighter than the threshold-matched level, because a moonlit field reproduced as dim as a dark-adapted eye sees it is too dark to play by on a monitor. It changes no light and no adaptation state, and nothing in photopic light (the rod share is zero there). | A deliberate departure from the eye model's visibility matching: under the Moon a surface can be shown brighter than the same surface late in dusk. `Mapping::aces_input` and the research gates state the model without the gain. `KESSEL_ROD_DISPLAY_GAIN` overrides it for review. Gate: `observer::tests::the_night_display_gain_is_a_named_bounded_departure_applied_only_by_the_display`. |
| Night display hue (2026-10-27) | `observer::ROD_DISPLAY_HUE` = (0.80, 1.00, 1.45): `crush.wgsl` reproduces the eye model's achromatic rod signal in a luminance-neutral cool grey (the mesopic Purkinje appearance: rod sensitivity peaks at 507 nm and the rod signal enters the opponent channels on the blue side; Kirk & O'Brien 2011, Jensen et al. 2000). The cone term keeps every source's own colour, so fire and lamps stay warm; a daylight frame is untouched (rod share zero). | A reproduction choice: moonlit snow is shown blue-grey where the model's rod channel is hueless. `KESSEL_ROD_HUE=r,g,b` overrides it for review. Gate: `observer::tests::the_rod_display_hue_is_luminance_neutral_cool_and_named`. |
| Veiling glare (2026-10-27, raymarch backend) | `stylized_bloom.wgsl` `fs_glare`: the eye's intraocular scatter spreads every bright source over the frame by the CIE general disability glare equation (Vos & van den Berg 1999), L_v = E 5/theta^2, sampled as four octave rings (1.5-12 deg) of eight taps over the Gaussian-blurred bright pass, each ring 2.2 % of the source luminance per octave. `GLARE_SCALE` = 0.5 halves the CIE veil so the sun near the horizon leaves the field playable. | A reproduction model of the observer, not scene light: it adds no illumination to any surface or volume. The raymarch display chain only; `KESSEL_RM_SKIP=4096` disables it for review. |

## Simulation

| System | Live truth | Remaining debt / release gate |
|---|---|---|
| Exterior cannon flight | Authoritative projectile pool with fixed substeps, `ShellSpec` velocity loss/gravity and swept contacts is being integrated; focused moving-target/range/frame-schedule tests pass. | Unified game tests and a playable delayed-impact capture must pass after renderer integration. No trigger-time outcome or forced landing may remain. |
| Armor / terminal ballistics | Armor tables, obliquity, range-dependent penetration and persistent scars exist. A penetration generates one stable-ID fragment population shared by the damage forecast, visible chips and contact audio; entry is no longer clamped to the cabin proxy. Since 2026-11-02 a kinetic round on the player's Tiger is followed layer by layer behind the struck plate through the CAD-measured components (casing, interior, liquids, live rounds, far wall), each with a named law, and lodges where it stops (see "Layered damage model"). | P0: damage forecasts the fragment bodies immediately against frozen targets, while their later visible paths meet moving state. Interior fragments meet the vehicle's own source triangles since 2026-11-02 (no perforation of any surface by a fragment, named; see "Layered damage model"); scars weaken resistance; on the raymarch backend they are drawn as cut and displaced geometry (see "Armor scars, 2026-10-27") but the retained collision triangles are not perforated. The layered chain's laws behind the plate, the component casings and masses, and the stopped-round depth rule are named reduced models, not validated terminal ballistics. See `ARMOR_FRAGMENT_DEVELOPMENT_20260929.md` for the online-contact and shared-geometry migration gates. |
| Vehicle loss | Module/fire states and crew bodies exist. | P0: enemy penetration currently collapses too readily to an instant kill; random turret-pop/cook-off decisions are not derived from ammo/fuel/module energy. Replace with the shared vehicle damage state. |
| Crew / gore | Four co-op roles, persistent bodies/ragdolls, independent blood grains and persistent source-surface film exist. Live blood transport/deposition uses retained source triangles (`surface_blood`); the current presentation multiplier is 8 with an 8,192-grain pool. Since 2026-11-02 a residual penetrator's own path reaches crew through the same anatomical boxes as fragments, with its real mass and speed. | Since 2026-11-03 crew bodies (ragdolls) inside the vehicle meet the visible interior: each of the eleven boxes sweeps its centre and eight corners from its previous pose to its current one against the posed source triangles (`sweep_crew_segment`), is moved back out along the face's normal 1 mm clear and bounces with the former authored coefficients (named: the box's edges and faces between corners are not swept, so a thin rod can pass between two corners of a large box; up to four faces per substep). The former proxy floor, ceiling, basket cylinder and hull box remain only as a tunnelling guard 8 cm outside themselves (`CREW_SEGMENT_ESCAPE_SLACK_M`, named), and the proxy breech box is no longer a contact; without vehicle surfaces (unit tests) the proxies are the compartment as before. Test: `a_body_thrown_at_drawn_steel_stops_at_it_and_rests_on_the_drawn_floor` (50 mm and 3 mm walls). since 2026-11-02 a casualty in solo play is drawn as voxel cells and the strike that killed him is solved on them as a bonded lattice (see "The dead as voxel bodies" under the layered damage model), while the living, the couch co-op renderer and a restored save still draw a part whole or as bare bone; hot steel fragments meet the visible triangle interior since 2026-11-02; grain/film pools can saturate; the instant 0.24 J crew-lethality rule and unconsumed energy through tissue remain authored approximations (the layered damage model did not change them). Blood density is not calibrated fluid volume or extra wound energy. Do not carry the former blood-proxy statement forward as current. |
| World collision/destruction | Tank/tank separation, trees, destructible structures, rubble and drive-over classes work; since 2026-09-29 the Tiger's contacts go through the push model (tank push row) and the manifest's walls, piers, doorways and sheds are destruction records. | P0: AABBs/yawed footprints, hard-coded infantry cover cylinders and several authored structure shapes are proxies. Replace them with source-derived triangle/convex data; no invisible blocker may contradict the rendered object. |
| Campaign outcomes | Route, resources, AI navigation/combat and authoritative telemetry run headlessly. | P0: Act-III allied hits/kills and civilian/town loss counters include scripted scalars. Outcomes must come only from simulated entities/events. |
| First level: campaign Act 1 on the urban district (2026-09-29, `docs/ACT1_FIRST_LEVEL.md`) | Campaign Act 1 plays the urban-intro workshop: the route starts at the district spawn (the level design's entry gate) and leaves by its west exit gate; the district's defenders (the winter session's street encounter: two T-34 platoons on street beats, two 45 mm guns sited by the field-of-fire probe) are fielded at the start and the director raises no route contact until their armour is destroyed or the Tiger has left the district (450 m from the spawn); survivors keep their slots and fight on under the ordinary campaign AI, and route contacts take the free slots. It is a gate on when contacts spawn; no outcome is scripted. | Named limits: (1) the breakout radius is a distance from the spawn, not the district's outline. (2) `campaign-ai-run`, which proves the level headlessly, plays with an omniscient scripted crewman (it knows where unseen crews are; it lays by setting the gunner's desired lay, the turret following at its real traverse rate), and its field track repair takes the kit and is set down at the break instead of walking there; the ritual itself is the player's. (3) `KESSEL_CAMPAIGN_AI_INVINCIBLE=1` is a named debug gate for traversal proofs: it suppresses every mutation of the Tiger by hits. None of these run in play. |
| Fire, smoke and blast | Persistent particle state, fire growth, extinction/scattering and physical light sources exist. | Fire/smoke are procedural particle/medium models, not thermochemical fluid simulation. Their status must remain named as such; visual particles may not decide damage independently. The near-miss, track-crush, blast-skirt and structure-collapse dust (`Game::spawn_dust` into `snow_puffs`) formerly rendered growing opaque boxes, visible again in the 2026-10-01 staged impact clip. Those box slots now remain hidden: `drive_dust_volumes` submits finite, soft noise-density supports to the existing extinction/scattering renderer. Prescribed expansion, dilution and upward drift are cosmetic, with shared weather-wind advection; they are not fluid/granular dynamics. The shared twelve-support limit admits dust only after fire and weapon smoke, nearest first. Solid debris meshes remain separate. |
| Soviet 45 mm anti-tank guns (2026-09-28, `at_guns`) | Emplaced guns fire authoritative BR-240/BR-240P projectiles through the shared flight, armour and exterior-damage path; crews are ordinary soldiers; cannon rounds and close HE wreck the gun. In a street district (2026-09-29, `street_encounter`) the two sites come from a field-of-fire probe on the built district: each candidate's field is measured with the game's own sight test (`los_clear`) and a line-of-fire test against what the authoritative projectile strikes (standing records, story cover, doors, trees, terrain), not with footprints. Sources: `docs/AT_GUNS_20260928.md`. | Named simplifications: code-native low-poly mesh from published dimensions (whitewash assumed); hit box is the shield and carriage only; perception uses the tank crews' weather envelope and a line-of-sight test; hearing reaches 1.5 km for a 45 mm report (0.62× obstructed, as the tanks); doctrine timings (lay, trail shift, aimed interval, cover) and dispersion are tuning; no relocation. The street probe samples its kill zone every 8 m and its weights (flank, range band, line weights) are tuning. Gate: `at_guns::tests` and the ignored GPU ambush test (courtyard and urban workshops). |
| Winter T-34 platoons in a street district (2026-09-29, `winter_encounter_ai::Streets`, `street_nav`) | Leaders, wingmen and searches drive A* routes over a 4 m grid of cells measured on the built district: a cell is drivable on the district's ways (streets, square, old road) when no story cover, standing record or occluder lies within 3 m of it up to the hull's roof and relief across that box is under 1.2 m; the manoeuvre veto keeps the hull's width clear 4 and 8 m ahead. Measured over 900 s of patrol and 240 s of search and fighting after contact: no hull sample inside a building record or story cover. | Named simplification (pre-existing, now stated): enemy T-34 hulls do not collide with building walls or story cover (track crush and wall contact are the player's only); in the streets the routes and the veto are what keep them off, so a hull shoved by another vehicle can pass into a wall. The grid is a discretisation of the route clearance (an obstacle between cell centres can be missed); it is route planning, not collision. P0 with the world-collision row: enemy hull contact with the source geometry. |
| Compiled buildings (2026-09-28, `building_runtime`, `building_set`) | Every building instance (the featured house and each `building_instances` placement of a manifest revision) answers walking, stair traversal, weather shelter, support under the feet, wall collision, infantry cover, container search and light fixtures from its compiled layout (floors, rooms, portals, stairs, slabs, containers) and the kit runtime shapes, in its own frame; damage, support loss and collapse clusters run through its own `BuildingState`, and a placement's condition goes through the same damage path at instantiation. The featured house answers exactly as its former hard-coded queries: regression over 2.4 M shelter points and 1.35 M surface and support samples in every single-module failure state and 24 combinations, and 1.2 M walk steps in every wall, door and stair failure state and 12 combinations. | Named simplifications, each measured: (1) collision, damage and sight proxies are the axis-aligned boxes the kit generates from every asset's source triangles (`building_kit::proxies`, tracked as the generated table `building_kit_shapes.rs`, whose test rebuilds it from the installed meshes), within each asset's measured error (walls 0.022 m, windows 0.053-0.075 m, doorways 0.027 m, stair flights 0.075-0.102 m, the holed slab 0.101 m, roof sheets 0.20-0.22 m; the world error scales with the module's largest scale factor). Since the integration (2026-09-29) this holds for the featured house too; its former hand-declared boxes made a gable roof and a stair flight their bounding boxes (enclosing air up to 2.0 m and 3.0 m), closed the holed slab's stair opening for shells and sight, stood the frame-only window's 0.16 m wall 0.42 m deep, and left 0.3 m of each doorway's visible brick jamb open to feet (aperture taken as 2.2 m where the jambs stand at ±0.8 m); its physical door leaves (`world_doors`) now fill the measured 1.6 m opening instead of reaching 0.3 m into each jamb. The same lintel-for-opening mistake remains for the manifest's `doorway_hr_1` frames (leaves for 1.6 m in a 1.0 m opening between jambs at ±0.5 m; Act I's 576 manifest doors), outside the building runtime; (2) stair walking is an analytic surface: a straight flight is linear between the two floors (up to 0.471 m below a tread top just past a riser, in the asset frame); a dog-leg (the grammar's stairs) is three segments, flight, landing and flight, each linear (0.16-0.17 m from the treads) and carrying and gating only feet within 1.0 m of it, because the flights of stacked storeys share their XZ corridors; the grammar's external steps climb linearly from the pad to the entry floor with a flat threshold over the wall base behind them; feet from the ground outside may not step onto a floor more than 0.45 m above them (a raised ground floor is entered by its steps); (3) a failed foundation still carries feet at the entry-floor height, and a failed suspended floor carries no one (the feet fall to the terrain); (4) instance yaw is a multiple of 90° (placement contract); (5) broad phase (2026-09-28, F2, `spatial_grid`, `world_index`): the set's queries, tank crush, both HE blasts, shell and cover rays, the on-foot target test, stealth sight lines, infantry cover rays and occupancy, on-foot story-cover collision, the door leaves and (integration, 2026-09-29) the T-34 patrol probe go through uniform XZ grids (16 m cells for records, occluders, story cover and doors, 32 m for building instances; two levels: a building's boxes in one cell form a group) instead of scanning every record; each query returns a superset of the boxes it can meet (registration padded 1 cm) and keeps its exact test and its scan order, so every answer equals the scan's (6,000 random queries per kind compared on the built test revision, 4,000 per set query on a damaged 64-house district); this is a data structure, not an approximation; (6) rendering (2026-09-28, F2, `building_render`): every module is an instance of its kit asset's shared prototype at the module's exact world transform (vertices within 1e-4 m of the former loose meshes, tested), and every module with solids owns one grounded rubble instance; the structural render delta hides the one and shows the other, so a failed module leaves the raster stream (zero matrix: its triangles have no area) and the TLAS (no slot) while its object ID and exact source record stay; rubble (solid city, 2026-09-29) is one mound per failed module laid where it fell, drawn from the same triangles tanks, feet and shells meet (the rubble row below; the earlier render-only lump mounds and their measured visible-volume excess are gone), prototypes per kit asset in one of four variants (a damaged, sooted or snowed variant takes its base's mound, for the solid pile and the drawn mound alike: row "Urban ruin detail"); pages group one prototype's instances per 128 m cell for the renderer's spatial culling, and prototypes whose textures are one decoded image (an asset's rubble, a base's variants and their shared atlas) share one texture upload (`Renderer::instanced_material`). Measured on the test revision (7 buildings): 469 instances in 90 pages over 35 prototypes, 721 fewer scene meshes and BLASes, renders equal to the loose-mesh renders except seam and sampling-noise pixels (99.3 to 99.8 % of pixels identical); (7) building light fixtures share the renderer's 32 point lights; since 2026-09-28 (F2) the budget keeps the lights with the largest irradiance scale at the camera, I / max(d², r²), instead of the largest peak radiance I / r² wherever they were (with 652 identical fixtures in the 162-house test revision the old rule dropped every light near the camera, the courtyard's authored lamps included); lights beyond the budget still contribute nothing (a named capacity, reported at build and by winter-bench); (8) cellars (cellars and scenes, 2026-09-29; see the terrain-openings row): a cellar storey a live stair reaches is opened; its flights carry and gate feet (layered, the flight's top lip meeting the entry landing, and a side step off its lowest treads where they stand against a spine wall), the lowest storey carries feet, weather shelter reaches down to it, and the stair hall's cellar floor is whole (`GRAMMAR_REVISION` 3). A failed module's mound is laid by the solid city's pile on what carries it: inside the excavation the heightfield reports the cellar floor, so a mound lands on the live floor over the cellar (the vault or slab) or, where that floor is absent or has failed, on the cellar floor; its occluder stands on the mound's own base there. Named limit (merge of 2026-09-29): the cellars branch's render-only shift that kept a perimeter module's pad mound clear of the excavation edge and kept a cellar mound within its room (`rubble_clear_of_opening`) is not taken, because it moved the drawn mound away from the solid one; a laid mound is one rigid heightfield at one base height, so one that straddles an excavation edge rests on the rim and overhangs the pit instead of slumping into it. A cellar no live stair reaches stays sealed: the terrain is continuous over it, it keeps its structural role, and its walls stay out of the tank's reach and sight (HE blast damage still reaches a buried cellar wall within its radius: blasts have no occlusion). (9) frame (integration, 2026-09-29, brief §7): a blueprint's local y = 0 is grade on the flattened pad, so a raised ground floor stands on its plinth; the featured house's slab bottoms stand on the pad in a district placement (its floor 0.4 m up); (10) sight occluders of failed modules (verifier, 2026-09-29): a failed module's occluder is its proxy's XZ footprint cut to 0.6 m (knee height) standing on its building's ground, under its rubble (`collapsed_occluder_on`). Before, it kept the legacy rule for walls (the box's own base, top lowered to 0.6 m above it), so a module that fell from an upper storey left an invisible sight blocker floating where it had stood: 11,384 of the 12,562 failed-module boxes of the urban-intro district at build stood more than 3 m up, the highest 20 m. Since the solid city (2026-09-29) a laid mound's occluder is the module's footprint from its building's ground to the lowest point of the mound's crest region (`solid_city::rubble_occluder`), never above the drawn rubble; the mound's apron beyond the footprint still casts no sight occlusion; the occluder index reaches each module's building ground, and the GPU equivalence test still finds the indexed answers equal to the scans. Since the solid city (2026-09-29) the player Tiger rests on and meets floors, plinths, steps, stairs, walls under its whole track footprint and rubble (the tank ground contact row); before, only walls, pillars, partitions and door frames met its bow and it stayed on the terrain, driving under a raised ground floor whose slab cut its hull. Capacity, not physics: LAN replicates at most 1024 instances per act, and per instance 2048 modules, 300 partly damaged modules, 128 doors and 128 containers; a record beyond that is refused alone and reported. Gate: `building_runtime::{tests, derived_regression}`, `building_set::tests`, `building_kit::tests`, `building_render::tests`, `spatial_grid::tests`, `lan::tests`, the ignored GPU tests `game::urban_runtime_gpu_tests`. |
| Urban building kit collision proxies (2026-09-28, `building_kit`) | Every kit module (reviewed Pizza GLBs, whole, restricted to some source primitives by material name, cut to half-spaces at load, or placed as a composite of such parts) gets axis-aligned proxy boxes generated from its loaded source triangles: conservative triangle/voxel overlap at 5 cm (roof sheets 10 cm), a per-shape fill rule (walls solid between their faces, slabs and flights between top and soffit, piers across, sheets and rods as a surface layer), greedy box merging, and faces snapped onto the source's own vertex planes. `building_kit::tests` measures each set against a reference voxelisation at half the voxel size and asserts the bound. Measured: plain walls, wall strips, doorways, slabs, piers 0.022-0.050 m; window walls 0.072-0.075 m; brick and boarded gables, stair flights, boarded doorway, holed slab, rubble mound 0.075-0.102 m; rods, railings, crates, bedstead 0.042-0.092 m (the crate is the unmarked Mega Pack board crate since 2026-09-29, 0.092 m: the verifier refused the Mega Pack II crate, whose four sides carry readable "DO NOT STACK" and "FRAGILE" stencils); the pitched roof slope at 10 cm voxels 0.220 m (bound 0.23 m). Openings are asserted free of source geometry and of proxies. Since the integration (2026-09-29) these generated proxies are the building runtime's collision, damage and sight solids for every kit asset, the featured house's nine included: `cargo run -- urban-kit-shapes` writes them with each built mesh's bounds and measured error into the tracked table `src/building_kit_shapes.rs`, so no runtime query loads a mesh, and `building_kit::tests::generated_shapes_match_the_live_generator` fails when the table differs from the installed meshes. The walking roles (floor tops, straight flights, the dog-leg's three segments, the lamp's light) are declared per asset and measured against the built mesh by `declared_roles_lie_on_their_meshes_within_the_declared_error`. | Named simplifications: (1) proxies are AABBs of a voxel fill rule, measured against a voxel reference (plus its 0.022 m half-diagonal), not an exact solid; (2) module transforms scale kit modules non-uniformly (fitted bay, documented storey pitch and wall thickness), which keeps proxies exact AABBs; the world error is the bound times the module's largest scale factor, and the texture stretches with the module; (3) timber casements and steel window grids carry no proxy: their 0.05-0.1 m members leave the aperture open; (4) the board floor is the source's board and ceiling planes 2 cm apart, so the 0.25-0.3 m joist depth of a timber floor is absent from geometry and proxy; (5) a composite renders with its first part's texture (existing loader behaviour for multi-material meshes); (6) a parapet or plinth strip and a boarded gable are source walls cut to half-spaces at load (`building_kit::clip_mesh`): the cut is exact on the source triangles and keeps their UV scale, and the open cut through a wall's two faces is closed by a planar cap whose texture continues the face's UVs at the source's measured density, so the cap's brick courses are not bonded to the face's; (7) round 2 (2026-09-29): the damaged bays' variants take one table entry per base and damage (`{base}.v.{damage}`, 28 entries), generated from the base with the structural cuts and measured against the surface with every cut (row "Urban ruin detail"); the slab stub is six clipped parts of the reviewed slab; the hanging slab, the one part turned other than a quarter turn about Y, is voxelised where it lies (its turned source boxes would only be their bounding box): 0.103 m, bound 0.12 m; snow deposits have a shape without solids or roles. Gate: `building_kit::tests`, `python3 tools/runtime_asset_cook.py validate`, `python3 tools/test_runtime_asset_cook.py`. |
| Urban building grammar (2026-09-28, `building_grammar`) | Merchant houses, 1930s apartment blocks, wooden houses and workshops are generated from the urban-fabric research's documented dimensions (R1, photo analysis P). A ruin is a state of the structural model: burned timber (floors, roofs, stairs, casements, furniture, gate leaves, whole wooden houses above their plinth) is absent from the blueprint, and `condition_damage` fails chosen modules through `BuildingState::apply_damage`, whose support graph and collapse clusters carry the failure on. Walls stand on the walls below them, never on floors (P G8); floors, stairs and roofs stand on the walls; a gabled roof is two sheet slopes per bay, each carried by its own eaves wall (and, on a workshop, by the steel lintel over the bay), so a breach under one slope takes that slope down and tears the roof open while the other stays (P G11, G16); every floor carries its construction (timber, concrete, brick vault, on grade: P G9). `building_grammar::tests` validates, compiles, damages and round-trips every typology × condition × size corner. | Named simplifications: (1) kit modules are scaled non-uniformly to the fitted bay, storey pitch and wall thickness; openings then differ from the documented ranges by the errors stated in `building_grammar::params` (e.g. block windows 1.50 × 1.62 m, sill 0.83 m against 1.3-1.45 × 1.45-1.6 m, 0.9 m; merchant windows 1.45 × 1.96 m, sill 1.07 m against 1.1-1.4 × 1.9-2.2 m, 0.9 m; semi-basement windows 1.45-1.5 × 0.44-0.49 m at sill 0.10-0.14 m against 0.8-1.2 × 0.5-0.8 m at 0.2-0.4 m), and textures stretch with the module; (2) masonry arching is a support edge from the three pieces below (a quarter bay either side), so breaches widen and step back at bay granularity; since round 2 (2026-09-29) the bays beside, under and over each failed bay show the breach's stepped edge, broken top or corbelled arch, and pitting, spalling and soot are modelled (row "Urban ruin detail" below); torn roof iron (P G16) is not; (3) brick cellar vaults are flat masonry slabs; (4) burned floors and roofs lie as one rubble mound per room, 0.12 m per burned floor plus 0.15 m (0.3-1.0 m, R1 §6.3), in the lowest surviving room; (5) attachments (casements, furniture, rubble mounds, gate leaves) have max integrity 65535 and fail only with the module carrying them, through an any-member cluster, so a hit on them cannot fail their wall; their own destruction is not modelled; (6) no door leaves exist, so doorways are open connectors and blueprints carry no `DoorDef`; (7) a merchant house's entrance landing is taken to stand on a brick vault [I], so its cellar stair survives the fire; (8) defensive changes are window embrasures (lower four fifths of a ground-floor window bricked up) and boarded or bricked-up doorways; wall loopholes are not modelled (loophole sizes are undocumented in the sources read, R1 §7.2); (9) a failed module is removed whole; since round 2 a failed concrete slab leaves stubs on the walls that still carry it or hangs from one (row "Urban ruin detail"), while the photographed torn and curled roof iron and V-collapsed rafters (P G13, G16) are not in the blueprint; (10) the offline previews (`tools/worldgen/preview_blueprint.py`) are Blender Eevee renders with a sun and sky for judging form against the photographs, not the shipping renderer. Gate: `building_grammar::tests`. Integration (2026-09-29): an upper storey's stair-hall landing reaches the top edge of the flight arriving from below; before, the ground storey's thicker walls left a 0.13 m gap in the floor between them (`GRAMMAR_REVISION` 2). The runtime walks these blueprints (external steps, dog-leg segments; see the compiled-buildings row), tested for every typology, condition and size corner by `building_runtime::grammar_tests`. Round 2 (2026-09-29): `GRAMMAR_REVISION` 3. |
| Urban ruin detail (2026-09-29, round 2: `building_kit_damage`, `building_kit` variants and deposits, `building_grammar::ruin_detail`) | What the condition did to a building is laid into its blueprint as states of its modules, built from the source meshes at load, never as decals: (1) the fire fells a two-row block's spine storey (1-1.5 bricks) wherever the floors on both of its faces burned; the stair box, a storey over a brick vault or on grade, and every spine behind concrete floors stand (P V5, G8-G10; R1 S10c); (2) 95 kit variants of 7 bays: the bay beside a failed module keeps the breach's stepped edge (steps of 0.28 m, four to five courses), the bay under a failed one a top broken back in steps, the bay over one a corbelled stepped arch, so every breach, collapse and ragged top has the outline of the structural model's own failures (P G11, G12, V12); outer bays of shelled, partly collapsed and (fewer) burned buildings are pitted (1.5 pits per m² of 0.08-0.30 m, 0.4 as deep as wide, with the outer half-brick spalled 0.12 m deep at the opening's head corners in patches 1.1-1.6 m on the long side, 0.45-0.65 m into the pier, over the lintel and down the jamb, the low end of G14's 1-3 m that a 3 m bay's piers allow; G14) and some holed (0.52 and 0.24 m round holes through a window bay's piers, 0.92 and 0.28 m in a plain bay; G11); the cuts are convex prisms removed from the source triangles and closed by reveals and floors at the source's UV density; (3) soot is a deposit in the texels: a copy of the source texture mixed in linear light with soot's reflectance (0.04) at coverage 0.55 inside burned rooms and over each opening's head, 0.25 on the rest of a burned storey's outer face (G15); bearing walls whose timber floor burned at their foot keep its joist pockets (0.18 × 0.22 m, 0.25 m deep, 0.9 m apart; G9); (4) snow (the district's aged snow, 0.86 linear, as the tableaux) lies on each bay's ledges (sills, breach treads, hole bottoms, broken tops) and, as attached `Debris` deposits that fail with their hosts, on every top nothing stands on (wall tops, parapets, cornices, stacks, piers, gables once the roof has gone, the top balcony): cells whose topmost up-facing surface sees the sky straight up or 35° in from the outer side (L8, V11); (5) a failed reinforced-concrete slab leaves a ragged 0.30-0.62 m stub on each line of walls that still carries it, or hangs from a side wall over the open storeys below it (at most 58°; G9, G12); both are ordinary structural slabs carried by those walls alone. Every variant of a base shares one atlas image, and instanced prototypes sharing texel images share one upload. Measured on the district at build: 112 kit assets in use (34 without the ruin detail), 27,114 modules (+17 %: about 3,500 snow deposits and 104 stubs and hanging slabs), 1,509 (asset, 128 m cell) pages of standing modules (559), TLAS demand 49,526 of 65,536 (45,607 before). | Named simplifications: (1) the spine rule classifies the documented outcome; it is not a stability calculation (no section, mortar or fire-duration data exist); (2) these are the condition's states, laid in when the building is instantiated: a module the player's fight fails at runtime still vanishes whole, and no new edge, hole, stub, hanging slab, soot or snow appears; its neighbours keep their states; (3) a variant's collision is generated from its base with the structural cuts (holes, edges, arches, broken tops) and measured against the surface with every cut, so pits, spalls and pockets lie inside the proxies within the recorded error (0.075-0.115 m in the asset frame, bound the base's plus 0.19 m; the world error scales with the module); (4) snow deposits have no solids (feet, tanks and shells pass through the 6-8 cm on a ledge) and are static: no snowfall, drift, wind scour or melt, and a top exposed by the fight carries none; (5) the soot coverages are read from the photographs' tone [I], not measured, and the head zone is a fixed trapezoid over every opening of a burned storey: P G15 finds no neat plume over every window at the photographs' resolution and asks for stains where flames vented, from large openings and breaches; the zones are uniform (in the final image of the burned block c3r2.w01 a head zone is about 15 % darker than the base build's, the rest of the face about 5 %), and a breach's own stain is not modelled; (6) the damage layouts assume the typical module scale (bay 0.75, storey 1.2, wall 1.3 of the kit's): a hole is round on the 1930s blocks and up to 20 % taller than wide on merchant storeys; (7) a variant's rubble is its base's mound: when the fight fails a damaged variant, its mound holds up to 32 % more than the variant's own solids (an arch bay; edges and tops 15-23 %, holes 2-6 %); the condition's own failures keep their base asset and are exact; (8) the hanging slab's bars are not modelled; its angle is set by the open height it swings over, and its collision is its placed triangles voxelised at 5 cm (0.103 m, bound 0.12 m); (9) to bound the variant count, breach-edged, arched, broken-topped and holed bays keep their soot but not the joist pockets, and every bay of one base and damage shows the same holes, pits and spalls (one seed per base and damage), so a shelled facade repeats one pattern per kind of window bay; (10) the kit's walls are bare face brick: P G3 finds 1930s blocks bare or rendered, and no rendered wall, and so no spalled render (patches of 0.2-1.0 m exposing brick), exists; the half-brick spalls and the pits are recesses whose floors carry the face's own brick texture (a fresh fracture's colour is not modelled), so they read by their reveals' shading. Gate: `building_kit::damage::tests`, `building_kit::tests` (every variant's bounds, atlas regions and snow; collision open through holes and edges, closed over pits; deposits on their tops), `building_grammar::tests` (the spine and sight lines, edges/tops/arches against the model's failures, surface states, snow exposure, slab remains, the condition failing the same modules), `building_placement::urban_district_sample_tests` (window axes). |
| Player Tiger component damage (2026-09-28, `tiger_damage`) | Every strike damages the component it lands on, penetrating or not, deterministically from the projectile seed; modules carry specific faults with the Tigerfibel's field repairs; turret-mounted parts take fragments where they are. Sources: `docs/TIGER_DAMAGE_REPAIR_20260928.md`. | Named simplifications: (1) zoning reads the contact on the two coarse armour boxes, and hull-box strikes above the 2.0 m roofline are re-read as turret strikes by height and bearing, until the exact-mesh armour seam exists; (2) per-zone effect probabilities and repair durations are tuning anchored to Zabel's report and firing trials, not measured rates; (3) an opaque vision block is shown by skipping its raster port on the shader backend, so its source face keeps its opaque material (the ray backend still transmits); (4) a sight knocked out of true is its whole pose turned 2–5 mils about the trunnion; (5) the driver frees a jammed visor without the toolbox (his own key); (6) faults and exterior state are not replicated over LAN or carried between campaign acts beyond working/broken; (7) the gunner's hand traverse rate (1.5°/s) is tuning — the sources say only "several minutes" a full turn. Gate: `tiger_damage::tests` and the ignored GPU repair and hand-traverse tests. |
| Urban-intro district (2026-09-28, `tools/worldgen/urban_intro_district.py`, workshop `act1-urban-intro-20260928`) | An offline, deterministic generator writes the district into a workshop: 338 `building_instances` inside the grammar's limits (tool version 4, density pass of 2026-09-29: 284 main buildings in R1 §4's typology shares and 54 summer kitchens and yard cottages counted apart; 203 before), which the runtime builds (341 buildings with the featured house and the Gumrak pair; TLAS demand 62,615 of 131,072 at build, 49,895 before), level pads with aprons, graded streets, 31 razed-plot mounds and one fallen-facade ridge sized by mass balance from R1's rubble volumes, 34 carved craters, and 6,482 manifest dressing ops from reviewed kit (3,644 before). Street-frontage occupancy, planned buildings on the frontage where building is allowed (receipt `frontage_occupancy`): core 0.78, ordinary 0.62, fringe 0.46 (R1 §4: 0.75-0.9, 0.6-0.8, 0.4-0.6; before the pass 0.76, 0.48, 0.31, the blocks' short sides 5 % built); facade to facade 18 m on ordinary streets, 24 m on the tram street, 12 m in lanes where both building lines stand. Battle traces (R1 §6.3-§7, P L7, L10, L11, S3, S6, S7): 312 small-crater ejecta patches (R1's inferred 1,300 per km² of all sizes, 90 % under 5 m), 232 soot patches at burned fronts, 6 wrecks (4 added along one side of the side streets, each in a 5-15 m kit halo) and 28 firing positions with boxes, boards and a helmet. Every building placed before the pass keeps its id, footprint, pad and condition, and the tableau hosts' yards take nothing new (tested). The courtyard workshop's terrain (± one cell) and ops are preserved exactly. Sources and per-element provenance: the receipt `urban-intro.receipt.json` (R1 urban fabric, P photo analysis). | Named simplifications: (1) terrain features live on the 4.29 m heightfield: mound volumes measure 0.86–1.03 of their mass-balance volume (receipt), narrower features and the trenches of P L10 are not carved; (2) the 130 standing utility poles use the editor's reviewed telegraph pole (`pillar_hr_2`, `environment_authoring::utility_scene_plan`), a static brick pillar with an exact-box hard stop where a timber pole would snap under a tank; replace it with a timber pole whose path matches `manifest_scatter_profile`'s hinge rule; (3) 162 yard walls, 255 gate piers, 69 wall stubs and 8 ridge chunks are manifest walls and piers, destruction records under the push model and shells since 2026-09-29 (tank push row), leaving a debris burst rather than a heap; (4) debris (≤ 0.35 m), drifts (≤ 0.45 m), timber and shed remains carry no collision, so a tank crosses them without resistance; (5) wires are level meshes without collision (the manifest has no pitch), and spans whose attachments differ by more than 1.2 m are omitted (9); (6) Act-I winter terrain shading (`terrain_material.wgsl`, mood 0) is a constant fresh-snow albedo of 0.91–0.95 that never samples the drape, so no street, path or rut shows on the ground, and the 478 snow-drift meshes (material 0.76–0.84) render darker than the ground snow they lie on; the opt-in `--texture` drape affects spring shading and offline previews only; (7) density pass: pads that share a heightfield sample stand within 0.12 m, so on the DEM's 4-7 % slopes a party-wall row breaks into a gate or yard wall every one or two houses (848 m of frontage left open for it, receipt `dense_frontage_left_open_m`); stepped floors across a party wall need a finer terrain under buildings; (8) battle traces: craters under 5 m are only their ejecta, a flat 0.07 m scorched-earth mesh without collision (their bowls are below the heightfield's resolution), the soot patches are the same mesh, and the size split of R1's crater population is an inference; trenches, barbed wire, hedgehogs, barricades, spent cases, abandoned guns and dead horses are absent (no terrain opening as district data, no reviewed asset). Gate: `python3 tools/worldgen/test_urban_intro_district.py` and `building_placement::urban_district_sample_tests`. |
| Urban-intro tableaux (2026-09-29, S, `urban_tableaux`, `urban_tableau_meshes`; cellars and scenes 2026-09-29) | Ten sourced tableaux on ten hosts (catalog `catalog/urban_intro_tableaux.v1.json`, provenance per scene): R2's five, two of them now continued down their hosts' cellar stairs (the firing step under the embrasure with the September gun; the red-cross dressing station's two rooms), and the owner's three historical scenes (28 Sep) as five: the burned household (cellar shelter and courtyard trench), the cellar ward and the doctor's cellar 58 m away, the Gumrak store and the house across the road (built by the district tool beside the old road west, outside the district). They are resolved at build in the local frame of a room of their host building and move with it (tested under relocation and a half turn of every host). A piece resting on or hanging from a module leaves with that module's failure, and a light or smoke on it goes out. Static pieces take collision from their source geometry: the Kh tram car's own solid boxes (113; exact at quarter-turn yaws, measured 0.063 m on its 1.6 % grade from the tilt of each 1.2 m segment), the dugout's stepped boxes containing its mound (45; the mesh measures its steps against the mound surface: at most 0.25 m, at the toe below grade; 0.20 m elsewhere) and the Pz III wreck's source-surface voxel cells (64; bound 0.721 m at scale 0.72). The pressure lamp (2,800 K, 5 W/sr) and the stove fire (1,850 K, 2.5 W/sr) are ordinary shadowed point emitters inside their own open cage and open fire door, so their light leaves only through real openings; the cellar dressing station adds two car headlamps on a battery (2,900 K, 0.4 W/sr each, their filaments inside the lamp bodies, whose shells shadow them into beams on the high preset; the laptop preset's forward path carries no local-emitter light yet (laptop row), so there every tableau lamp shows only its own glowing parts) and two wick tins (1,900 K, 0.03 W/sr). The new static pieces take collision from their solids: the brick firing step (exact under its quarter turn, 0.03 m jitter), the door table on trestles (0.3 m: the trestle boxes enclose the air between their legs) and the fallen 45 mm gun (tilted parts as boxes, measured by the mesh, turned 180 degrees: 0.34 m). A group may dig an excavation (the household's courtyard trench, 14 x 1.2 m, 0.9 m deep): a terrain opening with a drawn earth floor, cut with the cellars (terrain-openings row). A pushable prop needs a scatter profile (authored, or from its asset name) or the catalog fails. | Named simplifications: (1) cloth (the canopy, the red-cross cloth, the blankets) is static geometry without collision and without wind; feet pass through a blanket as through one pushed aside; (2) the tram is a static obstacle: a 12.65 t car a Tiger could shove or crush, its 0.2-0.35 m fragment holes are not in its collision and its wheels do not turn; (3) the dugout's floor is at grade under a heaped mound instead of a pit dug 0.5-0.7 m (the terrain could not be opened when it was authored; excavations now exist and it has not been re-dug); it has no destruction state, so it stops a tank; (4) snow on the pieces is static: `snow_cap` lays 5 cm on every topmost up-facing cell (0.12 m grid, normal y at least 0.8) of the tram and the wreck as they rest, clipped to their bounds; snow on the horse, the dead, the canisters and the mound is authored geometry. No snowfall, drift, melt, footprints or drag marks; (5) the stovepipe smoke is a smoke-only source in the world fire system's soot pages: an analytic column (rise 0.55 m/s, life 4-6.5 s, wind drift) in the same dark soot shades as burning buildings, not a fluid or a wood-smoke colour, as in the fire row above; (6) the covered dead and the horse carry no collision, like the engine's other corpses; (7) the lamp and stove powers are hand-set, not calibrated (the compact-emitter row's calibration debt); (8) the wind in the slot is one generated gust clip played on re-entry, not acoustics of the opening; (9) static boxes are world-axis-aligned: at a yaw off the quarter turns, or tilted to the terrain, a box's world AABB is larger than the rotated box, and that inflation is measured per box and included in the reported bound; (10) the unlit cellars cannot be seen: rank 1's cellar, the household's shelter, the ward and the doctor's cellar have no emitter by their sources (abandoned, a cold hearth), daylight reaches them only through small windows and a stair, and the high preset's single secondary bounce (surface-transport row) with the fixed gameplay exposure leaves them black at the district's authored dusk and at the inspect exposure cap alike (`captures/urban2_*_inspect_x360.png`); the red-cross ward room's wick tin is likewise lost at the authored exposure. Their composition is judged in a review-only copy of the inspect workshop with real shadowed work lamps (`tools/worldgen/urban_tableau_views.py --write-worklamp`, captures `urban2_*_worklamp.png`), never in the game; the player has no carried light. Deeper transport, a carried torch (a modelled emitter) or bounded eye adaptation would each change this; none is done here; (11) the pressure lamp's hiss (`assets/sounds/cellars_20260929/pressure_lamp_hiss.ogg`, written by `tools/worldgen/cellar_scene_audio_20260929.py`) is generated noise at a hand-set level, not a recorded or calibrated source; (12) the historical scenes' GLBs from `assets/authoring/historical_20260925` (charred joists and planks, ash bricks, Gumrak variants) and the battlefield scorch mesh are outside the reviewed runtime asset catalog (`catalog/runtime_assets.v1.json`); (13) glass (the plain bottle) is drawn opaque; (14) the ward's relocation into a residential cellar is against the scene catalog's own placement rule (a factory cellar); the owner's decision and the conflict are recorded in its place envelope. Gate: `urban_tableaux::tests`, `urban_tableau_meshes::tests`, `cargo run -- urban-tableaux` (per-piece bounds). |
| Tiger static tipping and the CANYON course (2026-09-29, `tank_support`, `src/canyon.rs`, `tools/worldgen/canyon_course.py`) | A rest pose that would roll the player Tiger past its static tipping angle is refused like the other limits (a move may not make it worse): atan(1.93 / 1.30) = 56.0°, the downhill track's outer edge measured from the baked hull (1.93 m off the centre line) over a centre of gravity 1.30 m above the track bottoms. With the 35° gradient limit, no face whose gradient exceeds sqrt(tan² 35° + tan² 56°) = tan 58.6° can be climbed at any heading (unit test over headings 0-90° on 60, 64 and 76° faces). The CANYON course's walls are real terrain: loess-like cliffs of 66-78° over a talus foot, eased over about a 3 m cell; the tool proves offline that every path from the start to the plateau crosses a sample whose gradient exceeds tan 58.6° (central differences over two cells, gentler than the drawn triangles) except through the goal disc, and that a path under 35° reaches the goal; the GPU test drives the Tiger at the walls head-on and obliquely and it is stopped by the step and gradient rules within 4 m of the floor. The course's defence is the winter encounter's own (platoons on beats over a measured nav grid, 45 mm guns sited by the field-of-fire probe, riflemen); its finish is the run's clock when the crewed Tiger enters the goal disc, after which the Soviet crews stop firing while picture and sound fade. | Named simplifications: (1) the tipping angle is static: speed, turning, a track dropping into a hole and soft ground are not modelled, and a real Tiger past it would roll over, where the model refuses the move; the centre-of-gravity height is an engineering estimate [I] (no source found); (2) the course is a playtest geometry, deeper (25-45 m) and steeper than the Stalingrad balkas it is composed from, and its plateau ends at the 3 km map edge; (3) AI tanks and infantry do not meet the gradient or tipping limits: the T-34s keep to nav cells with under 1.2 m of relief and the riflemen to the floor, so none climbs the walls, but that is their navigation, not their physics; (4) the finish stops the Soviet crews' fire (`set_playtest_return_fire(false)`) for the fade, a presentation rule after the run is scored. Gate: `tank_support::tests::a_face_steeper_than_the_combined_limits_cannot_be_climbed_at_any_heading`, `python3 tools/worldgen/test_canyon_course.py`, the ignored GPU test `game::canyon_game::tests::the_canyon_course_fields_its_defence_stops_the_tiger_at_its_walls_and_completes_at_the_goal`, and the film `catalog/shader_reviews/canyon_fight_20260929.v1.json`. |
| Tank ground contact and obstacles (2026-09-29, solid city, `tank_support`, `solid_city`) | Every frame the player Tiger's hull rests on the highest supports under its tracks: the terrain (its drawn triangles, row below), the rubble mounds and burned-floor layers that start within a step of its tracks, and every building module solid its tracks can carry on (floors, plinths, foundations, external steps, stairs, wall stubs and tops). The rest is the exact rest of a rigid hull on those supports: the plane of least centre-of-mass height lying above every support, found on the tracks' upper convex hulls, with the track bottom's profile measured from the baked hull (flat from z -2.0 to +2.2 m, rising to 0.95 m over the sprocket and 0.93 m over the idler) lifting it where a support meets the rising ends. The Tiger's documented limits refuse a move at the point where its tracks meet the obstacle (bisection to 0.1 % of the move): a step over 0.762 m (US War Department TM-E 30-451, 1945, p. VII-82, "Step 2 feet 6 inches") and a gradient over 35° (Spielberger, *Der Panzerkampfwagen Tiger und seine Abarten*, pp. 190-191, via de.wikipedia; the baked belly measures 0.469 m); a trench wider than 2.50 m (Spielberger) tips the hull in once its centre of mass is past the near rim. A violation never grows: a move that would make one worse by more than rounding (1e-5) is refused. Climbing is power-limited: 478 kW (650 PS) times a drive efficiency of 0.6 [I] against the weight's pull and rolling resistance on the grade (0.041 since 2026-10-03, chosen so the drivetrain's own level limit in 8th gear, speed drag and torque curve included, is the model's 12 m/s; the earlier 0.043 left the automatic box topping out at 8.4 m/s in 7th, and a clutch-in rpm gate held any standing start on a grade above about 3 degrees in 1st gear): 2.3 m/s on 10°, 0.8 m/s (2.9 km/h, first gear 2.8 km/h) at 35°. Since 2026-09-29 the belly is a support like the tracks: a crest between them that reaches the 0.47 m clearance carries the hull high-centred (the plane of least centre-of-mass height over tracks and belly), the belly takes the share of the weight the lever between its contact and the tracks' gives it, the tracks lose that share of their grip and the belly drags on what carries it (`tank_push::belly_drag`), so a hull balanced on a crest hangs there with its tracks turning while one lightly on it grinds over; a building solid under the belly (front steps, a footing) is pressed on like any obstacle. A building solid the tracks or bow meet higher than a step, or hanging over the hull, is pressed on by the push model (row below: a wall or plinth that carries the building is punched through by momentum, one that carries nothing bends over, front steps shift, a slab met edge-on slides off its bearing, timber breaks), wherever under the track footprint it stands; rubble and terrain cannot be crushed and stop it. A suspended floor, landing or stair the tank rests on, directly or through the rubble on it (floors within 0.5 m of a loaded station), fails at once: 57 t on about 5 m² of track is about 11 t/m², where floors of the period were designed for a live load of the order of 0.2 t/m² [I]. Telemetry (pitch, roll, what carries it, the refusing limit, floors given way, extremes) is in every mechanic-review receipt. | Named simplifications, each bounded: (1) a rigid hull on rigid tracks, no torsion-bar travel; each track is its centre line carrying the highest of five points across its 0.78 m width (0.195 m apart) at stations 0.1 m apart, the belly nine points 0.2 m apart at stations 0.2 m apart, so a peak between samples of a surface of slope at most s stands at most about 0.1 s above them (0.08 m on a rubble flank at its steepest; a building box's face is met within 0.1 m of travel); (2) a track takes up a new, higher support only at its leading end, where the rising run meets it (within 0.76 m of the ground plane under the hull, then judged by the step rule over a 1 m window less what 35° explains), or where it already lies within 0.05 m of it: a standing tank never climbs (before this rule a pitched hull admitted a wall top up to a step above its raised nose and rose up a facade frame by frame to 42.7°); (3) the pose follows its rest at a limited rate (pitch and roll 1.5 rad/s; height at twice the speed plus 0.1 m/s, so rubble landing around a standing tank leaves the hull in it, reported as embedded) and falls under gravity where its supports drop faster than it can drive down 60°; a hull placed by a spawn or fixture is set on its rest; (4) the belly's load share is the lever between the mean station of its contact and the mean of the tracks' touching stations about the centre of mass (all of it when the tracks hang), not a statically indeterminate solve, and rubble under the belly is rigid (it does not compact); (5) a solid hanging over the tracks (underside above 0.95 m, the track run's highest point) meets the upper hull or turret and is pressed on like one under the bow; a floor, landing or stair met edge-on is held only by friction on its own weight (how firmly one is cast into its walls is not known); (6) building solids are the kit's generated proxy boxes (their measured errors in the building rows above); (7) the resistances are the push model's named estimates (row below), not measurements of the district's walls; (8) only the player Tiger: AI tanks and infantry are still grounded on the terrain alone and pass through floors and rubble (their navigation debt), and LAN clients receive the hull's position and heading, not its pitch and roll. Gate: `tank_support::tests`, `game::solid_city::tests` and its ignored GPU tests on the urban-intro district (a 0.45 m footing climbed and bridged; a raised ground floor crushed into by momentum, its steps shoved aside and its floors giving way, the bow inside the house, never past 36° standing; a rubble slope climbed more than 1 m; at a crawl the Tiger held by a workshop wall stronger than its push, which a ram then breaks, and the 1.31 m concrete floor that used to hold it slid off its bearing; the ploughing film; a soldier up a rubble mound a step at a time; a round costed by the rubble it crosses), the headless `crush`, `treefall` and `overpen` regressions, and the four `catalog/shader_reviews/urban2_solid_city_*` films. |
| Tank push (2026-09-29, `tank_push`; `Game::push_resistance`, `press_on_structure`, `trees::Trees::crush`) | Every obstacle the player Tiger's bow, tracks or belly meet gives way by its strength against the tank's push, or holds, and the speed pays: `m dv/dt = F_push(v) - F_resist - rolling - m g sin(slope)` while pressing, the obstacle absorbing `F_resist` times the travel until its breach work is done (its HP pool takes the same share, so shell damage already taken counts). The push is the smaller of the engine through the gears (0.6 x 478 kW over the speed, no lower than first gear's 2.8 km/h: 369 kN) and the tracks' grip (0.5 x 57.25 t x g = 281 kN). Resistances: a tree by the bending strength of its trunk, measured from the drawn mesh at the bow's height (green-wood modulus of rupture 40 MPa, Wood Handbook FPL-GTR-282 table 5-3a, 34-50 MPa), so trunks up to about 0.43 m fall and a sapling costs almost nothing; a free-standing brick wall, pier or stub (the manifest's yard walls and gate piers, a burned house's footing) by bending and overturning about its base (flexural tensile strength 0.1 MPa, EN 1996-1-1 table 3.6); a wall piece that still carries its building by punching shear round the bow's contact (0.15 MPa, EN 1996-1-1 table 3.4, 0.1-0.3 MPa; concrete 1.0 MPa), shared among the pieces the bow straddles, so a crawl stops at a load-bearing wall and a ram breaks it (a 0.51 m wall needs about 195 kJ, a Tiger at 3 m/s carries 257 kJ); front steps as loose blocks; a slab, landing or stair met edge-on by friction (0.6) on its own weight; timber, sheet iron and poles 30 kN; bunker concrete never. The manifest's walls, window walls, breached walls, doorways, piers and poles, FPS walls and shed, garage and warehouse shells are destruction records since 2026-09-29 (shells and the push bring them down, their proxies fall together, feet and sight meet them until they fall), no longer static story cover. | Named estimates [I], each with its source range: the tractive coefficient 0.5 (J. Y. Wong, *Theory of Ground Vehicles*, 0.3-0.7 from snow to firm soil), the drive efficiency 0.6, the bow's contact 3.4 x 1.6 m at 1.2 m, the masonry and concrete strengths above, a wall's breach travel (half its peak over its thickness, softening), a trunk's 0.3 m and a board's 0.3 m, the belly's steel-on-masonry friction 0.5; the tank's own acceleration model (6 m/s², tuning) still drives it between contacts. A pressed tank below 0.15 m/s against something stronger than its push is at rest (the drive's creep is not energy the obstacle can take). What stays static: wrecks (source-surface proxies), tableau pieces such as the tramcar and the legacy story overlay's columns stop the bow unyielding; pushing a wreck or a car along is not modelled. A felled manifest wall leaves its debris burst, not a heap (no rubble mesh of its own). AI tanks are not under the push model. Gate: `tank_push::tests`, `tank_support::tests`, `trees` tests, the ignored GPU tests `game::solid_city::tests` (above) and the headless `crush` and `treefall` regressions. |
| Rubble (2026-09-29, solid city, `building_rubble`, `building_set`) | A failed module's rubble is one mound: a heightfield (nodes 0.3 m apart, coarser on the largest modules: at most 48 cells a side) of a frustum at the angle of repose of brick rubble, 37° [I, R1 §8.5], measured as Euclidean distance from its core, with an apron below its base and seeded relief lumps of up to 0.10 m, holding exactly its solid volume times a bulking of 1.5 for masonry and concrete, 0.6 for timber and 0.05 for sheet iron and steel (P G17 [I]). It is laid where the module fell, on whatever lies under it: the ground, the floors under it, or the mounds already laid there, the base found by bisection so the volume added above them equals the budget (within 1 %). The union of the laid surfaces keeps every flank at the repose slope plus relief (measured steepest drawn flank less relief: 1.074 tan 37°). The mesh drawn and the surface queried are the same triangles (measured: 7.2e-7 m); burned-floor layers (the grammar's Debris modules) are heightfields generated from their mesh's top at 0.01 m cells (measured 0.017 m in the asset frame). Rubble carries tank tracks (row above) and feet, which climb it a 0.45 m step at a time (the buildings' step-up); a round entering it loses 47 mm of penetration per metre crossed (the brick transit's 35 mm per wall of about 0.5 m, by the density of loose rubble, 1/1.5 [I]), with 0.4 per metre of yaw disruption and 20 mrad per metre of deflection [I]; HE bursts at its surface; small arms stop in it. The weight on rubble reaches the floors it lies on. Its sight occluder rises from its building's ground to the lowest point of its crest region. Rubble that starts above a track's or a foot's reach (a burned-floor layer on an upper floor) is not their ground and not a round's medium below it (before, a layer 10 m up read as a 9.9 m step to a tank in the street). On the district at build: TLAS demand 45,597 of 65,536. | Named simplifications: (1) a mound is one rigid heightfield surface: no overhangs, voids or single chunks; it does not slump, compact under a track or slide, and a tank does not displace it; (2) mounds are laid in failure order; when a floor, foundation or stair carrying a mound fails, the building's whole pile is laid again in the same order at once (the rubble drops with it; it does not fall), as it is when a snapshot restores a module; (3) the rubble of a failed module in a sealed cellar stays buried (no mound); (4) the repose angle, bulking factors and ballistic costs are engineering estimates [I], not measurements of Stalingrad rubble; (5) AI infantry and AI tanks do not meet rubble (the tank row's (8)). Gate: `building_rubble::tests` (volume, stacking, flank, render against collision, layer surface), `building_render::tests`, `game::solid_city::tests`. |
| Authored terrain: the drawn triangles are the collision surface (2026-09-29, solid city; was the verifier's P1 of 2026-09-29) | Both presets draw authored terrain from its full collision grid (Act I revisions 1024², 2.09 M triangles; the homeland acts 1536² and 1280², 4.71 M and 3.27 M), every vertex a source sample and every quad split along its (i+1, j)-(i, j+1) diagonal (`terrain_opening::build_mesh`, which also clips the opened cellars and trenches out of it: terrain openings row; its refinement round them is moot on the full grid), and `Heightfield::height_at`, which grounds tanks, feet, pads, props and rounds, interpolates on those same triangles (it was bilinear, a quarter of a cell's twist off the drawn triangles: up to 0.2 m on the urban-intro streets). Measured: `tools/worldgen/terrain_render_gap.py` reports 0.000 m on building perimeters, pads and streets; `terrain::tests::height_at_is_the_drawn_triangle_surface` (a twisted, stepped grid) and the campaign terrain test (every 101st triangle of all three acts) hold the drawn surface to the collision surface within 1e-4 m and 2e-3 m (float rounding at world coordinates). Before, the 4090 preset drew a stride-sampled 512² mesh and the laptop 256², up to -0.44..+0.37 m (4090) and -1.32..+1.42 m (laptop) off building bases and -1.25..+1.01 m off the streets, and the two presets drew different occluders. | (1) The raster and ray-scene cost of the full grid is paid on both presets: on this RTX 4090 about 3 ms more on the shader backend at the Act I spawn and no difference within the noise on the ray-traced backend or the laptop preset's raster (docs/URBAN_INTRO_20260928.md, solid city); a real laptop GPU was not measured; (2) the decimated mesh survives only as the debug gate `KESSEL_DEBUG_TERRAIN_RENDER_RES` (debug builds, explicit opt-in; `debug_gates::tests` proves it closed in both shipping presets); (3) manifest operations placed offline by the worldgen tools still take bilinear heights (`world_authoring.sample_height`): an authored op on a twisted cell sits up to a quarter of that cell's twist off the surface (0.019 m at most on the district's pads and building perimeters, up to 0.2 m at a few street cells); (4) the district's flattened pads and 0.15 m pad-to-street steps are one-cell (4.29 m) ramps of the grid, as the collision always was; (5) the legacy non-manifest paths (the Gaea probe, the homeland fallback, the DEM window) keep their own meshes. Gate: `terrain::tests`, `debug_gates::tests`, `python3 tools/worldgen/terrain_render_gap.py` (0 on the live configuration). |
| Terrain openings: cellars and trenches (cellars and scenes, 2026-09-29, `terrain_opening`, `terrain`, `building_runtime`, `game::open_cellars`) | A cellar a live stair reaches, and an excavation a tableau digs, is an axis-aligned world rectangle (`TerrainOpening`; a cellar's lies midway inside the thickness of its foundation walls, tested for every typology at four yaws) in which the heightfield has no surface of its own: `Heightfield::height_at` answers the host's cellar floor (or the trench's floor), and every consumer that grounds on it (feet, soldiers, tanks, tableau pieces, debris) meets that floor. The render mesh is clipped exactly on the terrain's own planes (a 3 x 3 half-plane partition per opening, canonical crossings, bit-exact vertex dedup and edge points inserted into neighbouring triangles: watertight, tested), its soil faces stand from the cut edge down to the footings, a trench's earth floor is drawn, and the region round each opening is drawn from the collision grid on both presets (terrain render row). Every mover meets an excavation face as the step it is: feet climb at most 0.45 m (a jump clears more), AI soldiers without navigation neither climb nor drop more than 0.45 m (they stop at a rim), tanks climb at most the Tiger's 0.762 m step and bridge an opening narrower than its 2.50 m trench, the solid city's sourced limits (`tank_support::TIGER_STEP_M`, `TIGER_TRENCH_M`; the cellars branch's 0.79 m and 2.3 m were unified to them in the merge of 2026-09-29): an AI tank through `Heightfield::vehicle_height_at` and the rim gate, the player's Tiger through the solid city's rigid track contact, whose tracks meet the rims and the cellar floor as supports, and only the player's Tiger drives over a rim into a pit (`Tank::drops_into_pits`; AI tanks are kept out). The floor over a cellar fails under the Tiger by the solid city's one rule (a suspended floor, landing or stair that carries the tank gives way; tank ground contact row); the cellars branch's second rule, which broke every floor box the hull's footprint met, is not taken. On the urban-intro district: 60 cellars opened, one trench dug, 15,366 m² of terrain cut, 2,590 source triangles clipped, 2,029 soil faces (build log). | Named simplifications: (1) openings are axis-aligned rectangles (building yaws are quarter turns by the placement contract; a trench must be authored axis-aligned in its host's frame); (2) an AI tank's hull height is the pit floor or the grade under its centre (kinematic, no pitch); the player's Tiger falls under gravity onto its supports and tips at the rate of the tank ground contact row, with no bounce or damage; a tank in a pit climbs out only where a face is at most 0.762 m; (3) the gates are applied to each move's segment against the faces it crosses; since 2026-09-29 an AI walker heads for the first corner of the shortest way round the openings near its straight line (an exact visibility graph over their corners grown 0.8 m, `TerrainOpenings::walk_toward`), but it never paths down a stair into a pit; in mixed combat a soldier without a contact is walked to his post by the doctrine (the learned policies see every objective as a contact and halt at their engagement distance) and, when the movement gate stops a step into cover, steps round it by the first free heading turned 15 degrees at a time: a local rule that finds its way round a pier, a wall end or a wreck, not a path planner (a concave trap can still hold him). Before, a pier in a gateway held a rifleman sent to a 45 mm gun 14 m short; (4) soil faces and the trench floor are drawn earth-coloured without snow, and no snow drifts into a trench or cellar; (5) debris, casings and blood land on `height_at`, so inside an opening they land on its floor, and on a building's upper floors they still land on the terrain rule (the brief's item (e) for building floors is not done); (6) the dugout (tableaux row, item 3) is not re-dug. Gate: `terrain_opening::tests`, `terrain::tests`, `building_runtime::grammar_tests` (`soldiers_walk_down_the_cellar_stair_once_the_opening_is_cut`, `a_cellar_opening_is_cut_inside_its_foundation_walls`), `python3 tools/worldgen/terrain_render_gap.py --refine FILE`. |
| River water and declared bridges (spring bridge map, 2026-11-02, `river_crossings`, `tools/worldgen/spring_bridge.py`) | A map's `river` block draws a water surface along its centreline at the authored level, falling downstream; the channel, its 66-degree cut banks and the floodplain are the heightfield itself. A map's `crossings` are deck, pier and rail boxes; the deck's exact box is the support for the player's Tiger, AI tanks, wrecks and feet through the same queries as the expedition theatre's bridges, and the boxes are cover. A tank is kept out of the river only by the banks exceeding its sourced limits (35 deg gradient, 0.762 m step, `tank_support`). | Named simplifications: (1) the water is an opaque, smooth (roughness 0.16 by default; a map may set its own colour and roughness, the spring bridge v2 map 0.05 and a clear blue), flat-coloured surface: no transmission, refraction, absorption with depth, flow, waves or wetting, the same as the theatre's river; (2) there is no buoyancy, wading depth, engine flooding or drowning: a vehicle that reaches the bed drives on it; (3) the water level is a smoothed, monotone profile of the ground along the course, not a hydraulic solution; (4) decks are axis-aligned along world X and indestructible; (5) a crossing's steel trusses (through truss over the deck, deck truss under it) are drawn and their chords are cover, but no member collides with a vehicle: the carriageway inside them is the deck's full width and 6 m clear, tested. Gate: `river_crossings::tests`. |

## Audio (sound levels, 2026-09-28)

Destruction pressure timbre update (2026-10-01, after v6 crew review): the finite authored
compression/rebound retains a reduced 32–41 Hz displacement and an aligned 98–109 Hz body.
This redistributes the waveform's energy out of subbass before its existing A-weighted placement;
it changes no physical source level, propagation/headset law or common master setting. It is
authored reproduction timbre, not a simulated blast pressure history. The controlled live-graph
comparison and limits are recorded in `docs/CATASTROPHIC_AUDIO_20261001.md`; regression
`acoustics::tests::destruction_pressure_keeps_audible_body_through_the_crew_headset` checks the
calibrated headset path, and the existing catastrophic hierarchy test guards relative scale.
The player's internal ammunition sound carries explicit source provenance: a Buttoned/HatchOpen
listener in the same compartment does not receive exterior hull, snow or cover filtering, while
its headset, air path, travel delay, hearing and steel reverb remain. Identical enemy-wreck sounds
retain exterior propagation. Escape through a hatch to a head-out/on-foot listener is not solved;
those views retain the existing exterior transfer approximation.

The owner's rule (28 Sep 2026): "your own gun should be extremely loud ... ultimately everything
should be as loud as it would be in real life. its a simulator." Every source is calibrated in
physical units and propagated to the ear; one reproduction transform, named below as such, turns
ear levels into samples. Sources: [docs/research/SOUND_LEVELS_20260928.md](research/SOUND_LEVELS_20260928.md)
("SL"). No 88 mm, WWII turret interior, tank-hull insertion loss or in-vehicle impact measurement
exists; those figures are the report's scalings and derivations ([E]/[D]).

| System | Live truth | Remaining debt / release gate |
|---|---|---|
| Source calibration (`sound_levels`) | One entry per source: Z-peak (dB re 20 µPa) and short-term A-weighted level LAF at a stated reference, spectrum class, distance law, durations and the report's provenance tag. Own 88 at the ear 163/139 buttoned, 168/142 hatch open, 182/152 head-out; guns scaled by propellant charge through Kingery-Bulmash (88 177 dB at 10 m side ↔ 0.90 kg TNT; charges not in SL estimated from muzzle energy at the anchors' efficiency); HE bursts by fill (mean of bare and Fano-cased); hits on own armour by SL §5's ringing-energy law or the confined quasi-static pressure of a filler bursting inside; Tiger interior 93/110/115 dB(A); T-34 106 dB(A) at 10 m. | Estimates for sources the report does not cover are tagged [E] and `in_research: false`: wind at the ear, foley and mechanisms (breech, tools, hatches, footsteps, paper), starter, hydraulic traverse, exterior armour strikes, bullets on steel and ground, spall fragments, ammunition-explosion and ground-strike TNT equivalents, Panzerfaust launch (PTRD figures), recoil stroke. Muzzle brakes of the D-25T/ML-20S/D-10S/M-10T are general knowledge. The 45 mm side level is 167.5 dB at runtime (the report's own unbraked −2.5 dB applied) against the table's 170. Gate: `sound_levels::tests`. |
| Propagation (`sound_propagation`) | Per event at hear-time, per octave band 31.5 Hz–8 kHz: Kingery-Bulmash (Swisdak fit) for blast, 166 − 20·log r for rifle-class blast, spherical for engines/voices/foley, the Whitham N-wave evaluated at the listener; muzzle directivity (+7.5 forward, −5/−10 rear); ISO 9613-1 air absorption from the act's temperature and humidity (winter −20 °C, 80 %; spring the ISO +15 °C/70 % row as a stand-in); CRREL snow ground effect relative to hard ground; Maekawa barrier loss when the ray is blocked. Speed of sound from temperature (319 m/s at −20 °C) in the delay queue, pressure fronts, crew blast impulse and review tools. | Named approximations: beyond Z = 198.5 the midpoint exponent 1.24 of the report's 1.1–1.38 bracket; below Z = 2.9 the fit's local slope continued; the snow model is a fit (9 dB/octave above f_x = 100·(r/10)^−0.35 Hz, capped 8 + 14·log(r/10) dB) that ignores source and receiver heights; the barrier assumes a 1 m path difference (no diffraction geometry); no refraction by wind or temperature gradients, no Doppler, no ground reflection as a separate arrival; class spectra are nominal [E] shapes; LAF follows the peak law. The waveform filter is a fitted cascade of 16 high-shelf biquads (fit error ≤ 2 dB at band centres, tested); the level is computed, not taken from the filter. Gate: `sound_propagation::tests`. |
| Listener enclosure and headset | Buttoned: the report's band insertion loss (upper end of the range at 31.5/63 Hz so the whole-impulse rule holds) and whole-impulse peak loss (−25 dB low-frequency blast, −35 dB cracks and small arms); open hatch 3–10 dB [E]; head-out and on foot 0 dB. Aboard, the Dfh.b supra-aural headset estimate on every source. Own sources (gun, MG, strikes, engine) calibrated at the ear. Buttoning up makes the own engine louder (+8 dB against an open hatch, SL §7/§8). | The report's insertion loss is itself an estimate; one hatch state covers every lid; no per-hatch geometry or ball-mount path; head shadow per band is estimated and the waveform's head shadow is the older fixed filter; the headset attenuation is an estimate for an unmeasured Dfh.b. A strike on the own hull is heard at the interior law (SL §5) whatever the enclosure: head-out and hatch-open listeners get no separate exterior radiation path. The own engine head-out is the exterior level carried from 10 m to ~4 m [E]. |
| Binaural rendering (`acoustics::spatial`, 2026-10-27) | Every positioned voice is placed around the head by one deterministic stage: Woodworth's spherical-head interaural delay (87.5 mm, 0.66 ms abeam, fractional delay on the far ear), Brown–Duda's first-order head-shadow filter per ear (unity at low frequency, 2× at the facing ear and 0.1× in the shadow at high frequency), the owner's constant-power presentation pan on top. `EarLevel.transmission` carries how an exterior sound came in: direct, through the closed plates (buttoned and every periscope view), or through `open` hatches; the plate-borne share keeps 30 % of its lateral angle, the hatch share arrives from 45° above, the two shares from SL §8's band insertion losses under the voice's spectrum, rendered separately and summed by √share. Streaming voices (vehicles, fires, loose track, elevation gear) ramp the same model per sample with the listener's current enclosure. Levels are untouched: `propagate` sets them before and the LAF placement normalises after. Measured abeam: dominant interaural lag 10 frames buttoned, 17 through one hatch, 29 head-out; a 3 kHz tone abeam is shadowed, not silenced; 125 Hz ILD < 1.5 dB. The sweep capture (`crew-battle … spatial-sweep`) fails unless its recorded audio pans with the turret and brightens as the hatch opens. | A rigid sphere, no pinna or torso: no elevation cue, no front/back cue; the hearing model's far-ear band table (SL §4 [E]) is 2–5 dB deeper than the sphere from 500 Hz to 2 kHz (within 3 dB at 4 and 8 kHz), so the two head shadows (dose and waveform) disagree there. No measured HRTF is bundled (no permissively licensed set was taken in; the stage is one filter pair per voice, built to be replaced). The 30 % plate retention and the 45° aperture elevation are estimates; the hull is one enclosure, not per-plate radiation; loops blend by their pool's spectrum class, not the voice's ear spectrum; one speed of sound at the head (343 m/s, 7 % short of the −20 °C air's ITD). Game bearings are horizontal: elevation is carried but unused. The presentation pan (and `widened_bearing_deg`) are owner-directed departures: at exactly 90° the far ear is silent by the pan law, whatever the head model says. Gate: `acoustics::spatial::tests`, `audio::vehicles::tests::exterior_vehicle_direction_follows_the_listeners_hatches`, the sweep receipt's `spatial_proof.passed`. |
| Traced room acoustics (`acoustics::raytrace`, `game_acoustics`, 2026-10-28) | World voices and foley are heard in a reflected field traced through the scene: stochastic energy ray tracing per octave band (geometric acoustics), the Tiger's 16 articulated source frames voxelized at 5 cm in their own frames, the marched terrain, cover boxes within 150 m; handbook absorption and scattering per material [E]; ISO 9613-1 air per band; receiver sphere plus diffuse rain (shadow-ray next-event estimation); a binaural IR from band noise shaped by the echogram, FFT-convolved and added to the dry path. Cached per listener cell, hatch state, turret bucket and source bucket; a fixed ray budget per update (48 High / 16 Laptop) keeps runs deterministic. Measured in `sound-world`: buttoned cabin +8.8 dB reflected at 1 kHz with T20 0.17 s; cupola open +4.0 dB, 89 % of rays out; head out −7.4 dB; a brick wall 3 m to the right adds ~20 dB over open snow and returns from the right. | Geometric acoustics: no diffraction, no transmission through plates (the dry path keeps SL §8's insertion loss), no modes below ~100 Hz, no phase; early reflections are per-bin band noise, not image sources. 5 cm voxels merge sub-cell features and thicken every wall by one cell. Trees, doors and building interiors beyond their boxes are absent outdoors. Fields come from bucketed stand-in sources, not each source's exact place. Cost bounds, named: the wet path convolves a voice's first 0.75 s; at most 4 (High) / 2 (Laptop) traced convolutions per update, the rest take the parametric tail; a new place uses a neighbouring field, else the parametric tail, until its trace is spent; no trace runs until the cabin voxels, built on a worker thread from world load (~0.6 s), have arrived; MG receiver rounds keep the parametric path. The parametric Schroeder tail therefore remains as a fallback (and in offline tools); it is no longer the shipping room model where a trace exists. Gate: `acoustics::raytrace::tests`, the ignored real-cabin test, `sound-world` receipt `checks`. |
| Library recordings for handling, breech, wounds, materials and footsteps (`audio::recorded_fx`, 2026-10-28) | OWNER-DIRECTED: recorded families (ACOUSTICS.md section 13) at each event's place through the foley path or the propagation queue, without back-to-back repeats, at most six new voices per update. Component damage adds its material under the strike; wounds are one composite per man per strike by the damage model's fragment and blast work; footsteps follow the foley ground under each foot. | Every 1 m level, the breech resampling factors (0.62 / 0.85), the 0.33 s ram-to-close gap, the stride lengths, the wound severity bands and the energy levelling are estimates [E]. The recordings are library foley, not recordings of the objects; a rifle magazine pitched down stands in for a 400 kg breech block. A hand ram's seat cannot be foreseen, so its slide is cut to 30 ms before the strike. Blast work over 150 J stands in for a body thrown against steel (the model does not record what a body struck). Unhooked for want of events: headphones, crates and bins, dome and panel switches, smoke dischargers, run-over bodies, glass and bottles, 12.7-25 mm rounds, random misfires. Gate: `audio::recorded_fx::tests`, `game::game_hit_sounds::layer_tests`, `recorded-fx-reel`. |
| Tiger 88, T-34 report, small-arms strikes, the player's body (2026-10-29) | OWNER-DIRECTED picks (ACOUSTICS.md sections 14-17): the Tiger's 88 from rifle recordings pitched for weight (inside / outside voices), the T-34's report from old rifle shots at 0.72x through the propagation queue, bullet strikes shaped from suppressed-SMG shots per hit, impact_body under the player's own wounds. | None of these sources is a recording of the event: they are rifle and SMG shots re-pitched and filtered. Pitch factors, the hull-colour share (0.33), the peak hold (-2.5 dBFS), the plate-mode table and the whine/tick odds are estimates [E]. The A-weighted calibration lets a hull-darkened enemy report peak near -1 dBFS buttoned. Gate: `main-gun-candidates` and `recorded-fx-reel` receipts. |
| Own-gun voice candidates (`audio::MainGunVoice`, 2026-10-28) | Diagnosis: the shipping shot drives the master limiter 7-15 dB at every listening place. Candidates A/B/C are single voices at the calibrated level with the hull's insertion loss buttoned and the peak held at -4 dBFS (limiter 0.0 dB). | OWNER-DIRECTED presentation: a -4 dB trim for the firer and the peak hold lower the reproduced level under the calibrated one (the ears still take the calibrated level). B and C are composed from rifle and explosion recordings, not a cannon. `MAIN_GUN_VOICE` stays `Current` until the owner chooses. Gate: `main-gun-candidates` receipt. |
| Tank-hit recordings (`audio::hit_recordings`, `game_hit_sounds`, 2026-10-28) | OWNER-DIRECTED: every penetration, knockout and destruction of a tank is one of eight long close-explosion recordings (owner's Rust & Blood library, never the same twice in a row), every non-penetrating strike one of five short ones, fourth-order high-passed from 1.5 kHz (hardness 0) to 60 Hz (hardness 1). Hardness rises with the round's energy (kinetic + filler on the plate, log 100 kJ–8 MJ), cos(obliquity) and penetration / effective thickness; the filter's A-weighted loss comes off the calibrated level. Placed by the existing emissions, propagation, presentation and traced rooms; on the own hull at the SL §5 level through the headset and cabin room. | The recordings are explosions, not recordings of shells on armour; the hardness weights (0.5 / 0.2 / 0.3), energy bounds and corner end points are estimates [E]. The presentation floor can lift a light world strike back up at range. On the own hull the recording's spectrum stands in for the struck plate's structure-borne radiation (no airborne plate loss applied). A penetration's former `ap_hit_tank` layer is gone (one voice at the penetration emission). Gate: `audio::hit_recordings::tests`, `game::game_hit_sounds::tests`, `hit-sounds` receipt `checks`. |
| Foley catalogue (`acoustics::foley`, 2026-10-28) | 314 cues synthesized from contact physics (modal strikes with Hertzian contact, granular breakage, stick-slip friction, Minnaert bubbles, nozzle and Aeolian flow, formant bursts), each placed at its 1 m reference level [E] propagated from the event's world position through `sound_propagation`, the binaural stage and its traced room; continuous cues as positioned loops. Wired to footsteps on surfaces the recordings lack, landings, handling, doors, stations, climbing, hatches, the cigarette's actions, the extinguisher, first aid, the vehicle's gears, brakes, starter, stall, cooling, MG barrel heat, track ground, crush stops and collapses, weather on the steel and the wind's tones. | Waveforms are models, not recordings of the objects; every 1 m level is an estimate against the existing anchors; mode tables are body-class ratios, not measured Tiger parts; the cold-snow squeak, the Aeolian dipole's U⁶ level law, the ventilator's blade-pass tone and the cooling-tick rates are estimates. Intercom voices, periscope and sight clicks, enemy infantry and allied vehicles remain unvoiced. Not perceptually auditioned. Gate: `acoustics::foley::tests`, `audio::foley_play::tests`. |
| Own engine direction and station level (2026-10-28) | The running bed comes from the engine compartment: outside directly, inside with its lateral angle kept by sqrt(direct share) of a firewall source (Q = 2) in a room of constant 12 m² (from the traced cabin's T20), and its level offset at the station from the same Hopkinson–Stryker law (≤ 0.7 dB across the crew stations). | The room constant is derived from an estimated cabin volume and surface; one source point for the whole engine bay; the running gear stays centred. Gate: `sound-world` receipt `engine_by_station`. |
| Engine, tracks and other vehicles | Own Maybach and tracks at the SL §7 interior law (93/110/115 dB(A)), no "fatigue-safe" cap, ±1.8 dB for torque demand [E]; the loop is measured at seven playback speeds. Enemy tanks (none were voiced before) at the T-34 exterior level scaled to their speed, propagated per frame, the four loudest voiced. | Every enemy type uses the T-34 calibration and a synthesized V-2/track loop; the per-band transfer of a vehicle voice is reduced to one one-pole low-pass; allied Tigers are not voiced. |
| **Reproduction transform (not physics)** | The tone map of SL §11 (L ≤ 110 dB(A): 1:1; 110–150: 2:1; above: 4:1) maps each voice's computed ear LAF to its short-term digital level; one global exposure puts the owner-verified buttoned own-gun recording's peak at −1.5 dBFS (measured crest 15.69 dB, so the curve's −30.5 dBFS(A) becomes −15.1); recordings and synthesized voices are normalised by their own measured LAF,max (IEC 61672 A-weighting, fast 125 ms); a lookahead true-peak limiter (4× interpolation, −1 dBFS ceiling, 1.5 ms window, 200 ms release) on the master output bounds the sum, live and in capture. Review films (`mechanic-review`) export the master output at unity (the former fixed 0.5 export gain is gone), and `impact-mix-review` renders every key event through the live graph and asserts the calibration's dB relations (own gun buttoned peak −1.5 dBFS and 17.4 dB (LAF) over the cross-country bed; every unlimited voice within 1.5 dB of the curve; nothing above −1 dBFS), and, heard through the ears, what the middle-ear reflex may and may not take (row below). | This is a reproduction transform like ACES and exposure, not acoustics. It is applied per voice, not to the sum: two equal overlapping events sum 3 dB higher than the curve would give their combined level. The limiter's 200 ms release lowers a limited event's short-term level too: the head-out gun plays ~2 dB (not the curve's 6.5) above the buttoned gun; the ear model carries the rest. The weather voice is normalised continuously (8 s); foley keeps its former call-site volume as a relative effort around an estimated class level. Gate: `reproduction::tests`, `audio::calibrated::tests`, `audio::master::tests`, `impact_mix_review::tests::every_calibration_relation_holds_through_the_live_graph`. |
| Ear model (`hearing`) | Driven by the pressure at each ear, per impulse: every own shot (88, MG34 per round, P08), every strike on the own hull, every propagated report, burst and crack hands both ears its incident peak (head shadow on the far ear, +4 dB for the ear facing the source), A-duration and spectrum class. SL §12 runtime fallback: duration credit 0/5/9 dB, a 4 dB-per-decade trade, TTS2 at 4 kHz = 2·(X − 157) (median ear), the §12 frequency shape (½ octave lower for low-frequency blasts), log-time recovery over 16/24/72 h, a third of the 24 h shift kept after acoustic trauma (TTS2 > 40 dB), a 2 h engine accumulator towards LAeq − 85, eardrum rupture Φ[(ln p − ln 100 kPa)/0.64] of the incident (reflected for a facing ear) pressure with a 30 dB conductive loss, tinnitus onset/level/pitch per §12, the middle-ear reflex already contracted when an impulse arrives (row below; it protects the next impulse, never the first). Impulses below an effective 140 dB add no dose. Real recovery is the default. It replaces the former authored exposure (accumulate/recover with a 240 s constant), the world duck after hits and the synthesized "ear fatigue" wash. | This is the report's [E] fit to measured anchors, not the AHAAH model it prefers: anchors met within the test bands (25–50 rifle shots 16 dB; 7 × 17-pdr ~51; 40 × 88 buttoned ~7; 5–20 × 88 head-out 38–42). The report's prose says 20 dB recovers to ~15 dB in 30 min; its own recovery law (implemented) gives 11.2 dB; the law is kept and the discrepancy recorded. One median ear (G = 2), no susceptibility spread; rupture and persistent tinnitus are deterministic draws from a counter hash. The engine's continuous level comes from the live audio block, so a device-less run accumulates none (and drives no reflex); only the ear state depends on it, which such a run cannot play. Gate: `hearing::tests`. |
| Middle-ear reflex (`hearing::Reflex`, 2026-09-29) | Sources retrieved for it: SL §9a. Driven by the short-term A-weighted level at the ear: a voice that carries an impulse (the own gun, P08 and Panzerfaust recordings; strikes and bullets on the own hull) by its own envelope as heard (5 ms blocks from where its sound begins, placed at the impulse's LAF); an impulse without a voice of its own (a propagated report, burst or crack, an MG round) at its LAF decaying at 120 dB/s; and the running engine's level at the ear as a continuous drive. Demand 0.6 dB per dB above an 80 dB(A) threshold (Dancer 1991) up to 15 dB (Brask 1979; Borg 1968), full below 0.7 kHz and half to 1.4 kHz (Borg 1968); a rising-only 125 ms integrator (Møller 1962's envelope detector and integrator); a 25 ms dead time (Dancer: 25–35 ms at high levels); first-order onset and recovery of 65 ms for impulses and 113 ms for sustained sound (Jones, Greene & Ahroon 2019); the sustained part adapts toward (drive − threshold)/30 dB of its demand with a 10 s time constant (Dallos 1964; Borg, Nilsson & Engström 1983; Lidén 1964; Kaplan 1977). It replaces full strength 30 ms after an impulse and a 2 s half-life (Lidén's adaptation under continued sound misread as relaxation). Result: 32.5 ms latency for the own 88 and 205 ms 2 dB above threshold (Dancer: 25–35 and 150 ms); 10.2 dB 100 ms after the shot, 14.5 dB at 250 ms, gone within a second; impulses three times a second meet a contracted reflex, once a second a relaxed one (Dancer: protection above 2–3 per second); a cross-country bed holds 12.5 dB, the road 15 dB, an idle 1.6 dB after a minute. The listener stage follows it between updates: each update publishes the contraction, how long it holds and its rate to the next, and every voice applies it as its own factor on the two lowest bands. Impulses reach the ears where their voices' sound begins: the ears advance last in each frame, and a voice's impulse at its measured onset (−40 dB re peak; the verified gun recordings open with ~50 ms of near-silence). | Estimates [E]: the 80 dB(A) threshold on an A-weighted drive; the 120 dB/s decay of an impulse without its own voice (the turret's reverberation, SL §1/§5); the integrator; the half share at 0.7–1.4 kHz; the adaptation law. The 65/113 ms time constants and Lidén's statements come from a secondary summary (Wikipedia); their primaries, and the rise-time tables of Hung & Dallos 1972, Church & Cudahy 1984 and Qiu & Stucker 1998, were not retrievable. Not modelled: the speed-up with level within a class (Hung & Dallos 1972), Dancer's fatigue over minutes, the underdamped response to 400–800 Hz carriers, ipsilateral/contralateral differences, the tensor tympani, individual variation (brief sounds elicit the reflex in only ~20–80 % of people, Deiters et al. 2019; the model is one reflexive median ear), and any drive but impulses and the own engine (wind, enemy vehicles, the MG between its rounds). The player's own reflex also acts on the reproduced sound at playback level; the overlap is not removed. A recording drives the reflex by its own envelope, which is not the physical pressure history of an 88 (the exterior recording stays near full scale for ~0.8 s; a real blast in the open is over in tens of milliseconds): the reflex follows what the listener hears, not what the gun would have done. Measured through the live graph (`impact-mix-review`): the verified buttoned own-gun recording through a fresh ear plays LAF −20.8 / peak −1.95 dBFS, 3.6 / 0.45 dB under its reproduction level: its short-term maximum comes 266 ms after its sound begins, when the reflex is nearly full, while it is within 1 dB of its peak 20 ms after onset, inside the dead time. Head-out through a fresh ear, LAF −13.8 / peak −1.0 (the limiter takes 3.9 dB, against 5.2 dB without the ears). Fired while driving cross-country, into the reflex the bed holds: LAF −22.4, peak −9.4 dBFS. The films' offline chain agrees with the live graph within 0.1 dB. In the calibration film (`captures/sound_calibration_20260929_reflex_v4.mp4`) the head-out shot plays LAF −13.6 / peak −1.0 and the buttoned shot, fired 2 s after full-throttle driving into a bed still holding part of the reflex, LAF −21.5 / peak −4.2 dBFS. Gate: `hearing::tests`, `audio::ear::tests::the_reflex_follows_its_published_course_between_updates`, `impact_mix_review::tests::every_calibration_relation_holds_through_the_live_graph`. |
| Listener stage (what the ears lose, as heard) | Every voice passes a five-band loudness-recruitment expander (< 0.7, 0.7–1.4, 1.4–2.8, 2.8–5, > 5 kHz): a band loses the full shift at threshold and none at 100 dB SPL, linearly between (SL §12); the reflex takes its contraction from the lowest band (half from the next) on its own course (row above), not on the expander's gain ramps; a ruptured ear takes its conductive loss. The fresh strike on the own hull plays untouched for 120 ms before following the ears (a named approximation: it keeps the strike's own onset from being heard through the damage it causes, but it also exempts those 120 ms from damage already present, e.g. a ruptured ear's conductive loss). Tinnitus is a one-third-octave band of noise at the modelled pitch, never a sine, placed 15–25 dB above the player's own threshold at the assumed playback calibration (bed at 78 dB(A)), on the trauma bus where only the KIA gate applies. The offline review films and the audition reel run the same chain (`audio::OfflineMix`: voice placement, ear timeline, listener stage, tinnitus, master). | Approximation named: the band level that drives the expander is estimated per voice from its own digital level through the inverse tone map (exact below 110 dB(A)); a voice does not see the other voices in its bands, so masking between voices is not modelled. The level detector averages over 10 ms with an onset detector (0.3 ms attack, the peak power less 6 dB) that re-polls the gains at once, so a loud transient born into a damaged ear keeps its onset within ~1 dB over its first 3 ms (tested) where the cochlea acts within a cycle. Tinnitus loudness relies on the assumed playback level, not a measured one. Hand-scale controls, foley and the traverse are placed at estimated levels "as heard", not through the headset model. Gate: `audio::ear::tests`, `audio::tinnitus::tests`, `audio::offline::tests`. |
| Passing rounds (near-miss cracks) | Every supersonic round, shell or bullet, is tested once per 240 Hz flight step against the segment it actually travelled (`passing_crack`): the Whitham N-wave (K = 0.53) at the listener's miss distance, emitted where the shock that reaches the listener leaves the trajectory, b/√(M²−1) before the abeam point, so a firer behind his own muzzle or a listener beyond where a round stopped hears no crack; it arrives b·√(M²−1)/(M·c) after the closest approach, from the emission point (up-range of the abeam point), through the hull, headset, air and the ears. The voice is a synthesized N-wave of the computed duration. It replaces the recorded near-miss sample played at the closest point when a round that missed struck the ground. | The speed and Mach number are the step's; the N-wave has no separate ground-reflected arrival; K = 0.53 reads 0–2 dB below numerical integration (SL §3); cracks under 90 dB peak are not voiced (and carry no dose). The arrival is quantised to the frame (≤ 16 ms at 60 Hz) plus up to one flight step. Gate: `game::listener_hearing::tests`. |
| **Removed pending simulation** | The outgoing-shell "shriek" heard head-out (a firer behind a Mach 2+ round hears its blast, never its shock) and the random horizon booms every 6–20 s (no simulated source) are off in both presets, reachable only through `KESSEL_DEBUG_SHELL_SHRIEK` / `KESSEL_DEBUG_AMBIENT_BOOMS` in a debug build. The booms return only with simulated off-map indirect fire (artillery, mortars) whose shells, bursts and reports are propagated like any other source. | Gate: `game::tests::shell_shriek_and_ambient_booms_are_behind_their_debug_gates` and `debug_gates::tests`. |
| Debug gates (`debug_gates`) | `KESSEL_DEBUG_HEARING_TIME_SCALE` (time compression of hearing recovery, named as such), `KESSEL_DEBUG_SHELL_SHRIEK`, `KESSEL_DEBUG_AMBIENT_BOOMS`. A release build never reads them; a debug build opens one only on an explicit opt-in; the process prints "NOT A SHIPPING CONFIGURATION" when any is open. | Gate: `debug_gates::tests::every_gate_is_off_in_both_shipping_presets` (Laptop and High, release and debug without opt-in). |
| Structure collapse and authored emitters (2026-09-29, merge of the calibrated sound with the urban intro) | A structure's collapse beat (one per structural event: a legacy wall, or a compiler delta that fails building modules, as when a tank grinds a wall down or a shell drops one) is heard as masonry falling: `sound_levels::PROP_IMPACT_1M` + 16 dB (`STRUCTURE_COLLAPSE_OVER_PROP_DB`), spherical spreading, through the ordinary propagation, headset and ears. Head-out with the headset: 121 dB peak / 93 dB(A) at 2 m, 112 / 84 at 5 m, 92 / 61 at 20 m, below the ear model's 140 dB dose threshold. Before the merge it borrowed the vehicle-kill blast of its voice (0.71 kg TNT: 198 / 167 at 2 m, 182 / 150 at 5 m, the own 88 head-out's level), so every wall the Tiger ground down in the district reached the ears as a detonation (measured on the district during a head-out crush: repeated limiter-level −1 dBFS peaks, LAF,max −21.6 dBFS(A)). | The +16 dB is an estimate [E]: a kit wall bay (~8.6 t of brick) dropping its centre of mass ~1.3 m releases ~110 kJ, about ten times a felled trunk heard at +6 dB, with radiated sound taken to scale with energy. One level per beat however much falls with it (a storey is heard as one bay); the voice is still the kill-explosion recording. Authored scene emitters (`SceneEnvironment`: the courtyard workshop's two generated cues and the urban tableau's slot wind) play at their authored gain through the World bus's listener stage, not through the per-voice calibration and tone map (the master limiter still bounds them). Gate: `game::tests::a_collapsing_structure_is_heard_as_falling_masonry_not_as_a_detonation`. |

## Repair interaction limits — 29 September 2026

The carried hammer now responds to the ordinary post-pickup LMB trigger independently of repair
eligibility. Its animation is not a swept hammer-head collision solver. A repair still uses a
reachable target point (1.65 m reach, 22 cm look-ray radius) and retained-scene line of sight
with a 28 cm endpoint tolerance. The traverse wheel itself is not part of that contact mesh.
Repair credit is an uninterrupted held-input timer; named procedures such as recoil-fluid refill,
boresighting and fuse replacement do not yet simulate their individual tools and operations.
Only eligible targets receive repair work and the repair-contact cue. The existing wordless
interaction dot identifies an eligible interior repair target instead of appearing everywhere
merely because tools are held. The track-pin ritual retains its own timer and hit accounting.
See `REPAIR_INTERACTION_20260929.md` for field-fault distinctions, supplies and remaining gaps.

## Sound source corrections — 29 September 2026

See `SOUND_MASS_20260929.md` and `research/IMPACT_STRUCTURAL_DECAY_20260929.md` for
implementation, source provenance and bounded evidence. These are source-waveform and
reproduction corrections, not measured Tiger acoustics.

- The procedural own-engine now runs continuously at output rate: only its combustion
  clock follows RPM, while the inherited structural/filter frequencies stay fixed. The
  complete source is normalized once against its measured LAF across seven rates.
  The engine level law, headset, reflex and listener model remain; optional recorded
  loops retain their former path. The coefficients, pseudo-gearbox and scalar enclosure
  transfer are still estimates. Separate travel-driven track contacts are not implemented.
- Structural impact modes use an explicitly inferred effective loss factor of 0.01,
  retaining existing support/breach factors. The formula is a damped-mode approximation,
  not geometry-solved hull vibration. A confirmed internal detonation selects broadband
  burst/rupture energy with subordinate ringing; dry penetrations and casualties alone do
  not manufacture an explosion. Actual filler/ammunition state gates both the waveform
  and the existing ear-level law. No duplicate calibrated blast or cosmetic fragment
  shower is added; normal fragment contact sound still uses its limited live event budget.
- The master limiter's release control uses f64 to avoid a tiny stuck residual gain.
  Its ceiling and 200 ms law remain. Receipts now expose current gain reduction and frames
  over 1 dB and 6 dB; old `limited_frames` totals counted any residual reduction and must
  not be read as the duration of audible pumping. This is a measurement/correctness fix,
  not a claim that the residual itself sounded hollow.
- Already emitted world sounds continue their existing travel and propagation during the
  terminal fade in native play and recorded encounters. Fire cookoff previously queued its
  voice after the final ordinary queue drain, so death prevented it from playing at all.
  Only acoustic arrivals and the listener state advance after death; no new gameplay event
  is generated. The existing generic cookoff/rack source calibrations and enclosure transfer
  are retained estimates, not a solved internal pressure history. Fixed fire films preserve
  every rendered frame and pre-fatal audio sample while delivering the queued cookoff once.
- Player-view and subjective acceptance are recorded separately in the sound review
  document. A matching digital LAF or passing unit suite is not headphone/speaker approval.

## Cupola raster direction correction — 29 September 2026

The seven commander cupola relays now use the actual observer pupil and preserve incident
ray directions, matching the existing reconstructed four-lens/two-mirror source path. The old
centered virtual pupil plus entrance-normal-to-horizon rotation lost the held eye's upward
direction. On left-front port `Turret_0139`, retained roof cap `Turret_0116` then covered the
centre as well as the lower field. Source tracing clears the centre and retains the lower cap.
Neither the cap nor any glass/casing geometry is removed, translated or made transparent.
Ordinary off-axis cupola views also use their actual incident direction, so their image changes
with eye position instead of being forced to the horizon. The explicit registration flag is
false for loader, driver, radio and gunner optics; their prior mapping is unchanged.

The raster implementation still samples every direction from one objective-centred exterior
pupil offset 8 mm beyond the glass, rather than each physical ray's exit point. Near-field
silhouettes can differ from the source trace. Finite internal aperture rejection and glass/lens
aberrations remain absent, and no general numerical error bound is claimed. The verified scope
is the angular mapping, retained source obstruction and tested held fields; it is not exact
periscope light transport or a historically measured prescription. See the focused and seven-port
fixtures `cupola_lower_field_20260929.v1.json` and `cupola_seven_fields_20260929.v1.json`.

## Cabin practical source consistency (2026-09-29)

The dome, four status lights and active breech light derive their visible mesh and positioned
source from one finite-cube record (`interior_rig::FiniteCubeLamp`): sRGB-decoded spectrum,
current emission/state, cube area and circumscribed radius. The isotropic source preserves the
visible diffuse cube's integrated flux (`I = L A / 4`), replacing unrelated hand-set strengths.
Dome flicker changes both outputs. Visible fixtures and their energy persist independently of
the camera station. TPZ and ignition retain their explicitly authored source power pending
registration of their emitting surfaces.

Raster surfaces and local media share eight stable fixture shadow identities. Only the actual
emitting mesh is omitted from its own map; other lamps and machinery still cast. The 5 mm
near plane retains the fan 27 mm above the dome point, formerly clipped at 35 mm. Existing
512-square faces/8 m range and surface receiver-plane filtering remain; the two depth banks
use 96 MiB. Inactive fixture slots do not reorder identities. The same source inputs and
occluder rules are used by shader High and Laptop.

Named limits: finite-cube total flux is represented by an isotropic point, not its exact angular
or near-field distribution; smoke tests visibility once at its optical-depth centroid rather
than along every step; physical High still shortens its source ray by the circumscribed source
radius and may skip blockers inside that radius. TPZ/ignition radiometry, the existing 50 mm
raster attenuation floor and other unshadowed positioned sources remain debt. The source
radiances are authored, not measured historical fixtures. No material, exposure, sky or
postprocessing darkness adjustment accompanies this pass. Evidence, errors and performance:
`CABIN_ATMOSPHERE_20260929.md`; gates: finite-source energy tests, cabin projection/exclusion
tests, full composed raster shader validation, and identical-input player-view films.

## Open finding — the player Tiger returns far less radiance than its albedo implies

Recorded 2026-09-15 with measurements so this can be picked up without re-deriving it.

Measured on the September raster backend; not yet re-measured on the raymarch backend, whose
material atlas (`gltf_material_channels.rs`) encodes the same factors on its own path.

Measured on the act-I chase camera (`cargo run -- ride`), sampling the hull region against the
snow in the same frame:

| Surface | Mean display luminance | Fraction at exactly 0.0 |
|---|---|---|
| Snow (terrain) | 0.689 | 0.0% |
| Tiger hull | 0.036 | 92.9% |

A dielectric at the hull's authored albedo (0.092 linear) under the irradiance that puts snow at
0.689 should land near 0.075. Exactly-zero across 93% of a lit surface is not a dark material, it
is an absent term.

What has been ruled out:

- **Albedo.** The loader was double-applying the sRGB transfer to every `baseColorFactor`-only
  material, so the hull's authored 0.092 reached the shader as 0.0074. That is fixed
  (`gltf_load::linear_to_srgb_u8`, with round-trip tests) and the tint now resolves to the correct
  86/255 sRGB byte. The render did not measurably change.
- **Metallic.** The bake omits `metallicFactor`, which defaults to 1.0 per spec, but the loader
  already discards factor scalars when there is no metallic-roughness texture and substitutes a
  fully rough dielectric. The surface is not being shaded as metal.
- **Missing sky term in general.** Textured world geometry lights correctly in the same frame: in
  `captures/crushchk_before.png` the PSX brick wall a few metres from the Tiger is legibly lit and
  textured while the Tiger beside it is near-black under identical illumination.
- **Overcast.** `KESSEL_OVERCAST` (debug-only override, added for this) opens the cloud deck to a
  clear sky with a full direct solar beam. The hull moves only from 92.9% to 90.3% at zero. It is
  receiving neither the sun nor the sky.

Remaining suspects, in the order worth checking: per-view self-visibility instance/ray masking of
the player's own vehicle leaking into its shading rather than only its camera visibility (the
first-person design means this surface is normally never seen, so such a bug would hide); and the
3.3M-vertex full-interior hull mesh interacting with the exact-hit reconstruction differently from
ordinary props.

This matters beyond the chase camera: the same masking would affect what the Tiger contributes to
reflections and to the light leaving it onto the snow around it.

## Mandatory gates

1. `cargo fmt --all -- --check`, release compilation, full tests and `git diff --check`.
2. Hal/RTX 4090 compiles every hardware ray-query shader and renders a High frame; `rtprobe` alone
   is insufficient because it does not compile the scene combine/volume path.
3. A source-contract test rejects reintroduction of screen-space GI/AO, ambient floors, aperture
   lights/cones, view-state energy, unshadowed proxy light, x-ray/outline/glass stipple, and
   trigger-time projectile verdicts.
4. Laptop and 4090 comparison uses identical scene/simulation inputs. Allowed differences are
   resolution, sample count, history/convergence, denoising and update cadence only.
5. Every remaining simplification is named here with its source, measured bound and replacement
   gate. If that information is absent, it cannot be described as exact.
6. Every opaque object visible in a shipping High frame is represented by the same source triangle
   and current transform in the ray scene. Per-view self-visibility uses instance/ray masks; it may
   never remove a crew body or effect from lighting and reflections for every view.
7. A luminous mesh either carries a documented radiometric/thermal emission state that the path
   integrator can hit, or it is non-emissive. A separate unshadowed glow object or point source may
   not impersonate emission from a non-luminous surface.

## Canyon adaptive arrivals and paper survey (2026-09-30)

`canyon_director.rs` changes encounter admission only. Finite resources, reload, penetration,
visibility and tank movement retain their existing laws. New actors use the original triangle
geometry/material/BLAS with fresh identities; wrecks are retained and history storage grows. A
sorted sparse instance lookup is updated without reuploading unchanged triangles or textures;
the geometry and material source IDs remain exact. Compact record capacity does not create hidden
actors, and exhaustion uses the full pack path instead of dropping any physical surface. The
650 m admission buffer, five silhouette probes and measured navigation cells are conservative
siting approximations, **not exact full-mesh visibility or continuous collision proof**. Existing
terrain/structure navigation and wreck occluder debt still applies. Exposed/blocked entries defer
instead of spawning on the player's view. No fog-only hiding, target fixes or difficulty-dependent
physical changes are introduced. The 60–120 minute duration remains a design target.

The Canyon drawing is source-derived cartography printed as albedo on existing paper, with normal
scene lighting. Aerial captures explicitly lower the physical fog density for terrain
inspection; ground/cupola frames retain the authored winter atmosphere. Neither
capture staging nor the aerial camera is presented as a completed human gameplay run.

## Explicit Canyon AI video driver (2026-09-30)

`canyon-ai-run` alone installs `CanyonPilot`; shipping menu play never creates it. This test
driver has privileged route knowledge, geometry-based target sight within 420 m, target-velocity
lead and a ballistic aim solution. It does not model pixel recognition, field of view or fog
recognition. These are AI test approximations, not claims of human-equivalent perception.
It emits ordinary drive, mouse and trigger inputs and native station actions, including the
interior manual reload. It cannot override damage, ammunition, enemy motion or the Canyon
director. It has no automatic repair. A failed/stalled policy is a failed attempt, not evidence
that the level is impossible for a human. Details and reproduction are in
`docs/CANYON_AI_HIGHLIGHTS_20260930.md`.

## Canyon camera and combat-media pass (2026-09-30, generator v6)

The user requested neutral winter white balance, stronger dither, warmer muzzle illumination,
more/directional pale gun smoke, denser blood, red flame edges, yellow fragment trails and
damage-dependent wreck fires. The document's optional white balance is diagonal three-channel
camera reproduction using diffuse environmental irradiance, not changed illumination, spectral
chromatic adaptation or a measured eye model. The shared filmic shoulder scales RGB together;
neutral response is unchanged. The older CPU diagnostic ACES helper remains channel-wise and
is not a promise of pixel equality with this presentation path. Both shipping presets share
the same scene and display equations. Native Canyon media uses the shader backend.

Muzzle power is now calibrated against the legacy cube's radiance and area, including its
mode-zero hot core, instead of an unrelated 420-unit pale light; the enemy 76 mm tier scales
that reference. The raster weapon grain field replaces the visible legacy cube, so the positioned
source is **not** an exact aggregate of that grain field. Angular distribution, shadow coverage
and near-field transport remain existing emitter debt. Internal catastrophic blast calibration
is shortened/reduced; persistent fire and moving-chip light remain their existing source-cluster
approximations. This is authored VFX calibration, not a validated explosion model.

The pale muzzle plume is a bounded raster presentation: a captured barrel axis, prescribed
forward expansion, delayed 12% wind entrainment, buoyancy and grain lifetimes. It does not simulate
compressible gas, turbulent entrainment or collisions of the smoke with scenery. The legacy
physical-backend volumetric muzzle puff is unchanged; plume geometry parity across backends is
not claimed. Additional fire soot and static/dynamic blood budgets are presentation changes,
not measured soot mass or conserved blood volume. Static connected cells share buried faces
and follow the real source heightfield. Wound/contact positions and injury outcomes are preserved.

Enemy wreck fire strength uses actual penetrating-round residual/filler energy, the existing
vented-turret/cook-off state and an authored normalized fuel-decay law. Constants are not historical
measurements; the existing stochastic catastrophic classification remains. Archived campaign
hulks and dead allies retain their older fixed strengths. LAN clients initialize an estimated
severity because the snapshot does not yet replicate the new reservoir. This pass is validated
for single-player Canyon, not new multiplayer fire-state parity.

Recorded fragment positions now survive the replay's zero-time rebuild. Motion history is only
presentation and contributes no duplicate source power. Replay time is deliberately nonlinear
to show entry/spall motion; it does not reapply a hit. No synthetic residual beam or casualty
event is added. Details and captured evidence: `docs/CANYON_EXTENDED_20260930.md`.

### 2026-10-01: projectile whistle, severity, sparks, turret and surface fire

The shell whistle is an explicitly requested authored timbre. Real swept cannon segments supply
its lifespan, position, speed, caliber, mass and filler fraction; no visual tracer can extend it
past terminal contact. A continuous voice retains oscillator/filter state across 240 Hz steps.
Changing acoustic filters use warmed parallel cascades and a ten-millisecond output crossfade;
the final filter taper fits within the real terminal segment.
Pitch variation and a restrained radial-motion bend are sound design, not measured wartime
whistle data or a supersonic Doppler solver. Level is an authored 115.5 dBA at ten metres for the
reference round. Shared air, snow, barrier, hull and headset transfer apply. Its playback stream
holds the initial range/c delay so later samples remain contiguous; moving-source retarded-time
transport is not solved. The existing conical ballistic crack remains a separate physical gate.

The external recorder now distinguishes ordinary wreck destruction from the authoritative
catastrophic classification, using the same ammunition-detonation and tearing-metal sources as
live play. Heat-triggered detonation of the player ammunition reservoir uses that same
catastrophic pair; ordinary secondary wreck cook-offs remain smaller. First-person review
records the actual live crew audio graph, including enclosure,
headset, hearing and terminal fade, while independent automated co-op inputs operate the gun.
It is an independently simulated encounter per viewpoint, not a synchronized replay or human
playthrough. Enemy catastrophic selection remains the inherited stochastic simplification;
player ammunition detonation still follows the existing internal damage result.

Spark presentation uses more, smaller grains and shorter individual streaks while retaining
original impact positions and motion. Their number and visual scale are authored, not a resolved
fragmentation distribution. The player cabin detonation retains flash and smoke but omits the
duplicate unconstrained exterior spark spray; the authoritative interior fragment solver and
its rendered fragments remain active. A separate crew-view check also exposed armor-impact
ejecta ignoring the struck plate normal. The corrected armor presentation confines those
cosmetic sparks and flash grains to the exterior of the authoritative contact plane. This
local plane is not a full hull or world collision solver for decorative grains. Detached enemy
turrets now use source-triangle-derived thin-shell
mass/inertia and sampled surface contact, instead of random high launch speeds, fast uniform
spin and a fixed landing height. Plate thickness, blast-energy coupling, contact restitution,
and friction are approximations; the source support sampling bounds are recorded at runtime.
See `docs/SPARK_REFINEMENT_20261001.md` and `docs/TURRET_DETACHMENT_20261001.md`.

Exterior fire follows damage-attached source hull/turret patches, including moving detached
parts. Its heat graph, spreading speed, gas wash and reservoir are authored approximations;
steel is not fuel and no CFD is claimed. The source-sampling and connection limits, bounded
geometry/light budgets, and legacy archived-hulk behavior are recorded in
`docs/SURFACE_WRECK_FIRE_20261001.md`. Actual visibility, contact and audio are checked separately
from CPU regression results in the capture receipts.

### Combat soundscape reproduction and running gear — 2026-10-01

- The default `balanced` playback curve lifts quiet voices by up to12dB below110dBA while retaining
  the gun anchor and loud-event curve. `KESSEL_AUDIO_RANGE=reference` selects the previous curve.
  This is an explicit global reproduction transform, not altered pressure/propagation/hearing; its
  inverse is used by the ear stage. Captures record the mode. It has no attack/lookahead or source
  preference and cannot pre-duck a future shot.
- Running gear now samples14 live ground-to-hull points and actual left/right travel. Track cadence
  follows travel and ground changes excite short contact kernels. Stationary tracks create no
  excitation; coasting works with the engine stopped. These are procedural audio approximations,
  **not** simulated suspension forces. Sample placement,13cm link cadence, contact modes and the
  nominal -12dB component relative to the existing drivetrain law are authored estimates. Source
  input displacement is measured, but acoustic calibration/error against a real Tiger is unknown.
  The vehicle's existing upright terrain-following physics is unchanged.

## Cabin density consolidation, 2026-10-01

The approved playable cabin scattering coefficient0.010 is now shared by the consolidated
raster medium, physical smoke density, source-to-surface extinction uniform, particles and
fine-grain attenuation. Source-derived air domains and geometry visibility are retained. This
restores the accepted cabin density; it does not brighten emitters or suppress outdoor fog
through apertures. Native consolidated review remains required.

### 2026-10-02 personal hand-prop source boundary

The cigarette/packet/lighter/flashlight implementation reuses existing Pizza source
meshes. `docs/HAND_PROPS_SOURCE_20261002.md` records the authored physical scale,
source-derived packet opening, source-atlas triangle lens partition, shared lighter
point-light anchor and held-item hardware-TLAS exclusion. Cigarette shortening
clips the burning ring and UVs, preserving filter position and radius. Ember is an
emissive cap with negligible remote illumination omitted; no GI claim is made.
The existing geometry shadow-map path remains authoritative for the lighter and
flashlight. Eight source/pose CPU checks and all15 native cigarette ritual mechanics
checks pass. Cabin stills remain visually unresolved: the props' camera-facing
surfaces are too dark to read. Texture/normal import checks found no source fault;
the forward path omits reflected practical-light bounce. No fill/emission/shadow
exception was added. A daylight material check and future cabin-light transport
refinement remain separate acceptance work, as recorded in the hand-prop note.

## Bounded transport (2026-10-03)

| Bound | What it does | Where | Error |
| --- | --- | --- | --- |
| Cabin fixture reach | The registered cabin practical lamps (dome, dial illuminator, status lamps) transport light and cast source-geometry shadows out to 12 m. A receiver farther away takes exactly zero from them. Their shadow cubes are rendered and sampled in the turret's frame and are redrawn only when an admitted caster moves relative to the lamp (1 mm, 0.2 mrad tolerance against the pose last rendered). They admit only casters lying wholly inside the reach, so the terrain and district-sized batches do not cast for these lamps. Every other point source keeps the whole-scene range. No lamp is ever unshadowed. | `src/cabin_shadows.rs` (`CABIN_FIXTURE_REACH_M`), `shaders/cabin_visibility.wgsl` | At the bound a lamp delivers 1/144 of its 1 m irradiance, so the step is 0.7 percent of an already dim source. A caster may be up to 1 mm from its drawn shadow before the cube is redrawn. A changed cube is redrawn at most 20 times a second (`CABIN_FIXTURE_UPDATE_HZ`), so a moving caster's shadow from these lamps can lag by up to 50 ms. Before this bound each lamp's six shadow faces redrew every world caster on every frame the tank moved (about 47,000 shadow draws per frame in the canyon). |

| Sub-texel movers in the coarse environment cascade | A moving caster whose largest extent is under one texel of the whole-scene visibility cascade is not drawn into that cascade. It is still drawn into the fine cascade around the viewer and into every lamp and sun shadow. | `src/sky_visibility.rs` (`below_coarse_texel`) | Such a caster could cover at most one coarse sample, and only when it happened to straddle a sample centre, so it was aliasing rather than resolved occlusion. Removing it removes that flicker and about half of the per-frame environment-visibility draws. |

| Key direction of the environment-visibility atlas (2026-10-28) | Each key tile takes the key light's direction rounded per component on its own lattice, half / (384 (half + 10 m)): 0.8 mrad for the local tile inside the tank, 1 mrad outside (capped so the shaders still recognise it as the key, within 1.41 mrad), 2.6 mrad for the 10.5 km coarse tile. | `src/sky_visibility.rs` (`key_direction_step`, `quantized_key`) | The rounding moves a caster at the tile's full depth sideways by under 0.44 of that tile's texel. Was 0.1 mrad for both, which the Canyon's compressed day crossed every frame. |
| Held footprints of the environment-visibility atlas (2026-10-28) | The local footprint keeps its lattice point until the eye is 0.75 of a lattice step away along an axis (was 0.5): its centre may lie 0.42 m (inside) or 1.13 m (outside) from the eye. The coarse cascade's fit carries 256 m of slack and is kept while it holds the loaded scene and the camera's far support and is at most 512 m looser than an exact fit. | `src/sky_visibility.rs` (`held_focus`, `held_coarse_fit`) | A coarse texel is up to 2.7 m wider than an exactly fitted one (5 % of 54 m); the local footprint covers 0.14 m (inside) less of the hull on its far side. The atlas no longer redraws whole cascades when a jammed tank rocks across a lattice line. |
| Soviet 4BO paint (2026-10-28) | The T-34's body and fittings, and the code-native Soviet rigs, are painted FS 595C 34102 (the chip matched to the Soviet 4BO colour card; CIELAB D65 37 / -5 / 11, sRGB 86/89/69, linear 0.093 / 0.100 / 0.060) at roughness 0.82; each tank's batch lies between it and the chips matched to surviving samples, FS 34095 and FS 34096. | `src/tank.rs` (`t34_paint_finish`, `SOVIET_4BO_SRGB`), `shaders/rm_gbuffer.wgsl` (`RM_4BO_*`) | No instrumental measurement of surviving 4BO is published; an FS chip match is a visual match on a standard card, and the card's colorimetry is tabulated to whole CIELAB units. |
| Other vehicles' finish (2026-10-28) | Every enemy vehicle and wreck, and the friendly Tigers, take a per-vehicle seed and fixed loads (mechanical wear 0.35-0.8, mud and grime 0.45-0.95, in winter a 40 % chance of a scrubbed whitewash 0.4-0.9) drawn as authored masks in the mesh's frame, with the Tiger's worn edges and corner grime in the shade pass for the nearest 16 within 250 m. | `src/vehicle_finish.rs`, `shaders/rm_gbuffer.wgsl` (`vehicle_finish`), `shaders/rm_shade.wgsl` | The loads are estimates for a winter-1942 front, fixed for the vehicle's life, not accumulated from its history; the whitewash share is an estimate of mixed practice, not a count. Instanced scenery wrecks and the abandoned Tigers are not included. |
| Unauthored GLB roughness and metal (2026-10-28) | A source material with a constant roughness below 0.3, no roughness map, a metallic factor under 0.5 and not glass (by name or blend mode) is drawn at roughness 0.75, the Tiger's paint: the audit of all 2,484 materials in 1,461 GLBs found 104 such, 92 at exactly 0 (the wreck library's KV-1, T-34-85, Panther and Tiger II, the live T-34's source, the PSX kit's planks and panels). A base-colour textured material with a constant metallic of 0.5 or more and no metallic map is a dielectric (one case: the frozen casualty's body, metallic 1.0). Untextured metal (rifle steel, cartridge brass) keeps its factors. `KESSEL_GLB_SOURCE_ROUGHNESS=1` keeps the source. | `src/gltf_load.rs` (`source_roughness`, `source_metallic`) | A rule on exporter defaults, not a per-asset measurement: a genuinely polished untextured dielectric under 0.3 would also be roughened (none was found that was not glass). |
| Wreck library finishes (2026-10-28) | The wreck library's KV-1 (`kv1.glb`, `kv1_wreck_detail.glb`) and T-34-85 take the T-34's 4BO paint and the track steel; its Panther and Tiger II take Panzergrau (sRGB 72/74/78, the Tiger's own swatch) at roughness 0.75 and the track steel. Their source bodies had no base colour (glTF white) at roughness 0, a white mirror. | `src/tank.rs` (`wreck_library_finish`, `soviet_wreck_finish`, `german_wreck_finish`) | The Panther and Tiger II did not exist in the winter of 1942 (the owner's library); in grey they read as the period's German armour. A KV-1's factory green is close to but not certainly 4BO. |
| Worn edges by pixel coverage (2026-10-28) | The shade pass's worn-to-steel edge (`rm_shade.wgsl`) covers the share of the pixel a 12 mm worn strip fills (`RM_WORN_STRIP_M`): the full effect where a pixel is under 12 mm across, a 5.5 cm pixel at 16 m and 480x270 a fifth of it. Before, every crease of a distant tank was a pixel-wide steel rim (the "weird highlight" along the KV-1's and T-34's edges). | `shaders/rm_shade.wgsl` | The strip width is an estimate of how far paint wears back from an edge. |
| Exterior media out of the cabin air (2026-10-28) | A media parcel whose centre lies outside every cabin domain (a burst's glow, a fireball, smoke, a cook-off's flame) adds nothing to the froxels in a cabin's air; the cabin's own fire grains lie inside and stay. | `shaders/rm_volume.wgsl` (`parcel_medium`) | The armour as a perfect seal for media: a holed or opened hull would let some in. |
| Froxel history after a medium goes out (2026-10-28) | The froxel grid's temporal filter (`rm_volume.wgsl`) drops history more than two to six times brighter or denser than the frame's fresh estimate toward it, at any rate; otherwise new estimates weigh 0.10 in thin air to 0.5 in emitting or dense media, as before. The gun's three-frame flash in clear air had stayed for two seconds as an orange disc beside the muzzle, widened by each frame's reprojection. | `shaders/rm_volume.wgsl` | The ratio's window (2 to 6) is chosen above the jitter noise of the grid's media, not derived; a medium that truly dims that fast within a frame (a lamp switched off in smoke) is followed at once, as it should be. |
| Ricochet, vent and cook-off sparks as streaks (2026-10-29) | A ricochet's, a pressure vent's and a full cook-off's record sparks (`identity.z`: 384 off a cannon round, 72 off a bullet, 192 from a vent, 768 from a cook-off) are drawn as dotted streaks of eight samples along each spark's path over 14 ms, each streak carrying the spark's light; the record's `SPARK_MULTIPLIER` samples per spark beyond those are not drawn. They had filled a continuous sheet that, fanning toward a camera or rising as a plume, drew as a solid wedge or cone. | `shaders/rm_sparks.wgsl` (`SPARK_TRAIL`) | The 14 ms streak stands for an eye's exposure, not a measured one; the spark count per ricochet is the game's effect content. |
| Spark-shower glows (2026-10-28) | The glow a spark shower adds to the air round its emitter is a brief tight bloom: an armour hit's at most 0.5 m for 0.25 s, a pressure vent's 1.2 m for 0.65 s, a full cook-off's at most 2.5 m for 1.2 s (`GLOW_MAX_RADIUS_M`). They had grown to 3 m for 1.6 s and, for a cook-off, to a 60 m sphere rising 120 m for 17 s, each drawn as a soft uniform disc. | `src/renderer_raymarch.rs` (`collect_parcels`) | An authored glow, not a scattering solve; the sparks, their light and the fire carry the shower. |
| Cabin lamps on plinths (2026-10-28) | Every cabin lamp (dome, annunciators, breech, sight, ignition) is a uniformly lit cube, no lens profile, bezel or cage, on a squat dark-steel plinth (1.2 times its width, 0.6 of its half size high) on a flat face of the tank's source mesh, found by a nearest-surface survey and checked by test; positions moved up to 28 cm from where the lamps floated. `Game::set_cabin_lights` switches them, and the TPZ's illuminator, off: dark glass in each lamp's colour (a twelfth of its lit colour), zero light. | `src/lamp_mounts.rs`, `src/game.rs`, `src/interior_rig.rs` | Owner-directed ("a fully lit cube, uniformly emissive, with no border", "on a small base"). The plinths and their placement are authored, not a period fitting; the annunciators and sight lamps are gameplay instruments. |
| Instrument dials and the TPZ illuminator (2026-10-28) | The instrument dials (source material "Illuminated Dial") have black enamel faces (sRGB 40), as period German dials and the surviving Bovington TPZ; no numerals (the source has none). The TPZ's illuminator lights its face as much as the dome lamp a metre off does (it was some 12,000 times that). The driver's ignition lamp is a finite cube at an indicator's power (it was an invisible point at twelve times the dome lamp). | `src/tank.rs` (`DIAL_FACE_SRGB`), `src/interior_rig.rs` (`TPZ_ILLUMINATOR_INTENSITY`, `IGNITION_FIXTURE`) | The face colour rests on period practice and one surviving example; the illuminator's power is set relative to the dome lamp, not measured. |
| A telescope's eyepiece seen from the cabin (revised 2026-10-31) | The gunner's actual planar source lens admits each fragment through a finite exit-pupil disk and apparent-field stop. Moving the eye moves the admitted bright patch; rejected rays leave the source lens opaque and dark. Admitted radiance is not multiplied by the former whole-lens area average. Single-stop pixel coverage integrates a circle against the local pixel footprint, retaining subpixel area; the rare pixel crossing both stop edges uses bounded 8x8 joint quadrature. The separate tracer overlay uses the same pupil. | `src/raster_optics.rs`, `shaders/raster_optics.wgsl`, `shaders/weapon_tracer_panel.wgsl`, `src/tiger_gunner_sight.rs` (`TZF_EXIT_PUPIL_M` 7 mm, `TZF_EYE_RELIEF_M` 25 mm) | Pupil and relief remain explicit estimates, not a recovered TZF prescription. Ideal planar stops do not simulate internal lenses, aberration, a finite human pupil or objective-side exit parallax. Pixel footprints use local derivatives; two-edge coverage is sampled. CPU ray/plane and area witnesses are supplied; new native tests and GPU appearance await parent validation. Other vision blocks retain their separately documented off-axis boundary-image fallback. |
| Retained optical capture metadata (2026-10-31) | A Low-preset skipped layer keeps the camera, texel extent and frame of the color/depth actually encoded. First nonzero use forces initialization; replacing, blocking/repairing or occluding/revealing the port invalidates that cache. The current physical entrance and eye remain current. A finite gunner pupil refreshes if its current visible rays no longer fit the retained capture. | `src/raster_optics.rs` (`CapturedView`, `retained_capture`, `capture_covers_visible_entrance`) | Reduced cadence still produces temporally old exterior motion and objective-origin parallax until refresh. This fixes stale-image/current-projection misregistration; it does not make cached images current or remove the separate vision-block acceptance approximation. |
| Sky cache accumulation (2026-10-28) | Each refresh of a sky-cache texel marches the clouds with a new jitter (an integer hash) and is blended in at 0.3; a sky jump writes it whole. | `src/renderer_raymarch.rs` (`SKY_CACHE_BLEND`), `shaders/rm_sky_cache.wgsl`, `shaders/sky.wgsl` (`cloud_jitter`) | A temporal average over a texel's last few refreshes (about 64 frames): a cloud feature moving across the sky trails over about a second. |
| Open-share firefly clamp (2026-10-28) | A new open-sky share estimate (the ray queries' and the screen gather's) may not exceed three times the same-surface history's share plus 0.02 before it is blended in. | `shaders/rm_shade.wgsl` (`share_clamped`) | Biases a share of 0.3 by under 2 % at two rays a frame; a share that opens suddenly rises at least 1.3 times a frame, so a hatch's light comes up over about a quarter second. |
| Relayed ports' one-pixel effect budget (2026-10-28) | Each relayed port draws at most `relay_effect_samples_per_pixel` (Low 8, High 16, Ultra 32) weapon-spark samples and as many fire and soot grain samples per native pixel of its view. Over budget a spark burst draws its share of samples, never fewer than its source count, each weighted by source / drawn; ground ejecta keep the full count's grain size and take the represented optical depth; hot rims are never thinned. Grains keep two samples per covered pixel of a parcel and drop whole far parcels, counted. Since 2026-11-01 a port's grains take their own rate (`relay_grain_samples_per_pixel`, Low 1, High 2, Ultra 4) without optional density, and over budget every parcel is thinned towards one sample per covered pixel (one per eight in a port) before any whole parcel is omitted (the 2026-11-01 section above). | `src/graphics.rs` (`relay_effect_budget`), `src/weapon_particle_math.rs` (`thinned_counts`), `shaders/weapon_particles.wgsl`, `src/renderer_fine_vfx.rs` | Thinning keeps each burst's emission and optical depth in expectation; the shower is sparser and its per-pixel noise higher (relative noise goes as 1 / sqrt(samples a pixel): a port drew 47 sparks a pixel in one strike before, 8 on Low now). Omitted far parcels are those beyond the budget, usually behind nearer smoke; their projected area is reported. The shader backend's main view keeps its absolute caps. Since the evening each burst first draws at most that rate over the pixels its conservative support covers (`footprint_count`; ground ejecta bounded by their own speed): a distant strike's first frames take a few hundred samples, each standing for more. |
| Clustered occluder in the environment-visibility atlas | A rigid mesh of 160,000 indices or more (the Tiger hull and turret) is drawn into the per-frame dynamic layer of the environment-visibility atlas from a vertex-clustered copy generated from its own source triangles at load: 6 mm cells, the first source vertex in a cell stands for the cell, collapsed triangles dropped. The rendered mesh, collision, sun shadows and lamp shadows still use the source triangles. | `src/sky_visibility.rs` (`clustered_occluder`), `src/renderer.rs` | No vertex moves more than 10.4 mm, under half of the fine cascade's 23 mm texel. Used only when clustering at least halves the triangle count. |

## Owner-directed gameplay departures

| Departure (date) | What it does | Where | Why |
| --- | --- | --- | --- |
| Powered-climb grade share (2026-10-03) | While the player's Tiger drives up a slope under power, only half of the weight's downhill pull is applied (`CLIMB_GRADE_SHARE = 0.5`). Coasting, braking, rolling back and descending feel the full grade. Enemy vehicles are unaffected. | `src/powertrain.rs` | The owner asked for hill speed above the historical estimate so long canyon climbs are not dead time. This is a deliberate, named gameplay value, not a claim about the vehicle. Set the constant to 1.0 to restore the unmodified model. |
| World-sound audibility floor, bass and panning (2026-10-03) | After the physical propagation to the ear, vehicle kills, cook-offs, penetrations and armour or ground strikes keep a minimum level (102 to 86 dB(A) by kind, falling 3 dB per doubling of distance beyond 150 m, by at most 16 dB), their three lowest octave bands lose up to 6 dB less on the path, and every world sound's bearing is widened so a small offset reads clearly left or right. A physical level above the floor is never reduced. | `src/game.rs` (`presented_world_ear`, `widened_bearing_deg`) | The owner could not hear T-34s blow up from inside the buttoned tank, wanted more weight in impacts, and wanted to judge direction by ear. These are presentation values, not claims about sound transmission through armour. |
| Loose brass rides with the tank (2026-10-03) | Spent 8.8 cm cases and machine-gun cases inside the tank's envelope are moved with the turret or hull by that frame's rigid motion each update, then solved against the real source triangles as if the frame were at rest. Inertial forces from acceleration, braking and turning are not applied, so brass does not slide when the tank stops. Outside the envelope a case is a world body; it stops at the terrain height with no bounce. | `src/game.rs` (`carry_with_frame`, `brass_aboard`, `drive_casings`), `src/mg_case_runtime.rs` | The earlier solver treated the brass as world bodies against a hull that moved under them, so the floor and walls struck every case on every update of a drive or traverse and produced an endless clinking. That was not physical either. The debt is the missing inertial forces. |
| Brass sleep and the solve cap (2026-10-28) | A spent 8.8 cm case aboard sleeps after 0.25 s that pass the solver's quiet test, or after 1 s of solved time in which its place in the turret frame stays within 1 cm and its long axis within 0.1 rad, even if it still rocks; a case wedged against two surfaces rocks at about 0.5 to 1 rad/s and never passes the quiet test. A sleeping case is held at its exact pose in the turret frame. It wakes when a collision surface moves in the vehicle's own frame (traverse, gun laying or recoil, a hatch, a shutter, the breech lever), when the turret's up direction tilts more than 2 degrees from where it settled, or when something strikes or kicks it. Driving alone does not wake it. At most three loose cases are solved per update. The others wait, the longest-waiting first and a case in flight before a disturbed one; a waiting case is held where it is and its next solve gets the waited time, but the solver still takes at most 1/30 s per update, so a case that waits longer moves in slow motion until it catches up. | `src/game.rs` (`drive_casings`, `brass_vehicle_frame_signature`, `BRASS_TILT_WAKE_DEG`, `BRASS_SOLVES_PER_UPDATE`), `src/shell_collision.rs` (`DropRest::needs_solve`) | Before this, any motion of the tank's world pose woke every case, so each one was re-solved every update of a drive or traverse. The cost grew with the rounds fired until the per-update triangle budget ran out, up to 26 ms in the integrator's 300 s run. The 2 degree tilt threshold, the 1 cm / 0.1 rad / 1 s stillness window and the cap of three are design values, not measured. Stopping a rocking case is a solver shortcut: the rocking is contact jitter, not a measured motion. Below 2 degrees a case on a slope stays put, though a rolling cylinder could roll at any tilt. While many cases are disturbed at once, as during a long traverse, each moves in slow motion. Machine-gun cases and loose live rounds still use the world-frame wake. |
| Cigarette vision boost (2026-10-03) | Each toke of at least half a second adds ten seconds, stacking to 120 seconds, during which the display grade's exposure is multiplied by up to 1.9 and its saturation by up to 1.18. It fades in over a second and out over the last two. Nothing in the scene changes: no emitter, medium, material or fog value. | `src/cigarette.rs` (`BOOST_SECONDS_PER_TOKE`), `src/renderer.rs` (`boosted_display_grade`) | The owner wants smoking to buy visibility, the game's central resource. It is a display-grade presentation value standing in for a steadier, sharper eye, not a claim about nicotine and night vision. |
| Tiger main gun: one recording, shaped by the path (2026-10-03) | The owner's recording is played through the world renderer for every listening position. Its level at the ear is the calibrated own-gun level. Its colour is the loss in each octave band along the path from the muzzle to the head (air, ground, hull, headset), its direction is the muzzle's bearing from the live head, and its tail is the reverberation of the place. The earlier exterior recording plays as a second layer 6 dB under it, on the same path, with the open-ground tail. For a listener inside the hull a recording made inside a firing tank is added 3 dB under the main one, with only the headset's loss and no added filter or room. | `src/game.rs` (`play_player_main_gun_report`, `own_gun_path`, `OUTSIDE_BLAST_LAYER_DB`) | The 6 dB and 3 dB layer balances are set by ear. Three recordings of one shot are a presentation choice, not three physical sources. |
| Outdoor reverberation from the surroundings (2026-10-03; since 2026-10-28 only the fallback where no traced field exists, see "Traced room acoustics") | The late tail outdoors is built from walls and wrecks within 85 m, the share of the horizon the ground closes off (sixteen bearings at 60, 150 and 320 m), and whether the ground is snow. | `src/game.rs` (`surroundings_reverb`, `terrain_enclosure`, `outdoor_reverb`) | An estimate of late reverberation, not a traced impulse response. There are no discrete echoes off individual faces yet. |
| Presentation pan under the binaural stage (2026-10-27) | The head model (`acoustics::spatial`) renders interaural delay and head shadow for every positioned voice, and the owner's full constant-power pan is kept on top of it, so a source abeam is still in one ear. `HeadModel::PHYSICAL` is the sphere alone. | `src/acoustics_spatial.rs` (`HeadModel::PRESENTED`) | The owner wanted direction judged by ear (row above, 2026-10-03); a bare spherical head gives under 2 dB of level difference below 500 Hz, where the engines and reports live. The pan is a presentation value, not a claim about the head. |
| Open hatches stack (2026-10-03) | With hatches open and the head inside, the hull's loss in each band is the one-hatch figure less 10 log10 of the number of open hatches, never below zero. | `src/sound_propagation.rs` (`loss_through_openings_db`) | Treats the five hatches as equal openings whose transmitted energies add. Their real areas and positions differ. |
| Recordings made at a distance, propagated again (2026-10-03) | Gun reports and strikes 700 m or more from the listener are voiced by the owner's recordings made at a distance. They are still placed at the propagated level and passed through the path's loss in each band, so the distance colour is partly counted twice. | `src/game.rs` (`recorded_gun_report`, `schedule_sound_from`, `DISTANT_RECORDING_M`) | The level is correct; the spectrum is darker than one pass of propagation would make it. The low bands that carry at range are barely affected. |
| Own gun buttoned up, fitted to the listening references (2026-10-04) | For a listener in the closed tank the three gun recordings are filtered by per-band tables fitted so the result has the octave-band balance measured on the owner's two reference recordings, and the main and outside layers are held for 0.9 s (dense scattered copies of the recording) inside a 1.7 s tail. This replaces the hull and headset path for this one sound. Measured result: within about 1.5 dB of the reference from 31.5 Hz to 1 kHz, 4 dB low at 2 kHz, 9 dB at 4 kHz, 13 dB at 8 kHz; level within 7 dB of peak for 0.8 s. | `src/audio_calibrated.rs` (`CABIN_GUN_FIT`, `sustain_roar`), `tools/audio_reference_bands.py`, test `cabin_gun_capture_for_reference_measurement` | A film mix is the target, not a measurement of a Tiger. The hold is a presentation device. The references' loudness relative to the engine drone (about 9 dB) is not reproduced: the game's calibrated levels still decide that. |
| Terrain visibility receiver lift (2026-10-04) | Sky and Sun visibility for the ground is evaluated 0.25 m above the surface along its smooth normal, so the coarse terrain triangles do not each show as a separate shade. | `shaders/stylized_mesh.wgsl` (`TERRAIN_VISIBILITY_LIFT_M`) | A receiver bias. Contact shadow on the ground from anything lower than 0.25 m is lost. The debt is the coarse mesh; a finer terrain mesh would remove the need. |
| Eye inside the hull and at the optics (2026-10-28) | Inside the hull the eye adapts to a quarter of the cabin's log-luminance changes about a reference that follows the meter over 20 s and never lies below the frame's log-average luminance (`observer::inside_meter`, `inside_reference_floor`, `INSIDE_RESPONSE`, `INSIDE_REFERENCE_TAU_S`): a port in view barely lifts the log-average, the whole cabin brightening lifts it at once (before that floor, 2026-10-28 night, a burning cabin stayed clipped for twenty seconds). The scotopic log-average is the photopic one shifted by the field's S/P ratio. Inside, a target more than half a decade above the adapted level is the whole cabin brightening, and the eye's rise speeds to a 0.6 s time constant (`observer::rise_tau`, `DAZZLE_TAU_S`); outside the comfortable rates stand. The TZF's optic pass and the binoculars from inside the cupola have their own eye (`OPTIC_EYE_VIEW`), adapted at once to its field when returned to after half a second (`OPTIC_RESNAP_GAP_S`). The binoculars from inside look from 0.6 m out along the level gaze, past the vision block's glass. | `src/observer.rs`, `shaders/exposure.wgsl`, `src/renderer.rs` (`eye_mode`), `src/main.rs`, `src/game.rs` | The owner wants adaptation as climbing in and out of the tank, not at every sight and port, and found the binocular field blown out. A real eye at an eyepiece adapts to its field over seconds and inside a dark cabin follows a bright port fully; both are departures. Between blocks the binoculars look past the cupola's steel. |
| Eye: range drawn in, slower and wider adaptation (2026-10-04; restricted 2026-10-28) | In the final display mapping, parts of the picture more than 1.5 stops from the eye's adapted level are drawn toward it at 0.32 of their distance, keeping colour, in a closed cabin view only (buttoned up, not the gun sight's or binoculars' full-field picture: `eye_range_strength`); applied to exterior views it lifted every dark object toward grey and read as near-camera haze (measurements in docs/RAYMARCHING_ENGINE.md, "Near-camera haze"). `KESSEL_EYE_RANGE=knee,slope` overrides it for review. The eye's meter weights the whole field 0.50, the 10 degree surround 0.35 and the fovea 0.15 (was 0.20, 0.30, 0.50), and the cone and rod adaptation time constants are 4.5 to 8 s (were 1.2 to 4 s), the pupil's constriction 1.2 s (was 0.3 s). | `shaders/crush.wgsl` (`eye_range`), `src/observer.rs` | Stands in for the retina's local adaptation, which one global exposure cannot give. The owner could see neither the cabin nor the view through the ports by day, nor anything at night, and found the fast adaptation disorienting. These depart from the research values the eye model was built on. |
| Sun's beam averaged over a disc; storm thins after dark; fast sky clock (2026-10-04) | The key beam at the camera is the mean of twelve samples over a 450 m disc. After dark the authored overcast, storm and haze fall to at most 0.42, 0.10 and 0.0016 so the Moon lights the ground. The sky clock runs fast (five minutes of day, two and a half each of sunrise and sunset, five of night) with true Sun and Moon positions. | `shaders/environment_probe.wgsl`, `src/game.rs` (`follow_the_night`), `src/renderer.rs` (`sky_clock_rate`) | The beam is still not evaluated per receiver, so there are no cloud shadows on the ground. The night weather is authored content chosen for play. |
| Volumetric clouds: heaped density field and a dedicated view march (2026-10-04) | The cloud layer's density is a heaped field (flat bases, footprint tapering with height, two scales of billow), identical in the shader and its CPU twin. In the sky pass the layer is marched on its own: 32 view steps to 32 km, and at each lit sample five steps toward the key body for self-shadowing. Multiple scattering in the cloud is a three-octave series; skylight on the cloud is the clear-sky radiance along the view ray scaled by height; haze before a far cloud is one exponential over 32 km. From below the layer each sample's source is at least the delta-Eddington exit radiance along the ray (2026-10-28), so a deck lit from above is as bright from below as the light it sends to the ground. | `shaders/physical_environment.wgsl` and `src/physical_environment.rs` (`cloud_relative_density`), `shaders/sky.wgsl` (`march_clouds`) | The raster sky only. The ray-traced path and the sky probe still evaluate the same density through the general atmosphere march, so their clouds are the right shape and place but flatly shaded. The multiple-scattering series and the skylight term are approximations, not transport solves. |
| Cloud shadows on the ground (2026-10-04) | A 48 x 48 grid (250 m cells, 12 km across, snapped to cells) of the cloud layer's Beer-Lambert transmittance toward the key body rides with the sky probe. In the raster mesh shader a receiver's sunlight is the probe's area-mean beam times its own cloud transmittance over the area's mean, capped at 6. | `shaders/environment_probe.wgsl` (`probe_cloud_shadows`), `shaders/stylized_mesh.wgsl` (`cloud_shadow`) | Terrain and meshes only: grass, particles and smoke still take the area-mean beam. 250 m cells soften shadow edges. Flat-layer geometry. |
| Ray-traced path: one light per pixel, spatial denoiser (2026-10-04) | Local lights are sampled one per pixel per frame in proportion to their unshadowed diffuse contribution and weighted by the inverse probability. Accumulated diffuse light passes a four-pass edge-aware a-trous filter (widths 1, 2, 4, 8) whose tolerance is wide for young history and tight for settled history, when `KESSEL_DIFFUSE_RECONSTRUCTION=1`. | `shaders/combine.wgsl` (`pick_light`), `src/surface_reconstruction.rs`, `shaders/surface_reconstruction.wgsl` | The light sampling is unbiased; the filter is not (it is a display reconstruction, never fed back into history). Vision-port views are not filtered and stay grainy. |


### Camp source skin frame correction (2026-10-30)

Camp actors use the original skin bind frame: the source joint-world transforms cancel the inverse binds at rest. The static loader applies the separate mesh-node transform, so camp rig admission reverses that exact per-vertex frame after checking loader order, including normal/tangent conversion. This removes a coordinate-space error that previously combined a shrunken, rotated surface with full-size source bone pivots. The existing ground-contact test tolerance is unchanged. The gait remains authored two-link IK and source-weighted skinning, without terrain-slope foot adaptation; it is not a biomechanical walking simulation.

### Warm cabin restoration and first-frame transport (2026-10-30)

Owner references `rm_ao6_32_commander_noon_shut.png`, `rm_int6_noon_radioman.png` and
`tpz_needle_widths_480.png` establish warm cabin practicals, bright local indicators and cool exterior
apertures as the direction. The dome's authored emission changes from 1.75 to 42 with the existing
0.003 calibration; its visible cube and transported flux still derive from the same source record.
The TPZ local illuminator is 0.0011 times that calibration (two percent of the former overpowering
0.055 source). Status-light outputs are unchanged. These are authored output choices, not measured
historical wattages, and retain the existing finite-cube/isotropic-source approximation above.

The display's sixfold outdoor rod-visibility boost now remains outdoors. Closed cabin views use
`CABIN_ROD_DISPLAY_GAIN=1` through the existing cabin-view weight, preserving ordinary rod response
without the extra blue-grey amplification overwhelming practical-light colour. This is explicitly
display reproduction; no scene radiance, shadow, cone/rod adaptation or exterior-night output changes.

Ray-query bindings are selected after the actual current scene TLAS is allocated/rebuilt, rather
than during the earlier uniform upload. The former order bound an empty/stale TLAS on startup and
could seed sky illumination through cabin armour. The physical-book first-page/return-page check
now measures 0.28347 versus 0.28469 cd/m2 (ratio 1.0043) in `expedition_manual_20261030_v8_tlas.json`,
against the earlier large discrepancy. This proves initialization consistency, not book readability
or a complete lighting solution. Subsequent warmth changes require separate visual review.

### Source crew balance and cartridge handling (2026-10-30, awaiting review)

Living solo crew body response uses the existing suspension's acceleration/angular motion and a
single projectile-momentum impulse per discharge. The muscle spring, travel limits and head
stabilization are authored approximations, not human-subject calibration. Original source bones,
weights and fixed-length two-link IK deform rendered geometry, collision contact geometry and the
existing anatomical volumes together; the volumes remain the prior eleven-part approximation.
Standing boots retain source ankle support points; loader steps use an authored short gait driven
by actual travel. This does not simulate foot friction or whole-body contact forces.

The new solo loader work path keeps one physical cartridge mesh and ammunition identity from rack
to chamber, including vulnerability in his hands and a retained loose body on interruption. It is
gated by exact source standing bindings. Rack selection is bounded to reachable loader-side storage;
travel uses the existing conservative floor/breech envelope. Swept shell-versus-cabin collision,
arbitrary rack access and exact hand/finger contact remain unverified. Idle observation targets the
source Loader Periscope eyepiece metadata. GPU witnesses and measured grip errors are required before
claiming the resulting action is correct.


### Loader v1 witness and source-contact follow-up (2026-10-30, build17 pending)

The build16 offscreen witness `captures/crew_loader_balance_20261030_v1_manifest.json`
passed five state assertions: one ordinary shot, one rack cartridge consumed, live ammunition
delta -1, reloaded breech, and surviving vehicle. Images at 6 and 9 seconds show a standing
loader moving a real cartridge and returning to his periscope. The camera is crowded by other
crew, so the clip is not a polished-animation acceptance result. The receipt exposed a real
9.4 cm miss for both hands at 5 seconds during low-rack carry; extraction, later carry, and
ram samples had zero reported hand error. Acceleration/braking generated bounded motion
(about 2.8 cm fore/aft maximum in the one-second samples), settling to near zero at rest.

The next source checkpoint replaces the arbitrary 35 cm crouch cap with the intersection of
both original arm reach spheres and a vertical pelvis travel bounded by original leg lengths.
Unreachable rack contacts are rejected, and cartridge movement/completion pauses rather than
extending arms. Spent-case stow is now owned by the active loader, has the same visible case as
its hand target, and drops that case when interrupted. An inaccessible rear-shelf path is
abandoned after six seconds with the case released at its last pose. The original manual/LAN
stow path remains. Step direction now follows actual travel relative to the body, including
sideways/backwards travel. Two focused source-rig tests were added; root owns compilation and
execution. These follow-up changes have only been parsed, not yet GPU-verified.

Remaining limits: authored IK contact points are palms, not articulated fingers. No complete
whole-body obstacle/cloth/self-collision solver exists. Root travel still uses the existing
conservative basket/breech envelope, turning in place is not a contact-planned pivot, and
reach/phase changes need further animation blending. These are exposed limits, not claims of
photorealistic motion. Current corpse registries have no automatic time/distance disappearance;
root's current KIA state review passed, but its very dark image does not prove visual corpse
persistence. Old video observations cannot yet be attributed to a particular failure.


Build18 source checkpoint correction: a CPU-only exact-source audit rejected the first
vertical-only crouch filter because it admitted no normal racks under the old floor envelope.
That incomplete checkpoint was not accepted as gameplay proof. The corrected solver adds
bounded forward flex about the original Spine_1 joint (0..0.5 rad) and chooses the matching two
brass grips, then solves the same original-length arm reach and planted-foot crouch interval.
The independent numeric audit admits actual source IDs 41, 47, 50, 53 and 62 at zero traverse;
source50 uses 0.479 m crouch / 0.25 rad waist flex, source53 0.445 m / 0.10 rad. A new actual
SHELL_SLOTS invariant verifies these five contacts using the source rig. This checkpoint still
awaits root's compiled tests and GPU witness. Hand reassignment and changing waist targets are
bounded kinematics; smooth regrasp/finger motion remains unfinished.


### Build18 loader v2 GPU witness reviewed (2026-10-30)

Evidence: `captures/crew_loader_balance_20261030_v2_manifest.json` and matching `.mp4`.
Five outcome checks passed: one shot, rack 64 -> 63, all represented live ammunition
161 -> 160, loaded breech, living tank. The load cycle completed by the 8-second receipt.
Case index 0 was explicitly owned for stow at 2 and 3 seconds. The 5-second low-rack carry
reported 0.3243 m crouch and zero hand error, replacing v1's 9.4 cm hand miss.

Inspected native-video extractions at 2/5/7/9/13 seconds, saved flat as
`captures/crew_loader_balance_20261030_v2_frame02.png`, `_frame05.png`, `_frame07.png`,
`_frame09.png` and `_frame13.png`. At 2 seconds both arms accompany the case; at 5 seconds
the loader crouches toward the low rack, with the hands substantially hidden by foreground
crew; at 7 seconds both arms extend at the breech; at 9 seconds he stands at his periscope;
at 13 seconds one arm bends toward a bracing contact. No obvious severe mesh failure was
visible in these views. The camera does not expose the feet/knees, and foreground body
overlap prevents a clean whole-body clipping verdict. Stills alone do not establish smooth
transitions, planted gait, realistic fingers, or believable turning/regrasp.

The receipt contains 16 active loader-hand observations over 10 one-second samples, all
reported as zero error. This is explicitly NOT a continuous-contact guarantee: the metric
compares each solved endpoint with its stored pose-time target, while source poses are
sampled at up to 30 Hz. It does not independently measure every intervening frame against
the current moving cartridge or test collision with the breech/other crew. Transient lag,
phase-transition pops, and finger/whole-body interference remain unproven. Build19's focused
CPU run passed all 13 arm/balance checks, including the three private-source tests explicitly
enabled with `--include-ignored` (`artifacts/expedition-20261030/focused-tests-19.log`).
No runtime changes were made during this review.

### Rolled vehicle support and combat targeting review (2026-10-30)

The original overturned-vehicle fallback integrated vertical gravity but froze the rolled angle.
The 2026-10-31 correction uses deduplicated drawn hull/turret source vertices to form the contact
polygon and applies gravity about its nearest support edge when the estimated centre of mass is
outside that polygon. A separated turret is excluded from hull support. Traction remains disabled.
This is bounded high-friction settling: heading is fixed, contact is inelastic, the existing
vehicle-spec COM and box inertia are estimates, and the COM estimate does not yet change after
turret loss. Sliding, full crash dynamics, barrel digging and deformation remain unsolved.
Angular substeps are at most 1/120 s and 10 mm travel at the farthest source point; new contact is
bisected 12 times with a 1 mm numerical contact band. Build44 native rollover regressions pass.
Build46 preserves every source vertex/contact while accelerating the unchanged support query with
an exact source-point BVH and conservative terrain rectangle bounds. The private Tiger receipt
uses 1,610,800 hull and 185,854 turret vertices. Flat support query/120Hz step improved from
27.617/81.123 ms to 0.083/0.169 ms; uneven grid from 26.730/81.260 to 0.173/0.464 ms. Detail plus
crater initially cost 5.905/12.374 ms (down from 86.045/248.568 ms). Build47's exact terrain-range
cache reduces the same detail step to 1.679 ms while preserving source equivalence. These are
native CPU fixtures, not game FPS claims.
All five BVH regressions and exact maximum/contact identity checks pass; no point decimation.
Source triangle edges can be as large as 5.449 m, so accelerating the point query does not remove
its between-vertex terrain-contact limitation. Current source load/BVH preparation is 949 ms.
Visual validation remains pending. The older witness below validates only the superseded vertical
contact fallback.
The source-vertex versus heightfield support test is a discrete contact approximation, not swept
triangle/terrain collision. The deliberately inverted 20-second witness
`captures/vehicle_inverted_fix_20261030_b.json` records no horizontal travel, no shots and no source
support penetration. It does not prove a naturally occurring crash or every terrain case.

Incapacitated/overturned AI cannot lay or fire: this is an explicit crew-control design rule, not a
claim that inversion mechanically prevents every gun discharge. Approach/hold/reverse combat range
is authored doctrine. Brawl movement, aiming and visibility now select the same eligible opposing
vehicle; ordinary detection, gun alignment, ammunition and reload checks still govern shots.
`captures/vehicle_brawl_fix_20261030_b.json` records three shots and three knockouts over 90 seconds.
Its video renders one frame per simulated second at 12x playback; it is an outcome/motion witness,
not real-time pacing evidence. Focused target, range, support and incapacity CPU checks pass in build19.

### Source-grounded rocks, released trees and stationary wrecks (2026-10-31)

Initial independent loose rocks now admit only closed source meshes with a uniform-solid-density
COM estimate. Candidate source faces supply possible resting orientations. Projected source and
terrain triangle intersections determine vertical support over whole faces; the source COM must
be supported by a nonzero-area contact polygon. The near-contact band is 5 mm, with a separately
computed f32 placement allowance. This is a bounded static placement test, not a settling
simulation: at most 512 source triangles, 4096 terrain cells and 32768 triangle pairs per candidate,
four cached source orientations. Unsupported, open, over-budget or opening-crossing candidates
are rejected. `scatter_field` rechecks the actual packed quaternion/scale before rendering and
collision receive the same transform. Rooted trees, furniture and rigid authored tableaux are
excluded from this independent-rock rule. Build44 integrates manifest admission and six native
source-grounding tests pass, including a 5.25 km translation. Visual validation remains pending;
no claim that every decorative wreck has been corrected.

The rooted-hinge candidate failed actual Pine validation: its tapered, buried root extended
6.3 mm beyond the soil intersection, jamming the visible trunk at 5.7 degrees. The replacement
releases the entire original mesh into the same source-sampled rigid contact solver as loose props.
It does not delete buried points or add root penetration allowances. Initial source burial is
projected out and measured (the actual fixture starts 25 mm deep); soil excavation/root deformation
is NOT modeled. Build47 showed that a constant 0.35 rad/s COM-only kick could be physically arrested by the flat
base. The replacement release estimate uses 5% of existing failure/bending work per estimated
stem mass, with a minimum of twice the actual source COM-over-support-edge potential barrier.
It initializes consistent angular and COM velocity about the actual forward support edge; that
edge starts at zero velocity instead of being driven into soil. Shot/blast transfer remains an
explicit estimate because those legacy APIs provide direction without momentum. Subsequent
position/orientation come from gravity,
source contact reactions, friction and estimated uniform thin-shell mass/inertia. Whole foliage
remains rigid/load-bearing; this is not a wood-fracture or compliant-branch simulation.

Source surface samples target 160 mm edges, with refinement enlarged if its 8192-sample budget
would be exceeded; original triangle vertices remain. Measured maximum spacing is emitted in
receipts. Integration is at most 1/120 s and 40 mm sampled-point travel; the contact band is 2 mm.
These are spatial discretization bounds, not a vertical terrain-error bound. Ground peaks between
samples and object/object contact remain unsolved. Sleep requires low motion, friction feasibility
and COM inside a genuine support polygon, never elapsed fall time or a terminal angle. Actual
contact points drive thuds, limited to one sound per 250 ms rather than two impacts for life.
Settled trees recheck source support after actual detail crater/rut revisions or changed openings;
coarse-only grids lack an immutable revision contract and are rechecked directly. No hidden
ground certificate survives world restoration. The original tree record layout remains intact;
the optional versioned released_motion_v1 packet saves full velocities, rotation and contact age.
Legacy falling saves retain their uploaded pose and explicitly migrate because they had no saved
velocity. Build47 passed save continuation and removed-support wake, but flat Pine/toy toppling failed
until the release correction above. The old 8 cm final-trajectory translation assertion was not
a physical invariant: the near/far f32 analytic callbacks differ by 48.5 micrometres in height and
0.001146 in unit normal. New tests separately require bit-identical local motion under identical
inputs, and clear/stable source support under the actual quantized callbacks. Actual Pine, source
release kinetic-work/root-velocity and corrected grade tests passed in build48 (six tree tests).
Actual Pine terminal angles were 1.940, 1.689 and 1.499 radians on -20%, flat and +20% grades;
all sampled points remained clear and the bodies reached supported rest. The quantized translated
case finished 34.3 mm apart, while identical local-input motion was bit-identical. This validates
the stated discrete solver, not continuous triangle/soil contact or full branch mechanics.

Dead upright AI now have a dedicated settle_wreck entry point to advance the existing suspension
once per host step, preserving damage while allowing a stationary hull to fall after support is
removed. Ordinary contact re-projection stays separate. Game integration and the native regression
pass in build44. Loose scatter now uses final rotated source samples and contact torque; fixed
minimum-box clearance and unconditional 4.5 s sleep were removed. It uses the same stated source
sampling/mass estimates and support/friction sleep rule. Object/object collision and waking old
sleeping scatter after changed terrain remain unresolved; tall hinged scatter retains its older
prescribed hinge. Build48 loose-prop and half-decoded rock-scale regressions pass.

Detached turrets require a stable actual contact polygon before sleeping and recheck support
each settled tick. Their support-only source cache aligns triangle winding to authored normals;
unreliable normals fail closed. This fixed build45's inward-wound box top/underside error; five
build46 turret fixtures pass. The actual T34 still failed 12-second settling, and translated
trajectories differed 0.628 m: sleep certification alone did not repair its old single-average-point
response. The build47 correction distributes impulses/friction over actual manifold vertices,
preserving the 1 mm response and 4 mm rest bands. Exact transformed source bounds skip impossible
hull sweeps; no proxy provides a contact. Actual turret source has 21,376 samples with 0.1793 m maximum sample edge. Build47 manifold/broadphase
reduced the actual source step to 3.061 ms and translated drift from 0.628 m to 0.003354 m, but rest still
failed. A subsequent causal fix removes phantom lift: signed source separation is now offset by
the existing 2 mm target before clamping, rather than adding 2 mm even to an already-clear point.
A dynamic plate regression checks that the unchanged 4 mm rest band stays engaged. All nine
native build48 turret fixtures pass. The actual T34 stabilized with zero terminal velocity/spin;
1,153 active steps averaged 3.562 ms in the CPU fixture. Near/far final displacement differed
2.383 mm. No persisted body fields changed. Moving-carrier
continuous collision and compliant impact remain unsolved. The new manifold/friction/source
tests and active cost receipt await native validation; no complete six-axis solver claim.

Static decorative wrecks remain a separate unresolved path. The current canyon launcher manifest
has316 battlefield/canyon wreck placements (232 authored tilts); The March has none. They are not
the retained Tiger/T34 Gen recipe. Some tableaux deliberately bury/mire or arrange compound pieces.
Exact source/placement ownership inventory is in
artifacts/static-wreck-grounding-20261031/manifest_wreck_inventory.json. The generic cached-source equilibrium/admission is now registered in the actual Game manifest
loop before rendering, walking, salvage and collision consume the shared page transform. Metadata
ownership identifies264 independent candidates and preserves52 horse/cart/sledge associations;
explicit mechanical assemblies, burial flags and unrecognized hand edits retain their owner poses.
Every source vertex and triangle remains. The COM uses an explicitly estimated uniform thin-shell
area distribution. Orientation search is bounded to12 degrees from the authored pose; build56
uses96 descent proposals plus two8-proposal authoring neighborhoods (112 total).
The 5 mm contact band and 10 mm COM inset gate acceptance. Affine terrain receives a conservative
whole-footprint certificate; rough terrain clips actual low source faces against terrain triangles
(maximum 250,000 pairs, 65,536 contacts/nodes, 100,000 source triangles). Geometry failures retain and
report the unresolved original rather than deleting it or claiming it is supported. Optional
KESSEL_GROUND_SUPPORT_RECEIPT writes a create-new per-index JSON receipt. Build47 ownership and five generic face/support tests passed, but native KV1 admission failed
because separate pitch/roll coordinate descent stuck on diagonal track supports. The corrected
search also follows coupled gravity descent about the actual source support edge, without
changing its caps or contact margins. Build48's eight native source tests passed. The actual
build49 Canyon receipt admits 124 of 264 independent wrecks, retaining 140 unresolved originals
(83 Unstable, 35 SearchBudget, 22 UnsupportedAfterClearance) and 109 cart/sledge/horse assembly
members. Manifest preparation took 46.478 seconds, including all prototype work. The close image
of source index269 plausibly shows the admitted KV-1 resting on its tracks; index260 in the first
overview remains unresolved and must not be presented as corrected. Later face-contact refinement
feeds actual clipped source points back into descent. Build51 diagnoses a separate search/final
clearance mismatch and reserves the measured allowance in both contact selection and the gravity
guide. Build52 preserves the converged step across small refinements; its actual map receipt admits
142 wrecks, retains122 unresolved originals and109 assemblies, and separately preserves43 authored
embedded rock formations. Preparation took62.953seconds including all prototype work. Index260
still fails its10mm inset. The build53 test-only full-face scan finds9 certified same-XZ nearby
orientations out of58 samples; none is downhill from the rejected pose. The lowest certified
sample raises represented COM by0.19455mm and has224.925mm inset. This supports a tiny initial
authoring adjustment, not a claim of gravity-driven motion. Pending build54 reserves8 of96 queries
for8 fixed radius/contact-band-derived neighbors, sharing the250,000 face-pair cap. Full original
face clearance,10mm inset,5mm band and12-degree original-pose bound remain unchanged. Every source
vertex's total displacement from the rejected face-fitted pose, including height adjustment, is
limited to5mm. A resulting receipt explicitly labels `nearby_full_face_certified_authoring_pose`;
it does not certify neighboring-prop collision or a swept falling/sliding path. Native260 remains
a required assertion. All14 build54 static tests passed, including native260 at95 queries and
23,282 face pairs. Actual54 admits143 and retains121 unresolved originals, but loses previously
safe11962/12521 while gaining260/1248/11689. Both regressions previously needed96 descent queries;
the88+8 reserve caused that loss. Pending55 explicitly restores96 descent proposals plus8 separate
authoring proposals (104 total), retaining the250,000 face-pair cap and every geometric gate.
Receipts split descent/authoring/total counts and expose their limits. Native55 restores both
96-query placements and11689, but1248 loses its earlier certified neighborhood. Pending56 saves
the best pose atquery88 while running96 descent only once; if the final eight-neighbor phase fails,
it re-certifies that saved pose and tries the same eight neighbors. The explicit maximum becomes
112 proposals, with one separately counted saved-pose face certificate, all within the unchanged
250,000 face-pair budget. Saved-reference query88 is recorded with any resulting5mm source-movement
bound. No index-specific transforms or physics thresholds are used. The five known-safe native
fixtures and both-neighborhood narrow-peak rejection pass in native56:15 focused tests passed,
0 failed (`foundation_b56_static.log`). Actual57 reports146 admitted,118 unresolved (93Unstable,
25SearchBudget),109 preserved owners and43 embedded formations. Exact source-identity comparison
retains all145 prior52/54 admissions with zero losses; owner identity/reason sets and formation
records are unchanged.1248 uses112 proposals/197,412 pairs at the saved88-query reference;
11962/12521 retain96-query descent-only results.11794 is one new admission. Observed maxima112
proposals/250,000 pairs respect the disclosed bounds. The51.678s preparation includes prototype
work and is not an FPS or isolated solver claim. Comparison receipt:
`artifacts/expedition-20261030/ground_canyon_b57_admission_comparison.json`.
This remains partial coverage:118 unresolved original poses are retained, not hidden or certified.
The source-overturned sledge requires more than 12 degrees and remains an explicit safe
rejection. These are partial coverage results, not a claim that all decorative wrecks are grounded.
This handles initial independent placement only, not dynamic settling of decorative wrecks after
later terrain changes. Editor preview is not yet updated to match this Game admission step.

### Bounded camp dressing and stocked shelves (2026-10-30)

Camp approach dressing uses existing source instances, eight metres of clear vehicle corridor,
source obstacles and terrain support samples. Disconnected requested endpoints now fall back to a
reachable endpoint without relaxing those exclusions. The actual camp seed retained 146 objects,
including 27 pushables, after final world cover insertion (`live-camp-v4.log`). Objects at or below
25 cm are treated as traversable visual debris. Larger accepted props use the existing source-bounds
pushable collision, not exact triangle contact; this approximation must not be described as a fully
physical rubble bed. Only the bounded approach is dressed, and incremental frame cost is unmeasured.

Medical/ammunition case placement measures real shelf decks and source case bottoms rather than
using guessed heights; the private-source support regression passes. This static fit does not imply
dynamic stacking. Stock counts remain finite and unchanged. The live medical review passes six
bedside checks with explicitly staged delivery, not a complete casualty rescue. The medical stock
view remains dark/noisy and the ammunition doorway is occluded by personnel; visual acceptance of
all stock, patient poses and complete camp activity remains open.

Source-20 additionally rejects any prop whose entire transformed footprint plus the existing
0.15 m obstacle clearance lies beyond the covered obstacle query. The new edge regression passes.
The independent v5 run still retains all 146 pieces/27 pushables and passes all six bedside checks;
approach/medical/ammunition images are byte-identical to v4. No performance conclusion is implied.


### Recovery source checkpoint (2026-10-30; execution pending)

The generic transport now has an authored lower hinge on fourteen retained rear-panel source
triangles. Render and contact share those faces, but the historical hinge behavior is not sourced.
Finite workshop items and body identities remain single-owned during timed cargo/lift procedures.
The handoff documents source-rig passenger routes, bounded collision, exact snapshot ownership,
and explicit limits. The authored180kg wheeled recovery rig uses disclosed fictional ratings;
it does not grant the light transport a Tiger tow capacity. No historical winch performance,
continuous cable dynamics, locked-track recovery, turret hoisting or complete vehicle damage is
claimed. Current code parses; pending root compile/tests and `recovery-gameplay-review` remain
required before calling this connected loop verified.

Field and passenger bullet contacts use posed source triangles and skin-weight body ownership.
Their first injury transfers the same finite person to the existing Casualty/retained-body owner.
This avoids a second HP pool but currently suspends autonomous duties even after a minor wound.
Original eleven-segment ragdoll approximations and sampled animation remain; no new medical,
ballistic or continuous whole-body accuracy is claimed. The transport has no validated armor
simulation, and field reserves do not yet become separate priority targets for enemy AI.


### Build26 follow-up checkpoint: finite refuelling and carrier mobility damage

Source parse reviewed; this follow-up has **not yet been compiled or run**. Root owns the next
serial build and `recovery-gameplay-review` run. The earlier build26 binary does not contain it.

- Carrier and winch each have a visible authored filler cap with the same geometry used for
  picking. The carrier cap mounts on an actual outward source cab triangle (local centre near
  -0.93,1.373,0.66); the winch cap sits on its authored motor housing. These are disclosed
  prototype additions, not evidence of a historical filler location. Targeting checks the real
  carrier, kit, Tiger and world surfaces for obstruction.
- A held existing jerrycan transfers up to0.8L/s with LMB held, capped at40L carrier /5L winch.
  Releasing LMB, moving aim away, boarding, moving the carrier or an empty can stops transfer.
  The exact destination fuel and active pour target/timer are binary-saved. The can remains the
  same Pickup/PortableItem saved by the complete world owner. No camp refill occurs.
- The can uses an assisted tilted handling pose near the filler. Its spout has not been measured
  against the source and there is no fluid simulation; do not describe this as a fully rigged
  hand/spout contact animation. Added CPU conservation/capacity/pause test and live harness
  carrier/winch fill phases. The harness explicitly stages each destination0.5L below capacity
  and equips an existing can; it neither duplicates nor refills that can.
- Fixed an existing finite-resource bug: `PortableItem::drain_fuel_liters` used to discard a
  positive remainder of0.01L or less. It now retains every positive remainder.
- Authoritative projectile travel queries actual source carrier faces. Hits on front wheel
  parts (10kJ) and running gear parts (25kJ) disable motion; an actual front bonnet hit (500kJ,
  localz>1.2) disables a disclosed **engine-bay proxy**. These are authored gameplay thresholds,
  not calibrated historical damage or FEA. The source has no internal engine. Recent projectile
  identity/part masks prevent counting one flight segment repeatedly and are saved exactly.
- Carrier skin still does not absorb projectile energy or provide calibrated armor. Passengers
  are not in a magical safe room. Carrier mobility damage currently has no repair operation;
  no claim that these flags can be reset by refuelling or camp return. Actual source damage
  does not alter the intact rendered mesh yet.

New focused filter: `carrier_damage_tests`. `halftrack_runtime::recovery::tests` now includes
metered fuel conservation and pause/capacity checks. Recovery-state validation additionally
rejects duplicate bays, conflicting people/cargo, nonfinite cable hooks and overfull fuel.


Build26 recovery review found the original14-triangle tailgate extraction incorrect: it moved
only the inner skin, leaving the outer panel and its reinforcing strips blocking the visible
winch. The corrected bounded source selection moves all78 rear-panel triangles (inner/outer
skins, top and attached latch/strip geometry); no triangle is deleted. CPU source survey proves
that the formerly blocked rear-to-winch line clears with this panel open. An ignored private
source test `complete_source_rear_panel_opens_real_loading_line` asserts the closed/open ray,
exact78-face admission and total triangle conservation. Runtime visual retry is still required.


### Spatial theatre climate checkpoint (source only; next root build pending)

`Renderer::theatre_weather` is an explicit persisted flag set from the actual continuous
TheatreRuntime. It uses unused existing uniform lanes, so GPU buffer sizes do not change.
`weather_density.wgsl` is now the one pure field source assembled into both raster optical
weather and raymarch pipelines. CPU WeatherView uses the corresponding function. Winter weight
is smoothstep(600,2600,-worldZ): full southern clarity beforeZ=-600, full winter afterZ=-2600.
Southern aerosol extinction is0.00012/m; northern density retains the existing winter-front
coefficient and drifting banks. The transition stays fixed on the map while banks advect.
Raymarch theatre mode bypasses its old globally winter spindrift/fog-bank formulation so its
field matches the CPU/raster calculation; the old path is unchanged in legacy acts.

Snow particle identities are thinned at each actual particle world position, not at the camera.
Terrain material mood and terrain snow glitter use the same fixed gradient. Optical sources,
AI recognition and camera rays all sample the geographical field. Global overhead clouds/sky
remain shared; this does not claim three independent weather simulations or seasons. Snow
cover on vehicles, ground friction and physiology have not been expanded into climate models.
The southern aerosol and transition distances are authored gameplay values, not historical
weather reconstruction.

Source parsing passed; shader validation and GPU images remain root-owned and pending. New
CPU filters: `weather_visibility::tests` and `regional_weather_tests`. Root should also run the
existing raster and raymarch shader-validation suites because shared function signatures changed.
No Cargo/test/GPU command has been run by this agent.

### Workshop build27 source checkpoint — cooling assets and recovery gates

Build26 `workshop-gameplay-review` passed the actual healthy native optical component path
(release, withdraw, insert, secure, test) with identity and old/new source-contact checks in
`captures/workshop_gameplay_20261030_b26a.json`. This is evidence for that bounded path only.
The new damaged-replacement review uses the current theatre, explicitly stages one finite spare
and one source-confirmed damage contact, and preserves the discarded damaged original. Its
build27 runtime result remains pending. Spare vision-block canonical transforms now correctly
undo the source block0 mount before fitting another mount.

The source owner now uses47 finite banks,24 mounts and72 contact frames. Six original named
cooling solids are removed by exact triangle multiset: Hull1102/1103 fans and1115/1125/1116/1126
grilles.12 private derivative GLBs and `catalog/tiger_workshop_cooling.v1.json` document the
source hash, topology and geometry changes. Original fan blades were fused to the support plate.
The derivative creates capped blade-tip/frame cuts with a measured minimum radial clearance of
1.482mm. It does not rotate the support plate. The original files are preserved; each derivative
is closed, consistently wound, positive-volume, with zero nonmanifold/boundary/degenerate faces.
These facts do not establish a service extraction path.

**Real obstruction found:** removing both grilles still leaves a straight fan lift intersecting
source plenums/deck after about12mm. Runtime blocks cassette withdrawal rather than passing the
component through that geometry. Additional disassembly/path investigation is ongoing. Grille
and cassette contacts are catalogued exposed source triangle centroids. Removal uses grouped
retainers and assisted object poses; it is not a bolt/thread or full rigid-body manipulation solver.

Four native rotors and four spare-cassette rotors have their own rendered and collision transforms.
Uncommissioned banks are absent from collision/acoustics. Native drive ratio1.0 and alternate1.15
are declared engineering assumptions; the wartime report establishes increased fan RPM, not
these exact ratios or an identified interchangeable kit. The spare's external casing is the same
source derivative; no separately modeled internal gearbox teeth are claimed. Fan affinity laws
produce linear flow and cubic shaft demand. The existing Powertrain deducts that actual demand
before gearing and from the same steering budget. Zero demand retains previous behavior.

The serializable two-node engine/coolant model conserves stored heat and records ambient loss,
finite leakage and boiling. Its120kg initial coolant corresponds to a120L approximation; heat
capacities, pump/radiator conductance, failure exposure and20C ambient are explicit engineering
estimates, not calibrated Maybach data. Natural radiator cooling is an estimated150W/K and uses
no fictitious fan motion or shaft power. There is no coolant-pressure, oil, freeze or CFD solver.
Existing fuel consumption remains independently authored; there is no measured fuel/heat map.
Powertrain accessory coupling uses the preceding ordinary frame (roughly16ms at60Hz).

**Release gate:** `LIVE_THERMAL_SERVICE_READY` is false in every preset. Rotor rotation is
kinematic presentation only. Candidate fan loads are evaluated for the diagnostic receipt,
but both their drivetrain debit and thermal time are gated off together. Coolant loss and
irreversible overheating do not advance. This prevents a destroyed fan from improving driving
while its heating consequence is unavailable. A regression compares healthy/damaged fan
calculations and ten seconds of ordinary driving/steering through this same release boundary.
A finite coolant refill and recoverable engine service path must be playable before enabling
these failures. This is explicitly incomplete; a constant current coolant display is not proof
of live heat simulation. Do not describe a finished physical cooling upgrade or full powertrain
repair loop. The existing source-supported authored digital fuel face also presents coolant C;
it is not a claimed historical instrument reconstruction.

Whole-world `workshop.runtime` owner is version2: cooling state/last sample/rotor phases join all
47 poses, finite parts, partial jobs, tool and input latches. The central required-owner version
must reject version1 binary layouts. New source tests cover six-root conservation, handed
replacement compatibility and stock exhaustion; six thermal tests and three accessory-power
regressions await the root's serialized test run.13 manual spreads cover the implemented optical,
track, lamp and cargo procedures plus current cooling-access limits. Build27's actual workshop
review passed51checks, including a damaged optical part retained on the ground and a finite
replacement fitted through ordinary input. Build29 adds a separate physical lamp installation,
switch and battery witness; that witness and the new coupled-gate regression remain pending.

### Recovery continuation checkpoint after build27, awaiting build28

Build27 ordinary recovery actions passed the staged review: the full78-triangle rear gate
opened; winch lowering, real-source cable attachment, casualty loading/unloading, spare cargo,
finite identity, and same-Tiger remount all passed. The rated winch moved the Tiger0.675m,
spent5.007mL, and demanded44.93kN against102kN available. This is an interaction/state witness,
not historical certification. Its sole fuel-test failure was an absent can: isolated scenes
clear legacy pickup records. Theatre initialization now clones the still-preallocated genuine
jerry-can mesh, supplies the depot cans and eight finite camp cans, and retains every remaining
litre in ordinary PortableItem state. Build28 must verify that correction.

New carrier field-service is explicitly a fictional prototype: four visible12kg service packs,
existing physical toolbox, actual source wheel/running-gear/bonnet picking, and20/40/60-second
held work. A pack is consumed only on completion; interrupted work remains, and all state is
saved. The source has no internal engine, so bonnet service must not be called mechanical engine
disassembly. The current pack moves to a service staging pose by assisted placement; hand carrying,
fastener animation and pack-specific collision are not yet implemented. No free camp refill.

New replacement boarding keeps the same finite PersonId as a source field body along a bounded
route. Actual source-support queries plan hull/turret steps; every movement sweeps the source
body vertices through the Tiger and world. Open-hatch and stationary-Tiger requirements persist.
Arrival alone binds a new station controller and imports the same person's casualty state.
Injury during ascent uses the existing retained-field-body owner. New arrivals keep1.05 stature;
driver/radio roots retain the original ankle anchor rather than shrinking to the old0.80 scale.
Source idle/seated blends move along the route; hand-over-hand grip animation is NOT authored.
The route has not yet passed actual-source runtime proof and may correctly stop at a fit conflict.
Do not call this a complete working boarding loop until that test passes. Multi-person body
contact along boarding and cancel/re-route UX remain review items. All of these statements are
source-level checkpoint status; build28 and full-world save roundtrip are pending root.

### Boarding posture checkpoint after build29 (2026-10-30)

The build29 witness stopped at a fixture panic before evaluating any route: clearing the soldier vector invalidated mixed-combat source indices. The review now preserves the entire array, disables mixed combat, and marks those fixture soldiers inactive. This correction is not evidence of boarding success.

`crew_hatch_posture.rs` now crouches at the original leg joints while keeping the base ankle targets, tilts at the source waist, and raises the forearms using the original two-link lengths. The same 914 weighted source vertices drive rendering and the existing vertex-sweep contact admission; the same joint matrices drive field-person injury anatomy. No stature reduction, hatch enlargement or collision bypass was added. Descent is subdivided into 10 cm route samples with seated posture blending. Tuck is persisted in transport.recovery owner v3 and restored at zero time. The new ignored private-source test checks original limb lengths, ankle targets and unchanged UVs. It still needs root execution.

Receipts now include the actual route target, standing/tuck amount, and source-frame/local/world position of a blocked vertex path. Exact 564-triangle continuous deforming-mesh collision is not claimed: admission still samples moving source vertices, with the existing source-derived anatomical bounds for other cabin occupants. Hand-to-rung constraints and reverse/cancel routing are still absent. Runtime fit must be decided by the next actual source witness.
### Build30 boarding/service result and follow-up

Actual private-source test `hatch_posture_keeps_original_limb_lengths_and_ankle_support` passed, as did four boarding owner/stature/broadphase tests. `replacement_boarding_20261030_b30a.json` shows normal carrier bonnet service passed, preserved partial work across transport owner restore, and consumed exactly one finite pack. It does not show successful replacement boarding: the driver stopped at the actual front hull at local(1.511,1.967,2.497) while crossing diagonally from the lower fender; the loader's rear-deck first step exceeded the existing 1.90m climb reach.

The follow-up routes both roles via the actual front fender, lifts before crossing the front roof, and routes around the raised original loader lid. Aperture centering measures opposite original source surfaces; descent placement centers the actual weighted mesh section through three roof-height planes. These are planned body translations, not expanded holes, scaled people, or contact waivers. The actual source sweep remains the final gate. A downward roof crossing can no longer take the old broad toe-support exemption. Runtime success remains pending the next review.

Camp relocation also now moves the actual loaded patient segments/centers in the same transition, preserves severed world pieces, refreshes cargo and recovery meshes, and defers while a replacement is boarding. Stowed equipment follows the carrier; deployed anchors retain their world coordinates. This fixes the prior one-frame/save discrepancy between relocated transport and retained patient bodies.
Additional source survey: `boarding_measured_apertures.json` measures the driver opening centre at +15.568 mm X from its interaction anchor. The loader opening is an oval approximately 0.553 m across X and 0.385 m fore/aft at the sampled lower roof planes. The loader now descends facing forward so his shoulders use the wide axis, then turns toward the breech below the roof and lowers his arms at the working post. This uses the original opening; its narrower axis is not expanded. The follow-up remains awaiting actual route verification.
### Engine access runtime checkpoint after build31

The visible engine leaf, its attached handles/covers, and its three articulated latches now
register as the same source triangles in rendering, acoustic contact and projectile contact.
The unchanged two carrier hardware objects are finite workshop banks47/48 (mounts24/25).
Workshop owner version3 persists their ownership plus leaf/latch fractions and held-action grip;
source frame capacity is80, with the four cover assemblies at73–76. The original assets remain
unchanged; catalogued closed-mesh service derivatives retain their measured source changes.

The candidate controller admits rigid rotation using actual source triangles, one-millimetre
maximum corner substeps and an arc-sagitta envelope. Its SAT contact query is conservative,
not a continuous exact rigid-body/contact-force solver; finite-axis padding can reject a clear
path. Arithmetic admission uses a declared38.15micrometre local f32 transform error bound.
An initially fitted seam may separate only with an analytical outward-motion proof. Closing
may return only to the catalog's original zero stop with that same proof run in reverse;
this does not authorize an intermediate collision or over-insertion. Carrier paths use actual
source triangles and their documented piecewise translations. Hand alignment remains the
existing assisted component interaction, not a simulated free-body manipulator.

Ordinary turret traverse and gun elevation now test the raised source cover with the same
relative-motion admission. There is no invented forbidden yaw sector. Dynamic engine cover
strikes classify as hull hardware even when above roof height. The controller and its review
are awaiting the root's next build/run; catalog service remains disabled while build31 media
loads the existing asset package. No cover gameplay success or engine exchange is claimed.

Build31 passed32 focused workshop tests. Build30's actual workshop review now passes the
lamp installation, source-lens emitter, finite battery drain, physical switch and saved battery
checks after the ordinary Loader-side approach. These are separate from unreleased fan/engine
service. The cooling thermal state and its accessory debit remain jointly release-gated;
destroying a candidate fan cannot grant propulsion.

### Engine access and ammunition service checkpoint after build33

The source service catalog was enabled for the build32 review. That review did **not** pass:
the first left latch remained at zero despite a valid ordinary held-input grip. The build33
private-source CPU regression reproduces the same first-step contact and records its exact
moving and obstacle triangles. Their common axial end plane has no fit clearance. The next
source correction must change the visible derivative and its identical contact mesh; no
tangential-contact exemption or hidden collider removal is authorized by this diagnosis.
Fitted mechanism transforms now originate at the actual local joints, avoiding kilometre-scale
world-position cancellation. This numerical correction alone did not clear the source test.

Workshop owner version4 adds interrupted four-bank ammunition-stowage inspection to the
existing finite parts, poses, access fractions and input latches. Actual source tray contacts,
fresh held input, stationary access and fire-out are required. Inspection certifies the current
aggregate post-fire ammunition service latch; it does not repair bent trays or replenish any
round. A new fire or rack failure invalidates prior checks. Live ammunition remains vulnerable
while that service latch is failed, and new ammunition intake is blocked until it is certified.
Build33 CPU validation of these new actions is in progress; no ordinary gameplay witness of
the four-bank inspection or cold recommission is claimed yet.

The unregistered major-service cassette and engine-core derivatives remain preparation work.
The cassette has a documented visible seam and measured source-volume mass estimate, but its
retainers, lifting fixture and live finite-hoist ownership are not implemented. A complete engine
extraction still intersects source attachments. There is no playable engine exchange, damaged
fuel-system replacement, or general restoration of a destroyed Tiger at this checkpoint.

Build33c subsequently passed the actual source sweep for all three latches. The two side locks
now have measured150micrometre axial seating clearance, with unchanged pin axial lengths.
The rear arm was rebuilt with solid Boolean cuts from the preserved nominal clevis: its
19.7mm journal bore and23.8mm head envelope are real visible faces, not the earlier nonlinear
vertex deformation that left a crossing cap face. The fixed journal/base is unchanged. The
package's fifteen hashes/topology checks pass. Full lid opening/closing and ordinary gameplay
remain separate pending checks; the successful latch test does not certify either one.

Build34 source adds a narrow cold recommission path for the existing recoverable Expedition
Tiger. A living embodied keeper must use the actual driver's starter, with safe inspected
stowage, no burning source patches or ignited round, intact Engine/Fuel modules, positive finite
fuel and shut/secured engine access. The existing1.8second crank is saved normally; readiness
is checked throughout and only its successful completion releases the machine-disable latch.
No component fault, casualty, cartridge, litre, scar or fatal record is reset. Failed unrelated
systems continue to block their own functions. Compilation and the disclosed staged-loss
review are pending; existing disabled-hull access/damage gates still require separate audit.

Build34 passed the focused cold-start, engine, rack and manual CPU checks, but the first actual
cold-review rack pose was blocked by the retained turret basket. Build35 changes only those
disclosed inspection poses to other existing bounded floor positions: the source-confirmed
reach stays1.17–1.54m, with no distance increase or source deletion. A full-cabin source regression
now rejects the old poses and checks the new ones; the actual-world witness remains pending.
New first-hit diagnostics distinguish the eye/target, source-local hit, world hit, normal and
world obstacle. The first review did confirm that the starter refuses uncertified stowage.

The full lid source sweep also found a fitted front-chamfer edge. Build35 extends the analytic
departure proof to cardinal support planes of each actual finite obstacle triangle, restricted
against its verified outward orientation. Every rotating corner must remain beyond that plane
through the whole trigonometric interval and initially touching corners must depart strictly
outward. Convexity then covers the complete moving face. This is not an endpoint-only waiver,
and all other obstacle triangles are still checked. The existing38.15micrometre coordinate
uncertainty is unchanged. Regression witnesses include the exact failed source pair, deeper or
incoming contact, returning motion, and a second genuine collision. Independent numerical
checks found no counterexample in12,000 intervals and1,433 admitted paths; this does not replace
the pending full-source stroke test. No additional underside mesh cut was made.

Build35 also keeps ordinary projectile and cartridge-fire consequences active for the original
recoverable Expedition Tiger while its machine-disable latch is set. The exception does not
apply to other dead vehicles. Catastrophic casualty handling in modes with actual crew actors
retains the physical blast solver's affected identities instead of also marking every roster
slot dead, including a keeper outside the tank. Finite live ammunition remains the detonation
source. These corrections and their regressions await the root's serialized validation.

The root's build35 tests subsequently passed twelve sweep regressions, five cold-start tests,
five rack tests and five engine-operability tests. Its actual cold review passed fifteen checks,
including all four ordinary held-input tray contacts and the driver's complete crank restoring
the same hull, before a separate lazy corpse-pool layout problem stopped save/resume validation.
That later failure does not certify continuation and is being handled by the crew/save owner.
Manual hand-service is now a separate helper: the original alive-only actuation block remains
intact, while the recoverable keeper can manipulate the existing finite rounds, casing and
breech and collect physical tools. No automatic loading, drive or fire authority is released.
The repair cue's matching visual-access correction is queued for build36.

Build35's complete lid stroke clears the earlier fitted deck edge but stops at opening step25:
the front plate meets retained rear-track carrier hardware `Turret_0141`, triangle271. The
source match is within0.17micrometre. This is a genuine new obstruction, not an initially fitted
seam. Nearby ordinary turret angles are under examination; no contact exemption or removal is
granted by this result. Neither the full cover operation nor heavy engine exchange is complete.


### Tiger fuel filler — build35 source checkpoint, pending native/live verification

The instant rear-zone fuel refill is removed. `tiger_refuel.rs` uses two retained source cap meshes, real source-contact picking and a finite interrupted0.8L/s can transfer. The original Hull_0875 plug is retained; the fused Hull_0902 inner cap is separated and a visible45mm-radius blind receiving well is cut into its tank. Hull_0708 receives explicit visible deck clearances (upper radius98.25mm; eccentric inner-cap throat radius70.3mm); the original assets remain unchanged. Render/contact use the same admitted closed derivatives, with hashes/topology in `catalog/tiger_fuel_service.v1.json`. This is authored CAD reconstruction, not a claim these bores were historically measured.

Guided translation paths use full-face source sweeps, including both caps and the remaining actual tank/turret, with bounded nonpenetrating fitted contact and100micrometre parked support clearance. Preliminary standalone CPU evidence passes all four complete motions; native Rust/live tests remain pending. Partial cap poses and pour work are retained in game.physical v3, while can contents and tank fuel stay with their existing owners. No separate valves, cap torque, liquid CFD, spilling or fuel leakage are claimed. The historical draft gives534L; this slice retains the existing540L gameplay calibration. See `docs/TIGER_FUEL_SERVICE_20261030.md` for provenance and exact limitations.

Build35 compiled and all five native fuel tests passed. Build36 adds source-frame-to-world picking, translated/yawed source regressions and base-point downward source support for deck cans. The existing prop integrator still is not a general full-shape rigid-body solver; it now uses the actual Tiger triangle support instead of an invisible terrain-only drop through the hull. Live proof remains pending.

### Living recovery threat — build36 source checkpoint

Ordinary enemy stealth, gun lay, winter orders, movement and AT-gun target selection no longer treat a disabled Tiger as the end of a living keeper's exposure. They follow the actual controlled person or occupied carrier source bounds/velocity, while retaining LOS, weather recognition, spotting accumulation and first-shot delays. Carrier detection uses the existing authored vehicle envelope, explicitly not a measured signature; it no longer inherits a crouched foot silhouette or the abandoned Tiger's engine state. AT-gun Tiger weak-point selection applies only to the Tiger; other targets use their real world location and normal AP stock. The AT crew's anti-overrun cover rule is restricted to vehicles. Living seated crew also retain clothing-fire exposure after hull disable. CPU/live verification of these changes is pending.

The carrier's existing projectile source-hit side effect could damage running gear but did not itself consume shell/bullet energy. The crew/transport owner is separately implementing actual source-material projectile contacts; this targeting change does not claim carrier armor resistance is complete.

### 31 October renderer and first-person review

The exact-zero optical-throughput candidate retains the physical entrance and depth occlusion;
it omits only exterior/effect work whose existing radiance multiplier is exactly zero. Its
same-build diagnostic baseline requires two explicit benchmark flags. The b37 commander fixture
contains four nonzero optical entrances, so it demonstrates no work reduction or performance gain;
its baseline and candidate reference images are pixel-identical. Ultra640 live-frame mean is
about72ms including simulation, not a30/60FPS acceptance. Ultra1024 is about86ms. A separate
single-sample diagnostic changes coverage quality and is not the default Ultra result.

Build39 additionally candidates conservative per-instance optical frustum submission using
source/grass envelopes and original instance ordinals. Main-view geometry, shadows, material,
sampling and alpha rules are unchanged. Same-build counters, edge/wind/large-coordinate tests
and matched image checks gate acceptance. See docs/OPTICAL_RENDER_WORK_AUDIT_20261031.md.

The closed-hatch commander look-down extension changes orientation at the same source eye,
with unchanged near plane and normal source occlusion. It does not claim new neck/body IK or
a head-volume clearance solver. The capture follows existing cartridge/case hand targets using
ordinary look input; intervening geometry remains visible. The b38 35-second arena fixture records
two real discharges and one completed NPC reload, but dark hand contact cannot be visually
certified. The combined180-second fixture ends in a real armour-versus-riflemen stalemate; it
is not a last-man-standing success or an independent human playtest. Recordings are offline24FPS,
not proof of live render speed. No hit, casualty or ammunition outcome is injected after staging.

The two-front/three-rear lamp schematic uses individual actual source-wall facet mounts after
the common plane failed native contact validation. Painted symbols remain source-projected;
the assignment loom is visibly raised at the fittings. Source geometry and contact tolerances
are unchanged. Build39 passed all five native panel tests and the complete headless panel
review, including physical LMB selection, both wounded pulse phases, and the raised selection
wire. The root visually inspected the two-front/three-rear source-mounted arrangement. This
review stages casualty states and does not certify the full walking seat-transfer journey.

The b39 per-instance optical candidate produced a pixel-identical reference but no speedup:
70.924ms baseline versus72.133ms candidate, including simulation at640x360 Ultra on the4090.
It is being retained only as an explicitly gated experiment, not a normal-play optimization.
The b39 first-person recording retains two actual discharges and one completed reload. Its
improved ordinary look direction exposes the cartridge and arms, but dark clothing/lighting
still prevent certifying whole-body clearance or detailed hand contact from the footage.
Near-zero hand endpoint errors are not proof of anatomical motion or collision correctness.

Build40 passed all seven focused native checks for the two explicitly gated culling candidates
and the expanded cabin-source registration. The terrain experiment retains original index
order, winding and duplicate triangles; its reference is pixel-identical, but reducing optical
terrain submission from8.37million to1.50million triangles did not improve this fixture's frame
time (71.044ms baseline,71.754ms candidate). Both culling experiments remain disabled in normal
play. The baseline still before/after the registration fix is also pixel-identical; that quiet
view alone does not demonstrate the corrected installed work lamp under operating conditions.

Printed GPU pass totals omit substantial environment-probe work in the earlier instrumentation.
The unclassified render-time remainder must not be reported as measured CPU overhead. Build41
adds diagnostic timestamps without changing work or pass boundaries. Fenced headless latency
and ordinary overlapping CPU/GPU throughput must also remain separately labeled.

Build42 now supplies the same-fixture two-frame throughput diagnostic with timestamps disabled,
warmup fully drained and the final GPU wait included. The quiet commander fixture completes
17.39frames/s at640x360 Ultra and14.52frames/s at1024x576 Ultra. The corresponding fenced640
result is14.03frames/s, and its reference is pixel-identical to the queued result. This is a
measurement-method distinction, not a new shipping renderer speedup; neither meets30FPS.

The actual large-map fuel review exposed a world/local round-trip precision loss at the45mm
receiving neck. The bound now uses the original hull-local first source contact, with no radius,
reach or source-contact relaxation. Build42 passes both actual-source CPU checks and16of17
live checks, including finite5L transfer,11-owner cold restore,12 exact continued ticks and
keeper-death interruption. Final inner-cap closure stalls on a sub-micrometre residual before
the exact endpoint. Build43 snaps a proposed path position to its directional endpoint only
inside the existing38.15micrometre source uncertainty, and still checks the full swept motion.
All three focused native checks pass, including a genuine intervening source obstruction;
the complete live closing sequence remains pending at this checkpoint.

The subsequent build43 gameplay run passes all19 fuel checks, including both complete cap
closures. Five stills and the full state receipt remain under captures/tiger_refuel_20261031_b43*.
The root's visual inspection found the downward diagnostic view exposes the player's own
head/body and obscures the can/hand presentation. That capture has not been promoted as a
certified normal first-person showcase; the ordinary view's visibility bracketing needs checking.
Mechanical completion does not certify whole-body animation or first-person presentation.


## Medical transport source correction - 2026-10-31, pending build45 proof

The build43 medical journey remains a failure. Field care, loading and the approximately
30m drive passed; the patient then jammed against the actual carrier bench/rear gate. The two
side stretchers exceeded the original 1.0m aisle, and fixed 1.40m body-center placement put
real flesh below the 1.04m cargo floor. First contact was the original bench at local x=-0.5,
y=1.4445m, near z=-2.3125m.

The candidate admits one centered 0.55m stretcher resting on that real floor. It measures the
retained flesh minimum, preserves relative joints, and rejects poses that do not fit the aisle
or intersect carrier source geometry. The complete original flesh is swept during withdrawal;
the established supported height and aisle line remain until the entire body clears the actual
open gate. The visible finite winch is also a body-contact surface and must be deployed and
moved aside before loading. A clear loading-aisle prompt explains this requirement.

Older saves with side/dual stretcher patients are explicitly rejected. No person is deleted,
healed or silently repositioned for migration. Empty transports and independent injured seated
passenger records keep their existing binary format. Four ordinary passenger seats are
unchanged. Loading retains one actor-owned body and casualty, changing attachment ownership
only; expedition location now follows its actual intermediate pose rather than the future berth.

Both review harnesses deploy and push aside the same finite winch. The continuous medical
review checks patient progress separately from helper progress, and requires complete flesh
clearance before travelling to the tent. Rust parsing passed. New pure/private-source tests and
a fresh end-to-end GPU journey await parent build45; no completed evacuation is claimed.

Limits: the two-second assisted loading animation still interpolates to a source-clear endpoint
without a full-path rigid-body sweep. Withdrawal translates the retained articulated pose; it is
not a two-helper lifting simulation. Field care consumes real finite supplies but has no bandage
hand animation. The build43 downward diagnostic view shows local chest/shoulders prominently;
it does not by itself prove a duplicate head, and is not a finished first-person medical showcase.
No view-only body removal was applied to conceal it.


### Build46 candidate — staged patient lift and reusable winch clearance

Build45's continuous medical witness failed before injury at the winch's sideways route; its
receipt and video remain `captures/foundation_medical_20261031_b45.*`. The revised ordinary
route walks around the anchored kit, takes its handle, pulls it aft clear of the real gate, then
rolls it sideways. The kit is never removed or replaced. This route is pending live verification.

The former unchecked two-second loading interpolation is replaced by saved, source-derived
waypoints: withdraw, raise outside the gate, rotate/center, move inward, lower onto the one
centered board. Original retained flesh triangles admit the complete route before ownership
changes; each update checks the proposed interval against current carrier, winch, world and
Tiger source triangles. Thin intersections use a conservative swept-triangle convex hull with
an analytic rotation bound; ground remains the heightfield checked at <=2cm/1degree samples.
No carrier floor is exempted. A blocked step preserves its actor, pose and timer. Helper reach
is bounded against the actual retained source; this is an assisted rigid-pose animation, not
a new physically simulated medic/lifting-arm animation. Detached limb behavior remains a
limitation of the prior retained-body representation. These candidate checks are not yet a
passed full medical trip.

Transport owner v6 persists the exact waypoints and blocked-load state. Its typed v5 migration
preserves all existing finite identities/resources, but explicitly rejects a v5 in-progress
unchecked lift; legacy side/dual stretcher layouts remain explicitly incompatible. No patient
is deleted, healed, stacked, or silently moved during migration. The passenger exit bridge
only reports the actual existing passenger pose/readiness; ordinary exit remains authoritative.

New source fixture placement uses the original retained geometry to start above real ground
and entirely behind the open ramp. This is disclosed initial injury staging, not an escape
from a failed gameplay collision. Both harnesses retain the actual finite winch in the field.


### Build46 native source checks — confirmed, continuous evacuation still pending

The parent-run build46 native suite passed all five pure `loading_sweep` cases, the private
`centered_stretcher_fits_retained_source_flesh_and_withdraws_past_open_gate` case (including
full staged loading and rejection of the obstructed old diagonal), and both the forward
rearward-then-sideward and reverse winch routes against the reviewed original carrier source.
These are source-fixture admission proofs, not a passed complete medical gameplay journey.

The review now requires the same actor's lift timer/route to have actually finished before
reporting loaded; attachment ownership alone is insufficient. A paused lift retains its failure
reason in the report. The staged recovery review derives its already-disclosed helper starting
cut and aiming point from the original fallen flesh; ordinary source picking and loading still
decide success. No patient is moved to make the pickup succeed. The continuous medical review
retains ordinary helper movement after its initial injury fixture.


### Exact terrain bounds and lossless physical deformation saves — build48 candidate

Physical upper bounds cache exact lattice values in lazy 8x8 tiles, capped at 4096 tiles. Full
crater/rut revisions invalidate the cache; inclusive query cells and final 16-epsilon magnitude
padding are unchanged. Queries retain one event-read generation. The previous compact static
cache's 16384-crater revision alias is cleared under the event writer lock; indices beyond its
17-bit-per-axis representation bypass that cache instead of aliasing another node. No terrain
height, crater shape, rut filter or physical collision tolerance is approximated.

The save audit found that renderer track pixels had been retained but physical Detail crater,
rut and walker state was absent. `world.environment` v2 now retains typed `terrain.detail` state:
original crater order/grid, signed rut samples, exact full revisions, dirty page keys, future
footstep cadence/side, source tread registrations and openings with their upload flag. The
immutable source grid identity is checked. Imports validate completely before publication and
clear both CPU caches even when old/new revision numbers are equal. Legacy environment v1 is
accepted only without Detail; missing physical damage is never reconstructed as fresh terrain.

The nested terrain.detail v2 candidate stores exact u32 float bits: sparse u16-indexed cells when
smaller, otherwise dense pages. Positive zero alone is implicit; negative zero and negative berms
remain exact. Dense terrain.detail v1 has an explicit lossless migration. The old 512-page travel
ceiling is removed; the existing 128MiB encoded owner budget remains authoritative. Dirty upload
keys are canonicalized so restoring a HashSet cannot change their order. Physical saved dirty
flags are not fabricated to rebuild renderer caches; the separate LOD invalidation/first-frame
rehydration path is parent-owned. Native equivalence, cold continuation, malformed payload and
GPU first-frame restore tests are required; no new performance or full-world pass is claimed here.


### Build50 medical evidence and source-swept assisted turning — candidate51

`captures/foundation_medical_20261031_b50.json` confirms the same finite patient completed
field airway/dressing care, source-swept carrier loading, a 30.5 m fuel-consuming journey,
and complete withdrawal through the original open gate. Final bed admission failed. The
patient was still at camp-local approximately (-4.50, 0.407, 0.061), touching the real inner
X-legs of the spare stretchers, while the helper had already entered the bedside aisle.
The original drag operation translated a permanently sideways pose: the flesh span across
that aisle was about 1.07 m, exceeding the roughly 0.91 m gap between those supports.
Moving the helper farther or widening the bed admission radius would not fix that obstruction.

Candidate51 adds ordinary assisted yaw alignment with the actual hand-pull direction, using
the least turn whether pulling head-first or feet-first. The authored limit is 30 degrees per
second. It rotates the existing articulated segments together, preserving height, stature,
relative joint distances, identity, wounds and saved anatomy. It does not turn an upright body,
a body with attached severed pieces, or a body still undergoing source-guided carrier/cab
withdrawal. Every rotation interval is at most 0.5 degrees and is admitted against full original
flesh triangles, source world/Tiger/carrier/kit solids and an analytic rotational arc bound.
Terrain height is sampled at no more than 0.25 degrees; this is not a continuous terrain/force
solver. An obstructed rotation is rejected, without lifting or shrinking the patient.

Pure and private-source regression cases were added for aligned/blocked/turnable poses and
for the complete original-camp corridor from doorway to bedside. They are awaiting parent-run
native verification at this checkpoint. The complete clinical admission/finite-medic journey
has not passed yet. No patient, furniture or collision has been teleported/removed for the review.


### Bounded assisted-turn index and exact terrain faces — candidate52

Parent-run native51 passed all four `drag_yaw_` cases, including the complete original crew /
medical-camp corridor proof. Review then identified two limitations before a live medical rerun:
a triangle-count cap alone did not bound half-metre spatial-bin insertions for long source faces,
and terrain vertex probes could miss a narrow peak crossing a flesh face interior.

Candidate52 bounds the turn index to its known COM-local yaw envelope and caps total cell-index
insertions at 262,144. Full original obstacle triangles are retained; only the lookup domain is
bounded. Queries leaving that domain fail closed. The turn also includes the exact physical
coarse/detail terrain lattice triangles, with the same cell diagonal and direct original node
heights, in the continuous flesh-face sweep. A 4,096-node patch limit bounds this extra work.
Terrain openings require clipped excavation topology not supplied by this path, so overlapping
openings explicitly reject the turn; no synthetic floor or ramp is created. Source/index/terrain
capacity rejections show a specific held-body notice. Existing vertex checks additionally reject
an already submerged pose; they are no longer the sole terrain-intersection test.

Added regressions cover kilometre-long faces with bounded insertion work, budget and out-of-domain
rejection, a terrain peak between clear flesh vertices, original detail-node/split correspondence,
and explicit opening/patch-capacity rejection. The actual camp-corridor test now uses this bounded
index and the physical ground faces too. These candidate52 additions await parent-run tests; no
new complete medical pass is claimed here.


### Build52 runtime/test mismatch and paced-turn correction — candidate54

Medical52 again stopped at the spare stretcher supports. Its open-ground samples show the
hand-target/COM position error was usually below 2 cm: the helper's paced walking was slower
than the assisted pull, so the body continually caught up. Production had incorrectly used
that near-zero position-servo error as the yaw direction; the 5 cm noise gate consequently
suppressed turning until furniture stopped translation. The earlier source test instead used
a unit direction and therefore did not exercise the actual control-input failure.

Candidate54 steers from the actual helper-to-COM horizontal line (normally about 0.75 m),
while retaining hand-target/COM error for translation. Angular speed, original-source sweeps,
ground topology, scale, joints and admission limits remain unchanged. The source corridor
regression now advances an actual helper at 0.2 m/s, keeps the servo lag below 5 cm, and requires
the complete source body to turn and translate through the furniture. A separate pure test
reproduces suppressed turning under the old input.

The review now waits for the held body's horizontal hand-target progress before completing
a dragged waypoint. Vertical support remains governed by source collision, and the final
medical action retains its actual three-dimensional reach check. The centreline helper point
accounts for the real trailing hand distance and avoids a former look gesture that would push
the patient under an active bed before turning. Samples include actual axis, pull line, servo
error, requested/admitted yaw and rejection; a final retained anatomy/source snapshot is
written once for exact CPU replay if the scenario fails. These candidate54 corrections await
native and live verification; the complete medical journey has not yet passed.

### Medical camp aisle correction after live b54 (2026-10-31)

The b54 continuous medical run still failed against a real spare-stretcher stand after the
retained Driver had aligned with the helper's pull direction. Its complete captured 1,692
source vertices span camp-local Z -0.5144 to +0.3254 m; the old stand's inner face was near
Z -0.48 m. This was an asymmetric injured articulation colliding with furniture, not a missing
yaw request. The earlier relaxed-body fixture did not represent that articulation.

Only the two spare stretcher stations are moved from Z +/-1.20 to +/-1.60 m. Their original
meshes and stands remain visible and solid. Four physical stretchers and two actual treatment
beds remain; no source scale, casualty articulation, collision tolerance, bed admission reach,
person, or supply changes. The original 1.10 m stretcher half-length leaves 0.20 m to the
canvas, and its X bounds leave 0.183 m to the supply racks. Stand geometry leaves a 1.70 m
floor-level aisle. The stretcher canvas overhang above that aisle still exists at its real height.

An ignored CPU regression loads the exact b54 failure receipt and complete camp source. It
requires the original layout to obstruct that injured Driver, then sweeps the same 1,692 flesh
vertices through the rearranged aisle, bounded yaw turn and actual bedside approach without
changing height, scale or articulation. A second source test checks furniture bounds, rack
separation and unchanged capacity. Python source measurements found the actual retained
horizontal radius is 0.6672 m and maximum local height is 0.5969 m, below the 0.65 m spare
stretcher canvas. Native tests and a new continuous live replay remain required at this checkpoint;
the failed b54 receipt and video are preserved.

### Held cartridge common-hull frame correction (2026-10-31; verified through build58)

The loader's last Carry/Ram pose is now stored relative to the actual hull, so shared
vehicle translation, pitch, roll and post-service contact projection carry the physical
round, hand targets, ammunition picking/hits and interrupted drop together. Turret-only
motion does not forcibly carry a rejected round: with a stationary hull, the retained
world result remains the previous behavior. New crew.runtime4 exports use a hull-local
matrix; v3 owner migration converts its original world matrix using the exact restored
saved hull, without ticking the procedure. The whole-world source/build fingerprint
gate is unchanged; this does not promise arbitrary older-binary save compatibility.

This is a frame-covariance correction, not cartridge contact simulation. AI Carry/Ram
still checks hand reach/posture without sweeping the cartridge against rack/breech/crew
source geometry. No change to that path, the conservative basket walking envelope,
the loose-shell solver or relative turret-motion response is claimed. The rejected
turret-local candidate remains outside registered source.

Five new tests cover source vertices, actual triangle picking, grip/drop pose, blocked
Carry/Ram through common hull movement and late chassis projection, frozen-hull turret
motion, accepted-pose parity and typed v3/v4 Work payloads. Legacy migration includes
camp-scale coordinates (-5280,31.9,3230), printing maximum source/grip/drop roundtrip
error; its f32 world matrix is not claimed bit-identical. All five tests passed in native57.
At the camp-scale coordinates, maximum source/grip error was0.244617mm and drop-source
error0.244260mm. Version4's zero-time payload bytes remained exact. The full native57 run
passed2390 tests, with92 ignored and75 filtered; production code was unchanged for release58.

Build56 passed three of the five new tests. The two failures were the test's ray-distance
comparison at an exact radial facet seam, after the geometry comparisons had passed.
The recorded near-origin legacy conversion errors were2.861 micrometres for source
vertices,1.907 micrometres for grips and2.708 micrometres after drop conversion. The
existing ammunition picker uses strict per-triangle barycentric tests and is not a
watertight seam intersector. Build57's frame test aims at an actual source-face interior
with the same distance tolerances, retains the seam measurements as explicit diagnostics,
and does not claim to have repaired that separate picker limitation.

The full-world witness now adds two bounded branches reached by one ordinary existing-round
discharge and the real loader procedure. It must observe a held cartridge partway through
Carry and Ram, preserve its source identity, compare all owners and exact zero-tick world
pose after cold reconstruction, then compare twelve ordinary120Hz continuation ticks.
Only initial observer placement aboard is a disclosed fixture; no Work phase, cartridge,
health or tank pose is injected. Release58 passed both branches: all eleven owner payloads,
source identity and world pose were exact before any simulation tick, then all eleven owner
payloads and Work matched after twelve ordinary120Hz continuation ticks. The same run also
passed the paper/manual, retained frontend candidate and corpse-allocation branches. Evidence:
`captures/foundation_world_20261031_b58.json` and its Carry/Ram snapshots and restored images.

The preserved build57 failure was a review-input mismatch: its uninterrupted loader branch
inherited an unmuted manual candidate, while the cold continuation was muted. Only acoustic
state diverged; exact zero-tick restoration and continued Work had already passed. Release58
applies and asserts the same existing mute policy on both sides, with both receipt flags true;
it retains every acoustic owner byte comparison. No production acoustic code or tolerance
changed. This proves muted owner continuity, not live-audio cache/voice continuity, and does
not close the separate cartridge source-sweep, seam-picker or moving-loader visual limitations.

### The March as a loop — 2 November 2026

- **Opening stock is authored state, not a model** (`catalog/expedition_theatre.v1.json` `start`,
  `expedition_theatre::Start`, applied by `Game::stow_main_gun_loadout` at the theatre's first
  install): 420 L, 24 PzGr, 8 SprGr. The fiction is a Tiger low on ammunition inside the pocket;
  no standard-issue claim is made and the catalog note says so. The spec refuses a loadout beyond
  the 64 cradles plus the chamber or fuel beyond the 540 L tank.
- **The map's range figure** (`standalone_fuel::road_range_km`) is the existing authored campaign
  drain law evaluated at 20 km/h (13.2 L/km, 32 km from 420 L); it adds no new law and inherits
  that law's standing limit (a gameplay calibration, not a measured Maybach fuel map). Gate:
  `standalone_fuel::tests::the_map_range_is_the_drain_law_at_road_speed`.
- **`expedition-loop-review` fixtures, disclosed in its receipt**: drives are staged along the
  catalog's road graph with the fuel charged by the drain law (the Tiger is placed, not driven);
  fights spend two PzGr per enemy tank by flag (no shot is fired; no ballistic claim); carts and
  cans are taken by flag rather than through the hands; the walk from camp to the Tiger and the
  remount are staged. The harness answers whether the loop's accounting closes, not whether the
  route can be driven or the fight won.

### The written order prints — 2 November 2026 (`src/order_printer.rs`, `src/order_receipt_render.rs`)

- **The teleprinter head's place is measured, not guessed.** Its turret-local box
  (x 0.84..0.99, y 2.06..2.22, z -0.60..-0.38) was chosen on a 5 cm occupancy map of the baked
  turret's source triangles between the traverse gearbox and flywheel below (source parts
  Turret_0137/0142, top y 1.99), the roof underside (y 2.30) and the wall (x 1.0 there), beside
  the commander's post (0.564, floor, -0.423). The hanging strip (76 x 228 mm) ends above the
  fixture at y 1.83. Gate: `order_printer::tests::the_head_and_strip_stand_clear_of_the_turret`
  (exact triangle-box test against the bake when the asset root is present). The head itself is
  a project-authored utility prop (a closed box, a slot plate, four keys), not a specific model.
- **The reviewed Enigma does not fit the commander's side.** Its transit case is 431 x 544 x
  290 mm; the only clear volume at the commander's left is about 150 x 160 x 220 mm. The machine
  therefore stands on the camp's orders table beside a second head (the detail's end of the
  line), seated on the table's measured top (`camp_table_top`, from the placed part's source
  triangles; a 0.80 m fallback is named in code); the hull Enigma at the radio station is unchanged.
- **The paper is a page teleprinter's**: 210 mm wide (the Siemens page printers' roll), torn off
  at 150 mm, 60 characters a line at ten to the inch inside 20 mm margins; it leaves the slot
  along the head's top edge and hangs leaning 25 degrees inboard so the commander in the cupola
  reads its face rather than its edge (the first still of 2 November showed a 76 mm tape hanging
  straight down, edge-on and illegible). The lean is a design value, not a modelled paper curl.
- **Print cadence is authored**: four lines a second (`LINE_SECONDS`), faster than a period
  teleprinter's ten characters a second, chosen so the order is readable in seconds; the sound is
  the bank's switch recordings at -14 dB per line, not a teleprinter recording.
- **The ink is albedo**, repainted only when a line is added, under the cabin's and the camp's
  real light; the strip in hand is a view-local raster-only object like the held map.
- **The line to Stab is words, not simulation** (`Game::stab_transmit`, `radio_llm::submit_stab`):
  the model path and the offline responder both receive only the simulation's own context
  (order, beat, stock, notice, camp, position, clock, weather) and return text that prints on
  the strip; nothing they say changes the simulation, spawns, supplies or alerts anything.
  The platoon radio's boundary (`docs/PLATOON_MODE.md`) applies unchanged.
- **Bridge orders stand on the surveyed deck.** The theatre moves a Bridge site onto the surveyed
  crossing at install; the authored sheet (written from the catalog) placed its contact, hold
  and bypass points on the catalog's point, 139 m off the Southern crossing's deck and 77 m off
  the Northern. `expedition_accept_order` re-seats every beat on the catalog point to the
  theatre's site position and logs it. Found by `expedition-loop-review` (2 November 2026),
  which now plays all eight orders through; the receipt is the gate.

### The manual as a Fibel, and the Tiger CAD in the game — 2 November 2026

- **The repair manual's pages are A5 landscape** (`src/repair_manual.rs`: 210 x 148 mm pages,
  the open book 0.432 m wide, ink at 1536 x 1086), set in the owner's Neue Haas Grotesk
  (`assets/fonts/NHaasGrotesk-Md/-Blk.otf` under the asset root, loaded at run time into the
  ink atlas as `ui::FACE_MANUAL`/`FACE_MANUAL_BOLD`; the bundled pixel faces stand in and the
  log says so when they are absent) with the Tigerfibel's design: letter-spaced capitals
  titles, a red rule, a red margin label, red lead words, choices as red arrows, the folio at
  the outer corner. The design is borrowed from D 656/27 (public-domain scans); the words are
  the game's own diagnosis tree, unchanged in meaning.
- **The plates are drawings of the owner's kit CAD, not of the vehicle.** `tools/bake_manual_plates.py`
  scales the 1/35 SolidWorks STEP set by 35 and projects it through FreeCAD TechDraw with hidden
  lines removed, a red layer for the part a page is about (the Fibel's spot colour), to
  `assets/manual/plates/<name>.png`; every plate's caption says "1/35 kit CAD, not to scale" and
  its report names the source. A page whose plate the asset root lacks keeps its schematic.
- **The Tiger's assemblies as meshes** (`tools/bake_tiger_cad_parts.py`, `_glb.py`):
  `assets/tanks/tiger/cad/<group>.obj/.glb` in the game's hull frame (the gearbox, steering
  unit, forward drive, final drives, fuel tanks, radiators, fans, sprockets, idlers, side
  plates, sponsons, the hull tub and the engine), tessellated at 4 mm deflection and placed by
  the damage session's measured registration: the game's own Tiger GLBs are this kit CAD at
  0.038919827 m per kit millimetre with identity rotation and a (0, 0.571, 2.658) m offset,
  fitted on 446 solid pairs to 0.17 mm RMS, which also means the game's Tiger is 11.2 % larger
  than the kit's stated 1:35 would make it. They are a kit's shapes: filled solids with no
  casing thickness, no interior detail, no fasteners the kit omitted; the game may use them as
  physical parts and the damage solver as component outlines, never as a measured vehicle.
  Nothing of this is committed; the bake reports carry the provenance.
- **The loupe is an optic composite, not a texture zoom** (`repair_manual::loupe_view`, crush
  mask mode 5, `main.rs` lens pass): while the right button is held over the open book, a lens
  pass renders the paper from the lens itself (its own view and temporal history, like the
  gunner's eyepiece and the binoculars), looking down the page normal with a field that spans
  `LOUPE_DIAMETER / LOUPE_POWER` of paper (21 mm across a 64 mm glass, 3x), and the reading
  frame seats that crushed picture in the circle the lens makes on the screen, resampled once.
  The lens's diameter, height over the page (55 mm) and power are design values of a hand
  glass, not an optical model: no refraction, no field curvature, no glass reflections; the
  ring is the only mesh. The lens view's eye adapts over its first frames as the optics' do.


### The manual and the sheet read as overlays — 2 November 2026 (`repair_manual::overlay_mode`)

- **What changed.** At the crush grid the Fibel's body text on the tilted physical page could
  not be read, so the owner chose a top-down reading: the open book and the torn-off sheet are
  drawn flat at the display's resolution over the crushed frame (`repair_manual::draw_manual_overlay`,
  `renderer::draw_order_sheet_overlay`) from the same retained ink the physical pages carry
  (2048 x 1448 pages, 1024 x 731 sheet, imported into the ink layer's image registry by
  `ui::Ui::import_image_view`), through the policy play already uses for the HUD's words. The
  pages are images; the boards, the gutter's shade, the chapter tabs, the turning leaf (its
  width the cosine of the leaf's angle, its shadow the sine), the ribbon's tail and the torn
  edges are the ink layer's rectangles.
- **What is not physical, named.** The overlay is not lit by the scene: no transport reaches it.
  Its brightness is a scalar from the reading light at hand (`Game::reading_light`: 1.0 with
  the cabin lamps on or on foot, 0.92 with the hand torch, 0.30 in the dark cabin), not a
  measured illuminance; the leaf's shade and its shadow on the page are drawn, not cast; the
  turn is the 2D projection of a hinged leaf, not a lit one. The ink is unchanged and not
  emissive. The loupe does not apply (nothing to magnify at the display's resolution).
- **Gate.** `KESSEL_MANUAL_3D=1` keeps the physical book and the sheet in hand in the scene,
  lit by transport, with the loupe; with it unset (both shipping presets) the overlay is the
  reading, the 3D parts stay invisible and the camera's field is untouched. Neither preset
  changes emitters, materials or any law: the choice is presentation. `manual-overlay-review`
  is the overlay's capture (`captures/<prefix>_overlay_*`), `repair-manual-review` the physical
  book's; `repair_manual::tests` hold the layout, the tabs and the leaf.
- **Why it is allowed.** The first principle admits the written manual and the written order;
  the second forbids faking light in the scene. The overlay fakes nothing in the scene: it is
  paper held up to the player's eye outside the image, as the map's HUD words already are. The
  debt is that the paper's brightness does not follow the cabin's real light; that is the
  scalar above, and the gate keeps the lit book.
- **Since the same night, on the pixel grid.** The owner found the display-resolution book too
  fine for the game ("adjust the information and text and font and drawing lines to fit the
  pixel size, not the other way around"). The overlays are now ink on the internal grid
  (`Renderer::request_pixel_ink`, flushed over the crushed frame before the blit, so they are
  upscaled with the frame's own pixels): the page and sheet textures are exactly the internal
  pixels they cover (`repair_manual::overlay_page_pixels`, `order_receipt_render::sheet_pixels`,
  nearest-sampled), the type is the 5 x 7 pixel face (`pixel_font::draw_at`, titles two dots),
  the rules and the schematics' pen one pixel, the plates re-baked for pixel line weights and
  thresholded to solid pixels (`tools/bake_manual_plates_px.py`, `assets/manual/plates/px<W>/`),
  the spine two hard lines, the shadows hard offsets, the page 1.25:1 (taller than A5 landscape,
  the owner's "a bit taller"), zoom two pixels per texel. The rest of this entry stands.

### The sky's month, discs, stars and the eye's night — 2 November 2026 (`docs/SKY_NIGHT_20261102.md`)

- **The Moon's month is ten days** (`celestial_sky::SkyClock::moon_cycle_days`,
  `KESSEL_MOON_CYCLE_DAYS`; 29.530589 restores the ephemeris exactly): the Moon's geometric
  position is evaluated at the act clock's start plus the elapsed time scaled so that the
  elongation from the Sun completes a cycle in ten of the act's days (by the mean motions, 2.9
  times faster). Its phase, rising, setting, distance and illuminance stay consistent with each
  other and with the Sun; only the month is shorter, as the day already is (`KESSEL_DAY_CYCLE_SCALE`).
  A named approximation of the day cycle's kind, tested against the real ephemeris at the real month.
- **A map may pace its own day** (`environment_lighting.day_cycle` {night_s, blue_s, twilight_s,
  day_s}, `Renderer::day_cycle_play_s`; absent is the campaign's 300 s night with its blue hours,
  150 s each twilight, 300 s day): the sky clock's rate in each band (night below -14 deg, blue
  hour -14 to -6, twilight -6 to +8, day) is the date's real seconds in that band over its play
  seconds, as before. The spring bridge map runs each blue hour for 1,500 play seconds, each
  twilight for 240 and the deep night for 120 (owner, 2 Nov 2026, of the blue-hour meadow film:
  "this should be like the whole night"). The Sun
  and the Moon stand where the ephemeris puts them at every instant; only the clock's rate
  changes, and with it the eye's pigment rate. A pacing choice that changes no light.
- **The discs are 1.6 times their angular size** (2.1 since 3 November 2026, below; `celestial_sky::DISC_DISPLAY_SCALE`,
  `KESSEL_DISC_SCALE`; 1 is the real sky): the irradiance is untouched, so the light on the ground
  and the disc's total light are unchanged, the disc's radiance falls by the square of the scale
  and the penumbrae widen with it. A presentation choice.
- **The stars have the real sky's population** (to magnitude 7.5 since 3 November 2026, below; `physical_environment::STAR_*`): a star in 16 %
  of the lattice's 55,296 cells, about 8,800 to magnitude 6.5, the count brighter than m rising as
  10^(0.5 m) (the real slope), each star's irradiance from its magnitude (V = 0 is 3.6e-11
  W m^-2 nm^-1 at 550 nm), drawn into one internal pixel by the raymarch pass as before. A seeded
  distribution with the real population, not a surveyed catalogue: the constellations are not the
  sky's. The CPU reference, the sky shader and the shade pass carry the same constants.
- **The eye's pigments run at the clock's rate** (`Renderer::pigment_rate`, `observer::Eye::step_at`,
  `exposure.wgsl` pigment_rate): with the day cycle on, the slow bleaching and regeneration of the
  cone and rod pigments advance at the sky clock's rate over real time (clamped 1 to 60; 1 with
  the cycle off), so a night of thirty play minutes is not spent in the cone plateau while rods
  that need forty real minutes recover: the crewman at midnight is as adapted as a real crewman at
  midnight. The pupil, the neural gains and the flash memory keep real time. A reproduction
  choice that changes no light; the baseline showed the fully adapted night blue-grey and the
  moonlit night bright, so the black the owner saw was the unadapted eye.
- **The night's hue is deeper outdoors** (`observer::ROD_DISPLAY_HUE`, 0.675/1.022/1.738,
  luminance-neutral); the cabin's own hue (`CABIN_ROD_DISPLAY_HUE`, the former 0.80/1.00/1.45,
  removed 2 November 2026, see below) was kept at first (`crush.wgsl` mixes the two by the buttoned-up view's eye_range_strength as it
  does the rod gain), so the interior's look is untouched; the interior stills' photometry before
  and after is in the sky note. Since 2 November 2026 the cabin's hue is removed (the owner: "the interior
  now has a very blue hue", once the compartment's paint became a white; "i think we can remove
  this"): inside the hull the rod term is drawn achromatic, as the rod signal is.
- **The display tonemap is AgX** (`crush.wgsl` `agx`, `tonemap`; `KESSEL_TONEMAP=aces` keeps the
  former fit): Sobotka's AgX in its minimal form (the inset, a 16.5-stop log2 encoding, the
  sigmoid, the outset, the 2.2 decode), chosen because bright colours desaturate toward white
  instead of skewing hue, which is what the owner's "oversaturated, deep fried" sunset was: the
  ACES fit turning the orange of a three-band sunset yellow. A reproduction transform like the fit
  it replaces; it changes no light. The CPU reference `observer::aces` stays as the mapping's
  monotonic reference in the tests. The exposure into the curve, ×1.3, is the named
  `observer::AGX_KEY_TRIM` (the key at about 0.27 display-linear as under the former fit; AgX
  alone gives 0.21), a reproduction choice kept on 2 November 2026 when the leftover constants were
  reviewed.
- **The aerosol is a winter continental background** (`AtmosphereParameters::default`:
  Mie scattering 4.0e-5 and extinction 4.444e-5 per metre at sea level over the 1,200 m scale
  height, optical depth about 0.05 at 550 nm, single-scattering albedo 0.9; first set at 0.08,
  which whitened the day sky until the clouds lost contrast): AERONET continental
  winter climatologies run 0.05 to 0.10, and the cauldron's smoke adds; Bruneton's 0.005 was a
  clear mountain sky, under which the three-band sunset was the saturated orange of a
  monochromatic transmittance. A sourced estimate, not a measurement of the day. Spectral since
  2 November 2026 (Angstrom alpha 1.0; see "Spectral aerosol and the surfaces' clear air").
- **The pastel palette** (`KESSEL_LOOK=deltap-pastel`, `crush.wgsl` `DP_PASTEL`, mode 4): thirty-two
  display-sRGB entries authored for the owner's taste (green-blues and teals, creams, two warm
  dawn tones, snow, steel, fire), dithered by the look's OKLab best-pair search as the PICO-8 mode
  is. A recording-medium choice in the sense of the look's other modes: it discards tonal
  precision and never adds light.
- **Ozone is band-averaged** (`AtmosphereParameters::default`: 1.311e-6, 1.881e-6, 0.228e-6 per
  metre for the red, green and blue bands): the Chappuis absorption averaged over each band's
  width in place of Bruneton's point samples at 680, 550 and 440 nm (0.650, 1.881, 0.085 e-6).
  The point samples absorbed the green band three times more than the red through the long
  twilight paths and painted the civil twilight magenta; the averages keep it blue-grey. The
  three-band model's own approximation, stated; the illuminance gates still pass.

### The sky through the day — 3 November 2026 (`docs/SKY_DAYNIGHT_20261103.md`)

The owner's direction of 2 November carried on: the sky and environment lighting, the day and
night cycle, the sunrise and sunset transitions; more and brighter stars at one pixel each;
bigger discs; the Moon's phases over a ten-day month.

**Held behind `KESSEL_SKY_LOOK=full`** (`celestial_sky::sky_look_full`; the owner's rule of
3 November 2026: a change to how the picture reads goes behind a setting and comes to him as
before-and-after stills, the default only on his yes): the twilight's higher orders under the
spherical cosine (`AtmosphereParameters::spherical_higher_orders`, the GPU flag in
`secondary_direction.w`), the earthshine, and the Moon's disc by day. Unset, those three are the
former sky: the plain cosine (the Earth's shadow black, as the fault below), no earthshine, no
Moon while the Sun keys. The parts he asked for by name are on either way: the stars, the discs'
size, the Moon's phase and terminator, and the clock's pace.

- **Twilight's higher orders in a spherical atmosphere** (`physical_environment::spherical_cosine`,
  `chapman`; `shaders/physical_environment.wgsl` `physical_spherical_cosine`, `physical_chapman`).
  A named approximation in place of a fault. The multiple-scattering field (the two-stream
  estimate of the clear column above a point, and the deck's diffuse source) took the plain cosine
  of the body's zenith angle and was zero wherever the body stood below the point's horizon, so
  the instant the Sun set every second-order photon vanished: the Earth's shadow and the whole
  antisolar sky after sunset rendered from single scattering alone, black at the horizon (the
  spring bridge at 16:45Z), and under the canyon's deck the cloud base lost its sunlit air above
  at the same instant. The two-stream solve now takes the effective cosine 1/Ch, Chapman's
  grazing-incidence function for the molecular scale height (Schueler's 2012 approximation, with
  the planet's screening past the horizon by Ch(x, chi > 90) = 2 Ch(x sin chi, 90)
  exp(x - x sin chi) - Ch(x, 180 - chi)). In an exponential atmosphere the energy a column takes
  from a beam per unit of ground is the irradiance over Ch, so the plane solve with this cosine
  is the sphere's while Ch varies slowly up the column; below the horizon the screening,
  exp(-R delta^2 / 2H) for a depression delta, is constant up the column to first order. It is the
  plain cosine above some 20 deg, about 1/35 at the horizon, 1/5,000 at -6 deg and nothing by
  -18 deg. The deck's own top still takes the plain cosine (a surface the Sun has set on). Measured
  (clear, the new Moon of 6 Jan 1943 at Stalingrad, CPU reference): 920 lx at sunset, 134.5 at
  -3 deg, 5.16 at -6, 0.093 at -9, 0.0024 at -12 against USNO 171's 984, 66.4, 2.99, 0.116, 0.0044,
  every value within a factor of two and falling steadily; the antisolar sky a degree up at -3 deg
  28 cd/m^2, blue over red 2.0, against the zenith's 23. Limits: one scale height for the
  column (the aerosol's is shorter, its share at grazing light overstated); no refraction of the
  grazing beam; the ground's upward flux still left out. Gate: `celestial_sky::tests::
  twilight_falls_as_measured_and_the_earths_shadow_is_lit`, `physical_environment::tests::
  a_hazy_sky_whitens_toward_the_horizon_away_from_the_sun`.
- **The Moon's disc shows its phase** (rm_common.wgsl `rm_moon_disc`, `celestial_sky::MoonDisc`,
  `CelestialLighting::moon_disc`). Before, the disc was one flat circle whose brightness alone
  followed the phase. A point of the disc is now a point of the lunar sphere facing the eye, sunlit
  where its normal faces the Sun (the Sun's direction from the Moon taken as from the Earth, within
  0.15 deg); the sunlit part carries the ephemeris' whole irradiance at one radiance (named: the
  lunar surface's weak limb darkening taken as none, the Lommel-Seeliger law's mean); the drawn
  sunlit share is (1 - m.s)/2 from the two directions, tested against the ephemeris' illuminated
  fraction to 0.01 through a ten-day month that waxes and wanes. The night side shows the
  earthshine: the full Moon's radiance times the Earth's geometric albedo (0.367), its solid angle
  from the Moon and its Lambert phase at the supplement of the Moon's (about 1e-4 of the full Moon
  beside a crescent, tested): the ashen disc. The disc is drawn whichever slot carries the Moon's
  light and by day too (its light is still dropped while the Sun keys, `MOON_DROPPED_BOUND`; its
  disc had vanished with it and popped in as the Sun crossed -1 deg). The shader renderer's
  `physical_disc_radiance` (reflections there, not the shipping raymarch) keeps the flat disc.
- **The discs are 2.1 times their angular size** (`celestial_sky::DISC_DISPLAY_SCALE`, from 1.6;
  "a bit bigger again"): the Moon about twelve internal pixels across at 1024 x 576 so the phase
  reads; irradiance untouched as before. A presentation choice.
- **More stars, to magnitude 7.5** (`physical_environment::STAR_*`): 170 cells a cube face, a star
  in 16 %, about 27,700 over the sky with the same 10^(0.5 m) count law, the count to 7.5 that a
  dark-adapted eye reaches under a sky with no artificial light (Bortle class 1, as the 1940s
  steppe was; the real sky has some 25,000). Still one internal pixel each. Physical, not a choice.
- **The one-pixel star's flux times 4 on the display** (`physical_environment::STAR_POINT_GAIN`,
  rm_shade.wgsl `RM_STAR_POINT_GAIN`). A presentation choice: a star is a point, seen by its
  flux over the eye's summation area; the scene's 90 deg field is shown on a panel the player sees
  some 50 deg across, so a pixel's flux reaches him in about a third of the scene's solid angle
  while the sky round it keeps its radiance, and every star reads about three times fainter
  against the night than outdoors. Drawn into the star's pixel only; it lights nothing (the
  diffuse starlight in the night-sky layer is unchanged). The star pixel's ceiling rises from 3 to
  5 times the display's white (`RM_STAR_CEILING`), past AgX's white so the brightest few dozen read
  full white through the dither; the deltaP bloom adds its half-texel halo at most.
- **The clock's pace changes smoothly** (`renderer::blended_clock_rate`, `PACE_BLEND_HALF_DEG` 2):
  the four phases' rates are blended in their logarithm by a smoothstep 2 deg either side of each
  edge (-14, -6, +8 deg). The spring bridge runs its blue hour some seventy times slower than the
  night beside it; the former linear blend over one degree made the blue hour's end a lurch, and
  the eye's pigment rate, which follows the clock, lurched with it. The phases' play times move by
  a few per cent. A pacing choice that changes no light. Gate: `renderer::atmosphere_tests::
  the_clocks_pace_passes_between_phases_without_a_step`.
- **The far cloud banks take the cascades' shadow** (rm_shade.wgsl `rm_low_cloud_far`): beyond the
  froxel grid the banks' key beam is now occluded by the depth cascades as far as the far cascade
  reaches (some 700 m), so at a low Sun a bank in the lee of a rise or the town is shaded with the
  field under it instead of standing sunlit and white over it. Beyond the far cascade a bank still
  takes no terrain shadow (named; the open item is a horizon map of the terrain).

### The frame after the first kill — 2 November 2026

The owner, playing the merged line: "fps tanks when i hit first t34 and never recovers".
Measured with `gfxbench high 600` under `KESSEL_BENCH_LIVE_FRAME=1 KESSEL_BENCH_KILL=60` (a
catastrophic kill of the nearest enemy at timed frame 60, the summary split before and after it,
the simulation's phases after it): the GPU frame did not change (16.4 to 12.8 ms mean) and the
simulation went from 3.9 to 12.9 ms a frame (worst 34.7), the phase `wrecks_fire_haze` at 7.4 ms
of it, for a wreck 1.7 km away. The cost was the thrown turret's rest test: a settled body
re-certified its support every frame by sweeping every surface sample of the turret against
the wreck's hull (`wreck_turret::Body::supported_at_rest`). The test is a function of the body's
pose, the hull's pose and the terrain, none of which changes while the body is settled, so the
body now keeps the hull pose it was certified against and repeats the test only when that pose
changes or every two seconds (`REST_RECHECK_S`); a restored body re-tests on its first step. A
cached result of the same test, not a change to the contact model; if the terrain ever deforms
under a wreck, the cache must be keyed on it too. With the turret quiet the simulation still
climbed to 120 to 170 ms a frame ten seconds after the kill, all of it in `enemy_ai`, and the
probes placed it in `settle_incapacitated_enemy`: a living enemy that has capsized runs the
rolled hull's support solve (`vehicle_rollover::advance_query`, every triangle sample of its
hull and turret against the terrain, substepped) every frame for as long as it lies there,
60 to 280 ms an update for one T-34. The solve's result is a function of the hull's state and the
terrain; a capsized hull at rest on a stable support now keeps the state it was certified in
(position, attitude, heading, heave and angular rates, `Ride::rolled_rest`), as does a hull the
solve leaves exactly as it found it (a fixed point: the capsized T-34 lay a hair above its
support with nothing to integrate, so the stable-support exit never came), and is solved again
only when something changes that state (an impulse, a displacement) or every two seconds
(`ROLLED_REST_RECHECK_S`). A cached result of the same solve, not a change to the rollover
model. Measured after (the same bench, 900 frames): the simulation 3.8 ms before the kill,
21 ms through the turret's five seconds of tumbling, then 5.5 to 8 ms for the rest of the run
(was 120 to 170), the enemy AI 0.09 ms, the run 78 fps mean with a worst frame of 19 ms. The
one open question from it: a T-34 of the platoon capsized while manoeuvring 1.8 km away ten
seconds after the kill, which is the AI's driving, not the frame. Also in the same measure: the daylight apertures' measure every eighth frame (above),
and the run log's `[frames]` lines and the audio's starved-block count at exit for the next
playtest; the bench's `KESSEL_BENCH_KILL` split, its per-second series and its probes stay for
the next report.

### Daylight through the ports — 2 November 2026 (raymarch backend)

The owner, buttoned up at noon: "light still doesn't seem to make it into the interior from the
side vision port or the gun sights during day. it looks identical to night." Measured in the
act-1 world (`sky-review`, the interior section rebuilt from the editor's hall to the world):
the loader's seat displayed at 0.0487 at noon and 0.0475 on the moonlit night with the lamps on,
and with them off at noon the scene mean was 0.16 cd/m^2; the gunner's sight picture was full
daylight (1,745 cd/m^2) and the loader's port showed the steppe through a slot a few pixels
tall, so the pictures were right and the transport into the cabin was not. The cause: a cabin
surface's sky came only from the two cosine-weighted occlusion rays a pixel casts each frame
(`rt_open_share`, glass passed), and a Tiger side port is a glass block at the inner end of a
slot through 22 cm of armour (`tools`-side ray test of the bake: open at the glass's centre,
walled two centimetres above and below) that subtends a few degrees: the rays never found it.

- **The apertures' term** (`shaders/rm_aperture.wgsl`, `rm_shade.wgsl`, `rm_volume.wgsl`,
  `renderer_raymarch::aperture_records`): every visible pane of the player's vehicle, the direct
  glass (the ports, the visors) and the periscopes' windows, but the sight's eyepiece, is carried
  to the frame piece by piece (`pane_quads`, one per connected piece of a glass page: the
  turret's page holds both port blocks and the sight's lens) as the piece's centre and principal
  axes with its transmittance (the pane shader's dielectric entry and laminate absorption over
  the glass, less the glass load). A compute pass asks the scene's own geometry (ray queries,
  passing the periscopes' windows and lenses and folding at their mirrors) which of the piece's
  two shorter axes, either way, is the pane's face (the one most of whose rays leave the vehicle
  within 8 m: a port's block is thicker than it is tall, so its proportions do not say), how much
  of that face is open (a 9 x 5 grid of rays), the tilt across each axis at which a ray from the
  open part's centroid still escapes (bisected; the two sides averaged), and the direction the
  centre ray leaves in (a periscope's, folded); the shaders take the measured face. A surface in
  the crew compartment then takes each pane as a finite source: the cached sky along the pane's
  measured direction (the mean upwelling ground band below the horizon), its projected solid
  angle by Lambert's contour integral (exact for a quad above the surface's plane), the pane's
  transmittance, the measured open share, the slot's acceptance (1 - tan(angle) / tan(half-angle),
  exact for a straight rectangular slot), and one ray toward the pane for the compartment's
  fittings in the way; the diffuse and the split-sum specular lobes both take it. The cabin's
  air takes the same source over the pane's solid angle through the medium's phase (no
  occlusion ray in the froxel pass). The stochastic occlusion rays of a compartment surface now
  count panes as cover (`rm_ray_open_sealed`, the periscope path kept), so a pane is counted
  once. No opening is invented: a pane walled in measures closed and gives nothing. Named
  approximations: the sky over a pane's field is taken along one direction (a port's field is
  degrees wide); the exterior seen through a port (the deck, the fender, a wall beside the tank)
  is the ground band, not resolved geometry; the acceptance is symmetric per axis; a relay's thin
  lens passes a ray straight on the energy path (its imaging stays the relay captures'); without
  ray queries the record is open and unrestricted on the piece's shortest axis and no fitting
  shadows the pane (the raymarch backend's portable fallback). The key beam through a slot was
  the shadow cascades' here; since the same day it is traced (below, "Sun shafts through the openings"). Gate for the A/B: `KESSEL_RM_SKIP=2097152` leaves the term
  out; `KESSEL_RM_PROBE` prints the measure (`[rm-aperture]`). Cost: the measure's ~200 ray
  queries per pane run on one thread each and cost 1.07 ms a frame every frame (gfxbench, the
  merged line of 2 November 2026, the largest single pass); it now runs every eighth frame or
  when the pane count changes (`APERTURE_MEASURE_PERIOD`): the panes ride rigid parts and a
  turret's traverse moves a port against the hull slowly, so a record at most eight frames old
  is the same measure. A sampling cadence, not a transport change; open: the pass could use a
  workgroup per pane.
- **What the measure says of the Tiger** (`KESSEL_RM_PROBE`, act 1): each turret side port reads
  20 % open with half-angles 23.5 x 1.6 degrees (a 1.2 cm slot through 22 cm of housing: the
  bake, confirmed by a CPU ray test), so a wall opposite takes sky only in a band under 6 cm
  tall; the cupola's periscope entry windows 80 % open at 43 x 18 degrees but outside the
  compartment behind their housings; the driver's visor 96 % open at 38 x 20 degrees. With the
  records forced open (`KESSEL_RM_APERTURE_UNMEASURED=1`) the lamps-off noon cabin went from
  0.16 to 0.51 cd/m^2 mean; measured, it is the physics of a buttoned-up Tiger: the ports and
  the sight pictures are bright, the walls are not. The honest daylight is the hatch: the
  loader's seat with the cupola open at noon, lamps off, 0.55 cd/m^2 mean against 0.16 buttoned
  up (`captures/demo-20261101-daylight_in_20261102l_hatch_sky_*`, `interior_noon_hatch*` in
  `sky-review`). Open: the cupola periscopes' lower windows (the ones that face the commander)
  measure closed (`captures/demo-20261101-daylight_in_20261102p_sky.json`, records 3, 5, 7, 9,
  11, 13, 15) although the rays follow the mirrors' folds and pass the windows and lenses, with
  eight segments and the rays started outside the pane's slab; so the cupola ring takes no
  daylight from its periscopes yet. The cause is not found (the housing between the lower
  window and the mirror, or the periscope's actual optics); until it is, the cupola by day is
  its relayed pictures and the hatch.
- **The night gain at most the key** (`crush.wgsl`, `observer::rod_display`): the rod channel
  times the night display gain (ROD_DISPLAY_GAIN 6 outdoors) is now capped at the display's
  key, 0.18: the gain lifts the dark toward mid-grey and never past it, the moonless night
  unchanged below the key. A bound on a named display departure, not a change to the eye or the
  scene, written against the owner's "some exposures are fully white at night", "in the sight".
  Not the cause: the model's test (`the_night_gain_lifts_the_rods_toward_the_key_and_never_past_it`)
  puts the lifted channel of an eye adapted to the moonlit sight field at 0.11 and the cones
  below 1e-3, and the headless sight at the Christmas Eve Moon (`captures/demo-20261101-sight_night_20261102_sim*.jpg`,
  the eye converged) is a pale field, not white; the live white-out's cause is open (the
  moment, the lamps' state and the time spent head out before it are not known).
- **A star's pixel at most white** (`rm_shade.wgsl` `rm_pixel_star`; the eye's state buffer bound
  to the shade, `rm_display_white`: the radiance the eye maps to display white, 1 / its cone
  exposure): the pixel star carried
  its whole flux in one pixel, orders above the night's white, the deltaP bloom grew a flare round
  it and the flare hopped as the star crossed pixels under the head's sway ("bright night
  flashing star flares"). The display now shows a star as a white pixel, as the eye sees a point.
  Named approximation: the eye's own glare round a bright star is not drawn. The shader
  backend's sky draws stars as radiance over the lattice cell and is unchanged.
- **The modes start at dawn** (`KESSEL_SKY_START`, the launcher's `--sky-start`, default
  `dawn`): the clock started at local apparent noon, and under the launcher's ninety-minute
  cycle the Sun left the full day a quarter of an hour in ("day changes to night way too soon").
  A clock choice, not a lighting change; the ephemeris lights the dawn.
- **The cabin dimmer again, the round lamp recoloured** (the owner: "turn down the dial and cabin
  lights too and rest more on the light from outside which should still be quite dark during the
  day"; "white light color indicator should become yellow or blue"): the practicals' power scale
  0.0018 to 0.0012 and the TPZ illuminator 0.030 to 0.018 of it (real emitter changes); the
  sight's round lamp armour-piercing yellow, high explosive red (APCR stays blue).
- **The tinted look's pull outdoors only** (`crush.wgsl`): a presentation change back to the
  2 November morning state; the cabin stills the owner preferred had been rendered that way.

### Sun shafts through the openings — 2 November 2026 (raymarch backend)

The owner: "we need light shafts coming from the ports inside the tank". Engineering note:
`docs/RAYMARCHING_ENGINE.md`, "Sun shafts through the openings".

- **Removed: the cascades as the compartment's key beam** (with ray queries). They let the beam
  through every vision block at full strength (glass casts no depth), let none down a periscope
  (its windows and mirrors cast), resolved a port's 1.2 cm slot with one 1.2 cm texel under a
  3 x 3 filter, and leaked: with the loader's hatch open under a 35 degree Sun they lit an arc
  along the turret ring and a bar beside the hatch that no line to the Sun reaches
  (`captures/agent-ab9914c8cec360e9c-shafts_20261102x_cascades_sky_shafts_loader_hatch_smoke.jpg`
  against `..._shafts_20261102x_sky_shafts_loader_hatch_smoke.jpg`).
- **Live: the beam traced through the vehicle** (`rm_rt.wgsl` `rm_beam_trace`, `rm_vehicle_beam`;
  `rm_medium.wgsl` `rm_cabin_beam`; `rm_shade.wgsl`, `rm_volume.wgsl`). A compartment surface
  facing the Sun and each cell of the cabin's air trace toward the body through the scene's own
  triangles: opaque stops it; a direct-view pane (rt optic kind 3, `OPTIC_PANE`; every other ray
  still counts it as cover) passes it at the pane material's throughput, the same numbers as the
  apertures' records (`pane_transmittance`: entry 0.96, absorption 3.4/2.0/3.8 per metre along
  the path between the two faces, the glass load's share); a periscope window or thin lens
  passes it at 0.957 a face (uncoated crown glass, n 1.52); a mirror folds it at 0.90 (silvered
  or aluminised glass, 0.85-0.95). Beyond 8 m the cascades at the exit point and the cloud deck's
  and low banks' transmittance. A periscope out of parallel is found by a second trace along the
  met mirrors' inverse fold. The surface's Lambert and GGX terms take the direction the beam
  arrives from; no screen-space contact march for it.
- **Live: the shaft's sampling in the cabin's air.** Four stratified points through each cabin
  cell's depth (`RM_BEAM_STRATA`): a beam thinner than a cell (8 cm deep at 1 m, 15 cm at 2 m) is
  held as the share of the cell it crosses. Sampling, not transport. The froxel slicing
  (0.25..320 m) is unchanged.
- **The cabin medium, sourced** (values unchanged): dust floor 1.3e-3 /m (about 0.4 mg/m^3 of
  fine dust at 3 m^2/g, Hand & Malm 2007, JGR 112 D16203), 4.5e-3 /m a round fired and 1.4e-3 /m
  a breech venting, saturating at 0.042 /m (about 5 mg/m^3 of smoke at 8.7 m^2/g, Mulholland &
  Croarkin 2000, Fire and Materials 24, 227); albedo 0.80, g 0.55. The dust and smoke loads per
  round are estimates of a mass, not measured for the KwK 36.
- **Named approximations**: a point Sun (the disc's penumbra, about 5 mm per metre behind an
  edge, not drawn); refraction in a pane not followed (the path in the glass along the
  unrefracted line, a few per cent long obliquely; one entry face for direct glass, as the pane
  material and the records); periscope faces and mirrors at normal incidence; a fold whose
  straight trace never meets its mirror is not searched for; twelve segments; beyond 8 m the
  cascades. Portable path and A/B: without ray queries, or with `KESSEL_RM_SKIP=8388608`
  (`SKIP_BEAM_TRACE`), the cascades as before.
- **Verified** (headless `sky-review` stills in the locked look, deltap-real 15/15/7 at
  1024x576, `captures/agent-ab9914c8cec360e9c-shafts_20261102x_sky_*`): the loader's hatch open
  under a 35 degree mid-March Sun throws a patch on the far wall and, after four rounds' smoke,
  a shaft from the hatch to it; shut, no beam; the driver's visor toward a 6 degree Sun lights
  his fittings, away from it nothing. Not shown in a picture: a beam through a side port's slot
  (it admits the Sun within about 1.1-1.6 degrees of its axis, i.e. only at sunrise or sunset
  across the hull, and lands on the breech; the port stills found none in the frame), the cupola
  periscopes (their lower windows still measure closed, above), the gunner's seat (the AI
  gunner's body fills that eye in the harness). Cost not measured. Gate: `beam_tests` (a CPU
  replica of the trace over a slot, a glass block and a periscope with parallel and skewed
  mirrors; the shader's constants against it and `pane_transmittance`),
  `every_raymarch_shader_validates_with_and_without_ray_queries`.
- **Open, found on the way**: the cabin air's apertures' sky term (`rm_volume.wgsl`) has no
  occlusion ray, so a periscope's upper window outside the compartment lights the cabin's air
  through its housing; with the hatch shut the smoke reads grey-lit (`..._shafts_loader_hatch_shut`).
  The sky-review interior stills ran under the act's own weather, which can shade the tank with
  a cloud; the shaft stills set clear air.

### The Tiger at the real vehicle's size — 2 November 2026 (`docs/TIGER_SCALE_20261102.md`)

- **The bake was 1.112 times the kit's 1:35** (the damage session's registration: the GLBs are the
  owner's RM-5010 kit CAD at 38.919815 per kit millimetre), so the Tiger stood 11 % larger than
  the real vehicle. `tank::Parts::frame_scale` (35 / 38.919815 for the Tiger, 1 for the enemies)
  is applied once at the end of `Tank::hull_model`, about the ground contact; the turret and gun
  compose on it, so the bake and everything authored in its frames come to the real size, and
  the registration's 0.17 mm RMS is the geometry's own error. `center_h` and `hull_half_extents`
  are now world metres. The printed sheet keeps its paper size in the scaled frame.
- **Gate**: `KESSEL_TIGER_TRUE_SCALE=1` turns the frame scale on (`tank::tiger_true_scale_enabled`,
  `Parts::resolved` in `Tank::new`); off, the bake keeps its 1.112 size as before, in both
  shipping presets, until the second stage lands. Named debt behind the gate: the real-size meshes
  and human offsets aboard (the crew's bodies and eye heights, the rounds in the cradles and in
  hand, the hands' reaches, handholds, personal space) are composed through the scaled frame and
  so stand about 10 % small until each carries its inverse scale or is re-entered in frame units
  (`Parts::local`); the crew harnesses before and after measure it (the same 15 of 20 crew-solo
  checks and all five climb checks pass with the gate on). The camp table's head stays at the
  kit's size while the turret's is at the real size.
- **The cabin dimmer, the deck lower** (2 November 2026, the owner on the tinted stills): the
  cabin practicals' art-calibrated power scale 0.003 to 0.0018 (`interior_rig::CABIN_PRACTICAL_POWER_SCALE`,
  a real emitter change, not a grade), the eye's draw-in knee 1.5 to 2.0 stops and slope 0.32
  to 0.55 (`observer::EYE_RANGE_*`, the owner-directed presentation of local adaptation, now
  lifting the dark corners less), and the winter cloud deck's bases 900 to 450 m and tops 2,200
  to 1,300 m with smaller cells (`CloudLayerParameters`, the act-1 profile): winter stratus and
  stratocumulus bases run 300 to 800 m, and the lower deck foreshortens and moves against the
  ground instead of reading as a map on the sky. The act-1 profile also carries the winter
  aerosol that the atmosphere default carries. The thinner deck let a quarter more of the
  moonless night through (2.5e-4 lx overcast against the literature's 3e-5..2e-4,
  `moonlight_and_the_moonless_night_are_as_dark_as_measured`): its liquid water content is
  0.38 g/m^3 (0.25 before), which keeps the deck's water path at 0.32 kg/m^2 and lies within
  winter stratus's 0.2-0.5 g/m^3.

## Spring 1943 vegetation assets, 2026-11-02 (`tools/vegetation/spring1943.py`, `tools/vegetation/leaf_optics.py`)

Generated GLB trees, shrubs and war-damaged trees (docs/VEGETATION_SPRING1943.md). They use only
the existing mesh path (no renderer change); what they are and are not:

| Part | What the asset carries | Named approximation and source |
| --- | --- | --- |
| Foliage geometry | Single-triangle leaves, 3-5 per twig, about 3 x real leaf length (0.12-0.38 m), on the skin of clumps on the crown shell, each clump fed by a drawn twig, limb and trunk. Opaque triangles: what is drawn is exactly what shadows and occludes (no alpha cards; the raymarch G-buffer alpha-tests only grass). | Leaves are enlarged about 3 x and far fewer than a real crown's (leaf area is bounded by the triangle budget, 4,000-11,000 leaf triangles per tree), so crowns are more open than a measured leaf area index would give; the gaps are a property of the generator, not a measured gap fraction. |
| Leaf reflectance | Base colour = PROSPECT-D leaf reflectance rho at 680/550/440 nm (the meadow's model and lanes, `src/leaf_optics.rs`, ported line for line), spring pigment estimates per species, three chlorophyll tones; scorched and wilted leaves from the same model with brown pigment and low water. Cuticle roughness 0.45. | Leaf transmittance tau is computed but NOT rendered: GLB surfaces shade as opaque one-sided reflectors, so back-lit foliage lacks its transmitted share (for a spring leaf tau about equals rho at 550 nm). Pigment parameters are estimates inside Jacquemoud & Ustin (2019) ranges, not species measurements. |
| Bark, char, wood, soil, petals | Constant albedos from published reflectance ranges (birch periderm 0.5, furrowed bark 0.07-0.13, charcoal 0.034, fresh sapwood splinters 0.46/0.37/0.25, chernozem 0.085, white petals 0.65). | Estimates, not spectra of these specimens; one constant per material, no texture. |
| Destructible trunk | `tree_*` names are live trees in `src/trees.rs`; the trunk contact radius is measured from the drawn mesh at the bow's height as for every tree; the generator keeps foliage and hanging broken limbs clear of the trunk below 1.35-2.2 m so that section is the trunk. | The trees' fall is the existing rigid-body release; no fracture of the generated limbs. Shrubs, fallen trunks and stumps are static dressing with no collision (as the library's logs, ferns and stumps): a tank drives through them visually. |

## Small bright sources and the eye's adaptation, 2026-11-02 (`shaders/exposure.wgsl`, `observer::GLARE_*`)

In the blue hour (spring-bridge-v2, 1943-05-12T18:20Z) any view with the burning town in frame
went nearly black, sky included: lit windows and fires at ~100 cd/m^2 covering 0.3 % of a frame
whose log-average is 0.002 cd/m^2 entered the meter's arithmetic field, Gaussian and fovea
averages at their share and set the adaptation 300 times above the rest of the field (metered
0.59 cd/m^2 on commander_approach against 0.0007 on column_east). The authored lights are
unchanged; no grade.

| Part | What the live code computes | Named approximation and source |
| --- | --- | --- |
| Glare treatment of the meter | Outdoors, a meter cell brighter than `GLARE_RATIO` (100) times the previous frame's log-average luminance enters the fovea, Gaussian and field windows at that log-average (photopic; scotopic shifted by the field's S/P) and adds its disability-glare veil L_v = 10 E / theta^2 to each window: the 252 E deg^2 integral over 0.5..30 deg spread over the field or weighted by the Gaussian at the source, and over the fovea its disc mean 20 ln((R^2 - theta^2) / (0.1 R)) / R^2 (10 / theta^2 outside). E is the cell's luminance above the background times its solid angle. The treatment stands down while such cells cover more than `GLARE_MAX_COVERAGE` (25 %) of the field. The first outdoor snap waits one frame for the background (lane 30). Result: commander_approach metered 0.59 -> 0.067 cd/m^2 (now mostly the veil of the fire it fixates), field_west -> 0.023; column_east and grass_close unchanged (mean pixel difference 0.1-0.2 of 255). | Local retinal adaptation reduced to one adaptation state plus the CIE 146:2002 / Stiles-Holladay veil (no age factor, no wavelength dependence). `GLARE_RATIO` = two decades: Naka-Rushton (n = 1) response at 100 x semi-saturation is 99 % of maximum, semi-saturation tracking adaptation (Valeton & van Norren 1983). The background is one frame late; each 4x4-pixel cell is one point source at its centre; the fovea's veil from a source outside it is taken at its centre; `GLARE_MAX_COVERAGE` is a design bound. CPU reference and test: `observer::glare_cell`, `observer::fovea_veil_per_lux`, `a_small_bright_source_adapts_the_field_through_its_veil_not_its_share_of_the_average`. Identical in both presets. |

## Night lights lose their hue: the rods' local share, 2026-11-02 (`shaders/crush.wgsl` `eye_response`, `observer::local_rod_weight`)

The owner, of spring-bridge-v2 at the blue hour (1943-05-12T18:20Z): "why do these lights at night
all look like this weirdly red white phosphorous color? ... any hue or color or temperature is
completely obliterated". Measured by following the crush's HDR input (`KESSEL_DUMP_HDR=FILE`,
debug only: the crush's input after the bloom and the eye's lanes) through the display stages on
the CPU, commander_approach, warm pixels (red over twice blue) by photopic luminance band; hue is
the OKLab hue angle of the scale-free colour, C its chroma (1850 K source: h 45, C 0.20):

| Band (cd/m^2), pixels | Transport (HDR) | Eye: cones | Eye: cones + rods | AgX | Quantised (15,15,7) | Shown (jpg) |
| --- | --- | --- | --- | --- | --- | --- |
| 0.03-0.1 (halo, far road), 13,038 | h 51 C 0.18 | h 51 | **h 264 C 0.08** | h 267 | h 268 | h 264 |
| 0.1-0.3 (road), 17,648 | h 46 C 0.20 | h 46 | **h 271** | h 281 | h 282 | white-lavender |
| 1-3 (fire-lit ground), 4,884 | h 54 C 0.18 | h 54 | **h 347 C 0.06** (pink) | h 357 | h 351 | h 338 |
| 3-10, 660 | h 51 C 0.19 | h 51 | **h 25 C 0.11** | h 24 C 0.08 | h 20 | h 358 |
| 10-100 (flames, windows), 423 | h 46 C 0.20 | h 46 (peak 7.5) | h 42 | **h 46 C 0.05** | h 57 C 0.03 | h 67 C 0.05 |
| 1000+ (fire cores), 72 | h 45 C 0.20 | h 45 (peak 217-576) | h 45 | **h 19 C 0.03** (pink-white) | h 9 C 0.01 | h 3 C 0.003 |

Transport, the Planck colour of the authored lights (1850 K -> (1, 0.16, 0) linear) and white
balance (off on this map) keep the hue. Two display stages lose it:

1. **The eye's rod term** (0.003-10 cd/m^2: the halo, the road, the ground round the fires). Every
   pixel of the night carried the rods' whole display term, capped at the key, in the night's
   blue hue, (0.10, 0.15, 0.26) once the rods are in their Weber range, under its cones; at 1-3
   cd/m^2 the cones gave (0.22, 0.05, 0.00): blue over orange, pink.
2. **The tone curve** (above ~10 cd/m^2). The metered eye (0.066 cd/m^2) puts a 3,457 cd/m^2
   fire core at 576 in eye units, 11.6 stops over the key and 5.5 past AgX's log ceiling (12.5);
   AgX turns the clipped orange pink-white (h 45 -> 19, C 0.20 -> 0.03), its outset leaves the
   [0, 1] range (peak 1.17) and the quantiser's clamp finishes it (C 0.01). The quantiser itself
   keeps hue wherever its input is in range (3-10 cd/m^2: C 0.077 -> 0.077).

What the code computes now: the displayed rod term is scaled by the rods' local share,
(1 - m_local) / (1 - m_field), m_local the CIE 191:2010 mesopic coefficient of the pixel's own
photopic and scotopic luminance, never below the field's (crush lane `eye.x`, copied from the
eye's lane 14 every metered frame on both backends). The mesopic weight is set by the luminance a
place is adapted to; the retina adapts locally, and a place lit far above the field is adapted
toward its own light (the premise of `GLARE_RATIO`). A pixel no brighter than the field keeps the
whole term (the night field, the moonlit snow, unchanged); a photopic pixel (from about 5 cd/m^2)
takes none.

Measured after, the shown frame, same view and pixels: 1-3 cd/m^2 h 338 C 0.10 -> h 32 C 0.12
(red-orange), 3-10 h 358 C 0.08 -> h 51 C 0.14 (orange), 10-100 h 67 C 0.05 -> h 70 C 0.06; 0.03-1 cd/m^2 unchanged in hue (rod-seen, below), the fire cores
unchanged (AgX, below). Daylight (09:00Z, commander_approach and field_west) and the 16:40Z frame:
pixel-identical. The in-game cabin station views (`raymarch-showcase` dial set: commander,
commander_lamps, loader_annunciators, driver_ignition, tpz_close): pixel-identical.

Named approximations and open items:

- Each pixel's own luminance stands for its local adaptation: complete local adaptation, an upper
  bound on the rods' retreat (the retina's local adaptation is partial and pooled over space).
- Outdoors only: in a closed cabin view (`eye_range_strength`) the rod term is unchanged, so the
  owner's approved cabin lamps keep their look; the cabin keeps its own local-adaptation
  presentation (`eye_range`). Physically the same eye; open. The `lamps` showcase close-ups, which
  place an observer camera in the cabin with `interior_light` 0 (exterior display state), do change:
  the annunciators' glow on the wall is greener.
- 0.03-1 cd/m^2 (the road, the bloom halo round the town) stays lavender. There the rods carry
  the eye's response (CIE 191 m 0.38-0.65 locally; the cones are at their increment threshold,
  0.22 cd/m^2 at this adaptation); the lavender is the owner's two night departures on that rod
  signal, `ROD_DISPLAY_GAIN` 6 (the road shown as bright as snow) and `ROD_DISPLAY_HUE` (deep blue
  over orange cones). With achromatic rods the same pixels are a desaturated warm grey
  (evidence still `lookbook_optneutralrods_commander_approach`, `KESSEL_ROD_HUE=1,1,1`). Owner's
  decision; not changed.
- The fire cores stay pink-white: AgX in the locked look. A hue-keeping curve shows them orange
  (evidence still `lookbook_optaces_commander_approach`, `KESSEL_TONEMAP=aces`), but changes the
  look and every bright colour by day. Owner's decision; not changed.

Gates: `observer::tests::fire_lit_ground_keeps_its_cones_colour_and_the_dark_field_keeps_its_rods`
(CPU statement and the shader's use of it), `renderer::tests` (the crush uniform's 208 bytes, the
eye lane at 192, the three lane copies on both backends), `observer::spatial_meter_tests`
(shader validation). Identical in both presets.

## Effect glow on the one incandescence law, 2026-11-02 (`incandescence::fx_kelvin`, `fx_glow`; the shaders' `incandescent_glow`)

An emissive effect mesh (gunfire, an impact, a burning box) and its light were drawn in a
hand-tinted orange, (1, 0.24, 0.03) mixed 18 % toward the mesh's colour, with an added white-hot term
above a drive of 6, in five places (`rm_gbuffer.wgsl`, `mesh.wgsl`, `gbuf.wgsl`, `stylized_mesh.wgsl`,
`combine.wgsl`) and three CPU copies (the main flash's light, the volumetric flash colour, the
ray-traced emission tint). Now each is the Planck colour (red band 1) of 1,500 K plus 150 K per unit
of drive, to 2,800 K, at 0.30 of the drive: the old orange was that law at about 1,800 K, so a low
drive looks as before and a strong one turns white-hot by the same law the fire, the sparks and the
glowing metal use (test: `an_effect_glows_on_the_planck_law_and_matches_the_old_orange_at_a_low_drive`).
The drive-to-temperature map is an estimate. In the raymarch backend an own-colour glowing instance
batch is now pushed as kind 2 (its authored colour, `set_batch_own_emissive`), not kind 0.

## The vehicle surface model, 2026-11-02 (`src/vehicle_surface.rs`, `shaders/rm_vsm.wgsl`; `docs/VEHICLE_SURFACE_MODEL_20261102.md`)

Every material layer on the player's Tiger is placed by its cause. Phase 1 measures the causes the
geometry carries; phase 2 builds the paint and steel on them; phase 3 simulates the processes that
act on a vehicle in the field. With the field installed, the player's hull, turret, gun, roof
hatches, visor holder and guides, cupola grip and lid, periscope housings, deck plate and fuel caps
take this finish in place of the plain fallback paint (`paint_source_metal`, drawn only until the
field is installed or with it switched off, at the same grey and white) and the shade pass's
screen-space worn edges and corner grime (which stand down for every pixel near the player's
vehicle). The former noise layers (`apply_whitewash`, `apply_wear`, `cabin_grime`) were removed on 2
November 2026. `KESSEL_VSM_LAYERS=N` keeps layers 1..N for review; 255 shows the fallback paint. The
old swatches were aligned the same day so the friendly Tigers, the German wrecks and every part off
the field match: exterior sRGB 105/108/115 (satin), compartment 221/216/206, basket floor a neutral
93/93/91 (it took the source's green-grey 58/70/64). The cupola's lid is its own bake, added to the
turret's field at its hinge, closed. Within a marked part the finish replaces only pixels whose
authored base colour is one the field repaints (the fallback grey and white, the floor paint, track
and cable steel, or the source's metal at metallic 0.5 and above; `vsm_repaints`, rm_gbuffer.wgsl):
a painted instrument face, a dial's white back and a lamp keep their own material. A 1 cm cell's
class cannot resolve a 4 cm dial in the gunner's contact zone; the turret position indicator's white
face read as worn steel until this gate (owner report, 2 November 2026).

| Part | What the live code computes | Named approximation and source |
| --- | --- | --- |
| The field | A sparse grid of 1 cm cells (the kit-CAD frame's units, 1.112 cm of the real vehicle while the true-scale gate is off) on the surfaces of the hull, turret and gun GLBs, in 4x4x4 bricks with a 64-bit occupancy mask: 7.94 M cells, 212 MB on the GPU with the process and snow channels, baked in 168 s on 16 threads and cached by the meshes' hash. Each surface point's record sits 9 mm off it along its visible normal (the two faces of a thin sheet in different cells); the shader blends the eight records round the point by trilinear weight, leaving missing ones out. | One representative surface point per cell (the sample nearest the cell's centre), so a cell straddling two faces carries one face's measures. Turret at zero traverse and gun level for the occluders; the field does not follow a turned turret's shadowing of the hull. Hatch lids, periscopes and other separate meshes are occluders where the GLBs hold them at rest but do not sample the field yet. |
| Openness | At reaches of 0.6, 2 and 6 cm: the share of 24 rays (a Fibonacci sphere, rotated per cell by a hash of its key) cast from half the reach off the surface that run the reach unblocked. Flat 0.75, convex above, creases below (test: `openness_reads_a_face_an_edge_and_a_crease`). | A geometric accessibility measure, the standard proxy for where paint chips and grime packs; 24 rays give a sampling noise of about 0.1 per cell before the trilinear blend. |
| Sky and enclosure | 32 cosine-weighted hemisphere rays from 0.3 mm off the surface against all three meshes and the ground plane: the share free for 25 cm (enclosure) and the share reaching 40 m upward (sky). | The vehicle's own geometry only: terrain, buildings and other vehicles do not occlude. |
| Thickness | The first hit straight into the surface within 25 cm. | The wall behind the face, not the plate's nominal thickness: a bracket on a plate reads the bracket. |
| Sharp convex edges | Mesh edges welded at 0.05 mm (99.7 % of the meshes' edges join exactly two triangles) whose faces meet at more than 12 degrees and convexly; per cell the nearest such edge within 2.5 cm of a face that borders it, stored as a point on its line (0.1 mm units) and an octahedral direction (about 1.4 degrees), with its dihedral angle. The shader measures the pixel's distance to that line exactly. | Corners use the two nearest records of different direction among the eight round the point. Coarsely tessellated curves (a barrel's facets) read as shallow edges, kept with their angle so later layers can weight them. |
| Material class | From the source material name, the reviewed node lists (`TRACK_STEEL`, `CABLE_EXHAUST_STEEL`, `PAINTED`), the running gear's envelope, and the CAD registration's assembly folders where at least 60 % of a node's mass candidates agree (`catalog/tiger_surface_cad_folders.v1.json`, `tools/vsm_cad_folders.py`). | The folders come from SolidWorks mass candidates (several parts can share a mass), so only the running gear's and the barrel's folders are used; road wheel tyres are not separated from their discs yet. |
| Review views | `KESSEL_VSM_VIEW=class|open_fine|open_mid|open_coarse|sky|enclosure|edge|thickness|cells|albedo|roughness|metallic|rounding|flow|rust_carried|dirt_carried|mud|heat|soot|oil|standing_water` draws the channel as unlit false colour on the marked meshes (`KESSEL_VSM_VIEW_GAIN`); fx-review part `materials` (`KESSEL_VSM_REVIEW=field` the channels, default the layer progression lit and as unlit albedo); `tools/vsm_contact_sheet.py`. The progression's interior stills open the roof hatches for daylight (a disclosed harness action). | A debug view, off unless set (test: `the_review_view_is_off_unless_asked`); it writes emission only to show placement and is never a shipping finish. The finish writes albedo, roughness, metallic and the shading normal only (test: `the_vehicle_surface_finish_writes_no_light`). |
| Paint and steel (layers 1-3) | Substrate by class (rolled plate or sheet by measured thickness 12 mm, oxidised manganese track, heat-scaled exhaust, blackened breech steel where the gun's wall is 2 cm or more inside); red-oxide primer RAL 8012 linear (0.156, 0.039, 0.034) from the RAL CIELAB values; the exterior topcoat a medium grey (0.142, 0.151, 0.170; L* 46) matched in lightness to the owner's reference image (its lit faces sRGB ~135 and mid faces ~95 under studio light) and given the slight blue lean of the period's German greys so that it reads neutral under the low winter sun (sunlit snow measures R/G 1.05, B/G 0.95 on the display) — the owner's choice over RAL 7021's blue-black (0.041, 0.045, 0.051; L* 25) (2 November 2026: "too dark and blue and green ... more neutral"; "see this reference for the grey approx"; "exterior still looks green can we make it neutral grey"); outside the paint is satin (roughness 0.50, chalking to 0.70) with a small metallic share 0.15 and the bared steel smoother (0.30, dark steel 0.42), the owner's choice ("a bit shinier") over a flat field paint, a departure from a dielectric paint named here; bare steel neutral; inside, a white at RAL 9010's slightly warm chromaticity (0.72, 0.688, 0.62), the owner's choice over the period's Elfenbein RAL 1001 (2 November 2026: "a neutral white and we'll play with lighting instead"; the cool neutral first tried read blue in the compartment's daylight); the machinery (engine, radiators, fans, transmission, steering: `Class::Mechanical`, by the CAD folders and the engine bay's and gearbox's regions) is bare metal under oil, the owner's choice where the period finish is undocumented (sources give a blue-grey or grey-green engine bay and a satin black Maybach, none all red); spray thickness varying the topcoat by +/-5 % over tens of centimetres. | The CIELAB values are the standards' nominal colours converted to linear sRGB (D65), not measured wartime paint; steel and oxide reflectances (bare 0.48-0.56, blackened 0.14, rust 0.155/0.045/0.012 to 0.06/0.022/0.01) are published-range estimates. The topcoat no longer depends on the environment: the whitewash is not applied to the player's vehicle. |
| Edge rounding (layer 4) | Within the edge record's reach the shading normal turns toward the neighbouring face by up to half the dihedral angle over a fillet of 3.5 mm outside, 2.5 mm inside, plus 3 mm where the crew's contact wears it, for the nearest edge and a corner's second. | A normal approximation of a geometric round: the triangles stay sharp, silhouettes are unchanged. The radii are estimates of paint build-up and wear. |
| Wear (layers 5-6) | Chips: a wear depth through topcoat (primer halo) to steel from the edge's distance (e-folding 7 mm inside, 10-25 mm outside), angle (25-80 degrees), prominence (6 cm openness), height and forward position on the hull, the crew's contact, the mechanical-wear load and a 12 cm field of runs along each edge; shaped by value noise at 9 and 25 mm in the vehicle's frame. Scattered chips on open faces outside. Contact: the crew's hand zones and floors (stations from `cabin_medium`, the commander's eye, the loader's arc from rack to breech), the roof hatches' rims and the deck lanes outside, wearing through on edges and high points and polishing steel. Running gear wears through to steel. | The calibration (bands, rates, noise) is an estimate from service photographs, not a measured wear law; chip shapes are noise inside cause-placed regions; contact zones are fixed points and capsules, not tracked crew motion. |
| Corrosion, deposits, weather (layers 7-9) | Rust on bared steel by wetness (outside: the sky and trapped water; inside: dry but the floor), creeping under the paint's broken edge; grime by openness and enclosure (a months-in-service floor plus the campaign loads), hands' grime; inside and on the machinery a dulling film over every surface and an oil-and-grime coat gathering in corners and enclosed spaces and toward the floor (below 1.6 m in the hull frame); dust on up-facing interior faces away from contact, propellant soot under the turret roof; UV chalking of exterior paint by sky exposure. | Amounts are scaled by the existing whole-vehicle loads (`surface_wear`), not accumulated per cell over the campaign. |
| Snow on the vehicle (phase 4; `vehicle_surface::snow_catch`, rm_vsm_core.wgsl) | Per cell a byte of how much of the lying snow a surface keeps: the sky it sees (0.08-0.5 smoothstep), less the exhaust's heat and the engine deck over the radiators (to 15 %), less the crew's routes outside (the roof hatches' rims, measured from the source GLBs' hatch subtrees, and the deck lane beside the turret; to 15 %). The depth programs and the G-buffer read the same byte at a vertex's nearest cell, so both lift a face identically; the lying share and the relief normal take it too. The whitewash is not applied to the player's vehicle in any environment; the lying snow's load is the vehicle's own (`snow_cover`). `SNOW_KEPT` = 0: the crew keep their vehicle brushed clear and snow slides off the barrel, so no lying snow is drawn on it (the owner, 2 November 2026: keep the Tiger grey; snow on it is to come from the particle system); the catch stays in the field for that work. The running gear's packed snow is unaffected. The replacement deck plate (`tiger_refuel`), the roof hatches, the visor holder and guides, the cupola grip and the periscope housings take the field like the hull. | The snow is the earlier lying-snow relief and noise, now placed by the field; it is not deposited by a simulated fall with wind, does not slide off steep faces by a snow mechanics model, and does not melt by a heat balance (the deck's warmth is a fixed share). The nearest-cell lookup at a vertex steps at 1 cm. |
| Processes (layers 10-14; `vehicle_surface::processes`, 25 s at bake) | Water: three drops from every cell the sky reaches walk the surfaces under gravity in 6 mm steps (re-projected by ray casts, rounding edges whose next face overhangs less than about 53 degrees, dripping otherwise, meandering on flats), depositing flow, picking up dirt where they stand and rust where they cross sharp exposed edges, laying both down as they run. Track throw: clods from every link cell at eight speeds (-4.5 to 10 m/s) with the run's velocity (top forward, wraps turning, bottom still) flown ballistically to their strike. Exhaust: heat by distance from the pipes (e-folding 6 cm); 120,000 buoyant random-walk gas parcels with varied wind leaving soot where they touch. Oil: drops from each sprocket's hub until they reach the track. Barrel heat and propellant soot by position along the gun. Each channel is a byte, saturating over its own 99th percentile. | Geometric particle models, not fluid dynamics: no film thickness, evaporation, freezing or wind field; one gravity direction (the vehicle level); the throw ignores air drag and clod size; the plume is a random walk, not a solved flow; the oil sources are the sprockets' hubs only. The renderer draws the results as streak, spatter and stain patterns shaped by noise inside the simulated amounts. |

## The eye adapts place by place, and a step into daylight is a second, 2026-11-03 (`shaders/eye_local.wgsl`, `crush.wgsl`, `exposure.wgsl`, `observer::LOCAL_*`, `observer::outside_rise_tau`)

The owner: "we need to improve this auto exposure setting its horrible. can't see anything
interesting because its either too dark or screen is entirely white ... the exposure effect is just
so blinding its ruining the whole game". Measured with `eye-review` (headless, live 60 Hz frames,
the locked look; the numbers are in the change's report and `captures/agent-...-eye_*_frames.jsonl`).
No grade, no tint, no change to any emitter: both changes are to the eye's reproduction transform.

| Part | What the live code computes | Named approximation and source |
| --- | --- | --- |
| Local adaptation (`LOCAL_POOLING_SIGMA_DEG`, `LOCAL_SIMULTANEOUS_RANGE_LOG`, `LOCAL_ADAPTATION_STRENGTH`) | After the meter, each meter cell's mean photopic luminance (up to 4 x 4 pixels, 256 x 144 cells) is taken as log10 and pooled by a separable Gaussian of 0.428 deg (`eye_local.wgsl`, CPU twin `pool_log_luminance`). In the crush each pixel's cone term is scaled by Ferwerda's visibility-matching law at a local adaptation luminance log La = log A + k clamp(P - log Lm, -1.85, 1.85) instead of at the eye's A: TVI(A) / TVI(La) (`local_cone_gain`, generated into the shaders as `obs_local_cone_gain`). A eye's cone adaptation (lane 12), Lm the luminance metered this frame (lane 15), P the pooled log luminance, k = 0 since 2026-11-03 (was 0.5; the owner found it flattened the picture, "we look like we are on low graphics mode"), so the shipped eye is the single-state eye and this term is inert unless `KESSEL_EYE_LOCAL` sets a strength for review. A region at the metered level is shown as before; a decade darker is lifted 10^k, a decade brighter held down as much, never reordered. The rods keep their own local response (`rod_response`). Off under a fixed debug exposure. Identical in both presets. | Spatial extent: Vangorp, Myszkowski, Graf & Mantiuk 2015 (ACM TOG 34(6) 166), the wide term of their best model (sigma 0.428 deg, weight 0.654), pooled in the log domain as their model #7 does (their best model pools through fitted sigmoids; the 0.0824 deg term is below a pixel here; a cell is 4 x 4 pixels). Range: 3.7 log units of simultaneous range (Kunkel & Reinhard 2010, APGV; Jiang & Fairchild 2021 measured 3.3): a neighbourhood beyond half of it from the adapted level adapts no further. Strength k: a reproduction choice, the owner's to set. Single cones adapt on their own (MacLeod, Williams & Makous 1992) so at a fixated point in daylight it approaches 1, but the whole picture is shown at once while a crewman's gaze would have moved over it. `KESSEL_EYE_LOCAL=sigma_deg,strength` overrides both for review (strength 0: the single-state eye); no launcher sets it. The cone TVI is flat below 10^-2.6 cd/m^2, so at night the local cone term fades by itself (the site of adaptation moves to broader post-receptoral pools at low light, Dunn, Lankheet & Rieke 2007). Tests: `local_adaptation_tests`. |
| A step into daylight (`OUTSIDE_DAZZLE_FROM_LOG` 0.2, `OUTSIDE_DAZZLE_RAMP_LOG` 0.3, `OUTSIDE_DAZZLE_MESOPIC_FROM` 0.75) | Outside the hull and at the optics the neural adaptation rising toward a target more than 0.2 decades above it relaxes toward the dazzle time constant (0.6 s, full from 0.5 decades), in proportion to how photopic the target is (the CIE 191 mesopic coefficient from 0.75, about 0.9 cd/m^2, to 1, 5 cd/m^2); inside the hull the 2026-10-28 rule stands (from 0.5 decades, full at 1.0). Below the onset, and toward a mesopic or scotopic target, the owner's comfortable 4.5 s stands, so an outdoor glance between sky and snow does not swing the picture and a fire's glow over the night field does not cost the night for seconds after it dies. Before: climbing out of the cupola into the noon snow held 60 % of the frame at white a quarter second out, 47 % at one second, 26 % at three, 11 % at six. | Light adaptation to a photopic step up does most of its work in the first second (Crawford 1947, Proc. R. Soc. B 134:283; Baker 1949, J. Opt. Soc. Am. 39:172), measured with photopic fields; the 0.6 s is the existing dazzle constant. The onset and the mesopic gate are design bounds named here; outdoors no `inside_meter` damping stands in front of the rate. Test: `a_flash_dazzles_briefly_and_a_step_into_daylight_recovers_within_seconds`. |
| A weapon's flash acts through its afterimage only | While the meter registers a flash (a transient source, a field-energy onset and bright cells 30 times the adaptation), the neural rise keeps the comfortable rate: the dazzle rate is not applied on top of the flash-blindness fit (lanes 10-11). | The recovery fit of the 1968 flash-blindness review already is the eye's response to a flash of that energy; under the dazzle rate a night muzzle flash's 83 ms raised the neural state 0.45 decades, which then fell at the 6 s dark rate (counted twice). |
| The snap holds a quarter second (`SNAP_HOLD_S` 0.25, lane 31) | A view's snap to its field (its first frame; an optic's eye returned to after `OPTIC_RESNAP_GAP_S`) repeats for 0.25 s of frames. | Scheduling of the snap only (`Eye::adapted` of each frame). A view's first frames come from temporal histories as old as its last use: the TZF at night first metered 2.8 cd/m^2 of the noon it last saw, snapped and bleached its cones to it (adaptation 2,000 cd/m^2 falling to 25 and held there by the bleach) and stayed black for the minute (eye-review, 2026-11-03). |

The 2026-10-04 draw-in of the closed cabin view (`eye_range`, the "Eye: range drawn in" row above, an
owner-directed presentation of local adaptation) still runs after the local adaptation; with the
cabin's dark lifted and its ports held down by the eye itself, fewer regions reach its 2-stop knee.
Retiring it is the owner's call. Neither changes any light: the meter, the transport and the
emitters are untouched; the strength 0 (`KESSEL_EYE_LOCAL=0.428,0`) reproduces the single-state eye.

Measured (`eye-review`, Canyon v2 seed 29 and spring-bridge-v2, 1024x576, deltap-real 15,15,7; share
of the crushed frame at black, luma code <= 2, and at white, >= 250; "local off" is this change with
`KESSEL_EYE_LOCAL=0.428,0`, the time course alone):

| Scenario | Before | After | Local off |
| --- | --- | --- | --- |
| Head out of the cupola into noon snow: seconds over 10 % white / recovered | 6.3 s / 7.5 s (60, 47, 26, 11 % white at 0.25, 1, 3, 6 s) | 0.5 s / 0.6 s (27 % at 0.25 s, 0 % from 1 s) | 0.8 s / 1.2 s |
| Head back in: peak black / recovered | 58 % / 10 s | 4 % / at once | 62 % / 12.5 s |
| Loader's compartment by day, a hatch open: black / near black (<= 10) | 12.6 % / 41.7 % | 0.0 % / 12.1 % | 14.4 % / 46.8 % |
| Commander buttoned by day: near black / white | 1.2 % / 1.3 % | 0.0 % / 0.06 % | 16.8 % / 0.8 % |
| Driver's visor by day: black / near black | 4.8 % / 39.8 % | 0.2 % / 24.9 % | 25.4 % / 73.9 % |
| TZF by day (the dark cabin round the eyepiece): black | 11.4 % | 1.9 % | 17.4 % |
| TZF at night, a shot fired: recovered | 9.2 s (snapped to its stale noon, held dark by the bleach) | 1.2 s | - |
| Main gun at night, head out: seconds over 50 % black / recovered | 1.45 s / 6.1 s | 1.1 s / 4.4 s | - |
| Noon head out, four bearings: black | 0.0-1.5 % | 0.0-0.1 % | 0.0-1.5 % |
| Blue hour, spring bridge, toward the burning town: black | 56.7 % | 55.7 % | - |

By day the gun and an HE strike do not move the eye (white under 0.2 %). The blue hour is unchanged:
below about 1 cd/m^2 the cone TVI flattens and the rods carry the picture, so the local cone term
is small there by the law, and the dark ground beside the fires stays dark (the fires' veil, 2
November). Cost: `eye-local` 0.012 ms a frame (gfxbench-drive, 1200 frames, KESSEL_GPU_PROFILE=1),
`crush` +0.001 ms, `exposure` unchanged at 0.415 ms.

## The gun's fumes in the crew compartment, 2026-11-03 (`src/cabin_fumes.rs`; `rm_medium.wgsl` `rm_fume_density`, `rm_case_plume`, `rm_gas_density`)

The owner, of a range still with sunlit haze in the turret: a lighter haze should leave the breech as
the spent case is pulled, the case should go on venting for about three seconds wherever it is, many
shots should stack as in a real Tiger, and the fan up top should draw the haze out slowly. Before
this a shot added a flat 0.11 of the cabin haze at the trigger (with the breech shut) and the breech's
opening 0.035, drained linearly at 0.03 a second: one shot was gone in a few seconds and nothing
stacked. Those kicks are removed; `interior_haze` now carries only a hit's residue.

Sources. Foreign Military Studies P-002 "Tank ventilation" (via kamenn.eu): in the Tiger, roof
ventilator drawing, hatches shut, empties left inside, carbon monoxide stood at 0.05-0.1 % after each
shot and fell to practically nothing one to two minutes after firing stopped; lidding each case as it
came out improved it; the powder gas is about half carbon monoxide. The turret ventilator is rated at
12 m³ a minute (the Tiger turret manual, via tiger1.info). It runs while the engine turns, as its hum
does (`game_acoustics`).

| Part | What the live code computes | Named approximation and source |
| --- | --- | --- |
| Two zones | Litres of undiluted fume in an upper layer (the turret air above 2.00 turret-frame units, a little over the breech, `LAYER_Y`) and a lower one, in the 10 m³ of free air (`fea_damage::compartment::TIGER_CREW_AIR_M3`, named there). Fresh fume is hot and rises into the upper layer; the fan's 0.2 m³/s and the hull's leakage draw from it while the same flow comes in low and rises through the interface; 0.05 m³/s exchanges the layers; an open hatch exchanges 0.05 m³/s with the outside on its layer (the cupola, loader's and escape hatches upper, driver's and radio operator's lower; open past half travel). Integrated in 50 ms steps. The upper layer's share of the air is measured from the source compartments (`cabin_medium::Medium::volume_split`). | A zone model: each layer well mixed, a fixed interface 15 cm thick, the fume a passive tracer once in a layer (no cooling of the layer, no deposition on the walls). Estimates: the interface height, the exchange 0.05 m³/s, the leakage half an air change an hour, the hatch exchange (buoyant exchange through a 0.5 m opening with Δρ/ρ 0.15, 0.035 m³/s, rounded up for wind). Test: a shot's fume is of P-002's measured carbon monoxide (0.04 % against 0.05-0.1 %, a factor of two allowed); five shots stack; the fan clears them to 5 % in 45-150 s (P-002: one to two minutes); buttoned with the engine off they stay; an open hatch clears faster; the fume is conserved until the air carries it out. |
| The breech puff | Nothing leaves while the block stays shut after the shot (the owner, 2026-11-03): the bore's gas is released only once the block is open (the loader's lever past its open threshold, the solo crew's opening, the lone survivor's fumble), the case's venting only as it is drawn out (test `fume_leaves_only_an_opened_breech`). When the block opens on a fired case, 5 L of the bore's gas leave the breech; the drawn puff (`rm_gas_density`, the shape's axes unchanged) carries the fume not yet spread, at its centre the fume over the shape's volume-weighted fill (`PUFF_PROFILE_MOMENT`, its profile integrated), at most undiluted; the fume passes to the upper layer with a 0.7 s e-folding as the puff spreads. The fan's sink flow (0.2 m³/s through a hemisphere about the intake under the roof, `sink_velocity`) carries the puff toward the intake by its age. The former fixed envelope (0.30 cap, a fade from 1.2 to 2.2 s) is removed. | Estimates: 5 L (the 8.8 cm L/56 bore, 30 L, and a 4 L chamber of gas at ambient pressure near 1,000 K is about 10 L at 293 K; half leaves rearward), the 0.7 s spreading time. The puff's shape and growth stay authored; the sink is a point sink in a flat open roof. The puffs' shared saturation at 0.6 /m (`rm_puff_extinction`) still holds it, so its first second is drawn lighter than its fume (up to 17 /m). |
| The case venting | Each case drawn out of the breech (the loader's pull and the crew's ejection) carries 3 L of fume and vents it from its mouth with a 1 s e-folding, ended at 3.5 s, wherever its body is: in the hands, the rack, on the floor; inside the hull's box its fume joins the upper layer. It is drawn as a pure buoyant plume from the mouth (Morton, Taylor and Turner: half-width 1.2 alpha (z + z0) with alpha 0.1, centreline concentration (z0 / (z + z0))^(5/3) of the mouth's, Gaussian across), its strength at the mouth the undiluted fume's extinction times the source flux to the two-thirds, leaning with the fan's sink flow against a 0.3 m/s rise, ending where it meets the upper layer. | Estimates: 3 L (the case's 5.5 L of hot gas, about 1.6 L at 293 K, and its residue's first seconds), the 1 s e-folding (the owner's three seconds; a buoyant exchange through the 81 mm mouth is of that order), the rise speed. Similarity profiles from the mouth, a straight leaning axis; a case thrown out of the hull vents into the open, undrawn. The reload fixture (`debug_reload`) never opens the block and releases no fume. |
| Visibility | The layers and plumes add to the cabin haze's extinction inside the cabin domain (its scattering: albedo 0.8, asymmetry 0.55); the undiluted fume's extinction is 17 /m. | Estimate; no measured smoke yield of the propellant was found. The chain: 0.5 % of the Pzgr 39's 2.5 kg charge (JH.Dv. 481/60, via Wikipedia) taken as smoke-forming (the flash reducer's potassium salts and residue; flash-reducer patents note those salts make the smoke), 12.5 g a shot; the compartment's share is its 8 L of the charge's ~2,200 L of gas at 293 K, 0.36 %, about 45 mg; a fine salt smoke's mass extinction taken as 3 m²/g. A case mouth's 8 cm of fume is nearly opaque; one shot spread through the compartment is 0.014 /m, under the roof right after it about ten times that. |

Not yet: the hit residue (`interior_haze`) and a fire's soot (`interior_soot`) still drain linearly
at the old fan and hatch rates, not through this zone model; machine-gun fumes are not modelled.
No render has checked the look yet (the owner's editor holds the GPU); `cargo run -- haze` and
`fx-review` stills will.

## Point footprints: one centred square, never a star, 2026-11-03 (`crush.wgsl` `defocus_disc`, `src/point_footprint.rs`)

The owner, with a crop of a hot fragment drawn as a bright pixel with dimmer pixels above, below,
left and right: "never use a star particle shape". The particle passes (rm_points, rm_sparks,
weapon_particles, pixel_particles, stylized_particles) already draw each point as one pixel of the
internal grid (test `every_point_particle_is_one_pixel_square`); a hot chip is its own cube at true
size through the G-buffer. The cross came after them: a close hit's concussion defocus in the
crush was five nearest-sampled taps, the centre (0.40) and the four axial pixels (0.15 each) at 1
to 5.5 pixels, so every bright pixel threw dimmer copies along the axes and none on the diagonals:
a solid plus at a weak concussion, a four-armed star at a strong one. It was on for seconds after
every penetration of the player's own tank, when the chips are in view.

| Part | What the live code computes | Named approximation and source |
| --- | --- | --- |
| Concussion defocus | A blur circle: every internal pixel within 2 to 5.5 pixels (2 + 3.5 times the concussion), weighted by its share inside the circle (its centre's distance from the rim, a pixel wide), normalised; the frame mixes toward it by the concussion as before. From 2 pixels up the diagonal neighbours are as fully covered as the axial ones (test `the_concussed_eye_spreads_a_point_into_a_disc_not_a_cross`, which also fails the old five taps). | The defocus itself stays an authored effect of a concussion (its strength and timing from `concussion_visual_strength`); the disc is geometric optics' uniform blur circle without diffraction or aberration; the radius starts at 2 pixels (was 1) because a smaller disc on the pixel grid covers the axial neighbours more than the diagonal. Up to 109 nearest fetches a pixel, only while a concussion lasts. |
| Chip cubes | A chip (`Renderer::set_batch_point_square`, the hot-fragment head pages) projecting 3 pixels or more is its unit cube at true size; under 3 pixels it is a filled square of whole pixels, ceil(its size) on a side, about the pixel (or pixel corner) nearest its centre, at its centre's depth, its glow scaled by its projected area over the square's so its light is unchanged (rm_gbuffer.wgsl `point_square`; tests `a_small_chip_fills_its_whole_square_and_keeps_its_light`, `the_gbuffer_draws_the_mirrored_square_for_the_chips`). A 2-3 pixel cube at true size covered the four axial pixel centres and not the diagonals: a plus (the owner: fill the full square). | The square is the pixel-scale stand-in for the cube's silhouette: its area is the cube's edge squared (one face on), its glow spread evenly (not each pixel's own coverage); a cooled chip's albedo is not scaled (no coverage blending in the G-buffer). Shadows and ray-traced geometry keep the true cube. The tails stay true-size boxes. |

Open: the shade pass's screen gather truncates its sample offsets toward zero (`rm_shade.wgsl`, the
gather's `vec2<i32>(step * radius_px * t)`), so a surface pixel beside a bright resting chip gathers
it a little more along the axes than the diagonals (weak: blended at 0.2 with a 2x history cap).

## Hot bodies for the heat shimmer, 2026-11-03 (`src/hot_surfaces.rs`, `Game::hot_surfaces`)

The owner wants the air to shimmer round hot things; the refraction is fea-damage's. This side
reports what it owns, once a frame, from the models that already carry each temperature: a
penetration's rim (`rim_temperature`: its worked steel cooling by conduction, the plate's normal,
the ring's area), a hot fragment (its solved temperature, its cube's area), the cabin fire's burning
patches and the burning wrecks (at a flame temperature, their area times the fire model's
intensity), and a spent case (its wall cooling from the breech, on a clock started when it is drawn
out, kept until within 5 K of the surroundings).

| Part | What the live code computes | Named approximation and source |
| --- | --- | --- |
| Fires | A cabin patch: its surface point and normal, 0.12 m radius, area pi r² times its intensity; a wreck: its deck anchor or fire entry, 0.72 m (the drawn crown), area times its strength; both at 1,150 K. | The flame temperature is McCaffrey's continuous-flame region of a buoyant diffusion flame (about 1,100-1,200 K), not solved; the area scaling by intensity is named; no heat release rate is reported. The FEA breaches' fires (`fea_fire_sources`) are fea-damage's own. |
| Spent cases | The wall at 423 K as it leaves the breech, cooling toward 293 K with a 360 s time constant: rho c t / (h_conv + h_rad), steel 7,850 kg/m³ and 490 J/kg K, a 1.5 mm wall, 8 W/m²K of natural convection and 8 of radiation (emissivity 0.8 near 360 K); 0.22 m² of wall; reported for about 18 minutes. | Estimates: the initial 423 K (a fired large-calibre case is far too hot to hold; no 8.8 cm measurement found), the wall, the film coefficients; one lumped temperature for the whole case. |

## Explosion and fire light through the armour, 2026-11-03 (`rm_visibility.wgsl` `rm_light_housing_m`, `renderer_raymarch.rs` `draw_scene`)

The owner: "some explosions are shining light straight through armor on tiger". Two exemptions meant
for a lamp's own fixture applied to every light:

1. A light's shadow cube left out every mesh whose bounding box held the light (its "housing").
   The Tiger's hull box holds any blast, cook-off or burning vent against its plates, so for such
   a light the whole hull cast no shadow. Now a vehicle's armour (the player's surface-model and
   painted parts, other vehicles' finished meshes) is never a housing to a light outside the crew
   compartment's air (`cabin_medium::Medium::contains_world`); a light in the compartment (the
   cabin lamps, an interior fire or blast) keeps the exemption, its light held inside by the cabin
   gate of the shade and volume passes.
2. A light without a cube took one ray-query shadow ray that stopped 4 source radii short
   (0.15-0.3 m) of the source, past its fixture. A blast's light 5 cm to 1.2 m off a plate
   stopped before the plate. Now only a lamp (a scene light under 5 cm across) stops short of its
   fixture; a weapon burst's light (blast, cook-off, vent, rim), a tracer's head and the game's
   explosion, flash and fire lights (6-35 cm sources) stop at their own radius, in both the
   surfaces' and the medium's shadow rays (test `no_light_but_a_lamp_skips_the_steel_before_it`).

Named remaining gaps: a light is classed as a lamp by its kind and size (the light carries no
fixture flag); a lamp's shadow ray still skips 0.15-0.3 m before it, so armour within that of a
lamp outside the compartment is skipped; without ray queries (the portable preset) a light without
a cube is shadowed only by the screen's contact shadow, at surfaces and in the medium (as before).
