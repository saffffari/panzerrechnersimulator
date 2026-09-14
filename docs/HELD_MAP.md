# Physical held map

M opens a real folded paper mesh, 260 × 155 mm with a 0.75 mm edge thickness. It uses ordinary
scene lighting, depth and projection. The source grid has separate fold normals, not a rectangle
drawn over the final image. The imported 52.5 mm pocket compass used by C rests on the paper at
its actual scale; its separate magnetic needle uses the current viewing heading. Its south half
is neutral grey and the north half remains red.

The compact sheet is held 175 mm from the eye and the compass rests on its lower-left fold.
C holds that same compass 122.5 mm from the eye. These physical dimensions and matching hold
distances bring the navigation objects closer than their previous intersecting poses while
preserving their apparent framing. The final v3 GPU review confirms that both the complete paper
and the C compass clear the loader port without intersecting its frame. They still use the normal world depth buffer. Reading M
temporarily uses an ordinary eye field of at least 60 degrees, rather than visor magnification.

Existing survey cartography, discovered map sheets, scout marks, LMB graphite, RMB pins and
their saved normalized paper coordinates are retained. They are drawn into a persistent 1024 ×
768 GPU albedo target before the scene pass. Paper UVs sample the original canvas region, so
folding and perspective automatically keep annotations attached to the paper. The target is
repainted only when the ink or cursor changes. It needs no readback, mesh rebuild or physics
collision work. This uses one additional scene mesh and reuses the two compass mesh slots.

The old flat paper shadow, fake compass, fake pencil and rectangular hand silhouettes are
removed. Map controls remain readable on the final display; the paper and compass are part of
the scene. Pin symbols remain cartographic annotations, not separately simulated pushpins.

M preserves the compass's C-equipped state, so folding returns an already equipped compass to
its normal pose. C does not equip a second compass while the map is open. Held rounds/cases and
field props must be put down first; opening the map never discards them. The extinguisher is
stowed like it is when equipping C. Ordinary first-person weapons are hidden while holding paper.

The runtime GLB remains unchanged. `hand_compass::load` replaces the imported needle palette's
blue texels with neutral grey on its private texture copy and scales both loaded mesh pages by
0.5 to the 52.5 mm pocket diameter. Geometry proportions, red north material and physical
cardinal markings retain their source asset data and existing attribution.

## Integration and verification

`Game::set_map_held` is the input gate. After periscope refresh and selecting the current view
camera, `Game::prepare_held_map` poses the paper/compass. Queue `ui::draw_held_map_print` into an
empty UI queue, call `Renderer::render_held_map_texture`, then draw the ordinary scene. Finally
`ui::draw_held_map` queues only the map controls. This ordering is shared by native play and the
first-person mechanic recorder.

The new `act1_solo_crew_physical_map.v1.json` fixture builds the real campaign Act One with the
selected winter revision and five mortal singleplayer crew. It exercises M, pencil, pin, folding,
C and reopening M. It does not manufacture map sheets or disable enemy fire. The recorder
accepts only run seed/live policy with `setup.act_one_playtest`; practice swarms, fixture damage,
teleports and artificial LAN crews are rejected. No checkpoint is written by the recorder.

CPU tests cover paper geometry/projection, compass scale/orientation and grey/red palette
separation. Runtime visual acceptance must inspect readability, registered annotations, ordinary
scene occlusion and both C/M compass appearances. A successful state receipt alone does not
establish those visual results.

Final reviewed capture: `captures/astra/act1_crew_map_20260913_v3.mp4`. Both the map and compass
stills were inspected; the full sheet, ink, grey/red needle and ordinary scene occlusion are
readable. The matching manifest records the interaction checks separately from visual review.
