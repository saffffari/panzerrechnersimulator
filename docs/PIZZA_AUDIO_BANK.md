# Owner-private Pizza Doggy audio bank

Kessel can add a small Pizza Doggy recording layer without making proprietary audio a source or
package dependency. The procedural soundbank remains the audible base for every event. A clean
checkout, an incomplete bank, a stale receipt, a failed hash, an undecodable OGG, or a missing opt-in
all resolve to the same procedural-only runtime.

This is an owner-development workflow, not a public asset bundle. Never commit, publish, attach, or
redistribute the source OGGs, cooked bank, receipt, candidate report, or audition render by itself.

## First reviewed selection

`catalog/pizza_audio_selection.v1.json` is the tracked source-relative closure, reviewed source-byte
fingerprints, and gain/license policy. The first pass promotes only filenames whose physical role is
unambiguous:

| Runtime event | Selected pool | Layer gain | Base |
|---|---:|---:|---|
| `luger_shot` | `old_pistol_fire_ufx_1..3` | 0.14 | procedural Luger always |
| `rifle_shot` | `old_rifle_fire_ufx_1..4` | 0.12 | procedural rifle always |
| `impact_ground` | `explosion_distant_short_ufx_1..2` | 0.07 | procedural ground impact always |
| `kill_explosion` | `explosion_distant_long_ufx_1..2` | 0.08 | procedural destruction always |

The pistol/rifle pools are deterministic round-robin variants with a stable event offset. World
voices are converted to source-domain mono and pass through the same live bearing, obstruction,
reverb, concussion and closed-hull transmission stages as the procedural source.

Button/lever, weapon handling/reload, firearm/object drop, hammer and ricochet groups remain marked
`unresolved-owner-audition-required`. Discovery may list them, but cooking copies only the 11 exact
promoted paths. ROT's Horror Audio Bundle has no selected runtime cue; no melodic, harsh or tonal
loop enters this pass.

Every promoted variant must declare both `expected_bytes` and `expected_sha256`. They are currently
explicitly `null` because the retained owner library is not available in this checkout. That honest
pending state keeps `audit` useful, but `validate-sources`, `cook`, bank validation, and runtime
loading all fail closed until the owner reviews and pins the real values. A filename and a freshly
generated receipt are not treated as proof that source bytes were reviewed.

The Maybach engine/death, turret drive, KwK36 interior/exterior reports, enemy main-gun identity,
incoming-shell/near-miss language, tank-hit and penetration cores, turret pop/rip, ambience, and
shell shriek are protected from Pizza receipt mapping.

## Validate and cook

The private source root must be the retained `_extracted` directory containing the versioned pack
wrappers. The tool only reads it.

```sh
python3 tools/pizza_audio_bank.py audit

KESSEL_PIZZA_AUDIO_ROOT=assets/pizza/_extracted \
  python3 tools/pizza_audio_bank.py fingerprints \
    --output artifacts/audio/pizza-source-fingerprints.private.json

KESSEL_PIZZA_AUDIO_ROOT=assets/pizza/_extracted \
  python3 tools/pizza_audio_bank.py validate-sources

KESSEL_PIZZA_AUDIO_ROOT=assets/pizza/_extracted \
  python3 tools/pizza_audio_bank.py cook \
    --output assets/sounds/pizza_private \
    --ack-license owner-private-game-integration-only
```

`fingerprints` writes a new, owner-private candidate report containing the observed byte count and
SHA-256 for every exact selected path. It never edits or blesses the tracked manifest. Review the
report against the retained purchase/version evidence, then manually copy each accepted pair into
that cue's `expected_bytes` and `expected_sha256` fields in the manifest. Keep the report under the
gitignored `artifacts/audio/` tree (the command refuses any other output location) and never publish
it.

After all fingerprints are reviewed, `cook` requires a new output directory. It resolves every
canonical relative source below the declared root, requires its bytes to match the tracked pins,
copies only that closure to `cues/<cue-id>.ogg`, normalizes file metadata, and writes
`BANK-RECEIPT.json`. It rejects pending or mismatched fingerprints, missing/escaping sources,
symlinks that leave the root, duplicate IDs/sources, unsafe paths, unsupported events, excessive
variants/gains, and any output overwrite.

Validate an existing bank independently:

```sh
python3 tools/pizza_audio_bank.py validate-bank \
  --bank assets/sounds/pizza_private
```

The bank validator re-hashes every OGG and requires its byte count/hash to match both the tracked
selection and the exact receipt-only closure. The Rust runtime binds each receipt file to the same
compiled expected values, compares the receipt's complete license/runtime policy objects to the
compiled manifest, rejects pending fingerprints and extra/missing/symlink files, re-hashes and
decodes every selected OGG, and enables the entire bank atomically. There is no partial licensed
mode.

Runtime use remains an additional explicit decision:

```sh
KESSEL_ENABLE_PIZZA_AUDIO=1 cargo run --release
```

The authoritative Linux runtime payload cook does not discover or copy this bank.
`assets/sounds/pizza_private/` and `artifacts/audio/` are gitignored, and the locked non-GLB runtime
payload contains no private audio path. The same exclusion remains true of archived Mac packages.

## Private audition

Discovery-only candidate filenames can be reviewed without changing the selected closure:

```sh
KESSEL_PIZZA_AUDIO_ROOT=assets/pizza/_extracted \
  python3 tools/pizza_audio_bank.py candidates \
    --output artifacts/audio/pizza-candidates.private.json
```

After a real bank is cooked, generate a short gain-matched WAV and labeled CSV (requires `ffmpeg`
and `ffprobe`):

```sh
python3 tools/pizza_audio_bank.py audition \
  --bank assets/sounds/pizza_private \
  --output artifacts/audio/pizza-private-audition
```

The audition output is owner-private. Promote a candidate only after listening, then add its exact
source-relative path and conservative layer gain to the tracked manifest in the same reviewed
change. Never make candidate search terms an implicit cook glob.

## Regression checks

```sh
python3 tools/test_pizza_audio_bank.py
cargo test audio::tests
```

The suites cover no-source checkouts, pending-fingerprint refusal, a non-mutating candidate
fingerprint report, an exact pinned 11-cue fixture cook while unresolved discovery groups remain
present, source/receipt traversal, symlink escape, closure drift, source and receipt hash/size pins,
full receipt policy binding, package exclusion, gain bounds, deterministic variants, protected
identities, resampling, and the procedural fallback bank.
