# Handheld compass

C raises a real 3D pocket compass; C or Q stows it. A held live round, spent case, or field prop
must be put down first. The compass uses the actual viewing heading and follows the existing world
convention: north is negative Z, east is positive X. Its red needle turns independently of the body.

The existing map has a drawn compass in its UI. This addition instead loads two physical mesh pages
and renders them with the scene's materials and lighting. Both pages are camera-local raster props.

The source is **Artjom Shebunin (artie31), Lowpoly survival item pack**, creator-published under
[CC0](https://opengameart.org/content/lowpoly-survival-item-pack). The original `compass` and
`compass steer` body/needle geometry is retained. Changes normalize the body to a 105 mm diameter,
give it grey-green paint and an ivory face, and add physical cardinal letters and 24 graduations.
This is a stylized pocket compass, not a claim of a particular historical German instrument model.

Runtime asset and attribution:

- `/home/alex/panzer/tank_simulator/assets/props/hand_compass_artie31/hand_compass.glb`
- `/home/alex/panzer/tank_simulator/assets/props/hand_compass_artie31/SOURCE_AND_LICENSE.txt`
- `/home/alex/panzer/tank_simulator/assets/props/hand_compass_artie31/ORIGINAL_README.txt`
- Original ZIP, Blender source, texture and adaptation script:
  `/home/alex/panzer/tank_simulator/source_assets/external/artie31-survival-cc0/`

`catalog/hand_compass_asset.v1.json` records source and runtime checksums. The launcher preflight
requires the runtime GLB. Unit tests cover separate source mesh pages and the four cardinal headings.
A CPU material preview is in `captures/astra/hand_compass_artie31_source_review_20260909.png`;
it is an asset review, not in-game visual acceptance.
