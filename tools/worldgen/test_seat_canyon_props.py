import unittest
import numpy as np
from seat_canyon_props import Ground, rotation, seat

class SeatingTests(unittest.TestCase):
    def test_triangle_height_matches_twisted_drawn_cell(self):
        ground=Ground(np.array([[0.,1.],[2.,7.]]),1.)
        self.assertAlmostEqual(float(ground(-.25,-.25)),.75)
        self.assertAlmostEqual(float(ground(.25,.25)),4.25)

    def test_sloped_plane_fits_wreck_normal_and_contacts_at_any_heading(self):
        points=np.array([[x,0.,z] for x in (-1.,1.) for z in (-2.,0.,2.)])
        ground=lambda x,z: .22*x-.18*z+3.
        expected=np.array([-.22,1.,.18]);expected/=np.linalg.norm(expected)
        for yaw in (0.,37.,90.,163.,281.):
            p,rec=seat(points,[0,12.,ground(12.,5.),5.,yaw,1.],ground)
            r=rotation(yaw,*p[6]);np.testing.assert_allclose(r@np.array([0.,1.,0.]),expected,atol=1e-7)
            posed=points@r.T+np.array(p[1:4]);clearance=posed[:,1]-ground(posed[:,0],posed[:,2])
            np.testing.assert_allclose(clearance,-.035,atol=1e-6)

    def test_wall_keeps_vertical_and_seats_entire_base(self):
        points=np.array([[x,y,z] for x in (-2.,2.) for y in (0.,3.) for z in (-.2,.2)])
        ground=lambda x,z: .25*x+.12*z
        p,rec=seat(points,[0,0.,0.,0.,38.,[1.,1.,1.]],ground,upright=True)
        self.assertEqual(p[6],[0.,0.]);self.assertLessEqual(rec['clearance_range_m'][1],-.03499)
        self.assertGreater(rec['remaining_wall_height_m'],1.0)

if __name__=='__main__':unittest.main()
