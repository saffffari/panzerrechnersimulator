// Raster unit-relay image, sampled on the exact authored entrance face. Every port has its
// own exterior objective camera; the capture is linear HDR and receives the main tone map once.
struct Panel {
    main_view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    source_direction_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    source_eye: vec4<f32>,
    layer: vec4<u32>,
    pupil_eye_field: vec4<f32>, // source-local actual eye, tan(apparent half field)
    pupil_center_radius: vec4<f32>, // source-local lens center, exit pupil radius (0: vision block)
    pupil_axis_relief: vec4<f32>, // source-local axis toward eye, eye relief
};
@group(0) @binding(0) var<uniform> panel: Panel;
@group(0) @binding(1) var exterior: texture_2d_array<f32>;
@group(0) @binding(2) var nearest_sampler: sampler;
struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) source: vec4<f32>,
    @location(1) local: vec3<f32>,
};
@vertex fn vs(@location(0) position: vec3<f32>) -> Out {
    let relative = (panel.model * vec4(position, 1.0)).xyz - panel.eye.xyz;
    var out: Out;
    out.position = panel.main_view_proj * vec4(relative, 1.0);
    // Source-verified cupola unit relays use the actual observer pupil and preserve direction.
    // Other ports retain their registered virtual pupil and entry-to-objective rotation.
    // CPU capture fitting and this pane projection use the same declared mapping.
    let source_ray = (panel.model * vec4(position, 1.0)).xyz - panel.source_eye.xyz;
    out.source = panel.source_direction_view_proj * vec4(source_ray, 0.0);
    out.local = position;
    return out;
}
// Oriented triangle/disk intersection, integrated along each polygon edge. This preserves
// the area of a subpixel pupil instead of replacing it with a half-lit fwidth edge.
fn pupil_cross(a: vec2<f32>, b: vec2<f32>) -> f32 { return a.x*b.y-a.y*b.x; }
fn pupil_edge_piece(a: vec2<f32>, b: vec2<f32>) -> f32 {
    let middle=(a+b)*0.5;
    if (dot(middle,middle)<=1.0) { return pupil_cross(a,b)*0.5; }
    return atan2(pupil_cross(a,b),dot(a,b))*0.5;
}
fn pupil_edge_area(a: vec2<f32>, b: vec2<f32>) -> f32 {
    let d=b-a;let aa=dot(d,d);
    if (aa<1.0e-16) { return 0.0; }
    let bb=dot(a,d);let disc=bb*bb-aa*(dot(a,a)-1.0);
    if (disc<=0.0) { return pupil_edge_piece(a,b); }
    let root=sqrt(disc);
    let t0=clamp((-bb-root)/aa,0.0,1.0);
    let t1=clamp((-bb+root)/aa,0.0,1.0);
    let q0=a+d*t0;let q1=a+d*t1;
    return pupil_edge_piece(a,q0)+pupil_edge_piece(q0,q1)+pupil_edge_piece(q1,b);
}
fn pupil_circle_coverage(point: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>,
    center: vec2<f32>, radius: f32) -> f32 {
    let p=point-center;
    let reach=max(length(dx+dy),length(dx-dy))*0.5;
    if (length(p)+reach<=radius) { return 1.0; }
    if (length(p)-reach>=radius) { return 0.0; }
    let area=abs(pupil_cross(dx,dy));
    if (area<1.0e-16) { return select(0.0,1.0,length(p)<=radius); }
    // Unit-disk coordinates avoid cancellation at the millimetre scale of the actual stop.
    let a=(p-(dx+dy)*0.5)/radius;let b=(p+(dx-dy)*0.5)/radius;
    let c=(p+(dx+dy)*0.5)/radius;let d=(p+(-dx+dy)*0.5)/radius;
    let overlap=abs(pupil_edge_area(a,b)+pupil_edge_area(b,c)+
        pupil_edge_area(c,d)+pupil_edge_area(d,a));
    return clamp(overlap*radius*radius/area,0.0,1.0);
}
// The registered source entrance is a planar disk. Project its pixel footprint into that
// plane, then intersect both physical stops. Pixel derivatives only discretize coverage;
// they never widen the actual pupil or apply a second radiance/area loss.
fn pupil_transmission(in: Out) -> f32 {
    let dx3=dpdx(in.local);let dy3=dpdy(in.local);
    if (panel.pupil_center_radius.w<=0.0) { return 1.0; }
    let n=panel.pupil_axis_relief.xyz;
    let basis=select(vec3(1.0,0.0,0.0),vec3(0.0,1.0,0.0),abs(n.x)>0.8);
    let u=normalize(cross(n,basis));let v=cross(n,u);
    let point3=in.local-panel.pupil_center_radius.xyz;
    let eye3=panel.pupil_eye_field.xyz-panel.pupil_center_radius.xyz;
    let axial=dot(eye3,n);
    if (axial<=0.0) { return 0.0; }
    let p=vec2(dot(point3,u),dot(point3,v));let eye=vec2(dot(eye3,u),dot(eye3,v));
    let dx=vec2(dot(dx3,u),dot(dx3,v));let dy=vec2(dot(dy3,u),dot(dy3,v));
    let field_radius=axial*panel.pupil_eye_field.w;
    let field=pupil_circle_coverage(p,dx,dy,eye,field_radius);
    if (field<=0.0) { return 0.0; }
    let t=panel.pupil_axis_relief.w/axial;let scale=1.0-t;
    // Exactly at eye relief all rays meet the same point in the exit-pupil plane.
    // Treat only numerical degeneracy here; an ordinary near-pupil eye still gets the disks.
    if (abs(scale)<1.0e-7) {
        return select(0.0,field,length(eye)<=panel.pupil_center_radius.w);
    }
    let center=-eye*t/scale;let radius=panel.pupil_center_radius.w/abs(scale);
    let pupil=pupil_circle_coverage(p,dx,dy,center,radius);
    if (field>=1.0 || pupil<=0.0) { return pupil; }
    if (pupil>=1.0) { return field; }
    // Only pixels crossed by BOTH stop edges require joint integration. Multiplying two
    // independent coverages would lose radiance in their overlap. Bounded 8x8 quadrature
    // resolves that rare intersection; isolated/subpixel pupil areas use the integral above.
    var admitted=0.0;
    for (var y=0u;y<8u;y+=1u) { for (var x=0u;x<8u;x+=1u) {
        let q=p+dx*((f32(x)+0.5)/8.0-0.5)+dy*((f32(y)+0.5)/8.0-0.5);
        if (distance(q,eye)<=field_radius && distance(q,center)<=radius) { admitted+=1.0; }
    }}
    return admitted/64.0;
}
struct PaneOutput { @location(0) color: vec4<f32>, @location(1) glow: vec4<f32> };
@fragment fn fs(in: Out) -> PaneOutput {
    // 2 identifies a visible exterior relay; 1 remains the ordinary lamp-lens glow mask.
    // Both are depth-tested on the real pane, so a hidden/occluded optic cannot seed bloom.
    return PaneOutput(vec4(relayed(in) * panel.eye.w * pupil_transmission(in), 1.0), vec4(0.0, 0.0, 0.0, 2.0));
}
// The raymarch backend uses the same per-fragment finite pupil, after source depth testing.
// A rejected ray leaves opaque dark glass; it does not discard the physical lens.
@fragment fn fs_relay(in: Out) -> @location(0) vec4<f32> { return vec4(relayed(in) * panel.eye.w * pupil_transmission(in), 1.0); }
fn relayed(in: Out) -> vec3<f32> {
    // A vision-block direction outside the captured field takes the nearest direction instead
    // of leaving a black hole in a daylit block (see OFF_AXIS_ACCEPTANCE in raster_optics.rs).
    // A direction behind the capture camera takes the field centre. This remains named vision-
    // block debt. Finite eyepieces fit/refresh their capture to enclose every visible source ray.
    var uv = vec2(0.5);
    if (in.source.w > 0.0) {
        uv = clamp(in.source.xy / in.source.w * vec2(0.5, -0.5) + vec2(0.5), vec2(0.0), vec2(1.0));
    }
    // Only the active upper-left viewport is populated. Clamp at its last texel center so the
    // nearest sample cannot read the cleared, unused remainder when uv reaches exactly one.
    let extent=vec2<f32>(panel.layer.zw);let allocated=vec2<f32>(textureDimensions(exterior));
    let sample_uv=clamp(uv*extent,vec2(0.5),extent-vec2(0.5))/allocated;
    return textureSampleLevel(exterior, nearest_sampler, sample_uv, i32(panel.layer.x), 0.0).rgb;
}
