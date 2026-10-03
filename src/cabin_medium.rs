//! Finite Tiger weather-exclusion domains from the actual source floor, roof and closed lids.
//!
//! Each rigid frame has a 2 cm XZ grid of source vertical intervals. Unlike a convex hull this
//! retains the stepped bow and rear deck recesses. The only synthetic faces are explicit medium
//! interfaces at the source turret-ring and cupola rims: they partition connected compartment
//! air between independently moving frames, and never occlude light. Opening a hatch exposes the
//! same finite air domain; it does not turn the entire cabin into outdoor blizzard instantly.
//!
//! This is a sampled source enclosure, not an exact air/steel Boolean or a ventilation solver.
//! It includes armor thickness and internal solid objects; rendered geometry still stops rays.
//! Each interval endpoint is a source-triangle/rim-membrane intersection at the cell center.
//! The constant-height column extends that point at most sqrt(2)*0.01 m horizontally. Sharp
//! roof steps and narrow features are resolved at 2 cm. That sampling statement is NOT a global
//! Hausdorff/topology guarantee. Disconnected columns are removed using registered cabin seeds.
use anyhow::{ensure, Context};
use glam::{Mat4, Vec2, Vec3};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

pub const DOMAINS: usize = 3;
pub const PITCH_M: f32 = 0.02;
/// Same boundary ownership tolerance as shaders/cabin_medium.wgsl.
pub(crate) const CABIN_RAY_EPSILON:f32=0.00001;
const EMPTY: [f32; 2] = [1.0, -1.0];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniform {
    pub world_to_local: [[[f32; 4]; 4]; DOMAINS],
    /// local minimum X, local minimum Z, pitch, enabled
    pub grid_min_step: [[f32; 4]; DOMAINS],
    /// active column width, height, texture layer, reserved
    pub grid_size: [[u32; 4]; DOMAINS],
    pub bounds_min: [[f32; 4]; DOMAINS],
    pub bounds_max: [[f32; 4]; DOMAINS],
}
impl Default for Uniform {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

#[derive(Clone)]
pub struct Medium {
    source: Arc<Source>,
}
struct Source {
    domains: [Domain; DOMAINS],
    texture: Vec<[f32; 2]>,
    size: [u32; 3],
}
struct Domain {
    label: &'static str,
    min: Vec3,
    max: Vec3,
    size: [u32; 2],
    cells: Vec<[f32; 2]>,
    source_triangles: usize,
    detached_columns: usize,
}
type Triangle = [Vec3; 3];

const HULL_ROOTS: &[&str] = &[
    "Hull_0619",
    "Hull_0708",
    "Hull_1174",
    "Hull_0205",
    "Hull_0849",
    "Hull_0975",
    "Drivers Hatch Pivot",
    "MG Operater",
];
const TURRET_ROOTS: &[&str] = &[
    "Turret_0101",
    "Turret_0105",
    "Turret_0115",
    "Turret_0116",
    "gunner_hatch",
    "escape hatch",
    "Turret_0113",
    "Turret_0159",
];
// These source rig registrations match tiger_interior.json / game::cupola_hatch_model.
const TURRET_PIVOT: Vec3 = Vec3::new(0.0, 0.524, 0.0192);
const CUPOLA_HINGE: Vec3 = Vec3::new(0.963, 2.614, -0.333);

/// Read positions only, with the same complete node-parent transforms as the runtime GLB loader.
/// A named root includes its complete source child assembly (especially the roof hatch discs).
fn source_parts(path: &str, roots: &[&str]) -> anyhow::Result<BTreeMap<String, Vec<Triangle>>> {
    let bytes = crate::resource::read(path)?;
    let gltf =
        gltf::Gltf::from_slice(&bytes).with_context(|| format!("parse cabin source {path}"))?;
    let blob = gltf
        .blob
        .as_deref()
        .context("cabin source must be a self-contained GLB")?;
    let mut parts = BTreeMap::<String, Vec<Triangle>>::new();
    fn visit(
        node: gltf::Node<'_>,
        parent: Mat4,
        selected: Option<&str>,
        roots: &[&str],
        blob: &[u8],
        parts: &mut BTreeMap<String, Vec<Triangle>>,
    ) -> anyhow::Result<()> {
        let model = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        let name = node.name().unwrap_or("");
        let selected = if roots.contains(&name) {
            Some(name)
        } else {
            selected
        };
        if let (Some(root), Some(mesh)) = (selected, node.mesh()) {
            let out = parts.entry(root.to_owned()).or_default();
            for primitive in mesh.primitives() {
                ensure!(
                    primitive.mode() == gltf::mesh::Mode::Triangles,
                    "non-triangle cabin source"
                );
                let reader = primitive.reader(|buffer| match buffer.source() {
                    gltf::buffer::Source::Bin => Some(blob),
                    _ => None,
                });
                let positions: Vec<_> = reader
                    .read_positions()
                    .context("cabin source positions")?
                    .map(|p| model.transform_point3(Vec3::from(p)))
                    .collect();
                let indices: Vec<u32> = reader
                    .read_indices()
                    .context("indexed cabin source")?
                    .into_u32()
                    .collect();
                for t in indices.chunks_exact(3) {
                    let triangle = [
                        positions[t[0] as usize],
                        positions[t[1] as usize],
                        positions[t[2] as usize],
                    ];
                    ensure!(
                        triangle.iter().all(|p| p.is_finite()),
                        "nonfinite cabin source"
                    );
                    out.push(triangle);
                }
            }
        }
        for child in node.children() {
            visit(child, model, selected, roots, blob, parts)?;
        }
        Ok(())
    }
    let scene = gltf
        .default_scene()
        .or_else(|| gltf.scenes().next())
        .context("cabin source scene")?;
    for node in scene.nodes() {
        visit(node, Mat4::IDENTITY, None, roots, blob, &mut parts)?;
    }
    for root in roots {
        ensure!(
            parts.get(*root).is_some_and(|v| !v.is_empty()),
            "missing cabin root {root}"
        );
    }
    Ok(parts)
}

/// A medium interface spans the actual projected circular rim, at that rim's source minimum Y.
/// Its convex 2D perimeter is justified only for these two circular source ring assemblies;
/// no hull/turret cabin geometry is replaced with a convex outer envelope.
fn rim_membrane(source: &[Triangle]) -> Vec<Triangle> {
    let min_y = source
        .iter()
        .flatten()
        .map(|p| p.y)
        .fold(f32::INFINITY, f32::min);
    let mut points: Vec<Vec2> = source
        .iter()
        .flatten()
        .map(|p| Vec2::new(p.x, p.z))
        .collect();
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    fn append(hull: &mut Vec<Vec2>, point: Vec2) {
        while hull.len() >= 2
            && (hull[hull.len() - 1] - hull[hull.len() - 2]).perp_dot(point - hull[hull.len() - 1])
                <= 0.0
        {
            hull.pop();
        }
        hull.push(point);
    }
    let mut lower = Vec::new();
    let mut upper = Vec::new();
    for &point in &points {
        append(&mut lower, point);
    }
    for &point in points.iter().rev() {
        append(&mut upper, point);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    let vertex = |p: Vec2| Vec3::new(p.x, min_y, p.y);
    (1..lower.len() - 1)
        .map(|i| [vertex(lower[0]), vertex(lower[i]), vertex(lower[i + 1])])
        .collect()
}

impl Domain {
    fn build(
        label: &'static str,
        triangles: Vec<Triangle>,
        seeds: &[Vec3],
    ) -> anyhow::Result<Self> {
        ensure!(!triangles.is_empty(), "empty cabin domain {label}");
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for &p in triangles.iter().flatten() {
            min = min.min(p);
            max = max.max(p);
        }
        let grid_min = (min / PITCH_M).floor() * PITCH_M;
        let grid_max = (max / PITCH_M).ceil() * PITCH_M;
        let width = ((grid_max.x - grid_min.x) / PITCH_M).round() as u32;
        let height = ((grid_max.z - grid_min.z) / PITCH_M).round() as u32;
        ensure!(
            width > 0 && height > 0 && width < 1024 && height < 1024,
            "invalid cabin grid {label}"
        );
        let mut cells = vec![[f32::INFINITY, f32::NEG_INFINITY]; (width * height) as usize];
        // Source triangles are normally tiny. Projected-bounds rasterization avoids an all-rays ×
        // all-triangles scan and creates the source support once, without decoding any textures.
        for &[aa, bb, cc] in &triangles {
            let a = aa.as_dvec3();
            let u = (bb - aa).as_dvec3();
            let v = (cc - aa).as_dvec3();
            let determinant = u.x * v.z - u.z * v.x;
            if determinant.abs() < 1e-13 {
                continue;
            }
            let lo = aa.min(bb).min(cc);
            let hi = aa.max(bb).max(cc);
            let x0 = (((lo.x - grid_min.x) / PITCH_M - 0.5).ceil() as i32).max(0);
            let x1 = (((hi.x - grid_min.x) / PITCH_M - 0.5).floor() as i32).min(width as i32 - 1);
            let z0 = (((lo.z - grid_min.z) / PITCH_M - 0.5).ceil() as i32).max(0);
            let z1 = (((hi.z - grid_min.z) / PITCH_M - 0.5).floor() as i32).min(height as i32 - 1);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let qx = f64::from(grid_min.x) + (x as f64 + 0.5) * f64::from(PITCH_M) - a.x;
                    let qz = f64::from(grid_min.z) + (z as f64 + 0.5) * f64::from(PITCH_M) - a.z;
                    let s = (qx * v.z - qz * v.x) / determinant;
                    let t = (u.x * qz - u.z * qx) / determinant;
                    if s >= -1e-7 && t >= -1e-7 && s + t <= 1.0 + 1e-7 {
                        let y = (a.y + s * u.y + t * v.y) as f32;
                        let cell = &mut cells[z as usize * width as usize + x as usize];
                        cell[0] = cell[0].min(y);
                        cell[1] = cell[1].max(y);
                    }
                }
            }
        }
        let mut keep = vec![false; cells.len()];
        let mut queue = VecDeque::new();
        let index_of = |point: Vec3| -> Option<usize> {
            let x = ((point.x - grid_min.x) / PITCH_M).floor() as i32;
            let z = ((point.z - grid_min.z) / PITCH_M).floor() as i32;
            if x >= 0 && z >= 0 && x < width as i32 && z < height as i32 {
                Some(z as usize * width as usize + x as usize)
            } else {
                None
            }
        };
        for &seed in seeds {
            let i = index_of(seed).with_context(|| format!("{label} seed outside source grid"))?;
            ensure!(
                cells[i][0] <= seed.y && seed.y <= cells[i][1],
                "{label} source closure misses seed {seed:?}: {:?}",
                cells[i]
            );
            if !keep[i] {
                keep[i] = true;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            let x = i % width as usize;
            let z = i / width as usize;
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let nx = x as i32 + dx;
                let nz = z as i32 + dz;
                if nx < 0 || nz < 0 || nx >= width as i32 || nz >= height as i32 {
                    continue;
                }
                let j = nz as usize * width as usize + nx as usize;
                let overlap = cells[i][1].min(cells[j][1]) - cells[i][0].max(cells[j][0]);
                if !keep[j] && overlap > 1e-6 {
                    keep[j] = true;
                    queue.push_back(j);
                }
            }
        }
        let mut detached = 0;
        for (i, cell) in cells.iter_mut().enumerate() {
            if !keep[i] || cell[1] <= cell[0] {
                if cell[1] > cell[0] {
                    detached += 1;
                }
                *cell = EMPTY;
            }
        }
        // Bounds include the whole final cell footprint. The selected source, not these bounds,
        // supplies every accepted interval; an AABB can never independently clear weather.
        min.x = grid_min.x;
        min.z = grid_min.z;
        max.x = grid_max.x;
        max.z = grid_max.z;
        Ok(Self {
            label,
            min,
            max,
            size: [width, height],
            cells,
            source_triangles: triangles.len(),
            detached_columns: detached,
        })
    }
    fn contains(&self, p: Vec3) -> bool {
        let x = ((p.x - self.min.x) / PITCH_M).floor() as i32;
        let z = ((p.z - self.min.z) / PITCH_M).floor() as i32;
        if x < 0 || z < 0 || x >= self.size[0] as i32 || z >= self.size[1] as i32 {
            return false;
        }
        let cell = self.cells[z as usize * self.size[0] as usize + x as usize];
        cell[0] <= p.y && p.y <= cell[1]
    }
}

impl Medium {
    pub fn load() -> anyhow::Result<Self> {
        static SOURCE: std::sync::OnceLock<Result<Arc<Source>, String>> =
            std::sync::OnceLock::new();
        let source = SOURCE.get_or_init(|| {
            Self::build()
                .map(Arc::new)
                .map_err(|error| format!("{error:#}"))
        });
        Ok(Self {
            source: source
                .as_ref()
                .map_err(|error| anyhow::anyhow!(error.clone()))?
                .clone(),
        })
    }
    fn build() -> anyhow::Result<Source> {
        let hull = source_parts("assets/tanks/tiger/baked/tiger_hull.glb", HULL_ROOTS)?;
        let turret = source_parts("assets/tanks/tiger/baked/tiger_turret.glb", TURRET_ROOTS)?;
        let hatch = source_parts(
            "assets/tanks/tiger/baked/tiger_hatch.glb",
            &["cupola hatch disc.001"],
        )?;
        let ring = rim_membrane(&turret["Turret_0113"]);
        let cupola_interface = rim_membrane(&turret["Turret_0159"]);
        let mut hull_tri: Vec<_> = hull.values().flatten().copied().collect();
        hull_tri.extend(ring.iter().map(|t| t.map(|p| p + TURRET_PIVOT)));
        let mut turret_tri: Vec<_> = TURRET_ROOTS[..6]
            .iter()
            .flat_map(|name| turret[*name].iter().copied())
            .collect();
        turret_tri.extend(ring);
        turret_tri.extend(cupola_interface.iter().copied());
        let mut cupola_tri = turret["Turret_0159"].clone();
        cupola_tri.extend(cupola_interface);
        cupola_tri.extend(
            hatch["cupola hatch disc.001"]
                .iter()
                .map(|t| t.map(|p| p + CUPOLA_HINGE)),
        );
        let domains = [
            Domain::build(
                "hull",
                hull_tri,
                &[
                    Vec3::new(0.5354, 1.6572, 1.5179),
                    Vec3::new(-0.66, 1.58, 1.48),
                    Vec3::new(0.0, 1.5, 0.0),
                ],
            )?,
            Domain::build(
                "turret",
                turret_tri,
                &[
                    Vec3::new(-0.85, 2.0334, 0.15),
                    Vec3::new(0.4966, 1.994, 0.5398),
                    Vec3::new(0.0, 1.54, 0.0),
                ],
            )?,
            Domain::build("cupola", cupola_tri, &[Vec3::new(0.564, 2.415, -0.423)])?,
        ];
        let width = domains.iter().map(|d| d.size[0]).max().unwrap();
        let height = domains.iter().map(|d| d.size[1]).max().unwrap();
        let mut texture = vec![EMPTY; (width * height * DOMAINS as u32) as usize];
        for (layer, domain) in domains.iter().enumerate() {
            for z in 0..domain.size[1] as usize {
                let src = z * domain.size[0] as usize;
                let dst = (layer * height as usize + z) * width as usize;
                texture[dst..dst + domain.size[0] as usize]
                    .copy_from_slice(&domain.cells[src..src + domain.size[0] as usize]);
            }
        }
        Ok(Source {
            domains,
            texture,
            size: [width, height, DOMAINS as u32],
        })
    }
    pub fn texture_data(&self) -> &[[f32; 2]] {
        &self.source.texture
    }
    pub fn texture_size(&self) -> [u32; 3] {
        self.source.size
    }
    pub fn uniform(&self, hull: Mat4, turret: Mat4) -> Uniform {
        let mut out = Uniform::default();
        for (i, domain) in self.source.domains.iter().enumerate() {
            let model = if i == 0 { hull } else { turret };
            if !model.is_finite() || model.determinant().abs() < 1e-10 {
                continue;
            }
            out.world_to_local[i] = model.inverse().to_cols_array_2d();
            out.grid_min_step[i] = [domain.min.x, domain.min.z, PITCH_M, 1.0];
            out.grid_size[i] = [domain.size[0], domain.size[1], i as u32, 0];
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for bits in 0..8 {
                let local = Vec3::new(
                    if bits & 1 == 0 {
                        domain.min.x
                    } else {
                        domain.max.x
                    },
                    if bits & 2 == 0 {
                        domain.min.y
                    } else {
                        domain.max.y
                    },
                    if bits & 4 == 0 {
                        domain.min.z
                    } else {
                        domain.max.z
                    },
                );
                let world = model.transform_point3(local);
                min = min.min(world);
                max = max.max(world);
            }
            out.bounds_min[i] = min.extend(0.0).to_array();
            out.bounds_max[i] = max.extend(0.0).to_array();
        }
        out
    }
    /// CPU counterpart of the existing shader column DDA, for gameplay sight through the same
    /// finite medium domains. AABB rejection never substitutes for the actual source intervals.
    fn domain_run(&self,u:&Uniform,origin:Vec3,ray:Vec3,start:f32,limit:f32,index:usize)->[f32;2] {
        let empty=[limit,limit];let grid=u.grid_min_step[index];let size=u.grid_size[index];
        if grid[3]<0.5 {return empty;}
        let mut begin=start.max(0.0);let mut end=limit;
        for axis in 0..3 {
            let lo=u.bounds_min[index][axis];let hi=u.bounds_max[index][axis];
            if ray[axis].abs()<1e-8 {if origin[axis]<lo||origin[axis]>hi {return empty;}continue;}
            let a=(lo-origin[axis])/ray[axis];let b=(hi-origin[axis])/ray[axis];
            begin=begin.max(a.min(b));end=end.min(a.max(b));
        }
        if end<=begin {return empty;}
        let model=Mat4::from_cols_array_2d(&u.world_to_local[index]);
        let p=model.transform_point3(origin);let d=model.transform_vector3(ray);
        let axes=[0,2];
        for axis in 0..2 {
            let v=d[axes[axis]];let o=p[axes[axis]];let hi=grid[axis]+size[axis] as f32*grid[2];
            if v.abs()<1e-8 {if o<grid[axis]||o>=hi {return empty;}continue;}
            let a=(grid[axis]-o)/v;let b=(hi-o)/v;begin=begin.max(a.min(b));end=end.min(a.max(b));
        }
        if end<=begin {return empty;}
        let mut cursor=begin;let mut run=empty;let mut found=false;
        while cursor<end {
            let sample=(cursor+CABIN_RAY_EPSILON).min((cursor+end)*0.5);
            let mut cell=[0i32;2];
            for axis in 0..2 {cell[axis]=(((p[axes[axis]]+d[axes[axis]]*sample-grid[axis])/grid[2]).floor() as i32)
                .clamp(0,size[axis] as i32-1);}
            let mut next=end;
            for axis in 0..2 {let v=d[axes[axis]];if v.abs()<1e-8 {continue;}
                let edge=grid[axis]+(cell[axis]+if v>0.0 {1}else{0}) as f32*grid[2];
                let crossing=(edge-p[axes[axis]])/v;
                if crossing>cursor+CABIN_RAY_EPSILON*0.1 {next=next.min(crossing);}}
            let column=self.source.domains[index].cells[cell[1] as usize*size[0] as usize+cell[0] as usize];
            let mut lo=cursor;let mut hi=next;
            if column[1]<=column[0] {hi=lo;}
            else if d.y.abs()<1e-8 {if p.y<column[0]||p.y>column[1] {hi=lo;}}
            else {let a=(column[0]-p.y)/d.y;let b=(column[1]-p.y)/d.y;
                lo=lo.max(a.min(b));hi=hi.min(a.max(b));}
            if hi>lo {if !found {run=[lo,hi];found=true;}
                else if lo<=run[1]+CABIN_RAY_EPSILON {run[1]=run[1].max(hi);}
                else {return run;}}
            else if found {return run;}
            if next<=cursor {break;}cursor=next;
        }
        run
    }
    pub(crate) fn next_span(&self,u:&Uniform,origin:Vec3,ray:Vec3,start:f32,limit:f32)->[f32;2] {
        let mut runs=[[limit;2];DOMAINS];let mut result=[limit;2];
        for (i,run) in runs.iter_mut().enumerate() {*run=self.domain_run(u,origin,ray,start,limit,i);
            if run[0]<result[0] {result=*run;}}
        if result[1]<=result[0] {return result;}
        loop {let before=result[1];
            for (i,run) in runs.iter_mut().enumerate() {
                if run[1]<result[1]-CABIN_RAY_EPSILON {*run=self.domain_run(u,origin,ray,
                    start.max(result[1]-CABIN_RAY_EPSILON),limit,i);}
                if run[0]<=result[1]+CABIN_RAY_EPSILON && run[1]>run[0] {result[1]=result[1].max(run[1]);}
            }
            if result[1]<=before+CABIN_RAY_EPSILON {break;}
        }
        result
    }

    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({"representation":"source floor/roof vertical interval grids with source ring-interface membranes",
            "pitch_m":PITCH_M,"texture_size":self.texture_size(),"uniform_bytes":std::mem::size_of::<Uniform>(),
            "domains":self.source.domains.iter().map(|d|serde_json::json!({"name":d.label,"grid_size":d.size,
                "source_triangles":d.source_triangles,"detached_columns_removed":d.detached_columns,
                "active_columns":d.cells.iter().filter(|v|v[1]>v[0]).count(),
                "local_min":d.min.to_array(),"local_max":d.max.to_array()})).collect::<Vec<_>>(),
            "limits":"2cm projected source sampling; includes armor/internal solids. Circular source-rim medium interfaces partition connected air. No ventilation. Endpoint sampling is not a formal global topology/Hausdorff guarantee."})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn contains(medium: &Medium, uniform: &Uniform, p: Vec3) -> bool {
        medium.source.domains.iter().enumerate().any(|(i, d)| {
            uniform.grid_min_step[i][3] > 0.0
                && d.contains(
                    Mat4::from_cols_array_2d(&uniform.world_to_local[i]).transform_point3(p),
                )
        })
    }
    #[test]
    fn gameplay_weather_uses_actual_source_cabin_spans_after_articulation() {
        let medium=Medium::load().unwrap();
        let hull=Mat4::from_translation(Vec3::new(120.0,9.0,-31.0))*Mat4::from_rotation_y(0.7);
        let u=medium.uniform(hull,hull);
        let inside=hull.transform_point3(Vec3::new(0.547,1.738,2.386));
        let ray=hull.transform_vector3(Vec3::Z);
        let span=medium.next_span(&u,inside,ray,0.0,100.0);
        assert_eq!(span[0],0.0);
        assert!(span[1]>0.0&&span[1]<0.20,"source front recess must reopen exterior weather: {span:?}");
        let outside=hull.transform_point3(Vec3::new(0.547,1.738,2.8));
        let clear=medium.next_span(&u,outside,ray,0.0,10.0);
        assert_eq!(clear,[10.0,10.0]);
        // A short segment entirely in the sampled source cavity has no exterior optical depth.
        let v=crate::weather_visibility::WeatherView{sigma:1.0,cabin:Some(Arc::new(medium)),
            cabin_uniform:u,..Default::default()};
        assert_eq!(v.transmittance(inside,inside+ray*0.01),1.0);
        assert!(v.transmittance(inside,inside+ray*10.0)<0.001);
    }

    #[test]
    fn source_columns_remove_front_and_rear_convex_free_air() {
        let medium = Medium::load().unwrap();
        let hull = &medium.source.domains[0];
        assert!(hull.contains(Vec3::new(0.547, 1.738, 2.386)));
        for z in [2.50, 2.60, 2.80] {
            assert!(
                !hull.contains(Vec3::new(0.547, 1.738, z)),
                "front recess {z}"
            );
        }
        assert!(
            !hull.contains(Vec3::new(-0.75, 2.10, -2.60)),
            "no false cabin above rear grille"
        );
        assert_eq!(std::mem::size_of::<Uniform>(), 384);
        let size = medium.texture_size();
        assert_eq!(
            medium.texture_data().len(),
            (size[0] * size[1] * size[2]) as usize
        );
        assert!(medium
            .texture_data()
            .iter()
            .flatten()
            .all(|v| v.is_finite()));
    }
    #[test]
    fn cabin_and_portal_samples_follow_tilt_and_rotating_turret() {
        let medium = Medium::load().unwrap();
        let hull = Mat4::from_translation(Vec3::new(60.0, 3.75, -280.0))
            * Mat4::from_rotation_y(0.47)
            * Mat4::from_rotation_x(0.08);
        for yaw in [0.0, 0.7, 1.87, 3.14] {
            let turret = hull * Mat4::from_translation(TURRET_PIVOT) * Mat4::from_rotation_y(yaw);
            let uniform = medium.uniform(hull, turret);
            for p in [
                Vec3::new(0.5354, 1.6572, 1.5179),
                Vec3::new(-0.66, 1.58, 1.48),
                Vec3::new(0.0, 1.5, 0.0),
            ] {
                assert!(
                    contains(&medium, &uniform, hull.transform_point3(p)),
                    "hull {p:?}"
                );
            }
            for p in [
                Vec3::new(-0.85, 2.0334, 0.15),
                Vec3::new(0.4966, 1.994, 0.5398),
                Vec3::new(0.564, 2.415, -0.423),
                Vec3::new(0.0, 1.54, 0.0),
            ] {
                assert!(
                    contains(&medium, &uniform, turret.transform_point3(p)),
                    "turret {p:?} yaw{yaw}"
                );
            }
            for (frame, p) in [
                (hull, Vec3::new(0.9768873, 2.251, 1.8890174)),
                (turret, Vec3::new(-0.7044487, 2.668445, -0.1133228)),
                (turret, Vec3::new(0.5643373, 2.8945968, -0.423)),
                (hull, Vec3::new(0.0, 0.30, 0.0)),
            ] {
                assert!(
                    !contains(&medium, &uniform, frame.transform_point3(p)),
                    "exterior {p:?}"
                );
            }
        }
    }
    #[test]
    fn sampled_roof_recess_and_disconnected_plate_do_not_fill_air() {
        let plane = |x: f32, z: f32, y: f32| {
            [
                Vec3::new(x, y, z),
                Vec3::new(x + 0.10, y, z),
                Vec3::new(x, y, z + 0.10),
            ]
        };
        let triangles = vec![
            plane(0.0, 0.0, 0.0),
            plane(0.0, 0.0, 0.20),
            plane(0.50, 0.50, 0.0),
            plane(0.50, 0.50, 0.50),
        ];
        let domain = Domain::build("fixture", triangles, &[Vec3::new(0.03, 0.10, 0.03)]).unwrap();
        assert!(domain.contains(Vec3::new(0.03, 0.10, 0.03)));
        assert!(!domain.contains(Vec3::new(0.03, 0.25, 0.03)));
        assert!(!domain.contains(Vec3::new(0.53, 0.10, 0.53)));
        assert!(domain.detached_columns > 0);
    }
}
