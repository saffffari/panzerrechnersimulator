# Tiger component damage and field repair — 28 September 2026

The owner asked for "the component damage system and repair for both interior and exterior
components of the tank", and for singleplayer to have one crewman, who therefore does every
repair himself. Before this change only a *penetration* could damage anything: its fragments
knocked modules out (working or not), bounces only dented the armour, and nothing on the outside of
the Tiger could break.

## What history says

Full sourced brief: `docs/research/TIGER_FIELD_DAMAGE_REPAIR_BRIEF_20260928.md` (Tigerfibel
D 656/27 read page by page; Lt. Zabel's report; NIIBT Kubinka 1943; the 1944 Soviet field trial;
Jentz & Doyle via Tiger1.info). Anti-tank weapon effects: the AT research brief in this session
(to be filed with the anti-tank guns).

The anchor case is Lt. Zabel's Tiger (3./s.Pz.Abt. 503, Semernikovo near Rostov, 10–11 Feb 1943),
also quoted on Tigerfibel p. 4. In 6 hours it took 227 anti-tank-rifle hits, 14 of 4.5/5.7 cm and
11 of 7.62 cm; none went through. Yet:

- a 4.5 cm hit on the cupola smashed a block's brackets; the block jammed and went opaque;
- both MGs were shot to pieces;
- a 7.62 cm hit on the mantlet sheared the gun-mount bolts, the recoil brake lost its oil and the
  gun stayed in full recoil;
- a 7.62 cm hit on the driver's front plate tore off the spare links clamped there;
- shock put the radio out; the turret ring was jammed for a while;
- road wheels were holed, two torsion bars broke, the idler worked out of its mount;
- welds cracked and a fuel tank leaked; the engine caught fire when the exhaust cover went;
- and the tank still drove 60 km on its own.

The Tigerfibel gives the crew's field fixes: top up the recoil brake (5.1 l; "wenn die Ölsicherung
ausgeschaltet hat, darf nicht geschossen werden", p. 56); the traverse fault table ("Kupplung
abklopfen", p. 63); boresighting a sight that "wanders out of true under the shocks" (p. 79); MG
barrel changes (pp. 58–61); re-seating radio plugs and fuses (p. 48); freeing a jammed driver's
visor ("im Winter und bei Beschuß klemmt sie manchmal", p. 40); fitting track links (p. 35). Spare
vision blocks were carried: 4 in two boxes in the turret. Spare links had no factory stowage
before April 1943; crews clamped them to the bow plate. Broken torsion bars, sheared mounts and
wrecked ring bearings were workshop jobs.

## What the game does now

`src/tiger_damage.rs` (pure, deterministic from the projectile seed) and its wiring in `game.rs`
and `equipment.rs`.

**Every strike on the Tiger, penetrating or not, damages what it lands on.** The zone comes from
the true contact point. The runtime armour is two coarse boxes and the hull box reaches above the
roofline, so a strike above the roof (hull-local y > 2.0 m) is treated as a turret strike, zoned by
height and bearing (a declared approximation until the exact-mesh armour seam exists).

| Zone | 14.5 mm rifle | 37/45 mm | 57/76 mm | 85 mm+ |
|---|---|---|---|---|
| Track band / wheels (low side) | tyres only; never breaks a track | breaks the track 70% (band) / 25%; wheels −8% | 85% / 40%; −14% | 95% / 60%; −25% |
| Sprocket / idler | — | track 75% / 50%; wheels | same | same |
| Bow plate | — | tears off 3 spare links 60% | tears off all 90% | all |
| Driver's visor | jams 50% | jams 85% | 85% | 85% |
| Ball mount | barrel bent | MG wrecked 60% | same | same |
| Mantlet | coax barrel 25% | recoil leak 15%, sight out of true 30%, coax 50% | mount sheared 25%, leak 40%, sight 50%, coax 55% | sheared 60%, sight smashed 40% |
| Turret ring band | jams 15% | jams 50% | jams 60%, wrecked 10% | wrecked 40%, jams 50% |
| Cupola | block opaque 80% | opaque, mounting smashed 50% | smashed 80%, neighbour cracks | same |
| Anywhere (shock) | — | radio 5% | radio 12%, sight (turret hits 25%) | radio 25%, sight 40% |

Probabilities are tuning anchored to Zabel and the trials, not measured rates. Penetrating hits
also keep the existing fragment model inside the hull.

**Faults, not just broken/working.** Each module carries a specific `Fault`:

| Fault | Effect | Field repair by the lone crewman |
|---|---|---|
| Recoil brake leaking | firing circuit cut; gun still elevates | top up at the cradle, loader's side (20 s) |
| Gun mount sheared | no fire, no elevation | workshop |
| Traverse hydraulics | no powered traverse | knock the clutch free at the traverse gear (3 s) |
| Fragment in the ring | no traverse at all | knock it out from the hull deck (12 s); unreachable from inside |
| Ring wrecked | no traverse | workshop |
| Sight out of true | the sight looks 2–5 mils off the bore: the reticle is on target, the shell is not | boresight at the sight (25 s) |
| Sight smashed | no reticle | workshop |
| MG barrel bent / MG wrecked | MG silent | barrel change (6 s) / workshop |
| Radio knocked out | set dead | reseat plugs and fuses (2 s) |

**Exterior state** (`Exterior`): road-wheel condition per side (top speed down to half, a pull
toward the worse side, never immobile on its own); seven cupola blocks (clear / opaque /
mounting smashed); 4 spare blocks; 6 spare links on the bow plate; the driver's visor jam; where a
ring fragment is wedged; the sight's error.

**What the crewman sees and does.**
- An opaque block passes no exterior view: the renderer skips its port and the entrance face
  keeps its opaque source material (shader backend). A spare goes in from inside at the block,
  8 s, one fewer spare. A smashed mounting cannot be swapped.
- A broken track now lies loose off its wheels whenever it is broken, not only while the toolbox
  is carried. Fitting a link uses one of the bow-plate spares; with none left the track cannot be
  rejoined in the field.
- A jammed visor will not open or close; the driver frees it at the visor control (hold, 10 s)
  with his own key.
- Repairs keep the existing embodied rule: carry the toolbox, stand where the part is in hand's
  reach with nothing in between, hold. Armour between the crewman and the part rules a job out,
  so the ring is knocked free from outside. The prompts name the job, in the game's existing
  prompt style.
- Turret-mounted parts (gun, sight, coax) now take fragments where they actually are; their
  damage spheres used to stay fixed in hull space.

## Evidence

- `tiger_damage::tests` (7): zoning on the real Tiger, rifles never break track while 45 mm
  usually does, a 76 mm mantlet hit shears the mount or leaks the brake (never both), determinism,
  the repairable/workshop split, running gear never immobilises.
- `equipment::tests::exterior_strikes_break_what_they_hit_and_the_lone_crewman_puts_the_field_faults_right`
  (GPU, ignored; passes locally): 7.62 cm on the mantlet leaks the brake and knocks the sight out
  of true (2–5 mils), both put right from inside; 4.5 cm cracks cupola block 2, a spare goes in;
  a ring jam cannot be reached from inside and is knocked out from the deck; the left track
  breaks and the wheels suffer; a bow-plate hit tears off all six links.
- Film: `catalog/shader_reviews/tiger_exterior_damage_cupola.v1.json`
  (`panzerrechnersystem mechanic-review <prefix> <scenario>`), output in
  `shader-graphics/captures/astra/tiger_damage_cupola_20260928*`.

## Limits and follow-ups

- Faults and exterior state are singleplayer: LAN clients still see only the working/broken mask,
  and the campaign carries modules between acts as working/broken only; exterior state resets per
  act.
- The ray-traced backend still shows a view through an opaque block; the winter map uses the
  shader backend.
- Not yet modelled: the hand traverse wheel (powered traverse needs the engine; TF p. 62), engine
  limp mode and cooling damage, exhaust-cover fire, fuel leaks, jammed hatches, smoke dischargers
  set off inside, spare links salvaged from wrecks, running-gear sound and ride roughness.
- Hits from anti-tank rifles and 45 mm guns arrive with the anti-tank weapons; today the only
  shells striking the Tiger are the T-34's 76 mm.
