# Total-war pass — 2026-09-15

Branch `claude/total-war-overhaul`, worked in isolation from
`codex/hal-high-renderer-acceptance-20260903`. 590 Rust tests and 19 Python asset-cook tests pass
at every commit.

The brief was that the game does not look or feel like the war it is set in: the map reads as
wilderness rather than a warzone, only a fraction of the collected assets are in use, and the
Tiger handles like a sprite rather than fifty-seven tonnes.

---

## 1. The map was not a warzone, and the numbers said so

Before this pass, across roughly 19 km² of playable ground in three acts:

| | Before | After |
|---|---|---|
| Bodies | 43 | 6,860 |
| Vehicle wrecks | 36 | 198 |
| Sourced tableaux | 0 placed | 185 |
| Distinct models used | 66 of 1,194 | 96 |
| Blood/drag stains | 0 | 2,368 |

Act III was 22,768 pine trees out of 37,685 placements — it was literally a forest. The generator
already had a good `battle_site` routine; it was called **twice** for Stalingrad. The system was
sound and the dosage was the error.

### What was authored

The commercial PSX libraries are generic urban/nature kits with no vehicles, artillery, obstacles,
draught animals or grave markers — exactly the objects that make an Eastern-Front battlefield
legible. `tools/worldgen/gen_war_props.py` generates 24 low-poly props (48–608 tris): panje wagons,
burned and HE-hit lorries, a wrecked field gun, dead and butchered draught horses, Czech hedgehogs,
wire pickets, sandbags, stretchers, supply canisters, grave crosses and mounds, stove chimneys, and
the three stained-snow surfaces.

`tools/worldgen/bake_corpse_poses.py` poses six frozen-dead postures from the rigged infantry source
and bakes them to 608-tri static meshes, so a corpse field reads as many men rather than one mesh
repeated.

### The warscape layer

Thirteen tableau classes in `gen_world.py`, each an attested recurring condition of the campaign
rather than a reconstruction of one identified incident, each recording provenance into the
manifest's authoring-only `warscape` block per the CLAUDE.md first principle.

The one worth calling out is `assault_field`: its dead are biased toward the objective rather than
scattered uniformly, so the density curve alone tells the player which way the attack was going and
exactly where it broke — no text, no marker.

Classes that touch atrocity — the open POW enclosure, the rear-facing Order No. 227 blocking
position — are built from material evidence and absence, never from staged victims. That is a
deliberate choice and also simply what the surviving photographs and the memorial sites look like.

---

## 2. A dead crewman now blocks his seat

A Tiger's fighting compartment has no spare volume. When the man at a post is killed his body is
physically in front of the controls, and the station is **occupied**, not "disabled": it is inert
until he is dragged clear on held R, and then he lies on the compartment floor for the rest of the
run because there is nowhere else to put him.

Clearing costs follow the internal geometry rather than balance taste — loader 4.5 s (the only half
of the turret with floor to work in), gunner 6.0 s (wedged between cradle and turret wall), both
hull posts worst at 7.4 / 8.2 s. Every one is longer than the longest seat change, so a casualty
always costs more than a reshuffle.

`cargo run -- crewpull` verifies it: **0.00000 rad** of turret movement under full traverse while a
body is in the gunner's seat, against **0.09806 rad** once it is clear.

---

## 3. The Tiger weighs fifty-seven tonnes now

Two things made it a sprite. It accelerated at a flat 6 m/s² and reached road speed in two seconds
(the real vehicle needed 25–35), and `hull_model` was translation × yaw, so the hull never pitched,
rolled or settled — the camera, gun, sights and crew were rigidly level at all times.

Replaced with a power-against-resistance balance, so the ponderousness falls out of the vehicle's
own figures rather than a tuned curve: 57,250 kg, ~331 kW at the sprockets, grip-limited at 0.62,
rolling resistance and track sinkage set by ground type. The hull is sprung on its torsion bars
against the plane its tracks stand on, sampled at the four corners of the contact patch, as a damped
second-order system per axis — it settles, lags and overshoots, and carries weight transfer.

Measured by `cargo run -- ride`:

| Surface | Sustained | Historical |
|---|---|---|
| Road | 43 km/h, 13.7 s to 10 m/s | ~45 km/h optimal |
| Snow | 25 km/h | cross-country 20–25 |
| Deep snow | 14 km/h | — |
| Mud | 16 km/h | — |

Nose +3.4° under power, −2.6° under braking, 5.9° of live hull attitude across a fifteen-second
drive. The cross-country figure landing inside the historical band from an independent power
balance is the part worth trusting.

---

## 4. The front is under fire

Shellfire was one distant boom every 6–20 s with no flash, no impact and no danger — scenery. A
barrage is now a battery firing on a map reference that walks its fall of shot along an axis, as
artillery does. Every round is a real impact with blast, debris and speed-of-sound delivery, so at
range the flash arrives well before the bang. Roughly one mission in seven is ranged on the player.

Rounds inside 7.5 m break what lives outside the armour — tracks, radio, traverse, gunner's sight —
through the existing module state rather than a bespoke health subtraction. Everything inside 48 m
rings the hull, so a crew feels a battery walking toward them before it can reach them.

`cargo run -- barrage`: burst at 2.9 m, turret traverse knocked out, modules 12 → 11.

---

## 5. A real colour-space bug, and one open finding

**Fixed.** glTF defines `baseColorFactor` as linear; the loader baked it into a flat tint uploaded
as `Rgba8UnormSrgb`, which the GPU sRGB-**decodes** on read. Storing the linear value raw applied
the transfer twice and darkened **every** factor-only material in the game by roughly an order of
magnitude — the Tiger's authored 0.092 armour reached the shader as 0.0074. `linear_to_srgb_u8`
fixes the round trip, with tests pinning it.

**Open, and recorded in `docs/PHYSICAL_TRUTH_AUDIT.md` with measurements.** That did not explain the
Tiger. On the act-I chase camera the hull sits at 0.036 mean luminance with **92.9% of its pixels at
exactly zero**, against snow at 0.689 in the same frame. A dielectric at its albedo under that
irradiance should land near 0.075, and exactly-zero across a lit surface is an absent term, not a
dark material. Ruled out: albedo, metallic, a missing sky term in general (the PSX brick wall metres
away in the same frame lights correctly and legibly), and overcast — `KESSEL_OVERCAST` was added as
a debug-only override to open the cloud deck, and a full direct solar beam moves the hull only from
92.9% to 90.3% at zero.

The leading suspect is per-view self-visibility masking of the player's own vehicle leaking from
camera visibility into shading. The first-person design means that surface is normally never seen,
which is exactly how such a bug would hide. It matters beyond the chase camera: the same masking
would affect what the Tiger contributes to reflections and to the light leaving it onto the snow.

---

## Captures

In `captures/`: `warprops_sheet_20260915` (authored prop sheet), `ws_dead_cluster` (corpse field),
`crewpull_{blocked,dragging,cleared}`, `ride_{rest,launch,slope,brake}` (hull attitude),
`barrage_{incoming,impact,after}`, `crushchk_before` (the wall/Tiger lighting comparison that
isolates the open finding).

## Not done

Named rather than half-built: repair mechanics beyond what already existed, FPS arms and body
animation, the handheld 3D map, building interiors, and fly swarms around decaying corpses. On the
last one — flies belong in Acts II and III, not the Stalingrad winter, where they would be dormant;
that is a nuance worth keeping rather than a detail to sprinkle everywhere.

Act-I livery is also worth a pass: the Tiger carries a single dark exterior material, while a
January-1943 Stalingrad vehicle would be Dunkelgrau under winter whitewash. That is a livery
decision rather than a bug, so it was left rather than guessed at.
