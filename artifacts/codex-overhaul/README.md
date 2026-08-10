# Codex visual-playtest receipt

- `act1-field-hospital.png` — abandoned triage rows, bodies and evacuation debris.
- `act2-old-house.png` — furniture plan without walls and the westbound luggage trail.
- `act3-autobahn-roadblock.png` — the separated prepared blocking line and wreck barrier.
- `act3-campaign-complete.png` — rendered frame from the complete Act 3 verifier.
- `campaign-ending-town-held.png` — reached by the end-to-end campaign run.
- `fire-optic-blast.png` / `fire-side-plume.png` — muzzle-blast obstruction and dissipation checks.

These are generated proof frames, not hand-authored concept art. Rebuild the campaign receipt with:

```sh
cargo run -- campaign-verify /tmp/kessel-proof
```
