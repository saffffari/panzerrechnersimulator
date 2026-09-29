# The first level: Act 1 from the district to the west exit (29 September 2026)

The owner asked to "confirm all critical gameplay stuff for map 1 and make sure the full first level
as outlined is playable", and decided that "the intro to the game is going to be urban combat". This
document fixes what the first level is, from the sources below, and how it is built and verified.
Nothing in it reaches the player as words (CLAUDE.md, first principle).

Sources: `docs/ACT1_MAP.md` (route tree, E to W breakout), `docs/ACT_ONE_PLAYTEST.md` (campaign
construction, objectives, death, saves), `docs/URBAN_INTRO_20260928.md` (the district, its street
encounter and tableaux), `docs/WORLD_AND_NAVIGATION.md` (paper map and compass, no waypoints),
`src/campaign.rs` `ACT1_OBJECTIVES`, `assets/worldgen/act1/level_design.v1.json` (entry gate at the
east spawn, exit gate in the west), `src/winter_mission.rs` (the winter session's field exercise).

## The flow

One crewman, one Tiger I, Stalingrad in January 1943.

1. **Start in the district.** The Tiger stands on the Act 1 spawn (1850, 250) in the courtyard of
   the urban-intro district (585 x 600 m of burned and shelled blocks, the tram street, the square,
   the tableaux). The crewman works every post: driver, gunner, loader (the 88 loaded by hand),
   commander, radio operator (hull MG); he can dismount and remount by the hatch ritual.
2. **Urban combat.** The district's defenders already stand in its streets: two T-34/76 platoons
   (3 + 2) on street beats with their riders, and two 45 mm anti-tank guns sited by the
   field-of-fire probe over the tram street and the ways out north. They find the Tiger by sight
   and sound. No route contact materializes until the district's armour is destroyed or the Tiger
   has fought its way out of the district (450 m from the spawn); crews left behind fight on.
3. **Choose a route west** (objective `Act1ChooseBreakoutRoute`): after 220 m of travel or the first
   sheet. The paper map and compass are the only navigation; there is no marker.
4. **Three guarded map sites.** The route generator picks three authored military sites in strict
   forward order between the spawn and the west exit. Each is guarded by a T-34 platoon that springs
   when the Tiger enters its prepared position. With the guard destroyed, the crewman dismounts and
   physically searches the folio; the first two sheets are the two city layers
   (`Act1WitnessCityLayers`, target 2). Site two and three do not materialize before the preceding
   sheet is recovered.
5. **The west cordon** (`Act1BreakWesternCord`): a blocking force of 2-4 T-34s on the final leg.
6. **The KV-2** (`Act1DefeatKv2`): the gatekeeper, destroyed or flanked.
7. **The exit** (`Act1ReachWesternExit`): with three sheets, no live contact and no mandatory
   contact left, the mounted Tiger inside the third sheet's inset extraction gate on the west edge
   completes the act; Campaign continues to Act 2, Playtest ends at Act 1.

Death is black and silence, then the act restarts from its entrance checkpoint (a new route for the
new attempt). Saves are act-entrance, same-attempt checkpoints.

The winter session (`tools/launch_winter_session.sh [--urban]`) remains a separate field exercise
on the same maps: house, wreck salvage, first and second shot, withdrawal; it is not the campaign.

## What was changed to make it so

- **Content.** Act One's content is the urban-intro workshop
  (`assets/worldgen/workshops/act1-urban-intro-20260928/manifest.v1.json`), which holds the whole
  Act 1 map (same terrain, spawn, goal, landmarks and 13,069 placements) plus the courtyard and the
  district: `act_manifest::playtest_manifest_path()` now prefers it when `KESSEL_PLAYTEST_MANIFEST`
  is unset (then the winter approach revision, then the canonical Act 1 manifest). The title menu's
  campaign, Playtest, the checkpoint Load and `campaign-ai-run` all select it.
- **Route.** Act 1's campaign route started at the map centre and left by any edge. It now starts
  at the level design's authored entry gate (the district spawn) and leaves by the edge of its
  authored exit gate (west); the three sites still vary by seed and attempt. The Tiger stands on the
  manifest's Spawn marker.
- **Opening.** On a map that lays the street encounter, campaign Act 1 fields the district's
  defenders at the start (`Game::field_campaign_street_opening`, the winter session's own fielding)
  and the director holds every route contact until their armour is destroyed or the Tiger has left
  the district; then the survivors fight on under the ordinary campaign AI and route contacts take
  the free enemy slots (a wave no longer clears every slot).
- **Launchers.** `tools/launch_winter_session.sh --campaign` opens the title menu on the first
  level without the winter-session override, so ONE PLAYER / ACT ONE CAMPAIGN (and ACT ONE /
  SKIP INTRO) play it; `tools/launch_current_build.sh --campaign` builds and starts the campaign
  directly (`--menu --campaign` for the title menu). The defaults (the courtyard session, `--urban`)
  are unchanged.
