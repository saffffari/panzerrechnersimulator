//! Three independent, bounded grass populations preserve close blades and distant field cover.
//! World-fixed roots, smooth overlapping detail transitions, shared wind and persistent treads.
use crate::renderer::Renderer;
use glam::{Mat4, Quat, Vec2, Vec3};
use std::collections::{HashMap, HashSet};
#[path = "grass_filament.rs"]
mod filament;
use filament::{Bend, TrackMemory};

const LAYERS: usize = 3;
const CAPACITIES: [usize; LAYERS] = [32_768, 24_576, 36_864];
// Winter distributes stems across overlapping patches, with a bounded instance budget.
const WINTER_CAPACITIES: [usize; LAYERS] = [2048, 16384, 20000];
pub const CAPACITY: usize = CAPACITIES[0] + CAPACITIES[1] + CAPACITIES[2];
const CELLS: [f32; LAYERS] = [2.0, 2.0, 8.0];
const CANDIDATES: [u32; LAYERS] = [1024, 4, 4];
const OUTER: [f32; LAYERS] = [14.0, 80.0, 400.0];
const INNER: [f32; LAYERS] = [0.0, 0.0, 36.0];
const FADE_IN: [f32; LAYERS] = [0.0, 0.0, 16.0];
const FADE_OUT: [f32; LAYERS] = [1.5, 20.0, 96.0];
const FOOTPRINT: [f32; LAYERS] = [0.16, 1.15, 4.35];
const TRIANGLES: [usize; LAYERS] = [12, 12, 8];
const BLADES: [usize; LAYERS] = [3, 1, 0];
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn near_density(d: f32) -> f32 {
    if d < 2.5 {
        256.0
    } else if d < 5.0 {
        256.0 - 224.0 * smooth(2.5, 5.0, d)
    } else if d < 10.0 {
        32.0 - 29.0 * smooth(5.0, 10.0, d)
    } else {
        3.0 * (1.0 - smooth(10.0, 14.0, d))
    }
}

/// Precompute each world-fixed candidate rank's visibility radius. Camera cell changes never
/// re-roll roots or switch detail instantaneously; the entire outer boundary is already hidden.
fn rank_radius(layer: usize, rank: u32) -> f32 {
    if layer > 0 {
        return OUTER[layer];
    }
    let density = (rank as f32 + 0.5) / (CELLS[0] * CELLS[0]);
    let (mut lo, mut hi) = (0.0, OUTER[0]);
    for _ in 0..16 {
        let mid = (lo + hi) * 0.5;
        if near_density(mid) > density {
            lo = mid
        } else {
            hi = mid
        }
    }
    (lo + hi) * 0.5
}
fn visibility(layer: usize, radius: f32, distance: f32) -> f32 {
    let outer = 1.0 - smooth((radius - FADE_OUT[layer]).max(0.0), radius, distance);
    outer
        * if FADE_IN[layer] == 0.0 {
            1.0
        } else {
            smooth(INNER[layer], INNER[layer] + FADE_IN[layer], distance)
        }
}
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 65536.0
}
fn winter_patch(x: f32, z: f32, seed: u32) -> f32 {
    let noise = |scale: f32, salt: u32| {
        let p = Vec2::new(x, z) / scale;
        let cell = p.floor();
        let t = p - cell;
        let t = t * t * (Vec2::splat(3.0) - t * 2.0);
        let value = |dx: i32, dz: i32| {
            hash01(
                seed ^ salt
                    ^ ((cell.x as i32 + dx) as u32).wrapping_mul(73_856_093)
                    ^ ((cell.y as i32 + dz) as u32).wrapping_mul(19_349_663),
            )
        };
        let a = value(0, 0) + (value(1, 0) - value(0, 0)) * t.x;
        let b = value(0, 1) + (value(1, 1) - value(0, 1)) * t.x;
        a + (b - a) * t.y
    };
    // Shared by all detail layers: low, coherent stands separated by broad *empty* snow.
    // World-fixed interpolation and a true zero region keep the gaps in place as the eye moves.
    smooth(
        0.38,
        0.64,
        noise(36.0, 0x8fe1_2943) * 0.82 + noise(8.0, 0x451e_9537) * 0.18,
    )
}
fn winter_density(layer: usize, patch: f32) -> f32 {
    // A broad stand fills with independent stems; the same mask leaves real snow clearings
    // in every detail layer. No isolated random tufts sprinkled through those clearings.
    [0.022, 0.42, 0.28][layer] * patch
}
/// Near blades use many independent roots. Coarse cover uses a jittered lattice: random
/// rejection and fully random positions leave meadow-sized holes even at high triangle counts.
/// The square canopy meshes overlap the maximum 20% root jitter without changing world roots.
fn candidate_position(layer: usize, cx: i32, cz: i32, rank: u32, key: u32) -> Vec2 {
    let cell = CELLS[layer];
    let random = Vec2::new(hash01(key.wrapping_add(1)), hash01(key.wrapping_add(2)));
    let local = if layer == 0 {
        random
    } else {
        (Vec2::new((rank % 2) as f32, (rank / 2) as f32)
            + Vec2::splat(0.5)
            + (random - Vec2::splat(0.5)) * 0.20)
            * 0.5
    };
    (Vec2::new(cx as f32, cz as f32) + local) * cell
}

struct Mask {
    w: usize,
    h: usize,
    rgba: Vec<u8>,
    half_extent: f32,
}
impl Mask {
    fn sample(&self, x: f32, z: f32) -> (f32, f32, f32) {
        if x.abs() > self.half_extent || z.abs() > self.half_extent {
            return (0.0, 0.0, 0.0);
        }
        let u = (x / (2.0 * self.half_extent) + 0.5).clamp(0.0, 1.0);
        let v = (z / (2.0 * self.half_extent) + 0.5).clamp(0.0, 1.0);
        let px = ((u * (self.w - 1) as f32) as usize).min(self.w - 1);
        let py = ((v * (self.h - 1) as f32) as usize).min(self.h - 1);
        let i = (py * self.w + px) * 4;
        (
            self.rgba[i] as f32 / 255.0,
            self.rgba[i + 1] as f32 / 255.0,
            self.rgba[i + 2] as f32 / 255.0,
        )
    }
    fn footprint(&self, x: f32, z: f32, r: f32) -> f32 {
        if x.abs() + r > self.half_extent || z.abs() + r > self.half_extent {
            return 0.0;
        }
        let pixel = |value: f32, size: usize| {
            ((value / (2.0 * self.half_extent) + 0.5) * (size - 1) as f32) as usize
        };
        let (x0, x1) = (pixel(x - r, self.w), pixel(x + r, self.w));
        let (z0, z1) = (pixel(z - r, self.h), pixel(z + r, self.h));
        let mut density = 1.0f32;
        // Visit all covered class texels, not just corners: even a narrow road through the
        // middle of a distant clump must keep that entire clump out of the road.
        for py in z0..=z1 {
            for px in x0..=x1 {
                let i = (py * self.w + px) * 4;
                density = density.min(
                    (self.rgba[i] as f32
                        + self.rgba[i + 1] as f32 * 0.28
                        + self.rgba[i + 2] as f32 * 0.12)
                        / 255.0,
                );
            }
        }
        density
    }
}
struct Root {
    model: Mat4,
    position: Vec3,
    radius: f32,
    wind_cell: (i32, i32),
    shelter: f32,
    compression: f32,
    target_compression: f32,
    direction: Vec2,
}
struct Layer {
    batch: usize,
    capacity: usize,
    center: Option<(i32, i32)>,
    roots: Vec<Root>,
    mats: Vec<Mat4>,
    radii: Vec<f32>,
    candidates_visited: usize,
}
impl Layer {
    fn new(batch: usize, kind: usize, winter: bool) -> Self {
        let capacity = if winter {
            WINTER_CAPACITIES[kind]
        } else {
            CAPACITIES[kind]
        };
        Self {
            batch,
            capacity,
            center: None,
            roots: Vec::with_capacity(capacity),
            mats: Vec::with_capacity(capacity),
            radii: (0..CANDIDATES[kind])
                .map(|k| rank_radius(kind, k))
                .collect(),
            candidates_visited: 0,
        }
    }
}
pub struct Grass {
    layers: [Layer; LAYERS],
    mask: Option<Mask>,
    mask_requested: bool,
    winter: bool,
    focus: Vec2,
    seed: u32,
    wind: HashMap<(i32, i32), Bend>,
    tracks: TrackMemory,
    contact_revision: u64,
}
impl Grass {
    pub fn new(
        r: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mask_path: Option<&str>,
        winter_authored: bool,
        half_extent: f32,
        seed: u32,
    ) -> Self {
        let mask = mask_path.and_then(|p| {
            match crate::resource::resolve(p)
                .and_then(|resolved| image::open(resolved).map_err(anyhow::Error::from))
            {
                Ok(img) => {
                    let rgba = img.into_rgba8();
                    let (w, h) = rgba.dimensions();
                    Some(Mask {
                        w: w as usize,
                        h: h as usize,
                        rgba: rgba.into_raw(),
                        half_extent,
                    })
                }
                Err(error) => {
                    println!("[grass] no ground-class mask at {p}: {error}; sparse dry stalks");
                    None
                }
            }
        });
        // A winter class map still describes dry stems. The former mask-presence switch
        // made authored road exclusions select summer geometry and density.
        let winter = winter_authored || mask.is_none();
        let layers = std::array::from_fn(|kind| {
            let batch = r
                .add_dynamic_instanced_pool(
                    device,
                    queue,
                    &filament::tuft_mesh(kind, winter),
                    if winter {
                        WINTER_CAPACITIES[kind]
                    } else {
                        CAPACITIES[kind]
                    },
                )
                .expect("bounded grass layer pool");
            r.set_batch_flags(batch, 4.0);
            Layer::new(batch, kind, winter)
        });
        Self {
            layers,
            mask,
            mask_requested: mask_path.is_some(),
            winter,
            focus: Vec2::ZERO,
            seed,
            wind: HashMap::new(),
            tracks: TrackMemory::default(),
            contact_revision: 0,
        }
    }
    pub fn stats(&self) -> serde_json::Value {
        let winter = self.winter;
        let blades = if winter {
            filament::WINTER_BLADES
        } else {
            BLADES
        };
        let triangles = if winter {
            filament::WINTER_TRIANGLES
        } else {
            TRIANGLES
        };
        let capacities: [usize; LAYERS] = std::array::from_fn(|i| self.layers[i].capacity);
        let within = |radius: f32| {
            self.layers[0]
                .roots
                .iter()
                .filter(|r| Vec2::new(r.position.x, r.position.z).distance(self.focus) <= radius)
                .count()
        };
        let counts: Vec<_> = self.layers.iter().map(|l| l.roots.len()).collect();
        let visible: Vec<_> = self
            .layers
            .iter()
            .enumerate()
            .map(|(kind, l)| {
                l.roots
                    .iter()
                    .filter(|r| {
                        visibility(
                            kind,
                            r.radius,
                            Vec2::new(r.position.x, r.position.z).distance(self.focus),
                        ) > 0.01
                    })
                    .count()
            })
            .collect();
        serde_json::json!({"visible_window_stalks":(0..LAYERS).map(|i|visible[i]*blades[i]).sum::<usize>(),
            "visible_window_roots":counts.iter().sum::<usize>(),"persistent_contact_cells":self.tracks.cell_count(),
            "max_stalks":(0..LAYERS).map(|i|capacities[i]*blades[i]).sum::<usize>(),"max_roots":capacities.iter().sum::<usize>(),
            "within_2_5_m":within(2.5)*blades[0],"within_5_m":within(5.0)*blades[0],
            "layer_root_counts":counts,"layer_visible_counts":visible,"layer_capacities":capacities,
            "layer_draw_distances_m":OUTER,"triangles_per_root":triangles,"blades_per_root":blades,
            "coarse_canopy_spacing_m":[1.0,4.0],"continuous_canopy":!winter,
            "maximum_triangles":(0..LAYERS).map(|i|capacities[i]*triangles[i]).sum::<usize>(),
            "visible_triangles":(0..LAYERS).map(|i|visible[i]*triangles[i]).sum::<usize>(),
            "ground_class_loaded":self.mask.is_some(),"ground_class_requested":self.mask_requested,
            "winter_stems":self.winter,"winter_fallback":self.mask.is_none(),"primary_raster_only":true,"secondary_visibility_and_self_shadow":false})
    }
    pub fn trample(&mut self, position: Vec3, heading: f32) {
        self.tracks.stamp(position, heading);
    }
    pub fn vegetation_at(&self, x: f32, z: f32) -> Option<bool> {
        self.mask.as_ref().map(|mask| {
            let (pasture, woods, crop) = mask.sample(x, z);
            pasture + woods + crop > 0.35
        })
    }
    fn rebuild(&mut self, cam: Vec2, ground: &impl Fn(f32, f32) -> f32) {
        self.focus = cam;
        let mut changed = false;
        for kind in 0..LAYERS {
            let cell = CELLS[kind];
            let center = ((cam.x / cell).floor() as i32, (cam.y / cell).floor() as i32);
            if self.layers[kind].center == Some(center) {
                continue;
            }
            changed = true;
            let layer = &mut self.layers[kind];
            layer.center = Some(center);
            layer.roots.clear();
            layer.candidates_visited = 0;
            let focus = Vec2::new(
                (center.0 as f32 + 0.5) * cell,
                (center.1 as f32 + 0.5) * cell,
            );
            // Diagonal half-cell guard: all roots becoming loaded/retired are fully below ground.
            let guard = cell * std::f32::consts::FRAC_1_SQRT_2 + 0.05;
            let reach = ((OUTER[kind] + guard) / cell).ceil() as i32;
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let (cx, cz) = (center.0 + dx, center.1 + dz);
                    let box_center = Vec2::new((cx as f32 + 0.5) * cell, (cz as f32 + 0.5) * cell);
                    let nearest = ((box_center - focus).abs() - Vec2::splat(cell * 0.5))
                        .max(Vec2::ZERO)
                        .length();
                    if nearest > OUTER[kind] + guard {
                        continue;
                    }
                    let seed = self.seed
                        ^ (cx as u32).wrapping_mul(73_856_093)
                        ^ (cz as u32).wrapping_mul(19_349_663)
                        ^ (kind as u32).wrapping_mul(0x5B43_9017);
                    for k in 0..CANDIDATES[kind] {
                        if nearest > layer.radii[k as usize] + guard {
                            break;
                        }
                        layer.candidates_visited += 1;
                        let key = seed.wrapping_add(k.wrapping_mul(31));
                        let point = candidate_position(kind, cx, cz, k, key);
                        let (x, z) = (point.x, point.y);
                        if Vec2::new(x, z).distance(focus) > layer.radii[k as usize] + guard {
                            continue;
                        }
                        let (_, woods, _) = self
                            .mask
                            .as_ref()
                            .map(|m| m.sample(x, z))
                            .unwrap_or((0.0, 0.0, 0.0));
                        let winter = self.winter;
                        let patch = if winter {
                            winter_patch(x, z, self.seed)
                        } else {
                            (x * 0.17 + z * 0.043).sin() * (z * 0.13 - x * 0.037).cos() * 0.5 + 0.5
                        };
                        let density = self
                            .mask
                            .as_ref()
                            .map(|m| {
                                let footprint = if winter { filament::WINTER_FOOTPRINT[kind]*1.1 } else { FOOTPRINT[kind] };
                                let cover = m.footprint(x, z, footprint);
                                if winter {
                                    // An authored winter map already places the surviving stands
                                    // and snow clearings. A second independent zero-valued mask
                                    // erased whole stands; use noise only for density variation.
                                    winter_density(kind,0.80+0.20*patch)*cover
                                } else if kind == 0 {
                                    cover * (0.82 + 0.16 * patch)
                                } else if cover > 0.0 {
                                    // Every eligible coarse cell needs a canopy; density varies in
                                    // blade height/colour rather than exposing random bare holes.
                                    1.0
                                } else {
                                    0.0
                                }
                            })
                            .unwrap_or_else(|| winter_density(kind, patch));
                        if hash01(key.wrapping_add(3)) >= density {
                            continue;
                        }
                        if layer.roots.len() >= layer.capacity {
                            continue;
                        }
                        let height = if !winter {
                            0.28 + hash01(key.wrapping_add(4)) * 0.34
                        } else {
                            0.06 + hash01(key.wrapping_add(4)) * 0.10 + patch * 0.025
                        };
                        let width = if winter {
                            0.90 + hash01(key.wrapping_add(5)) * 0.20
                        } else {
                            (if kind == 0 { 0.85 } else { 0.96 })
                                + hash01(key.wrapping_add(5)) * if kind == 0 { 0.25 } else { 0.08 }
                        };
                        let position = Vec3::new(x, ground(x, z) - if winter { 0.025 } else { 0.006 }, z);
                        let slope = {
                            let d = if winter {
                                filament::WINTER_FOOTPRINT[kind] * width
                            } else {
                                FOOTPRINT[kind]
                            };
                            let normal = Vec3::new(
                                ground(x - d, z) - ground(x + d, z),
                                2.0 * d,
                                ground(x, z - d) - ground(x, z + d),
                            )
                            .normalize();
                            Quat::from_rotation_arc(Vec3::Y, normal)
                        };
                        let model = Mat4::from_scale_rotation_translation(
                            Vec3::new(width, height / filament::HEIGHT, width),
                            slope
                                * Quat::from_rotation_y(if winter || kind == 0 {
                                    hash01(key.wrapping_add(6)) * std::f32::consts::TAU
                                } else {
                                    // Quarter turns preserve continuous square patch coverage.
                                    (hash01(key.wrapping_add(6)) * 4.0).floor()
                                        * std::f32::consts::FRAC_PI_2
                                }),
                            position,
                        );
                        let contact = self.tracks.sample(x, z);
                        layer.roots.push(Root {
                            model,
                            position,
                            radius: layer.radii[k as usize],
                            wind_cell: ((x / 8.0).floor() as i32, (z / 8.0).floor() as i32),
                            shelter: (1.0 - woods * 0.82).clamp(0.15, 1.0),
                            compression: contact.compression,
                            target_compression: contact.compression,
                            direction: contact.direction,
                        });
                    }
                }
            }
        }
        if changed {
            let live: HashSet<_> = self
                .layers
                .iter()
                .flat_map(|l| l.roots.iter().map(|r| r.wind_cell))
                .collect();
            self.wind.retain(|key, _| live.contains(key));
            for key in live {
                self.wind.entry(key).or_default();
            }
        }
    }
    pub fn update(
        &mut self,
        r: &mut Renderer,
        cam: Vec3,
        dt: f32,
        ground: impl Fn(f32, f32) -> f32,
        wind_at: impl Fn(Vec3) -> Vec2,
    ) {
        self.advance(cam,dt,r.time,ground,wind_at);
        self.present(r);
    }

    /// Integrate every substep, even when several updates share one presented frame.
    pub fn advance(&mut self,cam:Vec3,dt:f32,time:f32,
        ground:impl Fn(f32,f32)->f32,wind_at:impl Fn(Vec3)->Vec2) {
        self.rebuild(Vec2::new(cam.x, cam.z), &ground);
        if self.contact_revision != self.tracks.revision {
            for root in self.layers.iter_mut().flat_map(|l| &mut l.roots) {
                let contact = self.tracks.sample(root.position.x, root.position.z);
                root.target_compression = contact.compression;
                root.direction = contact.direction;
            }
            self.contact_revision = self.tracks.revision;
        }
        for (&(x, z), bend) in &mut self.wind {
            let point = Vec3::new((x as f32 + 0.5) * 8.0, 0.0, (z as f32 + 0.5) * 8.0);
            let wind = wind_at(point);
            let phase = time * 1.4 - point.x * 0.075 - point.z * 0.052;
            let gust = 0.78 + 0.18 * phase.sin() + 0.07 * (phase * 2.17 + 0.8).sin();
            bend.update(
                wind.normalize_or_zero() * (wind.length_squared() * 0.005 * gust).min(0.19),
                dt,
            );
        }
        for (kind,layer) in self.layers.iter_mut().enumerate() {
            for root in &mut layer.roots {
                if visibility(kind,root.radius,
                    Vec2::new(root.position.x,root.position.z).distance(self.focus))<=0.0001 {continue;}
                root.compression += (root.target_compression-root.compression)
                    * (1.0-(-dt.clamp(0.0,0.1)*18.0).exp());
            }
        }
    }

    fn build_mats(&mut self) {
        for (kind, layer) in self.layers.iter_mut().enumerate() {
            layer.mats.clear();
            for root in &layer.roots {
                let visible = visibility(
                    kind,
                    root.radius,
                    Vec2::new(root.position.x, root.position.z).distance(self.focus),
                );
                if visible <= 0.0001 {
                    continue;
                }
                let flat = root.compression;
                let target = Vec3::new(
                    root.direction.x * 0.98 * flat,
                    1.0 - 0.82 * flat,
                    root.direction.y * 0.98 * flat,
                )
                .normalize();
                let hinge = Quat::from_rotation_arc(Vec3::Y, target);
                let mut m = Mat4::from_translation(root.position)
                    * Mat4::from_quat(hinge)
                    * Mat4::from_translation(-root.position)
                    * root.model;
                // Continuous height morphs settle detail into terrain. Coarse geometry forms
                // broad, low silhouettes rather than spreading subpixel needles to the horizon.
                m.y_axis *= visible;
                let displacement = self.wind[&root.wind_cell].displacement
                    * root.shelter
                    * (1.0 - flat * 0.93)
                    * root.model.y_axis.truncate().length()
                    * visible
                    * [1.0, 0.72, 0.38][kind];
                m.x_axis.w = displacement.x;
                m.z_axis.w = displacement.y;
                layer.mats.push(m);
            }
        }
    }

    /// Presentation reads the final state; it never integrates wind or tread compression.
    pub fn present(&mut self,r:&mut Renderer) {
        self.build_mats();
        for layer in &self.layers {
            let uploaded = r.set_dynamic_instanced_mats(layer.batch, &layer.mats);
            debug_assert!(uploaded);
        }
    }
}

#[cfg(test)]
mod population_tests {
    use super::*;
    fn population(mask: Option<Mask>, seed: u32) -> Grass {
        Grass {
            layers: std::array::from_fn(|kind| Layer::new(usize::MAX, kind, mask.is_none())),
            mask_requested: mask.is_some(),
            winter: mask.is_none(),
            mask,
            focus: Vec2::ZERO,
            seed,
            wind: HashMap::new(),
            tracks: TrackMemory::default(),
            contact_revision: 0,
        }
    }
    fn constant_mask(rgb: [u8; 3]) -> Mask {
        Mask {
            w: 1,
            h: 1,
            rgba: vec![rgb[0], rgb[1], rgb[2], 255],
            half_extent: 384.0,
        }
    }
    #[test]
    fn deferred_grass_presentation_preserves_all_substeps_and_final_matrices() {
        let mut every=population(Some(constant_mask([255,0,0])),42);
        let mut deferred=population(Some(constant_mask([255,0,0])),42);
        for step in 0..5 {
            let camera=Vec3::new(0.4+step as f32*0.015,1.7,0.4);
            let track=Vec3::new(0.6,0.0,step as f32*0.02);
            every.trample(track,0.2);deferred.trample(track,0.2);
            let time=(step+1) as f32*0.05;
            let wind=|_:Vec3|Vec2::new(3.0+step as f32*0.4,-2.0);
            every.advance(camera,0.05,time,|_,_|0.0,wind);
            deferred.advance(camera,0.05,time,|_,_|0.0,wind);
            every.build_mats();
        }
        deferred.build_mats();
        let mut compressed=0;
        for (a,b) in every.layers.iter().zip(&deferred.layers) {
            assert_eq!(a.mats,b.mats,"presentation cadence must not change final grass geometry");
            assert_eq!(a.roots.len(),b.roots.len());
            for (a,b) in a.roots.iter().zip(&b.roots) {
                assert_eq!(a.compression,b.compression);
                assert_eq!(a.target_compression,b.target_compression);
                compressed+=usize::from(a.compression>0.0);
            }
        }
        assert!(compressed>0,"fixture must exercise persistent tread integration");
        for (key,a) in &every.wind {
            let b=&deferred.wind[key];
            assert_eq!(a.displacement,b.displacement);
            let mut a=*a;let mut b=*b;
            for _ in 0..3 {
                a.update(Vec2::new(-0.03,0.05),0.05);
                b.update(Vec2::new(-0.03,0.05),0.05);
                assert_eq!(a.displacement,b.displacement,"retained spring velocity must also agree");
            }
        }
        let before:Vec<_>=deferred.layers.iter().map(|layer|layer.mats.clone()).collect();
        deferred.build_mats();
        for (before,layer) in before.iter().zip(&deferred.layers) {
            assert_eq!(before,&layer.mats,"presentation alone must not advance the simulation");
        }
    }
    #[test]
    fn authored_winter_stands_survive_fallback_noise_clearings() {
        for seed in [0, 42] {
            let center = (-15..15).flat_map(|z|(-15..15).map(move |x|
                Vec2::new(x as f32*4.0,z as f32*4.0)))
                .find(|p|winter_patch(p.x,p.y,seed)==0.0).unwrap();
            let mut mask=Mask {w:201,h:201,rgba:vec![0;201*201*4],half_extent:100.0};
            for z in 0..201 {
                for x in 0..201 {
                    let p=Vec2::new(x as f32-100.0,z as f32-100.0);
                    mask.rgba[(z*201+x)*4]=if p.distance(center)<14.0 {255} else {0};
                    mask.rgba[(z*201+x)*4+3]=255;
                }
            }
            let mut grass=population(Some(mask),seed);
            grass.winter=true;
            grass.layers=std::array::from_fn(|kind|Layer::new(usize::MAX,kind,true));
            grass.rebuild(center,&|_,_|0.0);
            let close=grass.layers[0].roots.iter().filter(|r|
                Vec2::new(r.position.x,r.position.z).distance(center)<2.5).count();
            assert!(close>20,"an authored surviving stand was erased: {close} roots");
            assert!(grass.layers.iter().flat_map(|l|&l.roots).all(|r|
                Vec2::new(r.position.x,r.position.z).distance(center)<14.0),
                "the authored snow outside the stand must remain empty");
        }
    }
    #[test]
    fn winter_stems_respect_a_road_mask_across_every_detail_layer() {
        let mut mask = Mask { w: 769, h: 769, rgba: vec![0;769*769*4], half_extent:384.0 };
        for z in 0..769 {
            for x in 0..769 {
                let i=(z*769+x)*4;
                mask.rgba[i]=if (x as i32-384).abs()<=5 {0} else {255};
                mask.rgba[i+3]=255;
            }
        }
        let mut grass=population(Some(mask),42);
        grass.winter=true;
        grass.layers=std::array::from_fn(|kind| Layer::new(usize::MAX,kind,true));
        grass.rebuild(Vec2::ZERO,&|_,_|0.0);
        for (kind,layer) in grass.layers.iter().enumerate() {
            assert!(!layer.roots.is_empty(),"winter verge is still populated at layer {kind}");
            let mesh=filament::tuft_mesh(kind,true);
            for root in &layer.roots {
                // Check the actual stem mesh footprint, not only its anchor.
                for vertex in &mesh.vertices {
                    let p=root.model.transform_point3(Vec3::from(vertex.pos));
                    assert!(p.x.abs()>4.,"winter stem entered the road at {p:?}, layer {kind}");
                }
            }
        }
        assert_eq!(grass.stats()["winter_stems"],true);
        assert_eq!(grass.stats()["ground_class_loaded"],true);
    }
    #[test]
    fn full_pasture_keeps_close_blades_and_reserved_400_metre_canopy() {
        for seed in [0, 42, 20260905] {
            let mut grass = population(Some(constant_mask([255, 0, 0])), seed);
            grass.rebuild(Vec2::ONE, &|x, z| 0.03 * x - 0.02 * z);
            for (kind, layer) in grass.layers.iter().enumerate() {
                assert!(
                    layer.roots.len() < CAPACITIES[kind],
                    "layer {kind} hit ceiling: {}",
                    layer.roots.len()
                );
                assert!(
                    layer.candidates_visited < 65_000,
                    "bounded candidate work: layer {kind} {}",
                    layer.candidates_visited
                );
                assert!(layer.roots.iter().all(|r| (r.position.y
                    - (0.03 * r.position.x - 0.02 * r.position.z - 0.006))
                    .abs()
                    < 1e-5));
            }
            let near = grass.layers[0]
                .roots
                .iter()
                .filter(|r| Vec2::new(r.position.x, r.position.z).distance(grass.focus) < 2.5)
                .count();
            assert!(
                near * BLADES[0] > 10_000,
                "dense close blades: {}",
                near * BLADES[0]
            );
            let visible_in = |kind: usize, lo: f32, hi: f32| {
                grass.layers[kind]
                    .roots
                    .iter()
                    .filter(|r| {
                        let d = Vec2::new(r.position.x, r.position.z).distance(grass.focus);
                        d > lo && d < hi && visibility(kind, r.radius, d) > 0.1
                    })
                    .count()
            };
            assert!(
                visible_in(1, 26.0, 60.0) > 8500,
                "middle distance cannot be spent on close roots"
            );
            assert!(
                visible_in(2, 180.0, 340.0) > 15_000,
                "canopy beyond the old 180m boundary: {}",
                visible_in(2, 180.0, 340.0)
            );
            assert!(grass.stats()["maximum_triangles"].as_u64().unwrap() < 1_000_000);
            eprintln!("grass density fixture seed {seed}: {}", grass.stats());
        }
    }
    #[test]
    fn actual_canopy_geometry_covers_open_pasture_across_all_lod_transitions() {
        let mut mask = constant_mask([255, 0, 0]);
        mask.half_extent = 4096.0;
        let mut grass = population(Some(mask), 42);
        grass.rebuild(Vec2::new(0.37, -0.62), &|_, _| 0.0);
        let meshes: Vec<_> = (0..LAYERS)
            .map(|kind| filament::tuft_mesh(kind, false))
            .collect();
        let bins: Vec<HashMap<(i32, i32), Vec<&Root>>> = grass
            .layers
            .iter()
            .enumerate()
            .map(|(kind, layer)| {
                let mut bins: HashMap<(i32, i32), Vec<&Root>> = HashMap::new();
                for root in &layer.roots {
                    bins.entry((
                        (root.position.x / CELLS[kind]).floor() as i32,
                        (root.position.z / CELLS[kind]).floor() as i32,
                    ))
                    .or_default()
                    .push(root);
                }
                bins
            })
            .collect();
        let above_ground = |p: Vec2, a: Vec3, b: Vec3, c: Vec3| {
            let av = Vec2::new(a.x, a.z);
            let ab = Vec2::new(b.x, b.z) - av;
            let ac = Vec2::new(c.x, c.z) - av;
            let ap = p - av;
            let denominator = ab.perp_dot(ac);
            if denominator.abs() < 1e-8 {
                return false;
            }
            let u = ap.perp_dot(ac) / denominator;
            let v = ab.perp_dot(ap) / denominator;
            u >= -1e-5
                && v >= -1e-5
                && u + v <= 1.00001
                && a.y + u * (b.y - a.y) + v * (c.y - a.y) > 0.0
        };
        let covered = |point: Vec2| {
            for kind in 1..LAYERS {
                let cell = (
                    (point.x / CELLS[kind]).floor() as i32,
                    (point.y / CELLS[kind]).floor() as i32,
                );
                for z in -1..=1 {
                    for x in -1..=1 {
                        if let Some(roots) = bins[kind].get(&(cell.0 + x, cell.1 + z)) {
                            for root in roots {
                                let visible = visibility(
                                    kind,
                                    root.radius,
                                    Vec2::new(root.position.x, root.position.z)
                                        .distance(grass.focus),
                                );
                                if visible < 0.001 {
                                    continue;
                                }
                                let mut model = root.model;
                                model.y_axis *= visible;
                                // Only the four canopy top faces count. Counting a bounding circle
                                // or the decorative blades would miss real holes between patches.
                                for corner in 0..4 {
                                    let world: [Vec3; 3] = std::array::from_fn(|i| {
                                        let index = meshes[kind].indices[corner * 6 + i] as usize;
                                        model.transform_point3(Vec3::from(
                                            meshes[kind].vertices[index].pos,
                                        ))
                                    });
                                    if above_ground(point, world[0], world[1], world[2]) {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            false
        };
        for distance in [1.0f32, 12.0, 30.0, 55.0, 70.0, 110.0, 250.0, 340.0] {
            let mut hits = 0;
            let mut samples = 0;
            for direction in 0..8 {
                let angle = direction as f32 * std::f32::consts::TAU / 8.0 + 0.13;
                let center = grass.focus + Vec2::new(angle.cos(), angle.sin()) * distance;
                for z in 0..12 {
                    for x in 0..12 {
                        let point =
                            center + (Vec2::new(x as f32, z as f32) - Vec2::splat(5.5)) * 0.37;
                        samples += 1;
                        hits += usize::from(covered(point));
                    }
                }
            }
            assert!(
                hits as f32 / samples as f32 > 0.995,
                "visible soil holes at {distance}m: {} of {samples}",
                samples - hits
            );
        }
    }
    #[test]
    fn world_roots_repeat_and_cell_changes_do_not_create_visible_births() {
        let mut grass = population(Some(constant_mask([255, 0, 0])), 42);
        grass.rebuild(Vec2::new(7.99, 7.99), &|_, _| 0.0);
        let keys = |g: &Grass, kind: usize| {
            g.layers[kind]
                .roots
                .iter()
                .map(|r| {
                    (
                        (r.position.x.to_bits(), r.position.z.to_bits()),
                        (r.model, r.radius),
                    )
                })
                .collect::<HashMap<_, _>>()
        };
        let old: Vec<_> = (0..LAYERS).map(|i| keys(&grass, i)).collect();
        grass.rebuild(Vec2::new(8.01, 8.01), &|_, _| 0.0);
        for (kind, old) in old.iter().enumerate() {
            let new = keys(&grass, kind);
            for (key, (model, radius)) in &new {
                if let Some((before, _)) = old.get(key) {
                    assert_eq!(before, model);
                } else {
                    let p = Vec2::new(f32::from_bits(key.0), f32::from_bits(key.1));
                    assert_eq!(
                        visibility(kind, *radius, p.distance(grass.focus)),
                        0.0,
                        "new layer {kind} root appeared above ground"
                    );
                }
            }
            for (key, (_, radius)) in old {
                if !new.contains_key(key) {
                    let p = Vec2::new(f32::from_bits(key.0), f32::from_bits(key.1));
                    assert_eq!(
                        visibility(kind, *radius, p.distance(grass.focus)),
                        0.0,
                        "old layer {kind} root disappeared above ground"
                    );
                }
            }
        }
        grass.rebuild(Vec2::new(7.99, 7.99), &|_, _| 0.0);
        for (kind, before) in old.iter().enumerate() {
            assert_eq!(*before, keys(&grass, kind));
        }
    }
    #[test]
    fn bare_roads_and_world_edges_keep_their_authored_distribution() {
        let mut bare = population(Some(constant_mask([0, 0, 0])), 42);
        bare.rebuild(Vec2::ZERO, &|_, _| 0.0);
        assert!(bare.layers.iter().all(|l| l.roots.is_empty()));
        let mask = Mask {
            w: 3,
            h: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 0, 255, 255, 0, 0, 255],
            half_extent: 3.0,
        };
        assert_eq!(
            mask.footprint(-0.1, 0.0, 0.9),
            0.0,
            "clumps must not bridge a bare road"
        );
        assert_eq!(mask.sample(4.0, 0.0), (0.0, 0.0, 0.0));
        let mut pixels = vec![255, 0, 0, 255].repeat(9);
        pixels[5 * 4] = 0;
        let narrow = Mask {
            w: 9,
            h: 1,
            rgba: pixels,
            half_extent: 2.0,
        };
        assert!(narrow.sample(0.0, 0.0).0 > 0.99 && narrow.sample(1.1, 0.0).0 > 0.99);
        assert_eq!(
            narrow.footprint(0.0, 0.0, 1.1),
            0.0,
            "an interior road texel must exclude the whole clump"
        );
    }
    #[test]
    fn winter_stands_form_broad_bounded_patches_with_real_snow_clearings() {
        for seed in [0, 42, 20260905] {
            for focus in [Vec2::ZERO, Vec2::new(1850.0, 250.0), Vec2::new(1912.0, 244.0)] {
                let mut winter = population(None, seed);
                winter.rebuild(focus, &|_, _| 0.0);
                let counts: Vec<_> = winter.layers.iter().map(|l| l.roots.len()).collect();
                assert!(
                    counts[0] < WINTER_CAPACITIES[0] && counts[1] > 100 && counts[2] > 100,
                    "winter fills broad stands while preserving snow gaps: {counts:?}"
                );
                for layer in &winter.layers {
                    assert!(
                        layer.roots.len() < layer.capacity,
                        "winter must not truncate cells"
                    );
                    assert!(layer.candidates_visited < 65_000);
                    let mut shortest = f32::INFINITY;
                    let mut tallest = 0.0_f32;
                    for root in &layer.roots {
                        let height = root.model.y_axis.truncate().length() * filament::HEIGHT;
                        shortest = shortest.min(height);
                        tallest = tallest.max(height);
                    }
                    assert!(tallest <= 0.461, "winter straw must stay below low viewing positions");
                    if layer.roots.len() > 10 {
                        assert!(tallest > shortest * 2.0, "distinct short and tall tufts");
                    }
                    assert!(layer.roots.iter().all(|root|
                        winter_patch(root.position.x, root.position.z, seed) > 0.0),
                        "all distance layers must respect the same completely bare snow gaps");
                }
                let stats = winter.stats();
                assert_eq!(stats["continuous_canopy"], false);
                assert!(stats["maximum_triangles"].as_u64().unwrap() < 2_000_000);
                assert!((0..LAYERS).map(|i| WINTER_CAPACITIES[i] * filament::WINTER_TRIANGLES[i]).sum::<usize>()
                    < 2_000_000);
                assert!(stats["max_roots"].as_u64().unwrap() < CAPACITY as u64);
                eprintln!("winter grass seed {seed} at {focus}: {stats}");
            }
        }
        // Dense stands and open snow are deliberate spatial patches, not uniform clutter.
        let values: Vec<_> = (-25..25)
            .flat_map(|z| (-25..25).map(move |x| winter_patch(x as f32 * 2.0, z as f32 * 2.0, 42)))
            .collect();
        assert!(values.iter().filter(|&&v| v == 0.0).count() > values.len() / 12);
        assert!(values.iter().filter(|&&v| v > 0.9).count() > 10);
        for layer in 0..LAYERS {
            assert_eq!(winter_density(layer, 0.0), 0.0,
                "a background density floor would put clutter back into every snow gap");
        }
    }
    #[test]
    fn winter_ground_height_changes_do_not_thin_or_reroll_the_stands() {
        let focus = Vec2::new(1850.0, 250.0);
        let mut flat = population(None, 20260905);
        let mut snow = population(None, 20260905);
        flat.rebuild(focus, &|_, _| 0.0);
        let terrain = |x: f32, z: f32| 2.0 + 0.03 * (x - focus.x) - 0.02 * (z - focus.y);
        snow.rebuild(focus, &terrain);
        for kind in 0..LAYERS {
            assert_eq!(flat.layers[kind].roots.len(), snow.layers[kind].roots.len());
            for (a, b) in flat.layers[kind].roots.iter().zip(&snow.layers[kind].roots) {
                assert_eq!((a.position.x, a.position.z), (b.position.x, b.position.z));
                // Winter stems are rooted 25mm into snow; the 6mm green-map placement would
                // expose their bases. Relief must translate the same roots without rerolling.
                assert!((b.position.y - terrain(b.position.x, b.position.z) + 0.025).abs() < 1e-5);
                assert!((a.model.y_axis.truncate().length() - b.model.y_axis.truncate().length()).abs() < 1e-5);
                assert_eq!(a.radius, b.radius);
            }
        }
        // Terrain relief does not alter the patch mask or refill its pure snow gaps.
        assert_eq!(winter_density(0, 0.0), 0.0);
        assert!(winter_density(0, 1.0) > 0.0);
    }
    #[test]
    fn winter_tuft_positions_and_shapes_stay_fixed_when_crossing_cells() {
        let mut winter = population(None, 42);
        let keys = |g: &Grass, kind: usize| {
            g.layers[kind]
                .roots
                .iter()
                .map(|r| {
                    (
                        (r.position.x.to_bits(), r.position.z.to_bits()),
                        (r.model, r.radius),
                    )
                })
                .collect::<HashMap<_, _>>()
        };
        winter.rebuild(Vec2::new(1911.99, 247.99), &|_, _| 0.0);
        let old: Vec<_> = (0..LAYERS).map(|kind| keys(&winter, kind)).collect();
        winter.rebuild(Vec2::new(1912.01, 248.01), &|_, _| 0.0);
        for (kind, old) in old.iter().enumerate() {
            let new = keys(&winter, kind);
            for (key, (model, radius)) in &new {
                if let Some((before, _)) = old.get(key) {
                    assert_eq!(before, model, "existing tuft moved or changed shape");
                } else {
                    let position = Vec2::new(f32::from_bits(key.0), f32::from_bits(key.1));
                    assert_eq!(
                        visibility(kind, *radius, position.distance(winter.focus)),
                        0.0
                    );
                }
            }
            for (key, (_, radius)) in old {
                if !new.contains_key(key) {
                    let position = Vec2::new(f32::from_bits(key.0), f32::from_bits(key.1));
                    assert_eq!(
                        visibility(kind, *radius, position.distance(winter.focus)),
                        0.0
                    );
                }
            }
        }
        winter.rebuild(Vec2::new(1911.99, 247.99), &|_, _| 0.0);
        for (kind, before) in old.iter().enumerate() {
            assert_eq!(*before, keys(&winter, kind));
        }
    }
    #[test]
    fn active_playtest_mask_keeps_its_768_metre_world_registration() {
        let resolved = crate::resource::resolve(
            "assets/worldgen/derived/aa4d7e350a597265/playtest_ground_class.png",
        )
        .expect("versioned Playtest class mask");
        let rgba = image::open(resolved).unwrap().into_rgba8();
        let (w, h) = rgba.dimensions();
        let mask = Mask {
            w: w as usize,
            h: h as usize,
            rgba: rgba.into_raw(),
            half_extent: 384.0,
        };
        assert!(mask.sample(20.0, 20.0).0 > 0.98);
        assert_eq!(mask.sample(-67.0, 94.0), (0.0, 0.0, 0.0));
        let mut grass = population(Some(mask), 20260905);
        grass.rebuild(Vec2::splat(20.0), &|_, _| 0.0);
        assert!(grass.layers[0].roots.len() > 8000);
        assert!(grass.layers[2].roots.len() > 1000);
    }
}
