# Sound prompts: what to record next (9 November 2026)

A working list of prompts for the sound-effect generator: sounds the game lacks, sounds worth
replacing, and palettes to build. Written after reading what is installed: 211 named effect files
under `assets/sounds`, 286 library clips, and the audio code.

## How the game uses a sound, and what that means for a prompt

- The game places every source itself. It propagates it to the ear, puts it through the hull and
  the cabin's traced room, and sets its level from a calibrated table. So the default request is
  a **dry source**: close, mono, one event, no room, no echo, no music, no voices.
- The engine, the running gear and the shell whistle are synthesised today. Recorded loops can
  replace or underlie them, so those rows ask for steady loops with no events in them.
- Where a space is the character of the sound (a horizon of artillery, a shout heard through
  armour) a baked "wet" take is worth having as well. Those rows are marked **wet**. For any dry
  row, add one of the wet endings below to get a comparison take.

Prompt recipe: *what it is, what it is made of, what happens to it* + *how close the microphone
is* + *the envelope (sharp attack, long ring, sudden stop)* + *what must not be in it*.

Dry ending, used unless a row says otherwise:
`close microphone, dry, no reverb, no echo, no music, no voices, single clean event`

Wet endings to try:
- interior: `heard from inside a closed steel tank, boxy resonant steel cabin, muffled highs, low hull resonance`
- through armour: `heard through thick steel plate from inside, dull and muffled, only low and mid frequencies survive`
- distant open ground: `heard from one kilometre across open steppe, delayed dull thump, soft rolling echo, air has eaten the highs`
- canyon: `heard in a rocky ravine, hard slap echo arriving twice, long decaying tail`
- winter: `heard across snow-covered ground, dead and close, no echo, soft edges`

Columns: **Sec** is the length to ask for. **Takes** is how many different variants to keep, so a
sound does not repeat. **Loop** rows should be generated with the looping switch on. Rows marked
**(replace)** improve a file that exists; the rest are new.

## 1. Tiger interior: machinery and the working cabin

| ID | Sound | Prompt | Sec | Takes | Notes |
|---|---|---|---|---|---|
| IN-01 | Ventilator | Small electric extractor fan in a steel housing running steadily, thin whirr with a faint bearing tick, slight flutter of air through a grille. | 10 | 2 | loop; roof fan after firing |
| IN-02 | Hydraulic traverse pump | Hydraulic pump driven off an engine shaft, constant oily whine with a low gear growl underneath, pressure steady. | 10 | 2 | loop; pitch follows engine speed |
| IN-03 | Turret traversing under power | Fifty-tonne steel turret turning on a ball race, deep rolling rumble of steel balls in a greased track, slow, heavy, with a faint hydraulic hiss. | 8 | 3 | loop; (replace) the one turret clip |
| IN-04 | Turret stops | Heavy rotating steel mass brought to a stop, rumble dies in half a second, one soft clunk of backlash in the gear. | 2 | 3 | |
| IN-05 | Traverse handwheel | Hand turning a small geared steel handwheel, fine regular clicks of a ratchet, light gear whirr, greased. | 4 | 3 | cycle: one turn |
| IN-06 | Elevation handwheel | Hand turning a larger worm-gear handwheel under load, slower heavier clicks, a little squeak of dry bronze. | 4 | 3 | cycle: one turn |
| IN-07 | Gun cradle moving | Long heavy gun barrel tilting on its trunnions, low metallic groan and a soft creak of the balance spring. | 3 | 3 | elevation changes |
| IN-08 | Travel lock | Steel clamp thrown off a gun breech, sharp clack of a latch then a heavy bar swinging to rest with a thud. | 2 | 2 | |
| IN-09 | Round drawn from rack | Large brass cartridge sliding out of a steel clip rack, long smooth scrape of brass on painted steel, clip springs back with a ping. | 2 | 4 | |
| IN-10 | Round in the arms | A fifteen-kilogram brass and steel shell handled, hollow dull knock of the case against a steel wall, cloth rustle. | 2 | 3 | |
| IN-11 | Round rammed home | Heavy shell pushed fast into a gun chamber, sliding brass rush ending in a solid seated thud. | 1 | 4 | (replace) breech_ram |
| IN-12 | Breech block rises | Vertical steel breech block snapping shut under spring force, one hard dense clack with a short ring. | 1 | 4 | (replace) breech_close |
| IN-13 | Gun fires, inside | Large cannon fired, heard from beside its breech: a huge blunt pressure thump felt more than heard, no crack, then the breech slamming back and forward on its slides. | 3 | 4 | wet; (replace) main_gun_fire_int |
| IN-14 | Recoil and run-out | Heavy gun sliding back on oiled rails and returning, hydraulic buffer hiss, a deep metallic knock at each end. | 2 | 3 | layer under IN-13 |
| IN-15 | Case ejected | Hot empty brass case thrown out of a breech, loud bell-like clang against a steel guard, then a hollow tumble. | 2 | 5 | (replace) breech_extract |
| IN-16 | Case into the bin | Empty brass case dropping into a canvas and sheet-metal bin onto other cases, dull clank and brassy rattle settling. | 2 | 4 | |
| IN-17 | Cases rolling on the floor | Several empty brass shell cases rolling and knocking on a steel floor as it tilts, slow hollow ringing rolls. | 6 | 3 | when moving with a full bin |
| IN-18 | Fumes after the shot | Hot gas venting from an open gun breech, a short soft roaring hiss fading to nothing. | 2 | 3 | |
| IN-19 | Coaxial MG belt | Cloth-and-steel ammunition belt pulled from a tin and fed into a machine gun, links ticking over steel. | 3 | 3 | |
| IN-20 | MG cocked | Machine gun cocking handle pulled back and released, long spring stretch and a solid forward clack. | 1 | 3 | |
| IN-21 | MG barrel change | Hot machine gun barrel unlatched and slid out of its jacket, latch click, steel sliding, faint sizzle. | 3 | 2 | |
| IN-22 | Hatch closed from inside | Thick steel hatch lid pulled down onto its seat, heavy dead clang, then a locking lever thrown with a clack. | 2 | 3 | (replace) tank_hatch_closing |
| IN-23 | Hatch pushed open | Heavy steel lid lifted on a sprung hinge, low creak, the lid thudding against its stop. | 2 | 3 | (replace) tank_hatch_opening |
| IN-24 | Cupola crank | Small crank handle wound to raise a steel lid on a screw post, fast fine ratcheting, lid scraping its rim. | 4 | 2 | |
| IN-25 | Vision block swapped | Thick glass block pulled out of a steel frame and another pushed in, rubber squeak, glass knocking steel, clamp snapped. | 3 | 2 | |
| IN-26 | Seat | Sprung steel tractor seat taking a man's weight, short leather creak and spring squeak. | 1 | 4 | |
| IN-27 | Boots on the cabin floor | Hobnailed leather boots shifting on a ribbed steel floor plate, short scrapes and knocks. | 3 | 4 | (replace) footstep_metal for interior |
| IN-28 | Loose kit rattling | Mess tins, a helmet and tools rattling together in a steel box on a moving vehicle, irregular tinny clatter. | 8 | 3 | loop; scales with speed |
| IN-29 | Hull groan in a turn | Massive steel hull under twisting load, slow deep groan and one sharp tick of a stressed weld. | 3 | 4 | hard turns, rough ground |
| IN-30 | Torsion bar creak | Thick steel bar twisting under load and relaxing, low rubbery creak, almost a moan. | 2 | 4 | per station over bumps |
| IN-31 | Intercom keyed | Throat microphone circuit switched in, soft click and a thin electric hum with faint crackle. | 1 | 3 | |
| IN-32 | Radio receiver idle | Valve radio receiver with nothing on the channel, steady soft hiss, a slow low hum, an occasional crackle. | 10 | 2 | loop |
| IN-33 | Radio tuning | Valve receiver dial swept across a band, hiss swelling, whistles sliding in pitch, a moment of morse and it is gone. | 5 | 3 | |
| IN-34 | Radio valve warm-up | Valve set switched on, relay click, hum rising over two seconds to a steady hiss. | 3 | 1 | |
| IN-35 | Lamp switch | Bakelite rotary switch turned one step, firm dry click. | 1 | 3 | (replace) switch_button |
| IN-36 | Turret position dial | Small mechanical indicator driven by a flexible shaft, faint continuous ticking whirr. | 4 | 1 | loop while traversing |
| IN-37 | Water bottle | Felt-covered steel flask unscrewed and drunk from, cap squeak, two swallows, slosh. | 3 | 2 | |
| IN-38 | Cabin at rest | Inside a parked steel tank with the engine off, near silence, slow metallic ticks of cooling steel, a faint drip. | 12 | 2 | loop; wet |

## 2. Driver's station and the power train

| ID | Sound | Prompt | Sec | Takes | Notes |
|---|---|---|---|---|---|
| DR-01 | Main switch | Heavy battery isolator lever thrown, solid clunk and a brief electric buzz. | 1 | 2 | |
| DR-02 | Fuel pump priming | Small electric fuel pump ticking fast then slowing as pressure builds. | 4 | 2 | |
| DR-03 | Electric starter | Large electric starter motor cranking a big cold petrol engine, laboured rising whine, heavy slow compression thuds. | 4 | 3 | (replace) engine_start |
| DR-04 | Inertia starter wound by hand | Hand-cranked flywheel starter spun up, a gear whine climbing slowly in pitch over many turns from a growl to a high sing. | 12 | 2 | driven by crank speed; see TL-30 |
| DR-05 | Inertia starter engaged | Spinning flywheel clutched onto an engine, pitch dropping fast with a grinding bite, engine turning over heavily. | 3 | 3 | |
| DR-06 | Engine catches | Big V12 petrol engine firing unevenly on a few cylinders, coughs, then all cylinders join into a smooth heavy idle. | 5 | 3 | |
| DR-07 | Engine refuses | Big petrol engine cranking, one cough, a wheeze, and it dies with a shudder. | 4 | 3 | cold or flat battery |
| DR-08 | Backfire | Single loud bang from a hot exhaust, short flat crack with a metallic rasp in the pipe. | 1 | 3 | |
| DR-09 | Engine through the firewall, idle | Large V12 petrol engine idling behind a steel bulkhead, deep even throb, fan roar, valve gear ticking faintly. | 10 | 2 | loop; wet; under the synth |
| DR-10 | Engine through the firewall, pulling | Large V12 petrol engine working hard behind a steel bulkhead, thick roar, intake moan, the bulkhead buzzing. | 10 | 2 | loop; wet |
| DR-11 | Gear preselected | Small lever moved in a gate, light click, then a hiss of hydraulic oil and one firm clunk as the gear takes. | 2 | 4 | each shift |
| DR-12 | Gearbox whine | Heavy gearbox under load, steady many-toothed whine with a slow beat in it. | 10 | 3 | loop; one per gear range |
| DR-13 | Gearbox refuses | Gear selector forced, harsh grinding of gear teeth that will not mesh, a jolt. | 2 | 3 | |
| DR-14 | Steering wheel | Large steering wheel turned on a hydraulic unit, soft oil hiss, a mechanical click at each step. | 2 | 3 | |
| DR-15 | Emergency steering lever | Long steel lever hauled back against a band brake, ratchet clicks and a squeal of brake lining on a drum. | 2 | 3 | |
| DR-16 | Brake | Band brake clamping a large steel drum, rising squeal and a shudder. | 2 | 3 | |
| DR-17 | Pedal | Steel pedal pressed to the floor and released, linkage rattle and a spring return clack. | 1 | 3 | |
| DR-18 | Driver's visor | Armoured visor shutter cranked shut, small gear clicks and a steel plate seating. | 2 | 2 | |
| DR-19 | Engine shutdown | Big petrol engine switched off, firing stops, flywheel spins down through a rattle to silence, fuel gurgles. | 4 | 2 | (replace) engine_shutdown |
| DR-20 | Engine cooling | Hot engine and exhaust ticking and pinging as they cool, irregular, slowing. | 12 | 2 | loop after shutdown |

## 3. Tiger outside: engine, exhaust and tracks

| ID | Sound | Prompt | Sec | Takes | Notes |
|---|---|---|---|---|---|
| TG-01 | Idle | Maybach V12 petrol tank engine idling, heard two metres from the exhausts: deep smooth burble, soft exhaust flutter, cooling fans humming. | 10 | 2 | loop |
| TG-02 | Low revs | The same engine at low steady revs, burble tightening into a thick even drone. | 10 | 2 | loop |
| TG-03 | Mid revs under load | The same engine pulling hard at mid revs, loud full-throated roar with a hollow intake note. | 10 | 2 | loop |
| TG-04 | High revs | The same engine near its limit, hard strained howl, fans screaming. | 8 | 2 | loop |
| TG-05 | Overrun | The same engine with the throttle shut while rolling, popping and gargling in the exhaust. | 6 | 2 | loop |
| TG-06 | Exhaust flame | Brief jet of flame from a hot exhaust stack, soft whump and flutter. | 1 | 3 | night, high revs |
| TG-07 | Tracks on a hard road | Wide steel tank tracks on packed earth, slow: each link slapping down with a flat clank, steady rhythm, pins squeaking. | 10 | 2 | loop; under the synth |
| TG-08 | Tracks on gravel | Wide steel tracks crushing gravel, constant grinding crunch with link slaps buried in it. | 10 | 2 | loop |
| TG-09 | Tracks in mud | Wide steel tracks churning deep mud, wet sucking and slapping, clods falling from the links. | 10 | 2 | loop |
| TG-10 | Tracks on snow | Wide steel tracks on dry packed snow, dull squeaking crunch, links muffled. | 10 | 2 | loop |
| TG-11 | Track squeal in a turn | Steel track dragged sideways on hard ground, long rising screech of steel on stone. | 3 | 3 | |
| TG-12 | Sprocket and idler | Large toothed steel wheel driving a track, regular meshing clack, a ringing whirr. | 8 | 2 | loop |
| TG-13 | Road wheels | Many rubber-tyred steel wheels rolling on a steel track, low drumming rumble. | 8 | 2 | loop |
| TG-14 | Suspension bottoming | Heavy vehicle dropping onto its bump stops, deep double thud and a rattle of fittings. | 1 | 4 | |
| TG-15 | Crushing a fence | Timber fence and posts flattened under steel tracks, splintering cracks and a dragging scrape. | 3 | 3 | |
| TG-16 | Crushing masonry | Brick wall pushed over and ground under tracks, heavy tumbling rubble and gritty crunch. | 4 | 3 | |
| TG-17 | Over a wreck or gun | Steel tracks climbing over a field gun, shrieking bending metal, a wheel collapsing. | 4 | 2 | |

## 4. T-34 and other vehicles

| ID | Sound | Prompt | Sec | Takes | Notes |
|---|---|---|---|---|---|
| TS-01 | T-34 idle | Big V12 diesel tank engine idling, harsh uneven clatter, loud injector knock, exhaust chuffing. | 10 | 2 | loop; must not sound like the Tiger |
| TS-02 | T-34 at speed | The same diesel at high revs, raw rattling roar, heavy black-smoke bark from the exhausts. | 10 | 2 | loop |
| TS-03 | T-34 pulling away | The same diesel revved hard from idle, a clash of gears, then pulling. | 4 | 3 | |
| TS-04 | T-34 gear change | Unsynchronised tank gearbox shifted with force, loud grinding crunch and a hammer blow on the lever. | 2 | 3 | |
| TS-05 | T-34 tracks | Wide cast steel tracks with loose pins on hard ground at speed, loud ringing clatter, a metallic slap at every link, much brighter than rubber-tyred wheels. | 10 | 2 | loop; steel-rimmed wheels |
| TS-06 | T-34 track squeal | Dry steel track pins squealing rhythmically as the track runs. | 6 | 2 | loop |
| TS-07 | T-34 turret | Electric turret traverse motor, fast rising whirr with a gear rattle. | 3 | 3 | |
| TS-08 | T-34 gun, close | 76 mm tank gun fired thirty metres away, sharp cracking bang with a hard metallic edge and a dust slap. | 2 | 4 | (replace) t34_gun |
| TS-09 | T-34 hatch | One large heavy steel hatch thrown open against its stop, big hollow clang. | 2 | 3 | |
| TS-10 | T-34 hit and stopped | Tank engine cut off mid-roar by an impact, tracks clattering to a halt, steam hissing. | 3 | 3 | |
| TS-11 | T-34 passing | Diesel tank passing ten metres away at speed, clatter and roar rising and falling. | 8 | 3 | wet; reference only |
| TS-12 | Truck | Small petrol lorry engine labouring on a dirt road, gearbox whine, body rattling. | 10 | 2 | loop |
| TS-13 | Field gun unlimbered | Wheeled field gun dropped off a tow hook, trail spades clanging, wheels on gravel. | 4 | 2 | |

## 5. Hits on the tank, by what struck it

Each of these is wanted twice: the dry outside source, and a wet take "heard from inside the
struck vehicle" for comparison with what the game's own hull and cabin model makes of the dry one.

| ID | Sound | Prompt | Sec | Takes | Notes |
|---|---|---|---|---|---|
| HT-01 | Rifle bullet on thick plate | Rifle bullet striking thick armour plate and disintegrating, tiny hard tick with a short high ping. | 1 | 6 | (replace) bullet_on_steel |
| HT-02 | Machine-gun burst on plate | Burst of bullets raking thick steel plate, fast uneven hail of ticks and pings, some whining off. | 2 | 5 | |
| HT-03 | Submachine-gun burst on plate | Pistol-calibre bullets spattering on steel at close range, dull rapid patter, softer than rifle fire. | 2 | 4 | PPSh |
| HT-04 | Anti-tank rifle | Heavy 14.5 mm armour-piercing bullet hitting thick plate, sharp whip-crack and a deep short thunk, plate rings briefly. | 1 | 5 | |
| HT-05 | Light AP, held | 45 mm solid shot stopped by thick plate, hard bright clang like a sledgehammer on an anvil, half a second of ring. | 2 | 5 | (replace) tank_hit_light_calibre |
| HT-06 | Medium AP, held | 76 mm solid shot stopped by 100 mm plate, enormous dense bang-clang, the whole hull ringing like a struck bell, long low shudder. | 3 | 5 | (replace) tank_hit_nonpenetration |
| HT-07 | Glancing shot | Solid shot skidding off sloped armour, harsh ripping scrape and a howling ricochet whine flying away. | 3 | 5 | |
| HT-08 | Shot lodged | Solid shot burying itself in thick plate without passing through, a deep dead thunk, almost no ring. | 1 | 4 | |
| HT-09 | Penetration | Armour-piercing shot punching through steel plate, a sharp crack of the plate failing, a flat bang, a spray of fragments rattling inside a steel box. | 2 | 5 | (replace) tank_hit_penetration |
| HT-10 | Spall shower | Flakes of steel thrown off the inside of an armour plate, a burst of small bright pings and rattles on steel. | 1 | 5 | inside, after HT-06 |
| HT-11 | HE on the armour | 76 mm high-explosive shell bursting on steel plate, flat ear-splitting slap, then earth and splinters raining on metal. | 3 | 4 | |
| HT-12 | Heavy HE | 122 mm high-explosive shell bursting against a tank, huge slamming boom that drags the air, long patter of falling debris. | 4 | 3 | |
| HT-13 | Hit on the track | Shot striking a steel track and road wheel, dull clanking crack, torn rubber, a link ringing as it flies. | 2 | 4 | |
| HT-14 | Hit on sheet metal | Shot tearing through a thin sheet-steel stowage box, brief ripping punch and tinny flutter. | 1 | 4 | turret bin, mudguards |
| HT-15 | Hit on the gun barrel | Shot striking a long hollow steel gun barrel, a pure high bell note that sings for a second. | 2 | 3 | |
| HT-16 | Hit on the mantlet | Shot striking a very thick cast steel block, the dullest heaviest clank, no ring at all. | 1 | 3 | |
| HT-17 | Vision block cracked | Thick laminated glass block struck hard, sharp crack and a crazing crackle spreading through it. | 1 | 4 | |
| HT-18 | Grenade on the deck | Hand grenade exploding on a steel deck, flat hard crack, fragments zinging off plate. | 2 | 4 | |
| HT-19 | Bundled charge | Bundle of grenades exploding against a hull, heavy double-edged blast, a deep buckling groan of steel afterwards. | 3 | 3 | |
| HT-20 | Mine under the track | Anti-tank mine going off under a track, deep muffled ground thump, a metallic crack, earth falling for two seconds. | 3 | 3 | |
| HT-21 | Petrol bomb | Glass bottle smashing on steel and its fuel igniting, wet shatter then a soft rushing whoomph. | 2 | 5 | |
| HT-22 | Near miss, shell | Large shell passing a metre away at supersonic speed, a violent tearing crack with no tail. | 1 | 5 | (replace) near_miss |
| HT-23 | Near miss, ground burst | Shell bursting in soft ground beside a vehicle, heavy crump, clods and stones drumming on steel plate. | 3 | 4 | |
| HT-24 | Aircraft cannon run | 23 mm cannon shells walking across a steel deck, a fast row of sharp bangs coming nearer and passing over. | 2 | 3 | |
| HT-25 | Ears after a hit | High thin ringing in the ears with all other sound dulled, slowly fading. | 8 | 2 | wet; with the tinnitus model |
| HT-26 | Hull after a hit | Massive steel structure humming and settling after a blow, low fading drone with two or three ticks. | 4 | 3 | tail under HT-06 |

## 6. Distant explosions and the far battle

All **wet**: the distance is the sound. The game's propagation will still place them.

| ID | Sound | Prompt | Sec | Takes | Notes |
|---|---|---|---|---|---|
| DS-01 | Tank gun, 1 km | Tank gun fired a kilometre away across open ground, a dull flat thump, then a softer slap of echo off a far tree line. | 3 | 5 | (replace) main_gun_fire_distant |
| DS-02 | Tank gun and strike, 2 km | Very distant gun report followed two seconds later by the faint crack of its shell striking steel. | 5 | 4 | |
| DS-03 | Battery firing | Four field guns firing one after another several kilometres away, soft heavy thuds felt in the chest. | 5 | 3 | |
| DS-04 | Shells arriving far off | Distant shells landing in a row, each a low crump, rolling together. | 5 | 3 | |
| DS-05 | Shell inbound | Artillery shell coming in overhead, rising tearing whistle that cuts off just before impact. | 3 | 4 | pair with HT-23 |
| DS-06 | Rocket salvo | Salvo of rockets launched far away, a rising moaning howl of many voices, then a rippling carpet of bursts. | 10 | 2 | |
| DS-07 | Heavy MG far off | Slow heavy water-cooled machine gun a kilometre away, measured hollow knocking. | 4 | 4 | Maxim |
| DS-08 | Fast MG far off | Fast-firing machine gun a kilometre away, short ripping bursts like tearing canvas. | 3 | 4 | MG 34 |
| DS-09 | Rifle fire far off | Scattered rifle shots across a wide front far away, single flat pops at uneven intervals. | 8 | 3 | |
| DS-10 | Tank burning far off | Ammunition cooking off in a burning vehicle far away, irregular pops and cracks, then one deep boom. | 8 | 3 | |
| DS-11 | Magazine explosion | Tank's ammunition exploding all at once a kilometre away, a deep double boom and a long rolling rumble. | 6 | 3 | |
| DS-12 | Horizon rumble | Continuous artillery duel beyond the horizon, a low uneven grumble like distant thunder that never stops. | 15 | 2 | loop |
| DS-13 | Engines beyond the ridge | Several tanks moving out of sight a kilometre away, a faint mixed drone and thin clatter carried on the wind. | 12 | 2 | loop |
| DS-14 | Canyon echo of a gun | Cannon shot in a rocky ravine, the report slapping back off the walls three times, each duller. | 5 | 3 | |
| DS-15 | Aircraft high up | Formation of piston-engined aircraft passing high overhead, slow beating drone. | 12 | 2 | loop |
| DS-16 | Dive | Single-engined aircraft diving, engine note rising to a scream, pulling out. | 6 | 2 | |
| DS-17 | Flare | Signal flare fired and bursting high, soft pop, a faint hiss as it burns and falls. | 5 | 2 | |
| DS-18 | Silence after a barrage | Open country just after shelling stops, a few stones still falling, one bird, wind. | 10 | 2 | |

## 7. Tool palette: each tool on each surface

The aim is a matrix the repair sequences can draw from. Keep every take short and clean.

| ID | Sound | Prompt | Sec | Takes | Notes |
|---|---|---|---|---|---|
| TL-01 | Sledge on armour plate | Heavy sledgehammer striking very thick steel plate, flat dense clank, almost no ring. | 1 | 5 | (replace) hammer_strike |
| TL-02 | Sledge on a pin drift | Sledgehammer striking a steel drift held on a track pin, bright ringing crack, the pin shifting with a scrape. | 1 | 6 | track pins |
| TL-03 | Sledge on a track link | Sledgehammer striking a cast steel track link lying on the ground, dull heavy clonk. | 1 | 4 | |
| TL-04 | Hammer on thin sheet | Hammer beating a dented sheet-steel mudguard, loud tinny booming bangs. | 1 | 5 | |
| TL-05 | Hammer on a hub | Hammer striking a wheel hub to free it, solid knock with a short hollow ring. | 1 | 4 | |
| TL-06 | Hammer on wood | Hammer driving a wooden bung into a hole in a steel tank, woody thock, the steel booming faintly. | 1 | 4 | fuel tank bung |
| TL-07 | Miss | Hammer glancing off steel and hitting the ground, a skidding clink and a thud. | 1 | 3 | |
| TL-08 | Spanner seated | Open-ended spanner pushed onto a large hex nut, small metallic click. | 1 | 4 | |
| TL-09 | Nut breaks free | Seized steel nut breaking loose under a spanner, sharp crack and a squeal of rusty thread. | 1 | 5 | first stroke |
| TL-10 | Nut turned | Large nut turned a sixth of a turn with a spanner, short gritty creak of thread. | 1 | 6 | cycle |
| TL-11 | Nut spun off by hand | Loose nut spun down a thread with the fingers, fast light whirring rattle. | 2 | 4 | |
| TL-12 | Nut dropped on steel | Large steel nut dropped onto a steel deck, bright bounce, spin and settle. | 2 | 4 | |
| TL-13 | Nut dropped in dirt | Large steel nut dropped onto dry earth, a single soft thud. | 1 | 3 | |
| TL-14 | Bolt tightened hard | Spanner hauled on a tight bolt, a straining creak ending in a dead stop. | 1 | 4 | |
| TL-15 | Wing nut | Wing nut spun on a stud by hand, light quick rattle and a snug squeak. | 2 | 4 | fan grille clamps |
| TL-16 | Screwdriver in a slot | Large screwdriver turning a slotted screw in steel, grinding twist and a slip. | 1 | 4 | |
| TL-17 | Crowbar set | Steel bar jammed into a gap between steel plates, hard scrape and a bite. | 1 | 4 | |
| TL-18 | Crowbar levering | Steel bar levering a stuck hatch, long groaning creak of bending metal, then a bang as it gives. | 3 | 4 | |
| TL-19 | Crowbar slips | Steel bar slipping off its purchase, ringing clang and a clatter on plate. | 1 | 3 | |
| TL-20 | Clamp strap opened | Spring steel toggle clamp released, snap and a small rattle of its wire loop. | 1 | 4 | tool clamps |
| TL-21 | Clamp strap shut | Spring steel toggle clamp closed over a wooden handle, firm snap. | 1 | 4 | |
| TL-22 | Tool lifted from its clamps | Long wooden-handled tool lifted off a steel deck, handle knocking steel, head scraping. | 1 | 4 | |
| TL-23 | Track pin sliding | Long steel pin sliding out through track link eyes, a hollow ringing scrape for a second. | 2 | 4 | |
| TL-24 | Track link dropped | Cast steel track link, thirty kilograms, dropped onto hard earth, single heavy clank and thud. | 1 | 4 | |
| TL-25 | Track link seated | Steel track link pushed into place between two others, knuckles knocking and aligning. | 2 | 3 | |
| TL-26 | Track run dragged | Length of steel track dragged over the ground by a cable, grinding rattle of many links. | 5 | 2 | |
| TL-27 | Jack ratchet | Heavy rack jack wound under load, slow loud ratchet clacks, the rack creaking. | 4 | 3 | cycle: one turn |
| TL-28 | Jack takes the weight | Jack head meeting a steel arm and lifting, a groan, a shift, a tick of settling metal. | 2 | 3 | |
| TL-29 | Jack let down | Rack jack released step by step, clacks and a final heavy settle of the vehicle. | 3 | 2 | |
| TL-30 | Starter crank turned | Long steel crank turned in a socket, heavy geared resistance, a low growl rising with each turn. | 3 | 3 | cycle; with DR-04 |
| TL-31 | Starter port cover | Domed steel cover unbolted and lifted off a plate, bolt squeak, a hollow knock as it comes away. | 2 | 2 | |
| TL-32 | Road wheel drawn off | Heavy rubber-tyred steel wheel slid off an axle stub, greasy scrape, then it drops and wobbles to rest. | 3 | 3 | |
| TL-33 | Wheel nut set | Six large nuts run up one after another, six quick whirrs and six squeaks. | 6 | 2 | |
| TL-34 | Tow cable dragged on steel | Thick steel wire rope dragged across a steel deck, harsh rasping slither. | 3 | 3 | |
| TL-35 | Tow cable dragged on ground | Thick steel wire rope dragged over stony ground, hissing scrape. | 3 | 3 | |
| TL-36 | Cable eye on a shackle | Steel cable eye dropped over a shackle pin, heavy clink, the pin pushed through and clipped. | 2 | 3 | |
| TL-37 | Cable taking strain | Steel wire rope going taut under a great load, rising creak and a deep twang. | 3 | 3 | |
| TL-38 | Cable parts | Steel wire rope snapping under load, a rifle-shot crack and a whipping whistle. | 1 | 3 | |
| TL-39 | Track adjuster | Large threaded adjuster turned with a T-handled wrench, slow heavy clicks, the track creaking tighter. | 3 | 3 | cycle |
| TL-40 | Shovel in earth | Spade driven into dry earth and a load thrown, crunch, heave, scatter. | 2 | 4 | also gravel, snow, frozen ground |
| TL-41 | Axe in wood | Axe chopping a green log, solid wet chock, a chip flying. | 1 | 4 | |
| TL-42 | Wire cutters | Heavy cutters snipping barbed wire, sharp snip and the wire springing away with a twang. | 1 | 4 | |
| TL-43 | Hacksaw on steel | Hacksaw cutting a steel rod, harsh rhythmic rasp. | 3 | 2 | loop |
| TL-44 | Grease gun | Lever grease gun pumped into a nipple, squelch and a click. | 1 | 3 | |
| TL-45 | Fuel can poured | Twenty-litre steel can emptied through a funnel, deep glugging, the can booming as it empties. | 8 | 2 | |
| TL-46 | Water into a radiator | Water poured into a hot radiator, hiss and bubbling gurgle. | 5 | 2 | |
| TL-47 | Filler cap | Large threaded steel cap unscrewed and lifted, gritty turns and a hollow knock. | 2 | 3 | deck caps |
| TL-48 | Aerial rod | Steel aerial rod unscrewed from its base and withdrawn, thread squeak, a springy wobble. | 2 | 2 | |
| TL-49 | Extinguisher | Hand-pumped brass fire extinguisher squirting liquid, pump strokes, a sharp jet hissing on hot metal. | 4 | 3 | |
| TL-50 | Rag on glass | Dry cloth rubbed over thick glass, short squeaks. | 1 | 3 | wiping an optic |

## 8. Things breaking

| ID | Sound | Prompt | Sec | Takes | Notes |
|---|---|---|---|---|---|
| BR-01 | Track breaks | Steel track pin shearing under load, sharp ping, then the track run flailing and slapping off its wheels in a long clatter. | 3 | 4 | |
| BR-02 | Track thrown in a turn | Steel track riding off its sprocket, screech, a row of bangs, the vehicle lurching to a stop. | 3 | 3 | |
| BR-03 | Torsion bar snaps | Thick spring-steel bar breaking, a deep gunshot bang with a twanging aftershock. | 1 | 3 | |
| BR-04 | Road wheel breaks up | Rubber tyre tearing off a steel wheel at speed, flapping thuds and a grinding of bare metal. | 3 | 3 | |
| BR-05 | Sprocket teeth shear | Steel gear teeth stripping, three or four cracking bangs and a spinning whine with nothing to bite. | 2 | 3 | |
| BR-06 | Final drive fails | Heavy gear train destroying itself, grinding crunch, chunks of metal thrown around a housing, a seized stop. | 3 | 3 | |
| BR-07 | Gearbox fails | Gearbox losing oil pressure and jamming, whine dropping to a shudder and a clunk. | 3 | 3 | |
| BR-08 | Engine seizes | Big engine running without oil, rising metallic shriek, a knock that gets louder, a dead locked stop. | 5 | 2 | (replace) engine_death |
| BR-09 | Engine misfiring | Big petrol engine running on ten of twelve cylinders, uneven lumpy beat and popping. | 8 | 2 | loop |
| BR-10 | Radiator holed | Pressurised hot coolant bursting from a hole, loud hiss of steam and spattering water. | 5 | 2 | |
| BR-11 | Fuel line cut | Petrol spraying from a cut pipe onto metal, thin hissing spatter and drip. | 5 | 2 | |
| BR-12 | Hydraulic line bursts | High-pressure oil line bursting, a sharp spit and a spraying hiss dying away. | 2 | 3 | traverse, steering |
| BR-13 | Turret ring jammed | Steel fragment wedged in a rotating ring, grinding screech and a dead stop, a motor straining. | 2 | 3 | |
| BR-14 | Optic smashed | Telescope lenses shattering inside a steel tube, a crack and a fine tinkle of glass falling down the tube. | 2 | 3 | |
| BR-15 | Hinge shears | Thick steel hinge pin breaking, hard crack and a heavy lid falling crooked. | 2 | 2 | |
| BR-16 | Fittings torn off | Tools and boxes torn from a steel hull by a blast, a rattle of metal objects flung and landing. | 3 | 3 | |
| BR-17 | Mudguard in the track | Bent sheet steel dragged into a moving track, prolonged shrieking scrape. | 3 | 2 | |
| BR-18 | Aerial shot off | Thin steel rod cut through and whipping away, a ping and a thin whirr. | 1 | 2 | |
| BR-19 | Electrical short | Heavy cable shorting against steel, loud crackling buzz and a pop. | 2 | 3 | (replace) electric_spark |
| BR-20 | Bulb and valve | Small glass bulb bursting, tiny dry pop and tinkle. | 1 | 3 | lamps, radio valves |
| BR-21 | Weld cracks | Thick weld seam cracking under shock, one sharp dry tick and a short creak. | 1 | 4 | sprung plates |
| BR-22 | Breech jams | Breech block stopping half way, a clack with no follow-through, a scrape of brass. | 1 | 3 | |
| BR-23 | Recoil cylinder leaks | Oil forced past a seal under high pressure, short wet hiss at each movement. | 1 | 3 | |
| BR-24 | Propellant burns | Cartridge propellant igniting in a confined steel space, a fierce jetting roar building in a second. | 4 | 3 | cook-off, inside |
| BR-25 | Rounds cooking off | Ammunition bursting one by one in a fire, sharp bangs at uneven intervals with a roar under them. | 8 | 3 | |

## 9. Fire and heat

| ID | Sound | Prompt | Sec | Takes | Notes |
|---|---|---|---|---|---|
| FR-01 | Fuel fire on the deck | Petrol burning on a steel deck, soft steady roar with fluttering gusts. | 10 | 2 | loop |
| FR-02 | Deck fire heard inside | Fire burning on the outside of a steel roof heard from below, muffled low roar, the plate ticking. | 10 | 2 | loop; wet |
| FR-03 | Engine bay fire | Oil and rubber burning in an enclosed metal compartment, thick crackling roar, pops of bursting hose. | 10 | 2 | loop |
| FR-04 | Paint blistering | Paint bubbling and crackling on hot steel, fine dry sizzle. | 6 | 2 | loop |
| FR-05 | Steel heating | Thick steel plate expanding as it heats, slow irregular pings and creaks. | 10 | 2 | loop |
| FR-06 | Rubber burning | Rubber tyre burning, bubbling hiss and heavy lazy flames. | 8 | 2 | loop |
| FR-07 | Fixed extinguisher | Pressurised extinguisher discharging into an engine bay, loud rushing hiss for three seconds, then drips. | 5 | 2 | |
| FR-08 | Fire dies | Small fire smothered, roar collapsing to a hiss and steam. | 3 | 2 | |
| FR-09 | Burning cloth | Clothing on fire, soft fast fluttering flame. | 4 | 3 | bailing crews |
| FR-10 | Beating out flames | Hands slapping at burning cloth, quick frantic pats. | 3 | 3 | |

## 10. Soviet crews leaving a burning tank

Wordless voice sounds only here; spoken Russian lines belong to the voice work, not this list.

| ID | Sound | Prompt | Sec | Takes | Notes |
|---|---|---|---|---|---|
| SV-01 | Hatch flung open | Heavy tank hatch thrown open from inside in panic, clang, then smoke and fire roaring out of the opening. | 2 | 3 | |
| SV-02 | Climbing out | Man hauling himself out of a steel hatch fast, boots kicking steel, cloth dragging, hands slapping armour. | 3 | 4 | |
| SV-03 | Dropping to the ground | Man jumping two metres from a vehicle onto earth, heavy landing, a stumble, a grunt. | 2 | 4 | |
| SV-04 | Rolling to put out fire | Man rolling over and over on dry ground, thudding and scuffing, flames fluttering and dying. | 4 | 3 | also on snow |
| SV-05 | Crawling away | Man dragging himself over rough ground on his elbows, laboured breath, scraping cloth. | 5 | 3 | |
| SV-06 | Running away | One man sprinting away over earth in heavy boots, footfalls fading. | 5 | 3 | wet |
| SV-07 | Cry of pain | Man crying out in pain as he burns, hoarse, wordless, breaking off. | 2 | 5 | |
| SV-08 | Shout to others | Man shouting a wordless warning to others at the top of his voice, urgent, hoarse. | 2 | 5 | |
| SV-09 | Coughing in smoke | Man choking and coughing in thick smoke, retching. | 4 | 4 | |
| SV-10 | Trapped | Fists hammering on steel from inside, muffled shouting. | 4 | 3 | wet |
| SV-11 | Hull fire with crew gone | Tank burning hard with its hatches open, roar from the openings like a furnace, small bursts inside. | 12 | 2 | loop |

## 11. Infantry around the tank and on it

For the planned swarm: many men close in, climb on, and try the hatches. Each is wanted dry, and
the "on the tank" rows also as a wet take heard from inside.

| ID | Sound | Prompt | Sec | Takes | Notes |
|---|---|---|---|---|---|
| NF-01 | Men running, earth | A dozen men in boots running over dry earth, loose equipment rattling, breath. | 6 | 3 | loop |
| NF-02 | Men running, snow | A dozen men running through shin-deep snow, squeaking crunch and heavy breathing. | 6 | 3 | loop |
| NF-03 | Men going to ground | Several men throwing themselves flat, bodies and rifles hitting earth. | 2 | 4 | |
| NF-04 | Massed shout | Large body of men shouting together as they charge, a long rising wordless roar from far off coming closer. | 8 | 3 | wet |
| NF-05 | Shouts through armour | Men shouting to each other just outside thick steel, heard from inside, words lost, only muffled barks. | 5 | 4 | wet |
| NF-06 | Rifle, close | Bolt-action rifle fired ten metres away, hard dry crack. | 1 | 5 | |
| NF-07 | Rifle bolt | Rifle bolt worked fast, four sharp metallic movements. | 1 | 4 | |
| NF-08 | Submachine gun, close | Fast-firing submachine gun burst ten metres away, a ripping buzz of sharp light cracks. | 2 | 5 | PPSh |
| NF-09 | Light machine gun | Pan-fed light machine gun firing short bursts, slower heavier thudding cracks. | 2 | 4 | DP |
| NF-10 | Grenade primed | Grenade fuse struck or lever released, a click and a faint fizz. | 1 | 3 | |
| NF-11 | Grenade lands on steel | Hand grenade landing on a steel deck and rolling, two hard knocks and a rattling roll. | 2 | 4 | then HT-18 |
| NF-12 | Grenade lands in dirt | Hand grenade thudding into earth near a vehicle. | 1 | 3 | |
| NF-13 | Bottle thrown | Glass bottle with a burning rag flying through the air, faint flutter of flame, then HT-21. | 1 | 3 | |
| NF-14 | Boots climbing the hull | Man scrambling up the side of a steel vehicle, boots scraping and kicking plate, a hand slapping for a hold. | 3 | 5 | |
| NF-15 | One man on the roof | Boots walking and shifting on a steel roof directly overhead, hollow thumps and scrapes. | 5 | 4 | wet |
| NF-16 | Many men on the hull | Ten men climbing over a steel vehicle at once, a confusion of thumps, scrapes and knocking equipment from every side. | 10 | 3 | loop; wet |
| NF-17 | Rifle butt on a hatch | Wooden rifle butt pounding on a steel hatch lid, heavy woody bangs. | 3 | 4 | |
| NF-18 | Hatch handle tried | Steel handle rattled and wrenched from outside, a lid jerked against its lock. | 3 | 4 | |
| NF-19 | Hatch pried | Steel bar worked under a hatch lid, scraping, creaking, a sudden bang as the lock gives. | 4 | 3 | |
| NF-20 | Hatch torn open from outside | Heavy lid swung up by several hands and slammed back on its stop, outdoor sound rushing in. | 2 | 3 | wet |
| NF-21 | Vision slit smashed | Rifle butt smashing a thick glass block in a steel frame, crack, a second blow, glass grinding. | 2 | 3 | |
| NF-22 | Something in the gun barrel | Mud and a rag rammed into the mouth of a steel tube, wet stuffing sounds. | 2 | 2 | |
| NF-23 | Shots into the hatch | Submachine gun fired down through an open hatch into a steel compartment, deafening hammering bursts and ricochets ringing round the walls. | 2 | 4 | wet |
| NF-24 | Grenade dropped inside | Hand grenade dropped through a hatch, bouncing on a steel floor among brass cases, rolling. | 2 | 3 | wet |
| NF-25 | Man falls off the hull | Body knocked off a moving vehicle, a thud on steel, a scrape, a heavier thud on the ground. | 2 | 4 | |
| NF-26 | Men swept off by the MG | Bullets striking men and steel together at close range, dull hits among ringing ones, bodies falling against plate. | 3 | 3 | |
| NF-27 | Turret turned under men | Steel turret rotating with men clinging to it, boots dragging and skidding, a shout cut short. | 3 | 3 | |
| NF-28 | Pistol in the turret | Automatic pistol fired upward inside a small steel room, a flat stunning bang and a ringing ricochet. | 1 | 4 | wet |
| NF-29 | Anti-tank rifle fired | Long heavy single-shot rifle fired thirty metres away, a booming crack with a kick of dust. | 2 | 3 | |
| NF-30 | Digging in | Entrenching tools biting into earth fast, several men, urgent. | 6 | 2 | loop |

## 12. How to work through it

1. Start with sections 5, 1 and 6. Those are the three the owner named first, and they are where
   the present files are thinnest: three hit recordings cover every calibre today.
2. For each row keep the number of takes listed, loudness untouched. The game sets the level.
3. Name files `<ID>_<nn>.ogg` (for example `HT-06_03.ogg`) so a row can be traced back to its
   prompt.
4. Nothing here is wired into the game yet. Each new family needs an entry in the sound tables
   before it plays.

## 13. What the owner had already starred (read 9 November 2026)

About fifty takes were starred on the generator's Favorites page before this list existed. What
they have in common is the lesson: nearly all are **wet takes recorded from inside the turret**,
and several are **whole scenes** in one prompt rather than single events.

| Starred prompt (shortened) | Nearest rows here |
|---|---|
| Heavy steel tank hatch unlock and swing open to outside air | IN-23, NF-20 |
| Heavy steel tank track breaking with a loud clang and shower of sparks | BR-01 |
| Tank round impacting earth nearby | HT-23 |
| Rapid thuds of small chunks of metal, or small stones, on a cramped steel-walled vehicle | HT-10, HT-23 debris tail |
| Gravelly ball bearings on Tiger frontal armour with faint interior ricochets | HT-02, HT-10 |
| Inside a Tiger, muffled thud of a shell on thick armour; small bullets barely heard through the turret | HT-01 to HT-06, wet interior |
| AP shell on the mantlet or glacis, recorded in tight stereo inside a resonant turret, with low-frequency weight | HT-06, HT-07, HT-16, wet interior |
| Round penetrates the hull, fragments tear through crew and components | HT-09, HT-10 |
| Hatch clangs in winter, boots, panicked Soviet crew shouts; crews scrambling out of stricken tanks | SV-01 to SV-08, TS-09 |
| Propellant ignites, hiss and popping inside a cramped tank, pressure venting, the turret thrown off | BR-24, BR-25, DS-11 |
| A 30-second scene: hit, fire, shorts, crew fighting it, heat building, cook-off | a scene, not a row |

Two changes follow for the next batches:

1. Section 5 (hits) gets a wet interior take beside every dry one, with the wording that worked
   for the owner: "recorded inside the tank turret, tight stereo, resonant steel compartment,
   distinct low-frequency weight; the impact itself is outside the armour".
2. Scenes are worth generating as scenes. They cannot be mixed by the game the way single events
   can, so they suit scripted moments and reference listening, not the event tables.

The first batch rendered from this list (82 takes: sections 5, 1 and 6, one take each, older v2
effect model) is in the captures folder `sfx-elevenlabs-20261109`, with `prompts.json` recording
what was sent. The generator now offers a newer effect model; the render script still asks for v2.
