import copy
import importlib.util
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch

import numpy as np

sys.path.insert(0,str(Path(__file__).parent))
import world_authoring as w
import editor_mcp


class RevisionTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
        self.patch=patch.object(w,"ROOT",self.root);self.patch.start()
        for path in ("assets/a.glb","assets/sounds/room.ogg","assets/ground.png"):
            p=self.root/path;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b"fixture")
        self.value={"schema":"kessel.act","version":1,"act_id":2,"act":"Study","seed":7,
            "terrain":{"res":17,"spacing":2,"heights":"heights.npy","texture":"assets/ground.png"},
            "assets":["assets/a.glb"],"placements":[[0,0,0,0,0,1],[0,2,0,0,0,1]],
            "markers":[{"name":kind,"kind":kind,"x":0,"y":0,"z":z} for kind,z in [("Spawn",10),("Goal",12),("Monument",0)]],
            "authoring":{"mode":"playtest","campaign_canonical":False}}
        self.heights=np.zeros((17,17),np.float32)
        self.revision=w.publish("base",self.value,self.heights,{})
        self.source=self.revision["manifest"]

    def tearDown(self):self.patch.stop();self.temp.cleanup()

    def scene(self):
        return {"schema":"kessel.scene-block","version":1,"name":"Test room",
            "anchor":{"origin_m":[0,0,0],"forward":"+z","grounding":"rigid_anchor"},
            "terrain":{"datum":"anchor_ground","res":3,"spacing_m":2,"apron_m":4,"heights_m":[0]*9},
            "items":[{"id":"bed","asset":"assets/a.glb","dx":2,"dy":1,"dz":0,"deg":0,"s":[1,1,1]}],
            "sockets":[{"id":"first_person_entry","kind":"entry","position_m":[0,1.65,4],"heading_deg":180}],"connectors":[],
            "metadata":{"provenance":{"placement_class":"attested_condition","source_ids":["witness_1"],"source_urls":["https://example.org/witness"],"placement_constraints":["april"]}},
            "environment":{"sound_emitters":[{"id":"room","asset":"assets/sounds/room.ogg","position_m":[2,1,0],"volume":.3,"radius_m":10,"duration_s":0,"playback":"loop"}],
                "authored_lights":[{"id":"lamp","kind":"point","position":[2,2,0],"direction":[0,-1,0],"temperature_k":2800,"tint_rgb":[1,1,1],"radiant_intensity":.05,"source_radius_m":.03,"range_m":10,"cone_angle_deg":90,"flicker":0,"enabled":True}]}}

    def test_fork_preserves_source_and_terrain(self):
        before=(self.root/self.source).read_bytes();old=w.world_hash(self.source)
        result=w.fork_world(self.source,"fork")
        self.assertNotEqual(result["manifest"],self.source)
        self.assertEqual((self.root/self.source).read_bytes(),before)
        self.assertEqual(w.world_hash(self.source),old)
        np.testing.assert_array_equal(w.load_world(result["manifest"])[1],self.heights)

    def test_empty_overlay_serializes_required_runtime_ops_array(self):
        path=(self.root/self.source).parent/"manifest_edits.json"
        edits=json.loads(path.read_text())
        self.assertEqual(edits["ops"],[])
        del edits["ops"];path.write_text(json.dumps(edits))
        with self.assertRaisesRegex(ValueError,"ops must be an array"):w.load_world(self.source)

    def test_terrain_mutation_invalidates_stale_client(self):
        old=w.world_hash(self.source)
        np.save((self.root/self.source).parent/"heights.npy",self.heights+1)
        with self.assertRaisesRegex(ValueError,"revision conflict"):
            w.apply_transaction(self.source,old,"stale",[])

    def test_stable_identity_preserves_unchanged_object_through_delete(self):
        current=w.inspect_world(self.source);remove,keep=[p["id"] for p in current["placements"]]
        result=w.apply_transaction(self.source,current["revision_hash"],"edit",[{"op":"remove","id":remove},{"op":"move","id":keep,"position_m":[8,1,3],"heading_deg":90,"scale":[1,1,1]}])
        out=w.inspect_world(result["manifest"])
        self.assertEqual(out["placements"][0]["id"],keep)
        self.assertEqual(out["placements"][0]["transform"][:3],[8,1,3])
        self.assertEqual(w.inspect_world(self.source)["placements_total"],2)

    def test_invalid_late_operation_publishes_nothing(self):
        baseline=list((self.root/w.REVISION_ROOT).rglob("manifest.v1.json"))
        with self.assertRaises(ValueError):
            w.apply_transaction(self.source,w.world_hash(self.source),"failed",[{"op":"sculpt","center_m":[0,0,0],"radius_m":3,"amount_m":1},{"op":"remove","id":"missing"}])
        self.assertEqual(list((self.root/w.REVISION_ROOT).rglob("manifest.v1.json")),baseline)
        np.testing.assert_array_equal(w.load_world(self.source)[1],self.heights)

    def test_scene_rigid_transform_matches_props_sources_and_socket(self):
        manifest=copy.deepcopy(self.value);manifest["placement_ids"]=w.placement_ids(manifest)
        edits={};w.add_scene(manifest,self.heights,edits,self.scene(),"scene.json",[4,3,0],90,"s1",diagnostic=True)
        np.testing.assert_allclose(manifest["placements"][-1][1:4],[4,4,-2],atol=1e-6)
        np.testing.assert_allclose(edits["sound_emitters"][0]["position_m"],[4,4,-2],atol=1e-6)
        np.testing.assert_allclose(edits["authored_lights"][0]["position"],[4,5,-2],atol=1e-6)
        np.testing.assert_allclose(manifest["authoring"]["scene_instances"][0]["sockets"][0]["position_m"],[8,4.65,0],atol=1e-6)
        self.assertEqual(edits["sound_emitters"][0]["id"],"s1_room")
        self.assertEqual(w.sample_height(self.heights,2,4,0),3)

    def test_exact_scene_requires_exact_campaign_anchor(self):
        scene=self.scene();p=scene["metadata"]["provenance"];p.update(placement_class="exact_landmark",exact_anchor={"act_id":1,"landmark_id":"grain","x":900,"z":1100,"tolerance_m":2})
        with self.assertRaisesRegex(ValueError,"exact landmark"):
            w.add_scene(self.value,self.heights,{},scene,"scene.json",[0,0,0],0,"s1")

    def test_scene_cannot_change_missing_source_into_placeholder(self):
        scene=self.scene();scene["items"][0]["asset"]="assets/missing.glb"
        with self.assertRaisesRegex(ValueError,"missing source"):w.validate_scene(scene)

    def test_path_escape_and_nonphysical_source_kind_rejected(self):
        for path in ("../outside.glb","/tmp/file.glb","assets/../x.glb"):
            with self.assertRaises(ValueError):w.repo_path(path)
        scene=self.scene();scene["environment"]["authored_lights"][0]["kind"]="area"
        with self.assertRaisesRegex(ValueError,"point lights"):w.validate_scene(scene)

    def test_apron_does_not_admit_wholly_outside_core(self):
        with self.assertRaisesRegex(ValueError,"core lies outside"):
            w.stamp_terrain(self.heights,2,self.scene()["terrain"],[21,0,0],0)

    def test_mcp_is_readable_and_never_accepts_shell_tool(self):
        response=editor_mcp.handle({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"world_inspect","arguments":{"manifest":self.source}}})
        self.assertFalse(response["result"]["isError"])
        self.assertEqual(response["result"]["structuredContent"]["placements_total"],2)
        failed=editor_mcp.handle({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"shell","arguments":{"cmd":"anything"}}})
        self.assertTrue(failed["result"]["isError"])

    def test_failed_publication_stays_pending(self):
        invalid=copy.deepcopy(self.value);invalid["placements"][0][5]=-1
        with self.assertRaises(ValueError):w.publish("invalid",invalid,self.heights,{})
        directories=list((self.root/w.REVISION_ROOT/"invalid").iterdir())
        self.assertTrue(all(p.name.startswith(".pending_") for p in directories))


if __name__=="__main__":unittest.main()
