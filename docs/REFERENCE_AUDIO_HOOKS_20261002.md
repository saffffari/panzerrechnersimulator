# Reference audio integration hooks

Worktree only: `/home/alex/panzer/reference-pass-20261002`. These hooks are being
implemented by Hermes; root owns the matching Game movement/contact calls.

## Loader movement and ammunition contacts

`audio::LoaderMotion { extraction_mps: f32, ram_mps: f32 }` contains signed actual
motion against the chamber/tray, not held input or a completion flag. Call
`Audio::set_loader_motion(motion, ear_laf_dba, bearing_deg)` every update, including
zero on interruptions, a change of seat, death or no contact. Reference level is
`audio::LOADER_FRICTION_REFERENCE_DBA`; pass it through the existing own-vehicle
ear/headset path once. Friction intensity/cadence follow velocity in the source.
Do not start another CasingExtract/ShellRam scrape after completing the motion:
those legacy cues become short release/seating contacts only.

`audio::CartridgeContact { kind: audio::CartridgeKind::{EmptyCase, LiveRound},
normal_speed_mps, tangential_speed_mps, mass_kg, bearing_deg, seed }` represents
one resolved contact. `audio::cartridge_contact_level_dba(&contact)` returns an
authored at-operator estimate for the ordinary own-vehicle headset path; then
call `Audio::play_cartridge_contact(contact, ear_laf_dba)`.

Replace the `bounced` first-impact-only sound gate with a short per-body cooldown
and minimum impact speed. Do not create contacts for every resting solver tick.
Use actual listener-relative bearing, impact speed, body mass and identity.
The source includes one contact and decaying modes, never fake timed rebounds.
Subsequent resolved contacts own subsequent rings. A very low-energy sliding
contact can use tangential speed; it must cease when the case settles.

## Other integrations

Enemy running-gear API and richer external terminal-source API follow in the
implementation notes as their signatures are frozen. Existing sound propagation,
main-gun and incoming-armor calibration, Mach-cone timing and source-event counts
must remain authoritative. No gameplay gain multiplier or global reverb bypass.
