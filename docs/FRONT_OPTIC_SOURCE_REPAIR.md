# Front optic source repair

The raster build registers twelve optical entries: seven cupola panes, the loader periscope, the gunner sight, two front roof periscopes and the fixed radio operator telescope. The direct driver vision block remains transparent glass.

Driver and radio roof views use the upper forward faces of their original housings. The radio eyepiece was exported as two opaque triangles inside `Hull_0299`; those exact triangles are partitioned from the moving lid and draw the live image. They retain the authored offset, rather than mirroring the driver's pane. Each pane follows its own original hatch pivot.

The fixed radio telescope had no finite opening through the two ball components ahead of it. Its source objective rim measures approximately 23 mm across. An ordinary ray from that rim hit `Hull_0200` about 78 mm ahead and `Hull_0175` about 146 mm ahead. A dense front scan found small mesh seams, but no usable aperture.

The runtime derivative reconstructs a finite, 24-sided bore on that rim's measured axis through only the exact triangles belonging to those two named components. Its start and exit are measured from source surface intersections around the lens. Interpolated UVs and all other hull triangles remain intact; the added bore walls use the source metal material. The relay objective is at the resulting outer opening. Original GLBs are unchanged.

CPU checks cover a 72-ray field through the real bore, 24 surrounding armor rays that must stay blocked, exact radio-pane triangle conservation and all three registered entries. These checks do not establish live visual acceptance. The root task must inspect the front views in the running game before treating them as accepted.

The new reload capture fixture is `catalog/shader_reviews/held_mouse_reload.v1.json`. It uses click-to-toggle and continuous LMB grip, observes case release and a complete second shot, and does not inject ammunition or chamber state. Earlier capture evidence is preserved.
