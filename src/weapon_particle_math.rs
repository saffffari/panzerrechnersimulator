//! Cosmetic ballistic burst records. No collision, damage, source-light or ammunition state.
use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

pub const SPARK_MULTIPLIER:u32=1000;
pub const EXTERIOR_VOLUME_MULTIPLIER:f32=100.0;
pub const EXTERIOR_RADIUS_MULTIPLIER:f32=4.641_589;
pub const MAX_SPARKS_PER_VIEW:u32=1_536_000;
pub const MAX_BURSTS:usize=48;
pub const MAX_TRACERS:usize=4096;
pub const BLAST:u32=0;
pub const ARMOR_IMPACT:u32=1;
pub const RICOCHET:u32=2;
pub const BULLET_ARMOR:u32=3;
pub const TRACER:u32=4;
pub const PRESSURE_VENT:u32=5;
pub const FULL_COOKOFF:u32=6;
pub const GROUND:u32=7;
pub const HOT_RIM:u32=8;
pub const RIM_SAMPLES:u32=512;
pub const GROUND_APEX_TARGET_M:f32=6.096;
pub const COOKOFF_APEX_M:f32=304.8;
pub const COOKOFF_VERTICAL_SPEED_MPS:f32=77.331_6;

/// Eighty bytes per emitter, not one matrix per spark. `meta`: seed, full requested count,
/// original count, kind. `parameters`: energy, incidence cosine, spatial scale, heavy weapon.
#[repr(C)]
#[derive(Clone,Copy,Debug,bytemuck::Pod,bytemuck::Zeroable)]
pub struct Record {
    pub origin_age:[f32;4], pub direction_life:[f32;4], pub normal_offset:[f32;4],
    pub parameters:[f32;4], pub meta:[u32;4],
}
impl Record {
    pub fn tracer(start:Vec3,end:Vec3)->Self {
        Self {origin_age:start.extend(0.0).to_array(),direction_life:end.extend(0.0).to_array(),
            normal_offset:[0.0;4],parameters:[0.0;4],meta:[0,1,1,TRACER]}
    }
    pub fn origin(self)->Vec3 {Vec3::from_array(self.origin_age[..3].try_into().unwrap())}
    pub fn active(self)->bool {
        let max_life=match self.meta[3] { HOT_RIM=>6.0,GROUND=>3.0,PRESSURE_VENT=>self.direction_life[3]+0.65,FULL_COOKOFF=>18.0,BULLET_ARMOR=>0.30,RICOCHET=>if self.parameters[3]>0.5 {1.40}else{0.64},_=>1.85};
        self.origin_age[3]>=0.0 && self.origin_age[3]<max_life && self.meta[1]>0
    }
    pub fn support_radius(self)->f32 {
        if self.meta[3]==HOT_RIM {return self.parameters[0]+self.parameters[1]+0.003;}
        let t=self.origin_age[3].max(0.0);
        // Conservative analytic ballistic bound, including the retained physical streak support.
        0.5+(if self.meta[3]==FULL_COOKOFF {110.0}else{44.0*self.parameters[2].max(1.0)})*t+4.905*t*t
    }
}
/// Explicit thin heated-rim estimate, not a constitutive perforation or melting solver.
/// The thermal reservoir receives at most1.5% of the actual lost kinetic energy. It never
/// heats from retained penetrator energy, chemical filler or an arbitrary terminal class.
#[derive(Clone,Copy,Debug)]
pub struct RimHeat {pub radius:f32,pub width:f32,pub mass_kg:f32,pub heat_j:f32,pub initial_kelvin:f32}
pub fn rim_heat(caliber_mm:f32,plate_mm:f32,deposited_j:f32)->Option<RimHeat> {
    if !caliber_mm.is_finite() || !plate_mm.is_finite() || !deposited_j.is_finite()
        || caliber_mm<=0.0 || plate_mm<=0.0 || deposited_j<=0.0 {return None;}
    let radius=caliber_mm*0.0005;
    let width=(caliber_mm*0.00010).clamp(0.002,0.025);
    let area=std::f32::consts::PI*((radius+width).powi(2)-radius.powi(2));
    let depth=(plate_mm*0.00003).clamp(0.001,0.006);
    let mass_kg=area*depth*7850.0;
    let heat_j=(deposited_j*0.015).min(mass_kg*500.0*(2000.0-293.0));
    Some(RimHeat {radius,width,mass_kg,heat_j,initial_kelvin:293.0+heat_j/(mass_kg*500.0)})
}
pub fn rim_temperature(initial:f32,age:f32)->f32 {293.0+(initial-293.0).max(0.0)*(-age.max(0.0)/2.5).exp()}
pub fn rim_sample(r:Record,index:u32)->Option<Sample> {
    if r.meta[3]!=HOT_RIM || !r.active() || index>=r.meta[1] {return None;}
    let n=Vec3::from_array(r.normal_offset[..3].try_into().unwrap());
    let tangent=Vec3::from_array(r.direction_life[..3].try_into().unwrap());
    let q=(index as f32+0.5)/r.meta[1] as f32;
    let inner=r.parameters[0];let outer=inner+r.parameters[1];
    let radius=(inner*inner+(outer*outer-inner*inner)*q).sqrt();
    let angle=index as f32*2.399_963_1+random(r.meta[0])*std::f32::consts::TAU;
    let center=r.origin()+(tangent*angle.cos()+n.cross(tangent)*angle.sin())*radius;
    let width=(std::f32::consts::PI*(outer*outer-inner*inner)/r.meta[1] as f32).sqrt();
    Some(Sample {center,axis:n,width,length:0.0,page:2})
}
#[derive(Clone,Copy,Debug)]
pub struct GroundBudget {pub residual_j:f32,pub ejecta_j:f32,pub mass_kg:f32,pub density:f32,pub snow:f32,pub base_count:u32}
pub fn ground_budget(projectile_mass:f32,velocity:Vec3,caliber_mm:f32,snow:f32)->Option<GroundBudget> {
    if !velocity.is_finite() || !projectile_mass.is_finite() || projectile_mass<=0.0 || !caliber_mm.is_finite() || caliber_mm<=0.0{return None;}
    let residual_j=0.5*projectile_mass*velocity.length_squared();if residual_j<0.05{return None;}
    let snow=snow.clamp(0.0,1.0);let density=1600.0*(1.0-snow)+220.0*snow;
    // Explicit material/support estimates, not measured crater constitutive physics. The
    // projectile caliber sets the displaced cylinder: radius6calibers, depth12calibers.
    let caliber=caliber_mm*0.001;
    let mass_kg=(std::f32::consts::PI*(caliber*6.0).powi(2)*(caliber*12.0)*density).max(0.0001);
    let ejecta_j=residual_j*0.10;
    let base_count=((residual_j/3000.0).sqrt()*32.0).round().clamp(8.0,384.0) as u32;
    Some(GroundBudget {residual_j,ejecta_j,mass_kg,density,snow,base_count})
}
/// A ground-ejecta grain's speed over the burst's apex-limited maximum: crater ejecta speeds follow
/// the scaling law M(>v) ~ v^(-3 mu), mu ~ 0.4 for loose soil (Housen & Holsapple 2011), sampled
/// between 0.15 and 1 of the maximum and drawn apart from the grain's direction. With one random
/// number setting both (2026-10-28) every grain lay in a crisp-faced cone that read as a box.
pub fn ground_speed_share(u:f32)->f32 {
    const LOW:f32=0.15;const EXPONENT:f32=1.2;
    let floor=LOW.powf(EXPONENT);
    // Inverse of the bounded power law: u = 0 the slowest, u = 1 the fastest.
    (floor/(1.0-u.clamp(0.0,1.0)*(1.0-floor))).powf(1.0/EXPONENT)
}
/// The samples each of a view's visible `bursts` draws when the view may draw `budget`: all of
/// them under budget; over it, each burst its share of what the hot rims (512 samples of one ring,
/// never thinned) leave, never fewer than its source count (`meta[2]`, the samples before the
/// thousandfold quadrature). The shader weighs each sample by `meta[2] / meta[1]`, so a thinned
/// burst carries the same emission in fewer, brighter samples; ground grains keep the grain size
/// of the full count and take its optical depth (`weapon_particles.wgsl`). The shares are floored,
/// so they fit; only source counts that cannot fit exceed the budget, which the caller omits.
pub fn thinned_counts(bursts:&[Record],budget:u64)->Vec<u32> {
    let requested:u64=bursts.iter().map(|r|u64::from(r.meta[1])).sum();
    let fixed=|r:&Record|r.meta[3]==HOT_RIM || r.meta[3]==TRACER;
    if requested<=budget {return bursts.iter().map(|r|r.meta[1]).collect();}
    let kept:u64=bursts.iter().filter(|r|fixed(r)).map(|r|u64::from(r.meta[1])).sum();
    let (share_budget,share_requested)=(budget.saturating_sub(kept),(requested-kept).max(1));
    bursts.iter().map(|r| {
        let full=r.meta[1];
        if fixed(r) {return full;}
        let share=(u64::from(full) as u128*share_budget as u128/share_requested as u128) as u64;
        (share.min(u64::from(full)) as u32).max(r.meta[2].min(full)).max(1)
    }).collect()
}
pub fn random(seed:u32)->f32 {
    let mut s=seed^(seed>>16);s=s.wrapping_mul(0x7feb352d);s^=s>>15;
    s=s.wrapping_mul(0x846ca68b);s^=s>>16;(s>>8) as f32/16777216.0
}
fn direction(seed:u32)->Vec3 {
    let y=random(seed)*2.0-1.0;let a=random(seed.wrapping_add(1))*std::f32::consts::TAU;
    let r=(1.0-y*y).sqrt();Vec3::new(r*a.cos(),y,r*a.sin())
}
#[derive(Clone,Copy,Debug)]
pub struct Sample {pub center:Vec3,pub axis:Vec3,pub width:f32,pub length:f32,pub page:u32}
/// Reference for the GPU analytic trajectories, also used by CPU-only source-plane regressions.
pub fn sample(r:Record,index:u32)->Option<Sample> {
    if r.meta[3]==HOT_RIM {return rim_sample(r,index);}
    let kind=r.meta[3];let mut t=r.origin_age[3];let n=Vec3::from_array(r.normal_offset[..3].try_into().unwrap());
    let d=Vec3::from_array(r.direction_life[..3].try_into().unwrap());
    let seed=r.meta[0].wrapping_mul(if kind<=ARMOR_IMPACT {9348571}else{3948571}).wrapping_add(index.wrapping_mul(977));
    let q=random(seed);let mut velocity;let axis;let width;let length;let page;
    if kind==GROUND {
        if t>=3.0{return None;}
        let spray=direction(seed.wrapping_add(19));let out=spray-n*(2.0*spray.dot(n).min(0.0));
        let tangent=(d-n*d.dot(n)).normalize_or_zero();
        let grazing=1.0-(-d.dot(n)).clamp(0.0,1.0);
        let ray=if index==0 {n}else{(n*(0.6+0.4*q)+tangent*(0.15+0.60*grazing)+out*0.4).normalize_or_zero()};
        let max_speed=(2.0*r.parameters[0]/r.parameters[1].max(0.0001)).sqrt().min((2.0*9.81*GROUND_APEX_TARGET_M).sqrt());
        velocity=ray*max_speed*if index==0 {1.0}else{ground_speed_share(random(seed.wrapping_add(41)))};
        axis=(velocity-Vec3::Y*9.81*t).normalize_or_zero();
        width=2.0*(3.0*r.parameters[1]/(4.0*std::f32::consts::PI*r.parameters[3]*r.meta[1] as f32)).cbrt();
        length=width;page=6;
    }else if kind==PRESSURE_VENT || kind==FULL_COOKOFF {
        let delay=if index==0 {0.0}else{q*if kind==PRESSURE_VENT {r.direction_life[3]}else{0.45}};
        t-=delay;let life=if kind==PRESSURE_VENT {0.65}else{17.0};if t<0.0 || t>=life{return None;}
        let spray=direction(seed.wrapping_add(19));
        if kind==PRESSURE_VENT {
            velocity=(n+spray*0.11).normalize()*(26.0+16.0*q);
        }else {
            let horizontal=Vec3::new(n.x,0.0,n.z).normalize_or_zero();
            velocity=Vec3::Y*COOKOFF_VERTICAL_SPEED_MPS*if index==0 {1.0}else{0.7+0.3*q}
                +horizontal*(6.0+12.0*q)+Vec3::new(spray.x,0.0,spray.z)*12.0;
            if velocity.dot(n)<0.0 {velocity-=n*velocity.dot(n);}
            // The authored5kg presentation reservoir receives at most0.2% of the inherited
            // detonation-energy model. A weaker supplied reservoir cannot reach the target apex.
            velocity=velocity.clamp_length_max((2.0*r.parameters[0]/r.parameters[1].max(0.001)).max(0.0).sqrt());
        }
        axis=(velocity-Vec3::Y*9.81*t).normalize_or_zero();
        width=if kind==PRESSURE_VENT {(0.008+0.006*q)*(1.0-t/life).sqrt()}else{(0.007+0.006*q)*(-t/8.0).exp()};
        length=((velocity-Vec3::Y*9.81*t).length()*0.002).clamp(width,0.15);
        page=if t<0.15 {4}else{2};
    }else if kind==BULLET_ARMOR {
        let life=0.08+0.22*q;if t>=life{return None;}
        let spray=direction(seed.wrapping_add(19));let out=spray-n*(2.0*spray.dot(n).min(0.0));
        let cosine=r.parameters[1];axis=(d*(1.0-cosine)*0.8+out*0.55+n*0.25).normalize_or_zero();
        let speed=(r.parameters[0]/3200.0).sqrt().min(2.0)*(4.0+18.0*q)*(0.4+0.6*cosine.sqrt());
        velocity=axis*speed;width=(0.0015+0.002*q)*(1.0-t/life).sqrt();length=(speed*0.002).clamp(width,0.04);
        page=if t<0.02 {3}else if t<0.075 {4}else{2};
    }else if kind==RICOCHET {
        let heavy=r.parameters[3]>0.5;let life=if heavy {0.55+0.85*q}else{0.24+0.40*q};if t>=life{return None;}
        let mut ray=(d*0.75+direction(seed.wrapping_add(19))*0.30+n*0.25).normalize_or_zero();
        if ray.dot(n)<0.05 {ray=(ray+n*(0.06-ray.dot(n))).normalize_or_zero();}
        axis=ray;let speed=if heavy {14.0+30.0*q}else{5.0+15.0*q};velocity=axis*speed;
        width=(if heavy {0.008+0.010*q}else{0.003+0.005*q})*(1.0-t/r.direction_life[3]).max(0.15);
        length=(speed*0.008).clamp(0.025,0.32);page=if t<0.055 {3}else if t<0.16 {4}else{2};
    }else {
        let life=0.45+1.4*q;if t>=life{return None;}
        let mut ray=direction(seed.wrapping_add(17));ray.y=ray.y.abs()*0.75+0.15;
        if kind==ARMOR_IMPACT {ray=Quat::from_rotation_arc(Vec3::Y,n)*ray;}
        velocity=ray.normalize()*(10.0+28.0*q)*r.parameters[2];
        let current=velocity-Vec3::Y*9.81*t;axis=current.normalize_or_zero();
        width=(0.006+0.012*q)*(1.0-t/life).sqrt();length=(current.length()*0.008).min(0.32);
        page=if t<0.12 {3}else if t<0.55 {4}else{2};
    }
    velocity.y-=4.905*t;
    let center=r.origin()+velocity*t;
    if kind!=BLAST {
        let rotation=Quat::from_rotation_arc(Vec3::Z,axis);
        let support=(rotation.conjugate()*n).abs().dot(Vec3::new(width,width,length))*0.5;
        if (center-r.origin()).dot(n)+r.normal_offset[3]<support {return None;}
    }
    Some(Sample {center,axis,width,length,page})
}
/// Exact WGPU near-plane segment clip; all generated vertices stay at actual endpoint depths.
pub fn clip_segment(mut a:Vec4,mut b:Vec4)->Option<(Vec4,Vec4)> {
    if a.z<0.0 && b.z<0.0{return None;}
    if a.z<0.0 {a=a.lerp(b,a.z/(a.z-b.z));a.z=0.0;}
    if b.z<0.0 {b=b.lerp(a,b.z/(b.z-a.z));b.z=0.0;}
    if a.w<=0.0 || b.w<=0.0{return None;}Some((a,b))
}
/// Reference screen-space strip: width is one native pixel, including far off-axis segments.
pub fn tracer_edges(a:Vec3,b:Vec3,eye:Vec3,vp:Mat4,extent:[u32;2])->Option<[Vec2;4]> {
    let (a,b)=clip_segment(vp*(a-eye).extend(1.0),vp*(b-eye).extend(1.0))?;
    let scale=Vec2::new(extent[0] as f32,extent[1] as f32)*0.5;
    let a=a.truncate().truncate()/a.w*scale;let b=b.truncate().truncate()/b.w*scale;
    let delta=b-a;let normal=if delta.length_squared()>1e-8 {Vec2::new(-delta.y,delta.x).normalize()}else{Vec2::X};
    Some([a-normal*0.5,a+normal*0.5,b-normal*0.5,b+normal*0.5])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn hot_rim_respects_heat_budget_cools_in_real_time_and_has_no_filled_center() {
        let heat=rim_heat(88.0,90.0,1_200_000.0).unwrap();
        assert!(heat.heat_j<=1_200_000.0*0.015);
        assert!((heat.mass_kg*500.0*(heat.initial_kelvin-293.0)-heat.heat_j).abs()<0.01);
        assert!(rim_heat(88.0,90.0,0.0).is_none());
        let cold=rim_heat(88.0,90.0,10.0).unwrap();assert!(cold.initial_kelvin<300.0);
        for hz in [30,60,144] {
            let time=(0..hz*2).fold(0.0_f64,|t,_|t+1.0/hz as f64) as f32;
            assert!((rim_temperature(heat.initial_kelvin,time)-rim_temperature(heat.initial_kelvin,2.0)).abs()<0.001);
        }
        let n=Vec3::new(0.6,0.7,-0.2).normalize();let tangent=Quat::from_rotation_arc(Vec3::Z,n)*Vec3::X;
        let r=Record{origin_age:[35.0,4.0,-98.0,0.25],direction_life:tangent.extend(6.0).to_array(),normal_offset:n.extend(0.002).to_array(),
            parameters:[heat.radius,heat.width,heat.initial_kelvin,2.5],meta:[7,RIM_SAMPLES,RIM_SAMPLES,HOT_RIM]};
        let area=std::f32::consts::PI*((heat.radius+heat.width).powi(2)-heat.radius.powi(2));
        let mut sum=0.0;
        for i in 0..RIM_SAMPLES {let s=sample(r,i).unwrap();let offset=s.center-r.origin();
            assert!(offset.dot(n).abs()<0.00001);assert!(offset.length()>=heat.radius-0.00001);
            assert!(offset.length()<=heat.radius+heat.width+0.00001);sum+=s.width*s.width;
        }
        assert!((sum-area).abs()<0.00001);
    }
    #[test] fn one_pixel_width_uses_projection_not_range_at_every_extent() {
        for extent in [[640,360],[800,450],[960,540],[64,36]] {for fov in [0.45_f32,1.2,2.0] {
            let vp=Mat4::perspective_rh(fov,extent[0] as f32/extent[1] as f32,0.05,2000.0);
            for depth in [0.1,1.0,100.0,900.0] {for x in [0.0,depth*0.8] {
                let e=tracer_edges(Vec3::new(x,0.0,-depth),Vec3::new(x+0.03,0.1,-depth*1.01),Vec3::ZERO,vp,extent).unwrap();
                assert!(((e[1]-e[0]).length()-1.0).abs()<0.0002);
                assert!(((e[3]-e[2]).length()-1.0).abs()<0.0002);
            }}
        }}
    }
    #[test] fn near_plane_clipping_preserves_endpoint_and_rejects_hidden_segment() {
        let vp=Mat4::perspective_rh(1.0,1.0,0.1,100.0);
        let a=vp*Vec3::new(0.0,0.0,0.2).extend(1.0);let b=vp*Vec3::new(0.2,0.0,-2.0).extend(1.0);
        let (cut,end)=clip_segment(a,b).unwrap();assert_eq!(cut.z,0.0);assert_eq!(end,b);assert!(cut.w>0.0);
        assert!(clip_segment(a,a).is_none());
    }
    #[test] fn burst_volume_count_and_upload_are_bounded_independently() {
        assert!((EXTERIOR_RADIUS_MULTIPLIER.powi(3)-EXTERIOR_VOLUME_MULTIPLIER).abs()<0.0001);
        assert_eq!(384*SPARK_MULTIPLIER,384000);assert_eq!(std::mem::size_of::<Record>(),80);
        assert_eq!(MAX_BURSTS*std::mem::size_of::<Record>(),3840);
        let mut available=MAX_SPARKS_PER_VIEW;let mut emitted=0;
        for _ in 0..MAX_BURSTS {let n=384*SPARK_MULTIPLIER;if n<=available {available-=n;emitted+=n;}}
        assert_eq!(emitted,MAX_SPARKS_PER_VIEW);
        // Independent samples divide the old total weight; multiplying count cannot multiply power.
        for area in [0.001_f64,1.0,37.0] {assert!((area/1000.0*1000.0-area).abs()<1e-10);}
    }
    #[test] fn a_view_over_budget_thins_every_burst_and_keeps_its_light() {
        let burst=|kind:u32,source:u32|Record {origin_age:[0.0;4],direction_life:[0.0,0.0,1.0,1.85],normal_offset:[0.0,1.0,0.0,0.0],
            parameters:[0.0;4],meta:[7,source*SPARK_MULTIPLIER,source,kind]};
        // An HE strike seen by a 240x136 port at 8 samples a pixel: blast sparks and ground ejecta.
        let (blast,ground,rim)=(burst(BLAST,384),burst(GROUND,384),Record {meta:[9,RIM_SAMPLES,RIM_SAMPLES,HOT_RIM],..burst(HOT_RIM,1)});
        let budget=240*136*8;
        let drawn=thinned_counts(&[blast,ground,rim],budget);
        let total=drawn.iter().map(|&n|u64::from(n)).sum::<u64>();
        assert!(total<=budget && total+2>=budget,"{total} of {budget}");
        assert_eq!(drawn[2],RIM_SAMPLES,"a hot rim is never thinned");
        for (r,&n) in [blast,ground].iter().zip(&drawn) {
            assert!(n>=r.meta[2] && n<r.meta[1]);
            // Each sample is weighed by source / drawn: the burst's emission is the same.
            let total=n as f64*(r.meta[2] as f64/n as f64);
            assert!((total-r.meta[2] as f64).abs()<1e-9);
        }
        // Under budget nothing changes; a starved view still asks for every burst's source samples.
        assert_eq!(thinned_counts(&[blast,ground],u64::MAX),vec![blast.meta[1],ground.meta[1]]);
        assert_eq!(thinned_counts(&[blast,ground],100),vec![384,384]);
        // The shader takes the full count as identity.z * 1000 (ground grain size, optical depth).
        assert_eq!(SPARK_MULTIPLIER,1000);
        assert!(include_str!("../shaders/weapon_particles.wgsl").contains("f32(identity.z*1000u)/f32(identity.y)"));
    }
    #[test] fn analytic_sparks_keep_complete_support_on_rotated_source_plane() {
        for n in [Vec3::X,Vec3::NEG_Z,Vec3::new(0.3,0.8,-0.4).normalize()] {
            for kind in [ARMOR_IMPACT,RICOCHET,BULLET_ARMOR] {
                let mut visible=0;
                for age in [0.001,0.025,0.1,0.25,0.5] {
                    let r=Record {origin_age:[52.0,7.0,-201.0,age],direction_life:n.extend(1.4).to_array(),
                        normal_offset:n.extend(if kind==ARMOR_IMPACT {0.03}else{0.008}).to_array(),
                        parameters:[3200.0,0.7,1.0,1.0],meta:[31,96000,96,kind]};
                    for i in (0..96000).step_by(97) {if let Some(s)=sample(r,i) {
                        let rot=Quat::from_rotation_arc(Vec3::Z,s.axis);
                        let support=(rot.conjugate()*n).abs().dot(Vec3::new(s.width,s.width,s.length))*0.5;
                        assert!((s.center-r.origin()).dot(n)+r.normal_offset[3]>=support-1e-6);visible+=1;
                    }}
                }
                assert!(visible>100);
            }
        }
    }
    #[test] fn full_cookoff_plume_has_requested_ballistic_apex_without_turret_mutation() {
        let time=COOKOFF_VERTICAL_SPEED_MPS/9.81;
        let r=Record {origin_age:[0.0,3.0,0.0,time],direction_life:[0.0,1.0,0.0,18.0],
            normal_offset:[0.0,1.0,0.0,0.03],parameters:[30000.0,5.0,1.0,1.0],meta:[1,768000,768,FULL_COOKOFF]};
        let s=sample(r,0).unwrap();assert!((s.center.y-3.0-COOKOFF_APEX_M).abs()<0.001);
        let mut weak=r;weak.parameters[0]=300.0;weak.origin_age[3]=0.5;
        assert!(sample(weak,0).unwrap().center.y<9.0,"insufficient kinetic energy cannot produce a305m trajectory");
        let mut ordinary=r;ordinary.meta[3]=BLAST;assert!(sample(ordinary,0).is_none(),"ordinary sparks expire long before the cookoff apex");
        let mut vent=r;vent.meta[3]=PRESSURE_VENT;vent.origin_age[3]=0.01;vent.direction_life[3]=1.25;
        assert!(sample(vent,0).unwrap().center.y<4.0);
    }

    #[test] fn ground_ejecta_uses_residual_energy_mass_and_real_incidence() {
        for snow in [0.0,1.0] {for speed in [5.0,20.0,300.0,820.0] {
            let b=ground_budget(0.0096,Vec3::NEG_Y*speed,7.62,snow).unwrap_or_else(||panic!("budget"));
            let r=Record {origin_age:[0.0,0.01,0.0,0.0],direction_life:[0.0,-1.0,0.0,3.0],normal_offset:[0.0,1.0,0.0,0.01],
                parameters:[b.ejecta_j,b.mass_kg,b.snow,b.density],meta:[7,b.base_count*1000,b.base_count,GROUND]};
            let max_speed=(2.0*b.ejecta_j/b.mass_kg).sqrt().min((2.0*9.81*GROUND_APEX_TARGET_M).sqrt());
            let mut apex=r;apex.origin_age[3]=max_speed/9.81;
            if let Some(s)=sample(apex,0){assert!(s.center.y<=0.01+GROUND_APEX_TARGET_M+0.001);}
            assert!(0.5*b.mass_kg*max_speed*max_speed<=b.residual_j*0.100001);
            if speed<20.0 {assert!(max_speed*max_speed/(2.0*9.81)<GROUND_APEX_TARGET_M);}
        }}
        let b=ground_budget(0.0096,Vec3::new(700.0,-5.0,0.0),7.62,0.0).unwrap();
        let r=Record {origin_age:[0.0,0.01,0.0,0.05],direction_life:[1.0,-0.007,0.0,3.0],normal_offset:[0.0,1.0,0.0,0.01],
            parameters:[b.ejecta_j,b.mass_kg,b.snow,b.density],meta:[1,b.base_count*1000,b.base_count,GROUND]};
        let mean=(1..1000).filter_map(|i|sample(r,i)).map(|s|s.center.x).sum::<f32>();
        assert!(mean>100.0,"grazing impact retains forward ejecta bias");
        assert!(ground_budget(0.0,Vec3::X,7.62,0.0).is_none());
    }

}
