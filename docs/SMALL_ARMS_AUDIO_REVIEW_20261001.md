# Small-arms source and timing audit — 2026-10-01

The real projectile sweep runs at240Hz. Rifle muzzle reports already used the active atmospheric sound speed, and passing projectiles already emitted a single Whitham/Mach-cone N-wave from their actual swept segment. Calibres below20mm do not receive the artillery shell whistle. Actual armor contact already has a short seeded metal tick and a distinct structure-borne route for a hit on the listener's tank.

The old fallback reused the same92Hz/noise rifle voice for tank MG reports. The verified private rifle recording is1.332s long, with about30ms leading silence and a long decay. It then received the live environment reverb. The source asset is unchanged and still admitted only by the existing exact SHA256/byte checks.

`audio_small_arms.rs` now prepares12 deterministic variants for each rifle and MG report. It removes the recording's leading silence, retains at most110ms of its early rifle report or60ms for the MG family, then tapers it. A short causal pressure/modal source supplies the redistributable fallback and a restrained component of the owned-recording mix. Early recorded reflections may remain: this is a crop, not de-reverberation. No long noise bed, artificial cannon bass or embedded room tail is added. The dry outputs last145ms and90ms respectively; ordinary world propagation supplies one live environment tail. The existing physical emission levels, hull/headset transfer and reproduction curve remain authoritative.

Generic EnemyGun, ground/armor impact, penetration, kill, turret pop/rip and cook-off sources now cycle four actual source-synthesis seeds. Variant0 is exactly the previous dry source. No time resampling or duplicate event is involved. Each distinct prepared source has its own calibration-cache fingerprint; equal-length variants cannot reuse the wrong source measurement.

The new65ms soft-body contact is distinct from steel and soil. Its109dBpeak/83dBA reference at1m is explicitly an authored, unsourced estimate. It is scheduled only for an actual terminal body contact. No gore recording or metallic ring is layered into it.

Capture audio metadata records each prepared source checksum, recipe, variant count and the verified upstream recording key. Optional old direct-play character layers remain independent; these world combat banks bypass them.

Root owns the gameplay scheduling corrections: replace the old fixed13Hz own-MG sustained loop with actual launch reports, avoid the universal1.2m source displacement/cover escape, use the current receiver against expanding acoustic wavefronts, and receipt source/arrival events. These are separate from source timbre changes. The MG34 outside table includes a burst LAeq; per-round launch playback must use LAE plus the LAF conversion, not apply that burst value to every round.

## Verification boundary

The source/variation checks pass4/4, calibration/routes/single-live-tail checks6/6, physical-level checks9/9, propagation9/9 and listener/Mach-cone checks5/5. The final endpoint of the body contact was explicitly zeroed after a test observed a2.97e-16 residual; that final fix is included in the next integrated test checkpoint. Logs are under `artifacts/runtime/small_arms_audio_*_20261001.*`. These are deterministic source/timing/calibration checks, not subjective listening acceptance. Native gameplay review remains required.
