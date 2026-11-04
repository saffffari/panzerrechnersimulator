# Live playground runtime

The range is a live playground. FIRE (F or Enter) launches into the current damaged
vehicle and resumes. Space pauses/resumes. RESET (Backspace) restores a fresh tank,
paused, preserving shot settings and camera. REC/R explicitly starts/stops MP4
recording. There is no automatic simulation cache or image cache in this path.

Interactive resolution uses the existing damage models with less spatial detail:

- crew cells aggregate 3 x 3 x 3 source voxels, summing mass;
- armour patches have at most 1,500 particles;
- projectiles have three cells across their calibre;
- fragment showers aggregate to at most 48 representative trajectories, preserving steel
  volume and kinetic energy while approximating angular distribution;
- blood uses fewer 20 mm droplets and a 10 mm film grid; film area capacity and growth
  area per second remain unchanged;
- crew integration runs on a worker, serial within the job, with at most 512
  integration steps. Extreme review spring stiffness is reduced to maintain a
  stable step over the same physical interval; this is an explicit approximation;
- unfinished plate and tissue workers are polled; the live scene does not join
  them merely because a fixed commit time has elapsed.

KESSEL_TISSUE_SOLVE=reference retains the original fine cells, projectile
resolution, plate budget and unrestricted material timestep for offline tools.
This option is not a live performance preset.

The lower-resolution solver still computes terminal plate geometry, fragments,
crew and component damage. The existing live plate rendering consumes measured
scars. Full per-voxel plate animation from the section-film solver is not yet
connected to the live scene. Do not claim that integration is complete.

Performance must be measured with warmed renderer pipelines and a penetrating
side hit, including crew damage and active effects. The headless live probe
checks live advancement without caches and pause/resume/reset. Its timing
includes GPU readback, so it is a regression witness, not a VSync FPS measurement.
No 60 FPS guarantee is established by a successful build or control test.
