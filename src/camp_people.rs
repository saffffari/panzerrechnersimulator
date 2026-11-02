//! Finite camp roster presentation and actual local tasks. A render slot is not a new person.
//! Walking/inspection does not create supplies, repair equipment or board a vehicle.
use super::*;
use crate::expedition::{PersonId, PersonLocation, Seat};
use crate::gltf_load::Vertex;
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};

const COUNT: usize = crate::expedition::INITIAL_PERSONNEL;
const WALK_SPEED: f32 = 0.95;
#[derive(Clone, Copy, Debug, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
enum Task {
    AtPost,
    WalkToPost,
    Salute,
    WalkToLoad,
    WaitingAtLoad,
    Medical,
}
struct Member {
    id: PersonId,
    label: String,
    role: Seat,
    medic: bool,
    available: bool,
    medical_target: Option<Vec3>,
    field: Option<Vec3>,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Actor {
    id: PersonId,
    label: String,
    role: Seat,
    position: Vec3,
    yaw: f32,
    task: Task,
    route: Vec<Vec3>,
    waypoint: usize,
    goal: Vec3,
    phase: f32,
    salute: f32,
    work: f32,
    timer: f32,
    rest_visit: bool,
    moving: bool,
    route_retry: f32,
    #[serde(skip)]
    vertices: Vec<Vertex>,
    model: Mat4,
}
pub(super) struct CampPeople {
    pub(super) slots: [usize; COUNT],
    rig: super::camp_people_rig::Rig,
    actors: [Option<Actor>; COUNT],
    elapsed: f32,
}

fn index(person: PersonId) -> Option<usize> {
    person
        .0
        .checked_sub(1)
        .and_then(|n| usize::try_from(n).ok())
        .filter(|&n| n < COUNT)
}

impl Game {
    pub(super) fn relocate_camp_people(&mut self,delta:Mat4,ids:&[PersonId],r:&mut Renderer)->anyhow::Result<()> {
        anyhow::ensure!(delta.is_finite(),"invalid camp transport transform");
        if let Some(camp)=self.camp_people.as_mut() {for (i,entry) in camp.actors.iter_mut().enumerate() {
            if let Some(actor)=entry.as_mut().filter(|a|ids.contains(&a.id)) {
                actor.position=delta.transform_point3(actor.position);
                actor.goal=delta.transform_point3(actor.goal);
                actor.route.iter_mut().for_each(|p|*p=delta.transform_point3(*p));
                actor.model=delta*actor.model;
                let f=delta.transform_vector3(Vec3::new(actor.yaw.sin(),0.,actor.yaw.cos()));
                actor.yaw=f.x.atan2(f.z);r.set_model(camp.slots[i],actor.model);
            }
        }}
        Ok(())
    }
}
fn role_label(role: Seat) -> &'static str {
    match role {
        Seat::Commander => "COMMANDER",
        Seat::Gunner => "GUNNER",
        Seat::Loader => "LOADER",
        Seat::Driver => "DRIVER",
        Seat::Radioman => "RADIO / BOW MG",
    }
}
fn appearance(role: Seat) -> usize {
    match role {
        Seat::Commander => 0,
        Seat::Gunner => 1,
        Seat::Driver => 2,
        Seat::Loader => 3,
        Seat::Radioman => 4,
    }
}
fn task_label(task: Task) -> &'static str {
    match task {
        Task::AtPost => "ON DUTY",
        Task::WalkToPost => "CHANGING POST",
        Task::Salute => "ACKNOWLEDGING",
        Task::WalkToLoad => "REPORTING TO LOAD POINT",
        Task::WaitingAtLoad => "WAITING AT LOAD POINT",
        Task::Medical => "MEDICAL DUTY",
    }
}

impl CampPeople {
    pub(super) fn allocate(scene: &mut Vec<CpuMesh>) -> anyhow::Result<Self> {
        let rig = super::camp_people_rig::Rig::load()?;
        let faces: Vec<_> = CREW_STANDING_GLBS
            .iter()
            .map(|path| {
                load_full_crew_mesh(path, "camp reserve")
                    .ok_or_else(|| anyhow::anyhow!("camp reserve source {path} unavailable"))
            })
            .collect::<anyhow::Result<_>>()?;
        let slots = std::array::from_fn(|i| {
            let mut mesh = rig.mesh.clone();
            let face = &faces[appearance(Seat::ALL[i % 5])];
            mesh.texture = face.texture.clone();
            mesh.metallic_roughness_texture = face.metallic_roughness_texture.clone();
            mesh.normal_texture = face.normal_texture.clone();
            mesh.metallic_factor = face.metallic_factor;
            mesh.roughness_factor = face.roughness_factor;
            let slot = scene.len();
            scene.push(mesh);
            slot
        });
        Ok(Self {
            slots,
            rig,
            actors: std::array::from_fn(|_| None),
            elapsed: 0.,
        })
    }
    pub(super) fn hide(&self, r: &mut Renderer) {
        for slot in self.slots {
            r.set_visible(slot, false);
        }
    }
    fn advance(
        &mut self,
        dt: f32,
        members: &[Member],
        camp: &support_camp_runtime::CampRuntime,
        hf: &terrain::Heightfield,
        r: &mut Renderer,
    ) {
        let dt = if dt.is_finite() { dt.max(0.) } else { 0. };
        let present: BTreeSet<_> = members.iter().map(|m| m.id).collect();
        for i in 0..COUNT {
            if self.actors[i].is_none(){r.set_visible(self.slots[i],false);}
            if self.actors[i]
                .as_ref()
                .is_some_and(|a| !present.contains(&a.id))
            {
                self.actors[i] = None;
                r.set_visible(self.slots[i], false);
            }
        }
        self.elapsed += dt.max(0.);
        // State motion uses the real frame delta; skinning samples it at the existing actor30Hz rate.
        let pose_due = self.elapsed >= 1. / 30. || dt == 0.;
        if pose_due {
            self.elapsed = 0.;
        }
        for member in members {
            let Some(i) = index(member.id) else {
                continue;
            };
            let root = camp.placement.root;
            let home = member.field.unwrap_or_else(||home_position(member.id, member.medic, &camp.placement.anchors, root));
            let other_positions: Vec<_> = self
                .actors
                .iter()
                .flatten()
                .filter(|a| a.id != member.id)
                .map(|a| a.position)
                .collect();
            let actor = self.actors[i].get_or_insert_with(|| Actor {
                id: member.id,
                label: member.label.clone(),
                role: member.role,
                position: Vec3::new(home.x, camp.standing_y(home,hf.height_at(home.x, home.z)), home.z),
                yaw: 0.,
                task: if member.medic {
                    Task::Medical
                } else {
                    Task::AtPost
                },
                route: Vec::new(),
                waypoint: 0,
                goal: home,
                phase: 0.,
                salute: 0.,
                work: 0.,
                timer: 28. + i as f32 * 2.7,
                rest_visit: false,
                moving: false,
                route_retry: 0.,
                vertices: Vec::new(),
                model: Mat4::IDENTITY,
            });
            actor.label.clone_from(&member.label);
            actor.role = member.role;
            actor.route_retry = (actor.route_retry - dt).max(0.);
            if member.medic {
                actor.task = Task::Medical;
                let goal = member.medical_target.unwrap_or(home);
                if goal.distance(actor.goal) > 0.05
                    || (actor.route_retry <= 0.
                        && actor.route.is_empty()
                        && actor.position.distance(goal) > 0.65)
                {
                    actor.goal = goal;
                    actor.route = route(camp, hf, actor.position, goal);
                    actor.waypoint = 0;
                    actor.route_retry = 1.0;
                }
            } else if actor.task == Task::AtPost && member.available && member.field.is_none() {
                actor.timer -= dt.max(0.);
                if actor.timer <= 0. {
                    actor.rest_visit = !actor.rest_visit;
                    actor.goal = if actor.rest_visit {
                        camp.placement.anchors.rest
                            + root.transform_vector3(Vec3::new(
                                (i % 5) as f32 * 0.8 - 1.6,
                                0.,
                                2.2 + (i / 5) as f32 * 0.8,
                            ))
                    } else {
                        home
                    };
                    actor.route = route(camp, hf, actor.position, actor.goal);
                    actor.waypoint = 0;
                    actor.task = Task::WalkToPost;
                    actor.timer = 25. + i as f32 * 2.7;
                }
            }
            if actor.task == Task::Salute {
                actor.timer -= dt.max(0.);
                actor.salute = (actor.salute + dt * 4.).min(1.);
                if actor.timer <= 0. {
                    actor.task = Task::WalkToLoad;
                    actor.route = route(camp, hf, actor.position, actor.goal);
                    actor.waypoint = 0;
                }
            } else {
                actor.salute = (actor.salute - dt * 4.).max(0.);
            }
            let old = actor.position;
            if actor.task != Task::Salute && actor.waypoint < actor.route.len() {
                let next = actor.route[actor.waypoint];
                let delta = next - actor.position;
                let direction = Vec3::new(delta.x, 0., delta.z).normalize_or_zero();
                let step = (WALK_SPEED * dt.max(0.)).min(Vec2::new(delta.x, delta.z).length());
                // Yield around another actual actor; the same source sweeps reject a detour
                // through canvas or furniture. This creates no invisible crowd collider.
                let attempt = [0., 0.65, -0.65, 1.15, -1.15]
                    .into_iter()
                    .find_map(|angle| {
                        let heading = Quat::from_rotation_y(angle) * direction;
                        let point = actor.position + heading * step;
                        (other_positions
                            .iter()
                            .all(|p| Vec2::new(p.x - point.x, p.z - point.z).length() > 0.52)
                            && clear_body(camp, actor.position, point))
                        .then_some((point, heading))
                    });
                if let Some((attempt, heading)) = attempt {
                    actor.position =
                        Vec3::new(attempt.x, camp.standing_y(attempt,hf.height_at(attempt.x, attempt.z)), attempt.z);
                    if heading.length_squared() > 0.5 {
                        actor.yaw = heading.x.atan2(heading.z);
                    }
                }
                if actor.position.distance(next) < 0.08 {
                    actor.waypoint += 1;
                }
            }
            if actor.waypoint >= actor.route.len() && actor.position.distance(actor.goal) < 0.65 {
                if actor.task == Task::WalkToLoad {
                    actor.task = Task::WaitingAtLoad;
                } else if actor.task == Task::WalkToPost {
                    actor.task = Task::AtPost;
                }
            }
            let moved = Vec2::new(actor.position.x - old.x, actor.position.z - old.z).length();
            actor.phase = (actor.phase + moved / super::camp_people_rig::STRIDE_M).fract();
            if dt > 0. {
                actor.moving = moved > 0.0001;
            }
            let working = (actor.task == Task::Medical
                && member.medical_target.is_some()
                && actor.position.distance(actor.goal) < 0.65)
                || (member.field.is_none() && actor.task == Task::AtPost && !actor.rest_visit && member.role == Seat::Gunner);
            actor.work += ((if working { 1. } else { 0. }) - actor.work) * (dt * 5.).clamp(0., 1.);
            if working {
                let facing =
                    root.transform_vector3(if member.medic { Vec3::NEG_X } else { Vec3::X });
                actor.yaw = facing.x.atan2(facing.z);
            }
            if pose_due || actor.vertices.is_empty() {
                actor.vertices = self
                    .rig
                    .pose(actor.phase, actor.moving, actor.salute, actor.work);
                assert!(
                    r.upload_pose_vertices(self.slots[i], &actor.vertices),
                    "camp actor source pose upload rejected"
                );
            }
            actor.model =
                Mat4::from_rotation_translation(Quat::from_rotation_y(actor.yaw), actor.position);
            r.set_model(self.slots[i], actor.model);
            r.set_visible(self.slots[i], true);
        }
    }

    fn picked(&self, eye: Vec3, dir: Vec3) -> Option<(PersonId, f32)> {
        let mut best = None;
        for actor in self.actors.iter().flatten() {
            if actor.position.distance(eye) > 4. {
                continue;
            }
            let inverse = actor.model.inverse();
            let origin = inverse.transform_point3(eye);
            let direction = inverse.transform_vector3(dir);
            for tri in self.rig.mesh.indices.chunks_exact(3) {
                if actor.vertices.is_empty() {
                    continue;
                }
                let p = [0, 1, 2].map(|k| Vec3::from_array(actor.vertices[tri[k] as usize].pos));
                if let Some(t) = ray_triangle(origin, direction, p) {
                    if t <= 2.6 && best.is_none_or(|(_, distance)| t < distance) {
                        best = Some((actor.id, t));
                    }
                }
            }
        }
        best
    }
}

fn home_position(
    id: PersonId,
    medic: bool,
    a: &crate::support_camp::CampAnchors,
    root: Mat4,
) -> Vec3 {
    if medic {
        return a.medic;
    }
    let i = id.0.saturating_sub(6) as usize;
    let side=if i<5 {-0.6}else{0.6};
    match i%5 {
        0=>a.orders+root.transform_vector3(Vec3::new(side,0.,0.7)),
        1=>a.parts+root.transform_vector3(Vec3::new(-1.1,0.,side)),
        2=>a.ammo+root.transform_vector3(Vec3::new(0.6,0.,side)),
        3=>a.rest+root.transform_vector3(Vec3::new(1.25,0.,side)),
        _=>root.transform_point3(Vec3::new(side,0.,-5.45)),
    }
}
fn clear_body(camp: &support_camp_runtime::CampRuntime, from: Vec3, to: Vec3) -> bool {
    let d = (to - from).normalize_or_zero();
    let side = d.cross(Vec3::Y) * 0.24;
    [0.15, 0.78, 1.48].into_iter().all(|height| {
        [-1., 0., 1.].into_iter().all(|offset| {
            let a = from + Vec3::Y * height + side * offset;
            let b = to + Vec3::Y * height + side * offset + d * 0.25;
            camp.contact(a, b).is_none()
        })
    })
}

/// Bounded local A* through the same visible tent/prop source contacts. The cells only select
/// routes; a final exact triangle sweep admits every step, preserving real door openings.
fn route(
    camp: &support_camp_runtime::CampRuntime,
    hf: &terrain::Heightfield,
    start: Vec3,
    goal: Vec3,
) -> Vec<Vec3> {
    if clear_body(camp, start, goal) {
        return vec![goal];
    }
    const CELL: f32 = 0.8;
    const N: usize = 65;
    let origin = camp.placement.anchors.origin - Vec3::new(25.6, 0., 25.6);
    let key = |p: Vec3| -> Option<usize> {
        let x = ((p.x - origin.x) / CELL).round() as i32;
        let z = ((p.z - origin.z) / CELL).round() as i32;
        (x >= 0 && z >= 0 && x < N as i32 && z < N as i32)
            .then_some(z.max(0) as usize * N + x.max(0) as usize)
    };
    let point = |k: usize| {
        let x = origin.x + (k % N) as f32 * CELL;
        let z = origin.z + (k / N) as f32 * CELL;
        Vec3::new(x, camp.standing_y(Vec3::new(x,0.,z),hf.height_at(x, z)), z)
    };
    let (Some(first), Some(last)) = (key(start), key(goal)) else {
        return Vec::new();
    };
    let mut previous = vec![usize::MAX; N * N];
    let mut cost = vec![u32::MAX; N * N];
    let mut open = BinaryHeap::new();
    cost[first] = 0;
    open.push((Reverse(0u32), first));
    let mut visited = 0;
    while let Some((_, current)) = open.pop() {
        visited += 1;
        if visited > N * N * 2 {
            break;
        }
        if current == last && clear_body(camp, point(current), goal) {
            let mut nodes = vec![goal];
            let mut p = last;
            while p != first {
                nodes.push(point(p));
                p = previous[p];
                if p == usize::MAX {
                    return Vec::new();
                }
            }
            nodes.reverse();
            return nodes;
        }
        let x = (current % N) as i32;
        let z = (current / N) as i32;
        for (dx, dz) in [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (-1, 1),
            (1, -1),
            (1, 1),
        ] {
            let (nx, nz) = (x + dx, z + dz);
            if nx < 0 || nz < 0 || nx >= N as i32 || nz >= N as i32 {
                continue;
            }
            let next = nz as usize * N + nx as usize;
            let added = if dx != 0 && dz != 0 { 14 } else { 10 };
            let next_cost = cost[current] + added;
            if next_cost >= cost[next] {
                continue;
            }
            let a = if current == first {
                start
            } else {
                point(current)
            };
            let b = point(next);
            if (a.y - b.y).abs() > 0.20 || !clear_body(camp, a, b) {
                continue;
            }
            cost[next] = next_cost;
            previous[next] = current;
            let h = (nx - (last % N) as i32)
                .unsigned_abs()
                .max((nz - (last / N) as i32).unsigned_abs())
                * 10;
            open.push((Reverse(next_cost + h), next));
        }
    }
    Vec::new()
}
fn ray_triangle(o: Vec3, d: Vec3, p: [Vec3; 3]) -> Option<f32> {
    let e = p[1] - p[0];
    let f = p[2] - p[0];
    let h = d.cross(f);
    let det = e.dot(h);
    if det.abs() < 1e-9 {
        return None;
    }
    let s = o - p[0];
    let u = s.dot(h) / det;
    if !(0.0..=1.).contains(&u) {
        return None;
    }
    let q = s.cross(e);
    let v = d.dot(q) / det;
    if v < 0. || u + v > 1. {
        return None;
    }
    let t = f.dot(q) / det;
    (t > 0.).then_some(t)
}

fn eligible_member(
    state: &crate::expedition::Expedition,
    person: &crate::expedition::Person,
    bound: &BTreeSet<PersonId>,
) -> bool {
    person.alive()
        && person.capable()
        && !bound.contains(&person.id)
        && state.controlled_person != Some(person.id)
        && (person.location == (PersonLocation::Camp {site: state.camp.site})
            || matches!(person.location,PersonLocation::OnFoot{..}))
        && !state.medical.occupies_bed(person.id)
}

impl Game {
    pub(super) fn advance_camp_people(&mut self, dt: f32, r: &mut Renderer) {
        let Some(state) = self.expedition_state() else {
            if let Some(people) = &self.camp_people {
                people.hide(r);
            }
            return;
        };
        let Some(camp) = &self.support_camp else {
            return;
        };
        let bound: BTreeSet<_> = self.singleplayer_actor_bindings().into_iter()
            .map(|(_, person)| person).collect();
        let patient = state
            .medical
            .active_patient
            .or_else(|| state.camp_triage().first().copied());
        let target = patient
            .and_then(|id| state.medical.beds.iter().find(|b| b.patient == Some(id)))
            .and_then(|bed| {
                camp.placement
                    .anchors
                    .medical_beds
                    .iter()
                    .find(|b| b.id == bed.id)
            })
            .map(|b| b.attend_from);
        let members: Vec<_> = state
            .people
            .iter()
            .filter(|p| eligible_member(state, p, &bound))
            .map(|p| Member {
                id: p.id,
                label: format!("{} {}", p.rank, p.surname),
                role: p.trained_role,
                medic: state.medical.medic == Some(p.id),
                available: state.available_for_duty(p.id),
                medical_target: target,
                field: match p.location {PersonLocation::OnFoot{position}=>Some(Vec3::new(position.x as f32,position.y as f32,position.z as f32)),_=>None},
            })
            .collect();
        if let Some(people) = &mut self.camp_people {
            people.advance(dt, &members, camp, &self.hf, r);
        }
    }
    pub fn camp_medic_at_bed(&self, bed_id: u8) -> bool {
        let Some(state) = self.expedition_state() else {
            return false;
        };
        let Some(medic) = state.medical.medic else {
            return false;
        };
        if !state
            .person(medic)
            .is_some_and(|person| person.alive() && person.capable())
        {
            return false;
        }
        let Some(actor) = index(medic).and_then(|i| self.camp_people.as_ref()?.actors[i].as_ref())
        else {
            return false;
        };
        self.installed_camp_anchors()
            .and_then(|a| a.medical_beds.iter().find(|b| b.id == bed_id))
            .is_some_and(|b| {
                actor.position.distance(b.attend_from) < 0.65 && actor.task == Task::Medical
            })
    }
    pub fn camp_person_hover(&self, eye: Vec3, dir: Vec3) -> Option<String> {
        if self.mode != Mode::Foot
            || !self.controlled_soldier_alive()
            || !eye.is_finite()
            || !dir.is_finite()
            || dir.length_squared() < 0.5
        {
            return None;
        }
        let dir = dir.normalize();
        let people = self.camp_people.as_ref()?;
        let (id, distance) = people.picked(eye, dir)?;
        if self.structure_or_story_on_sight(eye, eye + dir * (distance - 0.01).max(0.), 0.01) {
            return None;
        }
        let actor = people.actors[index(id)?].as_ref()?;
        if let Some(label)=self.replacement_boarding_offer(id){return Some(format!("{} / {}",actor.label,label));}
        if self.recovery_can_board_person(id) {return Some(format!("{} / BOARD TRANSPORT [LMB]",actor.label));}
        let available = self.expedition_state()?.available_for_duty(id);
        Some(format!(
            "{} / {} / {}{}",
            actor.label,
            role_label(actor.role),
            task_label(actor.task),
            if available {
                " / LMB REPORT TO LOAD POINT"
            } else {
                ""
            }
        ))
    }
    pub fn camp_person_interact(&mut self, eye: Vec3, dir: Vec3) -> bool {
        if self.mode != Mode::Foot
            || !self.controlled_soldier_alive()
            || self.crew_body_held_actor().is_some()
            || !eye.is_finite()
            || !dir.is_finite()
            || dir.length_squared() < 0.5
        {
            return false;
        }
        let dir = dir.normalize();
        let Some((id, distance)) = self.camp_people.as_ref().and_then(|p| p.picked(eye, dir))
        else {
            return false;
        };
        if self.structure_or_story_on_sight(eye, eye + dir * (distance - 0.01).max(0.), 0.01) {
            return false;
        }
        if self.replacement_boarding_offer(id).is_some(){if let Err(e)=self.begin_replacement_boarding(id){self.replacement_boarding_report(e.to_string());}return true;}
        if self.recovery_board_person(id).is_ok() { return true; }
        if !self
            .expedition_state()
            .is_some_and(|state| state.available_for_duty(id))
        {
            return true;
        }
        let Some(camp) = &self.support_camp else {
            return false;
        };
        let i = index(id).unwrap();
        let goal = camp.placement.anchors.load
            + camp.placement.root.transform_vector3(Vec3::new(
                (i % 4) as f32 * 0.8 - 1.2,
                0.,
                (i / 4) as f32 * 0.85,
            ));
        let actor = self.camp_people.as_mut().unwrap().actors[i]
            .as_mut()
            .unwrap();
        actor.task = Task::Salute;
        actor.timer = 0.85;
        actor.goal = goal;
        actor.route.clear();
        actor.yaw = (eye.x - actor.position.x).atan2(eye.z - actor.position.z);
        true
    }
    pub fn camp_people_receipt(&self) -> serde_json::Value {
        self.camp_people.as_ref().map_or_else(||serde_json::json!({"installed":false}),|people|
            serde_json::json!({"installed":true,"capacity":COUNT,"actors":people.actors.iter().flatten().map(|a|
                serde_json::json!({"person":a.id.0,"name":a.label,"position":a.position.to_array(),"goal":a.goal.to_array(),
                    "task":task_label(a.task),"moving":a.moving,"remaining_waypoints":a.route.len().saturating_sub(a.waypoint),"source_vertices":a.vertices.len()})).collect::<Vec<_>>(),
                "cargo_created":0,"field_damage_integration_complete":true}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_roster_never_draws_controlled_bound_patient_or_dead_people() {
        let mut state = crate::expedition::Expedition::new(17);
        let bound = (1..=5).map(PersonId).collect::<BTreeSet<_>>();
        let visible = |s: &crate::expedition::Expedition| {
            s.people
                .iter()
                .filter(|p| eligible_member(s, p, &bound))
                .map(|p| p.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(visible(&state), (6..=16).map(PersonId).collect::<Vec<_>>());
        state.controlled_person = Some(PersonId(6));
        state.person_mut(PersonId(7)).unwrap().casualty.dead = true;
        state.person_mut(PersonId(8)).unwrap().location = PersonLocation::Treatment {
            site: state.camp.site,
        };
        assert_eq!(visible(&state), (9..=16).map(PersonId).collect::<Vec<_>>());
        state.person_mut(PersonId(4)).unwrap().location = PersonLocation::Camp {
            site: state.camp.site,
        };
        assert!(
            !visible(&state).contains(&PersonId(4)),
            "retained bound actors must not duplicate"
        );
        assert_eq!(index(PersonId(0)), None);
        assert_eq!(index(PersonId(17)), None);
    }

    #[test]
    fn source_triangle_pick_respects_exact_surface_and_transform() {
        let triangle = [
            Vec3::new(-0.3, 0., 0.),
            Vec3::new(0.3, 0., 0.),
            Vec3::new(0., 1.5, 0.),
        ];
        let model =
            Mat4::from_rotation_translation(Quat::from_rotation_y(0.7), Vec3::new(4., 0.5, -9.));
        let inverse = model.inverse();
        let eye = model.transform_point3(Vec3::new(0., 0.75, 2.));
        let direction = model.transform_vector3(Vec3::NEG_Z);
        let distance = ray_triangle(
            inverse.transform_point3(eye),
            inverse.transform_vector3(direction),
            triangle,
        )
        .unwrap();
        assert!((distance - 2.).abs() < 1e-5);
        assert!(ray_triangle(Vec3::new(0.31, 0.75, 2.), Vec3::NEG_Z, triangle).is_none());
    }

    #[test]
    #[ignore = "private camp source assets; exact medical/load route contact proof"]
    fn medic_and_reserve_routes_use_actual_openings_and_props() {
        let hf = terrain::Heightfield {
            res: 129,
            spacing: 1.,
            heights: vec![0.; 129 * 129],
            min_h: 0.,
            max_h: 0.,
            openings: Default::default(),
            detail: None,
        };
        for yaw in [0., 0.6] {
            let root = Mat4::from_rotation_y(yaw);
            let camp = support_camp_runtime::CampRuntime::source_fixture(root).unwrap();
            let anchors = &camp.placement.anchors;
            let mut trips = anchors
                .medical_beds
                .iter()
                .map(|bed| (anchors.medic, bed.attend_from))
                .collect::<Vec<_>>();
            trips.extend((6..=15).map(|id| {
                (
                    home_position(PersonId(id), false, anchors, root),
                    anchors.load,
                )
            }));
            for (start, goal) in trips {
                let path = route(&camp, &hf, start, goal);
                assert!(!path.is_empty(), "route blocked from {start:?} to {goal:?}");
                let mut from = start;
                for to in path {
                    assert!(clear_body(&camp, from, to), "route crosses source surface");
                    from = to;
                }
                assert!(from.distance(goal) < 1e-5);
            }
        }
    }
}

impl CampPeople {
    pub(super) fn export_world_state(&self)->anyhow::Result<crate::expedition_world_save::StateFields> {
        Ok(crate::capture_world_fields!(self; actors, elapsed))
    }
    pub(super) fn import_world_state(&mut self,state:crate::expedition_world_save::StateFields)->anyhow::Result<()> {
        crate::restore_world_fields!(self,state; actors, elapsed);
        for (i,actor) in self.actors.iter_mut().enumerate() {
            if let Some(actor)=actor {
                anyhow::ensure!(index(actor.id)==Some(i)&&actor.position.is_finite(),"invalid camp actor identity/pose");
                actor.vertices=self.rig.pose(actor.phase,actor.moving,actor.salute,actor.work);
            }
        }Ok(())
    }
    pub(super) fn present_world_state(&self,r:&mut Renderer)->anyhow::Result<()> {
        for (i,actor) in self.actors.iter().enumerate() {
            if let Some(actor)=actor {
                anyhow::ensure!(r.upload_pose_vertices(self.slots[i],&actor.vertices),"restore camp actor source pose failed");
                r.set_model(self.slots[i],actor.model);r.set_visible(self.slots[i],true);
            }else {r.set_visible(self.slots[i],false);}
        }Ok(())
    }
}

impl Game {
    /// Initial mission placement transfers an existing reserve; it never creates a seventeenth person.
    pub fn place_expedition_survivor(&mut self,id:PersonId,position:Vec3)->anyhow::Result<()> {
        anyhow::ensure!(position.is_finite()&&!self.singleplayer_actor_bindings().iter().any(|(_,p)|*p==id),"survivor must be an existing unbound reserve");
        let runtime=self.expedition_runtime.as_mut().ok_or_else(||anyhow::anyhow!("expedition absent"))?;
        anyhow::ensure!(runtime.state.medical.medic!=Some(id),"the active camp medic cannot be a stranded mission reward");
        let person=runtime.state.people.iter_mut().find(|p|p.id==id).ok_or_else(||anyhow::anyhow!("survivor is outside finite roster"))?;
        anyhow::ensure!(person.capable()&&matches!(person.location,PersonLocation::Camp{..}),"person is not an available camp reserve");
        let ground=self.hf.height_at(position.x,position.z);
        person.location=PersonLocation::OnFoot{position:crate::expedition::WorldPosition{x:position.x as f64,y:ground as f64,z:position.z as f64}};
        if let Some(people)=&mut self.camp_people {people.actors[index(id).unwrap()]=None;}Ok(())
    }
    pub(in crate::game) fn camp_person_position(&self,id:PersonId)->Option<Vec3> {
        self.camp_people.as_ref()?.actors.get(index(id)?)?.as_ref().map(|a|a.position)
    }
    /// Existing rendered field actor only: no position inferred from a camp/mission marker.
    /// The camera uses the actor's source forward (+Z) in the foot controller's -Z convention.
    pub(in crate::game) fn camp_person_control_pose(&self,id:PersonId)->Option<(Vec3,f32,crate::lan::LanRole)> {
        let actor=self.camp_people.as_ref()?.actors.get(index(id)?)?.as_ref()?;
        if actor.vertices.is_empty() || !actor.position.is_finite() || !actor.model.is_finite() {return None;}
        let forward=actor.model.transform_vector3(Vec3::Z).normalize_or_zero();
        Some((actor.position,forward.x.atan2(-forward.z),body_post_for_seat(actor.role)))
    }
    pub(in crate::game) fn camp_person_set_world_position(&mut self,id:PersonId,p:Vec3)->anyhow::Result<()> {
        let state=&mut self.expedition_runtime.as_mut().ok_or_else(||anyhow::anyhow!("expedition absent"))?.state;
        let camp=state.camp.clone();let person=state.people.iter_mut().find(|person|person.id==id).ok_or_else(||anyhow::anyhow!("person is not finite roster member"))?;
        person.location=if Vec3::new(camp.position.x as f32,camp.position.y as f32,camp.position.z as f32).distance(p)<24. {
            PersonLocation::Camp{site:camp.site}
        }else{PersonLocation::OnFoot{position:crate::expedition::WorldPosition{x:p.x as f64,y:p.y as f64,z:p.z as f64}}};
        // Aboard actors were removed from the camp renderer. Restore the same PersonId at
        // the actual ramp exit, rather than recreating them later at their camp duty post.
        let label=format!("{} {}",person.rank,person.surname);
        let role=person.trained_role;
        if let Some(people)=self.camp_people.as_mut() {
            let i=index(id).ok_or_else(||anyhow::anyhow!("person outside camp source capacity"))?;
            let actor=people.actors[i].get_or_insert_with(||Actor{id,label:label.clone(),role,
                position:p,yaw:0.,task:Task::AtPost,route:Vec::new(),waypoint:0,goal:p,
                phase:0.,salute:0.,work:0.,timer:28.,rest_visit:false,moving:false,route_retry:0.,
                vertices:Vec::new(),model:Mat4::IDENTITY});
            actor.position=p;actor.goal=p;actor.task=Task::AtPost;actor.route.clear();actor.waypoint=0;actor.moving=false;
        }Ok(())
    }
}

impl Game {
    pub(in crate::game) fn camp_field_shapes(&self,from:Vec3,to:Vec3,margin:f32)->Vec<singleplayer_crew::field_people::Shape> {
        let Some(people)=&self.camp_people else{return Vec::new();};let mut shapes=Vec::new();
        for actor in people.actors.iter().flatten() {
            if !singleplayer_crew::field_people::near_segment(actor.position+Vec3::Y,from,to,margin)||actor.vertices.is_empty(){continue;}
            if self.singleplayer_actor_for_person(actor.id).is_some(){continue;}
            shapes.push(singleplayer_crew::field_people::Shape{person:actor.id,appearance:body_post_for_seat(actor.role),model:actor.model,
                vertices:actor.vertices.clone(),indices:people.rig.mesh.indices.clone(),parts:people.rig.field_parts(),
                segments:people.rig.field_segments(actor.model,actor.phase,actor.moving,actor.salute,actor.work)});
        }shapes
    }
    pub(in crate::game) fn remove_camp_field_actor(&mut self,id:PersonId) {if let Some(people)=&mut self.camp_people {if let Some(i)=index(id){people.actors[i]=None;}}}
}
fn body_post_for_seat(seat:Seat)->crate::lan::LanRole {match seat {Seat::Commander=>crate::lan::LanRole::Commander,Seat::Gunner=>crate::lan::LanRole::Gunner,
    Seat::Loader=>crate::lan::LanRole::Loader,Seat::Driver=>crate::lan::LanRole::Driver,Seat::Radioman=>crate::lan::LanRole::Radio}}

impl Game {pub(in crate::game) fn hide_claimed_field_people(&self,r:&mut Renderer) {
    if let Some(people)=&self.camp_people {for (i,a) in people.actors.iter().enumerate(){if a.is_none(){r.set_visible(people.slots[i],false);}}}
}}
