// @shared-stylized-particle-functions
// The main particle pass runs after camera response. Optic tiles remain HDR so their source
// snow/dust colours need its inverse response here, then receive that response exactly once
// when the entrance image is displayed. Particle positions, depth and coverage stay shared.
fn particle_inverse_aces(color: vec3<f32>) -> vec3<f32> {
    let y=clamp(color,vec3(0.0),vec3(0.999));
    let a=vec3(2.51)-2.43*y;
    let b=vec3(0.03)-0.59*y;
    return max((-b+sqrt(b*b+4.0*a*0.14*y))/(2.0*a),vec3(0.0));
}
@fragment
fn fs_hdr(in: VsOut) -> @location(0) vec4<f32> {
    let display=particle_display_color(in);
    return vec4(particle_inverse_aces(display.rgb)/max(eye.exposure,0.001),display.a);
}
