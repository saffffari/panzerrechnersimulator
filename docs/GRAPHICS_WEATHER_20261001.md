# Graphics, weather and effects review — October 1

Open **Graphics** from the title menu or **Esc → Settings → Graphics**. Arrows adjust a row;
Tab changes page; mouse clicks on the left/right side of a value decrement/increment it.
Changes preview with simulation paused. **Save preferences** writes the versioned local config
at `$XDG_CONFIG_HOME/panzerrechnersystem/graphics.v1.json` (or `$HOME/.config/…`).
`KESSEL_GRAPHICS_CONFIG` overrides its location. Back keeps session changes without saving.

Pages: Display (render/window resolution, fullscreen, VSync), Quality (shadow-map resolution,
transport sampling, optic update budget, FPS), Color (exposure, saturation, warmth, gamma,
ambient white balance), Retro (palette/dither), Time of day (authored/sunrise/day/sunset/night,
clock speed), Weather (authored/clear through **8×** storm intensity).

Display sampling does not alter world geometry, emitters or physics. Time of day and Weather
are explicitly world-condition overrides, not hardware quality settings. Returning either to
Authored restores the campaign state. Severe weather couples snow, suspended-snow extinction,
cloud coverage and wind, with slow fronts, shorter gusts and advected 3D density banks. It is an
art-directed atmospheric model, not a fluid simulation. Maximum discrete snow capacity rises
from 8,192 to 65,536 particles; increasing density preserves existing particle identities.

**L** toggles the handheld flashlight when alive, with a free hand and outside an optic. It uses
a short warm cone and geometry shadows; equipping another object stows it. It currently runs in
the shipping Canyon shader backend. It does not brighten the rest of the night. Dawn/dusk use
the celestial clock and shared atmosphere. The clear night field contains deterministic
illustrative stars, rotating with sidereal time and attenuated by cloud/air, not real constellations.

Viewport bloom now transports the aperture's RGB through its halo instead of substituting white.
Weapon color shifts from the old red energy tiers to cream/yellow/amber. Shell trails continue to
read the authoritative projectile. Impact flash, long ballistic cosmetic ejecta and lingering
smoke are separate components; cosmetic sparks do not decide damage. Cannon ricochets retain
outward reflected directions. Hot sparks travel well beyond a vehicle footprint over their life.

Exterior wreck, muzzle and explosion smoke use bounded, ray-integrated, soft 3D density supports
with coherent erosion, wind and distinct soot/propellant scattering. Exterior soot cube pages are
retired. Cabin soot retains its source-mesh clipping and sparse particle path, now with rounded
silhouettes; converting that cavity-constrained smoke to a density field remains separate work.
The 12-support volume pool admits up to three nearby sustained emitters and remaining transient
billows. Distant smoke admission is a bounded renderer limitation. The multiple-scattering cloud
closure remains column based; its lighting is now integrated through the actual 3D cloud density
rather than added at the cloud-base sphere. This is not full 3D multiple scattering or path tracing.

Existing seasonal groves mix both pine variants with four bare/broken tree forms, retaining the
same placement count and road/spawn exclusions. No new private asset package is distributed.
The old red-armband CC0 German model remains the infantry asset; the tank crew uses the earlier
AdrielCZ Panzertruppe model. Canyon has no allied infantry, which is why the former is absent there.

Validation artifacts live in `captures/` and `artifacts/runtime/`; see [the final review receipt](../catalog/graphics_weather_effects_20261001.json).
The impact review stages an emitter beside the source Tiger to compare scale; it is not a combat
victory or a recreation of the supplied video. The video remains an external visual reference.

Final verification: 1,444 unit tests and 21 focused integration checks passed, plus native GPU
resize/effect reviews. The six-second MP4 fully decoded and its contact sheet was visually reviewed.
The desktop title rendered the Graphics entry; synthetic X11 navigation did not establish an
end-to-end native input pass, so that is not claimed. Two older integration suites contain three
pre-existing source-string failures (recorded in the receipt), outside the passing focused checks.
