# Design backlog from the 3 October 2026 playtest session

Everything the owner asked for during the session that is not yet built, in the owner's intent,
grouped by system. Status is one of: **open**, **partly done** (with what exists), or **done**.
This is a requirements list. It describes targets, not implemented behaviour.

Two requests here change standing project rules, and are recorded as owner decisions:

- **On-screen text is now allowed where it serves play.** The fire prompt and radio messages
  are text. `CLAUDE.md`'s first principle (no written words) needs amending
  to say what text is permitted: short control prompts and radio messages.
- **Gameplay may depart from the historical model when the model makes dead time.** The uphill
  speed boost is the first such departure and is recorded in `docs/PHYSICAL_TRUTH_AUDIT.md`.

## 1. Story and structure

| Item | Status |
|---|---|
| The whole game is the journey west to Berlin. At the outset, in the blizzard, the crew is told to head west to defend Berlin. | open |
| The route ends at the Reichstag against an impossible number of enemy. | open |
| Ending one: the tank is destroyed and the player dies. | open |
| Ending two: the player bails out, runs to safety, escapes Berlin, finds a running car, and drives with direction guidance to a pristine farmhouse on a prairie. Home. | open |
| A pixelated black-and-white photograph of the commander's wife and son is taped to the left wall of the turret from the first minute. They are who is at the farmhouse. | open |
| The choice to leave exists only at the end, after both companion Tigers are knocked out. One of their crews sends a dying request: get out, find a vehicle, go home. Nothing else is said. The game never states that there is a choice, so staying or leaving is the player's own. | open |
| The companion tanks can be lost at any point, not only at the Reichstag. When the last one is knocked out its dying request is always sent, whenever that is. Earlier in the game there is nowhere to go, so in practice the way home is only reachable by a player who reaches the end with it still fresh. | open |
| The stay-and-fight ending is unscripted: it ends when the player is knocked out. As he dies he takes hold of the photograph, and if the tank burns it burns with him. Either ending stands alone as the one ending a player sees. Seen together, the meaning is plain. | open |
| The car exists only after the player bails out of the last stand, and only if he is the last one left. A second, fast vehicle (handling like a racing car next to the Tiger) is added later so the long distance to the farm driveway is covered quickly. | open |
| Decided: the farmhouse is empty and exactly as he left it. No damage, no sign of what happened to them. The player searches the ground floor and the upper floor, finds no one, then sits on the porch and chain smokes until the sun is down. The reunion is dropped. | open |
| The last scene is the porch at sunset over untouched farmland, with a pack of cigarettes to chain smoke until the sun is under the horizon. The cigarette has no effect there. It is only a cigarette. | open |
| Theme: an anti-war piece about the individual in the machine. To fight is to die. To not is to live in peace. | open |
| Messages go by radio. (The Enigma machine idea is dropped as inaccurate.) While the radio operator is alive the player can send a message from anywhere in the vehicle with Enter. With him dead the player has to go to the radio set. A language model handles the exchange, as the route toward a real story. | open |
| The radio set is a physical object at the radio operator's position with dials, knobs, switches and lights, and a headset, handset or microphone. Use an existing asset from the pizza packs or the other asset folders. Interacting with the headphones opens the message prompt. | open |
| Once the blizzard clears, a spring map. The player links up with a friendly AI-controlled tank. Through a language model the player sends and receives messages with it about the story, the current fight and enemy positions. | open |
| Third map: the player keeps his own Tiger for the whole game. (A King Tiger was considered; there is no Tiger II model with an interior, only its armour data, and staying in one tank builds more attachment.) He links up with two other Tigers and commands the platoon of three. The companions are friendly AI that must accurately carry out instructions sent by radio and reply by radio. The platoon makes the last stand at the Reichstag. | open |
| Berlin is deferred: it is the largest build. When built it must be true to life, reconstructed from historical maps with real building footprints and roads. Not the whole city: the player enters at one point on the perimeter and fights through dense urban blocks toward the Reichstag. Ground beyond is not playable, but the area must span enough blocks for the player and the enemy to manoeuvre. | open, deferred |
| Canyon run contents are procedurally generated and carry more action. Driving across empty land is wasted playtest time. | partly done: director fields up to 12 tanks, shorter lulls, reinforcements advance toward the Tiger. Map contents are still authored. |
| Very prominent, large landmarks, man-made and natural, that are marked on the map. | open |

### Stakes before the ending: the crew

| Item | Status |
|---|---|
| The game starts with a full crew. Each man has a name (the player names them, as in FTL) and performs his seat's job until he is killed. After that the player has to do that job himself. Loneliness is arrived at, not given at the start. | partly done: an AI crew with per-seat casualties and wounds exists (`singleplayer_crew`); there are no names and the solo start is the default. |
| The crew have no abilities beyond the player's own. What they add is delegation: from the cupola the commander marks a point and the gunner traverses onto it without the player entering the sights, and the tank can be driven from the cupola or any other seat while the driver lives. | partly done: driving from every station exists. Marking a point for the gunner does not. |
| Crew cannot be reassigned. Each man stays at his own seat, which gives him importance and individuality. If the driver is killed the tank can only be driven from the driver's seat; if the loader is killed the player loads by hand. The cards dealt are the cards played. | open |
| With a living gunner, the commander can go straight to the gun sights from the cupola: E on the gunner. The whole gunner figure, looking down his sights, is outlined as the target, so it is a large and easy thing to hit. The player still mans the gun himself. | done in code, dormant: it needs the full-crew start, which does not exist yet (the game still starts with a lone crewman). |
| A dead crewman's body stays in his seat. To use that seat the player must pull the body out and drag it out of the tank. | open |
| After a hit the player has to find out what happened by looking. A dead or unconscious gunner or loader has collapsed onto the turret floor; a driver or radio operator has slid down and slumped in his seat. No status readout. | partly done: crew casualties, wounds and witness bodies exist; the collapse and slump poses need checking against this. |
| A lightly wounded crewman is healed by lighting a cigarette and giving it to him. A man the damage model kills outright cannot be helped that way. Historical note: tanks carried a first-aid box of dressings and each man had field dressing packets; a purpose-made tourniquet was not standard individual issue, so a belt or strap is the plausible form. | open |
| Medical supplies are reinstated, so a wounded crewman can be treated and kept alive. | open: the earlier medical mechanic has to be found and restored. |

## 2. Controls: one interaction key

LMB currently means both "interact" and "use what is in the hand", which is confusing.

| Item | Status |
|---|---|
| One consistent action button. Left mouse is the action button (interact when the crosshair dot shows, otherwise fire or use the item in hand). Space enters and exits hatches, as it always did: E was tried for that and kept bailing the owner out in combat. E is only a second interact key. | done |
| E enters a spot (sight, hatch, seat); E again leaves it. | partly done: E enters. Leaving a spot still uses its existing key. |
| Items stow only in their own starting stowage spot. That spot lights up while the item is in hand and the cursor is over it. | open |
| Cupola hatch is worked by E on the lever below it. The H key is removed. | done |
| At the cupola Shift is head out and in, and held Space (1.5 s) climbs out, from either position. Both need the hatch already open; nothing opens it automatically. The lever pivot cylinder is part of the hatch lever target and outline. | done |
| C is the cigarette. The compass moves onto M, coupled with the map. | done |
| Arrow keys traverse and elevate the gun from the cupola and head-out, and remain available in the sights where the mouse also works. | done |
| Jump is disabled while the player is on fire. | done |
| Ignition is a solid red illuminated button, not a ring. The engine can only be restarted from that button at the driver's seat. | open |
| Physical dials for engine rpm and road speed, on the driver's right. Fuel is a dial, not text. | open |

## 3. Reloading

| Item | Status |
|---|---|
| Faster ram and faster ejection. | done: the case pulls in 0.3 s and a lined-up round rams in 0.35 s. |
| E equips a round; move it roughly onto the axis of the open breech; E rams it; E on the lever closes the breech. | done (not yet play-tested by hand) |
| Spent casings can be thrown out of the escape hatch or any open hatch to clear the floor. | open |
| Racks start visibly full. | partly done: they are logically full (64 of 64). The model has 64 cartridges where a Tiger carried 92, so some tray positions look empty. |

## 4. Health, blood and the cigarette

| Item | Status |
|---|---|
| Health is litres of blood, starting at a realistic volume for a six-foot man. | open |
| Wounds drain blood by severity and location. Organs, major arteries and veins, and other vital parts set the loss rate. | open |
| Blood drops are cube pixels that behave roughly like a fluid and stain what they touch, inside and outside the tank. | partly done: blood cubes and stains exist for casualties; no fluid behaviour, no player blood volume. |
| Cigarette tokes restore blood. One cigarette is one full bar. | partly done: each inhaled second restores body health and a whole cigarette restores a full bar. It heals the existing health value; litres of blood are not modelled yet. |
| At full health the player can chain smoke; the reward is a vision boost. The game is about visibility. | partly done: chain smoking works and a boost level (rises with tokes, fades over 50 s) is tracked. It is not yet applied to the picture. |
| Held like the Luger, then raised to centre screen, bottom third, while lighting and toking. | done |
| The pack sits in the left hand; holding RMB pulls one out and holds it. | open |
| Toking: the tip glows more orange, an inhale sound plays. Release exhales a smoke cloud from that spot, sized by the toke length. Toking too long causes coughing. | partly done: hold to toke, brighter tip, exhale from the held position scaled by the toke, cough after 4.5 s and a forced stop at 6 s. No inhale or cough sound yet. |
| Lighter and cigarette are light sources on the interior: lighter dim, cigarette dimmer but noticeable. The cigarette in hand stays a faint light and never blocks interaction. | done: the tip is a shadowed light, brighter during a toke, and E works with a cigarette in hand (it is pocketed when both hands are needed for a round). |
| The torch is the bright spotlight, centred on the crosshair, mostly for outdoor night use, and it alerts enemies. The cigarette is the low-visibility light for close work on foot at night. | open |
| Sounds: inhale, lighter, pulling out the pack, and sounds for the other inventory items. | open |
| The Luger's cowboy-style draw rotation is liked; apply it wherever the Luger is drawn. | open |

## 4b. Death

| Item | Status |
|---|---|
| Remove the heavenly sound on death and shorten the black screen. | done: no score plays; fade, two seconds of black, five seconds of card. |
| The fallen card shows a pixel black-and-white portrait of the same character model in his helmet with no face, so the card never reads as a tribute to a tank ace. | done as a hand-drawn pixel bust. A portrait rendered from the actual model is open. |
| The fallen card can be skipped with Space. After it come two replay shots. First a fixed aerial view from behind the Tiger (or the man on foot) with the gun that fired in frame, showing the round coming in. Then the interior, from the centre rear of the turret looking forward: commander and gunner on the left, loader on the right, and the driver and radio operator in frame if they can be. This is the shot that shows what the hit did, with every effect available. | partly done: both shots run after the card and Space skips the card and the shots. The exterior shot is a fixed raised camera with the round shown as a slow tracer and a flash at the strike. The interior shot is the existing rear-turret slow-motion aftermath. Neither has been seen running yet, and the interior framing has not been widened to take in the hull crew. |
| The interior death explosion sounds more explosive and catastrophic. | done: the ammunition detonation has a sustained roar and keeps its loudness through the mixer (a test that was failing before today now passes). Not yet judged by ear. |

## 4c. Sound

| Item | Status |
|---|---|
| T-34s blowing up must be heard. | done as an audibility floor on kills, cook-offs and strikes after the physical propagation. Not yet judged by ear. |
| Impacts need more bass. | done: the three lowest octave bands lose up to 6 dB less on the path for strikes and explosions. |
| More stereo panning, to judge direction by ear. | done: every world sound's bearing is widened (15 degrees off the nose plays at about 35). |

## 5. Fire

| Item | Status |
|---|---|
| Louder ignition, and a burning, crackling, hissing chemical fire sound that grows as the fire does. | open |
| Fire light moves from dull orange to bright red as the tank heats, by the size and place of the fire. | open |
| Fire reaching the ammunition, or the tank getting too hot, blows the whole tank up, turret pop included. | partly done: cook-off and turret pop exist; heat-driven escalation does not. |
| If the player catches fire, small centre-bottom text: press Space to pat down. Outside the tank, mashing Space goes prone and puts it out much faster, in the open and exposed. | done as rules and prompt (`src/player_burn.rs`). No flame visuals or sound on the body yet. |

## 6. Track repair

The current repair does not feel right. The target, simplified but realistic:

1. A track hit removes the link that was struck.
2. E takes a spare link from the ones stowed on the turret into the left hand. The hammer is in
   the right hand.
3. Cursor over the gap, insert the link.
4. Hold a headed pin in place and hammer it through the link.
5. If the tank was driven after the break, the ends are further apart: E grabs the looser end,
   the cursor drags it to within one link of the other end, then steps 3 and 4.

Spare links on the turret are finite. The hammer salvages links from the tracks of wrecked
Tigers. Status: **open**.

## 7. Weapons, ammunition and loot

| Item | Status |
|---|---|
| Kar98k and MP40 models on a rack in the turret, or near the floor right of the fire extinguisher. | open |
| Ammunition boxes at the start and much more ammunition, so infantry numbers can rise with it. | open |
| One weapon or Panzerfaust equipped at a time. Up to three spare magazines for the Luger, Kar98k and MP40; one spare Panzerfaust. The loaded magazine or round is extra. | open |
| Bullets are sized per calibre and are not universal. Soviet weapons are worth picking up. | open |
| Dead Soviets drop their firearm or one or two rounds. Dead Germans sometimes hold small-arms ammunition. Wrecked Tigers sometimes hold tank rounds. | partly done: bodies carry loose rounds. |
| An ammunition box inside the tank at the radio operator's seat stores collected ammunition, so a solo player goes there. | open |

## 8. Enemies and the battlefield

| Item | Status |
|---|---|
| Many more enemy tanks, operating in numbers. | partly done: see section 1. |
| Anti-tank guns throw sparks when hit and blow up on a direct hit. | open |
| Infantry crouch and use cover instead of standing exposed. | open |
| Wrecks read as wrecks: black soot and rust, open hatches, missing turrets, missing barrels, flipped over, lodged in buildings, in groups, with a hundred dead infantry around them. | open |
| Bodies use the same player and Soviet infantry models, frozen, with blood around them. A hundred times as many, clustered at plausible defensive points and around destroyed armour. | open |

## 9. Map

| Item | Status |
|---|---|
| Simple linework. No foliage. Very light contour lines. | open |
| Key landmarks, the starting point, and a clue to where to head first. | open |
| Pencil with LMB, erase with RMB. | partly done: pencil exists. |
| A pixel map with enough pixels to be usable. | open |
| A north, south, east, west indicator. | open |
| Pixel German text and numbers around the margins, for feel. | open |

## 10. Rendering and performance

| Item | Status |
|---|---|
| Vision ports are light sources for the interior and adapt to what is outside. The cupola ring must not be pitch black. | partly done: ports stay lit from every angle. They do not yet illuminate the interior. |
| Ports out of frame are not rendered; slits can be approximated as light sources. | partly done: ports outside the view are already skipped. Ports hidden behind cabin geometry are still rendered. |
| The game looks blurry. | open: the cause is the bloom halo, not the anti-aliasing. |
| Frame rate. | partly done. In the moving-commander benchmark the frame went from 66 ms to 28 ms and the gunner sight to 33 ms: cabin-lamp shadows ride the turret at 20 Hz, hidden ports are skipped, the arms no longer invalidate caches at the sights. The fine environment-visibility cascade (about 7 ms) is the largest cost left. Not yet measured in live play. |
| Experimental graphics upgrades, as render stills to choose from. | open |

## Suggested build order

1. **E interaction key and the reload flow.** It touches every later feature and removes the
   biggest source of confusion.
2. **Frame rate at the sights and in the cupola.** Nothing else reads well at 13 fps.
3. **Dense canyon**: procedural placement of wreck groups, body clusters, gun positions and
   armour along the whole route, so every playtest minute has something in it.
4. **Blood, fire on the player, and the cigarette loop.** One connected survival system.
5. **Track repair**, then **weapons and loot**.
6. **Map and landmarks**, which lead into the Berlin route.
7. **Radio messages and companion tanks**, and the two endings.
