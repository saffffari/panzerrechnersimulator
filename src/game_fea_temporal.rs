//! Live impact state. Local constitutive substeps consume elapsed simulation time;
//! fragments transfer to the flight integrator only after their cells clear the plate.
//! The plate and shot retain the existing calibrated constitutive/contact laws. They
//! are not a validated, fully coupled material experiment.
use super::*;
use crate::fea_damage::shot;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LiveBurst {
    pub context: LiveImpactContext, pub origin: Vec3, pub born:f32, pub energy_j:f32,
    pub visited:std::collections::BTreeSet<u32>,
    /// Current gas state, advanced with the world's clock, not a solved future history.
    pub gas_mass:f32, pub gas_energy:f32,
}

/// Equal antipodal pairs: exactly the residual steel mass and its centre-of-mass
/// momentum. The radial kinetic energy is an explicit share of the filler energy.
fn split_burst_body(body:HotFragment, filler_kg:f32, energy_j:f32) -> Vec<HotFragment> {
    let mass=(fragment_mass(&body)-filler_kg).max(0.0);
    if mass<=0.0{return Vec::new();}
    let n=64usize;
    let speed=(2.0*energy_j/mass).sqrt();
    let per=mass/n as f32;
    let size=(per/7850.0).cbrt();
    let mut out=Vec::with_capacity(n);
    for pair in 0..n/2 {
        let y=1.0-2.0*(pair as f32+0.5)/(n/2) as f32;
        let angle=pair as f32*2.3999632;
        let radial=(1.0-y*y).sqrt();
        let axis=Vec3::new(radial*angle.cos(),y,radial*angle.sin());
        for sign in [-1.0,1.0] {
            let i=out.len();
            out.push(HotFragment{id:(body.penetration_id<<24)|(1<<21)|i as u64,
                vel:body.vel+axis*(speed*sign),size,mass_kg:per,
                tracer_share:body.tracer_share*per/fragment_mass(&body),
                prev_pos:body.pos,origin:body.pos,age:0.,
                material_remaining_m:0.,material_force_n:0.,cross_section_m2:size*size,..body});
        }
    }
    out
}

impl Game {
    fn begin_live_burst(&mut self, mut context:LiveImpactContext, body:HotFragment)->Vec<HotFragment> {
        // The existing game fuze assumption is burst on clearing the plate. No historical
        // fuze-delay data are present in ShellSpec; this does not invent a measured delay.
        let filler=(context.event.shell.filler_g*0.001).min(fragment_mass(&body));
        let energy=filler*compartment::TNT_J_PER_KG;
        let radial_energy=energy*0.2; // authored fragmentation share, conserved below
        context.armor_part=ArmorPart::Hull;context.armor_local_point=body.pos;
        context.entry_w=self.player.hull_model().transform_point3(body.pos);
        let volume=compartment::TIGER_CREW_AIR_M3;
        let mass=101325.0*volume/(287.05*293.0);
        self.fea.bursts.push(LiveBurst{context,origin:body.pos,born:self.armor_clock_s,energy_j:energy-radial_energy,
            visited:Default::default(),gas_mass:mass,gas_energy:mass*(287.05/0.4)*293.0+energy-radial_energy});
        self.weapon_vfx.interior_blast(context.entry_w,(0.45*(filler/0.09).cbrt()).clamp(0.18,1.15));
        println!("[temporal] filler burst t={:.9} at={:?} energy={energy:.1} J",self.armor_clock_s,body.pos);
        split_burst_body(body,filler,radial_energy)
    }

    pub(in crate::game) fn advance_live_bursts(&mut self,dt:f32) {
        let mut bursts=std::mem::take(&mut self.fea.bursts);
        for b in &mut bursts {
            let age=(self.armor_clock_s-b.born).max(0.0);
            let elapsed=dt.min(age);
            if elapsed<=0.0{continue;}
            // Finite propagation to each current crew position. No future victim list.
            for body in self.physical_crew_bodies(BloodFrame::Hull) {
                if body.pos.distance(b.origin)>age*343.0 || !b.visited.insert(body.casualty.peer_id){continue;}
                self.apply_live_penetration_contacts(&b.context,PenetrationSolution{
                    penetration_id:b.context.penetration_id,frame:BloodFrame::Hull,entry:b.origin,
                    axis:Vec3::ZERO,fragments:Vec::new(),contacts:Vec::new(),result:SpallResult::default(),couch_hits:Vec::new(),
                },b.energy_j,Some(body.casualty.peer_id));
            }
            let cv=287.05/0.4;
            let temp=b.gas_energy/(b.gas_mass*cv);
            let pressure=b.gas_mass*287.05*temp/compartment::TIGER_CREW_AIR_M3;
            let openings=self.fea_crew_openings(0.01);
            // Quasi-static compartment pressure is admitted only after the wave can cross it.
            if age>=0.015 {
                for (i,o) in openings.iter().enumerate().filter(|(_,o)|o.hatch&&!o.open) {
                    if (pressure-101325.0)*o.area_m2>compartment::HATCH_LATCH_N {
                        if i<4 {self.roof_hatches.open[i]=true;}
                        else if i==CUPOLA_OPENING {self.hatch_open=true;}
                    }
                }
            }
            let area:f32=openings.iter().filter(|o|o.open).map(|o|o.area_m2).sum();
            if pressure>101325.0 && area>0.0 {
                let dm=(compartment::orifice_mass_flow(pressure,temp,101325.0,0.62*area)*elapsed).min(b.gas_mass*0.5);
                b.gas_mass-=dm;b.gas_energy-=dm*cv*1.4*temp;
            }
        }
        bursts.retain(|b|self.armor_clock_s-b.born<0.2);
        self.fea.bursts=bursts;
    }
}

#[derive(Clone, Debug)]
pub struct TemporalView {
    pub origin: Vec3, pub axes: [Vec3; 3], pub h: f32, pub half: [f32; 3],
    pub positions: Vec<Vec3>, pub temperatures: Vec<f32>, pub retained: Vec<bool>,
    pub shot_h: f32, pub shot_positions: Vec<Vec3>, pub shot_temperatures: Vec<f32>,
    pub shot_retained: Vec<bool>, pub t: f32, pub born: f32, pub steps: u64,
    pub initial_shot_mass: f32, pub released_shot_mass: f32,
}

pub struct TemporalImpact {
    context: LiveImpactContext, armor: VehicleArmorContact,
    pub sound: Option<crate::acoustics::ImpactParams>, pub struck_time_s: f32,
    born: f32, origin: Vec3, axes: [Vec3; 3], line: chain::ShotLine,
    plate: lattice::Lattice, shot: shot::Shot,
    plate_released: Vec<bool>, shot_released: Vec<bool>,
    previous_tip: Vec3, shot_mass_released: f32, scar_recorded: bool,
    steps: u64, wall_ms: f32, flash_emitted: bool, burst_emitted: bool, primary_transferred:bool,
}

impl std::fmt::Debug for TemporalImpact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemporalImpact").field("id", &self.context.penetration_id)
            .field("plate_time", &self.plate.t).field("shot_time", &self.shot.t_s).finish()
    }
}

impl TemporalImpact {
    pub fn checkpoint(&self)->TemporalSaved {
        TemporalSaved{context:self.context,facing:self.armor.facing,normal:self.armor.world_normal,
            born:self.born,struck:self.struck_time_s,origin:self.origin,axes:self.axes,
            plate:self.plate.impact,particle_budget:self.plate.n,cells_across:self.shot.impact.design.calibre_m/self.shot.h,
            plate_steps:self.plate.step,shot_steps:self.steps,plate_released:self.plate_released.clone(),
            shot_released:self.shot_released.clone(),previous_tip:self.previous_tip,shot_mass_released:self.shot_mass_released,
            scar_recorded:self.scar_recorded,flash_emitted:self.flash_emitted,burst_emitted:self.burst_emitted,primary_transferred:self.primary_transferred}
    }
    pub fn restore(s:TemporalSaved)->anyhow::Result<Self> {
        anyhow::ensure!(s.plate_steps<100000 && s.shot_steps<100000 && s.cells_across.is_finite() && s.cells_across>0. && s.cells_across<=16.,"invalid live impact checkpoint");
        let mut plate=lattice::Lattice::with_particle_budget(s.plate,s.particle_budget);
        plate.set_threads(1);
        if crate::ballistics::is_hesh(s.context.event.shell){plate.apply_surface_impulse(s.context.event.shell.filler_g*0.001*compartment::TNT_J_PER_KG*0.015,s.context.event.shell.caliber_mm*0.001);}
        plate.advance(s.plate_steps);
        let impact=shot::impact_of(&s.context.event,s.context.seed^0x5407_5407).ok_or_else(||anyhow::anyhow!("missing kinetic shot"))?;
        let mut shot=shot::Shot::with_cells(impact,s.cells_across);shot.advance_steps(s.shot_steps);
        anyhow::ensure!(s.plate_released.len()==plate.n && s.shot_released.len()==shot.len(),"live impact topology differs from checkpoint");
        let line=chain::ShotLine{entry:s.origin,dir:s.axes[0]*s.plate.obliquity_rad.sin()+s.axes[2]*s.plate.obliquity_rad.cos(),
            plate_path_m:s.plate.thickness_m/s.plate.obliquity_rad.cos().max(0.1),far_wall:None,max_s:10.};
        let armor=VehicleArmorContact{vehicle:ProjectileVehicle::Player,part:s.context.armor_part,facing:s.facing,
            local_point:s.context.armor_local_point,world_normal:s.normal,surface_plate:None};
        Ok(Self{context:s.context,armor,sound:None,struck_time_s:s.struck,born:s.born,origin:s.origin,axes:s.axes,line,
            plate,shot,plate_released:s.plate_released,shot_released:s.shot_released,previous_tip:s.previous_tip,
            shot_mass_released:s.shot_mass_released,scar_recorded:s.scar_recorded,steps:s.shot_steps,wall_ms:0.,
            flash_emitted:s.flash_emitted,burst_emitted:s.burst_emitted,primary_transferred:s.primary_transferred})
    }
    fn to_hull(&self) -> Mat4 {
        Mat4::from_cols(self.axes[0].extend(0.), self.axes[1].extend(0.),
            self.axes[2].extend(0.), self.origin.extend(1.))
    }
    pub fn view(&self) -> TemporalView {
        TemporalView {
            origin:self.origin, axes:self.axes, h:self.plate.h,
            half:[self.plate.dims[0] as f32*self.plate.h*0.5,
                self.plate.dims[1] as f32*self.plate.h*0.5,self.plate.impact.thickness_m],
            positions:(0..self.plate.n).map(|i|self.plate.position(i)).collect(),
            temperatures:self.plate.temp.clone(), retained:self.plate_released.iter().map(|r|!r).collect(),
            shot_h:self.shot.h, shot_positions:self.shot.pos.clone(), shot_temperatures:self.shot.temp.clone(),
            shot_retained:self.shot_released.iter().map(|r|!r).collect(),
            t:self.shot.t_s.max(self.plate.t), born:self.born, steps:self.steps,
            initial_shot_mass:self.shot.mass.iter().sum(), released_shot_mass:self.shot_mass_released,
        }
    }
}

/// A rewind restores material state only up to the saved step, never to an impact's end.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TemporalSaved {
    context:LiveImpactContext,facing:crate::ballistics::Facing,normal:Vec3,
    born:f32,struck:f32,origin:Vec3,axes:[Vec3;3],plate:lattice::PlateImpact,
    particle_budget:usize,cells_across:f32,plate_steps:u64,shot_steps:u64,
    plate_released:Vec<bool>,shot_released:Vec<bool>,previous_tip:Vec3,
    shot_mass_released:f32,scar_recorded:bool,flash_emitted:bool,burst_emitted:bool,
    #[serde(default)] primary_transferred:bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temporal_burst_conserves_mass_momentum_energy_and_tracer_share() {
        let body=HotFragment{active:true,mass_kg:6.,tracer_share:0.75,
            vel:Vec3::new(20.,-2.,500.),penetration_id:47,size:0.09,..Default::default()};
        let out=split_burst_body(body,0.1,80000.);
        let mass:f32=out.iter().map(fragment_mass).sum();
        let momentum:Vec3=out.iter().map(|p|p.vel*fragment_mass(p)).sum();
        let energy:f32=out.iter().map(|p|0.5*fragment_mass(p)*(p.vel-body.vel).length_squared()).sum();
        let share:f32=out.iter().map(|p|p.tracer_share).sum();
        assert!((mass-5.9).abs()<1e-5);
        assert!(momentum.distance(body.vel*5.9)<0.002);
        assert!((energy-80000.).abs()<0.1);
        assert!((share-body.tracer_share*5.9/6.).abs()<1e-6);
        let ids:std::collections::BTreeSet<_>=out.iter().map(|p|p.id).collect();
        assert_eq!(ids.len(),out.len());
    }
    #[test]
    fn temporal_shot_advances_only_elapsed_cfl_steps_and_partition_is_invariant() {
        let event=crate::ballistics::resolve(&crate::ballistics::PZGR_39,100.,
            crate::ballistics::ArmorScheme::TigerI.plate(crate::ballistics::Facing::HullFront),0.,7);
        let impact=shot::impact_of(&event,7).unwrap();
        let mut a=shot::Shot::with_cells(impact,3.);
        let mut b=shot::Shot::with_cells(impact,3.);
        let before=a.pos.clone();
        assert_eq!(a.advance_until(a.dt*0.5),0);
        assert_eq!(a.pos,before);
        let target=a.dt*24.25;
        a.advance_until(target);
        for i in 1..=24 {b.advance_until(b.dt*(i as f32+0.25));}
        assert!(a.t_s<=target && target-a.t_s<a.dt);
        assert_eq!(a.pos,b.pos);assert_eq!(a.vel,b.vel);assert_eq!(a.temp,b.temp);
        let pieces=a.pieces();
        let total:f32=pieces.iter().map(|p|p.mass_kg).sum();
        let original:f32=a.mass.iter().sum();
        assert!((total-original).abs()<1e-4);
    }
    #[test]
    fn temporal_checkpoint_restores_the_current_step_without_future_damage() {
        let event=crate::ballistics::resolve(&crate::ballistics::PZGR_39,100.,
            crate::ballistics::ArmorScheme::TigerI.plate(crate::ballistics::Facing::HullFront),0.,7);
        let impact=lattice::PlateImpact{material:crate::fea_damage::materials::MaterialId::RolledArmour,
            thickness_m:0.1,obliquity_rad:0.,projectile:lattice::ProjectileShape{mass_kg:event.shell.mass_kg,radius_m:0.044,nose_m:0.08},
            v_in:event.striking_v,motion:lattice::Motion::Perforate{v_out:event.residual_v},seed:7};
        let plate=lattice::Lattice::with_particle_budget(impact,1500);
        let shot=shot::Shot::with_cells(shot::impact_of(&event,7^0x5407_5407).unwrap(),3.);
        let mut state=TemporalImpact::restore(TemporalSaved{
            context:LiveImpactContext{arc:Arc::Front,shooter:Vec3::Z*10.,entry_w:Vec3::ZERO,shell_dir_w:Vec3::Z,
                armor_part:ArmorPart::Hull,armor_local_point:Vec3::ZERO,penetration_id:47,seed:7,event},
            facing:crate::ballistics::Facing::HullFront,normal:Vec3::NEG_Z,born:2.,struck:2.,origin:Vec3::ZERO,
            axes:[Vec3::X,Vec3::Y,Vec3::Z],plate:impact,particle_budget:1500,cells_across:3.,plate_steps:0,shot_steps:0,
            plate_released:vec![false;plate.n],shot_released:vec![false;shot.len()],previous_tip:Vec3::ZERO,
            shot_mass_released:0.,scar_recorded:false,flash_emitted:false,burst_emitted:false,primary_transferred:false}).unwrap();
        state.plate.advance(17);state.shot.advance_steps(23);state.steps=23;
        let bytes=serde_json::to_vec(&state.checkpoint()).unwrap();
        let restored=TemporalImpact::restore(serde_json::from_slice(&bytes).unwrap()).unwrap();
        assert_eq!(restored.plate.ux,state.plate.ux);assert_eq!(restored.plate.temp,state.plate.temp);
        assert_eq!(restored.shot.pos,state.shot.pos);assert_eq!(restored.shot.vel,state.shot.vel);
        assert_eq!(restored.shot_mass_released,0.);assert!(!restored.flash_emitted&&!restored.burst_emitted);
    }
}

impl Game {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::game) fn fea_begin_penetration(&mut self, projectile_id:u64, seed:u32,
        armor:&VehicleArmorContact, event:&crate::ballistics::ImpactEvent,
        impact_world:Vec3, dir_world:Vec3, origin_world:Vec3, arc:Arc, now_s:f32) -> Option<bool> {
        let shot_impact=shot::impact_of(event,seed ^ 0x5407_5407)?;
        let line=self.fea_shot_line(impact_world,dir_world,event);
        let normal=self.player.hull_model().inverse().transform_vector3(armor.world_normal).normalize_or_zero();
        let mut e3=-normal;
        if e3.dot(line.dir)<0. {e3=-e3;}
        let tangent=line.dir-e3*line.dir.dot(e3);
        let e1=tangent.try_normalize().unwrap_or_else(||e3.any_orthonormal_vector());
        let axes=[e1,e3.cross(e1),e3];
        let hesh=crate::ballistics::is_hesh(event.shell);
        let chemical=event.shell.kind==crate::ballistics::ShellKind::Heat;
        let calibre=shot_impact.design.calibre_m;
        let radius=calibre*0.5*if event.shell.kind==crate::ballistics::ShellKind::Apcr {APCR_CORE_FRACTION}else{1.};
        let motion=if hesh {lattice::Motion::Stop{depth_m:shot_impact.thickness_m}}else if event.terminal==crate::ballistics::Terminal::Penetration {
            lattice::Motion::Perforate{v_out:if chemical{shot_impact.v_in*(1.-event.effective_mm/event.pen_mm).max(0.).sqrt()}else{event.residual_v}}
        }else{lattice::Motion::Stop{depth_m:(event.pen_mm*0.001).min(event.plate.thickness_mm*0.001)}};
        let impact=lattice::PlateImpact {
            material:if event.plate.material==crate::ballistics::ArmorMaterial::Cast {
                crate::fea_damage::materials::MaterialId::CastArmour
            }else{crate::fea_damage::materials::MaterialId::RolledArmour},
            thickness_m:shot_impact.thickness_m, obliquity_rad:line.dir.dot(e3).clamp(-1.,1.).acos().min(1.4),
            projectile:lattice::ProjectileShape{mass_kg:shot_impact.design.mass_kg,radius_m:if hesh{event.shell.caliber_mm*0.001}else{radius},
                nose_m:lodged::nose_length_calibres(event.shell.kind)*radius*2.},
            v_in:shot_impact.v_in,motion,seed,
        };
        let reference=super::super::crew_tissue::reference_solve();
        let mut plate=lattice::Lattice::with_particle_budget(impact,if reference{lattice::MAX_PARTICLES}else{1500});
        plate.set_threads(1); // short CFL slices: no new thread team on every output step
        if hesh {plate.apply_surface_impulse(event.shell.filler_g*0.001*compartment::TNT_J_PER_KG*0.015,event.shell.caliber_mm*0.001);}
        let shot=shot::Shot::with_cells(shot_impact,if reference{SHOT_CELLS_ACROSS}else{3.});
        let tip=shot.pos.iter().copied().max_by(|a,b|a.z.total_cmp(&b.z)).unwrap_or(Vec3::ZERO);
        let context=LiveImpactContext {arc,shooter:origin_world,entry_w:impact_world,shell_dir_w:dir_world,
            armor_part:armor.part,armor_local_point:armor.local_point,penetration_id:projectile_id,seed,event:*event};
        self.fea.live_contexts.insert(projectile_id,context);
        self.fea.current=None;
        self.fea.penetrations.push(TemporalImpact {
            context,armor:*armor,sound:None,struck_time_s:now_s,born:now_s,origin:line.entry,axes,line,
            plate_released:vec![false;plate.n],shot_released:vec![false;shot.len()],plate,shot,
            previous_tip:tip,shot_mass_released:0.,scar_recorded:false,steps:0,wall_ms:0.,flash_emitted:false,burst_emitted:false,primary_transferred:false,
        });
        println!("[temporal] impact id={projectile_id} t={now_s:.9}; no downstream solve or damage committed");
        Some(false)
    }

    pub(in crate::game) fn commit_fea_penetrations(&mut self) {
        let mut active=std::mem::take(&mut self.fea.penetrations);
        for p in &mut active {
            let until=(self.armor_clock_s-p.born).max(0.);
            let started=std::time::Instant::now();
            let plate_steps=((until-p.plate.t)/p.plate.dt).floor().max(0.) as u64;
            p.plate.advance(plate_steps);
            // Keep the deformed shot live until its cells have left the plate or settled.
            // Each call takes only the elapsed physical interval, never a terminal solve.
            let horizon=2.5*(p.shot.length_m+2.*p.shot.impact.thickness_m+p.shot.impact.design.calibre_m)/p.shot.impact.v_in.max(100.);
            p.steps+=p.shot.advance_until(until.min(horizon));
            p.wall_ms+=started.elapsed().as_secs_f32()*1000.;
            let transform=self.player.hull_model()*p.to_hull();
            let frame=BloodFrame::Hull;
            let to_frame=self.player.hull_model().inverse()*transform;
            let mut released=Vec::new();
            let make=|id:u64,pos:Vec3,vel:Vec3,size:f32,spin:Vec3,temp:f32,mass:f32,share:f32| HotFragment {
                id,penetration_id:p.context.penetration_id,active:true,settled:false,
                origin:to_frame.transform_point3(pos),prev_pos:to_frame.transform_point3(pos),
                entered_compartment:true,audio_whizzed:false,frame,pos:to_frame.transform_point3(pos),
                vel:to_frame.transform_vector3(vel),size,ang:Vec3::ZERO,spin:to_frame.transform_vector3(spin),
                age:0.,life:f32::MAX,bounces:0,seed:p.context.seed^(id as u32),temp_k:temp,
                lodged:false,embed_m:0.,mass_kg:mass,cross_section_m2:0.0,tracer_share:share, material_remaining_m:0.0,material_force_n:0.0,
            };
            // A disconnected group transfers once, only once all of its cells clear the back
            // face. Until then those exact moving cells remain in the local material solve.
            for cluster in p.plate.clusters() {
                if cluster.particles.iter().any(|&i|p.plate_released[i as usize]) {continue;}
                if !cluster.particles.iter().all(|&i|p.plate.position(i as usize).z-p.plate.h*0.5>p.plate.impact.thickness_m) {continue;}
                let id=(p.context.penetration_id<<24)|(1<<23)|cluster.particles[0] as u64;
                released.push(make(id,cluster.centre,cluster.velocity,cluster.size_m,cluster.angular_velocity,
                    cluster.temperature_k,cluster.mass_kg,0.));
                for &i in &cluster.particles {p.plate_released[i as usize]=true;}
            }
            let local_complete=p.plate.done && p.shot.t_s+p.shot.dt>=horizon;
            let pieces=p.shot.pieces();
            for (index,piece) in pieces.iter().enumerate() {
                if piece.cells.iter().any(|&i|p.shot_released[i as usize]) {continue;}
                if !piece.cells.iter().all(|&i|p.shot.pos[i as usize].z-p.shot.h*0.5>p.shot.impact.thickness_m)
                    && !(piece.velocity.z<0. && piece.cells.iter().all(|&i|p.shot.pos[i as usize].z+p.shot.h*0.5<0.)) && !local_complete {continue;}
                let primary=index==0 && !p.primary_transferred;
                let id=if primary {p.context.penetration_id}else{(p.context.penetration_id<<24)|(1<<22)|piece.cells[0] as u64};
                let mut body=make(id,piece.centre,piece.velocity,piece.size_m,piece.spin,piece.temperature_k,
                    piece.mass_kg,piece.mass_kg/p.context.event.shell.mass_kg);
                // Frontal area from retained voxels, not the bounding cube of a long rod.
                let footprint:std::collections::HashSet<_>=piece.cells.iter().map(|&i|{
                    let p=p.shot.pos[i as usize]/p.shot.h;(p.x.round()as i32,p.y.round()as i32)
                }).collect();
                body.cross_section_m2=footprint.len()as f32*p.shot.h*p.shot.h;
                if local_complete && piece.centre.z>=0. && piece.centre.z<=p.shot.impact.thickness_m {
                    if piece.velocity.length()<1. || !matches!(p.plate.impact.motion,lattice::Motion::Perforate{..}) {
                        body.settled=true;body.lodged=true;body.vel=Vec3::ZERO;
                    }else{
                        // The deformed residual continues down the bore it has already opened.
                        // Retain its current position and energy; there is no exit teleport.
                        let distance=if piece.velocity.z>=0. {p.shot.impact.thickness_m-piece.centre.z}else{piece.centre.z};
                        body.material_remaining_m=distance/(piece.velocity.z.abs()/piece.velocity.length()).max(0.01);
                        body.material_force_n=0.;
                    }
                }
                if primary {p.primary_transferred=true;}
                if primary && piece.centre.z>p.shot.impact.thickness_m && !p.burst_emitted && p.context.event.shell.filler_g>10. && p.context.event.shell.kind!=crate::ballistics::ShellKind::Heat && !crate::ballistics::is_hesh(p.context.event.shell) {
                    p.burst_emitted=true;
                    released.extend(self.begin_live_burst(p.context,body));
                }else{released.push(body);}
                self.fea.fragment_shapes.insert(id,(p.shot.h,piece.cells.iter().map(|&i|to_frame.transform_vector3(p.shot.pos[i as usize]-piece.centre)).collect()));
                p.shot_mass_released+=piece.mass_kg;
                for &i in &piece.cells {p.shot_released[i as usize]=true;}
                println!("[temporal] shell transfer t={:.9} id={id} mass={:.6} kg speed={:.2}; total released={:.6}",
                    self.armor_clock_s,piece.mass_kg,piece.velocity.length(),p.shot_mass_released);
            }
            if let Some(piece)=pieces.first().filter(|p0|!p.primary_transferred && p0.cells.iter().all(|&i|!p.shot_released[i as usize])) {
                let tip=piece.cells.iter().map(|&i|p.shot.pos[i as usize]).max_by(|a,b|a.z.total_cmp(&b.z)).unwrap();
                let old=transform.transform_point3(p.previous_tip);
                let new=transform.transform_point3(tip);
                let dt=old.distance(new)/piece.velocity.length().max(1.);
                self.weapon_vfx.record_fragment_tracer(p.context.penetration_id,piece.mass_kg/p.context.event.shell.mass_kg,old,new,
                    self.armor_clock_s-dt,self.armor_clock_s);
                if tip.z > p.shot.impact.thickness_m {
                    let from=if p.previous_tip.z<p.shot.impact.thickness_m {
                        p.previous_tip.lerp(tip,((p.shot.impact.thickness_m-p.previous_tip.z)/(tip.z-p.previous_tip.z)).clamp(0.,1.))
                    }else{p.previous_tip};
                    let mut context=p.context;context.armor_part=ArmorPart::Hull;context.armor_local_point=p.origin;
                    self.deliver_material_front(context,make(p.context.penetration_id,tip,piece.velocity,piece.size_m,
                        piece.spin,piece.temperature_k,piece.mass_kg,piece.mass_kg/p.context.event.shell.mass_kg),
                        to_frame.transform_point3(from),to_frame.transform_point3(tip));
                }
                p.previous_tip=tip;
            }
            if !released.is_empty() {
                // Context must be in the same frame as the newly transferred physical bodies.
                let mut context=p.context;
                context.armor_part=ArmorPart::Hull;
                context.armor_local_point=p.origin;
                self.fea.live_contexts.insert(context.penetration_id,context);
                self.stage_penetration_fragments(&released);
            }
            if !p.flash_emitted && p.shot.pos.iter().any(|v|v.z>p.shot.impact.thickness_m) {
                p.flash_emitted=true;
                let at=transform.transform_point3(Vec3::new(0.,0.,p.shot.impact.thickness_m));
                let ev=p.context.event;
                let deposited=0.5*ev.shell.mass_kg*(ev.striking_v.powi(2)-ev.residual_v.powi(2)).max(0.);
                self.weapon_vfx.interior_spall_sparks(at,p.context.shell_dir_w,deposited*0.006,
                    weapon_vfx::Weapon::Cannon(ev.shell.caliber_mm as u16));
                println!("[temporal] back-face crossing t={:.9} impact={}",self.armor_clock_s,p.context.penetration_id);
            }
            if p.plate.done && !p.scar_recorded {
                p.scar_recorded=true;
                let outcome=lattice_outcome(&mut p.plate,p.origin,p.axes,p.wall_ms);
                let chain=chain::ChainResult{crossings:Vec::new(),hits:Vec::new(),lodged:None,exit_speed:None,
                    burst:false,energy_in_j:0.,energy_removed_j:0.};
                self.fea.current=Some(ImpactSolve{projectile_id:p.context.penetration_id,line:p.line,chain,lattice:Some(outcome),shot:None});
                if !self.god {
                    self.record_vehicle_scar(&p.armor,&p.context.event,p.context.shell_dir_w,p.struck_time_s);
                    self.apply_exterior_strike(p.armor,&p.context.event,p.flash_emitted,p.context.seed,p.context.shell_dir_w);
                }
                self.fea.current=None;
            }
            self.fea.temporal_view=Some(p.view());
        }
        active.retain(|p| !p.plate.done || p.shot.t_s+p.shot.dt <
            2.5*(p.shot.length_m+2.*p.shot.impact.thickness_m+p.shot.impact.design.calibre_m)/p.shot.impact.v_in.max(100.));
        self.fea.penetrations=active;
    }
}
