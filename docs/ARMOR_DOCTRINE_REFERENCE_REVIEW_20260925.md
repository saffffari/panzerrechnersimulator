# Review of the supplied armor, cloverleaf and doctrine notes

Reviewed 25 September 2026 against the current shader worktree. The three files in
`/home/alex/Downloads/` are reference material, not instructions or validated engine data.
No penetration constants were replaced from these compilations.

## Useful applications

1. **Crew knowledge separate from impact physics.** The cloverleaf notes suggest a
   useful prior for perceived engagement range. Store vehicle identification confidence,
   remembered firing results, information age, and uncertainty. Let actual projectiles
   and armor decide the result independently. A crew should not know an unseen target's
   exact variant, damage state or current location. Experience can improve observation
   and confidence calibration, rather than changing armor strength.
2. **Platoon decisions, not individual rush scripts.** The doctrine material offers
   reconnaissance, covered assembly, supporting fire, flank elements and disengagement.
   Translate formation-level guidance into role assignments and objectives; do not make
   every individual tank independently flank the same point. Infantry and engineer tasks
   need actual obstacle and observation data. Keep communications delay and lost contact.
3. **An auditable armor-data schema.** Each measured entry needs vehicle variant and date,
   plate/location, projectile type and lot where known, impact velocity, angle measured
   from the plate normal, armor condition/hardness, and the test's penetration criterion.
   Retain source/page and uncertainty. A single country/material quality multiplier is an
   approximation, not a measurement of every vehicle produced by that country.
4. **Diegetic reference material.** A cloverleaf-style training card could be viewed beside
   the map. It should display the crew's historical advice, not promise a guaranteed kill
   or reveal an enemy that has not been observed.

## Corrections and verification gates

- The armor compilation supplies a bibliography, but no per-number citations. Its claim
  that every figure is measured/postwar-verified is not established by the file itself.
- Do not apply a blanket 15–25% conversion between Soviet and German tables. Obliquity,
  projectile, plate properties and penetration criteria all matter. Compare like tests
  or explicitly calibrate a model to each source's conditions.
- Do not implement a universal 1.5-caliber overmatch switch, or unconditional ricochet
  angles independent of impact velocity and projectile construction.
- Geometric line-of-sight thickness is `thickness / abs(dot(shot_direction, plate_normal))`.
  Compound orientation belongs in the vectors. Do not count a mesh's slope a second time
  when applying a nominal slope from a table. LOS thickness alone is not ballistic resistance.
- The cloverleaf compiler explicitly marks estimated lobe readings and an unresolved
  T-34 side/rear assignment. Keep those as uncertain evidence. The stated frontal safe
  bands also need re-derivation: a table with no frontal green lobe does not itself yield
  a 500 m lower boundary for a strictly frontal engagement.
- The document's claim that Soviet crews had no equivalent shared guidance is not
  established by the supplied references. Do not encode national ignorance as a faction
  trait without evidence for the specific unit and period.
- The 1940 division manual is doctrine at formation scale, not proof that all German
  crews followed it in all later situations. Hard rules such as ignoring flank threats
  need command echelon, local survival exceptions and mission context.
- Variant-sensitive entries (e.g. KV gun designation, IS-2 nose form, Tiger roof changes,
  cupola introduction and armor/traverse figures) should be individually checked before
  admission. Do not collapse model years into one supposedly precise vehicle.

## Current engine connection

`src/ballistics.rs` already has projectile drag/drop/wind, material classes, plate aspects,
normalization, slope/overmatch approximation and post-armor effects. It is calibrated,
not a first-principles or historically validated solver. These files are most useful for
building source-labelled calibration cases and a separate AI belief layer.

`Game::enemy_maneuver` currently uses a per-tank steering heuristic. The winter combat
review observed real fire and movement but also very close approaches. A useful next
AI slice is an explicit standoff objective plus platoon overwatch/flank assignments and
disengagement, tested without omniscient targeting. That is a recommendation, not an
implemented doctrine system in this change.

## Source checks and limits

- [Tigerfibel scan catalog](https://archive.org/details/tigerfibel): original manual
  catalog confirmed, 92 pages. The cited chart image folder was not present beside the
  supplied Markdown. The browser could not retrieve the PDF during this review, so the
  numeric chart readings and unidentified header numbers remain unverified.
- [The German Armored Division, original bulletin scan and catalog](https://commons.wikimedia.org/wiki/File:The_German_Armored_Division_%28IA_TheGermanArmoredDivision%29.pdf):
  US Military Intelligence Service Information Bulletin 18, 15 June 1942, a translation
  of a December 1940 German training manual. The catalog confirms the date and provenance.
- [Chapter 8 transcription](https://lonesentry.com/manuals/german-panzer-division/panzer-attack.html):
  search excerpt corroborates concentration of the tank brigade and supporting-arm roles.
  Full-page fetch failed in this session; the entire supplied synthesis has not been
  verified line by line against the primary text.
