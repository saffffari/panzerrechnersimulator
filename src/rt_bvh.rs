//! Portable, source-triangle acceleration for secondary rays.
//!
//! [`RtSceneSurfaceData`] expands the exact vertices and instance transforms in a
//! [`SurfacePack`](crate::rt_surface::SurfacePack) into world-space triangles. [`TriangleBvh`]
//! builds deterministic conservative bounds over those triangles and emits storage-buffer-safe
//! nodes plus source references. A leaf never substitutes an OBB, terrain heightfield, screen
//! sample, or average material: its record resolves back to the same instance, surface, primitive,
//! and packed triangle used by [`SurfacePack::evaluate_hit`](crate::rt_surface::SurfacePack::evaluate_hit).
//!
//! This module is renderer-independent. It does not bind the buffers or claim that a shipping
//! render path consumes them yet.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3};

use crate::rt_surface::{GpuInstanceRecord, SurfacePack};

/// A four-triangle leaf keeps the portable payload near 40 bytes per source triangle for balanced
/// scenes while still producing a real per-triangle acceleration structure.
pub const DEFAULT_BVH_LEAF_SIZE: usize = 4;

pub const BVH_NODE_INTERNAL: u32 = 0;
pub const BVH_NODE_LEAF: u32 = 1;

/// One software-BVH node. Bounds occupy full vec4 lanes for WGSL storage-buffer alignment.
///
/// `links` means:
/// - internal: `[left_child, right_child, split_axis, BVH_NODE_INTERNAL]`
/// - leaf: `[first_primitive, primitive_count, 0, BVH_NODE_LEAF]`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuBvhNode {
    pub bounds_min: [f32; 4],
    pub bounds_max: [f32; 4],
    pub links: [u32; 4],
}

impl GpuBvhNode {
    pub fn is_leaf(&self) -> bool {
        self.links[3] == BVH_NODE_LEAF
    }

    pub fn bounds(&self) -> RtAabb {
        RtAabb {
            min: Vec3::from_slice(&self.bounds_min[..3]),
            max: Vec3::from_slice(&self.bounds_max[..3]),
        }
    }
}

/// Exact leaf reference. No geometry is baked into this record; the shader follows it into the
/// packed source vertex/triangle/instance buffers.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct GpuBvhPrimitive {
    /// instance record, surface-relative primitive, surface record, packed triangle record.
    pub source: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtAabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl RtAabb {
    fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }

    fn from_triangle_conservative(points: [Vec3; 3]) -> Self {
        let raw_min = points[0].min(points[1]).min(points[2]);
        let raw_max = points[0].max(points[1]).max(points[2]);
        Self {
            min: Vec3::new(
                next_float_down(raw_min.x),
                next_float_down(raw_min.y),
                next_float_down(raw_min.z),
            ),
            max: Vec3::new(
                next_float_up(raw_max.x),
                next_float_up(raw_max.y),
                next_float_up(raw_max.z),
            ),
        }
    }

    fn include_aabb(&mut self, other: Self) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }

    pub fn contains_point(&self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.y >= self.min.y
            && point.z >= self.min.z
            && point.x <= self.max.x
            && point.y <= self.max.y
            && point.z <= self.max.z
    }

    pub fn contains_aabb(&self, other: Self) -> bool {
        self.contains_point(other.min) && self.contains_point(other.max)
    }

    pub fn extent(&self) -> Vec3 {
        self.max - self.min
    }
}

/// One exact transformed source triangle used while building the portable BVH.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtSceneTriangle {
    pub source: GpuBvhPrimitive,
    pub instance_custom_data: u32,
    pub world_positions: [Vec3; 3],
    pub conservative_bounds: RtAabb,
    pub centroid: Vec3,
}

/// Exact scene geometry derived from a [`SurfacePack`]. This is deliberately separate from the
/// BVH so a caller can rebuild or refit acceleration after authoritative instance transforms move.
#[derive(Clone, Debug, Default)]
pub struct RtSceneSurfaceData {
    triangles: Vec<RtSceneTriangle>,
    world_bounds: Option<RtAabb>,
    /// Dense pack-indexed records carrying the current authoritative transforms. Leaf
    /// `source[0]` indexes this directly, so GPU traversal never falls back to a stale pack-time
    /// model matrix.
    gpu_instances: Vec<GpuInstanceRecord>,
}

/// One currently active renderer instance. The pack index preserves the exact custom-data/surface
/// identity while `model` comes from the same authoritative transform used for the visible draw.
/// Supplying only active instances also prevents hidden objects from lingering in the portable BVH.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtSceneInstance {
    pub pack_instance_index: u32,
    pub model: Mat4,
}

impl RtSceneSurfaceData {
    /// Convenience snapshot using every transform stored in the pack. Dynamic renderers should use
    /// [`Self::from_active_instances`] with their current visible-instance transforms instead.
    pub fn from_surface_pack(pack: &SurfacePack) -> Result<Self, BvhBuildError> {
        let instances = pack
            .instances()
            .iter()
            .enumerate()
            .map(|(index, instance)| {
                Ok(RtSceneInstance {
                    pack_instance_index: checked_u32(index, "instance index")?,
                    model: Mat4::from_cols_array_2d(&instance.model),
                })
            })
            .collect::<Result<Vec<_>, BvhBuildError>>()?;
        Self::from_active_instances(pack, &instances)
    }

    /// Build only from current active instances and their current authoritative model matrices.
    /// This is the portable equivalent of writing a frame's TLAS instances; it does not reuse stale
    /// pack-time transforms and does not leave invisible geometry intersectable.
    pub fn from_active_instances(
        pack: &SurfacePack,
        active_instances: &[RtSceneInstance],
    ) -> Result<Self, BvhBuildError> {
        let mut triangles = Vec::new();
        let mut world_bounds: Option<RtAabb> = None;
        let mut seen = BTreeSet::new();
        let mut gpu_instances = pack.instances().to_vec();

        for active in active_instances {
            if !seen.insert(active.pack_instance_index) {
                return Err(BvhBuildError::DuplicatePackInstance(
                    active.pack_instance_index,
                ));
            }
            let instance_index = active.pack_instance_index;
            let instance = pack
                .instances()
                .get(instance_index as usize)
                .ok_or(BvhBuildError::UnknownPackInstance(instance_index))?;
            if !active
                .model
                .to_cols_array()
                .iter()
                .all(|value| value.is_finite())
            {
                return Err(BvhBuildError::NonFiniteInstanceTransform(instance.ids[0]));
            }
            let linear = Mat3::from_mat4(active.model);
            let determinant = linear.determinant();
            if !determinant.is_finite() || determinant == 0.0 {
                return Err(BvhBuildError::NonInvertibleInstanceTransform(
                    instance.ids[0],
                ));
            }
            let normal_to_world = linear.inverse().transpose();
            if !normal_to_world
                .to_cols_array()
                .iter()
                .all(|value| value.is_finite())
            {
                return Err(BvhBuildError::NonInvertibleInstanceTransform(
                    instance.ids[0],
                ));
            }
            gpu_instances[instance_index as usize] = GpuInstanceRecord {
                ids: instance.ids,
                model: active.model.to_cols_array_2d(),
                normal_to_world: padded_mat3_cols(normal_to_world),
            };
            let surface_index = instance.ids[1];
            let surface = pack
                .surfaces()
                .get(surface_index as usize)
                .ok_or(BvhBuildError::CorruptSurfacePack("instance surface index"))?;
            let model = active.model;

            for primitive_index in 0..surface.geometry[3] {
                let packed_triangle_index = surface.geometry[2]
                    .checked_add(primitive_index)
                    .ok_or(BvhBuildError::CountOverflow("packed triangle index"))?;
                let triangle = pack
                    .triangles()
                    .get(packed_triangle_index as usize)
                    .ok_or(BvhBuildError::CorruptSurfacePack("triangle offset"))?;
                if triangle.surface_index != surface_index {
                    return Err(BvhBuildError::CorruptSurfacePack("triangle surface index"));
                }

                let mut world_positions = [Vec3::ZERO; 3];
                for lane in 0..3 {
                    let vertex = pack
                        .vertices()
                        .get(triangle.indices[lane] as usize)
                        .ok_or(BvhBuildError::CorruptSurfacePack("triangle vertex index"))?;
                    let local = Vec3::from_slice(&vertex.position[..3]);
                    let world = model.transform_point3(local);
                    if !world.is_finite() {
                        return Err(BvhBuildError::NonFiniteWorldVertex {
                            instance_custom_data: instance.ids[0],
                            primitive_index,
                            vertex: lane as u32,
                        });
                    }
                    world_positions[lane] = world;
                }

                let conservative_bounds = RtAabb::from_triangle_conservative(world_positions);
                let centroid = triangle_centroid(world_positions);
                let source = GpuBvhPrimitive {
                    source: [
                        instance_index,
                        primitive_index,
                        surface_index,
                        packed_triangle_index,
                    ],
                };
                let scene_triangle = RtSceneTriangle {
                    source,
                    instance_custom_data: instance.ids[0],
                    world_positions,
                    conservative_bounds,
                    centroid,
                };
                if let Some(bounds) = &mut world_bounds {
                    bounds.include_aabb(conservative_bounds);
                } else {
                    world_bounds = Some(conservative_bounds);
                }
                triangles.push(scene_triangle);
            }
        }

        checked_u32(triangles.len(), "scene triangle count")?;
        Ok(Self {
            triangles,
            world_bounds,
            gpu_instances,
        })
    }

    pub fn triangles(&self) -> &[RtSceneTriangle] {
        &self.triangles
    }

    pub fn world_bounds(&self) -> Option<RtAabb> {
        self.world_bounds
    }

    /// Pack-indexed instance records with current transforms. Upload this array beside the BVH;
    /// `GpuBvhPrimitive::source[0]` indexes it directly.
    pub fn gpu_instances(&self) -> &[GpuInstanceRecord] {
        &self.gpu_instances
    }

    pub fn len(&self) -> usize {
        self.triangles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    pub fn cpu_bytes(&self) -> usize {
        self.triangles.len() * size_of::<RtSceneTriangle>()
    }

    pub fn gpu_instance_bytes(&self) -> usize {
        self.gpu_instances.len() * size_of::<GpuInstanceRecord>()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BvhMemoryCost {
    pub node_bytes: usize,
    pub primitive_reference_bytes: usize,
    /// CPU-only positions/custom-data retained for deterministic reference traversal. These are not
    /// part of the renderer upload.
    pub reference_triangle_bytes: usize,
    pub total_gpu_payload_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BvhStats {
    pub triangle_count: usize,
    pub node_count: usize,
    pub leaf_count: usize,
    pub internal_node_count: usize,
    pub max_depth: usize,
    pub max_leaf_occupancy: usize,
    pub world_bounds: Option<RtAabb>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CpuBvhPrimitive {
    world_positions: [Vec3; 3],
    instance_custom_data: u32,
    scene_triangle_index: u32,
}

/// Deterministic median-split triangle BVH. Node zero is the root whenever the scene is non-empty.
#[derive(Clone, Debug, Default)]
pub struct TriangleBvh {
    nodes: Vec<GpuBvhNode>,
    primitive_indices: Vec<GpuBvhPrimitive>,
    cpu_primitives: Vec<CpuBvhPrimitive>,
    stats: BvhStats,
}

impl TriangleBvh {
    pub fn build(scene: &RtSceneSurfaceData) -> Result<Self, BvhBuildError> {
        Self::build_with_leaf_size(scene, DEFAULT_BVH_LEAF_SIZE)
    }

    pub fn build_with_leaf_size(
        scene: &RtSceneSurfaceData,
        leaf_size: usize,
    ) -> Result<Self, BvhBuildError> {
        if leaf_size == 0 {
            return Err(BvhBuildError::ZeroLeafSize);
        }
        checked_u32(scene.len(), "scene triangle count")?;
        if scene.is_empty() {
            return Ok(Self::default());
        }

        let mut order: Vec<u32> = (0..scene.len() as u32).collect();
        let mut bvh = Self {
            nodes: Vec::with_capacity(scene.len().saturating_mul(2).saturating_sub(1)),
            primitive_indices: Vec::with_capacity(scene.len()),
            cpu_primitives: Vec::with_capacity(scene.len()),
            stats: BvhStats {
                triangle_count: scene.len(),
                world_bounds: scene.world_bounds(),
                ..BvhStats::default()
            },
        };
        emit_node(scene, &mut order, leaf_size, 0, &mut bvh)?;
        bvh.stats.node_count = bvh.nodes.len();
        debug_assert_eq!(bvh.primitive_indices.len(), scene.len());
        debug_assert_eq!(bvh.cpu_primitives.len(), scene.len());
        Ok(bvh)
    }

    /// Directly uploadable node array. Node zero is the root.
    pub fn nodes(&self) -> &[GpuBvhNode] {
        &self.nodes
    }

    /// Directly uploadable leaf array. Nodes address this array through `links[0..2]`.
    pub fn primitive_indices(&self) -> &[GpuBvhPrimitive] {
        &self.primitive_indices
    }

    pub fn stats(&self) -> BvhStats {
        self.stats
    }

    pub fn memory_cost(&self) -> BvhMemoryCost {
        let node_bytes = self.nodes.len() * size_of::<GpuBvhNode>();
        let primitive_reference_bytes = self.primitive_indices.len() * size_of::<GpuBvhPrimitive>();
        let reference_triangle_bytes = self.cpu_primitives.len() * size_of::<CpuBvhPrimitive>();
        BvhMemoryCost {
            node_bytes,
            primitive_reference_bytes,
            reference_triangle_bytes,
            total_gpu_payload_bytes: node_bytes + primitive_reference_bytes,
        }
    }

    /// Update exact triangle positions and every node bound without changing the deterministic leaf
    /// topology. The scene may be reordered, but it must contain the same source instances and
    /// primitives; visibility/topology changes require [`Self::build`] instead.
    pub fn refit(&mut self, scene: &RtSceneSurfaceData) -> Result<(), BvhBuildError> {
        if scene.len() != self.stats.triangle_count {
            return Err(BvhBuildError::RefitTriangleCountMismatch {
                expected: self.stats.triangle_count,
                actual: scene.len(),
            });
        }
        if scene.is_empty() {
            self.stats.world_bounds = None;
            return Ok(());
        }

        let by_source: BTreeMap<[u32; 4], (u32, &RtSceneTriangle)> = scene
            .triangles()
            .iter()
            .enumerate()
            .map(|(index, triangle)| {
                Ok((
                    triangle.source.source,
                    (checked_u32(index, "scene triangle index")?, triangle),
                ))
            })
            .collect::<Result<_, BvhBuildError>>()?;
        for (source, cpu) in self.primitive_indices.iter().zip(&mut self.cpu_primitives) {
            let Some(&(scene_triangle_index, triangle)) = by_source.get(&source.source) else {
                return Err(BvhBuildError::RefitMissingPrimitive(source.source));
            };
            cpu.world_positions = triangle.world_positions;
            cpu.instance_custom_data = triangle.instance_custom_data;
            cpu.scene_triangle_index = scene_triangle_index;
        }

        for node_index in (0..self.nodes.len()).rev() {
            let links = self.nodes[node_index].links;
            let bounds = if links[3] == BVH_NODE_LEAF {
                let first = links[0] as usize;
                let end = first
                    .checked_add(links[1] as usize)
                    .ok_or(BvhBuildError::CorruptBvh("leaf primitive range"))?;
                let primitives = self
                    .cpu_primitives
                    .get(first..end)
                    .ok_or(BvhBuildError::CorruptBvh("leaf primitive range"))?;
                let mut bounds = RtAabb::empty();
                for primitive in primitives {
                    bounds.include_aabb(RtAabb::from_triangle_conservative(
                        primitive.world_positions,
                    ));
                }
                bounds
            } else {
                let left = self
                    .nodes
                    .get(links[0] as usize)
                    .ok_or(BvhBuildError::CorruptBvh("left child"))?
                    .bounds();
                let right = self
                    .nodes
                    .get(links[1] as usize)
                    .ok_or(BvhBuildError::CorruptBvh("right child"))?
                    .bounds();
                let mut bounds = left;
                bounds.include_aabb(right);
                bounds
            };
            self.nodes[node_index].bounds_min = [bounds.min.x, bounds.min.y, bounds.min.z, 0.0];
            self.nodes[node_index].bounds_max = [bounds.max.x, bounds.max.y, bounds.max.z, 0.0];
        }
        self.stats.world_bounds = Some(self.nodes[0].bounds());
        Ok(())
    }

    /// Double-sided closest-hit reference traversal over the exact transformed source triangles.
    /// Bounds are evaluated in f64 after conservative f32 expansion; triangle intersection is also
    /// f64 and introduces no artistic thickness or small-triangle cutoff.
    pub fn trace_closest(&self, ray: RtRay) -> Option<RtBvhHit> {
        if self.nodes.is_empty() || !ray.is_valid() {
            return None;
        }

        let mut stack = Vec::with_capacity(self.stats.max_depth.saturating_add(1));
        stack.push(0u32);
        let mut closest_t = f64::from(ray.t_max);
        let mut closest: Option<RtBvhHit> = None;

        while let Some(node_index) = stack.pop() {
            let node = self.nodes.get(node_index as usize)?;
            if ray_aabb_interval(ray, node.bounds(), closest_t).is_none() {
                continue;
            }

            if node.is_leaf() {
                let first = node.links[0] as usize;
                let end = first.checked_add(node.links[1] as usize)?;
                for primitive_slot in first..end {
                    let cpu = self.cpu_primitives.get(primitive_slot)?;
                    let Some((t, barycentrics)) = ray_triangle(ray, cpu.world_positions) else {
                        continue;
                    };
                    let source = *self.primitive_indices.get(primitive_slot)?;
                    let replaces = t < closest_t
                        || (t == closest_t
                            && closest
                                .as_ref()
                                .is_none_or(|hit| source.source < hit.source.source));
                    if replaces {
                        closest_t = t;
                        closest = Some(RtBvhHit {
                            t,
                            barycentrics,
                            primitive_slot: primitive_slot as u32,
                            scene_triangle_index: cpu.scene_triangle_index,
                            instance_custom_data: cpu.instance_custom_data,
                            source,
                        });
                    }
                }
            } else {
                let left = node.links[0];
                let right = node.links[1];
                let left_near = self
                    .nodes
                    .get(left as usize)
                    .and_then(|child| ray_aabb_interval(ray, child.bounds(), closest_t))
                    .map(|interval| interval.0);
                let right_near = self
                    .nodes
                    .get(right as usize)
                    .and_then(|child| ray_aabb_interval(ray, child.bounds(), closest_t))
                    .map(|interval| interval.0);
                match (left_near, right_near) {
                    (Some(l), Some(r)) if l <= r => {
                        stack.push(right);
                        stack.push(left);
                    }
                    (Some(_), Some(_)) => {
                        stack.push(left);
                        stack.push(right);
                    }
                    (Some(_), None) => stack.push(left),
                    (None, Some(_)) => stack.push(right),
                    (None, None) => {}
                }
            }
        }
        closest
    }

    pub fn trace_any(&self, ray: RtRay) -> bool {
        self.trace_closest(ray).is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtRay {
    pub origin: Vec3,
    pub direction: Vec3,
    pub t_min: f32,
    pub t_max: f32,
}

impl RtRay {
    pub fn new(origin: Vec3, direction: Vec3, t_min: f32, t_max: f32) -> Self {
        Self {
            origin,
            direction,
            t_min,
            t_max,
        }
    }

    fn is_valid(self) -> bool {
        self.origin.is_finite()
            && self.direction.is_finite()
            && self.direction != Vec3::ZERO
            && self.t_min.is_finite()
            && self.t_max.is_finite()
            && self.t_min <= self.t_max
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtBvhHit {
    /// Parametric distance along the supplied (not implicitly normalized) ray.
    pub t: f64,
    /// Source triangle weights 1 and 2, ready for `SurfacePack::evaluate_hit` after f32 conversion.
    pub barycentrics: [f64; 2],
    pub primitive_slot: u32,
    pub scene_triangle_index: u32,
    pub instance_custom_data: u32,
    pub source: GpuBvhPrimitive,
}

impl RtBvhHit {
    pub fn surface_barycentrics(self) -> [f32; 2] {
        [self.barycentrics[0] as f32, self.barycentrics[1] as f32]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BvhBuildError {
    ZeroLeafSize,
    CountOverflow(&'static str),
    CorruptSurfacePack(&'static str),
    CorruptBvh(&'static str),
    UnknownPackInstance(u32),
    DuplicatePackInstance(u32),
    NonFiniteInstanceTransform(u32),
    NonInvertibleInstanceTransform(u32),
    RefitTriangleCountMismatch {
        expected: usize,
        actual: usize,
    },
    RefitMissingPrimitive([u32; 4]),
    NonFiniteWorldVertex {
        instance_custom_data: u32,
        primitive_index: u32,
        vertex: u32,
    },
}

impl fmt::Display for BvhBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLeafSize => write!(f, "BVH leaf size must be non-zero"),
            Self::CountOverflow(what) => write!(f, "{what} exceeds u32 storage"),
            Self::CorruptSurfacePack(what) => write!(f, "corrupt source surface pack: {what}"),
            Self::CorruptBvh(what) => write!(f, "corrupt triangle BVH: {what}"),
            Self::UnknownPackInstance(index) => write!(f, "unknown packed instance {index}"),
            Self::DuplicatePackInstance(index) => {
                write!(f, "packed instance {index} was supplied more than once")
            }
            Self::NonFiniteInstanceTransform(custom_data) => {
                write!(f, "instance {custom_data:#x} has a non-finite current transform")
            }
            Self::NonInvertibleInstanceTransform(custom_data) => {
                write!(f, "instance {custom_data:#x} has a non-invertible current transform")
            }
            Self::RefitTriangleCountMismatch { expected, actual } => write!(
                f,
                "BVH refit received {actual} triangles, expected the built topology's {expected}"
            ),
            Self::RefitMissingPrimitive(source) => {
                write!(f, "BVH refit is missing source primitive {source:?}")
            }
            Self::NonFiniteWorldVertex {
                instance_custom_data,
                primitive_index,
                vertex,
            } => write!(
                f,
                "instance {instance_custom_data:#x} primitive {primitive_index} vertex {vertex} transforms to a non-finite position"
            ),
        }
    }
}

impl std::error::Error for BvhBuildError {}

fn emit_node(
    scene: &RtSceneSurfaceData,
    order: &mut [u32],
    leaf_size: usize,
    depth: usize,
    bvh: &mut TriangleBvh,
) -> Result<u32, BvhBuildError> {
    let node_index = checked_u32(bvh.nodes.len(), "BVH node count")?;
    bvh.nodes.push(GpuBvhNode::zeroed());
    bvh.stats.max_depth = bvh.stats.max_depth.max(depth);

    let bounds = bounds_for_order(scene, order)?;
    if order.len() <= leaf_size {
        let first = checked_u32(bvh.primitive_indices.len(), "BVH primitive offset")?;
        let count = checked_u32(order.len(), "BVH leaf primitive count")?;
        for &scene_triangle_index in order.iter() {
            let triangle = scene.triangles.get(scene_triangle_index as usize).ok_or(
                BvhBuildError::CorruptSurfacePack("BVH scene triangle index"),
            )?;
            bvh.primitive_indices.push(triangle.source);
            bvh.cpu_primitives.push(CpuBvhPrimitive {
                world_positions: triangle.world_positions,
                instance_custom_data: triangle.instance_custom_data,
                scene_triangle_index,
            });
        }
        bvh.nodes[node_index as usize] = gpu_node(bounds, [first, count, 0, BVH_NODE_LEAF]);
        bvh.stats.leaf_count += 1;
        bvh.stats.max_leaf_occupancy = bvh.stats.max_leaf_occupancy.max(order.len());
        return Ok(node_index);
    }

    let axis = largest_centroid_axis(scene, order)?;
    order.sort_by(|a, b| {
        let ta = &scene.triangles[*a as usize];
        let tb = &scene.triangles[*b as usize];
        ta.centroid[axis]
            .total_cmp(&tb.centroid[axis])
            .then_with(|| ta.source.source.cmp(&tb.source.source))
            .then_with(|| a.cmp(b))
    });
    let middle = order.len() / 2;
    let (left_order, right_order) = order.split_at_mut(middle);
    let left = emit_node(scene, left_order, leaf_size, depth + 1, bvh)?;
    let right = emit_node(scene, right_order, leaf_size, depth + 1, bvh)?;
    bvh.nodes[node_index as usize] =
        gpu_node(bounds, [left, right, axis as u32, BVH_NODE_INTERNAL]);
    bvh.stats.internal_node_count += 1;
    Ok(node_index)
}

fn gpu_node(bounds: RtAabb, links: [u32; 4]) -> GpuBvhNode {
    GpuBvhNode {
        bounds_min: [bounds.min.x, bounds.min.y, bounds.min.z, 0.0],
        bounds_max: [bounds.max.x, bounds.max.y, bounds.max.z, 0.0],
        links,
    }
}

fn bounds_for_order(scene: &RtSceneSurfaceData, order: &[u32]) -> Result<RtAabb, BvhBuildError> {
    let mut bounds = RtAabb::empty();
    for &index in order {
        let triangle =
            scene
                .triangles
                .get(index as usize)
                .ok_or(BvhBuildError::CorruptSurfacePack(
                    "BVH scene triangle index",
                ))?;
        bounds.include_aabb(triangle.conservative_bounds);
    }
    Ok(bounds)
}

fn largest_centroid_axis(
    scene: &RtSceneSurfaceData,
    order: &[u32],
) -> Result<usize, BvhBuildError> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for &index in order {
        let centroid = scene
            .triangles
            .get(index as usize)
            .ok_or(BvhBuildError::CorruptSurfacePack(
                "BVH scene triangle index",
            ))?
            .centroid;
        min = min.min(centroid);
        max = max.max(centroid);
    }
    let extent = max - min;
    let mut axis = 0;
    if extent.y > extent.x {
        axis = 1;
    }
    if extent.z > extent[axis] {
        axis = 2;
    }
    Ok(axis)
}

fn triangle_centroid(points: [Vec3; 3]) -> Vec3 {
    // f64 avoids overflow in the split heuristic for otherwise-valid f32 world coordinates.
    let centroid = (points[0].as_dvec3() + points[1].as_dvec3() + points[2].as_dvec3()) / 3.0;
    centroid.as_vec3()
}

fn padded_mat3_cols(matrix: Mat3) -> [[f32; 4]; 3] {
    let columns = matrix.to_cols_array_2d();
    [
        [columns[0][0], columns[0][1], columns[0][2], 0.0],
        [columns[1][0], columns[1][1], columns[1][2], 0.0],
        [columns[2][0], columns[2][1], columns[2][2], 0.0],
    ]
}

fn ray_aabb_interval(ray: RtRay, bounds: RtAabb, upper_t: f64) -> Option<(f64, f64)> {
    let origin = ray.origin.as_dvec3();
    let direction = ray.direction.as_dvec3();
    let min = bounds.min.as_dvec3();
    let max = bounds.max.as_dvec3();
    let mut near = f64::from(ray.t_min);
    let mut far = f64::from(ray.t_max).min(upper_t);

    for axis in 0..3 {
        let d = direction[axis];
        if d == 0.0 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let inv = d.recip();
        let mut a = (min[axis] - origin[axis]) * inv;
        let mut b = (max[axis] - origin[axis]) * inv;
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        near = near.max(a);
        far = far.min(b);
        if near > far {
            return None;
        }
    }
    Some((near, far))
}

fn ray_triangle(ray: RtRay, points: [Vec3; 3]) -> Option<(f64, [f64; 2])> {
    let origin = ray.origin.as_dvec3();
    let direction = ray.direction.as_dvec3();
    let p0 = points[0].as_dvec3();
    let edge1 = points[1].as_dvec3() - p0;
    let edge2 = points[2].as_dvec3() - p0;
    let p = direction.cross(edge2);
    let determinant = edge1.dot(p);
    if determinant == 0.0 {
        return None;
    }
    let inverse_determinant = determinant.recip();
    let translated = origin - p0;
    let u = translated.dot(p) * inverse_determinant;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = translated.cross(edge1);
    let v = direction.dot(q) * inverse_determinant;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = edge2.dot(q) * inverse_determinant;
    if t < f64::from(ray.t_min) || t > f64::from(ray.t_max) {
        return None;
    }
    Some((t, [u, v]))
}

fn checked_u32(value: usize, what: &'static str) -> Result<u32, BvhBuildError> {
    u32::try_from(value).map_err(|_| BvhBuildError::CountOverflow(what))
}

fn next_float_up(value: f32) -> f32 {
    if value.is_nan() || value == f32::INFINITY {
        return value;
    }
    if value == 0.0 {
        return f32::from_bits(1);
    }
    let bits = value.to_bits();
    f32::from_bits(if value > 0.0 { bits + 1 } else { bits - 1 })
}

fn next_float_down(value: f32) -> f32 {
    if value.is_nan() || value == f32::NEG_INFINITY {
        return value;
    }
    if value == 0.0 {
        return f32::from_bits(0x8000_0001);
    }
    let bits = value.to_bits();
    f32::from_bits(if value > 0.0 { bits - 1 } else { bits + 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf_load::{CpuMesh, Vertex};
    use crate::rt_surface::{MaterialLod, SurfacePackBuilder};

    fn vertex(position: [f32; 3]) -> Vertex {
        Vertex {
            pos: position,
            normal: [0.0, 0.0, 1.0],
            uv: [position[0], position[1]],
            tangent: [1.0, 0.0, 0.0, 1.0],
        }
    }

    fn quad_mesh() -> CpuMesh {
        CpuMesh::basic(
            vec![
                vertex([-1.0, -1.0, 0.0]),
                vertex([1.0, -1.0, 0.0]),
                vertex([1.0, 1.0, 0.0]),
                vertex([-1.0, 1.0, 0.0]),
            ],
            vec![0, 1, 2, 0, 2, 3],
            None,
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        )
    }

    fn scene_with_instances(transforms: &[(u32, Mat4)]) -> (SurfacePack, RtSceneSurfaceData) {
        let mut builder = SurfacePackBuilder::new(32).unwrap();
        let surface = builder.add_mesh(&quad_mesh()).unwrap();
        for &(custom_data, transform) in transforms {
            builder
                .add_instance(custom_data, surface, transform)
                .unwrap();
        }
        let pack = builder.finish().unwrap();
        let scene = RtSceneSurfaceData::from_surface_pack(&pack).unwrap();
        (pack, scene)
    }

    #[test]
    fn gpu_layouts_are_storage_buffer_safe() {
        assert_eq!(size_of::<GpuBvhNode>(), 48);
        assert_eq!(size_of::<GpuBvhPrimitive>(), 16);
        assert_eq!(std::mem::align_of::<GpuBvhNode>(), 4);
        assert_eq!(std::mem::align_of::<GpuBvhPrimitive>(), 4);
    }

    #[test]
    fn scene_triangles_preserve_source_order_and_exact_instance_transform() {
        let transform = Mat4::from_translation(Vec3::new(4.0, -2.0, 7.0))
            * Mat4::from_scale(Vec3::new(-2.0, 3.0, 0.5));
        let (pack, scene) = scene_with_instances(&[(0x1234, transform)]);
        assert_eq!(scene.len(), 2);

        for (primitive_index, triangle) in scene.triangles().iter().enumerate() {
            assert_eq!(triangle.source.source[0], 0);
            assert_eq!(triangle.source.source[1], primitive_index as u32);
            assert_eq!(triangle.source.source[2], 0);
            assert_eq!(triangle.source.source[3], primitive_index as u32);
            assert_eq!(triangle.instance_custom_data, 0x1234);

            let source = pack.triangles()[primitive_index];
            for lane in 0..3 {
                let packed = pack.vertices()[source.indices[lane] as usize];
                let local = Vec3::from_slice(&packed.position[..3]);
                let expected = transform.transform_point3(local);
                assert_eq!(triangle.world_positions[lane], expected);
                assert!(triangle.conservative_bounds.contains_point(expected));
            }
        }
    }

    #[test]
    fn triangle_bounds_expand_exact_extrema_by_one_float_step() {
        let points = [
            Vec3::new(-3.0, 0.0, 8.0),
            Vec3::new(2.0, 4.0, 9.0),
            Vec3::new(1.0, -5.0, 7.0),
        ];
        let bounds = RtAabb::from_triangle_conservative(points);
        assert_eq!(bounds.min.x, next_float_down(-3.0));
        assert_eq!(bounds.min.y, next_float_down(-5.0));
        assert_eq!(bounds.min.z, next_float_down(7.0));
        assert_eq!(bounds.max.x, next_float_up(2.0));
        assert_eq!(bounds.max.y, next_float_up(4.0));
        assert_eq!(bounds.max.z, next_float_up(9.0));
        for point in points {
            assert!(bounds.contains_point(point));
        }
    }

    #[test]
    fn build_is_deterministic_and_every_node_is_conservative() {
        let (_, scene) = scene_with_instances(&[
            (20, Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0))),
            (10, Mat4::from_translation(Vec3::new(4.0, 0.0, 5.0))),
            (30, Mat4::from_translation(Vec3::new(-3.0, 1.0, 8.0))),
        ]);
        let a = TriangleBvh::build_with_leaf_size(&scene, 1).unwrap();
        let b = TriangleBvh::build_with_leaf_size(&scene, 1).unwrap();
        assert_eq!(a.nodes(), b.nodes());
        assert_eq!(a.primitive_indices(), b.primitive_indices());
        assert_eq!(a.stats(), b.stats());
        assert_eq!(a.stats().triangle_count, 6);
        assert_eq!(a.stats().leaf_count, 6);
        assert_eq!(a.stats().internal_node_count, 5);
        assert_eq!(a.nodes().len(), 11);

        let root = a.nodes()[0].bounds();
        for triangle in scene.triangles() {
            assert!(root.contains_aabb(triangle.conservative_bounds));
        }
        for node in a.nodes() {
            if !node.is_leaf() {
                assert!(node
                    .bounds()
                    .contains_aabb(a.nodes()[node.links[0] as usize].bounds()));
                assert!(node
                    .bounds()
                    .contains_aabb(a.nodes()[node.links[1] as usize].bounds()));
            } else {
                assert!(node.links[1] <= 1);
                assert!((node.links[0] + node.links[1]) as usize <= a.primitive_indices().len());
            }
        }
    }

    #[test]
    fn active_scene_uses_current_models_and_excludes_hidden_instances() {
        let (pack, _) = scene_with_instances(&[
            (0x10, Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0))),
            (0x20, Mat4::from_translation(Vec3::new(0.0, 0.0, 4.0))),
        ]);
        let scene = RtSceneSurfaceData::from_active_instances(
            &pack,
            &[RtSceneInstance {
                pack_instance_index: 1,
                model: Mat4::from_translation(Vec3::new(0.0, 0.0, 11.0)),
            }],
        )
        .unwrap();
        assert_eq!(scene.len(), 2);
        assert!(scene
            .triangles()
            .iter()
            .all(|triangle| triangle.instance_custom_data == 0x20));
        assert_eq!(scene.gpu_instance_bytes(), pack.instances().len() * 128);
        let current = scene.gpu_instances()[1];
        assert_eq!(current.ids, pack.instances()[1].ids);
        assert_eq!(
            Mat4::from_cols_array_2d(&current.model),
            Mat4::from_translation(Vec3::new(0.0, 0.0, 11.0))
        );
        let bvh = TriangleBvh::build(&scene).unwrap();
        let hit = bvh
            .trace_closest(RtRay::new(Vec3::ZERO, Vec3::Z, 0.0, 20.0))
            .unwrap();
        assert_eq!(hit.instance_custom_data, 0x20);
        assert!((hit.t - 11.0).abs() < 1.0e-12);
        let evaluated = pack
            .evaluate_hit_with_instance_record(
                hit.source.source[0],
                &scene.gpu_instances()[hit.source.source[0] as usize],
                hit.source.source[1],
                hit.surface_barycentrics(),
                MaterialLod::splat(0.0),
            )
            .unwrap();
        assert!((evaluated.world_position - Vec3::new(0.0, 0.0, 11.0)).length() < 1.0e-6);
    }

    #[test]
    fn refit_preserves_source_topology_and_updates_exact_bounds_and_hits() {
        let (pack, scene) = scene_with_instances(&[
            (0x10, Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0))),
            (0x20, Mat4::from_translation(Vec3::new(0.0, 0.0, 4.0))),
        ]);
        let mut bvh = TriangleBvh::build_with_leaf_size(&scene, 1).unwrap();
        let original_nodes = bvh
            .nodes()
            .iter()
            .map(|node| node.links)
            .collect::<Vec<_>>();
        let moved = RtSceneSurfaceData::from_active_instances(
            &pack,
            &[
                RtSceneInstance {
                    pack_instance_index: 1,
                    model: Mat4::from_translation(Vec3::new(0.0, 0.0, 12.0)),
                },
                RtSceneInstance {
                    pack_instance_index: 0,
                    model: Mat4::from_translation(Vec3::new(0.0, 0.0, 8.0)),
                },
            ],
        )
        .unwrap();
        bvh.refit(&moved).unwrap();
        assert_eq!(
            bvh.nodes()
                .iter()
                .map(|node| node.links)
                .collect::<Vec<_>>(),
            original_nodes
        );
        let hit = bvh
            .trace_closest(RtRay::new(Vec3::ZERO, Vec3::Z, 0.0, 20.0))
            .unwrap();
        assert_eq!(hit.instance_custom_data, 0x10);
        assert!((hit.t - 8.0).abs() < 1.0e-12);
        assert!(bvh
            .stats()
            .world_bounds
            .unwrap()
            .contains_aabb(moved.world_bounds().unwrap()));
    }

    #[test]
    fn closest_hit_maps_back_to_surface_evaluator() {
        let (pack, scene) = scene_with_instances(&[
            (0x22, Mat4::from_translation(Vec3::new(0.0, 0.0, 5.0))),
            (0x11, Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0))),
        ]);
        let bvh = TriangleBvh::build_with_leaf_size(&scene, 1).unwrap();
        let hit = bvh
            .trace_closest(RtRay::new(Vec3::ZERO, Vec3::Z, 0.0, 100.0))
            .unwrap();
        assert_eq!(hit.instance_custom_data, 0x11);
        assert_eq!(hit.source.source[1], 0);
        assert!((hit.t - 2.0).abs() < 1.0e-12);
        assert!((hit.barycentrics[0] - 0.0).abs() < 1.0e-12);
        assert!((hit.barycentrics[1] - 0.5).abs() < 1.0e-12);

        let evaluated = pack
            .evaluate_hit(
                hit.instance_custom_data,
                hit.source.source[1],
                hit.surface_barycentrics(),
                MaterialLod::splat(0.0),
            )
            .unwrap();
        assert!((evaluated.world_position - Vec3::new(0.0, 0.0, 2.0)).length() < 1.0e-6);
    }

    #[test]
    fn bvh_matches_brute_force_for_a_ray_grid() {
        let (_, scene) = scene_with_instances(&[
            (1, Mat4::from_translation(Vec3::new(-2.0, 0.0, 3.0))),
            (2, Mat4::from_translation(Vec3::new(0.0, 0.0, 6.0))),
            (3, Mat4::from_translation(Vec3::new(2.0, 0.0, 9.0))),
        ]);
        let bvh = TriangleBvh::build_with_leaf_size(&scene, 2).unwrap();
        for yi in -8..=8 {
            for xi in -16..=16 {
                let ray = RtRay::new(
                    Vec3::new(xi as f32 * 0.2, yi as f32 * 0.2, 0.0),
                    Vec3::Z,
                    0.0,
                    20.0,
                );
                let actual = bvh.trace_closest(ray);
                let mut expected: Option<(f64, [u32; 4], u32)> = None;
                for triangle in scene.triangles() {
                    if let Some((t, _)) = ray_triangle(ray, triangle.world_positions) {
                        let key = triangle.source.source;
                        let replace = expected.is_none_or(|(best_t, best_key, _)| {
                            t < best_t || (t == best_t && key < best_key)
                        });
                        if replace {
                            expected = Some((t, key, triangle.instance_custom_data));
                        }
                    }
                }
                assert_eq!(
                    actual.map(|hit| (hit.t, hit.source.source, hit.instance_custom_data)),
                    expected
                );
            }
        }
    }

    #[test]
    fn empty_scene_and_invalid_leaf_size_are_explicit() {
        let builder = SurfacePackBuilder::new(16).unwrap();
        let pack = builder.finish().unwrap();
        let scene = RtSceneSurfaceData::from_surface_pack(&pack).unwrap();
        let empty = TriangleBvh::build(&scene).unwrap();
        assert!(empty.nodes().is_empty());
        assert!(empty.primitive_indices().is_empty());
        assert_eq!(empty.stats(), BvhStats::default());
        assert!(!empty.trace_any(RtRay::new(Vec3::ZERO, Vec3::Z, 0.0, 1.0)));
        assert_eq!(
            TriangleBvh::build_with_leaf_size(&scene, 0).unwrap_err(),
            BvhBuildError::ZeroLeafSize
        );
    }

    #[test]
    fn memory_accounting_separates_upload_from_cpu_reference_data() {
        let (_, scene) =
            scene_with_instances(&[(1, Mat4::IDENTITY), (2, Mat4::from_translation(Vec3::Z))]);
        let bvh = TriangleBvh::build_with_leaf_size(&scene, 1).unwrap();
        let memory = bvh.memory_cost();
        assert_eq!(memory.node_bytes, bvh.nodes().len() * 48);
        assert_eq!(memory.primitive_reference_bytes, scene.len() * 16);
        assert_eq!(
            memory.total_gpu_payload_bytes,
            memory.node_bytes + memory.primitive_reference_bytes
        );
        assert_eq!(
            memory.reference_triangle_bytes,
            scene.len() * size_of::<CpuBvhPrimitive>()
        );
    }
}
