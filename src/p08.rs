//! Authored P08 moving parts. All pivots are measured in the unchanged local GLB; no barrel,
//! toggle, or magazine is resized during cycling. Source redistribution remains review-required.
use crate::gltf_load::CpuMesh;
use glam::{Mat4, Quat, Vec3};

pub const ASSET: &str = "assets/flavor/luger/baked/luger_runtime.glb";
const OFFSET: Vec3 = Vec3::new(0.000162863, -0.016070827, 0.0);
const REAR: Vec3 = Vec3::new(0.0, 0.062194296, -0.093497746);
const KNEE: Vec3 = Vec3::new(0.0, 0.061163053, -0.062189594);
const BOLT: Vec3 = Vec3::new(0.0, 0.062249073, -0.021320468);
const ROOTS: [&str; 4] = ["Cube.007", "Cylinder.010", "Cylinder.012", "Circle.001"];

/// A rigid 40 cm hold with the unchanged source sight line at the camera centre.
/// The actual front post and rear notch are the aiming reference. The same origin
/// drives the complete pistol and its physical muzzle in both on-foot and mounted paths;
/// no screen-space move, altered source geometry or separate projectile origin is used.
pub fn aimed_origin(eye: Vec3, forward: Vec3, rotation: Quat) -> Vec3 {
    const SIGHT_HEIGHT_M: f32 = 0.069_835;
    eye + forward * 0.40 - rotation * Vec3::new(0.0, SIGHT_HEIGHT_M, 0.0)
}


/// Frame/trigger, bolt, forward link, rear link/knobs, actual magazine, barrel/front sight.
pub fn load() -> anyhow::Result<[CpuMesh; 6]> {
    let mut parts: Vec<_> = crate::gltf_load::load_glb_partitioned_roots(ASSET, &ROOTS)?
        .into_iter().map(|part| part.transformed(Mat4::from_translation(OFFSET))).collect();
    anyhow::ensure!(parts.len() == 5 && parts.iter().all(|p| !p.indices.is_empty()), "incomplete authored P08 parts");
    let body = &mut parts[0];
    let mut barrel = body.clone();
    barrel.indices.clear();
    let mut fixed = Vec::new();
    // Weld duplicate position records for connectivity only (UV seams remain unchanged in the
    // renderer). Select whole disconnected source components, never a plane-cut strip of frame.
    let mut parents: Vec<usize>=(0..body.vertices.len()).collect();
    fn root(parents: &mut [usize], i: usize) -> usize {
        if parents[i]!=i { let next=parents[i]; parents[i]=root(parents,next); }
        parents[i]
    }
    let mut positions=std::collections::HashMap::new();
    for (i,v) in body.vertices.iter().enumerate() {
        let key=v.pos.map(f32::to_bits);
        if let Some(&previous)=positions.get(&key) {
            let a=root(&mut parents,i);let b=root(&mut parents,previous);parents[a]=b;
        } else {positions.insert(key,i);}
    }
    for tri in body.indices.chunks_exact(3) {
        let a=root(&mut parents,tri[0] as usize);
        for &i in &tri[1..] {let b=root(&mut parents,i as usize);parents[b]=a;}
    }
    let mut component_min=std::collections::HashMap::<usize,Vec3>::new();
    for &i in &body.indices {
        let key=root(&mut parents,i as usize);let p=Vec3::from_array(body.vertices[i as usize].pos);
        component_min.entry(key).and_modify(|lo|*lo=lo.min(p)).or_insert(p);
    }
    for tri in body.indices.chunks_exact(3) {
        let lo=component_min[&root(&mut parents,tri[0] as usize)];
        if lo.z>=0.0310675 && lo.y>0.040 {barrel.indices.extend_from_slice(tri);}
        else {fixed.extend_from_slice(tri);}
    }
    anyhow::ensure!(!barrel.indices.is_empty(), "authored barrel component missing");
    body.indices = fixed;
    for mesh in [body, &mut barrel] {
        mesh.min = Vec3::splat(f32::INFINITY); mesh.max = Vec3::splat(f32::NEG_INFINITY);
        for &i in &mesh.indices {
            let p=Vec3::from_array(mesh.vertices[i as usize].pos);
            mesh.min=mesh.min.min(p); mesh.max=mesh.max.max(p);
        }
    }
    parts.push(barrel);
    Ok(parts.try_into().ok().expect("six P08 source partitions"))
}

fn smooth(a: f32, b: f32, p: f32) -> f32 {
    let t=((p-a)/(b-a)).clamp(0.0,1.0); t*t*(3.0-2.0*t)
}

/// Present the actual magazine and open toggle inside the view before extraction starts.
/// This is a rigid hold adjustment, shared by hip and aimed reloads; the authored dimensions,
/// magazine travel and action timing stay unchanged. The previous downward dip hid the mag.
fn reload_pose(camera: &crate::camera::Camera, pos: Vec3, rot: Quat, p: f32) -> (Vec3, Quat) {
    let amount = smooth(0.0, 0.08, p) * (1.0 - smooth(0.93, 1.0, p));
    let forward = camera.forward();
    let right = camera.right();
    let up = right.cross(forward);
    let presentation = camera.pos + forward * 0.52 + right * 0.075 + up * 0.060;
    (pos.lerp(presentation, amount), Quat::from_axis_angle(forward, 0.32 * amount) * rot)
}

fn magazine_translation(p: f32) -> Vec3 {
    let extraction=if p<0.30 {smooth(0.08,0.30,p)} else {1.0-smooth(0.43,0.70,p)};
    Vec3::new(0.0,-0.89,-0.455)*(0.145*extraction)
}

/// Rigid two-link kinematics: the knee buckles UP while the bolt remains on its straight rail.
/// The original link endpoints coincide throughout the animation, including the hold-open pose.
pub fn action_models(open: f32, recoil: f32) -> [Mat4; 4] {
    let angle=-61.0_f32.to_radians()*open.clamp(0.0,1.07);
    let rear_rot=Quat::from_rotation_x(angle);
    let knee=REAR+rear_rot*(KNEE-REAR);
    let length=(BOLT-KNEE).length();
    let dy=BOLT.y-knee.y;
    let bolt=Vec3::new(0.0,BOLT.y,knee.z+(length*length-dy*dy).max(0.0).sqrt());
    let front_rot=Quat::from_rotation_arc((BOLT-KNEE).normalize(),(bolt-knee).normalize());
    let short=Mat4::from_translation(Vec3::NEG_Z*(0.003*recoil.clamp(0.0,1.0)));
    [
        short*Mat4::from_translation(bolt-BOLT),
        short*Mat4::from_translation(knee)*Mat4::from_quat(front_rot)*Mat4::from_translation(-KNEE),
        short*Mat4::from_translation(REAR)*Mat4::from_quat(rear_rot)*Mat4::from_translation(-REAR),
        short,
    ]
}

pub fn action_phase(foot: &crate::foot::FootSoldier) -> (f32,f32) {
    let cycle=(foot.p08_cycle_t/0.12).clamp(0.0,1.0);
    let elapsed=1.0-cycle;
    let short=smooth(0.0,0.15,elapsed)*(1.0-smooth(0.74,1.0,elapsed));
    if foot.reload_t>0.0 && foot.p08_reload_empty {
        let p=1.0-foot.reload_t/crate::foot::RELOAD_S;
        // An inserted full magazine does not release the hold-open. The last beat visibly
        // pulls the knobs slightly farther back, then lets the recoil spring close the action.
        let pull=0.055*smooth(0.81,0.86,p)*(1.0-smooth(0.86,0.89,p));
        return ((1.0+pull)*(1.0-smooth(0.89,0.93,p)),0.0);
    }
    if !foot.p08_chambered {
        return (if cycle>0.0 {smooth(0.10,0.42,elapsed)} else {1.0},short);
    }
    let open=smooth(0.10,0.42,elapsed)*(1.0-smooth(0.55,1.0,elapsed));
    (open,short)
}

impl super::Game {
    pub(super) fn update_p08_reload(&mut self, dt: f32, request: bool) {
        if request { self.foot.begin_p08_reload(); }
        let events=self.foot.advance_p08_reload(dt);
        if let Some(audio)=&self.audio {
            for (bit,key,gain) in [(1,"p08_mag_out",0.65),(2,"p08_mag_in",0.72),(4,"p08_toggle_pull",0.60),(8,"p08_toggle_close",0.72)] {
                if events&bit!=0 { audio.play_control(key,gain); }
            }
        }
    }

    pub(super) fn hide_p08(&self, r: &mut crate::renderer::Renderer) {
        for slot in [self.slots.luger,self.slots.luger_mag].into_iter().chain(self.slots.luger_action) {
            r.set_visible(slot,false);
        }
    }

    pub(super) fn draw_p08(&self, r: &mut crate::renderer::Renderer, mut pos: Vec3, mut rot: Quat) {
        let p=if self.foot.reload_t>0.0 {1.0-self.foot.reload_t/crate::foot::RELOAD_S} else {1.0};
        if self.foot.reload_t>0.0 {
            (pos, rot) = reload_pose(&r.camera, pos, rot, p);
        }
        r.reticle=false;
        let base=Mat4::from_rotation_translation(rot,pos);
        if let Some(arms)=&self.first_person {arms.hold_pistol(base,base*Mat4::from_translation(magazine_translation(p)));}
        r.set_visible(self.slots.luger,true);r.set_model(self.slots.luger,base);
        let (open,recoil)=action_phase(&self.foot);
        for (slot,model) in self.slots.luger_action.into_iter().zip(action_models(open,recoil)) {
            r.set_visible(slot,true);r.set_model(slot,base*model);
        }
        // Source magazine is already seated in gun-local coordinates. Withdraw and insert along
        // its measured slanted grip axis; keep its actual scale and alignment at both endpoints.
        r.set_visible(self.slots.luger_mag, !(0.30..0.43).contains(&p));
        r.set_model(self.slots.luger_mag,base*Mat4::from_translation(magazine_translation(p)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_toggle_joints_remain_connected_and_bolt_moves_on_rail() {
        for open in [0.0,0.2,0.5,0.8,1.0,1.055] {
            let [bolt,front,rear,_]=action_models(open,0.0);
            let k1=front.transform_point3(KNEE);let k2=rear.transform_point3(KNEE);
            assert!(k1.distance(k2)<1e-6);
            assert!(front.transform_point3(BOLT).distance(bolt.transform_point3(BOLT))<1e-6);
            assert!((bolt.transform_point3(BOLT).y-BOLT.y).abs()<1e-6);
            if open>0.0 {assert!(k1.y>KNEE.y);assert!(bolt.transform_point3(BOLT).z<BOLT.z);}
            for m in [bolt,front,rear] {assert!((m.determinant()-1.0).abs()<1e-5);}
        }
    }
    #[test]
    fn source_partition_preserves_all_triangles_and_real_magazine() {
        if !crate::resource::exists(ASSET) {return;}
        let original=crate::gltf_load::load_glb(ASSET).unwrap();let parts=load().unwrap();
        assert_eq!(parts.iter().map(|p|p.indices.len()).sum::<usize>(),original.indices.len());
        assert_eq!(parts[5].indices.len(),412*3,"complete authored barrel and front post");
        let mag=&parts[4];assert!((mag.max.y-mag.min.y-0.119).abs()<0.001);
        assert!((mag.max.x-mag.min.x-0.0204932).abs()<0.001);
    }
    #[test]
    fn reload_keeps_complete_source_magazine_and_open_toggle_in_view() {
        if !crate::resource::exists(ASSET) { return; }
        let parts = load().unwrap();
        for (aiming, fov) in [(false, 68.0_f32), (true, 45.0_f32)] {
            for aspect in [4.0 / 3.0, 16.0 / 9.0, 21.0 / 9.0] {
                for (yaw, pitch) in [(0.0, 0.0), (1.7, 0.6), (-2.4, -0.8)] {
                    let mut camera = crate::camera::Camera::new(aspect);
                    camera.yaw = yaw; camera.pitch = pitch; camera.fov_y = fov.to_radians();
                    let forward = camera.forward(); let right = camera.right();
                    let up = right.cross(forward);
                    let rot = crate::foot::held_weapon_rotation(forward, up);
                    let pos = if aiming { aimed_origin(camera.pos, forward, rot) }
                        else { camera.pos + forward * 0.35 + right * 0.16 - up * 0.20 };
                    for p in [0.08, 0.15, 0.299, 0.43, 0.55, 0.70, 0.86, 0.93] {
                        let (pos, rot) = reload_pose(&camera, pos, rot, p);
                        let base = Mat4::from_rotation_translation(rot, pos);
                        // Include the largest real hold-open pull throughout the extraction as
                        // a conservative envelope, as well as every point of the original mag.
                        let [bolt, front, rear, barrel] = action_models(1.055, 0.0);
                        let transforms = [Mat4::IDENTITY, bolt, front, rear,
                            Mat4::from_translation(magazine_translation(p)), barrel];
                        for (part, local) in parts.iter().zip(transforms) {
                            let clip = camera.view_proj() * base * local;
                            for &i in &part.indices {
                                let p = clip * Vec3::from(part.vertices[i as usize].pos).extend(1.0);
                                let ndc = p.truncate() / p.w;
                                assert!(p.w > camera.znear && ndc.is_finite());
                                assert!(ndc.x.abs() < 0.93 && ndc.y.abs() < 0.93,
                                    "complete reload geometry must retain a screen margin: {ndc:?}");
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn iron_sight_hold_preserves_native_notch_and_front_post() {
        if !crate::resource::exists(ASSET) { return; }
        let parts = load().unwrap();
        // Trace the actual source triangles at the native pixel centres, including the
        // rear V-notch, toggle and front post. Both windings count as conservative occlusion.
        let clear = |origin: Vec3, direction: Vec3| -> bool {
            !parts.iter().any(|mesh| mesh.indices.chunks_exact(3).any(|tri| {
                let a = Vec3::from_array(mesh.vertices[tri[0] as usize].pos);
                let e1 = Vec3::from_array(mesh.vertices[tri[1] as usize].pos) - a;
                let e2 = Vec3::from_array(mesh.vertices[tri[2] as usize].pos) - a;
                let h = direction.cross(e2);
                let det = e1.dot(h);
                if det.abs() < 1e-12 { return false; }
                let inv = det.recip();
                let s = origin - a;
                let u = inv * s.dot(h);
                let q = s.cross(e1);
                let v = inv * direction.dot(q);
                let t = inv * e2.dot(q);
                u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0
            }))
        };
        let eye = -aimed_origin(Vec3::ZERO, Vec3::Z, Quat::IDENTITY);
        let sight_eye=eye;
        let focal_pixels = 180.0 / 22.5_f32.to_radians().tan();
        let ray = |x: f32| Vec3::new(x / focal_pixels, -0.5 / focal_pixels, 1.0);
        for side in [-1.0, 1.0] {
            assert!(clear(sight_eye, ray(side * 1.5)), "gap beside front post must survive native pixels");
            assert!(!clear(sight_eye, ray(side * 0.5)), "retain the real front post");
            assert!(!clear(sight_eye, ray(side * 5.5)), "retain the real rear sight shoulders");
            let old_eye = Vec3::new(0.0, 0.069_835, -0.52);
            assert!(!clear(old_eye, ray(side * 1.5)), "regression reproduces former closed notch");
        }
    }

    #[test]
    fn sight_hold_keeps_source_muzzle_and_bore_at_any_orientation() {
        for (yaw,pitch) in [(0.0_f32,0.0_f32),(1.7,0.6),(-2.4,-0.8)] {
            let rotation=Quat::from_rotation_y(yaw)*Quat::from_rotation_x(pitch);
            let forward=rotation*Vec3::Z;
            let eye=Vec3::new(12.0,3.0,-8.0);
            let old=eye+forward*0.40-rotation*Vec3::new(0.0,0.069_835,0.0);
            let new=aimed_origin(eye,forward,rotation);
            let muzzle_local=Vec3::new(0.0,0.052_598_275,0.110);
            let before=old+rotation*muzzle_local;let after=new+rotation*muzzle_local;
            assert!((after-before).length()<2e-6);
            assert!(((after-eye).dot(forward)-0.510).abs()<2e-6);
            assert!((rotation*Vec3::Z-forward).length()<1e-6);
        }
    }

}
