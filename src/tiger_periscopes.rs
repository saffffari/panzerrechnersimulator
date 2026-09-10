//! Cupola optical parts registered to the preserved model. Armor remains unchanged. The ray
//! backend uses the source folds and finite relay lenses; the explicit shader backend samples
//! separate exterior pupil views on these exact entrance faces. Optical materials are declared
//! reconstructions because the export labels the internal faces as opaque `Material.001`.
use crate::gltf_load::{load_glb_partitioned_roots, CpuMesh, Vertex};
use glam::Vec3;

pub const WINDOW: u32 = 1;
pub const MIRROR: u32 = 2;
pub const RELAY_LENS: u32 = 3;
#[derive(Clone)]
pub struct OpticalPart {
    pub mesh: CpuMesh,
    pub kind: u32,
    pub focal_m: f32,
}
impl OpticalPart {
    fn new(mesh: CpuMesh, kind: u32) -> Self { Self {mesh,kind,focal_m:0.0} }
    fn lens(mesh: CpuMesh, focal_m: f32) -> Self { Self {mesh,kind:RELAY_LENS,focal_m} }
}
const CUPOLA_PART_COUNT: usize = 8;
pub const COMMANDER_EYE: Vec3 = Vec3::new(0.564, 2.415, -0.423);
pub const LOADER_ENTRY: Vec3 = Vec3::new(-0.7044486, 2.1240446, 0.3969902);
const LOADER_INDEX: usize = 7;
const LOADER_HOUSING: &str = "Loader Periscope.001_housing";
const LOADER_OBJECTIVE: Vec3 = Vec3::new(-0.7044486, 2.398, 0.44146136);
const LOADER_MIRROR_Z: f32 = 0.419;
// Four equal positive thin lenses at f, 3f, 5f and 7f along an unfolded 8f path form
// two successive unit-magnification image relays. The pair restores image orientation.
// This ideal prescription is reconstructed for the supplied casing, not a historical claim.
pub const LOADER_RELAY_FOCAL_M: f32 =
    ((LOADER_OBJECTIVE.y - LOADER_ENTRY.y) + (LOADER_OBJECTIVE.z - LOADER_ENTRY.z)) / 8.0;
pub const CUPOLA_NAMES: [&str; 7] = [
    "Turret_0129", "Turret_0139", "Turret_0140", "Turret_0163",
    "Turret_0164", "Turret_0205", "Turret_0206",
];
pub const CUPOLA_CENTERS: [Vec3; 7] = [
    Vec3::new(0.5643373,2.392244,-0.1227148),
    Vec3::new(0.3290518,2.392244,-0.2360223),
    Vec3::new(0.7996229,2.392244,-0.2360223),
    Vec3::new(0.2709411,2.392244,-0.4906220),
    Vec3::new(0.8577335,2.392244,-0.4906220),
    Vec3::new(0.4337636,2.392244,-0.6947950),
    Vec3::new(0.6949108,2.392244,-0.6947950),
];

#[derive(Default)]
pub struct FocusState {
    pub index: Option<usize>,
    pub phase: f32,
    pub(super) saved_near: Option<f32>,
}

/// Physical eye relief is kept outside the glass. The narrow display field frames the usable
/// objective view; this is a close-inspection camera crop, not a claim of optical magnification.
pub fn close_view(index: usize) -> (Vec3, Vec3, f32) {
    if index == LOADER_INDEX {
        // The relay preserves angular scale. This eye frames the clear hood opening with a
        // small downward gaze; the exterior roof and hood remain present outside its field.
        let pitch = -1_f32.to_radians();
        return (LOADER_ENTRY - Vec3::Z * 0.025,
            Vec3::new(0.0, pitch.sin(), pitch.cos()), 14_f32.to_radians());
    }
    let target = CUPOLA_CENTERS[index.min(6)];
    let mut radial=target-COMMANDER_EYE;radial.y=0.0;
    // A modest upward eye alignment sees over the retained roof fixtures. The objective and
    // casing stay in place; objects still occupy the lower field where their source rays hit.
    let pitch=3_f32.to_radians();
    let forward = radial.normalize()*pitch.cos()+Vec3::Y*pitch.sin();
    (target - forward * 0.025, forward, 14_f32.to_radians())
}

impl super::Game {
    pub(super) fn update_cupola_focus(&mut self, aim: bool, dt: f32, renderer: &mut crate::renderer::Renderer) {
        let commander = matches!(self.station, super::Station::Commander) && !self.commander_outside;
        let loader = matches!(self.station, super::Station::Loader) && !self.grab && self.carry_mesh.is_none();
        let available = matches!(self.mode, super::Mode::Tank)
            && (commander || loader) && self.switch_t <= 0.0
            && self.activity.is_none() && !self.climb.active && self.player.alive
            && self.binocular_raise < 0.01;
        if !available {
            if let Some(near) = self.cupola_focus.saved_near { renderer.camera.znear = near; }
            self.cupola_focus = FocusState::default();
            return;
        }
        let model = self.player.turret_model();
        if self.cupola_focus.index.is_none() {
            let eye = renderer.camera.pos;
            let look = renderer.camera.forward();
            let candidates: &[(usize, Vec3)] = if loader { &[(LOADER_INDEX, LOADER_ENTRY)] } else { &[] };
            let candidate = CUPOLA_CENTERS.iter().copied().enumerate().filter(|_|commander)
                .chain(candidates.iter().copied()).map(|(i, p)| {
                (i, (model.transform_point3(p)-eye).normalize_or_zero().dot(look))
            }).max_by(|a,b|a.1.total_cmp(&b.1));
            if let Some((i, dot)) = candidate.filter(|(_, d)|*d > 0.95) {
                let _ = dot;
                self.hover_label = Some(if loader { "PERISCOPE - HOLD RMB" }
                    else { "VISION BLOCK - HOLD RMB" });
                if aim {
                    self.cupola_focus.index = Some(i);
                    self.cupola_focus.saved_near = Some(renderer.camera.znear);
                }
            }
        }
        let held = aim && self.cupola_focus.index.is_some();
        self.cupola_focus.phase = super::tiger_driver_visor::advance_lean(self.cupola_focus.phase, held, dt);
        if let Some(index) = self.cupola_focus.index {
            let t = self.cupola_focus.phase;
            let blend = t*t*(3.0-2.0*t);
            let (eye, forward, fov) = close_view(index);
            renderer.camera.pos = renderer.camera.pos.lerp(model.transform_point3(eye), blend);
            let direction = renderer.camera.forward().lerp(model.transform_vector3(forward), blend).normalize();
            renderer.camera.set_forward(direction);
            renderer.camera.fov_y += (fov-renderer.camera.fov_y)*blend;
            renderer.camera.znear = renderer.camera.znear.min(0.005);
            renderer.optic_mask = 0.0;
            renderer.reticle = false;
            if !held && t <= 0.0 {
                if let Some(near) = self.cupola_focus.saved_near { renderer.camera.znear = near; }
                self.cupola_focus = FocusState::default();
            }
        }
    }
}
const HOUSINGS: [&str; 11] = [
    "gunner_hatch", "Turret_0129_housing", "Turret_0139_housing",
    "Turret_0140_housing", "Turret_0163_housing", "Turret_0164_housing",
    "Turret_0205_housing", "Turret_0206_housing", LOADER_HOUSING,
    super::tiger_gunner_sight::TUBE_ROOT, "escape hatch",
];
// This is the modeled lens recess, not a claimed historical laminate thickness.
const WINDOW_THICKNESS_M: f32 = 0.003892;

fn bounds(mesh: &mut CpuMesh) {
    mesh.min = Vec3::splat(f32::INFINITY);
    mesh.max = Vec3::splat(f32::NEG_INFINITY);
    for v in &mesh.vertices {
        mesh.min = mesh.min.min(Vec3::from(v.pos));
        mesh.max = mesh.max.max(Vec3::from(v.pos));
    }
}

fn select_triangles(source: &CpuMesh, select: impl Fn(usize) -> bool) -> CpuMesh {
    let mut out = source.clone();
    out.vertices.clear();
    out.indices.clear();
    for (i, tri) in source.indices.chunks_exact(3).enumerate() {
        if !select(i) { continue; }
        let first = out.vertices.len() as u32;
        out.vertices.extend(tri.iter().map(|&j| source.vertices[j as usize]));
        out.indices.extend([first, first + 1, first + 2]);
    }
    bounds(&mut out);
    out
}

fn face_normal(mesh: &CpuMesh) -> Vec3 {
    let p = |i| Vec3::from(mesh.vertices[mesh.indices[i] as usize].pos);
    (p(1) - p(0)).cross(p(2) - p(0)).normalize()
}

/// Close the existing planar lens across its existing recess. Both faces and four thin edges
/// are real triangles; the optical shader measures refraction and absorption along their path.
fn close_window(mut pane: CpuMesh, extrusion: Vec3) -> anyhow::Result<CpuMesh> {
    anyhow::ensure!(pane.indices.len() == 6, "periscope lens topology changed");
    let mut corners: Vec<Vertex> = Vec::new();
    for &i in &pane.indices {
        let vertex = pane.vertices[i as usize];
        if !corners.iter().any(|v| Vec3::from(v.pos).distance(Vec3::from(vertex.pos)) < 1e-6) {
            corners.push(vertex);
        }
    }
    anyhow::ensure!(corners.len() == 4, "periscope lens is not a quadrilateral");
    let center = corners.iter().map(|v| Vec3::from(v.pos)).sum::<Vec3>() / 4.0;
    let n = extrusion.normalize();
    let u = (Vec3::from(corners[0].pos) - center).normalize();
    let v = n.cross(u);
    corners.sort_by(|a, b| {
        let angle = |p: Vertex| {
            let q = Vec3::from(p.pos) - center;
            q.dot(v).atan2(q.dot(u))
        };
        angle(*a).total_cmp(&angle(*b))
    });
    pane.vertices.clear(); pane.indices.clear();
    let uv = corners[0].uv;
    let points: [Vec3; 4] = std::array::from_fn(|i| Vec3::from(corners[i].pos));
    let mut quad = |mut q: [Vec3; 4], normal: Vec3| {
        if (q[1] - q[0]).cross(q[2] - q[0]).dot(normal) < 0.0 { q.swap(1, 3); }
        let tangent = (q[1] - q[0]).normalize();
        let first = pane.vertices.len() as u32;
        pane.vertices.extend(q.map(|p| Vertex {
            pos: p.to_array(), normal: normal.to_array(), uv,
            tangent: [tangent.x, tangent.y, tangent.z, 1.0],
        }));
        pane.indices.extend([first, first+1, first+2, first, first+2, first+3]);
    };
    quad(points, -n);
    quad(points.map(|p| p + extrusion), n);
    for i in 0..4 {
        let j = (i+1)%4;
        let side_normal = (points[j] - points[i]).cross(n).normalize();
        quad([points[i], points[j], points[j]+extrusion, points[i]+extrusion], side_normal);
    }
    bounds(&mut pane);
    Ok(pane)
}

fn rectangle(template: &CpuMesh, center: Vec3, u: Vec3, v: Vec3, normal: Vec3) -> CpuMesh {
    let mut out = template.clone();
    out.vertices.clear();
    out.indices.clear();
    let mut points = [center-u-v, center+u-v, center+u+v, center-u+v];
    if (points[1]-points[0]).cross(points[2]-points[0]).dot(normal) < 0.0 {
        points.swap(1,3);
    }
    let uv = template.vertices[0].uv;
    let tangent = u.normalize();
    let polygon = points.map(|point| Vertex { pos:point.to_array(), normal:normal.to_array(), uv,
        tangent:[tangent.x,tangent.y,tangent.z,1.0] });
    super::tiger_driver_visor::append_polygon(&mut out.vertices,&mut out.indices,&polygon);
    bounds(&mut out);
    out
}

/// The tall export has no working optical prescription: its sloping case ends are not the
/// 45-degree reflecting surfaces. Add a small opposed pair within the existing clear tube and
/// reclassify only the rectangular objective region of its opaque exported front panel.
/// This is documented asset reconstruction, not a measured historical prism prescription.
fn loader_optical_parts(mut housing: CpuMesh, pane: CpuMesh) -> anyhow::Result<Vec<OpticalPart>> {
    anyhow::ensure!(housing.indices.len()==294*3 && pane.indices.len()==6,
        "loader periscope source topology changed");
    anyhow::ensure!(pane.center().distance(LOADER_ENTRY)<0.00001,
        "loader eyepiece moved; review source registration");
    let objective_center = LOADER_OBJECTIVE;
    let objective_half = Vec3::new(0.055,0.0075,0.0);
    let objective = rectangle(&pane,objective_center,Vec3::X*objective_half.x,
        Vec3::Y*objective_half.y,Vec3::Z);
    let front = select_triangles(&housing,|i|(48..=53).contains(&i));
    anyhow::ensure!((front.min.z-objective_center.z).abs()<0.00001
        && (front.max.z-objective_center.z).abs()<0.00001
        && objective.min.x>front.min.x && objective.max.x<front.max.x
        && objective.min.y>front.min.y && objective.max.y<front.max.y,
        "loader objective no longer fits the existing source panel");
    let planes = [(Vec3::X,objective.max.x),(-Vec3::X,-objective.min.x),
        (Vec3::Y,objective.max.y),(-Vec3::Y,-objective.min.y)];
    let mut retained = select_triangles(&housing,|i|!(48..=53).contains(&i));
    for tri in front.indices.chunks_exact(3) {
        let mut inside = tri.iter().map(|&j|front.vertices[j as usize]).collect::<Vec<_>>();
        for (normal,distance) in planes {
            let outside=super::tiger_driver_visor::half_polygon(&inside,normal,distance,false);
            super::tiger_driver_visor::append_polygon(&mut retained.vertices,&mut retained.indices,&outside);
            inside=super::tiger_driver_visor::half_polygon(&inside,normal,distance,true);
            if inside.is_empty() {break;}
        }
    }
    bounds(&mut retained);
    // The original upper case is shallow and sloped. This bounded reflector fits entirely below
    // it; the lower end of the objective also clears the unmodified Turret_0105 roof obstruction.
    let mirror_axis=(Vec3::Y+Vec3::Z).normalize();
    let mut mirrors=rectangle(&housing,Vec3::new(LOADER_ENTRY.x,LOADER_ENTRY.y,LOADER_MIRROR_Z),
        Vec3::X*0.055,mirror_axis*(0.020*2_f32.sqrt()),(Vec3::Z-Vec3::Y).normalize());
    let upper=rectangle(&housing,Vec3::new(LOADER_ENTRY.x,objective_center.y,LOADER_MIRROR_Z),
        Vec3::X*0.055,mirror_axis*(0.006*2_f32.sqrt()),(Vec3::Y-Vec3::Z).normalize());
    let base=mirrors.vertices.len() as u32;
    mirrors.vertices.extend(upper.vertices);
    mirrors.indices.extend(upper.indices.into_iter().map(|i|i+base));
    bounds(&mut mirrors);
    // Keep each original material page independent. There is no secondary image or emissive pane.
    let lenses = [1.0, 3.0, 5.0, 7.0].map(|station| {
        let center=Vec3::new(LOADER_ENTRY.x,
            LOADER_ENTRY.y + station*LOADER_RELAY_FOCAL_M - (LOADER_MIRROR_Z-LOADER_ENTRY.z),
            LOADER_MIRROR_Z);
        OpticalPart::lens(rectangle(&housing, center, Vec3::X*0.055, Vec3::Z*0.0215, Vec3::Y), LOADER_RELAY_FOCAL_M)
    });
    housing=retained;
    let mut parts=vec![OpticalPart::new(crate::tank::repaint_german_grey(housing),0),OpticalPart::new(mirrors,MIRROR),
        OpticalPart::new(close_window(pane,-Vec3::Z*WINDOW_THICKNESS_M)?,WINDOW),
        OpticalPart::new(close_window(objective,Vec3::Z*WINDOW_THICKNESS_M)?,WINDOW)];
    parts.extend(lenses);
    Ok(parts)
}

/// The remaining turret, articulated roof lid, and independently shaded source optical parts.
fn first_hit(parts: &[OpticalPart], origin: Vec3, direction: Vec3) -> Option<(usize, f32, Vec3)> {
    let mut best = None;
    let mut limit = 2.0f32;
    for (part, optical) in parts.iter().enumerate() {
        let mesh=&optical.mesh;
        for tri in mesh.indices.chunks_exact(3) {
            let [a,b,c] = [0,1,2].map(|i| Vec3::from(mesh.vertices[tri[i] as usize].pos));
            let e1=b-a; let e2=c-a; let h=direction.cross(e2); let det=e1.dot(h);
            if det.abs() < 1e-10 { continue; }
            let s=origin-a; let u=s.dot(h)/det; let q=s.cross(e1);
            let v=direction.dot(q)/det; let distance=e2.dot(q)/det;
            if u>=0.0 && v>=0.0 && u+v<=1.0 && distance>0.00005 && distance<limit {
                limit=distance;
                best=Some((part,distance,e1.cross(e2).normalize()));
            }
        }
    }
    best
}

/// Build a declared ideal relay from the exact chief-ray intersections, retaining both source
/// folds and both windows. Two reciprocal afocal pairs keep image orientation and angular scale.
fn cupola_relay(block:&[OpticalPart],index:usize)->anyhow::Result<Vec<OpticalPart>> {
    let target=CUPOLA_CENTERS[index];let forward=(target-COMMANDER_EYE).normalize();
    let mut p=target-forward*0.025;let mut d=forward;let mut inside=None;
    let mut windows=0;let mut mirrors=0;let mut path=Vec::new();
    for _ in 0..12 {
        let (part,distance,mut n)=first_hit(block,p,d)
            .ok_or_else(||anyhow::anyhow!("{} chief ray leaves its casing",CUPOLA_NAMES[index]))?;
        p+=d*distance;if n.dot(d)>0.0 {n=-n;}
        match block[part].kind {
            WINDOW=>{
                let exiting=inside.is_some();
                anyhow::ensure!(!exiting || inside==Some(part),"cupola window overlap");
                if !exiting && windows==1 {
                    anyhow::ensure!(mirrors==2,"cupola objective reached before both folds");
                    path.push(p);break;
                }
                let eta=if exiting {1.52}else{1.0/1.52};let cosine=-n.dot(d);
                let k=1.0-eta*eta*(1.0-cosine*cosine);
                anyhow::ensure!(k>0.0,"cupola chief ray internally reflects");
                d=(eta*d+(eta*cosine-k.sqrt())*n).normalize();
                if exiting {inside=None;windows+=1;path.push(p);}else{inside=Some(part);}
            },
            MIRROR=>{
                anyhow::ensure!(inside.is_none() && windows==1,"cupola fold order changed");
                path.push(p);mirrors+=1;d=d-2.0*d.dot(n)*n;
            },
            _=>anyhow::bail!("{} chief ray meets retained casing",CUPOLA_NAMES[index]),
        }
        p+=d*0.0001;
    }
    anyhow::ensure!(path.len()==4,"cupola chief ray prescription is incomplete");
    let lengths=[path[1].distance(path[0]),path[2].distance(path[1]),path[3].distance(path[2])];
    let length=lengths.iter().sum::<f32>();
    // All four finite planes lie BETWEEN the folds. A naive equal-power 8f layout puts its
    // end planes across the tilted mirrors, allowing off-axis rays to skip those lenses.
    let a=0.046;let gap=0.055;let middle=length-2.0*a-2.0*gap;
    let discriminant=gap*gap-2.0*a*middle;
    anyhow::ensure!(middle>0.001 && discriminant>0.0,"cupola relay no longer fits supplied case");
    let ratio=2.0*a/(gap+discriminant.sqrt());
    let outer=gap*ratio/(1.0+ratio);let inner=gap/(1.0+ratio);
    let right=forward.cross(Vec3::Y).normalize();
    let mut lenses=Vec::new();
    for (station,focal) in [a,a+gap,a+gap+middle,length-a].into_iter().zip([outer,inner,inner,outer]) {
        anyhow::ensure!(station>lengths[0] && station<lengths[0]+lengths[1],
            "cupola relay plane crossed a source fold");
        let axis=(path[2]-path[1]).normalize();
        let center=path[1]+axis*(station-lengths[0]);
        let across=axis.cross(right).normalize();
        let mesh=rectangle(&block[0].mesh,center,right*0.055,across*0.0175,axis);
        lenses.push(OpticalPart::lens(mesh,focal));
    }
    Ok(lenses)
}

pub fn load_turret() -> anyhow::Result<(CpuMesh, CpuMesh, Vec<OpticalPart>, CpuMesh, CpuMesh)> {
    let mut pages = crate::gltf_load::load_glb_partitioned_roots_with_material_override(
        "assets/tanks/tiger/baked/tiger_turret.glb", &HOUSINGS,
        Some(crate::tank::tiger_paint_finish),
    )?.into_iter();
    let body = pages.next().unwrap();
    let lid = pages.next().unwrap();
    let mut panes = load_glb_partitioned_roots(
        "assets/tanks/tiger/baked/tiger_rtt_turret.glb", &CUPOLA_NAMES,
    )?.into_iter();
    let loader_pane = panes.next().unwrap();
    let mut parts = Vec::new();
    for (i, pane) in panes.enumerate() {
        let housing=pages.next().ok_or_else(||anyhow::anyhow!("missing cupola housing {i}"))?;
        anyhow::ensure!(housing.indices.len() == 78 * 3, "{} housing changed", CUPOLA_NAMES[i]);
        // Exact source faces; fail closed if a replacement/re-bake changes their registration.
        let mirrors = select_triangles(&housing, |t| matches!(t, 26 | 27 | 44 | 45));
        let objective = select_triangles(&housing, |t| matches!(t, 76 | 77));
        let lower = select_triangles(&housing, |t| matches!(t, 44 | 45));
        let upper = select_triangles(&housing, |t| matches!(t, 26 | 27));
        anyhow::ensure!(face_normal(&lower).dot(face_normal(&upper)) < -0.99999
            && (lower.center().y - 2.38972).abs() < 0.002
            && (upper.center().y - 2.53501).abs() < 0.002
            && (pane.center().y - 2.39224).abs() < 0.002
            && (objective.center().y - 2.53060).abs() < 0.002,
            "{} optical faces moved; review source registration", CUPOLA_NAMES[i]);
        let objective_normal = face_normal(&objective);
        let entry_outward = -face_normal(&pane);
        let retained = select_triangles(&housing, |t| !matches!(t, 26 | 27 | 44 | 45 | 76 | 77));
        let mut block=vec![OpticalPart::new(crate::tank::repaint_german_grey(retained), 0),
            OpticalPart::new(mirrors, MIRROR)];
        // The exported pane is the inner recess face; extend toward the eye, not into its holder.
        block.push(OpticalPart::new(close_window(pane, -entry_outward * WINDOW_THICKNESS_M)?, WINDOW));
        block.push(OpticalPart::new(close_window(objective, objective_normal * WINDOW_THICKNESS_M)?, WINDOW));
        let relay=cupola_relay(&block,i)?;
        block.extend(relay);
        anyhow::ensure!(block.len()==CUPOLA_PART_COUNT,"incomplete cupola relay assembly");
        parts.extend(block);
    }
    let loader_housing=pages.next().ok_or_else(||anyhow::anyhow!("missing loader periscope housing"))?;
    parts.extend(loader_optical_parts(loader_housing,loader_pane)?);
    let sight_tube=pages.next().ok_or_else(||anyhow::anyhow!("missing gunner telescope housing"))?;
    let escape=pages.next().ok_or_else(||anyhow::anyhow!("missing side escape hatch assembly"))?;
    anyhow::ensure!(pages.next().is_none(), "unexpected turret partition");
    Ok((body, lid, parts, sight_tube, escape))
}

/// Register each exact source entry/objective pair for the explicit raster alternative. The
/// first face of close_window is the original pane; all eight assemblies retain the same layout.
pub fn raster_optic_specs(parts: &[OpticalPart], first_slot: usize) -> anyhow::Result<Vec<crate::renderer::RasterOpticSpec>> {
    anyhow::ensure!(parts.len() == 8 * CUPOLA_PART_COUNT, "raster periscope assembly count changed");
    parts.chunks_exact(CUPOLA_PART_COUNT).enumerate().map(|(index, block)| {
        anyhow::ensure!(block[2].kind == WINDOW && block[3].kind == WINDOW,
            "raster entrance/objective order changed");
        let entry = &block[2].mesh;
        let corners = [0,1,2,3].map(|i| Vec3::from(entry.vertices[i].pos));
        let center = corners.iter().copied().sum::<Vec3>() * 0.25;
        let hint = if index == LOADER_INDEX {Vec3::Z} else {
            let d = center - COMMANDER_EYE; Vec3::new(d.x,0.0,d.z).normalize()
        };
        let mut entry_outward = face_normal(entry);
        if entry_outward.dot(hint) < 0.0 {entry_outward = -entry_outward;}
        let objective = &block[3].mesh;
        let mut objective_outward = face_normal(objective);
        if objective_outward.dot(entry_outward) < 0.0 {objective_outward = -objective_outward;}
        anyhow::ensure!(entry_outward.dot(objective_outward) > 0.5,
            "raster unit relay requires forward-facing objective");
        Ok(crate::renderer::RasterOpticSpec {
            mesh:first_slot + index*CUPOLA_PART_COUNT + 2,
            entry_corners:corners,entry_outward,
            objective:objective.center(),objective_outward,
        })
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_optic_registration_preserves_all_eight_distinct_source_pupils() {
        let (_,_,parts,_,_) = load_turret().unwrap();
        let specs = raster_optic_specs(&parts,100).unwrap();
        assert_eq!(specs.len(),8);
        for (i,spec) in specs.iter().enumerate() {
            assert_eq!(spec.mesh,100+i*CUPOLA_PART_COUNT+2);
            let center=spec.entry_corners.iter().copied().sum::<Vec3>()*0.25;
            assert!(spec.objective.y>center.y+0.10);
            assert!(spec.entry_outward.dot(spec.objective_outward)>0.5);
            for other in &specs[..i] {
                assert!(spec.objective.distance(other.objective)>0.05);
                if i<7 {assert!(spec.objective_outward.dot(other.objective_outward)<0.95);}
            }
        }
    }

    fn source_path(parts: &[OpticalPart], eye: Vec3, target: Vec3) -> Option<(Vec3,Vec3)> {
        let mut p=eye; let mut d=(target-eye).normalize(); let mut inside=None;
        let mut mirrors=0; let mut windows=0;
        for _ in 0..12 {
            let (part, distance, mut n)=first_hit(parts,p,d)?;
            p+=d*distance;
            if n.dot(d)>0.0 { n=-n; }
            match parts[part].kind {
                WINDOW => {
                    let exiting=inside.is_some();
                    if exiting && inside!=Some(part) { return None; }
                    let eta=if exiting {1.52}else{1.0/1.52};
                    let cosine=-n.dot(d); let k=1.0-eta*eta*(1.0-cosine*cosine);
                    if k<0.0 { d=d-2.0*d.dot(n)*n; }
                    else {
                        d=(eta*d+(eta*cosine-k.sqrt())*n).normalize();
                        if exiting { inside=None; windows+=1; } else { inside=Some(part); }
                    }
                },
                MIRROR => { if inside.is_some() {return None;} d=d-2.0*d.dot(n)*n; mirrors+=1; },
                RELAY_LENS => {
                    if inside.is_some() { return None; }
                    let axis=-n;
                    let height=p-parts[part].mesh.center();
                    let tangent_height=height-axis*height.dot(axis);
                    d=(d/d.dot(axis)-tangent_height/parts[part].focal_m).normalize();
                },
                _ => return None,
            }
            p+=d*0.0001;
            if windows==2 && mirrors==2 && inside.is_none() { return Some((p,d)); }
        }
        None
    }

    #[test]
    fn all_seven_keep_housing_and_register_closed_windows_and_opposed_mirrors() {
        let (_, _, parts, _, _) = load_turret().expect("registered cupola source");
        assert_eq!(parts.len(), 64);
        for group in parts.chunks_exact(CUPOLA_PART_COUNT).take(7) {
            assert_eq!(group[0].mesh.indices.len(), 72 * 3);
            assert_eq!(group[1].mesh.indices.len(), 4 * 3);
            assert_eq!(group[2].mesh.indices.len(), 12 * 3);
            assert_eq!(group[3].mesh.indices.len(), 12 * 3);
            assert_eq!(group[..4].iter().map(|p| p.kind).collect::<Vec<_>>(), [0, MIRROR, WINDOW, WINDOW]);
            assert!(group[4..].iter().all(|p|p.kind==RELAY_LENS && p.focal_m>0.02 && p.focal_m<0.04));
        }
    }

    #[test]
    fn complete_turret_partition_consumes_every_eyepiece_without_requesting_an_empty_page() {
        // This is the actual Game startup partition, including the separate roof-lid path.
        // A second all-pane partition used to fail on its legitimate empty remainder before
        // the first frame, despite the individual optical-path tests passing.
        let (_,_,_,parts,_,_)=super::super::tiger_hatches::load_player_tiger()
            .expect("complete player assembly must load with all eight turret eyepieces consumed");
        assert_eq!(parts.iter().filter(|p|p.kind==WINDOW).count(),16);
        assert_eq!(parts.iter().filter(|p|p.kind==RELAY_LENS).count(),32);
    }

    #[test]
    fn registered_eye_traverses_every_cupola_block_without_skipping_opaque_geometry() {
        let (body, lid, mut parts, tube, escape)=load_turret().unwrap();
        parts.push(OpticalPart::new(tube,0));
        let targets: Vec<_>=parts.chunks_exact(CUPOLA_PART_COUNT).take(7).map(|p|p[2].mesh.center()).collect();
        parts.push(OpticalPart::new(body,0)); parts.push(OpticalPart::new(lid,0)); parts.push(OpticalPart::new(escape,0));
        for (name,target) in CUPOLA_NAMES.into_iter().zip(targets) {
            let (exit, direction)=source_path(&parts,COMMANDER_EYE,target)
                .unwrap_or_else(||panic!("{name} does not reach its actual objective"));
            assert!(exit.y>2.51 && exit.y<2.55, "{name} exit height {exit:?}");
            assert!(direction.dot((target-COMMANDER_EYE).normalize())>0.9999,
                "parallel mirror pair must preserve exterior direction");
        }
        for i in 0..7 {
            let (eye, forward, _)=close_view(i);
            assert!(source_path(&parts,eye,eye+forward).is_some(), "close-view path {i}");
        }
        let front=parts[2].mesh.center();
        assert!(source_path(&parts,COMMANDER_EYE+Vec3::Y*0.5,front).is_none(),
            "an eye above the housing should hit retained steel, not see through it");
    }

    #[test]
    fn cupola_full_fields_are_reciprocal_and_keep_actual_roof_obstructions() {
        let (body,lid,mut parts,tube,escape)=load_turret().unwrap();
        let optics=parts.clone();
        parts.extend([body,lid,tube,escape].map(|mesh|OpticalPart::new(mesh,0)));
        for i in 0..7 {
            let block=&optics[i*CUPOLA_PART_COUNT..(i+1)*CUPOLA_PART_COUNT];
            let (eye,forward,fov)=close_view(i);
            let right=forward.cross(Vec3::Y).normalize();let up=right.cross(forward).normalize();
            let mut clear=0;
            for y in [-1.0,-0.5,0.0,0.5,1.0] {
                for x in [-1.0,-0.5,0.0,0.5,1.0] {
                    let d=(forward+(right*x*(16.0/9.0)+up*y)*(fov*0.5).tan()).normalize();
                    let (exit,outgoing)=source_path(block,eye,eye+d)
                        .unwrap_or_else(||panic!("cupola {i} internal field ({x},{y}) blocked"));
                    assert!(outgoing.distance(d)<0.0001,"cupola {i} field changed angular direction");
                    let (back,backward)=source_path(block,exit,exit-outgoing).expect("reciprocal cupola field");
                    assert!(backward.distance(-d)<0.0001);
                    assert!((back-eye).cross(d).length()<0.0001,"reverse ray missed eye line");
                    let blocked=first_hit(&parts,exit,outgoing).is_some();
                    if x==0.0 && y==0.0 {assert!(!blocked,"cupola {i} held-eye center meets an exterior fixture");}
                    if !blocked {clear+=1;}
                }
            }
            assert!(clear>=15,"cupola {i} has only {clear}/25 clear source field samples");
            if i==1 {assert!(clear<25,"left-front port must retain its real roof fixture in the lower field");}
        }
    }

    #[test]
    fn loader_relay_has_useful_two_dimensional_field_and_keeps_the_exterior_hood() {
        let (body,lid,mut parts,tube,escape)=load_turret().unwrap();
        parts.push(OpticalPart::new(tube,0));
        let optical_parts=parts.clone();
        let lens_count=parts.iter().filter(|p|p.kind==RELAY_LENS).count();
        assert_eq!(lens_count,32);
        parts.push(OpticalPart::new(body,0));parts.push(OpticalPart::new(lid,0));parts.push(OpticalPart::new(escape,0));
        let (eye,forward,fov)=close_view(LOADER_INDEX);
        let right=Vec3::X;
        let up=forward.cross(right).normalize();
        for y in [-1.0,-0.5,0.0,0.5,1.0] {
            for x in [-1.0,-0.5,0.0,0.5,1.0] {
                let direction=(forward+(right*x*(16.0/9.0)+up*y)*(fov*0.5).tan()).normalize();
                let (exit,outgoing)=source_path(&parts,eye,eye+direction)
                    .unwrap_or_else(||panic!("loader field ({x},{y}) blocked inside relay"));
                assert!(outgoing.distance(direction)<0.0001,"relay must preserve orientation and angular scale");
                assert!(first_hit(&parts,exit,outgoing).is_none(),
                    "loader field ({x},{y}) remains blocked by the retained roof or hood");
                let (back,backward)=source_path(&parts,exit,-outgoing+exit)
                    .expect("unit relay must also work in reverse");
                assert!(backward.distance(-direction)<0.0001);
                assert!((back-eye).cross(direction).length()<0.0001,
                    "reverse ray must return to the same eye line");
            }
        }
        // The larger optical bundle really exists; a wider look still sees its actual exterior
        // protective hood instead of a solver that erases it to fabricate clear vision.
        let high=Vec3::new(0.0,15_f32.to_radians().tan(),1.0).normalize();
        let (exit,d)=source_path(&optical_parts,eye,eye+high).expect("wide relay bundle");
        assert!(first_hit(&parts,exit,d).is_some(),"retained exterior hood must still block skyward rays");
        let mut without_lenses=parts.clone();
        without_lenses.retain(|p|p.kind!=RELAY_LENS);
        assert!(source_path(&without_lenses,eye,eye+high).is_none(),
            "a mirror-only tube must not accidentally pass the broad field");
    }
}
