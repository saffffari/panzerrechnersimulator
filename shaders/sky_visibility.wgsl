// Near-field sky visibility atlas (src/sky_visibility.rs). Each environment direction is an
// orthographic depth view of the real scene geometry around the camera, rasterized into its own
// tile of one depth atlas. The stylized mesh pass compares receivers against these tiles to know
// which parts of the sky and ground hemispheres actually reach them.

struct SkyVisibility {
    view_proj:array<mat4x4<f32>,66>,
    directions:array<vec4<f32>,32>,
    grid:vec4<f32>,
    global_grid:vec4<f32>, // coarse world texel, near depth span, global depth span, reserved
    sun_direction:vec4<f32>,
};
@group(0) @binding(0) var<uniform> sky: SkyVisibility;

struct Push {
    model: mat4x4<f32>,
    params: vec4<f32>, // x = direction index for instance-rate batches
};
var<push_constant> pc: Push;

struct TileOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
};

// Orthographic views have w == 1, so the tile placement is an exact affine map of clip x/y.
fn tile(world: vec3<f32>, k: u32) -> TileOut {
    let p = sky.view_proj[k] * vec4(world, 1.0);
    let cols = sky.grid.x;
    let rows = sky.grid.y;
    let col = f32(k % u32(cols));
    let row = f32(k / u32(cols));
    var out: TileOut;
    out.clip = vec4(-1.0 + (2.0 * col + p.x + 1.0) / cols,
                    1.0 - (2.0 * row + 1.0 - p.y) / rows,
                    p.z, 1.0);
    out.local = p.xy;
    return out;
}

@vertex
fn vs(@location(0) pos: vec3<f32>, @builtin(instance_index) k: u32) -> TileOut {
    return tile((pc.model * vec4(pos, 1.0)).xyz, k);
}

// Instanced static modules keep their per-placement matrix on the instance rate; the direction
// rides the push constant. The affine rows match stylized_mesh.wgsl's vs_inst.
@vertex
fn vs_inst(@location(0) pos: vec3<f32>,
           @location(4) i0: vec4<f32>, @location(5) i1: vec4<f32>,
           @location(6) i2: vec4<f32>, @location(7) i3: vec4<f32>) -> TileOut {
    let model = mat4x4<f32>(vec4(i0.xyz, 0.0), vec4(i1.xyz, 0.0),
                            vec4(i2.xyz, 0.0), vec4(i3.xyz, 1.0));
    return tile((model * vec4(pos, 1.0)).xyz, u32(pc.params.x));
}

// A triangle may straddle its tile; the part outside belongs to no direction's footprint.
@fragment
fn fs(in: TileOut) {
    if (any(abs(in.local) > vec2(1.0))) { discard; }
}
