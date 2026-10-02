// Isolated analytic Tiger-like geometry study. No production tank mesh is replaced.
// Camera, shadow, environment and reflected rays query precisely the same distance field.
// Distances in metres. Numerical marching tolerance is explicit; materials are authored test values.
struct Globals {
    res: vec2<f32>,          // internal render resolution, in texels
    time: f32,
    frame: f32,

    cam_pos: vec3<f32>,
    storm: f32,
    cam_fwd: vec3<f32>,
    night: f32,
    cam_right: vec3<f32>,
    effects: f32,
    cam_up: vec3<f32>,
    _p3: f32,

    out_res: vec2<f32>,      // window resolution (used by the crush pass)
    dither_mode: f32,
    dither_strength: f32,

    palette_on: f32,
    fov: f32,                // vertical half-angle tangent
    exposure: f32,           // applied in the crush pass, not here
    pair_penalty: f32,
};
@group(0) @binding(0) var<uniform> g: Globals;


const PI:f32=3.14159265359;
const FAR:f32=90.0;
fn box(p:vec3<f32>,b:vec3<f32>)->f32 {
 let q=abs(p)-b;return length(max(q,vec3(0.0)))+min(max(q.x,max(q.y,q.z)),0.0);
}
fn cylinder(p:vec3<f32>,r:f32,h:f32)->f32 {
 let q=vec2(length(p.xz)-r,abs(p.y)-h);return min(max(q.x,q.y),0.0)+length(max(q,vec2(0.0)));
}
fn rot(p:vec3<f32>,a:f32)->vec3<f32>{return vec3(cos(a)*p.x-sin(a)*p.z,p.y,sin(a)*p.x+cos(a)*p.z);}
fn nearest(a:vec2<f32>,d:f32,m:f32)->vec2<f32>{return select(a,vec2(d,m),d<a.x);}
fn tank(p:vec3<f32>)->vec2<f32>{
 // Recognizable Tiger proportions, authored analytic solids. Not a reconstruction of source armor.
 var h=vec2(box(p-vec3(0.0,1.25,0.0),vec3(1.45,0.40,3.0)),2.0);
 h=nearest(h,box(p-vec3(0.0,0.88,-0.08),vec3(1.10,0.35,2.63)),2.0);
 // Broad track bands are a box difference; the same openings exist for every ray.
 let trackp=vec3(abs(p.x)-1.47,p.y-0.76,p.z);
 let outer=box(trackp,vec3(0.35,0.62,2.82));
 let inner=box(trackp,vec3(0.45,0.42,2.60));
 h=nearest(h,max(outer,-inner),3.0);
 let wheelz=clamp(round(p.z/0.64),-3.0,3.0)*0.64;
 let wheelp=vec3(p.z-wheelz,abs(p.x)-1.49,p.y-0.72);
 h=nearest(h,cylinder(wheelp,0.49,0.29),2.0);
 h=nearest(h,cylinder(wheelp,0.18,0.33),3.0);
 // Turret, mantlet, 88 mm barrel and muzzle brake. Static lay keeps A/B frames identical.
 let q=rot(p-vec3(0.0,1.76,0.12),-0.15);
 let shell=cylinder(q-vec3(0.0,0.41,0.0),1.12,0.48);
 h=nearest(h,shell,2.0);
 h=nearest(h,box(q-vec3(0.0,0.38,1.05),vec3(0.60,0.37,0.25)),2.0);
 h=nearest(h,cylinder(vec3(q.x,q.z-2.95,q.y-0.47),0.095,1.65),2.0);
 let brakep=vec3(q.x,q.y-0.47,q.z-4.57);
 let brake=max(box(brakep,vec3(0.17,0.14,0.24)),-box(brakep,vec3(0.25,0.072,0.13)));
 h=nearest(h,brake,3.0);
 h=nearest(h,cylinder(q-vec3(0.55,1.04,-0.35),0.34,0.20),2.0);
 h=nearest(h,cylinder(q-vec3(-0.54,0.93,-0.24),0.30,0.026),3.0);
 // Engine grilles and exhausts provide small occlusion and silhouette witnesses.
 let gx=clamp(round(p.x/0.18),-5.0,5.0)*0.18;
 h=nearest(h,box(p-vec3(gx,1.675,-2.13),vec3(0.034,0.025,0.55)),3.0);
 h=nearest(h,cylinder(p-vec3(0.65,1.25,-3.14),0.18,0.66),3.0);
 h=nearest(h,cylinder(p-vec3(-0.65,1.25,-3.14),0.18,0.66),3.0);
 return h;
}
fn solids(p:vec3<f32>)->vec2<f32>{
 var h=tank(p);let detail=scene_detail(p);h=nearest(h,detail.x,detail.y);
 // A broken masonry gate tests off-screen occlusion and near/far shadows.
 h=nearest(h,box(p-vec3(-5.2,2.5,-4.8),vec3(0.65,2.5,0.7)),4.0);
 h=nearest(h,box(p-vec3(4.7,2.5,-4.8),vec3(0.65,2.5,0.7)),4.0);
 h=nearest(h,box(p-vec3(-0.25,5.0,-4.8),vec3(5.6,0.45,0.7)),4.0);
 h=nearest(h,box(rot(p-vec3(4.7,0.22,2.0),0.45),vec3(0.6,0.22,1.0)),4.0);
 // A real small area emitter for the night/firelight test, and a reflective steel witness sphere.
 h=nearest(h,length(p-vec3(-2.8,0.65,2.6))-0.25,5.0);
 h=nearest(h,length(p-vec3(3.3,0.6,3.9))-0.6,3.0);
 return h;
}
fn map(p:vec3<f32>)->vec2<f32>{return nearest(solids(p),p.y,1.0);}
fn trace(o:vec3<f32>,d:vec3<f32>,limit:f32)->vec2<f32>{
 // Intersect the same infinite ground exactly, avoiding tiny grazing SDF steps along a plane.
 var boundary=limit;var boundary_material=0.0;
 if(d.y < -0.000001){let ground=-o.y/d.y;
 if(ground>0.003 && ground<=limit){boundary=ground;boundary_material=1.0;}}
 var t=0.004;
 for(var i=0;i<384;i+=1){let h=solids(o+d*t);if(h.x<max(0.0005,t*0.00012)){return vec2(t,h.y);}
 t+=max(h.x*0.90,0.0002);if(t>boundary){return vec2(boundary,boundary_material);}}
 return vec2(t,-1.0); // unresolved is diagnostic magenta, never treated as empty sky
}
fn normal(p:vec3<f32>)->vec3<f32>{let e=vec2(0.001,0.0);
 return normalize(vec3(map(p+e.xyy).x-map(p-e.xyy).x,map(p+e.yxy).x-map(p-e.yxy).x,map(p+e.yyx).x-map(p-e.yyx).x));}
fn sky(d:vec3<f32>)->vec3<f32>{
 // Authored environment emitter, not a simulated atmosphere or hidden surface fill.
 let night=g.night;
 return mix(vec3(0.14,0.23,0.38),vec3(0.0003,0.0006,0.0012),night);
}
fn sun_dir()->vec3<f32>{return normalize(vec3(-0.7,0.42,0.32));}
fn sun_power()->vec3<f32>{return mix(vec3(4.8,4.1,3.2),vec3(0.003,0.004,0.007),g.night);}
fn albedo(m:f32,p:vec3<f32>)->vec3<f32>{
 if(m==1.0){return vec3(0.77,0.82,0.86);}
 if(m==2.0){return vec3(0.19,0.235,0.15);}
 if(m==3.0){return vec3(0.46,0.49,0.52);}
 if(m==6.0){return vec3(0.20,0.17,0.13);}
 if(m==7.0){return vec3(0.045,0.080,0.058);}
 return vec3(0.25,0.23,0.21);
}
fn emissive(m:f32)->vec3<f32>{return select(vec3(0.0),vec3(130.0,58.0,10.0),m==5.0);}
fn visibility(p:vec3<f32>,d:vec3<f32>,distance:f32)->f32 {
 let hit=trace(p,d,distance);return select(0.0,1.0,hit.y==0.0);
}
fn direct(p:vec3<f32>,n:vec3<f32>,v:vec3<f32>,m:f32)->vec3<f32>{
 if(m==5.0){return emissive(m);}
 let alb=albedo(m,p);let metal=select(0.0,1.0,m==3.0);let f0=mix(vec3(0.04),alb,metal);
 let l=sun_dir();let nl=max(dot(n,l),0.0);var color=vec3(0.0);
 if(nl>0.0){let h=normalize(l+v);let nh=max(dot(n,h),0.0);let nv=max(dot(n,v),0.0001);
 let vh=max(dot(v,h),0.0);let F=f0+(vec3(1.0)-f0)*pow(1.0-vh,5.0);
 let a2=0.16;let D=a2/(PI*pow(nh*nh*(a2-1.0)+1.0,2.0));
 let k=0.245;let G=nv/(nv*(1.0-k)+k)*nl/(nl*(1.0-k)+k);
 let brdf=(vec3(1.0)-F)*(1.0-metal)*alb/PI+F*(D*G/max(4.0*nv*nl,0.0001));
 color+=brdf*sun_power()*nl*visibility(p+n*0.004,l,FAR)*weather_transmission(p,l,FAR);}
 // Four surface samples of the finite glowing sphere, with emitter-facing cosines and solid angle.
 let c=vec3(-2.8,0.65,2.6);let to=p-c;let axis=normalize(to);
 let side=normalize(cross(axis,select(vec3(0.0,1.0,0.0),vec3(1.0,0.0,0.0),abs(axis.y)>0.95)));
 let up=cross(side,axis);
 for(var i=0;i<4;i+=1){let a=(f32(i)+0.5)*1.5707963;
 let source=c+0.25*normalize(axis*0.80+side*cos(a)*0.60+up*sin(a)*0.60);
 let delta=source-p;let distance=length(delta);let ld=delta/distance;
 let facing=max(dot(normalize(source-c),-ld),0.0);let surface=max(dot(n,ld),0.0);
 let weight=2.0*PI*0.25*0.25/4.0;
 color+=(1.0-metal)*alb/PI*emissive(5.0)*surface*facing*weight/max(distance*distance,0.0001)
 *visibility(p+n*0.004,ld,max(distance-0.01,0.004))*weather_transmission(p,ld,distance);}
 return color;
}
fn incoming(o:vec3<f32>,d:vec3<f32>)->vec3<f32>{
 let hit=trace(o,d,FAR);if(hit.y<0.0){return vec3(1.0,0.0,1.0);}
 if(hit.y==0.0){return sky(d);}let p=o+d*hit.x;return direct(p,normal(p),-d,hit.y);
}
fn render(o:vec3<f32>,d:vec3<f32>)->vec3<f32>{
 let hit=trace(o,d,FAR);if(hit.y<0.0){return vec3(1.0,0.0,1.0);}
 if(hit.y==0.0){return sky(d);}let p=o+d*hit.x;let n=normal(p);let m=hit.y;
 if(m==5.0){return emissive(m);}var color=direct(p,n,-d,m);
 let alb=albedo(m,p);let metal=select(0.0,1.0,m==3.0);let f0=mix(vec3(0.04),alb,metal);
 let F=f0+(vec3(1.0)-f0)*pow(1.0-max(dot(n,-d),0.0),5.0);
 let tangent=normalize(cross(n,select(vec3(0.0,1.0,0.0),vec3(1.0,0.0,0.0),abs(n.y)>0.95)));
 let bitangent=cross(n,tangent);
 // Deterministic cosine quadrature, four secondary rays. Blocked directions hit scene surfaces.
 var bounce=vec3(0.0);
 for(var i=0;i<4;i+=1){let z=sqrt(1.0-(f32(i)+0.5)/4.0);let radius=sqrt(1.0-z*z);
 let a=f32(i)*2.3999632;let rd=tangent*(radius*cos(a))+bitangent*(radius*sin(a))+n*z;
 bounce+=incoming(p+n*0.004,rd)*0.25;}
 color+=(vec3(1.0)-F)*(1.0-metal)*alb*bounce;
 // Ideal smooth reflection witness. Paint retains only its small dielectric Fresnel share.
 color+=F*incoming(p+n*0.004,reflect(d,n));return color;
}
struct VsOut{@builtin(position) pos:vec4<f32>,@location(0) uv:vec2<f32>};
@vertex fn vs(@builtin(vertex_index) vi:u32)->VsOut{
 let uv=vec2(f32((vi<<1u)&2u),f32(vi&2u));var o:VsOut;o.pos=vec4(uv*2.0-vec2(1.0),0.0,1.0);o.uv=uv;return o;}
@fragment fn fs(in:VsOut)->@location(0) vec4<f32>{
 let ndc=vec2((in.uv.x*2.0-1.0)*g.res.x/g.res.y,in.uv.y*2.0-1.0);
 let d=normalize(g.cam_right*ndc.x*g.fov+g.cam_up*ndc.y*g.fov+g.cam_fwd);
 let hit=trace(g.cam_pos,d,FAR);
 let base=render(g.cam_pos,d);
 return vec4(weather_camera(g.cam_pos,d,hit,base),1.0);}
