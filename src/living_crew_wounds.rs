//! Persistent resolved wounds; never promotes peripheral injury to a casualty.
use super::*;
const COUNT: usize = crew_physics::CREW_BODY_PART_COUNT;
#[derive(Clone)]
pub(super) struct Wound {
    pub casualty: CouchCrewCasualty,
    pub work: [f32; COUNT],
    pub missing: [bool; COUNT],
    pub pulverized: [bool; COUNT],
}
struct LoosePart {
    part: usize,
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
    floor: f32,
}
#[derive(Default)]
pub(super) struct LivingWounds {
    pub people: Vec<Wound>,
    loose: Vec<LoosePart>,
    pools: Option<[usize; COUNT]>,
    pub masks: std::cell::RefCell<std::collections::HashMap<usize, u16>>,
}
impl LivingWounds {
    pub fn allocate(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer) -> Self {
        let source=crate::gltf_load::load_glb("assets/flavor/Characters/german-ww2-soldier-cc0/baked/german_ww2_soldier_relaxed_static.glb").ok();
        let pools = source.map(|source| {
            let pages = partition_relaxed_crew_mesh(&source);
            std::array::from_fn(|i| {
                let mesh = pages[i].clone().unwrap_or_else(|| {
                    crew_segment_fallback_mesh(crew_physics::CrewBodyPart::ALL[i], [40, 40, 40])
                });
                let prototype = r.register_instanced_prototype(device, queue, &mesh);
                r.add_moving_instanced_pool(device, prototype, 55)
                    .expect("living severed part pool")
            })
        });
        Self {
            pools,
            ..Default::default()
        }
    }
    pub fn detach(
        &mut self,
        body: &CouchCrewBody,
        events: &[CrewPartDamageEvent],
        parent: Mat4,
        floor: f32,
    ) {
        let old = self.missing(body.casualty.identity);
        for e in events {
            let i = e.result.part.index();
            if !e.result.detach_joint || e.result.hide_mesh_part || old[i] || self.loose.len() >= 55
            {
                continue;
            }
            let volume = body.hit_volumes[i];
            let pos = parent.transform_point3(volume.center);
            let impulse = e
                .strongest_contact
                .map(|c| {
                    crew_physics::fragment_impulse(
                        parent.transform_vector3(c.local_velocity),
                        e.result.fragment_work_j,
                    )
                })
                .unwrap_or(Vec3::ZERO);
            self.loose.push(LoosePart {
                part: i,
                pos,
                rot: Quat::from_mat4(&parent) * volume.rotation,
                vel: impulse / e.result.part.mass_kg(),
                floor,
            });
        }
    }
    pub fn update_draw(&mut self, dt: f32, r: &mut Renderer) {
        let Some(pools) = self.pools else {
            return;
        };
        let dt = dt.clamp(0.0, 0.05);
        let mut mats: [Vec<Mat4>; COUNT] = std::array::from_fn(|_| Vec::new());
        for p in &mut self.loose {
            p.vel.y -= 9.81 * dt;
            p.pos += p.vel * dt;
            let radius = crew_physics::CrewBodyPart::ALL[p.part].half_extents().y;
            if p.pos.y < p.floor + radius {
                p.pos.y = p.floor + radius;
                p.vel.y = 0.0;
                p.vel.x *= (-6.0 * dt).exp();
                p.vel.z *= (-6.0 * dt).exp();
            }
            mats[p.part].push(
                Mat4::from_rotation_translation(p.rot, p.pos)
                    * Mat4::from_translation(
                        -crew_physics::CrewBodyPart::ALL[p.part].rest_center(),
                    ),
            );
        }
        for (pool, mats) in pools.into_iter().zip(&mats) {
            r.set_moving_instanced_mats(pool, mats);
        }
    }
    pub fn record(&mut self, casualty: CouchCrewCasualty, events: &[CrewPartDamageEvent]) {
        if events
            .iter()
            .all(|e| e.result.outcome == crew_damage::CrewTissueOutcome::None)
        {
            return;
        }
        let i = if let Some(i) = self
            .people
            .iter()
            .position(|p| p.casualty.peer_id == casualty.peer_id)
        {
            i
        } else {
            if self.people.len() >= 5 {
                return;
            }
            self.people.push(Wound {
                casualty,
                work: [0.0; COUNT],
                missing: [false; COUNT],
                pulverized: [false; COUNT],
            });
            self.people.len() - 1
        };
        let p = &mut self.people[i];
        for event in events {
            let i = event.result.part.index();
            p.work[i] += event.result.total_work_j.max(0.0);
            p.missing[i] |= event.result.detach_joint;
            p.pulverized[i] |= event.result.hide_mesh_part;
        }
        for (parent, child) in [(3, 4), (5, 6), (7, 8), (9, 10)] {
            if p.missing[parent] || p.pulverized[parent] {
                p.missing[child] = true;
            }
        }
    }
    pub fn missing(&self, identity: crate::lan::LanRole) -> [bool; COUNT] {
        self.people
            .iter()
            .find(|p| p.casualty.identity == identity)
            .map(|p| std::array::from_fn(|i| p.missing[i] || p.pulverized[i]))
            .unwrap_or([false; COUNT])
    }
}
/// Bone family ownership follows the imported rigs, not screen-space wound placement.
pub(super) fn bone_part(name: &str) -> usize {
    let left = name.starts_with("Left") || name.starts_with("L_");
    let right = name.starts_with("Right") || name.starts_with("R_");
    if name.contains("Head") || name.contains("Neck") || name.contains("Eyes") {
        return 2;
    }
    if left || right {
        let side = usize::from(right);
        if name.contains("UpLeg") || name.contains("_Leg_") {
            return 7 + side * 2;
        }
        if name.contains("Leg")
            || name.contains("Foot")
            || name.contains("Toe")
            || name.contains("Knee")
            || name.contains("Ankle")
            || name.contains("Heel")
        {
            return 8 + side * 2;
        }
        if name.contains("ForeArm")
            || name.contains("Elbow")
            || name.contains("Wrist")
            || name.contains("Hand")
            || name.contains("Finger")
            || name.contains("Thumb")
        {
            return 4 + side * 2;
        }
        if name.contains("Arm") || name.contains("Shoulder") {
            return 3 + side * 2;
        }
    }
    if name.contains("Spine") || name.contains("Chest") {
        1
    } else {
        0
    }
}
pub(super) fn mask_indices(
    indices: &[u32],
    ownership: &[[f32; COUNT]],
    missing: [bool; COUNT],
) -> Vec<u32> {
    let mut out = indices.to_vec();
    for tri in out.chunks_exact_mut(3) {
        let mut weights = [0.0; COUNT];
        for i in tri.iter() {
            for p in 0..COUNT {
                weights[p] += ownership[*i as usize][p];
            }
        }
        let part = (0..COUNT)
            .max_by(|a, b| weights[*a].total_cmp(&weights[*b]))
            .unwrap();
        if missing[part] {
            tri[1] = tri[0];
            tri[2] = tri[0];
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeat_contacts_keep_missing_limb_and_do_not_mark_a_casualty() {
        let casualty = singleplayer_crew::casualty_identity(CrewRole::Commander);
        let result = crew_damage::resolve_crew_damage(crew_damage::CrewDamageInput {
            part: crew_physics::CrewBodyPart::LeftForearm,
            fragment_energy_j: 200.0,
            explosive_energy_j: 0.0,
            blast_distance_m: 1.0,
        });
        assert!(result.detach_joint);
        let event = CrewPartDamageEvent {
            result,
            strongest_contact: None,
            wound_local: Vec3::ZERO,
            ejection_local: Vec3::Z,
        };
        assert!(!crew_part_event_is_immediately_fatal(&event));
        let mut wounds = LivingWounds::default();
        wounds.record(casualty, &[event]);
        wounds.record(casualty, &[]);
        assert_eq!(wounds.people.len(), 1);
        assert!(wounds.missing(casualty.identity)[4]);
    }
    #[test]
    fn lost_arm_masks_only_its_owned_triangles_without_changing_vertices() {
        let mut missing = [false; 11];
        missing[4] = true;
        let mut ownership = vec![[0.0; 11]; 6];
        for w in &mut ownership[..3] {
            w[4] = 1.0;
        }
        for w in &mut ownership[3..] {
            w[1] = 1.0;
        }
        assert_eq!(
            mask_indices(&[0, 1, 2, 3, 4, 5], &ownership, missing),
            vec![0, 0, 0, 3, 4, 5]
        );
    }
    #[test]
    fn imported_bone_families_respect_left_right_and_distal_segments() {
        assert_eq!(bone_part("RightHandMiddle1"), 6);
        assert_eq!(bone_part("LeftForeArm"), 4);
        assert_eq!(bone_part("R_Knee_N1"), 10);
        assert_eq!(bone_part("LeftUpLeg"), 7);
    }
}
