//! Unconnected lumped engine/coolant circuit, not an exhaust-color or horsepower tier.
//! Tigerfibel pp.18–19: 120 litres and a 95 C stop/check instruction. Mass/density, heat
//! capacities, heat split, pump conductance, fan coefficients and failure exposure below are
//! engineering estimates, NOT measured HL230 data. No fluid pressure, freeze, oil or CFD solve.
//! Fixed-geometry fan laws: Q~rpm and shaft power~rpm^3 (DOE Fan System Sourcebook,
//! https://www.energy.gov/sites/default/files/2014/05/f16/fan_sourcebook.pdf, glossary).
//! Caller must supply source-measured radii and physically installed drive ratios and subtract
//! returned fan load in the existing shaft solver. This module grants no parts and changes no fuel.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};

#[derive(Clone,Copy,Debug,Serialize,Deserialize)]
pub struct CoolingParams {
    pub engine_capacity_j_k:f64, pub coolant_cp_j_kg_k:f64,
    pub pump_conductance_w_k:f64, pub stopped_conductance_w_k:f64,
    pub radiator_effectiveness:f64, pub radiator_conductance_w_k:f64,
    /// Estimated passive radiator-to-air UA; natural convection/radiation are lumped here.
    /// This does not spin a fan, manufacture fan airflow, or consume shaft power.
    pub passive_radiator_conductance_w_k:f64,
    pub air_density_kg_m3:f64, pub air_cp_j_kg_k:f64,
    pub fan_flow_coefficient:f64, pub fan_pressure_coefficient:f64, pub fan_efficiency:f64,
    pub idle_heat_w:f64, pub heat_per_shaft_w:f64,
    pub boil_c:f64, pub vapor_latent_j_kg:f64,
    pub damage_above_c:f64, pub failure_exposure_k_s:f64,
}
impl Default for CoolingParams {fn default()->Self {Self {
    engine_capacity_j_k:600_000.,coolant_cp_j_kg_k:3_800.,
    pump_conductance_w_k:24_000.,stopped_conductance_w_k:250.,
    radiator_effectiveness:0.72,radiator_conductance_w_k:8_000.,passive_radiator_conductance_w_k:150.,
    air_density_kg_m3:1.2,air_cp_j_kg_k:1_005.,
    fan_flow_coefficient:0.12,fan_pressure_coefficient:0.60,fan_efficiency:0.65,
    idle_heat_w:25_000.,heat_per_shaft_w:0.90,boil_c:105.,vapor_latent_j_kg:2_260_000.,
    damage_above_c:130.,failure_exposure_k_s:600.,
}}}
impl CoolingParams {fn validate(self)->Result<()> {
    ensure!([self.engine_capacity_j_k,self.coolant_cp_j_kg_k,self.air_density_kg_m3,
        self.air_cp_j_kg_k,self.fan_efficiency,self.vapor_latent_j_kg,self.failure_exposure_k_s]
        .iter().all(|x|x.is_finite()&&*x>0.),"positive finite cooling parameters required");
    ensure!([self.pump_conductance_w_k,self.stopped_conductance_w_k,self.fan_flow_coefficient,
        self.fan_pressure_coefficient,self.idle_heat_w,self.heat_per_shaft_w,self.radiator_conductance_w_k,
        self.passive_radiator_conductance_w_k]
        .iter().all(|x|x.is_finite()&&*x>=0.),"nonnegative finite cooling parameters required");
    ensure!(self.radiator_effectiveness.is_finite()&&(0.0..=1.0).contains(&self.radiator_effectiveness)
        &&self.fan_efficiency<=1.&&self.boil_c.is_finite()&&self.boil_c>0.
        &&self.damage_above_c.is_finite()&&self.damage_above_c>self.boil_c,"invalid cooling limits");Ok(())
}}
#[derive(Clone,Copy,Debug,Default)]
pub struct FanDrive {pub radius_m:f64,pub ratio:f64,pub healthy:bool}
#[derive(Clone,Copy,Debug)]
pub struct CoolingInput {
    pub running:bool, pub rpm:f64, pub shaft_power_w:f64, pub ambient_c:f64,
    pub fans:[FanDrive;4], pub coolant_connected:bool, pub radiator_connected:bool,
    /// A confirmed open coolant leak, in kg/s; zero means sealed. It never draws replacement fluid.
    pub leak_kg_s:f64,
}
impl Default for CoolingInput {fn default()->Self {Self {running:false,rpm:0.,shaft_power_w:0.,
    ambient_c:20.,fans:[FanDrive::default();4],coolant_connected:true,radiator_connected:true,leak_kg_s:0.}}}
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
pub struct CoolingState {
    /// Sensible energies relative to 0 C. Negative energy is valid for cold, unfrozen coolant.
    pub engine_j:f64,pub coolant_j:f64,pub coolant_kg:f64,
    pub boiled_kg:f64,pub leaked_kg:f64,pub heat_exposure_k_s:f64,pub failed:bool,
}
#[derive(Clone,Copy,Debug,Default,Serialize,Deserialize)]
pub struct CoolingStep {
    pub engine_c:f64,pub coolant_c:Option<f64>,pub airflow_m3_s:f64,pub fan_rpm:[f64;4],
    pub fan_shaft_load_w:f64,pub shaft_power_after_fans_w:f64,pub shaft_deficit_w:f64,
    /// Energy audit: delta(stored)=heat_in - air_out - escaped_fluid. air_out may be negative.
    pub heat_in_j:f64,pub air_out_j:f64,pub escaped_fluid_j:f64,pub balance_error_j:f64,
    pub boiled_kg:f64,pub leaked_kg:f64,pub warning:bool,pub failed:bool,
}
impl CoolingState {
    pub fn new(temperature_c:f64,coolant_kg:f64,p:&CoolingParams)->Result<Self> {
        p.validate()?;ensure!(temperature_c.is_finite()&&coolant_kg.is_finite()&&coolant_kg>=0.,"invalid initial coolant");
        let state=Self {engine_j:temperature_c*p.engine_capacity_j_k,coolant_j:temperature_c*coolant_kg*p.coolant_cp_j_kg_k,
            coolant_kg,boiled_kg:0.,leaked_kg:0.,heat_exposure_k_s:0.,failed:false};state.validate()?;Ok(state)
    }
    pub fn validate(&self)->Result<()> {
        ensure!(self.engine_j.is_finite()&&self.coolant_j.is_finite(),"nonfinite stored thermal energy");
        ensure!([self.coolant_kg,self.boiled_kg,self.leaked_kg,self.heat_exposure_k_s].iter()
            .all(|x|x.is_finite()&&*x>=0.),"invalid coolant mass/exposure");
        ensure!(self.coolant_kg>0.||self.coolant_j==0.,"empty coolant circuit retains energy");Ok(())
    }
    pub fn engine_c(&self,p:&CoolingParams)->f64 {self.engine_j/p.engine_capacity_j_k}
    pub fn coolant_c(&self,p:&CoolingParams)->Option<f64> {
        (self.coolant_kg>0.).then(||self.coolant_j/(self.coolant_kg*p.coolant_cp_j_kg_k))
    }
    pub fn advance(&mut self,dt_s:f64,input:CoolingInput,p:&CoolingParams)->Result<CoolingStep> {
        // Invalid persisted/input data cannot leave a partly advanced owner behind.
        let mut next=self.clone();let out=next.advance_inner(dt_s,input,p)?;*self=next;Ok(out)
    }
    fn advance_inner(&mut self,dt_s:f64,input:CoolingInput,p:&CoolingParams)->Result<CoolingStep> {
        self.validate()?;p.validate()?;
        ensure!(dt_s.is_finite()&&(0.0..=3_600.0).contains(&dt_s),"cooling step must be 0..3600 seconds");
        ensure!(input.rpm.is_finite()&&(0.0..=6_000.0).contains(&input.rpm)
            &&input.shaft_power_w.is_finite()&&(0.0..=2_000_000.0).contains(&input.shaft_power_w)
            &&input.ambient_c.is_finite()&&(-80.0..=80.0).contains(&input.ambient_c)
            &&input.leak_kg_s.is_finite()&&(0.0..=120.0).contains(&input.leak_kg_s),"input outside cooling model domain");
        ensure!(input.fans.iter().all(|f|f.radius_m.is_finite()&&(0.0..=1.0).contains(&f.radius_m)
            &&f.ratio.is_finite()&&(0.0..=4.0).contains(&f.ratio)),"fan outside cooling model domain");
        let before=self.engine_j+self.coolant_j;let mut out=CoolingStep::default();
        // Exact passive node exchange prevents temperature overshoot; <=50ms splitting bounds
        // heat-input/radiator/leak ordering. All requested elapsed time is consumed, never dropped.
        let count=(dt_s/0.05).ceil().max(1.) as usize;let h=dt_s/count as f64;
        for _ in 0..count {
            let running=input.running&&!self.failed;
            let mut flow=0.;let mut load=0.;let mut speeds=[0.;4];
            for (index,fan) in input.fans.iter().enumerate() {
                if !running||!fan.healthy {continue;}
                let rpm=input.rpm*fan.ratio;let tip=std::f64::consts::TAU*fan.radius_m*rpm/60.;
                let q=std::f64::consts::PI*fan.radius_m.powi(2)*tip*p.fan_flow_coefficient;
                let pressure=0.5*p.air_density_kg_m3*tip*tip*p.fan_pressure_coefficient;
                flow+=q;load+=pressure*q/p.fan_efficiency;speeds[index]=rpm;
            }
            out.airflow_m3_s=flow;out.fan_shaft_load_w=load;out.fan_rpm=speeds;
            let shaft=if running {input.shaft_power_w}else{0.};
            out.shaft_power_after_fans_w=(shaft-load).max(0.);out.shaft_deficit_w=(load-shaft).max(0.);
            let added=if running {(p.idle_heat_w+p.heat_per_shaft_w*shaft)*h}else{0.};
            self.engine_j+=added;out.heat_in_j+=added;
            if self.coolant_kg>0. {
                let capacity=self.coolant_kg*p.coolant_cp_j_kg_k;
                if input.coolant_connected {
                    let conductance=if running {p.pump_conductance_w_k*(input.rpm/3_000.).min(1.5)}else{p.stopped_conductance_w_k};
                    let reciprocal=1./p.engine_capacity_j_k+1./capacity;
                    let transfer=(self.engine_c(p)-self.coolant_j/capacity)/reciprocal
                        *(-(-conductance*reciprocal*h).exp_m1());
                    self.engine_j-=transfer;self.coolant_j+=transfer;
                }
                if input.radiator_connected {
                    let air_capacity=(flow*p.air_density_kg_m3*p.air_cp_j_kg_k).min(p.radiator_conductance_w_k);
                    let conductance=(p.passive_radiator_conductance_w_k+p.radiator_effectiveness*air_capacity)
                        .min(p.radiator_conductance_w_k);
                    let transfer=(self.coolant_j-capacity*input.ambient_c)
                        *(-(-conductance*h/capacity).exp_m1());
                    self.coolant_j-=transfer;out.air_out_j+=transfer;
                }
                let leaked=(input.leak_kg_s*h).min(self.coolant_kg);
                let lost=self.coolant_j*(leaked/self.coolant_kg);
                self.coolant_kg-=leaked;self.coolant_j-=lost;out.escaped_fluid_j+=lost;
                self.leaked_kg+=leaked;out.leaked_kg+=leaked;
                if self.coolant_kg>0.&&h>0. {
                    let excess=(self.coolant_j-self.coolant_kg*p.coolant_cp_j_kg_k*p.boil_c).max(0.);
                    let boiled=(excess/p.vapor_latent_j_kg).min(self.coolant_kg);
                    let lost=if boiled==self.coolant_kg {self.coolant_j}
                        else {boiled*(p.coolant_cp_j_kg_k*p.boil_c+p.vapor_latent_j_kg)};
                    self.coolant_kg-=boiled;self.coolant_j-=lost;out.escaped_fluid_j+=lost;
                    self.boiled_kg+=boiled;out.boiled_kg+=boiled;
                }
                if self.coolant_kg==0. {self.coolant_j=0.;}
            }
            if running {self.heat_exposure_k_s+=(self.engine_c(p)-p.damage_above_c).max(0.)*h;}
            if h>0. {self.failed|=self.heat_exposure_k_s>=p.failure_exposure_k_s;}
        }
        out.engine_c=self.engine_c(p);out.coolant_c=self.coolant_c(p);
        out.warning=out.coolant_c.is_some_and(|t|t>=95.)||out.engine_c>=105.;out.failed=self.failed;
        if self.failed {out.airflow_m3_s=0.;out.fan_rpm=[0.;4];out.fan_shaft_load_w=0.;
            out.shaft_power_after_fans_w=0.;out.shaft_deficit_w=0.;}
        out.balance_error_j=(self.engine_j+self.coolant_j-before)-(out.heat_in_j-out.air_out_j-out.escaped_fluid_j);
        ensure!([out.engine_c,out.airflow_m3_s,out.fan_shaft_load_w,out.shaft_power_after_fans_w,
            out.shaft_deficit_w,out.heat_in_j,out.air_out_j,out.escaped_fluid_j,out.balance_error_j]
            .iter().all(|v|v.is_finite())&&out.coolant_c.is_none_or(f64::is_finite),"nonfinite cooling result");
        self.validate()?;Ok(out)
    }
}

#[cfg(test)] mod tests {
    use super::*;
    fn driving(ratio:f64)->CoolingInput {CoolingInput {running:true,rpm:2_000.,shaft_power_w:300_000.,
        fans:[FanDrive{radius_m:0.25,ratio,healthy:true};4],..Default::default()}}
    #[test] fn sealed_exchange_conserves_energy_and_partitioned_time() {
        let p=CoolingParams {idle_heat_w:0.,heat_per_shaft_w:0.,boil_c:200.,damage_above_c:250.,..Default::default()};
        let mut one=CoolingState::new(20.,120.,&p).unwrap();one.engine_j=100.*p.engine_capacity_j_k;
        let mut many=one.clone();let total=one.engine_j+one.coolant_j;
        let input=CoolingInput{running:true,rpm:2_000.,radiator_connected:false,..Default::default()};
        let step=one.advance(10.,input,&p).unwrap();for _ in 0..600 {many.advance(1./60.,input,&p).unwrap();}
        assert!(step.balance_error_j.abs()<1e-6);assert!((one.engine_j+one.coolant_j-total).abs()<1e-6);
        assert!((one.engine_c(&p)-many.engine_c(&p)).abs()<1e-9);
    }
    #[test] fn fan_ratio_increases_flow_and_cubic_cost_and_reduces_heat() {
        let p=CoolingParams::default();let mut stock=CoolingState::new(80.,120.,&p).unwrap();let mut fast=stock.clone();
        let a=stock.advance(0.05,driving(1.),&p).unwrap();let b=fast.advance(0.05,driving(1.2),&p).unwrap();
        assert!((b.airflow_m3_s/a.airflow_m3_s-1.2).abs()<1e-12);
        assert!((b.fan_shaft_load_w/a.fan_shaft_load_w-1.2_f64.powi(3)).abs()<1e-12);
        assert!(b.shaft_power_after_fans_w<a.shaft_power_after_fans_w);assert!(b.coolant_c<a.coolant_c);
        let off=fast.advance(1.,CoolingInput{running:false,..driving(1.2)},&p).unwrap();
        assert_eq!((off.airflow_m3_s,off.fan_shaft_load_w,off.heat_in_j),(0.,0.,0.));
    }
    #[test] fn finite_leak_and_boil_export_energy_without_refill() {
        let p=CoolingParams::default();let mut s=CoolingState::new(120.,1.,&p).unwrap();
        let step=s.advance(1.,CoolingInput{leak_kg_s:0.2,..Default::default()},&p).unwrap();
        assert!(step.boiled_kg>0.&&step.leaked_kg>0.);assert!(step.balance_error_j.abs()<1e-6);
        let step=s.advance(1.,CoolingInput{leak_kg_s:10.,..Default::default()},&p).unwrap();
        assert_eq!((s.coolant_kg,s.coolant_j),(0.,0.));assert!(step.balance_error_j.abs()<1e-6);
        let step=s.advance(5.,driving(1.),&p).unwrap();assert!(step.coolant_c.is_none());
        assert_eq!(s.coolant_kg,0.);assert_eq!(step.air_out_j,0.);
        let restored:CoolingState=bincode::deserialize(&bincode::serialize(&s).unwrap()).unwrap();assert_eq!(restored,s);
    }
    #[test] fn failed_drive_and_disconnected_radiator_have_no_hidden_cooling() {
        let p=CoolingParams::default();let mut s=CoolingState::new(80.,120.,&p).unwrap();
        let broken=CoolingInput {fans:[FanDrive{healthy:false,..driving(1.).fans[0]};4],..driving(1.)};
        let out=s.advance(1.,broken,&p).unwrap();assert_eq!((out.airflow_m3_s,out.fan_shaft_load_w),(0.,0.));
        assert!(out.air_out_j>0.,"broken fans retain the passive radiator heat path");
        let out=s.advance(1.,CoolingInput{radiator_connected:false,..driving(1.)},&p).unwrap();
        assert!(out.fan_shaft_load_w>0.);assert_eq!(out.air_out_j,0.);
        s.failed=true;let before=s.engine_j+s.coolant_j;
        let out=s.advance(1.,CoolingInput{radiator_connected:false,..driving(1.)},&p).unwrap();assert!(out.failed);
        assert_eq!((out.fan_shaft_load_w,out.shaft_power_after_fans_w,out.heat_in_j),(0.,0.,0.));
        assert!((s.engine_j+s.coolant_j-before).abs()<1e-6);
    }
    #[test] fn zero_time_is_observation_and_invalid_advance_is_atomic() {
        let p=CoolingParams::default();let mut s=CoolingState::new(120.,1.,&p).unwrap();let before=s.clone();
        s.advance(0.,CoolingInput{leak_kg_s:1.,..driving(1.)},&p).unwrap();assert_eq!(s,before);
        assert!(s.advance(1.,CoolingInput{rpm:f64::NAN,..driving(1.)},&p).is_err());assert_eq!(s,before);
        let overflowing=CoolingParams{heat_per_shaft_w:f64::MAX,..p};
        assert!(s.advance(1.,driving(1.),&overflowing).is_err());assert_eq!(s,before);
    }
    #[test] fn stopped_connected_radiator_cools_without_fan_work_and_balances_energy() {
        let p=CoolingParams::default();let mut connected=CoolingState::new(80.,120.,&p).unwrap();
        let mut isolated=connected.clone();let before=connected.engine_j+connected.coolant_j;
        let out=connected.advance(60.,CoolingInput::default(),&p).unwrap();
        assert_eq!((out.airflow_m3_s,out.fan_shaft_load_w,out.heat_in_j),(0.,0.,0.));
        assert!(out.air_out_j>0.&&out.engine_c<80.&&out.coolant_c.unwrap()<80.);
        assert!((before-connected.engine_j-connected.coolant_j-out.air_out_j).abs()<1e-6);
        let out=isolated.advance(60.,CoolingInput{radiator_connected:false,..Default::default()},&p).unwrap();
        assert_eq!(out.air_out_j,0.);assert_eq!(isolated.engine_j+isolated.coolant_j,before);
    }
}
