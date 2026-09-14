# Shader Playtest checkpoint — 14 September 2026

Branch: `codex/shader-graphics-rebuild`.

## Included in this checkpoint

- Loading targets the visible breech mouth, uses a wider capture cone and reach, and retains
  alignment once insertion starts. Green brackets, loading text and a progress bar show the target.
  Holding LMB seats the round; releasing still drops it. Closed/occupied chamber gates remain.
- The physical paper map shows the current player position (including on foot), reveals the first
  search area immediately, and reveals later areas as map sheets are recovered. Search areas remain
  approximate. Their red labels are larger for the lower internal resolution.
- Engaged enemy platoons stop updating their last-known location when sight is lost. Alerted tanks
  search that remembered location; firing requires an individual sighting and clear line of sight.
- Airborne blood has a bounded screen-size floor so the existing 4 mm grains are easier to see.
  Grain counts, contact physics and damage decisions are unchanged.
- Prior work is preserved: shader bloom/dither and winter grass, driver controls and optics,
  held equipment/round collision fixes, display previews, weapon effects and recorded KIA replay.
  The launcher's default internal resolution is now 576×324.

## Validation

- Release build succeeds; 1,033 game tests pass, 10 explicitly ignored tests remain ignored.
- Seven shader/bloom tests pass. Launcher shell syntax and Git whitespace checks pass.
- `reload_guided_20260914`: all 13 runtime checks pass at 576×324. The ordinary held-mouse sequence
  fires, extracts and drops a case, picks up and seats one round, then fires again. The loading-guide
  frame was visually inspected. Fixture head turns are scripted; this is not a human playtest.
- `map_guided_20260914`: all nine runtime checks pass. The rendered paper and true-position marker
  were inspected; `map_guided_final_20260914` also passes all nine checks and its larger red search
  label was visually verified.
- `blood_readability_20260914`: recorded fatal-hit replay passes without changing crew outcomes;
  interior frames inspected. This does not establish that every live wound is easy to identify.
- Capture media and detailed receipts remain local in `captures/astra/`; logs and binaries are
  excluded from this source checkpoint.

## Outstanding requested work

The first objective's hill, additional bushes and ruined-building cover, explicit delayed infantry
reporting after surviving an attack, and the expanded exterior/interior penetration/explosion
presentation are not complete in this checkpoint. Live wound identification and wall blood still
need a dedicated visual pass. The current AI change fixes tank contact tracking, not every aspect
of encounter balance. Keep these requests active; this checkpoint is not campaign completion.
