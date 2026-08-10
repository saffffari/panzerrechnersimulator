//! grass.rs — runtime ground cover (kessel-map-design: engine-side grass, not placements).
//!
//! Camera-chunked instanced grass blades: deterministic counter-hash scatter, rebuilt only when
//! the camera crosses a chunk boundary. Density + wind come from the act's ground-class mask
//! (assets/terrain/…/ground_class.png, exported by tools/worldgen/gen_world.py: R = pasture,
//! G = woods, B = dark crop): pastures grow dense grass that RIPPLES in the wind, forest floor
//! grows sparse still tufts (wind needs open ground), bare plough gets stubble. The wind factor
//! rides each instance matrix's [3][3] slot into mesh.wgsl's `vs_inst` (params.w == 4 batch),
//! which sways the blade tops by `cam.time`. Winter acts without a mask get sparse dead straw.

use glam::{Mat4, Quat, Vec3};

use crate::gltf_load::{CpuImage, CpuMesh, Vertex};
use crate::renderer::Renderer;

const CHUNK: f32 = 24.0; // metres per scatter cell
const RADIUS: i32 = 4; // chunks kept live around the camera (9×9 window ≈ ±108 m)

fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 65536.0
}

struct Mask {
    w: usize,
    h: usize,
    rgba: Vec<u8>,
    half_extent: f32,
}

impl Mask {
    /// (pasture, woods, darkcrop) at world (x, z), each 0..1.
    fn sample(&self, x: f32, z: f32) -> (f32, f32, f32) {
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
}

pub struct Grass {
    batch: usize,
    mask: Option<Mask>,
    center: Option<(i32, i32)>,
    seed: u32,
    device: wgpu::Device, // ref-counted handle — set_instanced_mats needs it every rebuild
}

/// A crossed-quad grass blade clump: two perpendicular quads, 0.8 m wide × 0.55 m tall, with a
/// procedural blade-strip texture (alpha-tested in the shader). Origin at the ground.
fn blade_mesh() -> CpuMesh {
    let (w, h) = (0.4f32, 0.55f32);
    let mut vertices = Vec::with_capacity(8);
    let mut indices: Vec<u32> = Vec::with_capacity(12);
    for (dir, t) in [(Vec3::X, Vec3::Z), (Vec3::Z, Vec3::X)] {
        let base = vertices.len() as u32;
        let n = t; // face normal (lighting treats blades as thin cards; either side fine)
        for (k, (sx, y, u, v)) in [
            (-1.0f32, h, 0.0f32, 0.0f32),
            (1.0, h, 1.0, 0.0),
            (1.0, 0.0, 1.0, 1.0),
            (-1.0, 0.0, 0.0, 1.0),
        ]
        .iter()
        .enumerate()
        {
            let _ = k;
            let p = dir * (*sx * w) + Vec3::Y * *y;
            vertices.push(Vertex {
                pos: p.to_array(),
                normal: n.to_array(),
                uv: [*u, *v],
                tangent: [dir.x, dir.y, dir.z, 1.0],
            });
        }
        indices.extend_from_slice(&[
            base,
            base + 2,
            base + 1,
            base,
            base + 3,
            base + 2,
            base,
            base + 1,
            base + 2,
            base,
            base + 2,
            base + 3,
        ]);
    }
    // procedural blade-strip texture: vertical strips with tapered alpha, olive→straw gradient
    let (tw, th) = (32usize, 32usize);
    let mut rgba = vec![0u8; tw * th * 4];
    for x in 0..tw {
        let bh = hash01(x as u32 * 97 + 13);
        if bh < 0.35 {
            continue; // gap between blades
        }
        let blade_top = (bh * 0.55 * th as f32) as usize; // taller blades start higher
        let hue = hash01(x as u32 * 31 + 7);
        for y in blade_top..th {
            let t = (y - blade_top) as f32 / (th - blade_top).max(1) as f32; // 0 tip → 1 root
            let (r, g, b) = (
                (70.0 + hue * 60.0 + (1.0 - t) * 40.0) as u8,
                (95.0 + hue * 40.0 + (1.0 - t) * 45.0) as u8,
                (38.0 + hue * 22.0) as u8,
            );
            let i = (y * tw + x) * 4;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    }
    let tex = CpuImage {
        width: tw as u32,
        height: th as u32,
        rgba,
    };
    CpuMesh::basic(
        vertices,
        indices,
        Some(tex),
        Vec3::new(-w, 0.0, -w),
        Vec3::new(w, h, w),
    )
}

impl Grass {
    /// Register the grass batch on the renderer. `mask_path` = the act's ground_class.png (None →
    /// uniform sparse winter straw); `half_extent` maps mask pixels to act-local world metres.
    pub fn new(
        r: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mask_path: Option<&str>,
        half_extent: f32,
        seed: u32,
    ) -> Self {
        let mask = mask_path.and_then(|p| match image::open(p) {
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
            Err(_) => {
                println!("[grass] no ground-class mask at {p} — uniform sparse cover");
                None
            }
        });
        let batch = {
            let (n, _) = r.instanced_stats();
            r.add_instanced(
                device,
                queue,
                &blade_mesh(),
                &[Mat4::from_scale(Vec3::ZERO)],
            );
            r.set_instanced_mats(device, n, &[]);
            r.set_batch_flags(n, 4.0); // grass: wind sway + alpha-test, no shadow
            n
        };
        Self {
            batch,
            mask,
            center: None,
            seed,
            device: device.clone(),
        }
    }

    /// Keep the scatter window centred on the camera. Rebuilds only on chunk crossings.
    pub fn update(&mut self, r: &mut Renderer, cam: Vec3, ground: impl Fn(f32, f32) -> f32) {
        let cc = (
            (cam.x / CHUNK).floor() as i32,
            (cam.z / CHUNK).floor() as i32,
        );
        if self.center == Some(cc) {
            return;
        }
        self.center = Some(cc);
        let mut mats = Vec::new();
        for cj in (cc.1 - RADIUS)..=(cc.1 + RADIUS) {
            for ci in (cc.0 - RADIUS)..=(cc.0 + RADIUS) {
                let cid = (self.seed
                    ^ (ci as u32).wrapping_mul(73_856_093)
                    ^ (cj as u32).wrapping_mul(19_349_663))
                    & 0xFFFF_FFFF;
                let (cx, cz) = (ci as f32 * CHUNK, cj as f32 * CHUNK);
                // density + wind from the ground class at the chunk centre
                let (n_blades, wind) = match &self.mask {
                    Some(m) => {
                        let (pasture, woods, crop) = m.sample(cx + CHUNK * 0.5, cz + CHUNK * 0.5);
                        let n = (pasture * 46.0
                            + crop * 20.0
                            + woods * 6.0
                            + (1.0 - pasture.max(woods).max(crop)) * 10.0)
                            as u32;
                        (n, (1.0 - woods * 0.9).clamp(0.1, 1.0))
                    }
                    None => (9, 1.0), // maskless act (winter): sparse dead straw, full wind
                };
                for k in 0..n_blades {
                    let x = cx + hash01(cid.wrapping_add(k * 3 + 1)) * CHUNK;
                    let z = cz + hash01(cid.wrapping_add(k * 3 + 2)) * CHUNK;
                    let s = 0.7 + hash01(cid.wrapping_add(k * 3 + 3)) * 0.9;
                    let yaw = hash01(cid.wrapping_add(k * 7 + 5)) * std::f32::consts::TAU;
                    let mut m = Mat4::from_scale_rotation_translation(
                        Vec3::new(s, s * (0.8 + hash01(cid.wrapping_add(k * 11 + 9)) * 0.5), s),
                        Quat::from_rotation_y(yaw),
                        Vec3::new(x, ground(x, z), z),
                    );
                    m.w_axis.w = wind; // smuggled per-blade wind factor (vs_inst reads i3.w)
                    mats.push(m);
                }
            }
        }
        r.set_instanced_mats(&self.device.clone(), self.batch, &mats);
    }
}
