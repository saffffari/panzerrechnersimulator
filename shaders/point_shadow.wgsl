// Exact source geometry, no snap/cull or material-class exclusion. Perspective rasterization
// finds the visible source surface; linear face distance avoids losing cabin steel at long range.
struct Camera { view_proj:mat4x4<f32>, light_view_proj:mat4x4<f32>, cam_pos:vec3<f32>, far:f32 };
@group(0) @binding(0) var<uniform> cam:Camera;
struct Push {model:mat4x4<f32>, params:vec4<f32>};
var<push_constant> pc:Push;
struct Out { @builtin(position) clip:vec4<f32>, @location(0) face_distance:f32 };
fn project(model:mat4x4<f32>,position:vec3<f32>)->Out {
    // Subtract translations before adding the small source vertex. The cube projection is
    // centred at zero and cam_pos is this lamp, not the viewing eye. Forming absolute world
    // vertices first loses nearby steel detail as the vehicle moves across the map.
    let linear=mat3x3<f32>(model[0].xyz,model[1].xyz,model[2].xyz);
    let relative=linear*position+(model[3].xyz-cam.cam_pos);
    var out:Out;out.clip=cam.light_view_proj*vec4(relative,1.0);out.face_distance=out.clip.w;return out;
}
@vertex fn vs(@location(0) pos:vec3<f32>)->Out {return project(pc.model,pos);}
@vertex fn vs_inst(@location(0) pos:vec3<f32>,@location(4) a:vec4<f32>,@location(5) b:vec4<f32>,
    @location(6) c:vec4<f32>,@location(7) d:vec4<f32>)->Out {
    return project(mat4x4<f32>(a,b,c,d),pos);
}
@fragment fn fs(in:Out)->@builtin(frag_depth) f32 {return clamp(in.face_distance/cam.far,0.0,1.0);}
