//! Optional simulation diagnostic: retained anatomical state and actual module hit volumes.
//! This is a read-only UI layer. It never changes scene visibility, collision, damage, or lighting.
//! The body map is a schematic of the eleven modeled regions, not an organ/health simulation.

use glam::{Mat4, Vec2, Vec3};

use super::{crew_physics::CrewBodyPart, BloodFrame, Game, ModuleKind, COUCH_CREW_ROLES};
use crate::{renderer::Renderer, ui::Align};

pub const REGION_COUNT: usize = 11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionState {
    /// No retained anatomical injury result; this must never be presented as a healthy organ.
    NoRegionRecord,
    Retained,
    PrimaryContact,
    Detached,
    Removed,
}

impl RegionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::NoRegionRecord => "NO REGION RECORD",
            Self::Retained => "RETAINED",
            Self::PrimaryContact => "CONTACT",
            Self::Detached => "DETACHED",
            Self::Removed => "REMOVED",
        }
    }

    fn color(self) -> [f32; 4] {
        match self {
            Self::NoRegionRecord => [0.18, 0.25, 0.29, 1.0],
            Self::Retained => [0.25, 0.43, 0.48, 1.0],
            Self::PrimaryContact => [0.95, 0.67, 0.25, 1.0],
            Self::Detached => [0.91, 0.35, 0.22, 1.0],
            Self::Removed => [0.59, 0.09, 0.10, 1.0],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RegionReadout {
    pub id: u8,
    pub label: &'static str,
    pub state: RegionState,
    /// The actual pose-matched anatomical OBB, or retained last pose for a removed segment.
    pub world_from_region: Mat4,
    /// Follows the same retained-segment attachment as exposed bone rendering after tissue loss.
    pub world_from_bone: Mat4,
    pub half_extents: Vec3,
    pub present: bool,
    pub bone_retained: bool,
}

#[derive(Clone, Debug)]
pub struct CrewReadout {
    pub identity: &'static str,
    pub peer_id: Option<u32>,
    pub incapacitated: bool,
    pub has_anatomical_record: bool,
    pub regions: [RegionReadout; REGION_COUNT],
}

#[derive(Clone, Copy, Debug)]
pub struct ComponentReadout {
    pub label: &'static str,
    pub functional: bool,
    pub world_position: Vec3,
    /// The authoritative spherical hit radius; the ammunition rack uses actual cartridge OBBs.
    pub hit_radius_m: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub struct AmmunitionReadout {
    pub stable_id: u32,
    pub world_from_round: Mat4,
    pub half_extents: Vec3,
}

#[derive(Clone, Debug)]
pub struct DamageReadout {
    pub crew: [CrewReadout; 4],
    pub components: Vec<ComponentReadout>,
    pub ammunition: Vec<AmmunitionReadout>,
}

fn region_name(part: CrewBodyPart) -> &'static str {
    match part {
        CrewBodyPart::Pelvis => "PELVIS",
        CrewBodyPart::Torso => "TORSO",
        CrewBodyPart::Head => "HEAD",
        CrewBodyPart::LeftUpperArm => "L UPPER ARM",
        CrewBodyPart::LeftForearm => "L FOREARM",
        CrewBodyPart::RightUpperArm => "R UPPER ARM",
        CrewBodyPart::RightForearm => "R FOREARM",
        CrewBodyPart::LeftThigh => "L THIGH",
        CrewBodyPart::LeftShin => "L SHIN",
        CrewBodyPart::RightThigh => "R THIGH",
        CrewBodyPart::RightShin => "R SHIN",
    }
}

fn region_state(present: bool, detached: bool, primary: bool, recorded: bool) -> RegionState {
    if !recorded {
        RegionState::NoRegionRecord
    } else if !present {
        RegionState::Removed
    } else if detached {
        RegionState::Detached
    } else if primary {
        RegionState::PrimaryContact
    } else {
        RegionState::Retained
    }
}

/// Snapshot only retained simulation state. No damage result is re-solved for the display.
pub fn snapshot(game: &Game) -> DamageReadout {
    let live_bodies = super::couch_crew_bodies(game.couch_crew_frame,
        &game.couch_kia_peers, &game.couch_crew_identities);
    let crew = std::array::from_fn(|index| {
        let identity = COUCH_CREW_ROLES[index];
        let ragdoll = game.couch_ragdolls.iter().find(|body| body.casualty.identity == identity);
        let live_role = live_bodies.iter().find(|body|body.casualty.identity==identity)
            .map_or(identity, |body|body.casualty.role);
        let role = ragdoll.map_or(live_role, |body| body.casualty.role);
        let frame = ragdoll.map_or_else(||super::blood_frame_for_role(role), |body|body.frame);
        let parent = super::blood_parent_model(&game.player, frame);
        let (scale, live_segments) = super::crew_pose_segments_in_frame(&game.player, role, frame);
        let segments = ragdoll.map_or(&live_segments, |body| &body.segments);
        let scale = ragdoll.map_or(scale, |body| body.scale);
        let peer_id = ragdoll.map(|body| body.casualty.peer_id).or_else(|| {
            game.couch_crew_identities.iter().flatten()
                .find(|crew| crew.identity == identity).map(|crew| crew.peer_id)
        });
        let incapacitated = ragdoll.is_some() || super::couch_crew_role(role).is_some_and(|role| {
            game.crew.iter().any(|crew| crew.role == role && !crew.ok)
        });
        CrewReadout {
            identity: identity.label(),
            peer_id,
            incapacitated,
            has_anatomical_record: ragdoll.is_some(),
            regions: std::array::from_fn(|part_index| {
                let segment = segments[part_index];
                RegionReadout {
                    id: part_index as u8,
                    label: region_name(segment.part),
                    state: region_state(segment.present, segment.detached,
                        ragdoll.is_some_and(|body| body.impact_part == segment.part), ragdoll.is_some()),
                    world_from_region: parent * super::crew_segment_local_pose(segment),
                    world_from_bone: parent * ragdoll.map_or_else(
                        || super::crew_segment_local_pose(segment),
                        |body| super::crew_ragdoll_bone_local_pose(body, segment.part)),
                    half_extents: segment.part.half_extents() * scale,
                    present: segment.present,
                    bone_retained: segment.present || ragdoll.is_some_and(|body| body.bone_follow[part_index].is_some()),
                }
            }),
        }
    });
    let hull = game.player.hull_model();
    let components = game.modules.iter().map(|module| ComponentReadout {
        label: module.kind.name(),
        functional: module.functional,
        world_position: hull.transform_point3(module.kind.station()),
        hit_radius_m: (module.kind != ModuleKind::AmmoRack).then(|| module.kind.radius()),
    }).collect();
    let ammunition = game.live_ammunition_in_frame(BloodFrame::Hull).into_iter().map(|round| {
        AmmunitionReadout {
            stable_id: round.stable_id,
            world_from_round: hull * Mat4::from_rotation_translation(round.rotation, round.center),
            half_extents: round.half_extents,
        }
    }).collect();
    DamageReadout { crew, components, ammunition }
}

impl Game {
    pub fn damage_readout(&self) -> DamageReadout {
        snapshot(self)
    }
}

const INK: [f32; 4] = [0.79, 0.87, 0.83, 1.0];
const MUTED: [f32; 4] = [0.43, 0.55, 0.55, 1.0];
const BONE: [f32; 4] = [0.90, 0.87, 0.69, 0.95];
const FRAME: [f32; 4] = [0.15, 0.24, 0.26, 1.0];
const GRID_W: f32 = 640.0;
const GRID_H: f32 = 360.0;

fn text(renderer: &mut Renderer, value: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
    // VT323's visible capitals occupy only part of the em. Below this display-space
    // size, short status labels lose strokes even though their quads still fit.
    renderer.draw_text(value, x, y, size.max(0.023), color, Align::Left);
}

/// Pixel-step line in screen UV, bounded independently of projection values.
fn line(renderer: &mut Renderer, a: Vec2, b: Vec2, color: [f32; 4]) {
    if !a.is_finite() || !b.is_finite() { return; }
    let delta = (b - a) * Vec2::new(GRID_W, GRID_H);
    let steps = delta.abs().max_element().ceil().clamp(1.0, 1024.0) as usize;
    for i in 0..=steps {
        let p = a.lerp(b, i as f32 / steps as f32);
        if p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0 { continue; }
        renderer.fill_rect((p.x * GRID_W).floor() / GRID_W, (p.y * GRID_H).floor() / GRID_H,
            1.0 / GRID_W, 1.0 / GRID_H, color);
    }
}

fn border(renderer: &mut Renderer, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    line(renderer, Vec2::new(x,y), Vec2::new(x+w,y), color);
    line(renderer, Vec2::new(x+w,y), Vec2::new(x+w,y+h), color);
    line(renderer, Vec2::new(x+w,y+h), Vec2::new(x,y+h), color);
    line(renderer, Vec2::new(x,y+h), Vec2::new(x,y), color);
}

// Front-view region envelopes. Left/right are anatomical: the crew's left is screen-right.
const REGION_RECT: [[f32; 4]; REGION_COUNT] = [
    [20.,65.,24.,18.], [19.,23.,26.,44.], [24.,2.,16.,20.],
    [45.,26.,10.,27.], [46.,52.,14.,42.], [9.,26.,10.,27.], [4.,52.,14.,42.],
    [33.,79.,12.,32.], [33.,111.,17.,35.], [19.,79.,12.,32.], [14.,111.,17.,35.],
];

const ATLAS_W: usize = 64;
const ATLAS_H: usize = 148;
const BONE_LIGHT: [f32; 4] = [0.94,0.91,0.75,1.0];
const BONE_SHADE: [f32; 4] = [0.56,0.62,0.55,1.0];
const SOCKET: [f32; 4] = [0.06,0.10,0.12,1.0];

/// Original code-drawn pixel atlas, not a texture traced from the supplied reference. Organs are
/// fixed explanatory illustrations; only the eleven retained region records drive status colors.
struct AnatomyPixels { pixels: Vec<Option<[f32;4]>> }
impl AnatomyPixels {
    fn new() -> Self { Self { pixels: vec![None; ATLAS_W*ATLAS_H] } }
    fn pixel(&mut self,x:i32,y:i32,c:[f32;4]) {
        if (0..ATLAS_W as i32).contains(&x) && (0..ATLAS_H as i32).contains(&y) {
            self.pixels[y as usize*ATLAS_W+x as usize]=Some(c);
        }
    }
    fn rect(&mut self,x:i32,y:i32,w:i32,h:i32,c:[f32;4]) {
        for py in y..y+h { for px in x..x+w { self.pixel(px,py,c); } }
    }
    fn stroke(&mut self,a:(i32,i32),b:(i32,i32),width:i32,c:[f32;4]) {
        let (mut x,mut y)=a;
        let dx=(b.0-x).abs(); let sx=if x<b.0 {1} else {-1};
        let dy=-(b.1-y).abs(); let sy=if y<b.1 {1} else {-1};
        let mut err=dx+dy;
        loop {
            self.rect(x,y,width,width,c);
            if x==b.0 && y==b.1 {break;}
            let twice=2*err;
            if twice>=dy {err+=dy; x+=sx;}
            if twice<=dx {err+=dx; y+=sy;}
        }
    }
    fn path(&mut self,p:&[(i32,i32)],w:i32,c:[f32;4]) {
        for pair in p.windows(2) {self.stroke(pair[0],pair[1],w,c);}
    }
    fn polygon(&mut self,p:&[(i32,i32)],c:[f32;4]) {
        let ymin=p.iter().map(|p|p.1).min().unwrap_or(0).max(0);
        let ymax=p.iter().map(|p|p.1).max().unwrap_or(0).min(ATLAS_H as i32-1);
        let xmin=p.iter().map(|p|p.0).min().unwrap_or(0).max(0);
        let xmax=p.iter().map(|p|p.0).max().unwrap_or(0).min(ATLAS_W as i32-1);
        for y in ymin..=ymax {for x in xmin..=xmax {
            let (px,py)=(x as f32+0.5,y as f32+0.5); let mut inside=false;
            for i in 0..p.len() {
                let a=p[i]; let b=p[(i+1)%p.len()];
                if (a.1 as f32>py)!=(b.1 as f32>py)
                    && px<(b.0-a.0) as f32*(py-a.1 as f32)/(b.1-a.1) as f32+a.0 as f32 {inside=!inside;}
            }
            if inside {self.pixel(x,y,c);}
        }}
    }
    fn bone(&mut self,a:(i32,i32),b:(i32,i32),w:i32) {
        self.stroke((a.0+1,a.1+1),(b.0+1,b.1+1),w,BONE_SHADE);
        self.stroke(a,b,w,BONE_LIGHT);
    }
    fn draw(&self,r:&mut Renderer,origin:Vec2,sx:f32,sy:f32) {
        // Coalesce horizontal runs: hundreds of UI quads rather than one draw per atlas pixel.
        for y in 0..ATLAS_H {let mut x=0;while x<ATLAS_W {
            let color=self.pixels[y*ATLAS_W+x];let begin=x;x+=1;
            while x<ATLAS_W && self.pixels[y*ATLAS_W+x]==color {x+=1;}
            if let Some(c)=color {r.fill_rect(origin.x+begin as f32*sx,origin.y+y as f32*sy,
                (x-begin) as f32*sx,sy,c);}
        }}
    }
}

fn anatomy_pixels(crew:&CrewReadout)->AnatomyPixels {
    let mut p=AnatomyPixels::new();
    let shapes:[&[(i32,i32)];REGION_COUNT]=[
        &[(23,65),(40,65),(45,72),(40,80),(35,83),(28,83),(23,79),(19,72)],
        &[(23,23),(40,23),(46,28),(44,44),(40,57),(38,68),(25,68),(23,57),(19,44),(17,28)],
        &[(28,1),(35,1),(40,5),(41,13),(38,19),(35,22),(28,22),(24,18),(22,12),(24,5)],
        &[(44,25),(50,25),(54,30),(54,48),(52,53),(46,52),(45,36),(42,29)],
        &[(46,52),(53,52),(55,76),(59,82),(60,88),(57,94),(49,94),(47,87),(44,84),(44,79),(47,77)],
        &[(13,25),(19,25),(21,29),(18,36),(17,52),(11,53),(9,48),(9,30)],
        &[(10,52),(17,52),(16,77),(19,79),(19,84),(16,87),(14,94),(6,94),(3,88),(4,82),(8,76)],
        &[(34,79),(43,79),(44,86),(40,109),(34,110),(32,94)],
        &[(34,111),(40,111),(41,135),(43,140),(49,142),(49,146),(33,146),(32,139)],
        &[(20,79),(29,79),(31,94),(29,110),(23,109),(19,86)],
        &[(23,111),(29,111),(31,139),(30,146),(14,146),(14,142),(20,140),(22,135)],
    ];
    for (id,shape) in shapes.iter().enumerate() {
        let state=crew.regions[id].state;
        let base=state.color(); let fill=[base[0]*0.32,base[1]*0.32,base[2]*0.32,1.0];
        p.polygon(shape,fill);
        let mut outline=shape.to_vec();outline.push(shape[0]);p.path(&outline,1,base);
        if state==RegionState::Removed {
            let [x,y,w,h]=REGION_RECT[id];
            for yy in y as i32..(y+h) as i32 {for xx in x as i32..(x+w) as i32 {
                if (xx+yy)%6==0 && p.pixels.get(yy as usize*ATLAS_W+xx as usize)==Some(&Some(fill)) {
                    p.pixel(xx,yy,[0.35,0.08,0.07,1.0]);
                }
            }}
        }
    }
    // Fixed-color organs explain location only. Missing tissue suppresses the corresponding
    // illustration; there are no organ injury values, health gradients or inferred diagnoses.
    if crew.regions[2].present {
        p.polygon(&[(27,5),(30,3),(35,4),(38,7),(38,10),(26,10),(26,7)],[0.68,0.33,0.39,1.0]);
        p.path(&[(28,5),(30,7),(29,9),(32,8),(34,6),(36,8)],1,[0.89,0.52,0.52,1.0]);
        p.stroke((32,4),(32,10),1,[0.37,0.20,0.27,1.0]);
    }
    if crew.regions[1].present {
        let lung=[0.64,0.37,0.34,1.0];
        p.polygon(&[(25,31),(29,30),(30,35),(29,49),(25,53),(20,51),(21,40)],lung);
        p.polygon(&[(35,30),(39,32),(42,39),(44,49),(40,53),(35,50),(34,37)],lung);
        p.stroke((31,26),(31,36),2,[0.69,0.42,0.32,1.0]);
        p.path(&[(31,35),(27,39),(25,46)],1,[0.82,0.56,0.44,1.0]);
        p.path(&[(33,35),(37,40),(39,46)],1,[0.82,0.56,0.44,1.0]);
        p.polygon(&[(31,43),(34,41),(37,43),(38,48),(35,54),(31,50),(29,46)],[0.76,0.18,0.17,1.0]);
        p.rect(32,43,2,3,[0.94,0.38,0.25,1.0]);
        p.polygon(&[(22,53),(29,51),(35,53),(33,57),(28,60),(22,59)],[0.39,0.22,0.20,1.0]);
        p.polygon(&[(37,53),(41,55),(41,60),(38,63),(34,62),(33,59),(36,57)],[0.72,0.47,0.29,1.0]);
        p.path(&[(26,61),(24,63),(24,67),(39,67),(39,62)],2,[0.63,0.34,0.21,1.0]);
        p.path(&[(28,61),(35,61),(35,63),(28,63),(28,65),(36,65)],1,[0.90,0.64,0.35,1.0]);
    }
    if crew.regions[0].present && crew.regions[1].present {
        p.path(&[(26,69),(38,69),(38,72),(28,72),(28,74),(36,74),(36,76),(31,77)],2,[0.69,0.43,0.26,1.0]);
        p.path(&[(29,69),(35,69),(35,71),(31,71),(31,73)],1,[0.93,0.67,0.38,1.0]);
    }
    // Cranium, sockets, cheekbones and separated teeth/jaw.
    if crew.regions[2].bone_retained {
        p.path(&[(25,11),(24,8),(26,4),(29,2),(35,2),(38,4),(40,8),(39,12)],1,BONE_LIGHT);
        p.path(&[(25,11),(25,16),(28,19),(30,20),(35,20),(38,17),(38,11)],2,BONE_LIGHT);
        p.rect(26,11,12,5,BONE_LIGHT);
        p.rect(27,12,4,3,SOCKET);p.rect(34,12,3,3,SOCKET);
        p.rect(31,14,2,3,SOCKET);p.rect(30,17,6,2,SOCKET);
        for x in [28,30,32,34,36] {p.rect(x,17,1,2,BONE_LIGHT);}
        p.rect(29,20,7,1,BONE_SHADE);
    }
    if crew.regions[1].bone_retained {
        for y in (22..67).step_by(3) {p.rect(31,y,3,2,BONE_SHADE);p.rect(31,y,2,1,BONE_LIGHT);}
        p.path(&[(18,27),(22,25),(27,26),(30,28)],2,BONE_LIGHT);
        p.path(&[(34,28),(37,26),(42,25),(46,27)],2,BONE_LIGHT);
        p.path(&[(21,28),(23,30),(22,35),(20,32),(21,28)],1,BONE_SHADE);
        p.path(&[(43,28),(41,30),(42,35),(44,32),(43,28)],1,BONE_SHADE);
        p.rect(31,29,2,21,BONE_LIGHT);
        for y in [31,36,41,46] {
            p.path(&[(29,y),(25,y-1),(21,y+1),(21,y+3),(25,y+5),(30,y+5)],1,BONE_LIGHT);
            p.path(&[(34,y),(38,y-1),(42,y+1),(42,y+3),(38,y+5),(33,y+5)],1,BONE_LIGHT);
        }
        p.path(&[(24,52),(27,55),(30,54)],1,BONE_SHADE);
        p.path(&[(40,52),(37,55),(34,54)],1,BONE_SHADE);
    }
    if crew.regions[0].bone_retained {
        p.path(&[(29,69),(24,66),(20,68),(20,72),(23,77),(27,80),(30,77),(31,73)],2,BONE_LIGHT);
        p.path(&[(34,69),(39,66),(43,68),(43,72),(40,77),(36,80),(33,77),(32,73)],2,BONE_LIGHT);
        p.rect(30,69,4,5,BONE_SHADE);p.rect(31,70,2,3,BONE_LIGHT);
        p.path(&[(27,78),(30,81),(33,81),(36,78)],1,BONE_LIGHT);
    }
    // Paired humeri, radius/ulna, carpals and five separate digit rays per hand. Hands belong
    // to the modeled forearm region; they are not additional independently simulated regions.
    for (upper,fore,mirror) in [(5,6,false),(3,4,true)] {
        let m=|x:i32|if mirror {63-x} else {x};
        if crew.regions[upper].bone_retained {
            p.rect(m(16)-1,28,4,4,BONE_LIGHT);p.bone((m(15),32),(m(13),48),2);
            p.rect(m(13)-1,48,4,3,BONE_LIGHT);
        }
        if crew.regions[fore].bone_retained {
            p.rect(m(13)-1,53,4,2,BONE_LIGHT);
            p.bone((m(11),55),(m(10),75),1);p.bone((m(15),55),(m(14),75),2);
            p.rect(m(12)-2,77,5,3,BONE_SHADE);
            for (x,end) in [(7,88),(9,92),(11,93),(13,91),(15,88)] {
                p.bone((m(x),81),(m(x),end),1);p.pixel(m(x),end-3,BONE_SHADE);
            }
            p.path(&[(m(15),80),(m(17),82),(m(19),85)],1,BONE_LIGHT);
            p.path(&[(m(8),81),(m(10),79),(m(14),79),(m(16),81)],1,BONE_LIGHT);
        }
    }
    // Femoral necks, knee caps, separate tibia/fibula and ankle/foot phalanges.
    for (thigh,shin,mirror) in [(9,10,false),(7,8,true)] {
        let m=|x:i32|if mirror {63-x} else {x};
        if crew.regions[thigh].bone_retained {
            p.rect(m(25)-1,81,4,3,BONE_LIGHT);p.path(&[(m(25),83),(m(23),85),(m(26),104)],2,BONE_LIGHT);
            p.rect(m(26)-2,104,5,3,BONE_LIGHT);
        }
        if crew.regions[shin].bone_retained {
            p.rect(m(26)-1,109,3,3,BONE_LIGHT);p.rect(m(26)-2,114,5,2,BONE_SHADE);
            p.bone((m(25),116),(m(24),137),2);p.bone((m(28),116),(m(28),137),1);
            p.rect(m(25)-2,138,5,3,BONE_LIGHT);
            p.path(&[(m(25),141),(m(21),142),(m(18),143)],2,BONE_LIGHT);
            for x in 16..22 {p.pixel(m(x),144,BONE_LIGHT);}
            p.rect(m(27)-1,141,3,3,BONE_SHADE);
        }
    }
    // Status marks are tied only to recorded regions. Detached anatomy stays at its reference
    // atlas location; the live cutaway, not this frontal atlas, displays physical displacement.
    for (region,rect) in crew.regions.iter().zip(REGION_RECT) {
        if matches!(region.state,RegionState::Detached|RegionState::Removed) {
            let x=(rect[0]+rect[2]*0.5) as i32;let y=rect[1] as i32;
            p.stroke((x-2,y),(x+2,y+4),1,region.state.color());
            p.stroke((x+2,y),(x-2,y+4),1,region.state.color());
        }
    }
    p
}

impl DamageReadout {
    /// Full-page, four-crew anatomical readout. Queue before the existing renderer UI flush.
    /// `aspect` is output width / height, so the little anatomical cells remain square.
    pub fn draw_anatomy(&self, renderer: &mut Renderer, aspect: f32) {
        renderer.fill_rect(0.0,0.0,1.0,1.0,[0.025,0.042,0.052,1.0]);
        text(renderer,"CREW ANATOMY / SIMULATION DIAGNOSTIC",0.025,0.025,0.033,INK);
        text(renderer,"FRONT VIEW | 11 MODELED REGIONS | ORGAN INJURY NOT MODELED",0.025,0.066,0.020,MUTED);
        let legend = [RegionState::Retained, RegionState::PrimaryContact, RegionState::Detached, RegionState::Removed];
        for (i,state) in legend.into_iter().enumerate() {
            let x = 0.025+i as f32*0.235;
            renderer.fill_rect(x,0.108,0.009,0.013,state.color());
            text(renderer,state.label(),x+0.014,0.103,0.018,INK);
        }
        let sy = 0.00318;
        let sx = sy / aspect.max(0.1);
        for (index,crew) in self.crew.iter().enumerate() {
            let x = 0.018+index as f32*0.245;
            renderer.fill_rect(x,0.15,0.23,0.68,[0.035,0.064,0.073,1.0]);
            border(renderer,x,0.15,0.23,0.68,FRAME);
            text(renderer,&crew.identity.to_uppercase(),x+0.012,0.166,0.029,INK);
            text(renderer,if crew.incapacitated {"INCAPACITATED"} else {"OPERATIONAL"},
                x+0.012,0.207,0.023,if crew.incapacitated {RegionState::Detached.color()} else {MUTED});
            let origin=Vec2::new(x+0.115-32.0*sx,0.259);
            anatomy_pixels(crew).draw(renderer,origin,sx,sy);
            let removed = crew.regions.iter().filter(|part| part.state == RegionState::Removed).count();
            let detached = crew.regions.iter().filter(|part| part.state == RegionState::Detached).count();
            text(renderer,&format!("REMOVED {}  DETACHED {}",removed,detached),x+0.012,0.747,0.023,INK);
            text(renderer,if crew.has_anatomical_record {"MODEL CONTACT / RIG STATE"} else {"NO REGION INJURY RECORD"},
                x+0.012,0.783,0.023,MUTED);
        }
        text(renderer,"TANK COMPONENTS / RECORDED BINARY STATUS",0.025,0.842,0.020,INK);
        for (i,module) in self.components.iter().enumerate() {
            let x=0.025+(i%4) as f32*0.245;
            let y=0.879+(i/4) as f32*0.027;
            renderer.fill_rect(x,y+0.004,0.005,0.009,if module.functional {RegionState::Retained.color()} else {RegionState::Removed.color()});
            text(renderer,&format!("{} {}",if module.functional {"OK"} else {"OUT"},module.label),x+0.01,y,0.016,INK);
        }
        text(renderer,"ORGANS: REFERENCE ONLY. BONES: RETAINED RIG STATE. RETAINED DOES NOT MEAN UNINJURED.",0.025,0.969,0.016,MUTED);
    }

    /// Depth-independent overlay of the current anatomical rig and actual module collision
    /// volumes. It is intentionally labeled a diagnostic cutaway, not an X-ray gameplay effect.
    pub fn draw_cutaway(&self,renderer:&mut Renderer,view_proj:Mat4) {
        renderer.fill_rect(0.015,0.018,0.68,0.095,[0.02,0.035,0.045,0.92]);
        text(renderer,"SIMULATION DIAGNOSTIC / X-RAY",0.025,0.029,0.026,INK);
        text(renderer,"CURRENT CREW RIG + MODULE HIT VOLUMES",0.025,0.069,0.018,MUTED);
        for module in &self.components {
            if let Some(radius)=module.hit_radius_m {
                let color=if module.functional {[0.22,0.61,0.62,0.60]} else {[0.94,0.22,0.15,0.86]};
                for plane in 0..3 {
                    for i in 0..24 {
                        let a=i as f32*std::f32::consts::TAU/24.0;
                        let b=(i+1) as f32*std::f32::consts::TAU/24.0;
                        let ring=|t:f32|match plane {0=>Vec3::new(t.cos(),t.sin(),0.),1=>Vec3::new(t.cos(),0.,t.sin()),_=>Vec3::new(0.,t.cos(),t.sin())};
                        world_line(renderer,view_proj,module.world_position+ring(a)*radius,module.world_position+ring(b)*radius,color);
                    }
                }
            }
        }
        for round in &self.ammunition {
            draw_box(renderer,view_proj,round.world_from_round,round.half_extents,[0.64,0.54,0.22,0.48]);
        }
        for crew in &self.crew {
            for region in &crew.regions {
                if region.present {
                    draw_box(renderer,view_proj,region.world_from_region,region.half_extents,region.state.color());
                }
                if region.bone_retained {
                    let half=region.half_extents;
                    if region.id==2 {
                        // Skull silhouette and jaw in this region's actual current pose.
                        let points=[Vec3::new(-0.65,0.7,0.),Vec3::new(0.65,0.7,0.),Vec3::new(0.75,-0.15,0.),
                            Vec3::new(0.4,-0.75,0.),Vec3::new(-0.4,-0.75,0.),Vec3::new(-0.75,-0.15,0.)];
                        for i in 0..points.len() {region_line(renderer,view_proj,region,points[i]*half,points[(i+1)%points.len()]*half,BONE);}
                        for side in [-1.,1.] {
                            region_line(renderer,view_proj,region,Vec3::new(side*0.2,0.2,0.)*half,Vec3::new(side*0.5,0.2,0.)*half,BONE);
                        }
                    } else {
                        region_line(renderer,view_proj,region,Vec3::new(0.,-half.y*0.8,0.),Vec3::new(0.,half.y*0.8,0.),BONE);
                        if region.id==1 {
                            for level in [-0.45,-0.15,0.15,0.45] {
                                for side in [-1.,1.] {
                                    region_line(renderer,view_proj,region,Vec3::new(0.,half.y*level,0.),
                                        Vec3::new(half.x*side*0.85,half.y*(level-0.1),0.),BONE);
                                }
                            }
                        }
                    }
                }
            }
            for (child,parent) in [(1,0),(2,1),(3,1),(4,3),(5,1),(6,5),(7,0),(8,7),(9,0),(10,9)] {
                let a=&crew.regions[child];let b=&crew.regions[parent];
                if a.present && b.present && a.state!=RegionState::Detached {
                    world_line(renderer,view_proj,a.world_from_region.transform_point3(Vec3::ZERO),
                        b.world_from_region.transform_point3(Vec3::ZERO),BONE);
                }
            }
        }
        // A fixed, readable status key avoids projected text pileups inside a cramped compartment.
        renderer.fill_rect(0.015,0.735,0.36,0.248,[0.02,0.035,0.045,0.91]);
        text(renderer,"COMPONENT STATE",0.025,0.745,0.020,INK);
        for (i,module) in self.components.iter().enumerate() {
            text(renderer,&format!("{} {}",if module.functional {"OK "} else {"OUT"},module.label),
                0.025+(i/6) as f32*0.175,0.777+(i%6) as f32*0.030,0.016,
                if module.functional {MUTED} else {RegionState::Detached.color()});
        }
        renderer.fill_rect(0.41,0.94,0.575,0.042,[0.02,0.035,0.045,0.91]);
        text(renderer,"RIG BONES SCHEMATIC / NO ORGAN OR PRESSURE MAP",0.42,0.95,0.016,INK);
    }
}

fn project(view_proj:Mat4,world:Vec3)->Option<Vec2> {
    let clip=view_proj*world.extend(1.0);
    if !clip.is_finite() || clip.w<=0.0001 {return None;}
    let ndc=clip.truncate()/clip.w;
    if !(0.0..=1.0).contains(&ndc.z) {return None;}
    Some(Vec2::new(ndc.x*0.5+0.5,0.5-ndc.y*0.5))
}

fn world_line(renderer:&mut Renderer,vp:Mat4,a:Vec3,b:Vec3,color:[f32;4]) {
    if let (Some(a),Some(b))=(project(vp,a),project(vp,b)) {line(renderer,a,b,color);}
}

fn region_line(renderer:&mut Renderer,vp:Mat4,region:&RegionReadout,a:Vec3,b:Vec3,color:[f32;4]) {
    world_line(renderer,vp,region.world_from_bone.transform_point3(a),region.world_from_bone.transform_point3(b),color);
}

fn draw_box(renderer:&mut Renderer,vp:Mat4,model:Mat4,half:Vec3,color:[f32;4]) {
    let corners:[Vec3;8]=std::array::from_fn(|i|model.transform_point3(Vec3::new(
        if i&1==0 {-half.x}else{half.x},if i&2==0 {-half.y}else{half.y},if i&4==0 {-half.z}else{half.z})));
    for i in 0..8 {for bit in [1,2,4] {if i&bit==0 {world_line(renderer,vp,corners[i],corners[i|bit],color);}}}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atlas_test_crew() -> CrewReadout {
        CrewReadout {
            identity: "ATLAS TEST", peer_id: None, incapacitated: false,
            has_anatomical_record: false,
            regions: std::array::from_fn(|id| RegionReadout {
                id: id as u8, label: "REGION", state: RegionState::NoRegionRecord,
                world_from_region: Mat4::IDENTITY, world_from_bone: Mat4::IDENTITY,
                half_extents: Vec3::ONE, present: true, bone_retained: true,
            }),
        }
    }

    #[test]
    fn pixel_atlas_respects_retained_bones_and_does_not_color_organs_as_injuries() {
        let mut crew=atlas_test_crew();
        let intact=anatomy_pixels(&crew);
        let count_bone=|pixels:&AnatomyPixels|pixels.pixels.iter()
            .filter(|pixel|matches!(pixel,Some(c) if *c==BONE_LIGHT || *c==BONE_SHADE)).count();
        assert!(count_bone(&intact)>300,"recognizable skeleton needs more than eleven bone sticks");
        for region in &mut crew.regions {region.bone_retained=false;}
        assert_eq!(count_bone(&anatomy_pixels(&crew)),0,"no retained bones must draw no skeletal detail");
        // Removing head tissue suppresses the brain illustration while a retained skull remains.
        crew.regions[2].present=false;
        crew.regions[2].bone_retained=true;
        crew.regions[2].state=RegionState::Removed;
        let removed=anatomy_pixels(&crew);
        assert_eq!(removed.pixels[2*ATLAS_W+29],Some(BONE_LIGHT));
        assert!(!removed.pixels.iter().any(|pixel|*pixel==Some([0.68,0.33,0.39,1.0])));
    }

    #[test]
    fn removed_and_detached_state_outranks_contact_without_inventing_health() {
        assert_eq!(region_state(true,false,false,false),RegionState::NoRegionRecord);
        assert_eq!(region_state(true,false,false,true),RegionState::Retained);
        assert_eq!(region_state(true,false,true,true),RegionState::PrimaryContact);
        assert_eq!(region_state(true,true,true,true),RegionState::Detached);
        assert_eq!(region_state(false,true,true,true),RegionState::Removed);
        assert_eq!(REGION_RECT.len(),CrewBodyPart::ALL.len());
        for part in CrewBodyPart::ALL {assert!(!region_name(part).is_empty());}
    }

    #[test]
    fn diagnostic_projection_rejects_points_behind_camera_and_invalid_positions() {
        let projection=Mat4::perspective_rh(1.0,16.0/9.0,0.1,100.0);
        assert!(project(projection,Vec3::new(0.,0.,-3.)).unwrap().distance(Vec2::splat(0.5))<1e-6);
        assert!(project(projection,Vec3::new(0.,0.,3.)).is_none());
        assert!(project(projection,Vec3::new(f32::NAN,0.,-3.)).is_none());
        assert!(project(projection,Vec3::new(0.,0.,-0.01)).is_none());
    }
}
