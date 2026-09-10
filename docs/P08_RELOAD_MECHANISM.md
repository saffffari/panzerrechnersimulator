# P08 sidearm mechanism

The on-foot P08 uses its existing authored model's separate bolt (`Cube.007`), forward toggle
(`Cylinder.010`), rear toggle and knobs (`Cylinder.012`), and magazine (`Circle.001`). The imported
materials, scale, UVs and closed pose are retained. The former scaled modern pistol magazine and
floating display rounds are no longer used in the reload animation.

`src/p08.rs` measures the source hinge locations and solves a rigid two-link mechanism: the knee
moves upward, both link joints remain connected, and the bolt travels on a straight rail. The barrel
and front post are separate connected source components and move through a short recoil stroke.
The frame/receiver source has fused geometry; the small receiver extension is not a complete
internal mechanical reconstruction. There are no modeled fingers or magazine spring/follower.

`R` runs a magazine change. An empty weapon stays open while the magazine is removed and replaced;
the final animation draws the toggle a little farther back before spring closure. A tactical change
keeps the chamber loaded and does not run that action. Ammunition accounting supports eight magazine
rounds plus a retained chamber round, preserves reserve ammunition, and does not allow firing mid
reload. The initial load still contains eight ready rounds and eight in reserve. This change remains
scoped to the existing on-foot sidearm, without adding a new cabin equip interaction.

Four sounds are distinct samples/strokes from the owner's Rust & Blood sound pack: magazine out,
magazine in, toggle pull, and toggle close. Exact source bytes and playback crops are pinned in
`catalog/audio_feedback_20260905.json`. `tools/add_p08_feedback.py` can append those recipes to the
owner-private bank while retaining other selections. These are pistol foley adaptations, not claims
of recordings of this particular P08. The old generic pickup sound is not part of this reload.

The motion/state reference is the Century International Arms P.08 manual, pages 2–4:
https://stephentaylorhistorian.com/wp-content/uploads/2020/02/luger_p08.pdf
The existing asset's license remains unrecorded; `assets/flavor/luger/SOURCE.md` still governs
redistribution. No original GLB or source archive was changed.

CPU source geometry was visually inspected in `captures/astra/luger_parts_cpu_20260909.png` and
`captures/astra/luger_toggle_open_cpu_20260909.png` (colors separate parts for inspection). Those
images verify assembly geometry, not the in-game shader rendering or audio. Interactive reload,
weapon framing and an in-game audio audition are still required after the next build.
