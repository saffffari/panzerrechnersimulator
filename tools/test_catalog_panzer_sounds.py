"""CPU-only numerical/catalog regressions; no game, GPU, or source-asset modifications."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import wave

import numpy as np


SPEC = importlib.util.spec_from_file_location("catalog_panzer_sounds", Path(__file__).with_name("catalog_panzer_sounds.py"))
TOOL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOOL)
from import_soundscape_categories import merge_targeted


class CatalogTests(unittest.TestCase):
    def test_pcm24_sign_extension_and_channel_order(self):
        numbers = [-8388608, -1, 0, 1, 8388607, 4194304]
        raw = b"".join((x & 0xffffff).to_bytes(3, "little") for x in numbers)
        actual = TOOL.pcm_decode(raw, 3, 2)
        np.testing.assert_array_equal(actual, np.array(numbers).reshape(3, 2) / 8388608.0)

    def test_streamed_metrics_keep_silence_separate_from_a_safe_loop(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture.wav"
            # Deliberate DC and trailing silence: zero endpoints must not certify a seamless loop.
            frames = np.zeros((70_000, 2), dtype="<i2")
            frames[1:60_000] = (16384, -8192)
            with wave.open(str(path), "wb") as wav:
                wav.setparams((2, 2, 1000, len(frames), "NONE", "not compressed"))
                wav.writeframes(frames.tobytes())
            fmt, metrics = TOOL.analyze(path)
            expected = np.sqrt(np.square(frames.astype(float) / 32768).mean())
            self.assertAlmostEqual(metrics["rms_dbfs"], TOOL.db(expected), places=12)
            self.assertEqual(fmt["frames"], 70_000)
            self.assertIsNone(metrics["loop_seam"]["wrap_jump_peak_dbfs"])
            self.assertEqual(metrics["loop_seam"]["trailing_digital_silence_s"], 10.0)
            self.assertFalse(metrics["listened"])

    def test_content_duplicates_keep_night_intent_and_clean_alias(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            night = root / "night_ambiance"
            night.mkdir()
            clean = night / "D_-_Quiet.wav"
            with wave.open(str(clean), "wb") as wav:
                wav.setparams((1, 2, 44100, 64, "NONE", "not compressed"))
                wav.writeframes(np.zeros(64, dtype="<i2").tobytes())
            (root / "A.wav").write_bytes(clean.read_bytes())
            (night / "D_-_Quiet (1).wav").write_bytes(clean.read_bytes())
            first = TOOL.catalog(root, -30.0, -12.0)
            second = TOOL.catalog(root, -30.0, -12.0)
            self.assertEqual(first, second)
            self.assertEqual(first["inventory"]["source_files"], 3)
            self.assertEqual(first["inventory"]["unique_assets"], 1)
            entry = first["assets"][0]
            self.assertEqual(entry["source_path"], "night_ambiance/D_-_Quiet.wav")
            self.assertEqual(entry["role"], "night")
            self.assertTrue(entry["default_enabled"])
            self.assertEqual(len(entry["aliases"]), 2)
            self.assertEqual(entry["gain"]["suggested_db"], 0.0)
            self.assertNotIn("playback_path", entry)

    def test_explicit_import_is_scoped_and_does_not_reclassify_a_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "playground_people.wav"
            with wave.open(str(source), "wb") as wav:
                wav.setparams((1, 2, 44100, 64, "NONE", "not compressed"))
                wav.writeframes(np.zeros(64, dtype="<i2").tobytes())
            disabled = TOOL.catalog(root, -30.0, -12.0)["assets"][0]
            enabled = TOOL.catalog(root, -30.0, -12.0, (source.name,))["assets"][0]
            self.assertFalse(disabled["default_enabled"])
            self.assertTrue(enabled["default_enabled"])
            self.assertEqual(enabled["role"], disabled["role"])
            self.assertEqual(enabled["role"], "scene_bed")
            self.assertEqual(enabled["id"], disabled["id"])
            with self.assertRaises(ValueError):
                TOOL.catalog(root, -30.0, -12.0, ("../outside.wav",))

    def test_scene_and_filename_inferences_are_not_content_approval(self):
        self.assertEqual(TOOL.classify("oe_fxlp120_playgroundambience2.wav")[0], "scene_bed")
        self.assertEqual(TOOL.classify("airport_people.wav")[0], "scene_bed")
        self.assertEqual(TOOL.classify("unknown.wav")[0], "texture_candidate")
        info = TOOL.filename_metadata("oe_mus110_puddles_keys1_G#m.wav")
        self.assertEqual(info["bpm"]["value"], 110)
        self.assertEqual(info["key"]["tonic"], "G#")
        self.assertEqual(info["key"]["mode"], "minor")
        self.assertFalse(info["tempo_analyzed"])

    def test_folder_roles_and_contexts_keep_cabin_and_location_scopes_distinct(self):
        self.assertEqual(TOOL.classify("music/winter/night/A.wav")[0], "music")
        self.assertEqual(TOOL.classify("game_start_atmosphere/A.wav")[0], "intro")
        self.assertEqual(TOOL.classify("death_sounds/A.wav")[0], "stinger")
        self.assertEqual(TOOL.classify("effects/A.wav")[0], "effect")
        self.assertEqual(TOOL.classify("interior_tank_ambiance/winter/A.wav")[0], "cabin")
        self.assertEqual(TOOL.classify("combat/winter/night/A.wav")[0], "combat")
        role, tags, _ = TOOL.classify("scenes/Coffee Shop/busy/A.wav")
        self.assertEqual((role, tags), ("scene", ["coffee_shop"]))
        self.assertEqual(TOOL.context_token("Winter Storm"), "winter_storm")

    def test_music_import_intent_preserves_contexts_and_attenuates_without_compression(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "music" / "winter" / "night" / "theme.wav"
            source.parent.mkdir(parents=True)
            with wave.open(str(source), "wb") as wav:
                wav.setparams((2, 2, 44100, 64, "NONE", "not compressed"))
                wav.writeframes(np.full(128, 8192, dtype="<i2").tobytes())
            before = source.read_bytes()
            entry = TOOL.catalog(root, -30.0, -12.0)["assets"][0]
            self.assertEqual(entry["role"], "music")
            self.assertTrue(entry["default_enabled"])
            self.assertEqual(entry["contexts"], ["winter", "night"])
            self.assertEqual(entry["folder_contexts"], ["winter", "night"])
            self.assertEqual(entry["gain"]["target_rms_dbfs"], -26.0)
            self.assertAlmostEqual(entry["gain"]["suggested_db"] + entry["analysis"]["rms_dbfs"], -26.0)
            self.assertEqual(source.read_bytes(), before)

    def test_user_category_aliases_and_oneshots_preserve_source_names(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for index, folder in enumerate(["winter_atmosphere", "summer_atmosphere", "death_sounds", "game_start_atmosphere", "effects"]):
                source = root / folder / "sample.wav"
                source.parent.mkdir()
                with wave.open(str(source), "wb") as wav:
                    wav.setparams((1, 2, 44100, 64, "NONE", "not compressed"))
                    wav.writeframes(np.full(64, index + 1, dtype="<i2").tobytes())
            entries = {a["category_folder"]: a for a in TOOL.catalog(root, -30, -12)["assets"]}
            self.assertEqual(entries["winter_atmosphere"]["contexts"], ["winter"])
            self.assertEqual(entries["summer_atmosphere"]["contexts"], ["spring"])
            self.assertEqual(entries["summer_atmosphere"]["source_path"], "summer_atmosphere/sample.wav")
            for folder in ["death_sounds", "game_start_atmosphere", "effects"]:
                self.assertFalse(entries[folder]["loop"]["enabled"])
                self.assertEqual(entries[folder]["loop"]["crossfade_s"], 0)

    def test_role_aliases_keep_alternative_contexts_instead_of_impossible_conjunction(self):
        a = TOOL.role_memberships([TOOL.folder_assignment(p) for p in [
            "winter_atmosphere/A.wav", "summer_atmosphere/A.wav", "suspense/winter/A.wav"]])
        self.assertEqual(a["role_aliases"], ["day", "threat"])
        self.assertEqual(a["role_contexts"], {"day": [["winter"], ["spring"]], "threat": [["winter"]]})

    def test_explicit_loop_filename_and_alias_do_not_activate_unassigned_sources(self):
        for name in ["foo_loop.wav", "foo_loop_160.wav", "foo_LOOP (1).wav"]:
            a = {"source_path": "effects/plain.wav", "aliases": [name], "bpm": 160,
                 "default_enabled": False, "loop": {"enabled": False, "crossfade_s": 2}}
            TOOL.apply_filename_loop_instruction(a)
            self.assertTrue(a["loop"]["enabled"])
            self.assertEqual(a["loop"]["crossfade_s"], 0)
            self.assertEqual(a["filename_loop_sources"], [name])
            self.assertFalse(a["default_enabled"])
        for name in ["looping.wav", "foo_looper.wav", "myloop.wav"]:
            a = {"source_path": name, "loop": {"enabled": False}}
            TOOL.apply_filename_loop_instruction(a)
            self.assertFalse(a["explicit_filename_loop"])
            self.assertFalse(a["loop"]["enabled"])

    def test_targeted_solar_refresh_preserves_other_asset_and_content_id(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "night_ambiance").mkdir()
            for p, value in [(root / "original.wav", 200), (root / "night_ambiance" / "keep.wav", 100)]:
                with wave.open(str(p), "wb") as wav:
                    wav.setparams((1, 2, 44100, 64, "NONE", "not compressed"))
                    wav.writeframes(np.full(64, value, dtype="<i2").tobytes())
            previous = TOOL.catalog(root, -30, -12)
            original = next(a for a in previous["assets"] if a["source_path"] == "original.wav")
            keep = next(a for a in previous["assets"] if a["role"] == "night")
            (root / "sunrise").mkdir()
            (root / "original.wav").rename(root / "sunrise" / "sunrise_sound_start.wav")
            # A corrupt unrelated new source proves this path does not broadly re-analyze.
            (root / "unrelated.wav").write_bytes(b"not a wave file")
            addition = TOOL.catalog(root, -30, -12, folders=("sunrise", "sunset"))
            merged = merge_targeted(previous, addition, ["sunrise", "sunset"])
            self.assertEqual(len(merged["assets"]), 2)
            self.assertEqual(next(a for a in merged["assets"] if a["id"] == keep["id"]), keep)
            new = next(a for a in merged["assets"] if a["id"] == original["id"])
            self.assertEqual(new["role"], "stinger")
            self.assertTrue(new["default_enabled"])
            self.assertFalse(new["loop"]["enabled"])
            self.assertEqual(merged["inventory"]["targeted_refresh"]["pending_empty_categories"], ["sunset"])

    def test_blizzard_category_and_explicit_filename_are_separate_from_score(self):
        self.assertEqual(TOOL.classify("heavy_blizzard/Storm_Nexus.wav")[0], "blizzard")
        self.assertEqual(TOOL.classify("storm_heavy_blizzard_loop.wav")[0], "blizzard")
        self.assertEqual(TOOL.folder_assignment("heavy_blizzard/Storm_Nexus.wav")["contexts"], [])
        self.assertEqual(TOOL.classify("dusk/twilight.wav")[0], "stinger")
        self.assertEqual(TOOL.folder_assignment("dusk/twilight.wav")["contexts"], [])

    def test_engine_hiss_label_is_physical_and_does_not_assign_generic_effects(self):
        for name in ["destroyed_engine_hiss.wav", "broken_engine.wav", "hissbroken_loop.wav"]:
            self.assertEqual(TOOL.classify("effects/" + name)[0], "engine_hiss")
        for name in ["hiss.wav", "engine_hissing.wav", "synth_one_shot.wav"]:
            self.assertEqual(TOOL.classify("effects/" + name)[0], "effect")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "effects").mkdir()
            source = root / "effects" / "destroyed_engine_hiss.wav"
            with wave.open(str(source), "wb") as wav:
                wav.setparams((1, 2, 44100, 64, "NONE", "not compressed"))
                wav.writeframes(np.full(64, 200, dtype="<i2").tobytes())
            cue = TOOL.catalog(root, -30, -12, folders=("effects",))["assets"][0]
            self.assertEqual(cue["role_aliases"], ["engine_hiss"])
            self.assertEqual(cue["contexts"], [])
            self.assertFalse(cue["loop"]["enabled"])
            self.assertEqual(cue["gain"]["target_rms_dbfs"], -30)


if __name__ == "__main__":
    unittest.main()
