//! Optional expedition support-camp source geometry. No soldiers, supplies, healing, jobs, or
//! invisible collision volumes are created here. The expedition owns all people and inventory.
//! Canvas shelters are authored field-camp approximations, not a claimed historical tent model.
//! Each shelter is seated on its own real terrain patch. Only the workshop has a raised
//! concrete service slab; the rest of camp keeps its original ground.

use crate::gltf_load::{CpuMesh, Vertex};
use anyhow::{ensure, Context};
use glam::{Mat4, Quat, Vec3};
use std::ops::Range;

pub const MEDICAL_RADIUS_M: f32 = 10.0;
pub const QUARTERS_CAPACITY: usize = 16;
pub const STRETCHER: &str = "assets/authoring/scatter/stretcher.glb";
pub const TABLE: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Furniture/table_large_3.glb";
pub const BENCH: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Props/bench_mx_1.glb";
pub const CRATE: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Large Props/wooden_crate_1.glb";
pub const DRUM: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Props/metal_barrel_hr_1.glb";
pub const SHELF: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Props/shelf_mx_1.glb";
pub const AID_KIT: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Props/first_aid_kit_hr_1.glb";
pub const AMMO_BOX: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Items & Weapons/ammo_box_mp_1.glb";
// Upward-facing wooden deck planes measured from SHELF's source triangles. The metal rails
// sit 0.02 m lower; cases belong on the wood, not the rails or guessed evenly spaced levels.
const STOCK_SHELF_TOPS: [f32; 3] = [0.30, 0.80, 1.30];
// Keep the spare stands out of the floor-level casualty aisle. Their 1.10m source
// half-length still leaves 0.20m to the canvas at this offset; bed capacity is unchanged.
const MEDICAL_SPARE_STRETCHER_OFFSET: f32 = 1.6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CampCollision {
    /// Rendered fabric, not armor or a solid building. Gameplay may treat it differently from
    /// furniture, but it must not substitute a closed box for the open tent entrance.
    Canvas,
    Solid,
}

pub struct CampPart {
    pub name: String,
    /// All vertices are camp-local, including each prop's actual placement.
    pub mesh: CpuMesh,
    pub collision: CampCollision,
    pub source: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct MedicalBed {
    pub id: u8,
    /// The top of the existing stretcher canvas; +Z is its long axis.
    pub patient_pose: Mat4,
    pub attend_from: Vec3,
}

#[derive(Clone, Debug)]
pub struct CampAnchors {
    pub origin: Vec3,
    pub spawn: Vec3,
    pub medical_center: Vec3,
    pub medical_beds: [MedicalBed; 2],
    pub medic: Vec3,
    pub orders: Vec3,
    pub workshop: Vec3,
    pub parts: Vec3,
    pub quarters: [Vec3; QUARTERS_CAPACITY],
    pub cooking: Vec3,
    pub rest: Vec3,
    pub ammo: Vec3,
    pub fuel: Vec3,
    pub load: Vec3,
    pub tiger_parking: Vec3,
    pub recovery_parking: Vec3,
}

impl CampAnchors {
    fn local() -> Self {
        Self {
            origin: Vec3::ZERO,
            spawn: Vec3::new(0.0, 0.0, 2.0),
            medical_center: Vec3::new(-6.0, 0.0, 0.0),
            medical_beds: [-1.2, 1.2].map(|z| MedicalBed {
                id: if z < 0.0 { 1 } else { 2 },
                patient_pose: Mat4::from_translation(Vec3::new(-7.25, 0.69, z)),
                attend_from: Vec3::new(-6.15, 0.0, z),
            }),
            medic: Vec3::new(-6.15, 0.0, 0.0),
            orders: Vec3::new(-1.0, 0.0, -3.3),
            workshop: Vec3::new(6.0, 0.0, 1.0),
            parts: Vec3::new(8.4, 0.0, 4.0),
            quarters: std::array::from_fn(|i| {
                Vec3::new([-3.65,-1.2,1.2,3.65][i%4],0.43,-10.1+(i/4)as f32*1.3)
            }),
            cooking: Vec3::new(-7.4, 0.0, -9.0),
            rest: Vec3::new(-7.4, 0.0, -6.2),
            ammo: Vec3::new(-3.0, 0.0, 5.8),
            fuel: Vec3::new(-3.0, 0.0, 11.2),
            load: Vec3::new(0.0, 0.0, 8.0),
            tiger_parking: Vec3::new(6.0, 0.0, 1.0),
            recovery_parking: Vec3::new(6.0, 0.0, 11.2),
        }
    }

    fn transform(&mut self, delta: Mat4) {
        for p in [
            &mut self.origin,
            &mut self.spawn,
            &mut self.medical_center,
            &mut self.medic,
            &mut self.orders,
            &mut self.workshop,
            &mut self.parts,
            &mut self.cooking,
            &mut self.rest,
            &mut self.ammo,
            &mut self.fuel,
            &mut self.load,
            &mut self.tiger_parking,
            &mut self.recovery_parking,
        ] {
            *p = delta.transform_point3(*p);
        }
        for bed in &mut self.medical_beds {
            bed.patient_pose = delta * bed.patient_pose;
            bed.attend_from = delta.transform_point3(bed.attend_from);
        }
        for p in &mut self.quarters {
            *p = delta.transform_point3(*p);
        }
    }
}

/// Exact triangles of the meshes appended to the renderer, including real doorway gaps.
#[derive(Clone, Copy, Debug)]
pub struct CampTriangle {
    pub scene_mesh: usize,
    pub source_triangle: usize,
    pub vertices: [Vec3; 3],
    pub kind: CampCollision,
}

#[derive(Clone, Debug)]
pub struct PlacedPart {
    pub name: String,
    pub scene_mesh: usize,
    pub source: &'static str,
}

pub struct CampPlacement {
    pub meshes: Range<usize>,
    pub root: Mat4,
    baked_root: Mat4,
    pub anchors: CampAnchors,
    pub triangles: Vec<CampTriangle>,
    pub parts: Vec<PlacedPart>,
}

impl CampPlacement {
    /// Caller applies these model matrices and re-registers `triangles` after moving camp.
    /// Geometry, stations and source collision move together; no roster or stock is replenished.
    pub fn relocate(&mut self, root: Mat4) -> anyhow::Result<Vec<(usize, Mat4)>> {
        validate_root(root)?;
        let delta = root * self.root.inverse();
        self.anchors.transform(delta);
        for tri in &mut self.triangles {
            tri.vertices = tri.vertices.map(|v| delta.transform_point3(v));
        }
        self.root = root;
        let model = root * self.baked_root.inverse();
        Ok(self.meshes.clone().map(|slot| (slot, model)).collect())
    }
}

pub struct SupportCamp {
    pub parts: Vec<CampPart>,
    pub local_anchors: CampAnchors,
}

struct Props {
    stretcher: CpuMesh,
    table: CpuMesh,
    bench: CpuMesh,
    crate_mesh: CpuMesh,
    drum: CpuMesh,
    shelf:CpuMesh,
    aid_kit:CpuMesh,
    ammo_box:CpuMesh,
}
impl Props {
    fn load() -> anyhow::Result<Self> {
        let load = |path: &str| {
            crate::gltf_load::load_glb(path)
                .with_context(|| format!("optional support camp needs source asset {path}"))
        };
        Ok(Self {
            stretcher: load(STRETCHER)?,
            table: load(TABLE)?,
            bench: load(BENCH)?,
            crate_mesh: load(CRATE)?,
            drum: load(DRUM)?,
            shelf:load(SHELF)?,
            aid_kit:load(AID_KIT)?,
            ammo_box:load(AMMO_BOX)?,
        })
    }
}

impl SupportCamp {
    /// Explicit opt-in; no private camp asset is loaded during ordinary startup.
    pub fn load() -> anyhow::Result<Self> {
        Self::build(Props::load()?)
    }

    #[cfg(test)]
    pub(crate) fn load_with_spare_stretcher_offset(offset:f32) -> anyhow::Result<Self> {
        Self::build_with_spare_stretcher_offset(Props::load()?,offset)
    }

    /// Park the reviewed transport source as static geometry. This adds neither a drivable
    /// vehicle nor a cargo/towing capacity; those remain separate gameplay integration.
    pub fn include_static_halftrack(&mut self) -> anyhow::Result<()> {
        let asset = crate::halftrack::HalftrackAsset::load()?;
        let at = self.local_anchors.recovery_parking - Vec3::Y * asset.min.y;
        for (index, mesh) in asset.meshes.iter().enumerate() {
            self.prop(
                &format!("recovery.transport.{}", crate::halftrack::PART_NAMES[index]),
                mesh,
                at,
                0.0,
                crate::halftrack::ASSET_PATH,
            );
        }
        Ok(())
    }

    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.parts.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), part| (lo.min(part.mesh.min), hi.max(part.mesh.max)),
        )
    }

    pub fn append_to_scene(
        &self,
        scene: &mut Vec<CpuMesh>,
        root: Mat4,
    ) -> anyhow::Result<CampPlacement> {
        validate_root(root)?;
        let first = scene.len();
        let mut triangles = Vec::new();
        let mut parts = Vec::new();
        for part in &self.parts {
            let mesh = part.mesh.clone().transformed(root);
            let scene_mesh = scene.len();
            for (source_triangle, tri) in mesh.indices.chunks_exact(3).enumerate() {
                triangles.push(CampTriangle {
                    scene_mesh,
                    source_triangle,
                    kind: part.collision,
                    vertices: [0, 1, 2]
                        .map(|k| Vec3::from_array(mesh.vertices[tri[k] as usize].pos)),
                });
            }
            parts.push(PlacedPart {
                name: part.name.clone(),
                scene_mesh,
                source: part.source,
            });
            scene.push(mesh);
        }
        let mut anchors = self.local_anchors.clone();
        anchors.transform(root);
        Ok(CampPlacement {
            meshes: first..scene.len(),
            root,
            baked_root: root,
            anchors,
            triangles,
            parts,
        })
    }

    fn prop(&mut self, name: &str, mesh: &CpuMesh, at: Vec3, yaw: f32, source: &'static str) {
        self.parts.push(CampPart {
            name: name.to_owned(),
            mesh: mesh.clone().transformed(Mat4::from_rotation_translation(
                Quat::from_rotation_y(yaw),
                at,
            )),
            collision: CampCollision::Solid,
            source,
        });
    }

    /// Seat each compact shelter cluster independently; do not edit the world's heightfield.
    /// A little fabric/leg embedment is permitted on gentle ground, never a floating camp slab.
    pub fn conform_to_terrain(&mut self,root:Mat4,hf:&crate::terrain::Heightfield)->anyhow::Result<()> {
        validate_root(root)?;
        self.parts.retain(|p|p.name!="workshop.concrete.service.slab");
        let ground=|local:Vec3| {let p=root.transform_point3(local);hf.height_at(p.x,p.z)-root.w_axis.y};
        let groups=[("medical",Vec3::new(-6.,0.,0.),Vec3::new(2.9,0.,2.9)),
            ("quarters",Vec3::new(0.,0.,-8.3),Vec3::new(5.2,0.,3.4)),
            ("workshop",Vec3::new(6.,0.,1.),Vec3::new(3.5,0.,5.)),
            ("ammunition",Vec3::new(-6.,0.,5.8),Vec3::new(2.6,0.,2.2)),
            ("fuel",Vec3::new(-6.,0.,11.2),Vec3::new(2.4,0.,2.3)),
            ("rest",Vec3::new(-7.4,0.,-6.2),Vec3::new(1.2,0.,1.2)),
            ("cooking",Vec3::new(-7.4,0.,-9.),Vec3::new(0.7,0.,0.7)),
            ("orders",Vec3::new(-1.,0.,-3.8),Vec3::new(1.2,0.,0.8))];
        let mut offsets=std::collections::HashMap::new();
        for (name,center,half) in groups {
            let samples=[center,center+half,center-half,center+Vec3::new(half.x,0.,-half.z),center+Vec3::new(-half.x,0.,half.z)].map(ground);
            let low=samples.into_iter().fold(f32::INFINITY,f32::min);
            let high=samples.into_iter().fold(f32::NEG_INFINITY,f32::max);
            ensure!(low.is_finite()&&high-low<=0.65,"camp cluster {name} needs gentler terrain");
            let base=if name=="workshop" {high}else{low};
            offsets.insert(name,base);
            for part in self.parts.iter_mut().filter(|p|p.name.starts_with(name)) {
                part.mesh=part.mesh.clone().transformed(Mat4::from_translation(Vec3::Y*base));
            }
            if name=="workshop" {
                let mut pad=Batch::new([111,110,104]);
                pad.box_at(Vec3::new(center.x,(low-0.08+base+0.14)*0.5,center.z),
                    Vec3::new(7.,base-low+0.22,10.));
                self.parts.push(pad.part("workshop.concrete.service.slab",CampCollision::Solid));
            }
        }
        let a=&mut self.local_anchors;
        for bed in &mut a.medical_beds {bed.patient_pose.w_axis.y+=offsets["medical"];bed.attend_from.y=ground(bed.attend_from);}
        a.medical_center.y=offsets["medical"];a.medic.y=ground(a.medic);
        for p in &mut a.quarters {p.y+=offsets["quarters"];}
        a.workshop.y+=offsets["workshop"];a.parts.y+=offsets["workshop"];a.tiger_parking.y+=offsets["workshop"];
        for p in [&mut a.spawn,&mut a.orders,&mut a.cooking,&mut a.rest,&mut a.ammo,&mut a.fuel,&mut a.load,&mut a.recovery_parking] {p.y=ground(*p);}
        Ok(())
    }

    fn build(props: Props) -> anyhow::Result<Self> {
        Self::build_with_spare_stretcher_offset(props,MEDICAL_SPARE_STRETCHER_OFFSET)
    }

    fn build_with_spare_stretcher_offset(props:Props,spare_offset:f32) -> anyhow::Result<Self> {
        let anchors=CampAnchors::local();
        let mut camp=Self{parts:Vec::new(),local_anchors:anchors.clone()};
        let east=std::f32::consts::FRAC_PI_2;
        camp.parts.extend(tent("medical",anchors.medical_center,5.8,5.8,east));
        camp.parts.extend(tent("quarters",Vec3::new(0.,0.,-8.3),10.4,6.8,0.));
        camp.parts.extend(awning("workshop",anchors.workshop,7.,10.,3.5));
        camp.parts.extend(tent("ammunition",Vec3::new(-6.,0.,5.8),4.4,5.2,east));
        camp.parts.extend(tent("fuel",Vec3::new(-6.,0.,11.2),4.6,4.8,east));

        // Two active treatment beds and two clearly retained spare stretcher stations. Spare
        // furniture does not silently increase the coordinator's two-bed/one-medic capacity.
        let mut stands=Batch::new([70,68,62]);
        for (index,at) in anchors.medical_beds.iter().map(|b|b.patient_pose.transform_point3(Vec3::ZERO)-Vec3::Y*0.04)
            .chain([-spare_offset,spare_offset].map(|z|Vec3::new(-4.7,0.65,z))).enumerate() {
            let label=if index<2 {format!("medical.bed.{}.stretcher",index+1)}else{format!("medical.spare.stretcher.{}",index-1)};
            camp.prop(&label,&props.stretcher,at,0.,STRETCHER);
            for z in [-0.72,0.72] {
                for x in [-0.24,0.24] {stands.beam(at+Vec3::new(x,-0.65,z),at+Vec3::new(-x,-0.03,z),0.025);}
                stands.beam(at+Vec3::new(-0.31,-0.03,z),at+Vec3::new(0.31,-0.03,z),0.03);
            }
        }
        camp.parts.push(stands.part("medical.stretcher.stands",CampCollision::Solid));
        for (rack,z) in [-2.58,2.58].into_iter().enumerate() {
            camp.prop(&format!("medical.supply.rack.{rack}"),&props.shelf,Vec3::new(-6.,0.,z),if z>0.{std::f32::consts::PI}else{0.},SHELF);
            // Source aid cases sit on measured shelf levels. These are stocked appearance;
            // consumable treatment still uses the sole finite coordinator AidBox.
            for (row,top) in STOCK_SHELF_TOPS.into_iter().enumerate() {
                for column in 0..3 {
                    // Retain the same nine cases per rack. One middle pair is stacked on its
                    // real case height, making the thin source cases legible without extra stock.
                    let stacked = row==1 && column==1;
                    let x = [-6.47,-6.,-5.53][if stacked {0}else{column}];
                    let y = top-props.aid_kit.min.y
                        + if stacked {props.aid_kit.max.y-props.aid_kit.min.y}else{0.};
                    camp.prop(&format!("medical.aid.case.{rack}.{row}.{column}"),&props.aid_kit,Vec3::new(x,y,z),0.,AID_KIT);
                }
            }
        }
        camp.prop("medical.side.table",&props.table,Vec3::new(-8.25,0.,0.),east,TABLE);
        camp.prop("medical.open.case",&props.aid_kit,Vec3::new(-8.25,0.92,0.),east,AID_KIT);

        let mut wood=Batch::new([91,76,54]);let mut canvas=Batch::new([116,112,85]);
        for at in anchors.quarters {
            canvas.box_at(at-Vec3::Y*0.02,Vec3::new(1.90,0.04,0.64));
            for x in [-0.90,0.90] {
                wood.box_at(at+Vec3::new(x,-0.045,0.),Vec3::new(0.06,0.07,0.72));
                for z in [-0.28,0.28] {wood.box_at(at+Vec3::new(x,-0.24,z),Vec3::new(0.06,0.38,0.06));}
            }
            for z in [-0.34,0.34] {wood.box_at(at+Vec3::new(0.,-0.04,z),Vec3::new(1.94,0.06,0.05));}
        }
        camp.parts.push(wood.part("quarters.cot.frames",CampCollision::Solid));
        camp.parts.push(canvas.part("quarters.cot.canvas",CampCollision::Solid));
        for i in 0..8 {camp.prop(&format!("quarters.personal.case.{i}"),&props.crate_mesh,
            Vec3::new(-3.65+(i%4)as f32*2.43,0.,-9.45+(i/4)as f32*2.6),0.,CRATE);}

        camp.prop("orders.table",&props.table,Vec3::new(-1.,0.,-3.9),0.,TABLE);
        let mut board=Batch::new([87,73,52]);
        for x in [-1.65,-0.35] {board.beam(Vec3::new(x,0.,-4.4),Vec3::new(x,1.85,-4.4),0.045);}
        board.box_at(Vec3::new(-1.,1.45,-4.4),Vec3::new(1.45,0.8,0.05));
        camp.parts.push(board.part("orders.notice.board",CampCollision::Solid));
        camp.prop("workshop.bench",&props.table,Vec3::new(8.65,0.,-2.4),east,TABLE);
        camp.prop("workshop.parts.rack",&props.shelf,Vec3::new(8.75,0.,3.4),east,SHELF);
        for i in 0..8 {camp.prop(&format!("workshop.parts.case.{i}"),&props.crate_mesh,
            Vec3::new(8.5+(i%2)as f32*0.75,(i/4)as f32*0.36,4.2+(i%4/2)as f32*0.6),0.,CRATE);}

        // Full-size canonical game rounds in a visible wooden rack, plus source small-arms boxes.
        let round=crate::procedural_ordnance::ap_round_mesh();
        let mut rack=Batch::new([85,69,45]);
        for x in [-7.8,-5.8] {for z in [4.65,5.85] {rack.box_at(Vec3::new(x,0.95,z),Vec3::new(0.08,1.9,0.08));}}
        for (row,y) in [0.35,0.85,1.35].into_iter().enumerate() {
            for z in [4.7,5.8] {rack.box_at(Vec3::new(-6.8,y-0.08,z),Vec3::new(2.1,0.08,0.12));}
            for column in 0..9 {camp.prop(&format!("ammunition.shell.display.{row}.{column}"),&round,
                Vec3::new(-7.68+column as f32*0.22,y,5.25),0.,"canonical procedural_ordnance::ap_round_mesh");}
        }
        camp.parts.push(rack.part("ammunition.shell.rack",CampCollision::Solid));
        camp.prop("ammunition.smallarms.rack",&props.shelf,Vec3::new(-6.1,0.,7.6),std::f32::consts::PI,SHELF);
        for (row,top) in STOCK_SHELF_TOPS.into_iter().enumerate() {for column in 0..6 {
            // This source is an open box with projecting contents, not a flat stacking surface.
            let y = top-props.ammo_box.min.y;
            camp.prop(&format!("ammunition.smallarms.box.{row}.{column}"),&props.ammo_box,
                Vec3::new(-6.7+column as f32*0.24,y,7.6),0.,AMMO_BOX);
        }}
        for i in 0..8 {camp.prop(&format!("ammunition.reserve.case.{i}"),&props.crate_mesh,
            Vec3::new(-7.7+(i%4)as f32*0.8,(i/4)as f32*0.36,6.7),0.,CRATE);}
        for i in 0..12 {camp.prop(&format!("fuel.drum.{i}"),&props.drum,
            Vec3::new(-7.5+(i%4)as f32*0.85,0.,10.45+(i/4)as f32*0.82),0.,DRUM);}
        camp.prop("rest.table",&props.table,Vec3::new(-7.4,0.,-6.2),0.,TABLE);
        for (i,z) in [-5.1,-7.3].into_iter().enumerate() {camp.prop(&format!("rest.bench.{i}"),&props.bench,Vec3::new(-7.4,0.,z),0.,BENCH);}
        let mut stove=Batch::new([49,48,44]);
        stove.box_at(Vec3::new(-7.4,0.48,-9.),Vec3::new(0.65,0.75,0.65));
        for x in [-7.64,-7.16] {for z in [-9.24,-8.76] {stove.box_at(Vec3::new(x,0.10,z),Vec3::new(0.06,0.20,0.06));}}
        stove.beam(Vec3::new(-7.24,0.85,-9.15),Vec3::new(-7.24,2.6,-9.15),0.06);
        camp.parts.push(stove.part("cooking.stove.unlit",CampCollision::Solid));
        for part in camp.parts.iter_mut().filter(|p|p.name.starts_with("workshop")) {
            part.mesh=part.mesh.clone().transformed(Mat4::from_translation(Vec3::Y*0.14));
        }
        let mut pad=Batch::new([111,110,104]);
        pad.box_at(Vec3::new(6.,0.03,1.),Vec3::new(7.,0.22,10.));
        camp.parts.push(pad.part("workshop.concrete.service.slab",CampCollision::Solid));
        camp.local_anchors.workshop.y=0.14;camp.local_anchors.parts.y=0.14;camp.local_anchors.tiger_parking.y=0.14;
        ensure!(camp.parts.iter().all(|p|!p.mesh.indices.is_empty()&&p.mesh.min.is_finite()&&p.mesh.max.is_finite()),"invalid camp source");
        Ok(camp)
    }
}

fn validate_root(root: Mat4) -> anyhow::Result<()> {
    ensure!(
        root.is_finite() && root.determinant().abs() > 1e-8,
        "camp root is invalid"
    );
    let x = root.x_axis.truncate();
    let y = root.y_axis.truncate();
    let z = root.z_axis.truncate();
    ensure!(
        (x.length() - 1.0).abs() < 1e-4
            && (z.length() - 1.0).abs() < 1e-4
            && y.distance(Vec3::Y) < 1e-4
            && x.cross(y).distance(z) < 1e-4,
        "camp placement supports level rigid translation/yaw, not scaling or tilted ground"
    );
    Ok(())
}

struct Batch {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    color: [u8; 3],
}
impl Batch {
    fn new(color: [u8; 3]) -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            color,
        }
    }
    fn add(&mut self, mesh: CpuMesh) {
        let first = self.vertices.len() as u32;
        self.vertices.extend(mesh.vertices);
        self.indices
            .extend(mesh.indices.into_iter().map(|i| first + i));
    }
    fn box_at(&mut self, center: Vec3, size: Vec3) {
        self.add(crate::prims::box_mesh(
            center - size * 0.5,
            center + size * 0.5,
            self.color,
        ));
    }
    fn beam(&mut self, a: Vec3, b: Vec3, half_width: f32) {
        let d = b - a;
        let mesh = crate::prims::box_mesh(
            Vec3::new(-half_width, -d.length() * 0.5, -half_width),
            Vec3::new(half_width, d.length() * 0.5, half_width),
            self.color,
        );
        self.add(mesh.transformed(Mat4::from_rotation_translation(
            Quat::from_rotation_arc(Vec3::Y, d.normalize()),
            (a + b) * 0.5,
        )));
    }
    fn cloth(&mut self, points: [Vec3; 4]) {
        for p in [points, [points[3], points[2], points[1], points[0]]] {
            let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
            let t = (p[1] - p[0]).normalize();
            let first = self.vertices.len() as u32;
            for (at, uv) in p
                .into_iter()
                .zip([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            {
                self.vertices.push(Vertex {
                    pos: at.to_array(),
                    normal: n.to_array(),
                    uv,
                    tangent: [t.x, t.y, t.z, 1.0],
                });
            }
            self.indices
                .extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        }
    }
    fn part(self, name: &str, collision: CampCollision) -> CampPart {
        let min = self
            .vertices
            .iter()
            .fold(Vec3::splat(f32::INFINITY), |b, v| {
                b.min(Vec3::from_array(v.pos))
            });
        let max = self
            .vertices
            .iter()
            .fold(Vec3::splat(f32::NEG_INFINITY), |b, v| {
                b.max(Vec3::from_array(v.pos))
            });
        CampPart {
            name: name.to_owned(),
            collision,
            source: "authored support-camp geometry v1",
            mesh: CpuMesh::basic(
                self.vertices,
                self.indices,
                Some(crate::prims::solid_color(self.color)),
                min,
                max,
            ),
        }
    }
}

fn tent(name: &str, center: Vec3, width: f32, depth: f32, yaw: f32) -> [CampPart; 2] {
    let pose = Mat4::from_rotation_translation(Quat::from_rotation_y(yaw), center);
    let p = |x, y, z| pose.transform_point3(Vec3::new(x, y, z));
    let (w, d, eave, ridge) = (width * 0.5, depth * 0.5, 2.15, 3.0);
    let mut canvas = Batch::new([130, 124, 94]);
    let mut frame = Batch::new([87, 74, 51]);
    for x in [-w, w] {
        canvas.cloth([p(x, 0.05, -d), p(x, 0.05, d), p(x, eave, d), p(x, eave, -d)]);
        for z in [-d, 0.0, d] {
            frame.beam(p(x, 0.0, z), p(x, eave, z), 0.045);
        }
    }
    canvas.cloth([
        p(-w, 0.05, -d),
        p(w, 0.05, -d),
        p(w, eave, -d),
        p(-w, eave, -d),
    ]);
    // Two front flaps leave a genuine two-metre-wide opening. No collision cap spans the door.
    for (a, b) in [(-w, -1.0), (1.0, w)] {
        canvas.cloth([p(a, 0.05, d), p(b, 0.05, d), p(b, eave, d), p(a, eave, d)]);
    }
    for z in [-d, d] {
        canvas.cloth([
            p(-w, eave, z),
            p(w, eave, z),
            p(0.01, ridge, z),
            p(-0.01, ridge, z),
        ]);
        frame.beam(p(-w, eave, z), p(0.0, ridge, z), 0.045);
        frame.beam(p(w, eave, z), p(0.0, ridge, z), 0.045);
    }
    frame.beam(p(0.0, ridge, -d), p(0.0, ridge, d), 0.05);
    for step in 0..8 {
        let z0 = -d + depth * step as f32 / 8.0;
        let z1 = -d + depth * (step + 1) as f32 / 8.0;
        let sag = |z: f32| 0.06 * (std::f32::consts::PI * (z + d) / depth).sin();
        for side in [-1.0, 1.0] {
            canvas.cloth([
                p(0.0, ridge - sag(z0), z0),
                p(side * w, eave - sag(z0), z0),
                p(side * w, eave - sag(z1), z1),
                p(0.0, ridge - sag(z1), z1),
            ]);
        }
    }
    [
        canvas.part(&format!("{name}.canvas"), CampCollision::Canvas),
        frame.part(&format!("{name}.frame"), CampCollision::Solid),
    ]
}

fn awning(name: &str, center: Vec3, width: f32, depth: f32, height: f32) -> [CampPart; 2] {
    let mut canvas = Batch::new([116, 112, 87]);
    let mut frame = Batch::new([78, 69, 51]);
    let p = |x, y, z| center + Vec3::new(x, y, z);
    let (w, d) = (width * 0.5, depth * 0.5);
    for x in [-w, w] {
        for z in [-d, d] {
            frame.beam(p(x, 0.0, z), p(x, height, z), 0.065);
        }
    }
    for z in [-d, d] {
        frame.beam(p(-w, height, z), p(w, height, z), 0.07);
    }
    for x in [-w, w] {
        frame.beam(p(x, height, -d), p(x, height, d), 0.07);
    }
    canvas.cloth([
        p(-w - 0.2, height + 0.02, -d - 0.2),
        p(w + 0.2, height + 0.25, -d - 0.2),
        p(w + 0.2, height + 0.25, d + 0.2),
        p(-w - 0.2, height + 0.02, d + 0.2),
    ]);
    [
        canvas.part(&format!("{name}.awning"), CampCollision::Canvas),
        frame.part(&format!("{name}.frame"), CampCollision::Solid),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> SupportCamp {
        let box_mesh = |size: Vec3| {
            crate::prims::box_mesh(
                -size * Vec3::new(0.5, 0.0, 0.5),
                size * Vec3::new(0.5, 1.0, 0.5),
                [100; 3],
            )
        };
        SupportCamp::build(Props {
            stretcher: box_mesh(Vec3::new(0.635, 0.04, 2.2)),
            table: box_mesh(Vec3::new(1.6, 0.91, 0.69)),
            bench: box_mesh(Vec3::new(1.85, 0.63, 0.44)),
            crate_mesh: box_mesh(Vec3::new(0.72, 0.36, 0.504)),
            drum: box_mesh(Vec3::new(0.674, 1.0, 0.674)),
            shelf:box_mesh(Vec3::new(1.6,1.8,0.55)),
            aid_kit:box_mesh(Vec3::new(0.4,0.08,0.22)),
            ammo_box:box_mesh(Vec3::new(0.216,0.113,0.11)),
        })
        .unwrap()
    }

    #[test]
    fn two_medical_beds_and_one_medic_stay_in_camp_admission_radius() {
        let camp = fixture();
        let a = camp.local_anchors;
        assert_eq!(a.medical_beds.map(|b| b.id), [1, 2]);
        assert_eq!(a.quarters.len(), 16);
        for bed in a.medical_beds {
            assert!(
                bed.patient_pose
                    .transform_point3(Vec3::ZERO)
                    .distance(a.origin)
                    < MEDICAL_RADIUS_M
            );
            assert!(bed.attend_from.distance(a.origin) < MEDICAL_RADIUS_M);
        }
        assert!(a.medic.distance(a.origin) < MEDICAL_RADIUS_M);
        assert!(a.ammo.distance(a.fuel) > 5.0);
    }

    #[test]
    #[ignore = "private camp source assets; verifies actual source shelf and case contact"]
    fn stocked_cases_rest_on_visible_source_decks_or_other_cases() {
        let camp = SupportCamp::load().unwrap();
        for (stock_prefix, rack_prefix, count) in [
            ("medical.aid.case.", "medical.supply.rack.", 18),
            ("ammunition.smallarms.box.", "ammunition.smallarms.rack", 18),
        ] {
            let stock: Vec<_> = camp.parts.iter().filter(|p|p.name.starts_with(stock_prefix)).collect();
            assert_eq!(stock.len(), count, "rearrangement must not invent supply props");
            for item in stock {
                let center = (item.mesh.min+item.mesh.max)*0.5;
                let bottom = item.mesh.min.y;
                let mut support = f32::NEG_INFINITY;
                for part in camp.parts.iter().filter(|p|p.name!=item.name
                    && (p.name.starts_with(stock_prefix)||p.name.starts_with(rack_prefix))) {
                    for tri in part.mesh.indices.chunks_exact(3) {
                        let [a,b,c] = [0,1,2].map(|k|Vec3::from_array(part.mesh.vertices[tri[k]as usize].pos));
                        let normal=(b-a).cross(c-a);
                        if normal.y<=1e-8 || normal.x.abs()+normal.z.abs()>normal.y*1e-3 {continue;}
                        let xz=|v:Vec3|glam::Vec2::new(v.x,v.z);
                        let u=xz(b-a);let v=xz(c-a);let p=xz(center-a);
                        let denominator=u.perp_dot(v);
                        if denominator.abs()<1e-10 {continue;}
                        let s=p.perp_dot(v)/denominator;let t=u.perp_dot(p)/denominator;
                        if s>=-1e-5 && t>=-1e-5 && s+t<=1.00001 && a.y<=bottom+0.001 {
                            support=support.max(a.y);
                        }
                    }
                }
                assert!((bottom-support).abs()<0.001,
                    "{} bottom {} has no matching drawn support (highest {})",item.name,bottom,support);
            }
        }
    }

    #[test]
    fn compact_camp_conforms_without_modifying_the_terrain_and_only_workshop_has_slab() {
        let mut camp=fixture();
        let hf=crate::terrain::Heightfield{res:129,spacing:1.,
            heights:(0..129*129).map(|i|(i%129)as f32*0.015).collect(),
            min_h:0.,max_h:1.92,openings:Default::default(),detail:None};
        let before=hf.heights.clone();
        camp.conform_to_terrain(Mat4::IDENTITY,&hf).unwrap();
        assert_eq!(before,hf.heights);
        let slabs:Vec<_>=camp.parts.iter().filter(|p|p.name.contains("concrete")).collect();
        assert_eq!(slabs.len(),1);
        assert!(((slabs[0].mesh.max-slabs[0].mesh.min).x-7.).abs()<0.001);
        let (lo,hi)=camp.bounds();assert!(hi.x-lo.x<23.&&hi.z-lo.z<29.);
        assert_eq!(camp.parts.iter().filter(|p|p.name.starts_with("fuel.drum")).count(),12);
        assert_eq!(camp.parts.iter().filter(|p|p.name.starts_with("medical.bed")).count(),2);
        assert_eq!(camp.parts.iter().filter(|p|p.name.starts_with("medical.spare.stretcher")).count(),2);
    }

    #[test]
    fn collision_is_the_placed_visible_source_and_relocation_keeps_it_registered() {
        let camp = fixture();
        let root = Mat4::from_rotation_translation(
            Quat::from_rotation_y(0.35),
            Vec3::new(100.0, 2.0, -40.0),
        );
        let mut scene = Vec::new();
        let mut placed = camp.append_to_scene(&mut scene, root).unwrap();
        for tri in &placed.triangles {
            let mesh = &scene[tri.scene_mesh];
            for (k, index) in mesh.indices[tri.source_triangle * 3..tri.source_triangle * 3 + 3]
                .iter()
                .enumerate()
            {
                assert_eq!(
                    tri.vertices[k],
                    Vec3::from_array(mesh.vertices[*index as usize].pos)
                );
            }
        }
        let next = Mat4::from_translation(Vec3::new(-20.0, 1.0, 90.0));
        let models = placed.relocate(next).unwrap();
        assert!(
            placed
                .anchors
                .origin
                .distance(next.transform_point3(Vec3::ZERO))
                < 1e-4
        );
        for tri in &placed.triangles {
            let (slot, model) = models[tri.scene_mesh];
            assert_eq!(slot, tri.scene_mesh);
            let mesh = &scene[slot];
            let vertex =
                Vec3::from_array(mesh.vertices[mesh.indices[tri.source_triangle * 3] as usize].pos);
            assert!(model.transform_point3(vertex).distance(tri.vertices[0]) < 1e-4);
        }
        assert!(camp
            .append_to_scene(&mut scene, Mat4::from_scale(Vec3::splat(2.0)))
            .is_err());
    }

    #[test]
    fn medical_door_is_open_and_camp_geometry_is_finite() {
        let camp = fixture();
        for part in &camp.parts {
            assert!(part
                .mesh
                .vertices
                .iter()
                .all(|v| Vec3::from_array(v.pos).is_finite()
                    && Vec3::from_array(v.normal).is_normalized()));
        }
        let canvas = &camp.parts[0].mesh;
        // Door faces +X at x=-3.1; its centre corridor z=-1..1 stays empty below the eave.
        for triangle in canvas.indices.chunks_exact(3) {
            let p = [0, 1, 2].map(|k| Vec3::from_array(canvas.vertices[triangle[k] as usize].pos));
            if p.iter().all(|v| (v.x + 3.1).abs() < 1e-4) && p.iter().any(|v| v.y < 1.9) {
                assert!(p.iter().all(|v| v.z >= 0.999) || p.iter().all(|v| v.z <= -0.999));
            }
        }
    }

    #[test]
    #[ignore = "private camp source assets; CPU only"]
    fn actual_private_props_load_into_the_camp() {
        let camp = SupportCamp::load().unwrap();
        assert_eq!(
            camp.parts.iter().filter(|p| p.source == STRETCHER).count(),
            4 // two active treatment beds plus two spare physical stretcher stations
        );
        assert!(camp.parts.iter().any(|p| p.source == BENCH));
    }

    #[test]
    #[ignore = "private camp source assets; measured furniture clearance"]
    fn medical_spare_stretchers_leave_a_source_clear_floor_aisle() {
        let camp=SupportCamp::load().unwrap();
        assert_eq!(camp.local_anchors.medical_beds.map(|b|b.id),[1,2]);
        assert_eq!(camp.parts.iter().filter(|p|p.source==STRETCHER).count(),4);
        for part in camp.parts.iter().filter(|p|p.name.starts_with("medical.spare.stretcher.")) {
            assert!(part.mesh.min.z>=-2.701&&part.mesh.max.z<=2.701,"spare leaves original canvas clearance");
            assert!(part.mesh.min.x>-5.02&&part.mesh.max.x< -4.38);
            for rack in camp.parts.iter().filter(|p|p.name.starts_with("medical.supply.rack.")) {
                assert!(part.mesh.min.x-rack.mesh.max.x>0.15,"spare must not overlap a supply rack");
            }
        }
        let stands=&camp.parts.iter().find(|p|p.name=="medical.stretcher.stands").unwrap().mesh;
        let inner=stands.vertices.iter().map(|v|Vec3::from_array(v.pos)).filter(|p|p.x> -5.2)
            .map(|p|p.z.abs()).fold(f32::INFINITY,f32::min);
        assert!(inner*2.>=1.69,"source stand floor aisle is only {}m",inner*2.);
        assert!(stands.vertices.iter().map(|v|Vec3::from_array(v.pos)).all(|p|p.z.abs()<2.9));
    }
}
