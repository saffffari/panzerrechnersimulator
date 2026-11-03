// Raymarch backend without ray queries: the screen-space terms stand alone (see rm_rt.wgsl).
const RM_RAY_QUERIES: bool = false;
fn rm_ray_blocked(origin: vec3<f32>, dir: vec3<f32>, tmin: f32, tmax: f32) -> f32 { return -1.0; }
fn rm_ray_nearest(origin: vec3<f32>, dir: vec3<f32>, tmin: f32, tmax: f32) -> f32 { return -1.0; }
fn rm_ray_open(origin: vec3<f32>, dir: vec3<f32>, range: f32) -> bool { return true; }
fn rm_ray_open_sealed(origin: vec3<f32>, dir: vec3<f32>, range: f32) -> bool { return true; }
fn rm_ray_escape(origin: vec3<f32>, dir: vec3<f32>, range: f32) -> vec4<f32> { return vec4(dir, 1.0); }
// Without ray queries the beam into the compartment is the cascades' (rm_cabin_beam_traced).
struct BeamPath { throughput: vec3<f32>, toward: vec3<f32>, exit: vec3<f32>, open: bool };
fn rm_vehicle_beam(origin: vec3<f32>, sun: vec3<f32>) -> BeamPath { return BeamPath(vec3(1.0), sun, origin, true); }
