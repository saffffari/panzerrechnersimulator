"""Numerical policy contracts; no game build, GPU, or claims of listening."""
from pathlib import Path
import tempfile
import unittest
import wave

import numpy as np

from suggest_soundscape_cue_policy import envelope, suggest, TRANSITION_RISER_ID, named_effect_events
from merge_soundscape_cue_policy import merge, merge_sorted
from refresh_soundscape_musical_metadata import refresh


def asset(role="music", name="piece.wav", bpm=None):
    return {"id": "fixture", "role": role, "source_path": name, "sha256": "fixture",
            "bpm": bpm, "loop": {"crossfade_s": 2, "crossfade_curve": "equal_power"},
            "gain": {"suggested_db": -7}, "analysis": {"crest_factor_db": 16}}


def measured(duration):
    return {"duration_s": duration, "peak_window_rms_dbfs": -15,
            "last_2s_rms_dbfs": -70, "window_rms_dbfs": [-15, -45, -85]}


class PolicyTests(unittest.TestCase):
    def test_final_score_phrases_use_existing_foreground_roles_without_event_or_loop(self):
        from soundscape_routing import SCORE_PHRASES
        from collections import Counter
        self.assertEqual(Counter(SCORE_PHRASES.values()), {"music": 10, "combat": 12, "threat": 1})
        entries = []
        for path, role in SCORE_PHRASES.items():
            p = suggest(asset(role="effect", name=path), measured(5))
            self.assertEqual(p["role"], role)
            self.assertEqual(p["role_aliases"], [role])
            self.assertFalse(p["loop"])
            self.assertEqual(p["crossfade_s"], 0)
            self.assertNotIn("riser_peak_s", p)
            self.assertIsNone(p["bpm"])
            if role == "combat":
                self.assertIn("combat", p["contexts"])
            entries.append(p)
        self.assertEqual(named_effect_events(entries), {})

    def test_phrase_merge_preserves_treatment_and_registry_does_not_activate_deferred(self):
        old = {"cues": [{"asset": "phrase", "role": "effect", "gain_db": -3, "rack": {"wet": .08}},
                         {"asset": "deferred"}], "events": {"error": ["deferred"], "surprise_contact": ["phrase"]}}
        fresh = {"manifest_sha256": "new", "cues": [{"asset": "phrase", "role": "music", "loop": False,
            "gain_db": 0, "rack": {"wet": 0}}], "events": {}, "named_effect_registry": {"error": ["deferred"]}}
        manifest = {"assets": [{"id": "phrase", "role": "effect", "default_enabled": True,
            "playback_available": True, "runtime_assignment": {"finite_foreground_phrase": True}},
            {"id": "deferred", "role": "effect", "default_enabled": False}]}
        out, receipt = merge_sorted(old, old, fresh, manifest)
        self.assertEqual(out["cues"][0]["gain_db"], -3)
        self.assertEqual(out["cues"][0]["rack"], {"wet": .08})
        self.assertFalse(out["cues"][0]["loop"])
        self.assertEqual(out["events"], {})
        self.assertEqual(out["named_effect_registry"], {"error": ["deferred"]})
        self.assertEqual(receipt["retired_cue_ids"], ["deferred"])

    def test_full_sorted_scan_retires_stale_routes_but_keeps_authored_treatment(self):
        old = {"cues": [{"asset": "keep", "gain_db": 0, "role": "music"}, {"asset": "gone"}]}
        current = {"cues": [{"asset": "keep", "gain_db": -5, "role": "music", "rack": {"wet": .12},
                             "riser_peak_s": 5.4}, {"asset": "gone"}, {"asset": "unused"}],
                   "events": {"custom": ["keep", "gone"], "stale": ["unused"], "incoming_riser": ["keep"]}}
        fresh = {"manifest_sha256": "new", "cues": [{"asset": "keep", "role": "combat", "contexts": ["combat"],
                 "gain_db": 0, "rack": {"wet": 0}}], "events": {"intensity_transition_riser": ["keep"]}}
        manifest = {"assets": [{"id": "keep", "default_enabled": True, "playback_available": True, "role": "combat"},
                                {"id": "unused", "default_enabled": False, "role": "unused"}]}
        result, receipt = merge_sorted(old, current, fresh, manifest)
        self.assertEqual(len(result["cues"]), 1)
        cue = result["cues"][0]
        self.assertEqual((cue["gain_db"], cue["rack"], cue["riser_peak_s"]), (-5, {"wet": .12}, 5.4))
        self.assertEqual((cue["role"], cue["contexts"]), ("combat", ["combat"]))
        self.assertEqual(result["events"], {"custom": ["keep"], "intensity_transition_riser": ["keep"]})
        self.assertEqual(receipt["retired_cue_ids"], ["gone", "unused"])

    def test_named_effect_registry_is_literal_and_solar_loops_remain_finite_events(self):
        entries = [{"id": str(i), "role": "effect", "source_path": "effects/" + name} for i, name in enumerate([
            "PHOTAY_snare_marble_splash_cigarette_pickup.wav", "hammer_pickup2.wav",
            "trigger_pull_not_loaded.wav", "PHOTAY_bass_dull_stopped_impact.wav", "ordinary_texture.wav"])]
        events = named_effect_events(entries)
        self.assertEqual(events, {"cigarette_pickup": ["0"], "hammer_pickup": ["1"],
                                 "trigger_pull_not_loaded": ["2"], "impact_stopped": ["3"]})
        a = asset(role="stinger", name="sunrise/sunrise_loop.wav")
        policy = suggest(a, measured(18))
        self.assertTrue(a["explicit_filename_loop"])
        self.assertTrue(a["loop"]["enabled"])
        self.assertFalse(policy["loop"])
        self.assertEqual(policy["envelope"]["duration_s"], 18)

    def test_music_only_refresh_preserves_authored_clear_rack_gain_and_events(self):
        import copy
        assets = [{"id": "a", "source_path": "renamed.wav", "historical_source_paths": ["old_120_Am.wav"]},
                  {"id": "b", "source_path": "D_-_Night.wav"}]
        proposal = {"cues": [{"asset": "a", "gain_db": 0, "loop": False}, {"asset": "b"}], "events": {"x": ["a"]}}
        current = copy.deepcopy(proposal)
        current["cues"][0].update(musical_key=None, gain_db=-5, rack={"wet": .12})
        policy = {"cues": [{"id": a["id"], "envelope": {"duration_s": 10}} for a in assets]}
        result = refresh({"assets": assets}, policy, proposal, current)
        self.assertEqual(result[0]["assets"][0]["musical_key"], "A minor")
        self.assertEqual(result[3]["cues"][0]["bpm"], 120)
        self.assertIsNone(result[3]["cues"][0]["musical_key"])
        self.assertEqual(result[3]["cues"][0]["gain_db"], -5)
        self.assertEqual(result[3]["cues"][0]["rack"], {"wet": .12})
        self.assertNotIn("musical_key", result[2]["cues"][1])
        self.assertEqual(result[2]["cues"][1]["musical_metadata"]["key"]["tonic"], "D")
        self.assertEqual(result[3]["events"], current["events"])

    def test_streamed_quarter_seconds_retain_real_level_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "decay.wav"
            data = np.repeat(np.array([16384, 8192, 4096, 0], dtype="<i2"), 250)
            with wave.open(str(path), "wb") as wav:
                wav.setparams((1, 2, 1000, 1000, "NONE", "not compressed"))
                wav.writeframes(data.tobytes())
            e = envelope(path)
            self.assertEqual(e["duration_s"], 1)
            np.testing.assert_allclose(e["window_rms_dbfs"][:3], [-6.020599913, -12.041199827, -18.061799740], atol=1e-8)
            self.assertIsNone(e["window_rms_dbfs"][-1])
            self.assertEqual(e["active_window_fraction"], 0.75)

    def test_night_and_cabin_keep_requested_bed_scope_despite_faded_ends(self):
        for role in ["night", "cabin"]:
            p = suggest(asset(role=role), measured(78))
            self.assertTrue(p["loop"])
            self.assertFalse(p["listened"])
            self.assertFalse(p["originals_modified"])

    def test_explicit_loop_repeats_whole_file_even_when_bar_length_needs_authoring(self):
        periodic = suggest(asset(name="oe_fxlp120_electrify.wav", bpm=120), measured(8))
        self.assertTrue(periodic["loop"])
        self.assertEqual(periodic["crossfade_s"], 0)
        faded = suggest(asset(name="JVIEWS_tonal_loop_vhs_scifi_160_Bb.wav", bpm=160), measured(50.136))
        self.assertTrue(faded["loop"])
        self.assertEqual(faded["crossfade_s"], 0)
        self.assertTrue(faded["loop_region_requires_authoring"])
        self.assertEqual(faded["gain_db"], 0)
        self.assertEqual(faded["total_source_gain_db"], -7)

    def test_alias_loop_overrides_effect_default_but_unlabelled_phrase_stays_once(self):
        a = asset(role="effect", name="effects/plain.wav")
        a["aliases"] = ["effects/original_loop.wav"]
        policy = suggest(a, measured(5))
        self.assertTrue(policy["loop"])
        self.assertEqual(policy["crossfade_s"], 0)
        self.assertFalse(suggest(asset(), measured(5))["loop"])

    def test_tail_rack_is_restricted_to_energetic_finite_endings(self):
        m = {"duration_s": 20, "peak_window_rms_dbfs": -15,
             "last_2s_rms_dbfs": -25, "window_rms_dbfs": [-24, -27, -35, -95]}
        phrase = suggest(asset(name="Ancient_Instrument.wav", role="explore"), m)
        self.assertGreater(phrase["rack"]["wet"], 0)
        self.assertEqual(phrase["rack"]["tail_only_s"], 3)
        for role in ["night", "cabin", "stinger", "effect"]:
            self.assertEqual(suggest(asset(role=role), m)["rack"]["wet"], 0)
        self.assertEqual(suggest(asset(role="intro"), measured(20))["rack"]["wet"], 0)
        m["window_rms_dbfs"] = [-19, -23, -21]
        m["last_2s_rms_dbfs"] = -22
        accent = suggest(asset(role="combat", name="combat_start.wav"), m)
        self.assertFalse(accent["loop"])
        self.assertEqual(accent["role"], "stinger")
        self.assertEqual(accent["role_aliases"], ["stinger"])
        self.assertTrue(accent["energetic_eof_window"])
        self.assertGreater(accent["rack"]["wet"], 0)
        pending = suggest(asset(role="combat", name="surprise_contact.wav"), m)
        self.assertEqual(pending["role_aliases"], ["effect"])
        self.assertFalse(pending["loop"])

    def test_three_way_refresh_keeps_authored_rack_gain_and_location_overrides(self):
        old = {"cues": [{"asset": "a", "gain_db": 0, "loop": False}], "events": {"death": ["a"]}}
        current = {"cues": [{"asset": "a", "gain_db": -4, "loop": False, "rack": {"wet": 0.2}}],
                   "events": {"death": ["a"], "authored": ["a"]}, "locations": [{"id": "real"}]}
        new = {"cues": [{"asset": "a", "gain_db": -1, "loop": True, "rack": {"wet": 0.1}}, {"asset": "b"}],
               "events": {"death": ["a", "b"]}}
        result, receipt = merge(old, current, new)
        self.assertEqual(result["cues"][0], {"asset": "a", "gain_db": -4, "loop": True, "rack": {"wet": 0.2}})
        self.assertEqual(result["locations"], current["locations"])
        self.assertEqual(result["events"], current["events"])
        self.assertEqual(receipt["added_cue_ids"], ["b"])

    def test_solar_event_is_finite_while_heavy_blizzard_is_a_physical_loop(self):
        solar = suggest(asset(role="stinger", name="sunrise/sunrise_sound_start.wav"), measured(40))
        self.assertFalse(solar["loop"])
        self.assertEqual(solar["solar_events"], ["sunrise"])
        self.assertEqual(solar["role_aliases"], ["stinger"])
        blizzard = suggest(asset(role="blizzard", name="heavy_blizzard/Storm_Nexus.wav"), measured(40))
        self.assertTrue(blizzard["loop"])
        self.assertEqual(blizzard["rack"]["wet"], 0)

    def test_engine_hiss_sustains_only_when_user_marks_repeat(self):
        finite = suggest(asset(role="engine_hiss", name="effects/destroyed_engine_hiss.wav"), measured(27))
        self.assertFalse(finite["loop"])
        self.assertEqual(finite["role_aliases"], ["engine_hiss"])
        self.assertEqual(finite["rack"]["wet"], 0)
        sustain = suggest(asset(role="engine_hiss", name="effects/destroyed_engine_hiss_loop.wav"), measured(27))
        self.assertTrue(sustain["loop"])
        self.assertEqual(sustain["crossfade_s"], 0)
        self.assertEqual(sustain["rack"]["wet"], 0)

    def test_dusk_is_a_separate_finite_event_even_with_a_loop_named_source(self):
        dusk = suggest(asset(role="stinger", name="dusk/late_twilight_loop.wav"), measured(40))
        self.assertFalse(dusk["loop"])
        self.assertEqual(dusk["solar_events"], ["dusk"])
        self.assertNotIn("sunset", dusk["solar_events"])

    def test_approved_transition_riser_timing_follows_content_not_generic_name(self):
        a = asset(role="threat", name="suspense/renamed.wav")
        a["id"] = TRANSITION_RISER_ID
        a["role_aliases"] = ["threat"]
        cue = suggest(a, measured(10.5))
        self.assertEqual(cue["role"], "stinger")
        self.assertEqual(cue["role_aliases"], ["stinger"])
        self.assertFalse(cue["loop"])
        self.assertEqual(cue["riser_peak_s"], 5.404943)
        self.assertEqual(cue["riser_lead_s"], 4)
        self.assertEqual(cue["fade_in_s"], .08)
        self.assertEqual(cue["rack"]["wet"], 0)
        other = suggest(asset(role="threat", name="suspense/riser_transition.wav"), measured(10.5))
        self.assertNotIn("riser_peak_s", other)


if __name__ == "__main__":
    unittest.main()
