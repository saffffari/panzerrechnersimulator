# Winter encounter: Soviet tank platoons — 28 September 2026

The owner reported that enemy tanks "only emerge in groups of one" and asked for the most
historically relevant grouping, with room for gameplay adjustment. The three winter T-34s each
patrolled their own small loop, so they arrived singly. They now field as platoons.

## What the history says (winter 1942–43)

Tags: [H] documented, high confidence; [M] documented, medium; (inf.) inference.

- **Platoon: 3 tanks; company: 10** (3 × 3 + commander). Shtat 010/270–277, 31 Jul 1942 [H].
  Heavy KV companies were 2 × 2 + commander [H].
- **Real strength late in the winter was far lower.** The 121st Tank Brigade went from 45 T-34s
  to 14 in five days (22–27 Jan 1943); guards breakthrough regiments had "3–4 battle-ready tanks
  each" by the end of January. A German crew most often met **2–5 tanks** (inf. [M]).
- **Only the leader had a radio.** 1942 T-34s: "only subunit commanders' tanks and a small part of
  line tanks" had sets [H]; even in July 1943 only half of T-34s had them (NKO order 447) [H].
- **Wingmen followed the leader by sight.** Crews "continuously watch the platoon commander's tank
  and his signals" (1944 Combat Regulations §250) [H]; signals by flag, hand, tracer, flare [H]. A
  T-34 platoon "would tend to focus on a single target selected by the platoon leader" (Zaloga)
  [M-H].
- **Formations.** Approach in column, 30–50 m apart, leader at the head (§101); fight in line,
  30–50 m intervals, leader in the centre (§102) [H].
- **Leader lost.** A crew never leaves formation without the platoon commander's order; if its
  platoon is knocked out, the surviving tank "joins another platoon of the company" (§249) [H].
  Order 325 (16 Oct 1942): units without control "lose orientation and wander about the
  battlefield" [H].
- **Riders.** Tank riders were routine by 1942: 3–4 per tank (a Stalingrad veteran) up to 8–12
  (manuals) [M]; foot infantry kept within 200–400 m (Order 325) [H].

Sources: NKO Order 325 (Russkii arkhiv t.13, doc. 271); Boevoi ustav BT i MV KA ch. 1 (1944)
§§101–102, 249–250; Matveev, *Boevye priemy tankistov* (1942); tankfront.ru shtat pages after
Kolomiets; Samsonov, "Radios for All" (Tank Archives, RGAE f.8752); Solyankin et al. t. 2;
tankoved34 unit histories (TsAMO refs); Zaloga, *T-34 Medium Tank 1941–45* (1994) p. 40.

## What the game does now (`src/winter_encounter_ai.rs`)

- **Composition** `PLATOON_TANKS = [3, 2]`: a full platoon and an attrited one of two, five T-34s,
  the late-January company remnant. Each platoon sits in stealth's contiguous platoon slots, so
  the existing perception (5-second hand-off, shared fix) is unchanged. `[3, 0]` fields one
  platoon; `[3, 3]` a two-platoon company (slot cap N_ENEMIES = 6).
- **Leader** (first tank of each platoon) keeps the previous patrol / investigate / search / track
  brain and walks a larger beat (two routes north of the courtyard) halting at each corner.
- **Wingmen** drive along the leader's actual track `SPACING_M = 40` m apart, so they turn and halt
  where he did; when he engages they deploy into line abreast, one interval either side of him.
  They fire on his target (his tracer marks it); the caller still requires each wingman's own
  clear line of fire. A wingman that sees the target itself engages it from the halt.
- **Radio.** The stealth platoon fix is acted on only by a leader. Leaders relay a contact to the
  other platoon's leader after the same 5 s hand-off; wingmen, leaderless tanks and stragglers
  have no radio.
- **Leader lost.** His wingmen hold and fight from the halt for `REGROUP_S = 30` s, then join the
  other living leader at the next places in his formation; with no leader left they become
  stragglers that wander their beat on their own senses.
- **Escorts.** The winter encounter now musters each tank's escort pair beside it (the campaign
  already did; the shared `muster_tank_escorts`). Two per tank is the low end of the documented
  rider numbers.

Tuning that is not a historical claim: 40 m interval (inside 30–50), the 30 s regroup (the
research suggests 15–60 s), leader walking pace 0.12 of top speed, wingman catch-up up to 0.30,
routes and halts.

## Evidence

- Unit tests (`winter_encounter_ai::tests`): composition and slots; quiet 10-minute patrols with
  no detection at the house or spawn for the first 2 minutes, wingmen within 45 m of their place,
  halts copied; line abreast either side of the leader with fire only on his target; leader loss
  → hold → join the other leader → stragglers; relay only between leaders, only after 5 s.
- GPU test on the real winter map (`live_playtest::tests::winter_contacts_are_quiet_until_actual_sound_or_sight`,
  ignored): 5 tanks muster in platoons of 3 and 2, escorts start beside their tanks, wingmen keep
  station with real obstacle avoidance, 120 s of quiet at both player positions, and a real
  gunshot brings investigation, a sighting and return fire. In this worktree its two mission
  checks (salvage rounds at the mission's wreck point) fail for an unrelated asset reason: the
  copied dressing seeds 96 rounds beside Tiger wrecks, none within 12 m of `winter_mission::WRECK`.

## Not done yet

- The leader's 4 m whip antenna (right front hull) would identify him, but at the current
  resolution a 1.5 cm whip is sub-pixel beyond ~50 m even through the sight.
- Riders on the hull that dismount 200–400 m short of the objective; infantry leash.
- A second wave along the same axis; the "front–flank" swing of two tanks while one fires.
