# Canyon v2: extended first-person run and effects review

Native footage and stills are in the checkout's flat `captures/` directory. The requested new
Pi folder is `/home/saffari/Downloads/Panzerrechnersystem_Canyon_Extended_2026-09-30/`.
This is owner-private rendered-media delivery, not an asset/library or game-binary transfer.

## Actual attempts

Seed **137** reached a maximum of about 65% of the 4.93 km route, fired five rounds, completed
five physical manual reloads, destroyed three T-34s and two 45 mm guns, and stopped after
17:15 simulated time because the rule-based pilot could not make further progress. Eleven
T-34s had been admitted by the adaptive director; eight remained alive. The Tiger was alive,
with no broken modules. This is a navigation failure, not a completed level or a Tiger knockout.

The second probe, seed **211**, stopped at 13:52 near 60% progress, with one T-34 and two guns
destroyed; nine T-34s had been admitted. Both probes use ordinary damage and finite ammunition.
The known-route, geometry-assisted AI is an explicit test policy, not human-equivalent perception.
It cannot repair the tank automatically. The level's 60–120 minute duration is still a design
target, not demonstrated completion time.

The extended seed-137 film keeps all five discharges and complete reload cycles, plus cupola
travel and the actual final obstruction. Its cuts are listed in
`captures/canyon_v6_extended137_20260930_cuts.json`. Clean first-person footage has no editorial
captions. The highlights edit adds brief chapter labels and a results card. The separate
103-second T-34 encounter is an uninterrupted portion of that same attempt.

The standalone combat-lighting fixture stages two loaded Tiger shots near a live Soviet AT gun;
it is separate from the autonomous run. The terminal clip stages a catastrophic ammunition
penetration to review the interior-only witness replay. **It is not attached to the surviving
Tiger's run as a fabricated death.**

## Saved Canyon look and effect changes

- The v2 workshop now saves the accepted heavy-storm candidate: fog 0.0046, cloud occupancy
  0.98, storm 0.58, UTC 1943-01-20 13:20. Native resolution is 640×360, with 23 palette steps
  and full ordered/LDR dither. The previous v5 workshop is preserved separately.
- A document option balances the camera's cone channels to diffuse environmental irradiance.
  It removes the shared green/purple cast without making a combat flash drive white balance.
  The filmic shoulder preserves RGB ratios rather than clipping warm light toward white.
- Muzzle lights use a lower, warm reference-emitter calibration. Propellant smoke has a separate
  pale non-emissive material, more/larger grains, a 3.4-second cannon lifetime, forward barrel
  momentum and delayed weak wind entrainment. Black soot remains separate.
- Flame edges are redder; cabin soot capacity doubles and exterior fire smoke allows six times
  as many cubes per source. Current enemy wreck strength grows from penetrating impact energy,
  venting and cook-off state, then fades as its normalized fuel reservoir is consumed.
- Replay rendering no longer erases the fragments' previous positions on a zero-time redraw.
  Yellow cube heads retain real motion trails. The first 120 ms now spans the first 3.2 screen
  seconds, then the replay continues through the 3.2-second aftermath. No damage is reapplied.
- Blood retains 4 mm airborne cubes and 5 mm supported dynamic film cells. Pools increase to
  32,768 grains / 262,144 cells, with 49 film cells per measured contact. The native fatal fixture
  records more than 100,000 retained cells by its final phase, including turret-wall coverage.
- Static aftermath uses 354,822 small cells/drops at the same 753 sites. Connected 14 mm cores
  share buried faces; isolated 6–32 mm cuboids retain the splatter perimeter. Non-blood placements,
  terrain, native heights and the route are preserved exactly. These are authored aftermath props.

## Verification and remaining boundaries

Release compilation succeeded. Native host unit tests: **1430 passed, 54 ignored**. Separate
bloom, quantization, exposure ABI and pixel-particle contracts passed (20 checks). The wider
physical-truth integration group has 17 passes and one existing failure: its broad `glass_pipe`
substring guard also matches the baseline `raster_glass_pipe` names. No exception was added to
that gate. `cargo fmt --all -- --check` also reports extensive existing formatting debt beginning
in unchanged `ballistics.rs`; a whole-tree formatting rewrite is outside this visual pass.
`git diff --check` passes.

Native captures exercise the actual game shaders and actual first-person camera/reload path.
The separate physical High gate also compiled the hardware exact-surface/volume pipelines
on the RTX 4090, converged its eye meter, and wrote a visually checked frame; that non-Canyon
diagnostic is kept locally and omitted from the media gallery.
This does not replace a human interactive playtest. Review uses decoded frames, receipts,
complete media decoding, matched audio/video lengths and Pi SHA-256 verification. No audio
audition is claimed. Aerials deliberately reduce fog to inspect layout; ground/player captures
retain the authored storm. The final source and media hashes are in the capture receipts.

The hot interior is a new candidate: early fragment light remains brighter than the previously
liked late terminal frame. Preserve that older clip as a comparison. The retired synthetic
penetrator beam has not been re-enabled; this pass restores trails of recorded fragment bodies.
See `PHYSICAL_TRUTH_AUDIT.md` for the exact presentation and backend limitations.
