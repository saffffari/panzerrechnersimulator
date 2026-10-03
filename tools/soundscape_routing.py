"""Explicitly authored completion of the sorted library; no waveform/semantic guessing.

Current relative paths make this assignment subordinate to a later user folder move.
These finite phrases use existing foreground roles, never a new event scheduler.
"""
TONAL = (
    "05_PMLCG_Synth_One_Shot_C.wav", "19_PMLCG_Synth_One_Shot_C.wav",
    "PHOTAY_bass_buchla_04_C#.wav", "PHOTAY_bass_ocean_magnet_mid_E.wav",
    "PHOTAY_melodic_hit_dub_chord_03_B.wav", "PHOTAY_melodic_hit_marimba_01_C.wav",
    "PHOTAY_melodic_hit_rubber_buzz_G.wav", "oe_bs_psych_F.wav", "oe_bs_ufo_F.wav",
)
PERCUSSION = (
    "PHOTAY_kick_buchla_splash_01.wav", "PHOTAY_kick_defibulated_01.wav",
    "PHOTAY_kick_ocean_808_02_D.wav", "PHOTAY_kick_spring_slap_02.wav",
    "PHOTAY_percussion_buchla_block.wav", "PHOTAY_percussion_shaker_hihat.wav",
    "PHOTAY_snare_chopping_block_02.wav", "PHOTAY_snare_metal_01.wav",
    "oe_prc_woodthump7.wav",
)
SCORE_PHRASES = {
    **{f"effects/{name}": "music" for name in TONAL},
    **{f"effects/{name}": "combat" for name in PERCUSSION},
    "effects/PHOTAY_fx_hit_dub_fossil_04.wav": "threat",
    "music/PHOTAY_texture_salt_mine_riser_02_D.wav": "music",
    **{f"combat/{name}": "combat" for name in
       ("surprise-contact.wav", "surprise_contact.wav", "surprise_contact2.wav")},
}
DEFERRED = {
    "effects/OS_ORG_Footsteps_In_The_Snow.wav": "Multi-step recording requires source-step segmentation and actual snow foot contact binding; no fabricated footsteps.",
    "effects/PHOTAY_texture_redwood_campfire.wav": "Requires an actual spatial burning-source binding; not global ambience or score.",
    "effects/OS_ORG_Coin_Box_1.wav": "No corresponding coin-box interaction exists in this game.",
    "effects/error.wav": "No authored failure action is assigned; not an automatic error alarm.",
    "effects/error2.wav": "No authored failure action is assigned; not an automatic error alarm.",
    "effects/light_impact.wav": "Generic impact remains unassigned; do not stack over the bound precise light_round_impact source.",
}


def assignment(path: str) -> dict | None:
    if path in SCORE_PHRASES:
        return {"role": SCORE_PHRASES[path], "finite_foreground_phrase": True,
                "basis": "explicit final sorted-library authoring; existing programme/compatibility and rests; no physical trigger or new scheduler"}
    if path in DEFERRED:
        return {"enabled": False, "reason": DEFERRED[path],
                "basis": "explicitly deferred unrepresented or unprepared physical effect"}
    return None
