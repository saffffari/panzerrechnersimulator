// Raymarch backend without ray queries: the screen-space terms stand alone (see rm_rt.wgsl).
const RM_RAY_QUERIES: bool = false;
fn rm_ray_blocked(origin: vec3<f32>, dir: vec3<f32>, tmin: f32, tmax: f32) -> f32 { return -1.0; }
fn rm_ray_nearest(origin: vec3<f32>, dir: vec3<f32>, tmin: f32, tmax: f32) -> f32 { return -1.0; }
fn rm_ray_open(origin: vec3<f32>, dir: vec3<f32>, range: f32) -> bool { return true; }
fn rm_ray_open_sealed(origin: vec3<f32>, dir: vec3<f32>, range: f32) -> bool { return true; }
fn rm_ray_escape(origin: vec3<f32>, dir: vec3<f32>, range: f32) -> vec4<f32> { return vec4(dir, 1.0); }
