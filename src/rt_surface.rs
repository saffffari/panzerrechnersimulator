//! Source-exact surface data for hardware ray hits.
//!
//! This module deliberately stops at packing and deterministic CPU reference evaluation. It does
//! not bind buffers, modify the renderer, or claim that secondary rays are materially shaded yet.
//! The eventual ray shader can use the same records to resolve
//! `(instance_custom_data, primitive_index, barycentrics)` back to the exact [`CpuMesh`] triangle,
//! vertex attributes, material maps, and instance transform that produced the BLAS hit.
//!
//! Texture maps are copied byte-for-byte at mip zero. The additional mip levels reproduce the
//! renderer's current CPU box filter exactly. Atlas lookup is explicit (repeat within the material
//! rectangle, nearest within a mip, linear between mips), so neighboring atlas rectangles can never
//! bleed into a hit and no average-material or screen-space proxy is involved.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec2, Vec3, Vec4};

use crate::gltf_load::{CpuImage, CpuMesh, Vertex};

/// `wgpu::TlasInstance::custom_data` is a 24-bit payload.
pub const TLAS_CUSTOM_DATA_MAX: u32 = 0x00ff_ffff;
pub const DEFAULT_ATLAS_PAGE_EXTENT: u32 = 4096;
/// `GpuInstanceMaterialState.ids[3]`: this exact instance is a closed dielectric whose two
/// interfaces may be selected independently through the dedicated TLAS mask.
pub const INSTANCE_FLAG_CLOSED_DIELECTRIC: u32 = 1 << 0;

const BASE_COLOR_FALLBACK: [u8; 4] = [255, 255, 255, 255];
const METALLIC_ROUGHNESS_FALLBACK: [u8; 4] = [255, 255, 255, 255];
const NORMAL_FALLBACK: [u8; 4] = [128, 128, 255, 255];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum TextureSemantic {
    BaseColor = 0,
    MetallicRoughness = 1,
    Normal = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum TexelEncoding {
    /// RGB channels are encoded as sRGB; alpha remains linear UNORM.
    Srgb = 0,
    /// All channels are linear UNORM data.
    Linear = 1,
}

/// Dynamic emissive interpretation shared by raster and secondary-hit shading. The numeric values
/// intentionally match the renderer's existing authored modes; `FixedLinearRadiance` is the clean
/// extension point for emitters whose RGB energy is not derived from their base-colour texel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum InstanceEmissionMode {
    Disabled = 0,
    BaseColorNightGated = 1,
    BaseColorUngated = 2,
    RasterFxOrange = 3,
    FixedLinearRadiance = 4,
}

impl InstanceEmissionMode {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Disabled),
            1 => Some(Self::BaseColorNightGated),
            2 => Some(Self::BaseColorUngated),
            3 => Some(Self::RasterFxOrange),
            4 => Some(Self::FixedLinearRadiance),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub u32);

/// Storage-buffer-safe expansion of [`Vertex`]. Each logical `vec3` occupies a full 16-byte lane,
/// matching WGSL storage-buffer alignment without losing any source float.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuSurfaceVertex {
    pub position: [f32; 4],
    pub normal: [f32; 4],
    pub uv: [f32; 2],
    pub _uv_pad: [f32; 2],
    pub tangent: [f32; 4],
}

impl From<Vertex> for GpuSurfaceVertex {
    fn from(vertex: Vertex) -> Self {
        Self {
            position: [vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0],
            normal: [vertex.normal[0], vertex.normal[1], vertex.normal[2], 0.0],
            uv: vertex.uv,
            _uv_pad: [0.0; 2],
            tangent: vertex.tangent,
        }
    }
}

/// One BLAS primitive. Indices address the global packed vertex array; `surface_index` is retained
/// as an invariant check and an inexpensive shader-side material lookup key.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct GpuTriangleRecord {
    pub indices: [u32; 3],
    pub surface_index: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuSurfaceRecord {
    /// vertex offset/count, then triangle offset/count.
    pub geometry: [u32; 4],
    /// material index followed by reserved lanes.
    pub material: [u32; 4],
    pub bounds_min: [f32; 4],
    pub bounds_max: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuMaterialRecord {
    /// base-color, metallic-roughness, normal texture indices, reserved.
    pub textures: [u32; 4],
    /// metallic factor, roughness factor, reserved, reserved.
    pub factors: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct GpuTextureRecord {
    /// first mip-rect, mip count, [`TextureSemantic`], [`TexelEncoding`].
    pub layout: [u32; 4],
    /// source width, source height, source-map-present flag, reserved.
    pub source: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct GpuMipRect {
    /// atlas page, x, y, mip level.
    pub page_origin: [u32; 4],
    /// width, height, reserved, reserved.
    pub extent: [u32; 4],
}

/// Metadata for a separately uploadable atlas page. `texel_offset` addresses an optional flattened
/// `array<u32>` storage buffer; pages also retain their own exact texel vectors for separate binds.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct GpuAtlasPageRecord {
    /// flattened texel offset, width, height, [`TexelEncoding`].
    pub layout: [u32; 4],
}

/// Per-instance data needed after a ray-query hit. `normal_to_world` is the inverse-transpose of
/// the model's linear part. Tangents intentionally use `model` directly, matching `CpuMesh`'s
/// transform contract.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuInstanceRecord {
    /// custom data, surface index, reserved, reserved.
    pub ids: [u32; 4],
    pub model: [[f32; 4]; 4],
    /// Three padded columns of a 3x3 normal-to-world matrix.
    pub normal_to_world: [[f32; 4]; 3],
}

/// Frame-mutable material state aligned one-for-one with [`GpuInstanceRecord`]. Geometry,
/// textures, and material factors remain immutable in [`SurfacePack`]; this separate buffer lets a
/// `set_emissive`-style update reach secondary hits without repacking meshes or silently shading
/// from stale state.
///
/// `ids = [custom_data, surface_index, InstanceEmissionMode, flags]`. Flag bit 0 identifies a
/// closed direct-view dielectric whose entry/exit interfaces are transported by the High path.
/// `emission = [intensity, linear_r, linear_g, linear_b]`. RGB is a multiplier for base-colour
/// modes, the authoritative orange tint for `RasterFxOrange`, and direct radiance for
/// `FixedLinearRadiance`. This retains the renderer's current intensity-in-X convention while
/// making emitted RGB explicit.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuInstanceMaterialState {
    pub ids: [u32; 4],
    pub emission: [f32; 4],
}

impl GpuInstanceMaterialState {
    pub const fn disabled(instance_custom_data: u32, surface: SurfaceId) -> Self {
        Self {
            ids: [
                instance_custom_data,
                surface.0,
                InstanceEmissionMode::Disabled as u32,
                0,
            ],
            emission: [0.0, 0.0, 0.0, 0.0],
        }
    }

    pub const fn emissive(
        instance_custom_data: u32,
        surface: SurfaceId,
        mode: InstanceEmissionMode,
        intensity: f32,
        linear_radiance_or_tint: [f32; 3],
    ) -> Self {
        Self {
            ids: [instance_custom_data, surface.0, mode as u32, 0],
            emission: [
                intensity,
                linear_radiance_or_tint[0],
                linear_radiance_or_tint[1],
                linear_radiance_or_tint[2],
            ],
        }
    }

    pub fn mode(&self) -> Option<InstanceEmissionMode> {
        InstanceEmissionMode::from_u32(self.ids[2])
    }
}

/// Sorted lookup entry for sparse 24-bit TLAS custom data. A future WGSL consumer can binary-search
/// this table without allocating a 16-million-entry dense map.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct GpuInstanceLookup {
    /// custom data, instance-record index, reserved, reserved.
    pub key_value: [u32; 4],
}

#[derive(Clone, Debug)]
pub struct AtlasPage {
    pub gpu: GpuAtlasPageRecord,
    /// Exact RGBA8 bytes packed into a `u32` as `R | G<<8 | B<<16 | A<<24`.
    pub rgba8_texels: Vec<u32>,
}

impl AtlasPage {
    pub fn encoding(&self) -> TexelEncoding {
        encoding_from_u32(self.gpu.layout[3]).expect("packer emitted a valid page encoding")
    }

    pub fn width(&self) -> u32 {
        self.gpu.layout[1]
    }

    pub fn height(&self) -> u32 {
        self.gpu.layout[2]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MaterialLod {
    pub base_color: f32,
    pub metallic_roughness: f32,
    pub normal: f32,
}

impl MaterialLod {
    pub const fn splat(lod: f32) -> Self {
        Self {
            base_color: lod,
            metallic_roughness: lod,
            normal: lod,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampledMaterial {
    /// Linear RGB after exact sRGB decode; alpha is linear UNORM.
    pub base_color_linear: [f32; 4],
    /// Linear UNORM glTF data. G=roughness, B=metallic.
    pub metallic_roughness: [f32; 4],
    /// Linear UNORM tangent-space normal-map sample before `*2-1`.
    pub normal_map: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvaluatedSurfaceHit {
    pub instance_index: u32,
    pub instance_custom_data: u32,
    pub surface: SurfaceId,
    pub material: MaterialId,
    pub primitive_index: u32,
    pub vertex_indices: [u32; 3],
    /// Weights for source vertices 0, 1, and 2. Ray-query barycentrics supply weights 1 and 2.
    pub barycentric_weights: [f32; 3],
    pub local_position: Vec3,
    pub world_position: Vec3,
    /// Interpolated source normal, transformed by the instance inverse-transpose.
    pub world_interpolated_normal: Vec3,
    /// Exact face normal from the transformed source triangle winding.
    pub world_triangle_normal: Vec3,
    /// Re-orthogonalized tangent and its interpolated handedness.
    pub world_tangent: Vec4,
    pub world_bitangent: Vec3,
    pub uv: Vec2,
    pub material_sample: SampledMaterial,
    /// Normal-map result in world space using the exact interpolated TBN frame.
    pub world_shading_normal: Vec3,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceMemoryCost {
    pub vertex_bytes: usize,
    pub triangle_bytes: usize,
    pub surface_bytes: usize,
    pub material_bytes: usize,
    pub texture_record_bytes: usize,
    pub mip_rect_bytes: usize,
    pub instance_bytes: usize,
    pub instance_lookup_bytes: usize,
    pub atlas_texel_bytes: usize,
    pub source_level_zero_bytes: usize,
    pub derived_mip_bytes: usize,
    pub atlas_padding_bytes: usize,
    pub total_gpu_payload_bytes: usize,
}

#[derive(Clone, Debug)]
pub struct SurfacePack {
    vertices: Vec<GpuSurfaceVertex>,
    triangles: Vec<GpuTriangleRecord>,
    surfaces: Vec<GpuSurfaceRecord>,
    materials: Vec<GpuMaterialRecord>,
    textures: Vec<GpuTextureRecord>,
    mip_rects: Vec<GpuMipRect>,
    atlas_pages: Vec<AtlasPage>,
    instances: Vec<GpuInstanceRecord>,
    instance_lookup: Vec<GpuInstanceLookup>,
    source_level_zero_bytes: usize,
    derived_mip_bytes: usize,
}

impl SurfacePack {
    pub fn vertices(&self) -> &[GpuSurfaceVertex] {
        &self.vertices
    }

    pub fn triangles(&self) -> &[GpuTriangleRecord] {
        &self.triangles
    }

    pub fn surfaces(&self) -> &[GpuSurfaceRecord] {
        &self.surfaces
    }

    pub fn materials(&self) -> &[GpuMaterialRecord] {
        &self.materials
    }

    pub fn textures(&self) -> &[GpuTextureRecord] {
        &self.textures
    }

    pub fn mip_rects(&self) -> &[GpuMipRect] {
        &self.mip_rects
    }

    pub fn atlas_pages(&self) -> &[AtlasPage] {
        &self.atlas_pages
    }

    pub fn instances(&self) -> &[GpuInstanceRecord] {
        &self.instances
    }

    pub fn instance_lookup(&self) -> &[GpuInstanceLookup] {
        &self.instance_lookup
    }

    /// Allocate the exact one-record-per-instance dynamic state buffer in pack order. The sparse
    /// custom-data lookup already resolves to this same instance index.
    pub fn default_instance_material_states(&self) -> Vec<GpuInstanceMaterialState> {
        self.instances
            .iter()
            .map(|instance| {
                GpuInstanceMaterialState::disabled(instance.ids[0], SurfaceId(instance.ids[1]))
            })
            .collect()
    }

    /// Reject stale, reordered, non-finite, or nonsensical dynamic material data before upload.
    pub fn validate_instance_material_states(
        &self,
        states: &[GpuInstanceMaterialState],
    ) -> Result<(), DynamicMaterialStateError> {
        if states.len() != self.instances.len() {
            return Err(DynamicMaterialStateError::CountMismatch {
                expected: self.instances.len(),
                actual: states.len(),
            });
        }
        for (index, (state, instance)) in states.iter().zip(&self.instances).enumerate() {
            let expected = [instance.ids[0], instance.ids[1]];
            let actual = [state.ids[0], state.ids[1]];
            if actual != expected {
                return Err(DynamicMaterialStateError::InstanceMismatch {
                    index,
                    expected,
                    actual,
                });
            }
            let Some(mode) = state.mode() else {
                return Err(DynamicMaterialStateError::UnknownEmissionMode {
                    index,
                    mode: state.ids[2],
                });
            };
            if state.emission.iter().any(|value| !value.is_finite()) {
                return Err(DynamicMaterialStateError::NonFiniteEmission(index));
            }
            if state.emission.iter().any(|value| *value < 0.0) {
                return Err(DynamicMaterialStateError::NegativeEmission(index));
            }
            if mode == InstanceEmissionMode::Disabled && state.emission != [0.0; 4] {
                return Err(DynamicMaterialStateError::DisabledEmissionIsNonZero(index));
            }
        }
        Ok(())
    }

    pub fn instance_material_state_bytes(&self) -> usize {
        self.instances.len() * size_of::<GpuInstanceMaterialState>()
    }

    /// Concatenate atlas pages for a single storage-buffer upload. Page records already contain the
    /// correct offsets into this exact ordering.
    pub fn flattened_atlas_texels(&self) -> Vec<u32> {
        let mut texels = Vec::with_capacity(
            self.atlas_pages
                .iter()
                .map(|page| page.rgba8_texels.len())
                .sum(),
        );
        for page in &self.atlas_pages {
            debug_assert_eq!(page.gpu.layout[0] as usize, texels.len());
            texels.extend_from_slice(&page.rgba8_texels);
        }
        texels
    }

    pub fn memory_cost(&self) -> SurfaceMemoryCost {
        let atlas_texel_bytes = self
            .atlas_pages
            .iter()
            .map(|page| page.rgba8_texels.len() * size_of::<u32>())
            .sum::<usize>();
        let occupied = self.source_level_zero_bytes + self.derived_mip_bytes;
        let mut out = SurfaceMemoryCost {
            vertex_bytes: self.vertices.len() * size_of::<GpuSurfaceVertex>(),
            triangle_bytes: self.triangles.len() * size_of::<GpuTriangleRecord>(),
            surface_bytes: self.surfaces.len() * size_of::<GpuSurfaceRecord>(),
            material_bytes: self.materials.len() * size_of::<GpuMaterialRecord>(),
            texture_record_bytes: self.textures.len() * size_of::<GpuTextureRecord>(),
            mip_rect_bytes: self.mip_rects.len() * size_of::<GpuMipRect>(),
            instance_bytes: self.instances.len() * size_of::<GpuInstanceRecord>(),
            instance_lookup_bytes: self.instance_lookup.len() * size_of::<GpuInstanceLookup>(),
            atlas_texel_bytes,
            source_level_zero_bytes: self.source_level_zero_bytes,
            derived_mip_bytes: self.derived_mip_bytes,
            atlas_padding_bytes: atlas_texel_bytes.saturating_sub(occupied),
            total_gpu_payload_bytes: 0,
        };
        out.total_gpu_payload_bytes = out.vertex_bytes
            + out.triangle_bytes
            + out.surface_bytes
            + out.material_bytes
            + out.texture_record_bytes
            + out.mip_rect_bytes
            + out.instance_bytes
            + out.instance_lookup_bytes
            + out.atlas_texel_bytes;
        out
    }

    pub fn texture_semantic(&self, texture: TextureId) -> Result<TextureSemantic, EvalError> {
        let record = self.texture_record(texture)?;
        semantic_from_u32(record.layout[2]).ok_or(EvalError::CorruptPack("texture semantic"))
    }

    pub fn texture_encoding(&self, texture: TextureId) -> Result<TexelEncoding, EvalError> {
        let record = self.texture_record(texture)?;
        encoding_from_u32(record.layout[3]).ok_or(EvalError::CorruptPack("texture encoding"))
    }

    pub fn texture_source_present(&self, texture: TextureId) -> Result<bool, EvalError> {
        Ok(self.texture_record(texture)?.source[2] != 0)
    }

    /// Exact stored RGBA8 texel from one mip. Coordinates address the material-local rectangle, not
    /// the atlas page, and therefore cannot leak into a neighboring material.
    pub fn texture_texel_rgba8(
        &self,
        texture: TextureId,
        mip: u32,
        x: u32,
        y: u32,
    ) -> Result<[u8; 4], EvalError> {
        let rect = self.mip_rect(texture, mip)?;
        let width = rect.extent[0];
        let height = rect.extent[1];
        if x >= width || y >= height {
            return Err(EvalError::TexelOutOfRange {
                texture,
                mip,
                x,
                y,
                width,
                height,
            });
        }
        let page_index = rect.page_origin[0] as usize;
        let page = self
            .atlas_pages
            .get(page_index)
            .ok_or(EvalError::CorruptPack("mip page index"))?;
        let px = rect.page_origin[1] + x;
        let py = rect.page_origin[2] + y;
        let offset = (u64::from(py) * u64::from(page.width()) + u64::from(px)) as usize;
        let packed = *page
            .rgba8_texels
            .get(offset)
            .ok_or(EvalError::CorruptPack("atlas texel offset"))?;
        Ok(unpack_rgba8(packed))
    }

    /// Reconstruct a full mip in source row order. Mip zero must be byte-identical to `CpuImage`.
    pub fn texture_level_rgba8(&self, texture: TextureId, mip: u32) -> Result<Vec<u8>, EvalError> {
        let rect = *self.mip_rect(texture, mip)?;
        let mut rgba = Vec::with_capacity((rect.extent[0] * rect.extent[1] * 4) as usize);
        for y in 0..rect.extent[1] {
            for x in 0..rect.extent[0] {
                rgba.extend_from_slice(&self.texture_texel_rgba8(texture, mip, x, y)?);
            }
        }
        Ok(rgba)
    }

    /// Match the raster material sampler: repeat U/V, nearest min/mag within each mip, and linear
    /// interpolation between mip levels. `lod` is explicit because a secondary-ray integrator must
    /// derive it from a ray cone rather than screen derivatives.
    pub fn sample_texture(
        &self,
        texture: TextureId,
        uv: Vec2,
        lod: f32,
    ) -> Result<[f32; 4], EvalError> {
        if !uv.is_finite() || !lod.is_finite() {
            return Err(EvalError::NonFiniteSampleInput);
        }
        let record = self.texture_record(texture)?;
        let mip_count = record.layout[1];
        if mip_count == 0 {
            return Err(EvalError::CorruptPack("zero mip count"));
        }
        let max_lod = (mip_count - 1) as f32;
        let clamped = lod.max(0.0).min(max_lod);
        let lo = clamped.floor() as u32;
        let hi = (lo + 1).min(mip_count - 1);
        let t = clamped - lo as f32;
        let a = self.sample_nearest_mip(texture, uv, lo)?;
        if hi == lo || t == 0.0 {
            return Ok(a);
        }
        let b = self.sample_nearest_mip(texture, uv, hi)?;
        Ok(lerp4(a, b, t))
    }

    pub fn sample_material(
        &self,
        material: MaterialId,
        uv: Vec2,
        lod: MaterialLod,
    ) -> Result<SampledMaterial, EvalError> {
        let record = self
            .materials
            .get(material.0 as usize)
            .ok_or(EvalError::UnknownMaterial(material))?;
        let base_color_linear =
            self.sample_texture(TextureId(record.textures[0]), uv, lod.base_color)?;
        let metallic_roughness =
            self.sample_texture(TextureId(record.textures[1]), uv, lod.metallic_roughness)?;
        let normal_map = self.sample_texture(TextureId(record.textures[2]), uv, lod.normal)?;
        Ok(SampledMaterial {
            base_color_linear,
            metallic_roughness,
            normal_map,
            metallic: (metallic_roughness[2] * record.factors[0]).clamp(0.0, 1.0),
            roughness: (metallic_roughness[1] * record.factors[1]).clamp(0.045, 1.0),
        })
    }

    /// Resolve an exact ray-query hit. Ray-query barycentrics are the weights for triangle vertices
    /// 1 and 2; vertex 0 receives `1-bx-by`, matching the hardware convention.
    pub fn evaluate_hit(
        &self,
        instance_custom_data: u32,
        primitive_index: u32,
        barycentrics: [f32; 2],
        lod: MaterialLod,
    ) -> Result<EvaluatedSurfaceHit, EvalError> {
        let (instance_index, instance) = self.instance(instance_custom_data)?;
        self.evaluate_hit_from_instance(
            instance_index,
            instance,
            primitive_index,
            barycentrics,
            lod,
        )
    }

    /// Evaluate against a current authoritative instance transform while retaining the exact
    /// source surface/material identity registered in this pack. Portable BVH leaves expose the
    /// required dense `instance_index`; mismatched or stale records are rejected.
    pub fn evaluate_hit_with_instance_record(
        &self,
        instance_index: u32,
        current_instance: &GpuInstanceRecord,
        primitive_index: u32,
        barycentrics: [f32; 2],
        lod: MaterialLod,
    ) -> Result<EvaluatedSurfaceHit, EvalError> {
        let expected = self
            .instances
            .get(instance_index as usize)
            .ok_or(EvalError::InstanceRecordOutOfRange(instance_index))?;
        if current_instance.ids[0] != expected.ids[0] || current_instance.ids[1] != expected.ids[1]
        {
            return Err(EvalError::InstanceRecordMismatch {
                instance_index,
                expected: [expected.ids[0], expected.ids[1]],
                actual: [current_instance.ids[0], current_instance.ids[1]],
            });
        }
        if current_instance
            .model
            .iter()
            .flatten()
            .chain(current_instance.normal_to_world.iter().flatten())
            .any(|value| !value.is_finite())
        {
            return Err(EvalError::NonFiniteInstanceRecord(instance_index));
        }
        self.evaluate_hit_from_instance(
            instance_index,
            current_instance,
            primitive_index,
            barycentrics,
            lod,
        )
    }

    fn evaluate_hit_from_instance(
        &self,
        instance_index: u32,
        instance: &GpuInstanceRecord,
        primitive_index: u32,
        barycentrics: [f32; 2],
        lod: MaterialLod,
    ) -> Result<EvaluatedSurfaceHit, EvalError> {
        if !barycentrics[0].is_finite() || !barycentrics[1].is_finite() {
            return Err(EvalError::InvalidBarycentrics(barycentrics));
        }
        let weights = [
            1.0 - barycentrics[0] - barycentrics[1],
            barycentrics[0],
            barycentrics[1],
        ];
        if weights
            .iter()
            .any(|weight| *weight < -1.0e-5 || *weight > 1.0 + 1.0e-5)
        {
            return Err(EvalError::InvalidBarycentrics(barycentrics));
        }

        let instance_custom_data = instance.ids[0];
        let surface_id = SurfaceId(instance.ids[1]);
        let surface = self
            .surfaces
            .get(surface_id.0 as usize)
            .ok_or(EvalError::CorruptPack("instance surface index"))?;
        if primitive_index >= surface.geometry[3] {
            return Err(EvalError::PrimitiveOutOfRange {
                surface: surface_id,
                primitive: primitive_index,
                primitive_count: surface.geometry[3],
            });
        }
        let triangle_index = surface.geometry[2] + primitive_index;
        let triangle = self
            .triangles
            .get(triangle_index as usize)
            .ok_or(EvalError::CorruptPack("triangle offset"))?;
        if triangle.surface_index != surface_id.0 {
            return Err(EvalError::CorruptPack("triangle surface index"));
        }
        let vertices = [
            self.vertex(triangle.indices[0])?,
            self.vertex(triangle.indices[1])?,
            self.vertex(triangle.indices[2])?,
        ];

        let local_positions = vertices.map(|vertex| Vec3::from_slice(&vertex.position[..3]));
        let local_normals = vertices.map(|vertex| Vec3::from_slice(&vertex.normal[..3]));
        let local_tangents = vertices.map(|vertex| Vec4::from_array(vertex.tangent));
        let uvs = vertices.map(|vertex| Vec2::from_array(vertex.uv));

        let local_position = bary3(local_positions, weights);
        let local_normal = bary3(local_normals, weights).normalize_or_zero();
        let local_tangent4 = bary4(local_tangents, weights);
        let uv = bary2(uvs, weights);

        let model = Mat4::from_cols_array_2d(&instance.model);
        let normal_to_world = mat3_from_padded_cols(instance.normal_to_world);
        let linear_to_world = Mat3::from_mat4(model);
        let world_position = model.transform_point3(local_position);
        let world_interpolated_normal = (normal_to_world * local_normal).normalize_or_zero();
        let mut tangent = (linear_to_world * local_tangent4.truncate()).normalize_or_zero();
        tangent = (tangent - world_interpolated_normal * world_interpolated_normal.dot(tangent))
            .normalize_or_zero();
        let handedness = local_tangent4.w;
        let bitangent = world_interpolated_normal.cross(tangent) * handedness;

        // Rebuild the face normal from transformed vertices, rather than merely transforming the
        // local cross product. This preserves the actual BLAS winding even for mirrored instances.
        let world_triangle_positions =
            local_positions.map(|position| model.transform_point3(position));
        let world_triangle_normal = (world_triangle_positions[1] - world_triangle_positions[0])
            .cross(world_triangle_positions[2] - world_triangle_positions[0])
            .normalize_or_zero();

        let material_id = MaterialId(surface.material[0]);
        let material_sample = self.sample_material(material_id, uv, lod)?;
        let tangent_sample = Vec3::new(
            material_sample.normal_map[0] * 2.0 - 1.0,
            material_sample.normal_map[1] * 2.0 - 1.0,
            material_sample.normal_map[2] * 2.0 - 1.0,
        );
        let world_shading_normal = (tangent * tangent_sample.x
            + bitangent * tangent_sample.y
            + world_interpolated_normal * tangent_sample.z)
            .normalize_or_zero();

        Ok(EvaluatedSurfaceHit {
            instance_index,
            instance_custom_data,
            surface: surface_id,
            material: material_id,
            primitive_index,
            vertex_indices: triangle.indices,
            barycentric_weights: weights,
            local_position,
            world_position,
            world_interpolated_normal,
            world_triangle_normal,
            world_tangent: tangent.extend(handedness),
            world_bitangent: bitangent,
            uv,
            material_sample,
            world_shading_normal,
        })
    }

    fn texture_record(&self, texture: TextureId) -> Result<&GpuTextureRecord, EvalError> {
        self.textures
            .get(texture.0 as usize)
            .ok_or(EvalError::UnknownTexture(texture))
    }

    fn mip_rect(&self, texture: TextureId, mip: u32) -> Result<&GpuMipRect, EvalError> {
        let record = self.texture_record(texture)?;
        if mip >= record.layout[1] {
            return Err(EvalError::MipOutOfRange {
                texture,
                mip,
                mip_count: record.layout[1],
            });
        }
        self.mip_rects
            .get((record.layout[0] + mip) as usize)
            .ok_or(EvalError::CorruptPack("mip rect offset"))
    }

    fn sample_nearest_mip(
        &self,
        texture: TextureId,
        uv: Vec2,
        mip: u32,
    ) -> Result<[f32; 4], EvalError> {
        let rect = self.mip_rect(texture, mip)?;
        let width = rect.extent[0];
        let height = rect.extent[1];
        let repeated = Vec2::new(uv.x.rem_euclid(1.0), uv.y.rem_euclid(1.0));
        let x = ((repeated.x * width as f32).floor() as u32).min(width - 1);
        let y = ((repeated.y * height as f32).floor() as u32).min(height - 1);
        let rgba = self.texture_texel_rgba8(texture, mip, x, y)?;
        let encoding = self.texture_encoding(texture)?;
        Ok(decode_rgba8(rgba, encoding))
    }

    fn instance(&self, custom_data: u32) -> Result<(u32, &GpuInstanceRecord), EvalError> {
        let found = self
            .instance_lookup
            .binary_search_by_key(&custom_data, |entry| entry.key_value[0])
            .map_err(|_| EvalError::UnknownInstance(custom_data))?;
        let index = self.instance_lookup[found].key_value[1];
        let instance = self
            .instances
            .get(index as usize)
            .ok_or(EvalError::CorruptPack("instance lookup index"))?;
        if instance.ids[0] != custom_data {
            return Err(EvalError::CorruptPack("instance custom data"));
        }
        Ok((index, instance))
    }

    fn vertex(&self, index: u32) -> Result<GpuSurfaceVertex, EvalError> {
        self.vertices
            .get(index as usize)
            .copied()
            .ok_or(EvalError::CorruptPack("triangle vertex index"))
    }
}

#[derive(Clone, Debug)]
struct MipPayload {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingTexture {
    semantic: TextureSemantic,
    encoding: TexelEncoding,
    source_present: bool,
    mips: Vec<MipPayload>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TextureKey {
    semantic: TextureSemantic,
    encoding: TexelEncoding,
    width: u32,
    height: u32,
    byte_len: usize,
    hash: u64,
}

/// Fast candidate key for source-exact mesh interning. Hashes only select candidates; every hit is
/// subsequently compared byte-for-byte against the already packed surface, so a hash collision can
/// never alias two different BLAS primitive streams or materials.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct MeshKey {
    vertex_count: usize,
    index_count: usize,
    vertex_hash: u64,
    index_hash: u64,
    images: [SourceImageKey; 3],
    metallic_factor: u32,
    roughness_factor: u32,
    bounds_min: [u32; 3],
    bounds_max: [u32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SourceImageKey {
    present: bool,
    width: u32,
    height: u32,
    byte_len: usize,
    hash: u64,
}

#[derive(Debug)]
pub struct SurfacePackBuilder {
    max_page_extent: u32,
    vertices: Vec<GpuSurfaceVertex>,
    triangles: Vec<GpuTriangleRecord>,
    surfaces: Vec<GpuSurfaceRecord>,
    materials: Vec<GpuMaterialRecord>,
    textures: Vec<PendingTexture>,
    texture_dedup: HashMap<TextureKey, Vec<u32>>,
    mesh_dedup: HashMap<MeshKey, Vec<u32>>,
    instances: Vec<GpuInstanceRecord>,
    custom_data: BTreeMap<u32, u32>,
}

impl Default for SurfacePackBuilder {
    fn default() -> Self {
        Self::new(DEFAULT_ATLAS_PAGE_EXTENT).expect("the default atlas page extent is non-zero")
    }
}

impl SurfacePackBuilder {
    pub fn new(max_page_extent: u32) -> Result<Self, PackError> {
        if max_page_extent == 0 {
            return Err(PackError::ZeroAtlasPageExtent);
        }
        Ok(Self {
            max_page_extent,
            vertices: Vec::new(),
            triangles: Vec::new(),
            surfaces: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            texture_dedup: HashMap::new(),
            mesh_dedup: HashMap::new(),
            instances: Vec::new(),
            custom_data: BTreeMap::new(),
        })
    }

    /// Append one source mesh without transforming, simplifying, or reindexing it. Primitive order
    /// remains the source index-buffer order used by the BLAS. A byte-identical source mesh reuses
    /// its existing surface; instances remain distinct and retain their own TLAS custom data.
    pub fn add_mesh(&mut self, mesh: &CpuMesh) -> Result<SurfaceId, PackError> {
        validate_mesh(mesh)?;
        let mesh_key = source_mesh_key(mesh);
        if let Some(surface) = self.mesh_dedup.get(&mesh_key).and_then(|candidates| {
            candidates
                .iter()
                .copied()
                .find(|&candidate| self.surface_matches_source(SurfaceId(candidate), mesh))
        }) {
            return Ok(SurfaceId(surface));
        }
        let surface_index = checked_u32(self.surfaces.len(), "surface count")?;
        let material_index = checked_u32(self.materials.len(), "material count")?;
        let vertex_offset = checked_u32(self.vertices.len(), "vertex offset")?;
        let triangle_offset = checked_u32(self.triangles.len(), "triangle offset")?;
        let vertex_count = checked_u32(mesh.vertices.len(), "mesh vertex count")?;
        let triangle_count = checked_u32(mesh.indices.len() / 3, "mesh triangle count")?;
        vertex_offset
            .checked_add(vertex_count)
            .ok_or(PackError::CountOverflow("global vertex count"))?;
        triangle_offset
            .checked_add(triangle_count)
            .ok_or(PackError::CountOverflow("global triangle count"))?;

        let base = self.intern_texture(
            TextureSemantic::BaseColor,
            TexelEncoding::Srgb,
            mesh.texture.as_ref(),
            BASE_COLOR_FALLBACK,
        )?;
        let mr = self.intern_texture(
            TextureSemantic::MetallicRoughness,
            TexelEncoding::Linear,
            mesh.metallic_roughness_texture.as_ref(),
            METALLIC_ROUGHNESS_FALLBACK,
        )?;
        let normal = self.intern_texture(
            TextureSemantic::Normal,
            TexelEncoding::Linear,
            mesh.normal_texture.as_ref(),
            NORMAL_FALLBACK,
        )?;

        self.vertices
            .extend(mesh.vertices.iter().copied().map(GpuSurfaceVertex::from));
        for indices in mesh.indices.chunks_exact(3) {
            let mut global = [0; 3];
            for lane in 0..3 {
                global[lane] = vertex_offset
                    .checked_add(indices[lane])
                    .ok_or(PackError::CountOverflow("triangle vertex index"))?;
            }
            self.triangles.push(GpuTriangleRecord {
                indices: global,
                surface_index,
            });
        }
        self.materials.push(GpuMaterialRecord {
            textures: [base.0, mr.0, normal.0, 0],
            factors: [mesh.metallic_factor, mesh.roughness_factor, 0.0, 0.0],
        });
        self.surfaces.push(GpuSurfaceRecord {
            geometry: [vertex_offset, vertex_count, triangle_offset, triangle_count],
            material: [material_index, 0, 0, 0],
            bounds_min: [mesh.min.x, mesh.min.y, mesh.min.z, 0.0],
            bounds_max: [mesh.max.x, mesh.max.y, mesh.max.z, 0.0],
        });
        self.mesh_dedup
            .entry(mesh_key)
            .or_default()
            .push(surface_index);
        Ok(SurfaceId(surface_index))
    }

    fn surface_matches_source(&self, surface: SurfaceId, mesh: &CpuMesh) -> bool {
        let Some(surface_record) = self.surfaces.get(surface.0 as usize) else {
            return false;
        };
        let [vertex_offset, vertex_count, triangle_offset, triangle_count] =
            surface_record.geometry;
        if vertex_count as usize != mesh.vertices.len()
            || triangle_count as usize != mesh.indices.len() / 3
            || surface_record.bounds_min[..3]
                .iter()
                .zip(mesh.min.to_array())
                .any(|(packed, source)| packed.to_bits() != source.to_bits())
            || surface_record.bounds_max[..3]
                .iter()
                .zip(mesh.max.to_array())
                .any(|(packed, source)| packed.to_bits() != source.to_bits())
        {
            return false;
        }

        let Some(material) = self.materials.get(surface_record.material[0] as usize) else {
            return false;
        };
        if material.factors[0].to_bits() != mesh.metallic_factor.to_bits()
            || material.factors[1].to_bits() != mesh.roughness_factor.to_bits()
            || !self.pending_texture_matches(
                material.textures[0],
                TextureSemantic::BaseColor,
                TexelEncoding::Srgb,
                mesh.texture.as_ref(),
                BASE_COLOR_FALLBACK,
            )
            || !self.pending_texture_matches(
                material.textures[1],
                TextureSemantic::MetallicRoughness,
                TexelEncoding::Linear,
                mesh.metallic_roughness_texture.as_ref(),
                METALLIC_ROUGHNESS_FALLBACK,
            )
            || !self.pending_texture_matches(
                material.textures[2],
                TextureSemantic::Normal,
                TexelEncoding::Linear,
                mesh.normal_texture.as_ref(),
                NORMAL_FALLBACK,
            )
        {
            return false;
        }

        let vertex_start = vertex_offset as usize;
        let Some(vertices) = self
            .vertices
            .get(vertex_start..vertex_start + vertex_count as usize)
        else {
            return false;
        };
        if !vertices.iter().zip(&mesh.vertices).all(|(packed, source)| {
            let source = GpuSurfaceVertex::from(*source);
            bytemuck::bytes_of(packed) == bytemuck::bytes_of(&source)
        }) {
            return false;
        }

        let triangle_start = triangle_offset as usize;
        let Some(triangles) = self
            .triangles
            .get(triangle_start..triangle_start + triangle_count as usize)
        else {
            return false;
        };
        triangles
            .iter()
            .zip(mesh.indices.chunks_exact(3))
            .all(|(packed, source)| {
                packed.surface_index == surface.0
                    && (0..3).all(|lane| {
                        packed.indices[lane].checked_sub(vertex_offset) == Some(source[lane])
                    })
            })
    }

    fn pending_texture_matches(
        &self,
        texture: u32,
        semantic: TextureSemantic,
        encoding: TexelEncoding,
        image: Option<&CpuImage>,
        fallback: [u8; 4],
    ) -> bool {
        let Some(pending) = self.textures.get(texture as usize) else {
            return false;
        };
        let (width, height, rgba, source_present) = match image {
            Some(image) => (image.width, image.height, image.rgba.as_slice(), true),
            None => (1, 1, fallback.as_slice(), false),
        };
        pending.semantic == semantic
            && pending.encoding == encoding
            && pending.source_present == source_present
            && pending.mips.first().is_some_and(|mip| {
                mip.width == width && mip.height == height && mip.rgba.as_slice() == rgba
            })
    }

    /// Associate exactly one TLAS custom-data value with a source surface and transform.
    pub fn add_instance(
        &mut self,
        custom_data: u32,
        surface: SurfaceId,
        model: Mat4,
    ) -> Result<u32, PackError> {
        if custom_data > TLAS_CUSTOM_DATA_MAX {
            return Err(PackError::CustomDataOutOfRange(custom_data));
        }
        if surface.0 as usize >= self.surfaces.len() {
            return Err(PackError::UnknownSurface(surface));
        }
        if self.custom_data.contains_key(&custom_data) {
            return Err(PackError::DuplicateCustomData(custom_data));
        }
        if !model.to_cols_array().iter().all(|value| value.is_finite()) {
            return Err(PackError::NonFiniteInstanceTransform(custom_data));
        }
        let linear = Mat3::from_mat4(model);
        let determinant = linear.determinant();
        if !determinant.is_finite() || determinant.abs() <= 1.0e-12 {
            return Err(PackError::NonInvertibleInstanceTransform(custom_data));
        }
        let normal = linear.inverse().transpose();
        let index = checked_u32(self.instances.len(), "instance count")?;
        self.instances.push(GpuInstanceRecord {
            ids: [custom_data, surface.0, 0, 0],
            model: model.to_cols_array_2d(),
            normal_to_world: padded_mat3_cols(normal),
        });
        self.custom_data.insert(custom_data, index);
        Ok(index)
    }

    pub fn finish(self) -> Result<SurfacePack, PackError> {
        let PackedTextures {
            records,
            mip_rects,
            pages,
            source_level_zero_bytes,
            derived_mip_bytes,
        } = pack_textures(&self.textures, self.max_page_extent)?;
        let instance_lookup = self
            .custom_data
            .into_iter()
            .map(|(custom_data, instance_index)| GpuInstanceLookup {
                key_value: [custom_data, instance_index, 0, 0],
            })
            .collect();
        Ok(SurfacePack {
            vertices: self.vertices,
            triangles: self.triangles,
            surfaces: self.surfaces,
            materials: self.materials,
            textures: records,
            mip_rects,
            atlas_pages: pages,
            instances: self.instances,
            instance_lookup,
            source_level_zero_bytes,
            derived_mip_bytes,
        })
    }

    fn intern_texture(
        &mut self,
        semantic: TextureSemantic,
        encoding: TexelEncoding,
        image: Option<&CpuImage>,
        fallback: [u8; 4],
    ) -> Result<TextureId, PackError> {
        let (width, height, rgba, source_present) = match image {
            Some(image) => {
                validate_image(image, semantic)?;
                (image.width, image.height, image.rgba.as_slice(), true)
            }
            None => (1, 1, fallback.as_slice(), false),
        };
        let key = TextureKey {
            semantic,
            encoding,
            width,
            height,
            byte_len: rgba.len(),
            hash: stable_bytes_hash(rgba),
        };
        if let Some(candidates) = self.texture_dedup.get(&key) {
            for &candidate in candidates {
                let pending = &self.textures[candidate as usize];
                if pending.source_present == source_present && pending.mips[0].rgba == rgba {
                    return Ok(TextureId(candidate));
                }
            }
        }

        let id = checked_u32(self.textures.len(), "texture count")?;
        self.textures.push(PendingTexture {
            semantic,
            encoding,
            source_present,
            mips: build_renderer_mips(width, height, rgba),
        });
        self.texture_dedup.entry(key).or_default().push(id);
        Ok(TextureId(id))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PackError {
    ZeroAtlasPageExtent,
    CountOverflow(&'static str),
    InvalidIndexCount(usize),
    IndexOutOfBounds {
        triangle: usize,
        index: u32,
        vertex_count: usize,
    },
    InvalidBounds,
    NonFiniteMaterialFactor,
    InvalidImageDimensions {
        semantic: TextureSemantic,
        width: u32,
        height: u32,
    },
    InvalidImageByteCount {
        semantic: TextureSemantic,
        expected: usize,
        actual: usize,
    },
    UnknownSurface(SurfaceId),
    CustomDataOutOfRange(u32),
    DuplicateCustomData(u32),
    NonFiniteInstanceTransform(u32),
    NonInvertibleInstanceTransform(u32),
    AtlasTooLarge,
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroAtlasPageExtent => write!(f, "atlas page extent must be non-zero"),
            Self::CountOverflow(what) => write!(f, "{what} exceeds u32 storage"),
            Self::InvalidIndexCount(count) => {
                write!(f, "index count {count} is not divisible by three")
            }
            Self::IndexOutOfBounds {
                triangle,
                index,
                vertex_count,
            } => write!(
                f,
                "triangle {triangle} index {index} exceeds {vertex_count} vertices"
            ),
            Self::InvalidBounds => write!(f, "mesh bounds are non-finite or inverted"),
            Self::NonFiniteMaterialFactor => write!(f, "material factors must be finite"),
            Self::InvalidImageDimensions {
                semantic,
                width,
                height,
            } => write!(
                f,
                "{semantic:?} image has invalid dimensions {width}x{height}"
            ),
            Self::InvalidImageByteCount {
                semantic,
                expected,
                actual,
            } => write!(
                f,
                "{semantic:?} image has {actual} bytes, expected {expected}"
            ),
            Self::UnknownSurface(surface) => write!(f, "unknown surface {}", surface.0),
            Self::CustomDataOutOfRange(value) => {
                write!(f, "TLAS custom data {value:#x} exceeds 24 bits")
            }
            Self::DuplicateCustomData(value) => {
                write!(f, "TLAS custom data {value:#x} is already registered")
            }
            Self::NonFiniteInstanceTransform(value) => {
                write!(f, "instance {value:#x} transform is non-finite")
            }
            Self::NonInvertibleInstanceTransform(value) => {
                write!(f, "instance {value:#x} transform is non-invertible")
            }
            Self::AtlasTooLarge => write!(f, "packed atlas exceeds u32 address space"),
        }
    }
}

impl std::error::Error for PackError {}

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    UnknownTexture(TextureId),
    UnknownMaterial(MaterialId),
    UnknownInstance(u32),
    InstanceRecordOutOfRange(u32),
    InstanceRecordMismatch {
        instance_index: u32,
        /// `[custom_data, surface_index]`.
        expected: [u32; 2],
        actual: [u32; 2],
    },
    NonFiniteInstanceRecord(u32),
    MipOutOfRange {
        texture: TextureId,
        mip: u32,
        mip_count: u32,
    },
    TexelOutOfRange {
        texture: TextureId,
        mip: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    PrimitiveOutOfRange {
        surface: SurfaceId,
        primitive: u32,
        primitive_count: u32,
    },
    InvalidBarycentrics([f32; 2]),
    NonFiniteSampleInput,
    CorruptPack(&'static str),
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EvalError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicMaterialStateError {
    CountMismatch {
        expected: usize,
        actual: usize,
    },
    InstanceMismatch {
        index: usize,
        /// `[custom_data, surface_index]`.
        expected: [u32; 2],
        actual: [u32; 2],
    },
    UnknownEmissionMode {
        index: usize,
        mode: u32,
    },
    NonFiniteEmission(usize),
    NegativeEmission(usize),
    DisabledEmissionIsNonZero(usize),
}

impl fmt::Display for DynamicMaterialStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DynamicMaterialStateError {}

fn validate_mesh(mesh: &CpuMesh) -> Result<(), PackError> {
    if mesh.indices.len() % 3 != 0 {
        return Err(PackError::InvalidIndexCount(mesh.indices.len()));
    }
    for (triangle, indices) in mesh.indices.chunks_exact(3).enumerate() {
        for &index in indices {
            if index as usize >= mesh.vertices.len() {
                return Err(PackError::IndexOutOfBounds {
                    triangle,
                    index,
                    vertex_count: mesh.vertices.len(),
                });
            }
        }
    }
    if !mesh.min.is_finite()
        || !mesh.max.is_finite()
        || mesh.min.x > mesh.max.x
        || mesh.min.y > mesh.max.y
        || mesh.min.z > mesh.max.z
    {
        return Err(PackError::InvalidBounds);
    }
    if !mesh.metallic_factor.is_finite() || !mesh.roughness_factor.is_finite() {
        return Err(PackError::NonFiniteMaterialFactor);
    }
    if let Some(image) = &mesh.texture {
        validate_image(image, TextureSemantic::BaseColor)?;
    }
    if let Some(image) = &mesh.metallic_roughness_texture {
        validate_image(image, TextureSemantic::MetallicRoughness)?;
    }
    if let Some(image) = &mesh.normal_texture {
        validate_image(image, TextureSemantic::Normal)?;
    }
    Ok(())
}

fn validate_image(image: &CpuImage, semantic: TextureSemantic) -> Result<(), PackError> {
    if image.width == 0 || image.height == 0 {
        return Err(PackError::InvalidImageDimensions {
            semantic,
            width: image.width,
            height: image.height,
        });
    }
    let expected_u64 = u64::from(image.width) * u64::from(image.height) * 4;
    let expected = usize::try_from(expected_u64).map_err(|_| PackError::AtlasTooLarge)?;
    if image.rgba.len() != expected {
        return Err(PackError::InvalidImageByteCount {
            semantic,
            expected,
            actual: image.rgba.len(),
        });
    }
    Ok(())
}

fn checked_u32(value: usize, what: &'static str) -> Result<u32, PackError> {
    u32::try_from(value).map_err(|_| PackError::CountOverflow(what))
}

fn stable_bytes_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn source_mesh_key(mesh: &CpuMesh) -> MeshKey {
    MeshKey {
        vertex_count: mesh.vertices.len(),
        index_count: mesh.indices.len(),
        vertex_hash: stable_bytes_hash(bytemuck::cast_slice(&mesh.vertices)),
        index_hash: stable_bytes_hash(bytemuck::cast_slice(&mesh.indices)),
        images: [
            source_image_key(mesh.texture.as_ref(), BASE_COLOR_FALLBACK),
            source_image_key(
                mesh.metallic_roughness_texture.as_ref(),
                METALLIC_ROUGHNESS_FALLBACK,
            ),
            source_image_key(mesh.normal_texture.as_ref(), NORMAL_FALLBACK),
        ],
        metallic_factor: mesh.metallic_factor.to_bits(),
        roughness_factor: mesh.roughness_factor.to_bits(),
        bounds_min: mesh.min.to_array().map(f32::to_bits),
        bounds_max: mesh.max.to_array().map(f32::to_bits),
    }
}

fn source_image_key(image: Option<&CpuImage>, fallback: [u8; 4]) -> SourceImageKey {
    let (present, width, height, rgba) = match image {
        Some(image) => (true, image.width, image.height, image.rgba.as_slice()),
        None => (false, 1, 1, fallback.as_slice()),
    };
    SourceImageKey {
        present,
        width,
        height,
        byte_len: rgba.len(),
        hash: stable_bytes_hash(rgba),
    }
}

/// Byte-for-byte equivalent to `renderer::upload_texture`'s CPU mip generation.
fn build_renderer_mips(width: u32, height: u32, rgba: &[u8]) -> Vec<MipPayload> {
    let mip_count = if width >= 8 && height >= 8 {
        32 - width.min(height).leading_zeros()
    } else {
        1
    };
    let mut out = Vec::with_capacity(mip_count as usize);
    let (mut lw, mut lh, mut level) = (width, height, rgba.to_vec());
    for mip in 0..mip_count {
        out.push(MipPayload {
            width: lw,
            height: lh,
            rgba: level.clone(),
        });
        if mip + 1 == mip_count {
            break;
        }
        let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            let (y0, y1) = ((y * 2).min(lh - 1), (y * 2 + 1).min(lh - 1));
            for x in 0..nw {
                let (x0, x1) = ((x * 2).min(lw - 1), (x * 2 + 1).min(lw - 1));
                for channel in 0..4usize {
                    let sample =
                        |sx: u32, sy: u32| level[((sy * lw + sx) * 4) as usize + channel] as u32;
                    let sum = sample(x0, y0) + sample(x1, y0) + sample(x0, y1) + sample(x1, y1);
                    next[((y * nw + x) * 4) as usize + channel] = (sum / 4) as u8;
                }
            }
        }
        (lw, lh, level) = (nw, nh, next);
    }
    out
}

#[derive(Clone, Copy, Debug)]
struct RectRef {
    texture: usize,
    mip: usize,
    rect_index: usize,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug)]
struct Placement {
    rect: RectRef,
    x: u32,
    y: u32,
}

#[derive(Debug)]
struct PagePlan {
    encoding: TexelEncoding,
    width: u32,
    height_limit: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    used_height: u32,
    dedicated: bool,
    placements: Vec<Placement>,
}

impl PagePlan {
    fn regular(encoding: TexelEncoding, width: u32, height_limit: u32) -> Self {
        Self {
            encoding,
            width,
            height_limit,
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            used_height: 0,
            dedicated: false,
            placements: Vec::new(),
        }
    }

    fn dedicated(encoding: TexelEncoding, rect: RectRef) -> Self {
        Self {
            encoding,
            width: rect.width,
            height_limit: rect.height,
            cursor_x: rect.width,
            cursor_y: 0,
            row_height: rect.height,
            used_height: rect.height,
            dedicated: true,
            placements: vec![Placement { rect, x: 0, y: 0 }],
        }
    }

    fn try_place(&mut self, rect: RectRef) -> bool {
        if self.dedicated || rect.width > self.width || rect.height > self.height_limit {
            return false;
        }
        let (mut x, mut y, mut row_height) = (self.cursor_x, self.cursor_y, self.row_height);
        if rect.width > self.width.saturating_sub(x) {
            x = 0;
            let Some(next_y) = y.checked_add(row_height) else {
                return false;
            };
            y = next_y;
            row_height = 0;
        }
        if rect.height > self.height_limit.saturating_sub(y) {
            return false;
        }
        let right = x + rect.width;
        let bottom = y + rect.height;
        self.placements.push(Placement { rect, x, y });
        self.cursor_x = right;
        self.cursor_y = y;
        self.row_height = row_height.max(rect.height);
        self.used_height = self.used_height.max(bottom);
        true
    }
}

struct PackedTextures {
    records: Vec<GpuTextureRecord>,
    mip_rects: Vec<GpuMipRect>,
    pages: Vec<AtlasPage>,
    source_level_zero_bytes: usize,
    derived_mip_bytes: usize,
}

fn pack_textures(
    textures: &[PendingTexture],
    max_page_extent: u32,
) -> Result<PackedTextures, PackError> {
    let mip_total = textures
        .iter()
        .map(|texture| texture.mips.len())
        .sum::<usize>();
    let mut records = Vec::with_capacity(textures.len());
    let mut rect_refs = Vec::with_capacity(mip_total);
    let mut next_mip = 0usize;
    let mut source_level_zero_bytes = 0usize;
    let mut derived_mip_bytes = 0usize;
    for (texture_index, texture) in textures.iter().enumerate() {
        let first = checked_u32(next_mip, "mip rect offset")?;
        let count = checked_u32(texture.mips.len(), "texture mip count")?;
        let base = &texture.mips[0];
        records.push(GpuTextureRecord {
            layout: [
                first,
                count,
                texture.semantic as u32,
                texture.encoding as u32,
            ],
            source: [
                base.width,
                base.height,
                u32::from(texture.source_present),
                0,
            ],
        });
        for (mip, payload) in texture.mips.iter().enumerate() {
            if mip == 0 {
                source_level_zero_bytes = source_level_zero_bytes
                    .checked_add(payload.rgba.len())
                    .ok_or(PackError::AtlasTooLarge)?;
            } else {
                derived_mip_bytes = derived_mip_bytes
                    .checked_add(payload.rgba.len())
                    .ok_or(PackError::AtlasTooLarge)?;
            }
            rect_refs.push(RectRef {
                texture: texture_index,
                mip,
                rect_index: next_mip,
                width: payload.width,
                height: payload.height,
            });
            next_mip += 1;
        }
    }

    let mut plans = Vec::<PagePlan>::new();
    for encoding in [TexelEncoding::Srgb, TexelEncoding::Linear] {
        let group: Vec<_> = rect_refs
            .iter()
            .copied()
            .filter(|rect| textures[rect.texture].encoding == encoding)
            .collect();
        if group.is_empty() {
            continue;
        }
        let regular: Vec<_> = group
            .iter()
            .copied()
            .filter(|rect| rect.width <= max_page_extent && rect.height <= max_page_extent)
            .collect();
        let target_width = atlas_target_width(&regular, max_page_extent);
        let mut current_regular: Option<usize> = None;
        for rect in group {
            if rect.width > max_page_extent || rect.height > max_page_extent {
                plans.push(PagePlan::dedicated(encoding, rect));
                continue;
            }
            let placed = current_regular
                .and_then(|index| plans.get_mut(index))
                .is_some_and(|page| page.try_place(rect));
            if !placed {
                let mut page = PagePlan::regular(encoding, target_width, max_page_extent);
                debug_assert!(page.try_place(rect));
                plans.push(page);
                current_regular = Some(plans.len() - 1);
            }
        }
    }

    let mut mip_rects = vec![GpuMipRect::zeroed(); mip_total];
    let mut pages = Vec::with_capacity(plans.len());
    let mut flat_texel_offset = 0u64;
    for plan in plans {
        let height = plan.used_height.max(1);
        let texel_count_u64 = u64::from(plan.width) * u64::from(height);
        let texel_count = usize::try_from(texel_count_u64).map_err(|_| PackError::AtlasTooLarge)?;
        let mut texels = vec![0u32; texel_count];
        let page_index = checked_u32(pages.len(), "atlas page count")?;
        for placement in plan.placements {
            let payload = &textures[placement.rect.texture].mips[placement.rect.mip];
            for y in 0..payload.height {
                for x in 0..payload.width {
                    let source =
                        ((u64::from(y) * u64::from(payload.width) + u64::from(x)) * 4) as usize;
                    let packed = pack_rgba8([
                        payload.rgba[source],
                        payload.rgba[source + 1],
                        payload.rgba[source + 2],
                        payload.rgba[source + 3],
                    ]);
                    let destination = (u64::from(placement.y + y) * u64::from(plan.width)
                        + u64::from(placement.x + x))
                        as usize;
                    texels[destination] = packed;
                }
            }
            mip_rects[placement.rect.rect_index] = GpuMipRect {
                page_origin: [
                    page_index,
                    placement.x,
                    placement.y,
                    placement.rect.mip as u32,
                ],
                extent: [payload.width, payload.height, 0, 0],
            };
        }
        let offset = u32::try_from(flat_texel_offset).map_err(|_| PackError::AtlasTooLarge)?;
        pages.push(AtlasPage {
            gpu: GpuAtlasPageRecord {
                layout: [offset, plan.width, height, plan.encoding as u32],
            },
            rgba8_texels: texels,
        });
        flat_texel_offset = flat_texel_offset
            .checked_add(texel_count_u64)
            .ok_or(PackError::AtlasTooLarge)?;
        if flat_texel_offset > u64::from(u32::MAX) {
            return Err(PackError::AtlasTooLarge);
        }
    }

    Ok(PackedTextures {
        records,
        mip_rects,
        pages,
        source_level_zero_bytes,
        derived_mip_bytes,
    })
}

fn atlas_target_width(rects: &[RectRef], max_page_extent: u32) -> u32 {
    if rects.is_empty() {
        return 1;
    }
    let max_width = rects.iter().map(|rect| rect.width).max().unwrap_or(1);
    let area = rects
        .iter()
        .map(|rect| u64::from(rect.width) * u64::from(rect.height))
        .sum::<u64>();
    let root = (area as f64).sqrt().ceil().max(1.0) as u32;
    let power = root.checked_next_power_of_two().unwrap_or(max_page_extent);
    power.max(max_width).min(max_page_extent)
}

fn semantic_from_u32(value: u32) -> Option<TextureSemantic> {
    match value {
        0 => Some(TextureSemantic::BaseColor),
        1 => Some(TextureSemantic::MetallicRoughness),
        2 => Some(TextureSemantic::Normal),
        _ => None,
    }
}

fn encoding_from_u32(value: u32) -> Option<TexelEncoding> {
    match value {
        0 => Some(TexelEncoding::Srgb),
        1 => Some(TexelEncoding::Linear),
        _ => None,
    }
}

fn pack_rgba8(rgba: [u8; 4]) -> u32 {
    u32::from(rgba[0])
        | (u32::from(rgba[1]) << 8)
        | (u32::from(rgba[2]) << 16)
        | (u32::from(rgba[3]) << 24)
}

fn unpack_rgba8(packed: u32) -> [u8; 4] {
    [
        packed as u8,
        (packed >> 8) as u8,
        (packed >> 16) as u8,
        (packed >> 24) as u8,
    ]
}

fn decode_rgba8(rgba: [u8; 4], encoding: TexelEncoding) -> [f32; 4] {
    let unorm = rgba.map(|channel| f32::from(channel) / 255.0);
    match encoding {
        TexelEncoding::Srgb => [
            srgb_to_linear(unorm[0]),
            srgb_to_linear(unorm[1]),
            srgb_to_linear(unorm[2]),
            unorm[3],
        ],
        TexelEncoding::Linear => unorm,
    }
}

pub fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

fn bary2(values: [Vec2; 3], weights: [f32; 3]) -> Vec2 {
    values[0] * weights[0] + values[1] * weights[1] + values[2] * weights[2]
}

fn bary3(values: [Vec3; 3], weights: [f32; 3]) -> Vec3 {
    values[0] * weights[0] + values[1] * weights[1] + values[2] * weights[2]
}

fn bary4(values: [Vec4; 3], weights: [f32; 3]) -> Vec4 {
    values[0] * weights[0] + values[1] * weights[1] + values[2] * weights[2]
}

fn padded_mat3_cols(matrix: Mat3) -> [[f32; 4]; 3] {
    [
        [matrix.x_axis.x, matrix.x_axis.y, matrix.x_axis.z, 0.0],
        [matrix.y_axis.x, matrix.y_axis.y, matrix.y_axis.z, 0.0],
        [matrix.z_axis.x, matrix.z_axis.y, matrix.z_axis.z, 0.0],
    ]
}

fn mat3_from_padded_cols(columns: [[f32; 4]; 3]) -> Mat3 {
    Mat3::from_cols(
        Vec3::from_slice(&columns[0][..3]),
        Vec3::from_slice(&columns[1][..3]),
        Vec3::from_slice(&columns[2][..3]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    fn image(width: u32, height: u32, rgba: Vec<u8>) -> CpuImage {
        CpuImage {
            width,
            height,
            rgba,
        }
    }

    fn test_mesh(base: CpuImage, mr: CpuImage, normal: CpuImage) -> CpuMesh {
        let vertices = vec![
            Vertex {
                pos: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
            },
            Vertex {
                pos: [2.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
            },
            Vertex {
                pos: [0.0, 2.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 1.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
            },
        ];
        CpuMesh {
            vertices,
            indices: vec![0, 1, 2],
            texture: Some(base),
            metallic_roughness_texture: Some(mr),
            normal_texture: Some(normal),
            metallic_factor: 0.5,
            roughness_factor: 0.8,
            min: Vec3::ZERO,
            max: Vec3::new(2.0, 2.0, 0.0),
        }
    }

    fn one_pixel(rgba: [u8; 4]) -> CpuImage {
        image(1, 1, rgba.to_vec())
    }

    fn assert_vec3_close(actual: Vec3, expected: Vec3) {
        assert!(
            actual.abs_diff_eq(expected, 1.0e-5),
            "actual={actual:?}, expected={expected:?}"
        );
    }

    #[test]
    fn pod_layouts_match_wgsl_storage_lanes() {
        assert_eq!(size_of::<GpuSurfaceVertex>(), 64);
        assert_eq!(size_of::<GpuTriangleRecord>(), 16);
        assert_eq!(size_of::<GpuSurfaceRecord>(), 64);
        assert_eq!(size_of::<GpuMaterialRecord>(), 32);
        assert_eq!(size_of::<GpuTextureRecord>(), 32);
        assert_eq!(size_of::<GpuMipRect>(), 32);
        assert_eq!(size_of::<GpuAtlasPageRecord>(), 16);
        assert_eq!(size_of::<GpuInstanceRecord>(), 128);
        assert_eq!(size_of::<GpuInstanceLookup>(), 16);
    }

    #[test]
    fn geometry_and_level_zero_texels_are_lossless() {
        let base = image(
            2,
            2,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        );
        let mr = one_pixel([17, 18, 19, 20]);
        let normal = one_pixel([21, 22, 23, 24]);
        let mesh = test_mesh(base.clone(), mr.clone(), normal.clone());
        let mut builder = SurfacePackBuilder::new(8).unwrap();
        let surface = builder.add_mesh(&mesh).unwrap();
        builder.add_instance(7, surface, Mat4::IDENTITY).unwrap();
        let pack = builder.finish().unwrap();

        assert_eq!(pack.vertices().len(), mesh.vertices.len());
        for (packed, source) in pack.vertices().iter().zip(&mesh.vertices) {
            assert_eq!(&packed.position[..3], source.pos.as_slice());
            assert_eq!(&packed.normal[..3], source.normal.as_slice());
            assert_eq!(packed.uv, source.uv);
            assert_eq!(packed.tangent, source.tangent);
        }
        assert_eq!(pack.triangles()[0].indices, [0, 1, 2]);
        let material = pack.materials()[0];
        assert_eq!(
            pack.texture_level_rgba8(TextureId(material.textures[0]), 0)
                .unwrap(),
            base.rgba
        );
        assert_eq!(
            pack.texture_level_rgba8(TextureId(material.textures[1]), 0)
                .unwrap(),
            mr.rgba
        );
        assert_eq!(
            pack.texture_level_rgba8(TextureId(material.textures[2]), 0)
                .unwrap(),
            normal.rgba
        );
        assert_eq!(
            pack.texture_encoding(TextureId(material.textures[0]))
                .unwrap(),
            TexelEncoding::Srgb
        );
        assert_eq!(
            pack.texture_encoding(TextureId(material.textures[1]))
                .unwrap(),
            TexelEncoding::Linear
        );
        assert_eq!(
            pack.texture_encoding(TextureId(material.textures[2]))
                .unwrap(),
            TexelEncoding::Linear
        );
    }

    #[test]
    fn byte_identical_meshes_share_one_surface_without_merging_instances() {
        let mesh = test_mesh(
            one_pixel([17, 33, 65, 255]),
            one_pixel([0, 140, 91, 255]),
            one_pixel([128, 128, 255, 255]),
        );
        let mut builder = SurfacePackBuilder::default();
        let first = builder.add_mesh(&mesh).unwrap();
        let identical = builder.add_mesh(&mesh.clone()).unwrap();
        assert_eq!(identical, first);
        assert_eq!(builder.vertices.len(), mesh.vertices.len());
        assert_eq!(builder.triangles.len(), mesh.indices.len() / 3);
        assert_eq!(builder.surfaces.len(), 1);
        assert_eq!(builder.materials.len(), 1);

        let mut changed_vertex = mesh.clone();
        changed_vertex.vertices[0].pos[0] =
            f32::from_bits(changed_vertex.vertices[0].pos[0].to_bits().wrapping_add(1));
        assert_ne!(builder.add_mesh(&changed_vertex).unwrap(), first);

        let mut changed_index = mesh.clone();
        changed_index.indices.swap(1, 2);
        assert_ne!(builder.add_mesh(&changed_index).unwrap(), first);

        let mut changed_material = mesh.clone();
        changed_material.roughness_factor =
            f32::from_bits(changed_material.roughness_factor.to_bits().wrapping_add(1));
        assert_ne!(builder.add_mesh(&changed_material).unwrap(), first);

        let mut changed_texture = mesh.clone();
        changed_texture.texture.as_mut().unwrap().rgba[0] ^= 1;
        assert_ne!(builder.add_mesh(&changed_texture).unwrap(), first);

        builder.add_instance(1, first, Mat4::IDENTITY).unwrap();
        builder
            .add_instance(2, identical, Mat4::from_translation(Vec3::X))
            .unwrap();
        let pack = builder.finish().unwrap();
        assert_eq!(pack.instances().len(), 2);
        assert_eq!(pack.instances()[0].ids[1], first.0);
        assert_eq!(pack.instances()[1].ids[1], first.0);
    }

    #[test]
    fn nearest_repeat_samples_exact_texture_corners_and_srgb() {
        let base = image(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 128, 128, 128, 255,
            ],
        );
        let mesh = test_mesh(base, one_pixel([255; 4]), one_pixel([128, 128, 255, 255]));
        let mut builder = SurfacePackBuilder::default();
        builder.add_mesh(&mesh).unwrap();
        let pack = builder.finish().unwrap();
        let texture = TextureId(pack.materials()[0].textures[0]);

        assert_eq!(
            pack.sample_texture(texture, Vec2::new(0.25, 0.25), 0.0)
                .unwrap(),
            [1.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            pack.sample_texture(texture, Vec2::new(0.75, 0.25), 0.0)
                .unwrap(),
            [0.0, 1.0, 0.0, 1.0]
        );
        assert_eq!(
            pack.sample_texture(texture, Vec2::new(0.25, 0.75), 0.0)
                .unwrap(),
            [0.0, 0.0, 1.0, 1.0]
        );
        let grey = pack
            .sample_texture(texture, Vec2::new(0.75, 0.75), 0.0)
            .unwrap();
        let expected = srgb_to_linear(128.0 / 255.0);
        assert!((grey[0] - expected).abs() < 1.0e-6);
        assert_eq!(
            pack.sample_texture(texture, Vec2::new(1.25, -0.25), 0.0)
                .unwrap(),
            [0.0, 0.0, 1.0, 1.0]
        );
    }

    #[test]
    fn renderer_mips_and_linear_mip_filter_match_reference() {
        let mut rgba = vec![0u8; 8 * 8 * 4];
        for pixel in rgba.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        for (x, y, value) in [(0, 0, 0), (1, 0, 64), (0, 1, 128), (1, 1, 255)] {
            let offset = ((y * 8 + x) * 4) as usize;
            rgba[offset..offset + 3].fill(value);
        }
        let mesh = test_mesh(
            image(8, 8, rgba),
            one_pixel([255; 4]),
            one_pixel([128, 128, 255, 255]),
        );
        let mut builder = SurfacePackBuilder::default();
        builder.add_mesh(&mesh).unwrap();
        let pack = builder.finish().unwrap();
        let texture = TextureId(pack.materials()[0].textures[0]);
        assert_eq!(pack.textures()[texture.0 as usize].layout[1], 4);
        assert_eq!(pack.texture_texel_rgba8(texture, 1, 0, 0).unwrap()[0], 111);

        let uv = Vec2::new(0.125, 0.125);
        let level0 = pack.sample_texture(texture, uv, 0.0).unwrap();
        let level1 = pack.sample_texture(texture, uv, 1.0).unwrap();
        let between = pack.sample_texture(texture, uv, 0.5).unwrap();
        for channel in 0..4 {
            assert!((between[channel] - (level0[channel] + level1[channel]) * 0.5).abs() < 1.0e-6);
        }
    }

    #[test]
    fn hit_maps_custom_data_primitive_barycentrics_material_and_transform() {
        let mesh = test_mesh(
            one_pixel([128, 64, 32, 255]),
            one_pixel([0, 64, 128, 255]),
            one_pixel([128, 128, 255, 255]),
        );
        let rotation = Quat::from_rotation_y(0.6);
        let model = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 3.0, 4.0),
            rotation,
            Vec3::new(7.0, 8.0, 9.0),
        );
        let mut builder = SurfacePackBuilder::default();
        let surface = builder.add_mesh(&mesh).unwrap();
        builder.add_instance(0x40_0123, surface, model).unwrap();
        builder
            .add_instance(
                0x40_0124,
                surface,
                Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)),
            )
            .unwrap();
        let pack = builder.finish().unwrap();
        let hit = pack
            .evaluate_hit(0x40_0123, 0, [0.25, 0.25], MaterialLod::splat(0.0))
            .unwrap();

        assert_eq!(hit.vertex_indices, [0, 1, 2]);
        assert_eq!(hit.barycentric_weights, [0.5, 0.25, 0.25]);
        assert_vec3_close(hit.local_position, Vec3::new(0.5, 0.5, 0.0));
        assert_vec3_close(
            hit.world_position,
            model.transform_point3(hit.local_position),
        );
        let expected_normal = (Mat3::from_mat4(model).inverse().transpose() * Vec3::Z).normalize();
        assert_vec3_close(hit.world_interpolated_normal, expected_normal);
        assert_vec3_close(hit.world_triangle_normal, expected_normal);
        // The live raster fallback is the exact UNORM texel (128,128,255), whose X/Y decode to
        // 1/255 rather than mathematical zero. Preserve that tiny tilt instead of special-casing it.
        let fallback_xy = 128.0 / 255.0 * 2.0 - 1.0;
        let expected_shading = (hit.world_tangent.truncate() * fallback_xy
            + hit.world_bitangent * fallback_xy
            + expected_normal)
            .normalize();
        assert_vec3_close(hit.world_shading_normal, expected_shading);
        assert!((hit.material_sample.metallic - (128.0 / 255.0) * 0.5).abs() < 1.0e-6);
        assert!((hit.material_sample.roughness - (64.0 / 255.0) * 0.8).abs() < 1.0e-6);

        let mirrored = pack
            .evaluate_hit(0x40_0124, 0, [0.25, 0.25], MaterialLod::splat(0.0))
            .unwrap();
        assert_vec3_close(mirrored.world_triangle_normal, -Vec3::Z);
        assert_vec3_close(mirrored.world_interpolated_normal, Vec3::Z);

        for barycentrics in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            let corner = pack
                .evaluate_hit(0x40_0123, 0, barycentrics, MaterialLod::splat(0.0))
                .unwrap();
            let index = if barycentrics == [0.0, 0.0] {
                0
            } else if barycentrics == [1.0, 0.0] {
                1
            } else {
                2
            };
            assert_vec3_close(corner.local_position, Vec3::from(mesh.vertices[index].pos));
            assert_eq!(corner.uv, Vec2::from(mesh.vertices[index].uv));
        }
    }

    #[test]
    fn missing_maps_use_raster_equivalent_fallbacks_and_metadata() {
        let mut mesh = test_mesh(
            one_pixel([10, 20, 30, 40]),
            one_pixel([1, 2, 3, 4]),
            one_pixel([5, 6, 7, 8]),
        );
        mesh.texture = None;
        mesh.metallic_roughness_texture = None;
        mesh.normal_texture = None;
        let mut builder = SurfacePackBuilder::default();
        builder.add_mesh(&mesh).unwrap();
        let pack = builder.finish().unwrap();
        let material = pack.materials()[0];
        let base = TextureId(material.textures[0]);
        let mr = TextureId(material.textures[1]);
        let normal = TextureId(material.textures[2]);
        assert!(!pack.texture_source_present(base).unwrap());
        assert!(!pack.texture_source_present(mr).unwrap());
        assert!(!pack.texture_source_present(normal).unwrap());
        assert_eq!(
            pack.texture_level_rgba8(base, 0).unwrap(),
            BASE_COLOR_FALLBACK
        );
        assert_eq!(
            pack.texture_level_rgba8(mr, 0).unwrap(),
            METALLIC_ROUGHNESS_FALLBACK
        );
        assert_eq!(
            pack.texture_level_rgba8(normal, 0).unwrap(),
            NORMAL_FALLBACK
        );
    }

    #[test]
    fn sparse_custom_data_lookup_is_sorted_and_exact() {
        let mesh = test_mesh(
            one_pixel([255; 4]),
            one_pixel([255; 4]),
            one_pixel([128, 128, 255, 255]),
        );
        let mut builder = SurfacePackBuilder::default();
        let surface = builder.add_mesh(&mesh).unwrap();
        builder.add_instance(900, surface, Mat4::IDENTITY).unwrap();
        builder.add_instance(2, surface, Mat4::IDENTITY).unwrap();
        assert_eq!(
            builder.add_instance(2, surface, Mat4::IDENTITY),
            Err(PackError::DuplicateCustomData(2))
        );
        let pack = builder.finish().unwrap();
        assert_eq!(pack.instance_lookup()[0].key_value[0], 2);
        assert_eq!(pack.instance_lookup()[1].key_value[0], 900);
        assert!(matches!(
            pack.evaluate_hit(3, 0, [0.0, 0.0], MaterialLod::default()),
            Err(EvalError::UnknownInstance(3))
        ));
    }

    #[test]
    fn dynamic_material_state_is_aligned_validated_and_gpu_stable() {
        assert_eq!(size_of::<GpuInstanceMaterialState>(), 32);
        let mesh = test_mesh(
            one_pixel([255; 4]),
            one_pixel([255; 4]),
            one_pixel([128, 128, 255, 255]),
        );
        let mut builder = SurfacePackBuilder::default();
        let surface = builder.add_mesh(&mesh).unwrap();
        builder.add_instance(900, surface, Mat4::IDENTITY).unwrap();
        builder.add_instance(2, surface, Mat4::IDENTITY).unwrap();
        let pack = builder.finish().unwrap();

        let mut states = pack.default_instance_material_states();
        assert_eq!(states[0].ids, [900, surface.0, 0, 0]);
        assert_eq!(states[1].ids, [2, surface.0, 0, 0]);
        assert_eq!(pack.instance_material_state_bytes(), 64);
        pack.validate_instance_material_states(&states).unwrap();

        states[0] = GpuInstanceMaterialState::emissive(
            900,
            surface,
            InstanceEmissionMode::FixedLinearRadiance,
            3.5,
            [1.0, 0.25, 0.04],
        );
        assert_eq!(
            states[0].mode(),
            Some(InstanceEmissionMode::FixedLinearRadiance)
        );
        pack.validate_instance_material_states(&states).unwrap();

        let mut stale = states.clone();
        stale.swap(0, 1);
        assert!(matches!(
            pack.validate_instance_material_states(&stale),
            Err(DynamicMaterialStateError::InstanceMismatch { index: 0, .. })
        ));
        states[0].ids[2] = 99;
        assert!(matches!(
            pack.validate_instance_material_states(&states),
            Err(DynamicMaterialStateError::UnknownEmissionMode { index: 0, mode: 99 })
        ));
    }

    #[test]
    fn memory_report_accounts_for_every_gpu_array_and_atlas_padding() {
        let mesh = test_mesh(
            one_pixel([255; 4]),
            one_pixel([255; 4]),
            one_pixel([128, 128, 255, 255]),
        );
        let mut builder = SurfacePackBuilder::new(4).unwrap();
        let surface = builder.add_mesh(&mesh).unwrap();
        builder.add_instance(0, surface, Mat4::IDENTITY).unwrap();
        let pack = builder.finish().unwrap();
        let cost = pack.memory_cost();
        assert_eq!(cost.vertex_bytes, 3 * 64);
        assert_eq!(cost.triangle_bytes, 16);
        assert_eq!(cost.instance_bytes, 128);
        assert_eq!(cost.instance_lookup_bytes, 16);
        assert_eq!(cost.source_level_zero_bytes, 12);
        assert_eq!(
            cost.total_gpu_payload_bytes,
            cost.vertex_bytes
                + cost.triangle_bytes
                + cost.surface_bytes
                + cost.material_bytes
                + cost.texture_record_bytes
                + cost.mip_rect_bytes
                + cost.instance_bytes
                + cost.instance_lookup_bytes
                + cost.atlas_texel_bytes
        );
        assert_eq!(
            cost.atlas_padding_bytes,
            cost.atlas_texel_bytes
                .saturating_sub(cost.source_level_zero_bytes + cost.derived_mip_bytes)
        );
    }
}
