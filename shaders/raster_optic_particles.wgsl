// @shared-stylized-particle-functions
// The main particle pass runs after camera response. Optic tiles remain HDR so their source
// snow/dust colours need its inverse response here, then receive that response exactly once
// when the entrance image is displayed. Particle positions, depth and coverage stay shared.
fn particle_inverse_aces(color: vec3<f32>) -> vec3<f32> {
    let positive=max(color,vec3(0.0));
    let y=clamp(max(positive.r,max(positive.g,positive.b)),0.0,0.999);
    let a=2.51-2.43*y;let b=0.03-0.59*y;
    var peak=max((-b+sqrt(b*b+4.0*a*0.14*y))/(2.0*a),0.0);
    if(y>0.673290473){
        let ratio=(y-0.673290473)/0.326709527;
        peak=0.6+(exp(min(2.0*ratio/max(1.0-ratio,1e-5),60.0))-1.0)/3.047601074;
    }
    return positive*(peak/max(y,1e-12));
}
@fragment
fn fs_hdr(in: VsOut) -> @location(0) vec4<f32> {
    let display=particle_display_color(in);
    return vec4(particle_inverse_aces(display.rgb)/max(eye.exposure,0.001),display.a);
}
