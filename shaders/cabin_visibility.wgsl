// Eight stable fixtures plus eight admitted wreck fires, exactly src/cabin_shadows.rs::Uniform.
// Source geometry is captured within 8 m into 512² cube faces. Other positioned sources retain
// their existing unshadowed path; this is bounded raster visibility, not global ray transport.
const CABIN_SHADOW_LIGHTS: i32 = 17;
struct CabinShadows { lamps: array<vec4<f32>,17>, torch_position: vec4<f32>, torch_direction: vec4<f32>, range: vec4<f32> };
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
fn cabin_shadow_reference(major: f32, bias_m: f32) -> f32 {
    let near = cabin_shadows.range.x;
    let far = cabin_shadows.range.y;
    return min(far/(far-near) - far*near/((far-near)*max(major-bias_m,near)), 0.999999);
}
// Local raster media use the same cube bank/projection at their optical-depth centroid. This is
// one bounded visibility estimate, not a per-march-step or finite-area emitter transport solver.
fn cabin_volume_light_shadow(source: vec3<f32>, point: vec3<f32>) -> f32 {
    let lamp = cabin_fixture_index(source);
    let gain = hand_torch_gain(source,point);
    if (lamp < 0) { return gain; }
    let offset = point - source;
    let major = max(max(abs(offset.x),abs(offset.y)),abs(offset.z));
    if (major <= cabin_shadows.range.x) { return gain; }
    // Volume has no receiver surface/self-shadow. Keep only a sub-millimetre numerical bias.
    return gain * textureSampleCompareLevel(cabin_shadow_maps, shadow_samp, offset, lamp,
        cabin_shadow_reference(major, 0.0007));
}
