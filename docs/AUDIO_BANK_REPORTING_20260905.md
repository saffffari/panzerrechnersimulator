# Audio bank status reporting — 5 September 2026

This source-only pass corrects startup and capture descriptions. It is **not in the frozen v10.1 executable**. No waveform, event dispatch, gain, loop, control, catalog recipe or bank file changed. Cargo and GPU work remain owned by the main task.

The former blanket “procedural-only” message is replaced by load-specific messages and a summary of verified feedback recordings, resolved direct Sfx replacements, legacy reference voices, generated voices and verified optional v1 layers. Console summaries contain cue counts and status, without source-pack paths.

`Audio::capture_metadata()` preserves every existing timing/mixer field, updates the explanatory `clips` string, and adds **`audio_bank`**, an immutable snapshot created after loading. It does not reread environment variables or files when a capture requests the report.

| Field under `audio_bank` | Meaning |
|---|---|
| `schema`, `version`, `scope` | `kessel.audio-bank-status`, v1; Audio bank only, with scene emitter files owned by scene manifests. |
| `feedback.enabled_at_load`, `status` | Actual enable state and `disabled`, `unavailable`, `partial` or `loaded`. |
| `feedback.configured_count`, `loaded_count`, `not_loaded_keys` | Separates configured recipes from successfully verified/decoded/prepared sources. |
| `feedback.loaded[]` | Cue key, verified original `source_sha256`/`source_bytes`, `source_verified_at_load`, prepared mono frame count/sample rate, and checksum of prepared PCM. |
| `feedback.selection_manifest_sha256` | Identifies the exact compiled recipe manifest, including transformations. |
| `feedback.credit`, `license_id`, `owner_private`, `review` | Existing Pizza Doggy catalog attribution and private-bank review status; no new licensing claim. |
| `sfx_routes[]` | Each identity’s resolved `direct_source`, `world_source`, `feedback_key`, and v1 variant counts available versus active. A loaded but bypassed layer is not reported as active. |
| `loops[]`, `controls[]` | Actual loaded reference/generated loop sources and feedback/generated controls. |
| `legacy_references` | Requested/allowed state and actually loaded direct-reference identities. Explicitly states that these older decoded references lack source-hash verification. |
| `v1_layers` | Requested state, verified loaded count/status, selection ID/hash, private-bank designation and bypass policy. |
| `procedural_families` | Generated fallback, mechanism, armor/pressure/fragment/concussion and caliber-report families still present. |

The direct/world distinction is intentional: without a feedback replacement, direct playback may use an explicitly loaded legacy reference while world propagation uses its generated dry source. Tank MG retains its dedicated preserved identity. This pass reports those existing differences; it does not change them.

Four focused tests were added:

- `audio::feedback::tests::feedback_metadata_reports_disabled_missing_partial_and_loaded_without_source_paths`
- `audio::feedback::tests::bank_metadata_reports_recorded_rifle_bypass_and_preserved_tank_mg_layer`
- `audio::metadata::tests::bank_metadata_distinguishes_legacy_direct_sources_from_generated_world_sources`
- `audio::tests::capture_metadata_reports_load_snapshot_without_advancing_audio`

The report tests use local fixtures to exercise status and routing branches without changing process environment or files. They check source/recipe hashes, deterministic read-only reporting, no source paths, replacement bypass, legacy/world differences and an unchanged capture clock/sink count. They have not been run in this subtask. Rust syntax parsing and whitespace checks pass; the main task must run the compile/test gate before using this in a new binary.

The older optional v1 bank had a separate closure conflict: its receipt expects 12 files but the newer nested bank adds nine more. The reporting-only pass reports that as `not_loaded` when v1 is requested. The later [verified-partition pass](../artifacts/astra-audit-20260905/audio-partition-verification-20260905.json), after frozen v10.2, addresses that conflict without moving assets or changing cue selection.
