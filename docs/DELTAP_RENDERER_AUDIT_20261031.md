# DeltaP renderer source comparison, 2026-10-31

Read-only audit of the newer **`/home/alex/deltaP/.claude/worktrees/v1-steps`** checkout, not the stale top-level checkout. The source was read from `/dev/sdc2`, Btrfs top-level subvolume, mounted with `ro,nologreplay,subvolid=5` at `/tmp/deltap-readonly`; the full mounted source path was `/tmp/deltap-readonly/@home/alex/deltaP/.claude/worktrees/v1-steps`. `findmnt` confirmed `ro` and `rescue=nologreplay`. No DeltaP source, assets, game code or disk contents were modified. No executable, compilation or GPU benchmark was run.

The inspected `src/main.rs` and `shaders/march.wgsl` were last modified on October 9. Conclusions below come from the executed source paths, not their older comments: for example, the main render comment still says 160×240, but the active constants are **1024×576**.

## What it actually does

| Source, relative to the checkout above | Executed behavior | Consequence for this comparison |
| --- | --- | --- |
| `src/main.rs:50`, `:252`, `:351`, `:592` | 1024×576 main HDR image; one additional 160×120 bow-camera image; fullscreen triangle for each march; single-sample pipeline; bloom then display/crush. | This is not Panzer's four separately rasterized optical exteriors plus the main scene, nor its current 4×MSAA configuration. No measured DeltaP frame rate is established by this audit. |
| `shaders/march.wgsl:184`, `:221`, `:2014` | One shared world-distance query; up to 200 steps per segment, up to three segments. A cabin ray meeting glass continues outside and may queue one reflection. Periscope mode begins with the eye outside. | Exterior work follows the actual pixels that pass through glass. This is the most directly relevant architectural difference from full-image capture per visible Tiger optic. The segment limit and reflection cutoffs are explicit approximations. |
| `shaders/march.wgsl:239`, `:345`, `:2080`, `:3294` | One call site for each heavy march, normal and shading function. Normal uses four taps; AO uses eight. Repeated operations loop through one call site; uniform-only transforms are computed before the march. | It deliberately avoids shader expansion from duplicated large world-query call sites. This is worth auditing in Panzer's shader generation, but does not establish that Panzer has this compiler problem. |
| `shaders/models.wgsl:90`, `shaders/kitbash.wgsl:169`, `shaders/city.wgsl:89` | Prop-box lower bounds avoid eight texture taps away from props. Sparse SDF bricks use a spatial grid with at most eight listed instances per cell. City geometry uses bounding boxes and a max-height pyramid, descending only near possible surfaces. | Broadphase guards and data locality matter more than the label “raymarcher.” These representations are not equivalent to Panzer's independently movable, damaged source triangles. Imported SDF slabs visibly thicken thin surfaces; they cannot silently replace mechanical contact geometry. |
| `src/weather/atmosphere.rs:494`, `src/weather/sky.rs:239`, `:257`, `:330` | A 32×32 CPU sky table is cached. It rebuilds when sun direction changes by 0.0006, moon by 0.004, selected atmospheric values by 0.5%, or cloud terms by 0.003. The multiple-scattering table has separate 1% haze thresholds. | Reuse is tied to the inputs that change radiance. Panzer already has a raymarch sky cache, while its raster optical relays run their separate sky path. Sharing physically compatible atmosphere work is a promising investigation, not an approved change or a free drop-in cache. |
| `shaders/weather.wgsl:737`, `:783`, `:1113` | Visible clouds still perform substantial work: 40 cheap search slices, five entry bisections, seven skin steps per entry, at most three entries; light evaluation uses three coarse density taps. Stops at transmittance below 0.02. | DeltaP is not simply rendering a flat cheap sky. Its bounded sampling and cutoff error must be considered when comparing quality. |
| `shaders/gloss.wgsl:320`, `:353`; `shaders/march.wgsl:2385` | GGX-style direct highlights and Fresnel; one sharp reflection when roughness is below 0.32 and reflected weight exceeds 0.01. Untraced reflection uses an analytic cabin environment. | Materials are more elaborate than flat colors, but this is a tightly bounded hybrid. It is not a general multi-bounce physical light solve. |

## Shortcuts that should not be copied as physical-truth optimizations

The actual lighting code has deliberate artistic approximations:

- `shaders/boat.wgsl:1949`: only the nearest of six cabin lamps receives a shadow march; other lamps use compartment membership. Screen and door lights use AO instead of source occlusion. Some console materials skip the nearest lamp's shadow.
- `shaders/boat.wgsl:1832`, `:2063`, `:2092`: screen light gain is 3; the viewport has an added cool rim; a constant ambient floor keeps deep corners visible.
- `shaders/march.wgsl:428`, `:458`: lamp/screen haze gains are 3; closed-form lamp scatter is unshadowed inside coarse compartment slabs.
- `shaders/gloss.wgsl:339`: the rough cabin reflection is a few analytic blue/amber/dark directional lobes, not a visibility-tested scene capture.
- `src/weather/sky.rs:71`, `:412`: displayed sky radiance uses a gain of 6; the physical light-meter calculation is separately ungained. `shaders/weather.wgsl:1130` also limits the displayed sun-disc radiance.
- `shaders/weather.wgsl:878`: sea reflections use a coverage-plane cloud approximation instead of the visible cloud integration.

These choices can be good art direction for DeltaP. They are not equivalent to retaining all of Panzer's existing light paths, source occlusion and optical geometry.

## Applicable direction, with no extra candidate implemented

1. **Finish the same-build zero-throughput A/B first.** The current bounded Panzer candidate removes only work whose final radiance multiplier is exactly zero. It retains the physical entrance. DeltaP does not provide a reason to introduce a nonzero cutoff.
2. **Treat visible optical pixels as the eventual unit of work.** A future Tiger optical path could trace source-correct folded rays through the actual entrance/objective geometry, rather than capturing a rectangular exterior image per port. This would be an architectural feature with its own material, transparency, source-occlusion and AA validation—not a small shader replacement.
3. **Audit atmosphere duplication separately from geometry.** Panzer's b36 commander profile spent 8.096 ms/frame on four raster optical sky passes, alongside its separate 1.660 ms raymarch sky cache. A shared cache needs explicit objective-origin, atmosphere, time and directional-resolution validity; nearby spatial weather must not be silently reused across different origins.
4. **Keep safe bounds and shared computations, not artistic light omissions.** Source-conservative culling, existing pixel-footprint budgets, uniform-hoisted work and compiler inspection can preserve the game's physical rules. Replacing shadows with compartment gates or adding unoccluded fills cannot.

The audited DeltaP code supports these engineering directions. It does **not** prove that 1024×576 Ultra Panzer can reach 60 FPS, or that a wholesale SDF conversion would be faster at equal detail.

## Source fingerprints

| DeltaP source | SHA-256 |
| --- | --- |
| `src/main.rs` | `96743400d58c99e2a18f7b5655f108e5efd06117e35649123edb4226201a54bd` |
| `shaders/march.wgsl` | `1534d3d21a5ab65517acedc283d62a709e80bc6ff87335521c710f08dc350778` |
| `shaders/boat.wgsl` | `eea28083f7cf3fd14862342cf1b39d77086812aa4c4b9dfb85f8ad0cc66c4cc9` |
| `shaders/weather.wgsl` | `f6fdc746dc188a0482971a616d4f76593fe95ba9d76cc626afda4325ca73fd58` |
| `shaders/gloss.wgsl` | `49bc31fa19b9832e647ae36c12bc82a3330985ae22f36fd22996b845a7c37b1d` |
| `src/weather/sky.rs` | `dc3703cd270edfa8cb0cfdc88af31412391d58b730fd696ad811e58645caa9c1` |
| `src/weather/atmosphere.rs` | `11696245098d7488fe2ee3d228c33d561b392959142bd4200a0046396f70d3e1` |
