// Shared world-space medium for this analytic experiment. Extinction is in inverse metres.
// Single-scattering quadrature, not a multiple-scattering atmosphere/combustion solver.
fn wx_hash(p:vec3<f32>)->f32{var q=fract(p*vec3(0.1031,0.1030,0.0973));q+=dot(q,q.yzx+vec3(33.33));return fract((q.x+q.y)*q.z);}
fn wx_noise(p:vec3<f32>)->f32 {
 let i=floor(p);let q=fract(p);let u=q*q*(vec3(3.0)-2.0*q);
 return mix(mix(mix(wx_hash(i),wx_hash(i+vec3(1.,0.,0.)),u.x),mix(wx_hash(i+vec3(0.,1.,0.)),wx_hash(i+vec3(1.,1.,0.)),u.x),u.y),
 mix(mix(wx_hash(i+vec3(0.,0.,1.)),wx_hash(i+vec3(1.,0.,1.)),u.x),mix(wx_hash(i+vec3(0.,1.,1.)),wx_hash(i+vec3(1.,1.,1.)),u.x),u.y),u.z);
}
fn wx_fbm(p:vec3<f32>)->f32{return 0.60*wx_noise(p)+0.28*wx_noise(p*2.03+vec3(5.7))+0.12*wx_noise(p*4.17+vec3(13.1));}
fn wx_wind()->vec3<f32>{return vec3(3.4,0.0,1.25)*(0.7+0.4*g.storm);}
fn wx_density(p:vec3<f32>)->vec3<f32>{
 let advected=p-wx_wind()*g.time;
 let gust=wx_fbm(advected*0.075+vec3(0.0,0.0,g.time*0.017));
 // Fog and suspended snow share the same gust field instead of unrelated looping effects.
 let fog=g.storm*0.020*(0.40+1.20*gust)*exp(-max(p.y,0.0)*0.035);
 let cloud_height=smoothstep(15.0,20.0,p.y)*(1.0-smoothstep(27.0,34.0,p.y));
 let cloud=max(wx_fbm(advected*vec3(0.047,0.12,0.047))-0.34,0.0)*0.60*cloud_height*min(g.storm,1.5);
 // Advected, expanding smoke above the actual test emitter; no opaque box surface.
 let rise=max(p.y-0.8,0.0);let centre=vec3(-2.8,0.0,2.6)+wx_wind()*rise*0.09;
 let radial=length((p-centre).xz)/(0.48+0.18*rise);
 let plume=(1.0-smoothstep(0.1,1.0,radial))*smoothstep(0.6,1.5,p.y)*(1.0-smoothstep(5.0,10.0,p.y));
 let smoke=plume*max(wx_fbm(p*1.3-wx_wind()*g.time*0.2-vec3(0.0,g.time*0.9,0.0))-0.24,0.0)*1.4*g.effects;
 return vec3(fog,cloud,smoke);
}
fn weather_transmission(p:vec3<f32>,d:vec3<f32>,distance:f32)->f32 {
 if(g.storm<=0.0 && g.effects<=0.0){return 1.0;}
 let step=distance/6.0;var tau=0.0;
 for(var i=0;i<6;i+=1){let s=wx_density(p+d*((f32(i)+0.5)*step));tau+=(s.x+s.y+s.z)*step;}
 return exp(-tau);
}
fn wx_phase(c:f32,asym:f32)->f32{return (1.0-asym*asym)/(4.0*PI*pow(max(1.0+asym*asym-2.0*asym*c,0.01),1.5));}
// One additional volume-scattering order on environment connections. This is a bounded
// radiance integral, not a constant ambient fill under opaque clouds.
fn wx_sky_radiance(p:vec3<f32>,d:vec3<f32>,limit:f32)->vec3<f32>{
 let ray=trace(p,d,limit);let length=ray.x;let ds=length/4.0;var trans=1.0;var value=vec3(0.0);
 for(var i=0;i<4;i+=1){let sample=p+d*((f32(i)+0.5)*ds);let density=wx_density(sample);
  let sigma=density.x+density.y+density.z;let attenuation=exp(-sigma*ds);
  let solar=sun_dir();let incoming=sun_power()*wx_phase(dot(solar,-d),0.48)
   *visibility(sample,solar,FAR)*weather_transmission(sample,solar,FAR);
  let alb=(0.96*(density.x+density.y)+0.3*density.z)/max(sigma,0.00001);
  value+=trans*(1.0-attenuation)*alb*incoming;trans*=attenuation;
 }
 if(ray.y==0.0){value+=trans*sky(d);}return value;
}
fn wx_incident(p:vec3<f32>,toward_camera:vec3<f32>)->vec3<f32>{
 let sun=sun_dir();var result=sun_power()*wx_phase(dot(sun,toward_camera),0.48)
 *visibility(p,sun,FAR)*weather_transmission(p,sun,FAR);
 // Four environment directions; geometry and the same medium bound every incident connection.
 let dirs=array<vec3<f32>,4>(vec3(0.707,0.707,0.0),vec3(-0.707,0.707,0.0),vec3(0.0,0.707,0.707),vec3(0.0,0.707,-0.707));
 // Spatial deterministic direction stratification; sample count can be increased for convergence.
 let k=u32(wx_hash(floor(p*2.0))*4.0)%4u;let environment_direction=dirs[k];
 result+=wx_sky_radiance(p,environment_direction,FAR)*(2.0*PI)*wx_phase(dot(environment_direction,toward_camera),0.3);
 let centre=vec3(-2.8,0.65,2.6);let delta=centre-p;let distance=length(delta);let ld=delta/max(distance,0.0001);
 // The emitter is a sphere. Connection terminates at its real near surface, never a wide proxy.
 let end=max(distance-0.251,0.004);
 result+=emissive(5.0)*(PI*0.25*0.25/max(distance*distance,0.0625))
 *wx_phase(dot(ld,toward_camera),0.15)*visibility(p,ld,end)*weather_transmission(p,ld,end);
 return result;
}
fn wx_snow(o:vec3<f32>,d:vec3<f32>,limit:f32,base:vec3<f32>)->vec3<f32>{
 if(g.storm<=0.0){return base;}
 // World-anchored cellular snow; exact sphere intersections with cells traversed by this ray.
 // Wind advection and fall are continuous. Opaque scene depth bounds every flake.
 let cell_size=0.85;let motion=wx_wind()*g.time+vec3(0.0,-1.7*g.time,0.0);
 let origin=o-motion;var t=0.004;
 for(var i=0;i<112;i+=1){
  if(t>=limit || t>70.0){break;}
  let p=origin+d*t;let cell=floor(p/cell_size);let h=wx_hash(cell);
  let next_edge=(cell+select(vec3(0.0),vec3(1.0),d>vec3(0.0)))*cell_size;
  let safe_d=select(vec3(0.000001),d,abs(d)>vec3(0.000001));
  let crossings=(next_edge-p)/safe_d;
  let exit_t=t+min(crossings.x,min(crossings.y,crossings.z));
  if(h<min(g.storm*0.27,0.84)){
   let offset=vec3(wx_hash(cell+vec3(11.0)),wx_hash(cell+vec3(23.0)),wx_hash(cell+vec3(37.0)));
   let centre=(cell+vec3(0.16)+offset*0.68)*cell_size+motion;
   let radius=0.012+0.014*wx_hash(cell+vec3(71.0));let rel=o-centre;
   let b=dot(rel,d);let disc=b*b-dot(rel,rel)+radius*radius;
   if(disc>0.0){let at=-b-sqrt(disc);
    if(at>=t && at<min(exit_t,limit)){
     let point=o+d*at;let n=normalize(point-centre);
     // Small ice aggregate, Lambert approximation, shadowed by the actual scene.
     let l=sun_dir();let c=vec3(-2.8,0.65,2.6)-point;let dist=length(c);let ld=c/max(dist,0.0001);
     let irradiance=sun_power()*max(dot(n,l),0.0)*visibility(point,l,FAR)*weather_transmission(point,l,FAR)
       +wx_sky_radiance(point,vec3(0.0,1.0,0.0),FAR)*PI
       +emissive(5.0)*max(dot(n,ld),0.0)*0.0625*PI/max(dist*dist,0.0625)*visibility(point,ld,max(dist-0.251,0.004));
     let opacity=0.78;let foreground=wx_medium(o,d,at);
     return mix(base,foreground.rgb+foreground.a*irradiance*(0.92/PI),opacity);
    }
   }
  }
  t=max(exit_t+0.0005,t+0.0005);
 }
 return base;
}
fn wx_medium(o:vec3<f32>,d:vec3<f32>,limit:f32)->vec4<f32>{
 let step=limit/20.0;var transmission=1.0;var radiance=vec3(0.0);
 for(var i=0;i<20;i+=1){
  let at=(f32(i)+0.5)*step;let p=o+d*at;let density=wx_density(p);let sigma=density.x+density.y+density.z;
  if(sigma>0.00001){
   let extinction=exp(-sigma*step);let albedo=(0.96*(density.x+density.y)+0.30*density.z)/sigma;
   radiance+=transmission*(1.0-extinction)*albedo*wx_incident(p,-d);transmission*=extinction;
  }
  if(transmission<0.002){break;}
 }
 return vec4(radiance,transmission);
}
fn weather_camera(o:vec3<f32>,d:vec3<f32>,hit:vec2<f32>,base:vec3<f32>)->vec3<f32>{
 if(g.storm<=0.0 && g.effects<=0.0){return base;}
 let medium=wx_medium(o,d,min(hit.x,FAR));
 return wx_snow(o,d,min(hit.x,FAR),medium.rgb+base*medium.a);
}
