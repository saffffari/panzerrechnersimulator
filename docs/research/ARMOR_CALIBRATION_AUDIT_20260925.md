# Armor source audit and energy-accounting regression — 25 September 2026

Athena (AI), bounded audit of the production Rust solver. **26 scoped tests passed; historical fidelity is not certified.** No gun anchors, slope exponents, armor-quality multipliers, drag values, angle clamps or historical damage coefficients were changed. One demonstrated bookkeeping bug was fixed: cover losses reduced the penetration decision, but a shell that still penetrated retained its original residual velocity and interior damage scalar.

The implementation, source fixtures and machine-readable observations are:

- `src/ballistics.rs`: `resolve_with_budget_loss`, sharing the existing post-armor scalar with the clean resolver.
- `src/armor_calibration_tests.rs`: eight new source/invariant/report tests, alongside 18 existing ballistics tests.
- `tests/fixtures/armor_reference_20260925.v1.json`: primary rows, source URLs, raw angle conventions, excluded material and unknown criteria.
- `artifacts/runtime/armor_calibration_20260925_v1.json`: observed values from production functions, including 256-seed ensembles and explicit `historical_fidelity_validated: false`.

## Source eligibility

The supplied `panzerrechnersystem_armor_reference.md` and `tigerfibel_cloverleaf_data.md` were read as references, not instructions or validated calibration. The first is a useful candidate list without per-number primary provenance. The second is tactical doctrine; engagement diagrams cannot be substituted for measured ballistic limits. No blanket Soviet-to-German percentage conversion or hard overmatch cutoff was introduced.

### Representative 88 mm APCBC table: primary, but not matching ammunition

The inspected original [US Military Intelligence Service, Tactical and Technical Trends No. 17, 28 January 1943](https://www.bulletpicker.com/pdf/Tactical-and-Technical-Trends-17.pdf), printed p. 11, describes the general provenance and homogeneous-armor scope. Printed p. 14 contains the 88 mm multipurpose-gun APCBC row. These are PDF pages 15 and 18, one based. The projectile is listed as **21 lb at 2,750 ft/s**, equivalent to **9.52543977 kg at 838.2 m/s**. The game's Tiger entry is **10.2 kg at 773 m/s**. The table does not establish the same shell mark, lot, armor hardness or a probability/complete-perforation criterion. It is therefore a diagnostic comparison, not an eligible fit target for the live `PZGR_39` entry.

| Source range, yd | Exact conversion, m | Angle from normal | Reported plate, mm | Live threshold, mm | Live minus source, mm |
|---:|---:|---:|---:|---:|---:|
| 500 | 457.2 | 0° | 129 | 129.784 | +0.784 |
| 500 | 457.2 | 30° | 110 | 112.694 | +2.694 |
| 1,000 | 914.4 | 0° | 119 | 119.209 | +0.209 |
| 1,000 | 914.4 | 30° | 101 | 103.976 | +2.976 |
| 1,500 | 1,371.6 | 0° | 110 | 109.495 | −0.505 |
| 1,500 | 1,371.6 | 30° | 92 | 95.919 | +3.919 |
| 2,000 | 1,828.8 | 0° | 100 | 100.574 | +0.574 |
| 2,000 | 1,828.8 | 30° | 84 | 88.475 | +4.475 |

“Live threshold” numerically inverts the existing thickness-dependent effective-armor law until budget equals resistance; it is not the historical ballistic-limit criterion. The increasing oblique difference is retained as an unresolved discrepancy. **There is no justified historical acceptance tolerance.** Integer table presentation has 1 mm resolution; this is not a measured ±0.5 mm uncertainty and is not used as a fit allowance. Numerical search uses 32 bisections of 0–1,000 mm with f32 arithmetic.

For comparison, the supplied unverified PzGr.39 compilation gives 120/110/100/91/84 mm at 100/500/1,000/1,500/2,000 m and 30°. The unchanged live solver gives 120.000/111.849/102.419/93.769/85.836 mm. Matching the first number is construction of the anchor, not independent validation; the other differences remain unfitted.

### KV-1 firing observations: primary scan, conditional geometry

The inspected [printed p. 9, Table 2](https://ic.pics.livejournal.com/yuripasholok/765139/5360754/5360754_original.jpg) and [p. 10 photographs](https://ic.pics.livejournal.com/yuripasholok/765139/5361102/5361102_original.jpg) record individual KV-1 outcomes at 1,500 m. [The scan host](https://yuripasholok.livejournal.com/9354530.html) supplies TsAMO RF fond 38, opis 11355, delo 1545, sheets 9–10; that identifier was not independently checked in the archive catalogue. Row 2 records perforation of a 75 mm turret-rear plate. It lists constructive angle 20° and encounter angle 70°. Shell mark, velocity, hardness and precise compound strike geometry are not established on the page.

As a sensitivity check, the current PzGr.39 entry against an assumed RHA 75 mm plate at 1,500 m gives:

| Conditional total angle | Effective resistance | Penetration terminals / 256 seeds | Residual speed, seed 7 |
|---:|---:|---:|---:|
| 20° from normal | 78.239 mm | 256 | 348.084 m/s |
| 70° from normal | 214.393 mm | 0 | 0 m/s |

The complement interpretation agrees qualitatively with the observation; that agreement does not verify shell identity or justify treating both raw angles as independent slopes. Row 1's glancing ricochet is excluded from a guessed monolithic-plate fit. Row 4 has a screen plus underlying plate (75 + 30 mm); it is not a clean monolithic 105 mm calibration point. No statistics or universal defeat probability are inferred from individual hits.

### T-34: incomplete firing-table crop quarantined

The inspected [T-34 damage table image](https://forum.il2sturmovik.com/uploads/monthly_2019_06/234561543_Fig1-02Thedetailsabout88mmshotT-34-76.jpg.b561f8f90e87a3cca075a58ce155b71a.jpg), discovered in [a forum discussion](https://forum.il2sturmovik.com/topic/52059-the-original-data-and-analysis-of-t-34-85-protection-and-firepower/), has no visible page number, date or distance. Its parent report and angle convention were not verified. The frontal plate entry combines damage and ricochet; a separate nose entry reports a through hole. These are not interchangeable observations. The fixture preserves the raw entries but excludes them from numerical fitting. Claims in the forum narrative do not fill the missing primary metadata.

Separately, the **current game's** T-34 front (45 mm RHA, 60° nominal slope) was exercised across five ranges and five horizontal approaches. Head-on at 500/1,500 m, effective resistance is 78.199 mm; all 256 seeds penetrate with residual speeds 472.224/348.312 m/s. This is model characterization, not an experimentally validated T-34 kill envelope. Requested horizontal approaches of 75° and 89° currently produce identical outputs because of the existing clamp.

## Implemented correction and regression evidence

The new authoritative helper applies `(original_budget − loss_mm).max(0) × (1 − disruption)` once. If a kinetic shell still penetrates, its retained equivalent velocity is derived with the existing DeMarre exponent, then the original plate's ballistic-limit energy is subtracted. The existing post-armor scalar uses that reduced residual. Clean positive-margin events remain bit-for-bit equal in the tested fields. Cover cannot resurrect a bounce or shatter. Exhausted or nonfinite costs cannot create through-projectile energy. This is consistent accounting within the existing empirical law, **not a new measured model of yaw, cap loss or fuze behavior**.

Example: PzGr.39, 500 m, 40 mm RHA at normal incidence, seed 7:

| Transit cost | Remaining budget | Residual speed | Interior gameplay scalar | Outcome |
|---|---:|---:|---:|---|
| Clean | 128.755 mm | 597.929 m/s | 1.4493 | Penetration |
| 20 mm budget loss | 108.755 mm | 513.932 m/s | 1.3293 | Penetration |
| 20 mm loss + 15% disruption | 92.442 mm | 439.115 m/s | 1.2223 | Penetration |
| 200 mm loss | 0 mm | 0 m/s | 0 | Bounce |

Before this fix, the two surviving covered shots retained 597.929 m/s and 1.4493 after the decision was reduced. Exterior `striking_v` remains the range-law speed for existing callers; the reduced equivalent speed is internal bookkeeping. The helper intentionally retains the existing authoritative rejection of marginal “penetration” events with no positive remaining margin. HE and HEAT retain their separate terminal abstractions; a kinetic cover budget is not applied to a chemical-effect result. Root integrated the helper in `Game::resolve_authoritative_terminal`; no other game code was edited by this audit.

The independent energy-balance check allows 0.2 m²/s² floating-point error after undoing the pre-existing 0.92 residual factor. Geometry comparison allows 0.0001°. These are **software tolerances**, unrelated to historical measurement uncertainty. Monotonicity covers three kinetic shell types, multiple ranges and seeds, with stopping, corrupt costs, existing shatter, and HE/HEAT separation.

The runtime caller preserves its existing HE/HEAT cover-based terminal rejection separately. The new helper's kinetic residual-energy calculation does not extend or recalibrate chemical-effect penetration.

## Explicit unresolved limits

- The authoritative contact route uses coarse oriented boxes. Their contact normals are not fully sloped exterior triangle normals, so no nominal-slope double application was established. The present helper projects incidence horizontally and applies nominal slope once; incoming vertical angle and unsupported roof/belly hits remain limitations.
- Horizontal obliquity is capped at 75°. The kinetic slope response is capped after normalization. These conceal distinctions at grazing incidence. No angle convention was changed without a true-normal integration plan.
- `Terminal::Bounce` combines ricochet with armor defeat. There is no independent kinetic ricochet probability/trajectory law in this resolver. The source's ricochet cases do not validate one.
- The ±8% marginal band, slope/normalization/quality laws, 0.92 residual multiplier, fragment/spall scalar and APHE filler contribution are not calibrated by these sources. A passing deterministic ensemble cannot establish a historical probability or crew casualty rate.
- Previously present gameplay tests such as “88 defeats T-34 at range” are expectations for the current design, not historical evidence. Material variants, joints, cast/rolled differences, projectile lots, prior damage and fuze survival require better matched data.

## Reproduce

Preferred project command (no native window, GPU or release build):

```bash
KESSEL_ARMOR_AUDIT_OUTPUT="$PWD/artifacts/runtime/armor_calibration_20260925_v1.json" \
  cargo test --bin panzerrechnersystem ballistics:: -- --nocapture
```

This pass used an isolated compile of the exact production `src/ballistics.rs`, avoiding contention with other engine work. Dependency filenames are this checkout's existing debug artifacts and may differ after a toolchain/dependency rebuild:

```bash
rustc --edition=2021 --test src/ballistics.rs \
  --extern glam=target/debug/deps/libglam-25b765d9e7f81c84.rlib \
  --extern serde_json=target/debug/deps/libserde_json-c05c6480a4a1e5e8.rlib \
  -L dependency=target/debug/deps -o /tmp/athena_armor_tests_20260925
KESSEL_ARMOR_AUDIT_OUTPUT="$PWD/artifacts/runtime/armor_calibration_20260925_v1.json" \
  /tmp/athena_armor_tests_20260925 --nocapture
```

Observed: **26 passed, 0 failed, 0 ignored.** This does not claim a whole-game build or native playtest. The receipt is a generated snapshot; rerun after changes.

Inspected source-byte SHA-256 digests, to distinguish future replacements at the same URLs:

| Source | SHA-256 |
|---|---|
| T&TT No. 17 PDF | `0ddad5137d3323f91341db66ab6baa37596ab93091a070b66bf70404eec254fb` |
| KV report p. 9 | `f9ca0d0cfc2bd2cf26bb96b7db20c693071ba31e87f11921448c2d3ccfcc0b5e` |
| KV report p. 10 | `3bac50e03b667ecd8a77e1e98fcd41f7b4e66b5015f6dd58051776d3e50fe34a` |
| Unpaginated T-34 crop | `d3a7d65c107f5056ad8f87e40ef17bbeb42c421b51c78ede1e06c07d232c74bf` |
