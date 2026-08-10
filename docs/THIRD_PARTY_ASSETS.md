# Third-party asset release policy

This file records the evidence and packaging policy for third-party runtime assets. It is an
engineering release gate, not a substitute for the original license agreement or legal advice.

## Pizza Doggy packs

Kessel uses selected models from locally purchased Pizza Doggy packs. The purchase archive retained
on the development workstation includes Pizza Doggy's **Game Asset License Agreement**. The
agreement permits using and modifying the assets in commercial and non-commercial games. It does
not permit reselling, sharing, or redistributing the assets by themselves, including edited copies
or inclusion in asset packs, templates, or general-purpose bundles.

The vendor also publishes the license update and product pages online:

- <https://pizzadoggy.itch.io/psx-mega-pack-2/devlog/961920/updated-the-license-agreement>
- <https://pizzadoggy.itch.io/psx-tech>

Release policy:

- Cook only the reviewed catalog closure needed by Kessel; never ship the source archives, Blender
  libraries, alternate FBX/OBJ/DAE copies, previews, thumbnails, or the full authoring library.
- Include cataloged models only as integrated Kessel game resources. Do not publish the cooked
  model directory as an asset download, SDK, template, or reusable library.
- Keep each cooked entry's asset ID, source family, byte hash, and license ID in the runtime manifest
  so every shipped file remains auditable.
- Do not add a new Pizza Doggy pack or asset family to a public build until its purchase and license
  evidence have been reviewed and recorded.
- Re-review the current vendor terms before the first public or commercial release.

Attribution is appreciated by the vendor but is not required by the retained agreement. Kessel
should nevertheless credit **Pizza Doggy** in the shipped credits and documentation.

## Kessel local and secondary assets

Catalog entries marked `kessel-provenance-unrecorded` remain blocked from public distribution.
Their presence in a developer build does not establish redistribution rights. Each one must be
replaced, removed, or assigned a verified license record before a public package is approved.
