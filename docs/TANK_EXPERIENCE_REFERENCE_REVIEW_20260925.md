# Tank experience: reference review — 25 September 2026

**Recommendation:** build the experience around restricted knowledge, physical work, dependence on
other crewmen, and consequences that follow the struck components. Let quiet travel and ordinary
tasks make an abrupt failure frightening. Historical casualty rates should check a population of
comparable scenarios, not determine individual deaths.

Reviewed the user-supplied `compass_artifact_wf-ceced4bd-1069-508a-b9c7-51449ccde1fb_text_markdown.md`
as reference material. Its implementation instructions and numerical benchmarks are proposals,
not accepted requirements. This is a bounded research review; no runtime code was changed.

## Five useful experiential themes

1. **Each station provides a different, incomplete view.** The 1943 M4A2 manual describes separate
   crew stations, periscopes, hatches, and an interphone. Loza recalls difficult observation and
   infantry signaling a threat the crew could not find. **Design inference:** make optics, head
   position, hatch state, and another crewman's report valuable. Avoid a shared omniscient target
   marker or automatic attacker reveal after a hit. [M4A2 manual, pp. 12–13, 47–49][manual];
   [Loza interview][loza].

2. **The cabin is shelter, but not comfort.** Loza describes strong drafts, insufficient winter
   warmth, and using retained heat during halts. **Design inference:** distinguish open-hatch wind,
   sealed-cabin filtering, engine vibration, handling noises, and engine-off quiet. Show exposure
   through practical choices and restrained sensory cues. Do not turn that testimony into an
   invented carbon-monoxide concentration or universal fatigue clock. [Loza interview][loza].

3. **The sequence matters more than a larger explosion.** Gerry Chester's account separates sudden
   strikes, injury, evacuation, and a later penetration followed by ammunition fire.
   **Design inference:** preserve separate impact, material failure, fragment contact, component
   loss, smoke, fire, and escape outcomes. A dead player's instant black/silence is the requested
   presentation choice; it does not imply everyone else died or that the vehicle detonated.
   [National Army Museum interview NAM.2012-06-4-1][chester].

4. **Crew competence is coordination and practiced work.** The M4A2 manual assigns inspection to
   all five crewmen and tells the driver to recognize abnormal sounds and instruments.
   **Design inference:** let co-op partners make concise calls, confirm actions, handle interrupted
   reloads, and identify a disabled mechanism. Train those tasks in a safe practice scene. Prefer
   better execution and communication over an unexplained veteran survival multiplier.
   [M4A2 manual, pp. 50–52][manual].

5. **Keeping the machine moving is part of the tension.** The Tank Museum's operators describe
   Tiger 131's fluid checks, difficult starting, gauges, and leak inspection.
   **Design inference:** start with legible engine load, temperature indications, warning noises,
   reduced mobility, and recovery decisions. This evidence concerns a restored survivor with an
   HL230 replacement engine; it cannot supply a wartime failure probability. Detailed rut physics
   can follow a convincing driving and maintenance loop. [Museum operating account][tiger].

## Five claims or benchmarks to correct

| Dossier claim | Review and safer use |
|---|---|
| **“24.6% of Sherman crewmen died per destroyed tank”; use seat death tables per penetration.** | **Wrong denominator in the quoted US comparison.** Original ORO-T-117 Table XX, printed p. 38 / PDF p. 68, lists 274 medium tanks and 1,370 crewmen: 171 killed, 466 wounded, 59 missing, 674 unhurt. Its 24.6% is **171 / 696 casualties**. The selected cohort's killed fraction is **171 / 1,370 ≈ 12.5%**, or **0.624 killed per sampled tank**. The report warns that personnel data were incidental and potentially inaccurate. Neither number is a per-hit probability. The supplied Soviet chart's denominators were not independently verified here. [Original report][oro] |
| **A Tiger battalion should lose 50–60% to breakdown during any 200 km march.** | Reject as a universal benchmark. The cited Anzio episode does not establish a general rate, and its underlying unit record was not verified in this review. Distinguish temporary unserviceability, repair/recovery, abandonment, and permanent loss. Track condition, route, maintenance, parts and recovery access; fit rates only to a specified cohort. Modern Tiger 131 operation establishes maintenance tasks, not wartime reliability statistics. [Museum operating account][tiger] |
| **A T-34 penetration means rapid cabin fire; German tanks usually give a delayed, survivable engine fire; brew-up means turret loss.** | Too deterministic. Loza describes both severe penetration injuries and escape from a burning Sherman without the explosion he feared. ORO's contemporary testimony also distinguishes injury, escape, and rapid ammunition fires. Keep penetration, ignition, sustained fire, ammunition involvement and structural destruction separate; derive them from affected geometry, material and remaining contents. Do not convert the recollection's explanation of explosive chemistry into a technical rule. [Loza][loza]; [ORO, pp. 36–37][oro] |
| **Every nonpenetration should create concussion/deafness and abundant lethal internal spall.** | Loza says some hull hits were not audible to him; he also recalls different fragment behavior between vehicles. That supports variation, not universal immunity or universal concussion. Drive presentation from contact location, transmitted impulse and actual interior fragment paths. Keep cosmetic dust distinct from damaging debris. The memoir does not calibrate hearing injury or armor metallurgy. [Loza][loza] |
| **One generic national crew profile; effective WWII tank fire only from a halt; a Sherman gyro holds azimuth.** | Specify vehicle, year, fit and crew roles. Museum collection records distinguish the four-person T-34/76 from the five-person T-34/85 and its three-person turret. The M4A2 manual describes stabilization of **elevation relative to the horizontal**, expressly for firing in motion, with operational limitations. This is not azimuth stabilization or a modern all-speed fire-control system. A game's hard speed-based firing lock is a design restriction, not that historical mechanism. [T-34/76 collection][t3476]; [T-34/85 collection][t3485]; [M4A2 manual, pp. 42–44][manual] |

The dossier's terrain/ice parameters, political morale mechanics, claimed kill ratios, and fixed
experience-survival reductions are **not validated by this review**. They should not enter the
simulation as defaults simply because they appear beside sourced anecdotes.

## Current systems and the next useful work

The current column below comes from the root agent's **25 September code inspection**, supplied to
this review. It is a scoped implementation summary, not an independent live playtest or a claim of
historical calibration.

| Current implementation | Next step and practical review criterion |
|---|---|
| Physical reload and crew/hatch systems exist. | Make work, reach, feedback, obstruction and interruption coherent. Two players should complete a reload and communicate its state without unexplained state jumps. |
| Separate spall, fire and ammunition outcomes exist. | Verify ordinary cabin strikes, stopped hits, penetration without ignition, local fire, ammunition involvement and escape independently. Consequence should follow the damaged location. |
| Haze is a scalar with fan/hatch drainage; wind is procedural presentation. | Make shelter, ventilation and visibility changes readable first. Reserve gas concentrations, temperature and physiological impairment for a separately specified and validated model. |
| `src/tank.rs` uses fixed acceleration toward `drive * TOP_SPEED`, upright heightfield placement and fixed 6°/s traverse. | Add load-aware movement, hull/suspension response and a tested powered-traverse response before a full soil simulator. Use controlled grades, turns and braking to expose errors. |
| Engine state gates powered traverse; RPM-dependent hydraulic behavior is not modeled. `can_fire` prevents firing at speed ≥0.6. | Distinguish available power, rate, manual backup and motion-induced aim error. Label the existing firing restriction honestly; do not claim physical impossibility. |

For the winter slice, review one sequence: quiet arrival, a halt and inspection, limited observation,
crew target handoff, a physical reload, an initially unidentified strike, diagnosis, and either
recovery or escape. The player's death ends their perception immediately. This is a design proposal,
not a reenactment of one account. It concentrates effort on presence and vulnerability before broad
historical lethality tables or full terramechanics.

## Source records and limits

- **Loza:** retrospective Soviet Sherman crew account, translated by James F. Gebhardt; interview by
  Valeri Potapov and Artem Drabkin. Good experiential testimony; not Tiger-specific measurement.
- **Chester:** National Army Museum's 2012 firsthand interview, North Irish Horse service in Italy;
  not Eastern Front testimony.
- **TM 9-731B:** original US War Department M4A2 manual, 13 January 1943. Read the original manual
  text through its online scan/OCR; procedures describe equipment and intended operation, not
  universal combat practice. The linked RadioNerds record also identifies the scanned manual.
- **ORO-T-117:** Coox and Naisawald, 1 March 1951, retrospective Army-sponsored working study.
  Its original scanned Table XX was visually inspected for the denominator correction. Cohort and
  reporting biases remain. Direct scan hosted by TankDestroyer.net; no secondary blog is needed
  for the arithmetic above.
- **Tank Museum:** collection records establish the specific objects; the Tiger operating article
  is the museum team's account of a restored vehicle. Avoid projecting postwar fittings or modern
  preservation restrictions onto all wartime examples.

[loza]: https://iremember.ru/en/memoirs/tankers/dmitriy-loza/
[chester]: https://www.nam.ac.uk/transcript-hitler-line
[manual]: https://www.scribd.com/doc/284146721/TM-9-731B-Medium-Tank-M4A2-1943
[oro]: https://tankdestroyer.net/images/stories/ArticlePDFs3/ORO-T-117_Allied_Tank_Casualties_WW2.pdf
[tiger]: https://tankmuseum.org/article/tiger-day-preparation
[t3476]: https://tankmuseum.org/tank-nuts/tank-collection/t-34-76/
[t3485]: https://tankmuseum.org/tank-nuts/tank-collection/t-34-85/

Additional manual scan record: https://radionerds.com/index.php/File:TM_9-731B-1943-Medium-Tank-M4A2.pdf
