// Every admitted point source has one geometry visibility map; matches the packed 32-source ABI.
// 512 m is an explicit raster transport bound. Missing/out-of-range visibility is unresolved/dark,
// never an unshadowed fallback. The actual armor/terrain/apertures are the casters.
const CABIN_SHADOW_LIGHTS: i32 = 32;
struct CabinShadows { lamps: array<vec4<f32>,32>, projections:array<vec4<f32>,32>, torch_position: vec4<f32>, torch_direction: vec4<f32>, range: vec4<f32> };
// Angular emission of the handheld reflector. This multiplies the same emitter in surface
// and participating-medium transport; it is not a screen-space brightness patch.
fn hand_torch_gain(source: vec3<f32>, point: vec3<f32>) -> f32 {
    if (cabin_shadows.torch_position.w < 0.5 || distance(source,cabin_shadows.torch_position.xyz)>0.0001) {return 1.0;}
    let ray = normalize(point-source);
    return smoothstep(cabin_shadows.torch_direction.w,cos(radians(16.0)),dot(ray,cabin_shadows.torch_direction.xyz));
}
fn cabin_fixture_index(source: vec3<f32>) -> i32 {
    for (var lamp = 0; lamp < CABIN_SHADOW_LIGHTS; lamp += 1) {
        let fixture = cabin_shadows.lamps[lamp];
        if (fixture.w >= 0.5 && distance(source,fixture.xyz) <= 0.0001) { return lamp; }
    }
    return -1;
}
fn cabin_shadow_range(lamp: i32) -> vec2<f32> {
    return cabin_shadows.projections[lamp].xy;
}
// A registered cabin fixture has a declared transport bound (CABIN_FIXTURE_REACH_M). Beyond it
// the lamp contributes exactly nothing; that is a bounded source, not unresolved visibility.
fn cabin_shadow_reach_limited(lamp: i32) -> bool {
    return cabin_shadows.projections[lamp].z > 0.5;
}
fn cabin_shadow_reference(major: f32, bias_m: f32, lamp: i32) -> f32 {
    let limits = cabin_shadow_range(lamp);
    let near = limits.x;
    let far = limits.y;
    return clamp((major-bias_m)/far,0.0,1.0);
}
// Local raster media use the same cube bank/projection at their optical-depth centroid. This is
// one bounded visibility estimate, not a per-march-step or finite-area emitter transport solver.
fn cabin_volume_light_shadow(source:vec3<f32>,radius:f32,point:vec3<f32>)->vec3<f32> {
    return hand_torch_gain(source,point)*scene_point_transmission(source,radius,point);
}
