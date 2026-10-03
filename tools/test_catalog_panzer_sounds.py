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
    def test_unused_folder_overrides_effect_filename_triggers(self):
        for path in ["unused/heavy_blizzard_loop.wav", "unused/broken_engine_hiss.wav",
                     "unused/combat_start.wav", "unused/night_ambiance.wav"]:
            self.assertEqual(TOOL.classify(path)[0], "unused")
            self.assertFalse(TOOL.folder_assignment(path)["default_enabled"])

    def test_explicit_current_music_intent_restricts_roles_and_contexts(self):
        for path, role, contexts in [
            ("music/theme_combat_loop.wav", "combat", ["combat"]),
            ("music/night_melodic_synth_loop.wav", "music", ["night"]),
            ("music/vibey_driving_loop.wav", "driving", ["driving"]),
            ("combat/combat_inside_tank.wav", "combat", ["inside", "combat"]),
            ("music/PHOTAY_texture_salt_mine_riser_02_D.wav", "music", []),
        ]:
            assignment = TOOL.folder_assignment(path)
            self.assertEqual(assignment["role"], role)
            self.assertEqual(assignment["contexts"], contexts)
        a = {"source_path": "music/theme_calm_driving_loop_night.wav", "aliases": [],
             "historical_source_paths": ["music/old_combat_loop.wav"], "default_enabled": True}
        TOOL.apply_filename_usage(a)
        self.assertEqual(a["role_aliases"], ["explore", "driving"])
        self.assertEqual(a["role_contexts"], {"explore": [["night"]], "driving": [["night"]]})
        self.assertNotIn("combat", a["contexts"])

    def test_explicit_unprepared_foley_is_inactive_without_disabling_bound_controls(self):
        from soundscape_routing import DEFERRED, SCORE_PHRASES
        self.assertEqual(len(DEFERRED), 6)
        self.assertEqual(len(SCORE_PHRASES), 23)
        for path in DEFERRED:
            a = {"source_path": path, "default_enabled": True}
            TOOL.apply_filename_usage(a)
            self.assertFalse(a["default_enabled"])
            self.assertFalse(a["runtime_assignment"]["enabled"])
        for path in ("effects/fire_igniting.wav", "effects/light_round_impact.wav", "effects/hammer_pickup.wav"):
            a = {"source_path": path, "default_enabled": True}
            TOOL.apply_filename_usage(a)
            self.assertTrue(a["default_enabled"])
            self.assertNotIn("runtime_assignment", a)

    def test_musical_filename_labels_ignore_vendor_numbers_and_keep_explicit_modes(self):
        p73 = TOOL.filename_metadata("P73_tonal_loop_rumble_electric_piano_01_120_Gbm.wav")
        self.assertEqual(p73["bpm"]["value"], 120)
        self.assertEqual((p73["key"]["tonic"], p73["key"]["mode"]), ("Gb", "minor"))
        for name, bpm, tonic, mode in [
            ("theme_128bpm_C#_minor.wav", 128, "C#", "minor"),
            ("theme_Bb_major_bpm_94.wav", 94, "Bb", "major"),
            ("theme_87.5bpm_F#m.wav", 87.5, "F#", "minor"),
            ("oe_drm110_wax_top.wav", 110, None, None),
            ("factory120_808_01.wav", None, None, None),
        ]:
            parsed = TOOL.filename_metadata(name)
            self.assertEqual(parsed["bpm"]["value"], bpm)
            self.assertEqual((parsed["key"]["tonic"], parsed["key"]["mode"]), (tonic, mode))

    def test_same_content_renames_preserve_music_but_conflicts_stay_unknown(self):
        a = {"source_path": "combat/surprise_contact.wav", "historical_source_paths": ["oe_bs100_haunted_Em.wav"]}
        TOOL.apply_musical_metadata(a)
        self.assertEqual((a["bpm"], a["musical_key"]), (100, "E minor"))
        self.assertEqual(a["musical_metadata"]["key"]["sources"][0]["source_kind"], "same_content_historical_filename")
        a["aliases"] = ["new_110_Fmajor.wav"]
        TOOL.apply_musical_metadata(a)
        self.assertIsNone(a["bpm"])
        self.assertIsNone(a["musical_key"])
        self.assertEqual(a["musical_metadata"]["bpm"]["status"], "conflict")
        self.assertEqual(a["musical_metadata"]["key"]["status"], "conflict")

    def test_tonic_only_is_not_major_and_enharmonic_labels_are_not_false_conflicts(self):
        a = {"source_path": "night/D_-_Calm.wav"}
        TOOL.apply_musical_metadata(a)
        self.assertIsNone(a["musical_key"])
        self.assertEqual(a["musical_metadata"]["key"]["status"], "tonic_only")
        a = {"source_path": "theme_C#minor.wav", "aliases": ["theme_Dbmin.wav"]}
        TOOL.apply_musical_metadata(a)
        self.assertEqual(a["musical_key"], "C# minor")
        a["aliases"].append("theme_C#major.wav")
        TOOL.apply_musical_metadata(a)
        self.assertIsNone(a["musical_key"])

    def test_tempo_conflicts_within_one_filename_are_not_arbitrarily_first_match(self):
        a = {"source_path": "theme_120_140_Am_Cm.wav"}
        TOOL.apply_musical_metadata(a)
        self.assertIsNone(a["bpm"])
        self.assertIsNone(a["musical_key"])

    def test_reimport_history_requires_same_full_content_hash(self):
        old = {"assets": [{"id": "a", "sha256": "hash", "source_path": "oe_drm110_a.wav"}]}
        a = {"id": "a", "sha256": "hash", "source_path": "combat/plain.wav", "loop": {"enabled": True}}
        TOOL.preserve_musical_history({"assets": [a]}, old)
        self.assertEqual(a["bpm"], 110)
        self.assertEqual(a["loop"]["crossfade_s"], 0)
        other = {"id": "a", "sha256": "different", "source_path": "combat/plain.wav", "loop": {"enabled": False}}
        TOOL.preserve_musical_history({"assets": [other]}, old)
        self.assertIsNone(other["bpm"])

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
