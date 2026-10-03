# Observer transition correction

Looking toward a stationary bright port could satisfy the old flash detector: its
HDR field rose rapidly and the exterior exceeded the dark-adapted threshold. That
incorrectly created a weapons-flash afterimage. The 100 ms upward cone response also
changed ordinary exposure too abruptly during a glance.

The detector now requires an actual transient emitter in the game frame. Main and
hand-weapon flashes, blasts and incandescent impact fragments admit the detector;
static lamps, ports and sustained fires do not. Visible HDR radiance, coverage and
temporal rise still supply the flash energy. The flag itself creates no light or
afterimage. Death replay admits its recorded transient emitters in the same way.

Cone display adaptation now uses 1.2 s upward / 2.5 s downward constants, and rods
use 1.8 s / 4 s. These are gameplay reproduction timings, not physiological claims.
The accepted stationary look is preserved: spatial weights remain 50/30/20, while
the current arithmetic HDR meter, display curve, illumination, pupil and pigment
model remain unchanged.

The clock uniform is explicitly eight scalar lanes (32 bytes) in Rust, WGSL and
the actual-GPU fixture. CPU tests cover brief glances at 30/60/120 Hz, static port
arrival, a source behind armor without visible radiance onset, and a visible real
flash. The GPU fixture also exercises those inputs through the production shader.

This is a bounded source-admission fix, not per-pixel source attribution. A camera
turn across a bright opening coincident with an unrelated active flash can still
satisfy the global detector; identifying that mixed case needs image-space source
contribution/history rather than inferring it from brightness alone.

Build and GPU validation are coordinated by the parent task. This document records
the implementation and intended regression coverage, not a claim of native visual
acceptance.
