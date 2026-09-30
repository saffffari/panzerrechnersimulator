//! Source telescope registration. The supplied tube/lens are moved as complete assemblies;
//! their missing elevation linkage is reconstructed at the gun trunnion without barrel recoil.
//! This does not supply a historical TZF prescription or a new magnification model.
use crate::gltf_load::{load_glb_partitioned_roots, CpuMesh};
use crate::tank::{TIGER_PARTS, TIGER_SIGHT_EYEPIECE};
use glam::{Mat4, Vec3};

pub const TUBE_ROOT: &str = "Turret_0003.232";
const LENS_ROOT: &str = "gunner sight lens";
pub const FIRE_FRAME: usize = 8;
/// Front glass surface sits just proud of the authored eyecup rim, not deep inside the tube.
pub const EYEPIECE_GLASS_Z: f32 = 0.5328068;

pub struct Source {
    /// Tube and rear lens are gun-local, and use the laid gun frame (no recoil translation).
    pub parts: [CpuMesh; 2],
    /// Other source slit glass retains its original turret parent.
    pub turret_glass: CpuMesh,
}

fn gun_local(mesh: CpuMesh) -> CpuMesh {
    mesh.transformed(Mat4::from_translation(-TIGER_PARTS.gun_trunnion))
}

/// Select the complete authored front disk without replacing or reordering the closed lens.
/// Source triangles 0/1 happen to be tiny wedges on its opposite face, not an entrance quad.
fn entrance_indices(lens: &CpuMesh) -> std::ops::Range<u32> {
    let front: Vec<_> = lens
        .indices
        .chunks_exact(3)
        .enumerate()
        .filter_map(|(i, tri)| {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(lens.vertices[tri[k] as usize].pos));
            ([a, b, c]
                .iter()
                .all(|p| (p.z - lens.min.z).abs() < 0.000002)
                && (b - a).cross(c - a).z < 0.0)
                .then_some(i as u32)
        })
        .collect();
    assert_eq!(
        front.len(),
        40,
        "source gunner lens entrance tessellation changed"
    );
    assert!(
        front.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "gunner lens entrance must be a contiguous source index range"
    );
    front[0] * 3..(front[front.len() - 1] + 1) * 3
}

pub fn load(tube: CpuMesh) -> anyhow::Result<Source> {
    let mut pages=load_glb_partitioned_roots(
        "assets/tanks/tiger/baked/tiger_port_turret.glb", &[LENS_ROOT])?;
    let lens=pages.pop().expect("requested gunner lens");
    anyhow::ensure!(tube.indices.len()==5020*3 && lens.indices.len()==164*3,
        "gunner sight source topology changed; recheck pupil and mantlet bore");
    anyhow::ensure!(lens.center().distance(TIGER_SIGHT_EYEPIECE)<0.00001
        && (tube.min.z-0.5333068).abs()<0.00001
        && (tube.max.z-1.4689443).abs()<0.00001,
        "gunner sight source registration changed");
    let rim_offset = EYEPIECE_GLASS_Z - lens.min.z;
    let lens = lens.transformed(Mat4::from_translation(Vec3::Z * rim_offset));
    let _ = entrance_indices(&lens);
    Ok(Source {
        parts:[gun_local(crate::tank::repaint_german_grey(tube)),gun_local(lens)],
        turret_glass:pages.pop().expect("remaining turret slit glass"),
    })
}

/// Use the same linear, nearest-sampled exterior view and glass glow as the other optical panes.
/// The circular source lens supplies the silhouette; the corners only fit its capture frustum.
pub fn raster_optic_spec(lens: &CpuMesh, mesh: usize) -> crate::renderer::RasterOpticSpec {
    let z = lens.min.z;
    crate::renderer::RasterOpticSpec {
        mesh,
        entry_indices: entrance_indices(lens),
        entry_corners: [Vec3::new(lens.min.x,lens.min.y,z),
            Vec3::new(lens.max.x,lens.min.y,z), Vec3::new(lens.max.x,lens.max.y,z),
            Vec3::new(lens.min.x,lens.max.y,z)],
        entry_outward: Vec3::Z,
        objective: crate::tank::TIGER_SIGHT_OBJECTIVE,
        objective_outward: Vec3::Z,
        preserve_ray_direction: false,
    }
}

pub fn interior_camera(gun_model: Mat4) -> (Vec3,Vec3,f32) {
    // Step back slightly so the turret dial remains visible beside the optic circle.
    // Aim at the physical pupil; this framing is gameplay, not historical eye relief.
    let eye=Vec3::new(TIGER_SIGHT_EYEPIECE.x-0.03,TIGER_SIGHT_EYEPIECE.y+0.02,0.15)
        -TIGER_PARTS.gun_trunnion;
    let pupil=Vec3::new(TIGER_SIGHT_EYEPIECE.x,TIGER_SIGHT_EYEPIECE.y,EYEPIECE_GLASS_Z)
        -TIGER_PARTS.gun_trunnion;
    (gun_model.transform_point3(eye),gun_model.transform_vector3(pupil-eye).normalize(),
        66_f32.to_radians())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf_load::load_glb;
    use crate::tank::TIGER_SIGHT_OBJECTIVE;

    fn first_hit(mesh:&CpuMesh,origin:Vec3,direction:Vec3)->Option<f32> {
        let mut nearest=f32::INFINITY;
        for tri in mesh.indices.chunks_exact(3) {
            let [a,b,c]=[0,1,2].map(|i|Vec3::from(mesh.vertices[tri[i] as usize].pos));
            let e1=b-a;let e2=c-a;let h=direction.cross(e2);let det=e1.dot(h);
            if det.abs()<1e-10 {continue;}
            let s=origin-a;let u=s.dot(h)/det;let q=s.cross(e1);
            let v=direction.dot(q)/det;let t=e2.dot(q)/det;
            if u>=0.0 && v>=0.0 && u+v<=1.0 && t>0.00003 {nearest=nearest.min(t);}
        }
        nearest.is_finite().then_some(nearest)
    }

    #[test]
    fn circular_entrance_draws_the_complete_source_front_disk() {
        let mut pages = load_glb_partitioned_roots(
            "assets/tanks/tiger/baked/tiger_port_turret.glb",
            &[LENS_ROOT],
        )
        .unwrap();
        let original = pages.pop().unwrap();
        let original_indices = original.indices.clone();
        let shift = Vec3::Z * (EYEPIECE_GLASS_Z - original.min.z);
        let lens = gun_local(original.transformed(Mat4::from_translation(shift)));
        let spec = raster_optic_spec(&lens, 17);
        assert_eq!(
            lens.indices, original_indices,
            "source closed volume must remain intact"
        );
        assert_eq!(lens.indices.len(), 164 * 3);
        assert_eq!(spec.entry_indices, 120..240);
        let mut disk = lens.clone();
        disk.indices = lens.indices
            [spec.entry_indices.start as usize..spec.entry_indices.end as usize]
            .to_vec();
        let center = Vec3::new(lens.center().x, lens.center().y, lens.min.z);
        let radii = (lens.max - lens.min) * 0.5;
        let mut area = 0.0;
        for tri in disk.indices.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(disk.vertices[tri[k] as usize].pos));
            assert!([a, b, c].iter().all(|p| (p.z - center.z).abs() < 0.000002));
            let cross = (b - a).cross(c - a);
            assert!(cross.z < 0.0, "front entrance must face the observer");
            area += cross.length() * 0.5;
        }
        let disk_area = std::f32::consts::PI * radii.x * radii.y;
        assert!(
            area / disk_area > 0.99 && area / disk_area < 1.001,
            "the source tessellation must cover the full circular pupil, not isolated wedges"
        );
        for radius in [0.0, 0.3, 0.65, 0.92] {
            for k in 0..32 {
                let angle = k as f32 * std::f32::consts::TAU / 32.0;
                let point =
                    center + Vec3::new(angle.cos() * radii.x, angle.sin() * radii.y, 0.0) * radius;
                let distance = first_hit(&disk, point - Vec3::Z * 0.1, Vec3::Z)
                    .expect("every inner pupil ray must see the live entrance surface");
                assert!((distance - 0.1).abs() < 0.000002);
            }
        }
        // Regression witness: the old fixed six-index draw contains only rear edge wedges.
        let mut old = lens.clone();
        old.indices.truncate(6);
        assert!(first_hit(&old, center - Vec3::Z * 0.1, Vec3::Z).is_none());
    }

    #[test]
    fn complete_source_sight_rebases_without_losing_vertices_and_follows_elevation() {
        let mut source=load_glb_partitioned_roots("assets/tanks/tiger/baked/tiger_turret.glb",&[TUBE_ROOT]).unwrap();
        let original=source.pop().unwrap();
        let original_positions:Vec<_>=original.vertices.iter().map(|v|Vec3::from(v.pos)).collect();
        let sight=load(original).unwrap();
        let tube_rim = sight.parts[0].min.z;
        let glass_front = sight.parts[1].min.z;
        assert!((tube_rim - glass_front - 0.0005).abs() < 0.000002,
            "lens must sit at the eyecup lip rather than recessed in the cylinder");
        let optic = raster_optic_spec(&sight.parts[1], 17);
        assert_eq!(optic.mesh, 17);
        assert!(optic.entry_corners.iter().all(|p| (p.z - glass_front).abs() < 0.000002));
        assert!(optic.objective.abs_diff_eq(TIGER_SIGHT_OBJECTIVE, 0.000001));
        assert_eq!(optic.entry_outward, Vec3::Z);
        assert_eq!(sight.parts[0].vertices.len(),original_positions.len());
        for (vertex,original) in sight.parts[0].vertices.iter().zip(original_positions) {
            assert!(Vec3::from(vertex.pos).abs_diff_eq(original-TIGER_PARTS.gun_trunnion,0.000002));
        }
        let contacts=super::super::surface_fire::CabinFire::new(&[(FIRE_FRAME,&sight.parts[0])]);
        let tri=&sight.parts[0].indices[..3];
        let [a,b,c]=[0,1,2].map(|i|Vec3::from(sight.parts[0].vertices[tri[i] as usize].pos));
        let point=(a+b+c)/3.0;let normal=(b-a).cross(c-a).normalize();
        for pitch in [-0.1_f32,0.0,0.2] {
            let turret=Mat4::from_translation(Vec3::new(15.0,0.524,-30.0))*Mat4::from_rotation_y(0.43);
            let gm=turret*Mat4::from_translation(TIGER_PARTS.gun_trunnion)*Mat4::from_rotation_x(-pitch);
            let (eye,forward,_)=interior_camera(gm);
            let lens=&sight.parts[1];
            let pupil=gm.transform_point3(Vec3::new(lens.center().x,lens.center().y,lens.min.z));
            assert!((pupil-eye).cross(forward).length()<0.00001);
            assert!((pupil-eye).dot(forward)>0.38 && (pupil-eye).dot(forward)<0.40);
            let mut models=[Mat4::IDENTITY;super::super::surface_fire::FRAMES];
            models[FIRE_FRAME]=gm;
            let contact=contacts.sweep(gm.transform_point3(point+normal*0.0001),
                gm.transform_point3(point-normal*0.0001),&models).expect("articulated source contact");
            assert_eq!(contact.frame,FIRE_FRAME);
            assert!(contact.point.distance(point)<0.00002,
                "collision/contact must follow the same no-recoil sight frame as rendering");
        }
    }

    #[test]
    fn objective_uses_real_front_bore_with_natural_barrel_occlusion() {
        let gun=load_glb("assets/tanks/tiger/baked/tiger_gun.glb").unwrap();
        // The bore remains open from its rear annulus to the source-registered front camera.
        let annulus=Vec3::new(0.498174,1.998528,1.433916)-TIGER_PARTS.gun_trunnion;
        let to_objective=TIGER_SIGHT_OBJECTIVE-annulus;
        assert!(first_hit(&gun,annulus,to_objective.normalize())
            .is_none_or(|distance|distance>to_objective.length()));
        assert!(first_hit(&gun,TIGER_SIGHT_OBJECTIVE,Vec3::Z).is_none());
        let tan=(25_f32.to_radians()*0.5).tan();
        let mut blocked=0;
        for y in [-1.0,0.0,1.0] {
            for x in [-1.0,0.0,1.0] {
                let direction=Vec3::new(x*(16.0/9.0)*tan,y*tan,1.0).normalize();
                if let Some(distance)=first_hit(&gun,TIGER_SIGHT_OBJECTIVE,direction) {
                    assert!(x<0.0 && y<=0.0 && distance>0.8,
                        "unexpected source obstruction ({x},{y}) at {distance}m");
                    blocked+=1;
                }
            }
        }
        assert_eq!(blocked,2,"two left/lower field rays should naturally meet the barrel");
    }
}
