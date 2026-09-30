#!/usr/bin/env python3
"""Geometry and authoring-preservation checks for the Canyon Run generator."""
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import numpy as np
import gaea_canyon_run as ga

class CanyonRunTests(unittest.TestCase):
    def test_samples_the_drawn_triangle_not_bilinear_twist(self):
        grid=ga.SurfaceGrid(2,1)
        H=np.array([[0.,0.],[0.,4.]])
        self.assertEqual(grid.sample(H,-.25,-.25),0.)
        self.assertEqual(grid.sample(H,.25,.25),2.)

    def test_hand_edits_without_ops_are_preserved_before_regeneration(self):
        with tempfile.TemporaryDirectory() as tmp:
            dest=Path(tmp)
            for name in ga.cc.OUTPUT_FILES:
                (dest/name).write_text('{}')
            hashes={name:ga.cc.sha256_file(dest/name) for name in ga.cc.OUTPUT_FILES}
            (dest/'canyon-course.receipt.json').write_text(json.dumps({'outputs':hashes}))
            original=json.dumps({'ops':[],'building_instances':[{'id':'user-building'}]})
            (dest/'manifest_edits.json').write_text(original)
            with patch.object(ga,'build') as build:
                with self.assertRaisesRegex(ValueError,'Preserving hand-edited'):
                    ga.write(dest=dest)
                build.assert_not_called()
            self.assertEqual((dest/'manifest_edits.json').read_text(),original)

    @unittest.skipUnless((ga.DEST/'manifest.v1.json').exists(),'generate the Gaea workshop first')
    def test_workshop_has_complete_provenance_and_a_clear_travel_strip(self):
        rec=json.loads((ga.DEST/'canyon-course.receipt.json').read_text())
        self.assertEqual(rec['source_height_sha256'],ga.cc.sha256_file(ga.SOURCE/'Export.png'))
        for name,h in rec['outputs'].items():
            self.assertEqual(ga.cc.sha256_file(ga.DEST/name),h)
        e=json.loads((ga.DEST/'manifest_edits.json').read_text())
        self.assertEqual(len(e['canyon_course']['design']),7)
        self.assertEqual(len(e['canyon_course']['gun_zones']),3)
        self.assertGreaterEqual(len(e['canyon_course']['garrison']),12)
        self.assertGreaterEqual(len(e['building_instances']),50)
        H=np.load(ga.DEST/'heights.npy')
        self.assertTrue(np.isfinite(H).all())
        r=np.array(e['canyon_course']['route'])
        route=type('Route',(),{})()
        route.x,route.z=r.T
        route.s=np.r_[0,np.cumsum(np.linalg.norm(np.diff(r,axis=0),axis=1))]
        route.L=route.s[-1]
        check=ga.validate_travel_surface(route,ga.SurfaceGrid(len(H),rec['runtime_spacing_m']),H)
        self.assertLess(check['max_along_degrees'],20)
        self.assertGreater(route.L,4500)
        self.assertLess(route.L,5500)

if __name__=='__main__':unittest.main()
