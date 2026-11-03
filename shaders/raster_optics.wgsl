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
    relay_eye: vec4<f32>, // source-local actual eye (vision blocks with a registered stop chain)
    stops: array<RelayStop, 12>, // layer.y of them, in crossing order (raster_optics.rs RelayStop)
};
struct RelayStop {
    c0: vec4<f32>, // corner, w = kind (0 inner pane face, 1 window face, 2 mirror, 3 thin lens)
    c1: vec4<f32>, // corner, w = focal length (lens)
    c2: vec4<f32>,
    c3: vec4<f32>,
    normal: vec4<f32>,
    center: vec4<f32>, // lens optical centre
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
    let radiance = optic_radiance(in, dpdx(in.local), dpdy(in.local), pupil_transmission(in));
    return PaneOutput(vec4(radiance * panel.eye.w, 1.0), vec4(0.0, 0.0, 0.0, 2.0));
}
// The raymarch backend uses the same per-fragment finite pupil, after source depth testing.
// A rejected ray leaves opaque dark glass; it does not discard the physical lens.
@fragment fn fs_relay(in: Out) -> @location(0) vec4<f32> {
    let radiance = optic_radiance(in, dpdx(in.local), dpdy(in.local), pupil_transmission(in));
    return vec4(radiance * panel.eye.w, 1.0);
}

struct RelayResult { valid: bool, direction: vec3<f32>, throughput: vec3<f32> };
const RELAY_IOR = 1.52;
const RELAY_ABSORPTION_PER_M = vec3(0.080, 0.035, 0.020);
fn relay_fresnel(cos_incident: f32, eta_i: f32, eta_t: f32) -> f32 {
    let ci = clamp(abs(cos_incident), 0.0, 1.0);
    let eta = eta_i / eta_t;
    let st2 = eta * eta * max(1.0 - ci * ci, 0.0);
    if (st2 >= 1.0) { return 1.0; }
    let ct = sqrt(max(1.0 - st2, 0.0));
    let rs = (eta_i * ci - eta_t * ct) / max(abs(eta_i * ci + eta_t * ct), 1e-6);
    let rp = (eta_t * ci - eta_i * ct) / max(abs(eta_t * ci + eta_i * ct), 1e-6);
    return 0.5 * (rs * rs + rp * rp);
}
fn relay_inside(s: RelayStop, p: vec3<f32>) -> bool {
    let n = s.normal.xyz;
    return dot(cross(s.c1.xyz - s.c0.xyz, p - s.c0.xyz), n) >= 0.0
        && dot(cross(s.c2.xyz - s.c1.xyz, p - s.c1.xyz), n) >= 0.0
        && dot(cross(s.c3.xyz - s.c2.xyz, p - s.c2.xyz), n) >= 0.0
        && dot(cross(s.c0.xyz - s.c3.xyz, p - s.c3.xyz), n) >= 0.0;
}
// The registered stop chain (raster_optics.rs `relay_trace` is the same law, its CPU witness):
// every face crossed in order inside its finite outline, Snell at glass faces, mirrors and the
// ideal relay lenses as the ray backend's periscope path applies them. Leaving an outline is
// meeting the housing. The housing's own walls are not traced: the measured disagreement with
// the complete block is RELAY_STOP_CHAIN_ERROR (tiger_periscopes.rs).
fn relay_trace(origin: vec3<f32>, direction: vec3<f32>, count: u32) -> RelayResult {
    var out: RelayResult;
    var p = origin;
    var d = normalize(direction);
    var throughput = vec3(1.0);
    var in_glass = false;
    var glass_from = p;
    for (var i = 0u; i < count; i += 1u) {
        let s = panel.stops[i];
        let sn = s.normal.xyz;
        let denom = dot(d, sn);
        if (abs(denom) < 1e-6) { return out; }
        let t = dot(s.c0.xyz - p, sn) / denom;
        if (t <= 0.0) { return out; }
        p += d * t;
        if (!relay_inside(s, p)) { return out; }
        let n = select(sn, -sn, denom > 0.0);
        let kind = u32(s.c0.w + 0.5);
        if (kind == 2u) {
            d = normalize(reflect(d, n));
            throughput *= 0.90;
        } else if (kind == 3u) {
            let axis = -n;
            let cosine = dot(d, axis);
            if (cosine < 0.05) { return out; }
            let height = p - s.center.xyz;
            let tangent_height = height - axis * dot(height, axis);
            d = normalize(d / cosine - tangent_height / s.c1.w);
            throughput *= 0.96;
        } else {
            let eta_i = select(1.0, RELAY_IOR, in_glass);
            let eta_t = select(RELAY_IOR, 1.0, in_glass);
            let eta = eta_i / eta_t;
            let cosine = -dot(d, n);
            let k = 1.0 - eta * eta * (1.0 - cosine * cosine);
            if (k < 0.0) { return out; }
            // The inner window's faces (kind 0) bend the ray here; their reflection and loss are
            // the glass pass's, drawn over this pane (rm_glass.wgsl).
            if (kind == 1u) {
                throughput *= 1.0 - relay_fresnel(cosine, eta_i, eta_t);
                if (in_glass) { throughput *= exp(-RELAY_ABSORPTION_PER_M * distance(p, glass_from)); }
            }
            d = normalize(eta * d + (eta * cosine - sqrt(k)) * n);
            in_glass = !in_glass;
            glass_from = p;
        }
        p += d * 1e-5;
    }
    out.valid = !in_glass;
    out.direction = d;
    out.throughput = throughput;
    return out;
}

const RELAY_HAZE_TAPS = 8u;
// Radiance through the pane at this fragment. Without a registered chain the pane admits every
// incident direction (the declared vision-block mapping). With one, the incident ray and three
// more over the pixel's footprint on the pane are carried through the stops: a ray that leaves
// a stop meets the housing interior, which transports no light here (its radiance is not
// computed: a dark underestimate, never a brightening). The image is sampled along the incident
// direction, which the chain preserves (tested); that is what gives the pane its parallax.
fn optic_radiance(in: Out, dx: vec3<f32>, dy: vec3<f32>, pupil: f32) -> vec3<f32> {
    let count = min(panel.layer.y, 12u);
    if (count == 0u) { return relay_sample(in.source).rgb * pupil; }
    let eye = panel.relay_eye.xyz;
    var admitted = vec3(0.0);
    let centre = relay_trace(eye, in.local - eye, count);
    for (var k = 0u; k < 4u; k += 1u) {
        let o = vec2(f32(k & 1u), f32(k >> 1u)) * 0.5 - vec2(0.25);
        let r = relay_trace(eye, in.local + dx * o.x + dy * o.y - eye, count);
        if (r.valid) { admitted += r.throughput * 0.25; }
    }
    let image = relay_sample(in.source).rgb;
    if (!centre.valid && all(admitted <= vec3(0.0))) { return vec3(0.0); }
    // Grooves on the eye-side face where the incident ray crosses it take their share of the
    // image out and spread the exterior light arriving across them (glass_scratches.wgsl).
    let face = panel.stops[0].normal.xyz;
    let d = normalize(in.local - eye);
    let entry = eye + d * (dot(panel.stops[0].c0.xyz - eye, face) / dot(d, face));
    let toward_eye = select(face, -face, dot(face, eye - entry) < 0.0);
    let footprint = max(length(dx), length(dy));
    let groove = glass_scratches(entry, toward_eye, footprint);
    var haze = vec3(0.0);
    if (groove.coverage > 0.0) {
        let t = groove.tangent;
        let jitter = fract(sin(dot(in.position.xy, vec2(12.9898, 78.233))) * 43758.547);
        for (var k = 0u; k < RELAY_HAZE_TAPS; k += 1u) {
            // Deflection across the groove: rotate the ray about the groove direction.
            let phi = SCRATCH_SPREAD_RAD * ((f32(k) + jitter) / f32(RELAY_HAZE_TAPS) * 2.0 - 1.0);
            let dk = d * cos(phi) + cross(t, d) * sin(phi) + t * dot(t, d) * (1.0 - cos(phi));
            let r = relay_trace(entry - dk * 1e-4, dk, count);
            if (!r.valid) { continue; }
            let world = (panel.model * vec4(dk, 0.0)).xyz;
            let sample = relay_sample(panel.source_direction_view_proj * vec4(world, 0.0));
            // Light from outside the captured field is not known here: it adds nothing.
            haze += sample.rgb * sample.a * r.throughput / f32(RELAY_HAZE_TAPS);
        }
    }
    let clear = 1.0 - groove.coverage;
    return (image * admitted * clear + haze * groove.coverage * SCRATCH_FORWARD) * pupil;
}

// Exterior radiance along a capture-projected direction; w = 1 inside the captured field.
fn relay_sample(source: vec4<f32>) -> vec4<f32> {
    // A vision-block direction outside the captured field takes the nearest direction instead
    // of leaving a black hole in a daylit block (see OFF_AXIS_ACCEPTANCE in raster_optics.rs).
    // A direction behind the capture camera takes the field centre. This remains named vision-
    // block debt for ports without a registered stop chain. Chained blocks fit their capture to
    // every incident direction and let the chain decide admission. Finite eyepieces fit/refresh
    // their capture to enclose every visible source ray.
    var uv = vec2(0.5);
    var inside = 0.0;
    if (source.w > 0.0) {
        let ndc = source.xy / source.w;
        inside = select(0.0, 1.0, all(abs(ndc) <= vec2(1.0)));
        uv = clamp(ndc * vec2(0.5, -0.5) + vec2(0.5), vec2(0.0), vec2(1.0));
    }
    // Only the active upper-left viewport is populated. Clamp at its last texel center so the
    // nearest sample cannot read the cleared, unused remainder when uv reaches exactly one.
    let extent=vec2<f32>(panel.layer.zw);let allocated=vec2<f32>(textureDimensions(exterior));
    let sample_uv=clamp(uv*extent,vec2(0.5),extent-vec2(0.5))/allocated;
    return vec4(textureSampleLevel(exterior, nearest_sampler, sample_uv, i32(panel.layer.x), 0.0).rgb, inside);
}
