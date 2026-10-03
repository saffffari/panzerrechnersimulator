"""Numerical policy contracts; no game build, GPU, or claims of listening."""
from pathlib import Path
import tempfile
import unittest
import wave

import numpy as np

from suggest_soundscape_cue_policy import envelope, suggest
from merge_soundscape_cue_policy import merge


def asset(role="music", name="piece.wav", bpm=None):
    return {"id": "fixture", "role": role, "source_path": name, "sha256": "fixture",
            "bpm": bpm, "loop": {"crossfade_s": 2, "crossfade_curve": "equal_power"},
            "gain": {"suggested_db": -7}, "analysis": {"crest_factor_db": 16}}


def measured(duration):
    return {"duration_s": duration, "peak_window_rms_dbfs": -15,
            "last_2s_rms_dbfs": -70, "window_rms_dbfs": [-15, -45, -85]}


class PolicyTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
