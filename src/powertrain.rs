//! Tracked-vehicle longitudinal drivetrain, for the player Tiger and (since 2026-10-28) every AI
//! tank. Gear ratios use each vehicle's road speeds at rated revolutions and its mass and power
//! (`vehicle_spec`); the Tiger's row is the documented table this model was built on. The torque
//! curve, clutch/shift durations, tireless-track adhesion and yaw inertia are explicit engineering
//! estimates, not measurements; their engine speeds are stated for the Tiger's 3000 rpm and scale
//! with each engine's rated speed. This state drives both chassis motion and the engine mixer.

use crate::tank_support::{DRIVE_EFFICIENCY, ROLLING_RESISTANCE, SPEED_DRAG_N_S2_PER_M2};
use crate::vehicle_spec::{VehicleSpec, TIGER_I};

pub const IDLE_RPM: f32 = 800.0;
pub const RATED_RPM: f32 = 3000.0;
const G: f32 = 9.81;
const SHIFT_SECONDS: f32 = 0.24;
const SHIFT_COOLDOWN_SECONDS: f32 = 0.6;
const STEP_SECONDS: f32 = 1.0 / 120.0;
/// OWNER-DIRECTED GAMEPLAY DEPARTURE (2026-10-03), not physics: the share of the weight's
/// downhill pull the player's Tiger must overcome while climbing under power. At 1.0 the model
/// is the unmodified historical estimate, which leaves long canyon climbs at a walking pace with
/// nothing to do. Only a powered climb is eased; coasting, braking, rolling back and descending
/// feel the full grade. Recorded in docs/PHYSICAL_TRUTH_AUDIT.md. AI vehicles climb at 1.0
/// (`Powertrain::for_ai`).
pub const CLIMB_GRADE_SHARE: f32 = 0.5;
/// Braking force as a share of the weight on the tracks: full application, and the driver's
/// lighter foot when the tank is merely let roll to a stop. Below the hold speed he is fully on
/// the brakes; above the ease speed he uses the lighter figure. [E] driver behaviour, estimated.
const BRAKE_GRIP: f32 = 0.65;
const IDLE_BRAKE_ROLLING: f32 = 0.18;
const IDLE_BRAKE_HOLD_MPS: f32 = 0.6;
const IDLE_BRAKE_EASE_MPS: f32 = 2.5;
/// Wheel rpm below which the automatic box drops a gear; an upshift must be predicted to recover
/// above `UPSHIFT_RECOVERED_RPM` by the time the next selection is allowed. (At the Tiger's 3000
/// rated rpm; `Powertrain::rpm_of` scales them to other engines.)
const DOWNSHIFT_RPM: f32 = 1400.0;
const UPSHIFT_RECOVERED_RPM: f32 = 1500.0;
/// A short shift at `UPSHIFT_RPM` is taken only when the next gear keeps this acceleration reserve
/// (m/s^2); with less, the present gear is wound out to `WOUND_OUT_RPM` (the governor) first [E].
const UPSHIFT_RPM: f32 = 2650.0;
const WOUND_OUT_RPM: f32 = 2950.0;
const SHORT_SHIFT_RESERVE_MPS2: f32 = 0.10;
/// Engine speed above idle a slipping clutch holds at full throttle when the wheels turn slower [E].
const CLUTCH_SLIP_RPM: f32 = 700.0;
/// Share of rated power the torque curve below delivers in top gear at `tank::TOP_SPEED`
/// (2880 rpm: torque factor 1.007 at 0.96 of rated speed). `tank_support::ROLLING_RESISTANCE`
/// is derived from it, so the level power limit in 8th gear is that speed.
pub const TOP_SPEED_POWER_FRACTION: f32 = {
    let rpm = RATED_RPM * crate::tank::TOP_SPEED / (45.0 / 3.6);
    (1.07 - 0.07 * (rpm - 1800.0) / 1200.0) * rpm / RATED_RPM
};

#[derive(Clone, Copy, Debug)]
pub struct Controls {
    pub drive: f32,
    pub steer: f32,
    pub engine_running: bool,
    pub transmission_ok: bool,
    pub left_track: bool,
    pub right_track: bool,
    pub running_gear_factor: f32,
    pub parking_brake: bool,
    /// An AI driver's brake pedal (0-1, share of `BRAKE_GRIP`). `None`: the player's controls,
    /// where an opposing drive brakes and a released drive is the estimated idle foot.
    pub driver_brake: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Motion { pub distance: f32, pub yaw: f32, pub speed: f32 }

/// Powered shaft attempt on a broken track. Speed is gear-derived, not measured loose-link travel.
#[derive(Clone, Copy, Debug, Default)]
pub struct BrokenTrackDrive { pub belt_speed_mps: f32, pub load: f32 }

#[derive(Clone, Copy, Debug)]
pub struct Powertrain {
    pub spec: &'static VehicleSpec,
    /// Share of the grade a powered climb feels (`CLIMB_GRADE_SHARE` for the player, 1 for AI).
    pub climb_grade_share: f32,
    pub automatic: bool,
    pub gear: i8,
    pub rpm: f32,
    pub throttle: f32,
    pub load: f32,
    pub shift_remaining: f32,
    pub drive_force_n: f32,
    pub resistance_force_n: f32,
    /// Left/right source telemetry, present only while torque is actually transmitted without braking.
    pub broken_track_drive: [BrokenTrackDrive; 2],
    shift_cooldown: f32,
    yaw_rate: f32,
    observed_speed: f32,
    single_track: bool,
}

impl Default for Powertrain {
    fn default() -> Self {
        Self { spec:&TIGER_I,climb_grade_share:CLIMB_GRADE_SHARE,
            automatic:true,gear:1,rpm:IDLE_RPM,throttle:0.0,load:0.0,shift_remaining:0.0,
            drive_force_n:0.0,resistance_force_n:0.0,shift_cooldown:0.0,yaw_rate:0.0,
            observed_speed:0.0,single_track:false,broken_track_drive:[BrokenTrackDrive::default();2] }
    }
}

pub(crate) fn gear_speed(spec: &VehicleSpec, gear: i8) -> f32 {
    let (f,r)=(spec.forward_kmh,spec.reverse_kmh);
    if gear > 0 { f[(gear as usize).clamp(1,f.len())-1]/3.6 }
    else if gear < 0 { r[((-gear) as usize).clamp(1,r.len())-1]/3.6 }
    else { f32::INFINITY }
}

fn max_torque(spec: &VehicleSpec, rpm: f32) -> f32 {
    let rated = spec.engine_w / (spec.rated_rpm * std::f32::consts::TAU / 60.0);
    let rpm = rpm.clamp(spec.idle_rpm,spec.rated_rpm*1.2);
    // The Tiger-calibrated curve, in engine speed as a share of rated speed.
    let x = rpm * RATED_RPM / spec.rated_rpm;
    let curve = if x < 1800.0 { 0.65 + 0.42*(x-IDLE_RPM)/1000.0 }
        else { 1.07 - 0.07*((x-1800.0)/1200.0).clamp(0.0,1.0) };
    (rated*curve).min(spec.engine_w/(rpm*std::f32::consts::TAU/60.0))
}

pub(crate) fn wheel_force(spec: &VehicleSpec, gear: i8, rpm: f32) -> f32 {
    max_torque(spec, rpm) * (spec.rated_rpm * std::f32::consts::TAU / 60.0)
        * DRIVE_EFFICIENCY / gear_speed(spec, gear)
}

impl Powertrain {
    /// An AI vehicle's drivetrain: its own row, the full grade.
    pub fn for_ai(spec: &'static VehicleSpec) -> Self {
        Self { spec, climb_grade_share:1.0, rpm:spec.idle_rpm, ..Self::default() }
    }

    /// An engine speed stated for the Tiger's 3000 rated rpm, on this engine.
    fn rpm_of(&self, tiger_rpm: f32) -> f32 { tiger_rpm * self.spec.rated_rpm / RATED_RPM }
    fn mass(&self) -> f32 { self.spec.mass_kg }
    fn top_gear(&self) -> i8 { self.spec.forward_kmh.len() as i8 }
    fn top_reverse(&self) -> i8 { self.spec.reverse_kmh.len() as i8 }

    fn select(&mut self, gear: i8) {
        let gear=gear.clamp(-self.top_reverse(),self.top_gear());
        if self.gear != gear {
            self.gear=gear;
            self.shift_remaining=SHIFT_SECONDS;
            self.shift_cooldown=SHIFT_COOLDOWN_SECONDS;
        }
    }

    /// Sequential manual selection; reject a moving reversal or an engine-overspeed downshift.
    pub fn shift_manual(&mut self, delta: i8) -> bool {
        if delta==0 {return false;}
        let requested=(self.gear+delta.signum()).clamp(-self.top_reverse(),self.top_gear());
        if requested==self.gear {return false;}
        let belt_speed=self.observed_speed.abs()*if self.single_track {2.0}else{1.0};
        if requested!=0 && ((self.observed_speed.abs()>0.35 && requested.signum() as f32!=self.observed_speed.signum())
            || belt_speed/gear_speed(self.spec,requested)*self.spec.rated_rpm>self.rpm_of(3300.0)) {return false;}
        self.automatic=false;
        self.select(requested);
        true
    }

    pub fn select_automatic(&mut self) { self.automatic=true; }

    pub fn rpm_normalized(&self) -> f32 { ((self.rpm-self.spec.idle_rpm)/(self.spec.rated_rpm-self.spec.idle_rpm)).clamp(0.0,1.0) }

    fn automatic_selection(&mut self, demand: f32, speed: f32, pitch: f32, traction: f32) {
        if !self.automatic || self.shift_cooldown>0.0 {return;}
        if demand.abs()>0.02 && speed.abs()<0.12 && self.gear.signum() as f32!=demand.signum() {
            self.select(if demand>0.0 {1}else{-1});
            return;
        }
        if self.gear==0 {return;}
        let spec=self.spec;
        let rated=spec.rated_rpm;
        let mass=self.mass();
        let direction=self.gear.signum();
        let magnitude=self.gear.abs();
        let belt_speed=speed.abs()*if self.single_track {2.0}else{1.0};
        let rpm=belt_speed/gear_speed(spec,self.gear)*rated;
        let climb=direction as f32*pitch.sin();
        let climb=if climb>0.0 && demand.abs()>0.1 {climb*self.climb_grade_share}else{climb};
        let resistance=mass*G*(climb+ROLLING_RESISTANCE*pitch.cos()).max(0.0);
        let maximum=if direction>0 {self.top_gear()}else{self.top_reverse()};
        if demand.abs()>0.1 && rpm>self.rpm_of(UPSHIFT_RPM) && magnitude<maximum {
            let next=direction*(magnitude+1);
            // Do not upshift into a bog after the clutch-open interval loses climbing momentum.
            // The test is the wheel rpm the next gear will have recovered to when the box may
            // select again, not the rpm at the instant the clutch closes: on a grade the hull
            // slows through the open clutch, the clutch then slips at `CLUTCH_SLIP_RPM` (as in
            // `advance`) and the next gear's surplus force pulls the speed back up. Requiring
            // 1500 rpm at clutch-in held a Tiger starting on any grade over about 3 degrees in
            // first gear for good, with several times the needed force available in second.
            let belt_scale=if self.single_track {2.0}else{1.0};
            let coast_loss=resistance/mass*SHIFT_SECONDS*belt_scale;
            let after=(belt_speed-coast_loss).max(0.0);
            let next_rpm=after/gear_speed(spec,next)*rated;
            let engaged_rpm=next_rpm.max(spec.idle_rpm+self.rpm_of(CLUTCH_SLIP_RPM)*demand.abs());
            let available=wheel_force(spec,next,engaged_rpm)*traction*demand.abs();
            let effective_mass=mass*if self.single_track {3.4}else{1.0};
            let recovered=after+(available-resistance)/effective_mass*belt_scale
                *(SHIFT_COOLDOWN_SECONDS-SHIFT_SECONDS);
            let recovered_rpm=recovered/gear_speed(spec,next)*rated;
            // The next gear must also clear the downshift's own bogging margin (1.05), or the
            // box hunts between the two; and a gear that would leave almost no acceleration
            // reserve is taken only once the present one is wound out.
            let holds=available>(resistance*1.02+mass*0.015).max(resistance*1.05);
            let reserve=available-resistance>mass*SHORT_SHIFT_RESERVE_MPS2;
            if recovered_rpm>=self.rpm_of(UPSHIFT_RECOVERED_RPM) && holds && (reserve || rpm>=self.rpm_of(WOUND_OUT_RPM)) {
                self.select(next);
            }
        } else if magnitude>1 {
            let previous=direction*(magnitude-1);
            let previous_rpm=belt_speed/gear_speed(spec,previous)*rated;
            let bogging=demand.abs()>0.4 && rpm<self.rpm_of(2450.0) && wheel_force(spec,self.gear,rpm)*traction<resistance*1.05;
            if (rpm<self.rpm_of(DOWNSHIFT_RPM) || bogging) && previous_rpm<self.rpm_of(2850.0) {self.select(previous);}
        }
    }

    /// Integrate the full elapsed interval in <=1/120 s substeps for ordinary frame intervals.
    /// At most 1200 substeps bound pathological pauses; their whole interval is still consumed.
    pub fn advance(&mut self, dt:f32, speed:f32, pitch:f32, controls:Controls) -> Motion {
        self.broken_track_drive=[BrokenTrackDrive::default();2];
        if !dt.is_finite() || dt<=0.0 || !speed.is_finite() || !pitch.is_finite() {return Motion {speed,..Motion::default()};}
        let spec=self.spec;
        let rated=spec.rated_rpm;
        let mass=self.mass();
        let half_track=spec.steer_half_gauge_m;
        let steps=(dt/STEP_SECONDS).ceil().clamp(1.0,1200.0) as usize;
        let h=dt/steps as f32;
        let one=controls.left_track != controls.right_track;
        self.single_track=one;
        let side=if controls.left_track {-1.0}else{1.0};
        let any=controls.left_track || controls.right_track;
        let drivetrain_ready=controls.engine_running && controls.transmission_ok;
        let capable=drivetrain_ready && any;
        let raw_drive=if controls.drive.is_finite(){controls.drive.clamp(-1.0,1.0)}else{0.0};
        let steer=if controls.steer.is_finite(){controls.steer.clamp(-1.0,1.0)}else{0.0};
        let demand=if drivetrain_ready {(raw_drive+if one {side*steer}else{0.0}).clamp(-1.0,1.0)}else{0.0};
        let traction=controls.running_gear_factor.clamp(0.0,1.0);
        let mut v=if !any {0.0}else if one {speed.clamp(-half_track*0.35,half_track*0.35)}else{speed};
        let mut motion=Motion::default();
        for _ in 0..steps {
            self.shift_cooldown=(self.shift_cooldown-h).max(0.0);
            self.shift_remaining=(self.shift_remaining-h).max(0.0);
            if drivetrain_ready {self.automatic_selection(demand,v,pitch,traction);}
            let opposing=demand.abs()>0.02 && v.abs()>0.12 && demand.signum()!=v.signum();
            let selected=self.gear!=0 && demand.signum()==self.gear.signum() as f32;
            self.throttle=if controls.engine_running && !opposing {demand.abs()}else{0.0};
            let coupled=drivetrain_ready && self.gear!=0 && self.shift_remaining<=0.0;
            let belt_speed=v.abs()*if one {2.0}else{1.0};
            let wheel_rpm=belt_speed/gear_speed(spec,self.gear)*rated;
            let steering=if capable && !one {steer.abs()}else{0.0};
            let rpm_target=if !controls.engine_running {0.0}
                else if coupled && any {wheel_rpm.max(spec.idle_rpm+self.rpm_of(CLUTCH_SLIP_RPM)*self.throttle+self.rpm_of(850.0)*steering)}
                else {spec.idle_rpm+self.rpm_of(2200.0)*self.throttle+self.rpm_of(850.0)*steering};
            let response=1.0-(-h/if coupled {0.09}else{0.24}).exp();
            self.rpm+=(rpm_target.min(self.rpm_of(3600.0))-self.rpm)*response;
            let governor=((self.rpm_of(3150.0)-wheel_rpm)/self.rpm_of(180.0)).clamp(0.0,1.0);
            let mut force=if any && coupled && selected && !opposing {
                wheel_force(spec,self.gear,self.rpm)*self.throttle*governor*traction*self.gear.signum() as f32
            }else{0.0};
            // Steering consumes power from the same engine. The resistance coefficient and
            // effective rotational mass are estimates; they do not change the geometric pivot.
            let yaw_target=if capable && !one {steer*spec.max_yaw_rate*(self.rpm/self.rpm_of(1500.0)).clamp(0.0,1.0)*traction}else{0.0};
            self.yaw_rate+=(yaw_target-self.yaw_rate).clamp(-0.85*h,0.85*h);
            let steering_power=mass*G*0.10*half_track*self.yaw_rate.abs();
            let engine_power=max_torque(spec,self.rpm)*self.rpm.max(spec.idle_rpm)*std::f32::consts::TAU/60.0;
            let steering_share=(steering_power/(engine_power*DRIVE_EFFICIENCY).max(1.0)).clamp(0.0,0.8);
            force*=1.0-steering_share;
            let normal=mass*G*pitch.cos().max(0.0);
            force=force.clamp(-0.8*normal*traction,0.8*normal*traction);
            let compression=if coupled && self.throttle<0.02 && wheel_rpm>spec.idle_rpm {
                wheel_force(spec,self.gear,self.rpm)*0.10
            }else{0.0};
            // See CLIMB_GRADE_SHARE: eased only while the tracks are driving up the slope.
            // Keyed on the selected direction, not the instantaneous force, so the open clutch of
            // a gear change does not hand the full grade back and make the box hunt.
            let powered_climb=self.throttle>0.02 && controls.engine_running && (self.gear as f32)*pitch.sin()>0.0;
            let grade=mass*G*pitch.sin()*if powered_climb {self.climb_grade_share}else{1.0};
            let rolling=normal*ROLLING_RESISTANCE+SPEED_DRAG_N_S2_PER_M2*v*v;
            // The driver's foot. With no drive asked for he brakes: firmly enough at a crawl to
            // hold the tank on a slope, more gently at speed so it comes to rest like 57 tonnes
            // and not like a dropped anchor. Without this a stopped Tiger rolled away on any
            // slope past the rolling resistance (about 2 degrees), and with one track off it
            // turned as it rolled. An AI driver works his own pedal, with the same hold at a crawl.
            let hold=1.0-((v.abs()-IDLE_BRAKE_HOLD_MPS)/(IDLE_BRAKE_EASE_MPS-IDLE_BRAKE_HOLD_MPS)).clamp(0.0,1.0);
            let idle_brake=match controls.driver_brake {
                None if demand.abs()<0.02 => normal*(IDLE_BRAKE_ROLLING+(BRAKE_GRIP-IDLE_BRAKE_ROLLING)*hold),
                None => 0.0,
                Some(pedal) => {
                    let pedal=if pedal.is_finite() {pedal.clamp(0.0,1.0)} else {0.0};
                    let held=if demand.abs()<0.02 {BRAKE_GRIP*hold} else {0.0};
                    normal*(BRAKE_GRIP*pedal).max(held)
                }
            };
            let braking=if opposing || controls.parking_brake {normal*BRAKE_GRIP}else{idle_brake};
            let motive=force-grade;
            let direction=if v.abs()>1e-5 {v.signum()}else{motive.signum()};
            let resistance=rolling+compression+braking;
            let net=if v.abs()<1e-5 && motive.abs()<=resistance {0.0}else{motive-direction*resistance};
            let effective_mass=mass*if one {3.4}else{1.0};
            let mut next=v+net/effective_mass*h;
            if v*next<0.0 && motive.abs()<=resistance {next=0.0;}
            if !any || controls.parking_brake && v.abs()<0.02 {next=0.0;}
            let distance=(v+next)*0.5*h;
            motion.distance+=distance;
            motion.yaw+=if !any {0.0}
                else if one {side*distance/half_track}
                else if controls.parking_brake {0.0}else{self.yaw_rate*h};
            self.drive_force_n=force;
            self.resistance_force_n=grade+direction*resistance;
            self.load=if controls.engine_running {
                (if coupled && any {self.throttle*governor}else{0.02*self.throttle}+steering_share).clamp(0.0,1.0)
            }else{0.0};
            // Gear-derived shaft attempt, distinct from chassis traction and loose-link travel.
            // With both tracks absent, 2% throttle load represents estimated drivetrain/loose
            // running-gear drag only; it cannot produce ground force or move the vehicle.
            self.broken_track_drive=[BrokenTrackDrive::default();2];
            if coupled && selected && !opposing && !controls.parking_brake && self.throttle>0.02
                && (force.abs()>1.0 || !any) {
                let attempt=BrokenTrackDrive {
                    belt_speed_mps:gear_speed(spec,self.gear)*self.rpm/rated*self.gear.signum() as f32,
                    load:if any {(force.abs()/wheel_force(spec,self.gear,self.rpm).max(1.0)).clamp(0.0,1.0)}
                        else {0.02*self.throttle},
                };
                for (i,intact) in [controls.left_track,controls.right_track].into_iter().enumerate() {
                    if !intact {self.broken_track_drive[i]=attempt;}
                }
            }
            v=next;
        }
        self.observed_speed=v;
        motion.speed=v;
        motion
    }
}

/// An AI driver on the drivetrain (2026-10-28). The AI's `drive` command is the speed it wants as
/// a share of its vehicle's documented road speed (under the old kinematic controller it was a
/// share of one 12 m/s for every kind); the driver turns it into throttle and brake pedal like a
/// man at the levers, so the vehicle's own mass, power and gears decide how fast it gets there
/// and how long it takes to stop. Returns (signed throttle demand, brake pedal). Gains [E]: full
/// throttle from 1.5 m/s below the wanted speed, a light 3 % throttle coasting within 0.5 m/s
/// above it, the pedal rising to full at 4 m/s over it; a reversal brakes fully first.
pub const STOP_PEDAL: f32 = 0.4;

pub fn ai_driver(spec: &VehicleSpec, drive: f32, speed: f32) -> (f32, f32) {
    let drive = if drive.is_finite() { drive.clamp(-1.0, 1.0) } else { 0.0 };
    let want = drive * spec.road_speed;
    if want.abs() < 0.05 {
        // Stop: off the throttle and a firm, not a panic, pedal (0.4 of full: about 0.26 g, 14 m
        // from 30 km/h) [E]; the hold at a crawl is the drivetrain's own.
        return (0.0, STOP_PEDAL);
    }
    if speed.abs() > 0.3 && want.signum() != speed.signum() {
        return (0.0, 1.0);
    }
    let excess = speed.abs() - want.abs();
    let throttle = (-excess / 1.5).clamp(0.03, 1.0);
    let brake = ((excess - 0.5) / 3.5).clamp(0.0, 1.0);
    if brake > 0.0 { (0.0, brake) } else { (want.signum() * throttle, 0.0) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tank_support::TIGER_TRACK_X;
    fn gear_speed(gear:i8)->f32 {super::gear_speed(&TIGER_I,gear)}
    fn wheel_force(gear:i8,rpm:f32)->f32 {super::wheel_force(&TIGER_I,gear,rpm)}
    fn controls(drive:f32)->Controls {Controls {drive,steer:0.0,engine_running:true,transmission_ok:true,
        left_track:true,right_track:true,running_gear_factor:1.0,parking_brake:false,driver_brake:None}}
    fn run(state:&mut Powertrain, speed:&mut f32, seconds:f32, pitch:f32, input:Controls, dt:f32)->f32 {
        let mut distance=0.0;
        for _ in 0..(seconds/dt).round() as usize {let motion=state.advance(dt,*speed,pitch,input);*speed=motion.speed;distance+=motion.distance;}
        distance
    }
    #[test]
    fn broken_shaft_attempt_requires_transmitted_torque_and_never_uses_hull_coasting() {
        let mut state=Powertrain::default();
        let mut c=controls(1.0);c.right_track=false;
        state.advance(0.1,0.0,0.0,c);
        assert!(state.broken_track_drive[1].belt_speed_mps>0.0 && state.broken_track_drive[1].load>0.0);
        assert_eq!(state.broken_track_drive[0].load,0.0);
        for variant in 0..5 {
            let mut s=state;let mut input=c;
            match variant {0=>input.drive=0.0,1=>input.parking_brake=true,
                2=>input.engine_running=false,3=>input.transmission_ok=false,
                _=>{input.drive=-1.0;}}
            s.advance(1.0/60.0,0.4,0.0,input);
            assert!(s.broken_track_drive.iter().all(|side|side.load==0.0 && side.belt_speed_mps==0.0),"variant={variant}");
        }
        let mut detached=Powertrain::default();let mut input=c;input.left_track=false;
        let motion=detached.advance(0.5,0.0,0.0,input);
        assert_eq!(motion.speed,0.0);assert_eq!(motion.distance,0.0);assert_eq!(motion.yaw,0.0);
        assert_eq!(detached.drive_force_n,0.0);
        assert!(detached.broken_track_drive.iter().all(|side|side.load>0.0 && side.belt_speed_mps>0.0));
        input.parking_brake=true;detached.advance(0.1,0.0,0.0,input);
        assert!(detached.broken_track_drive.iter().all(|side|side.load==0.0));
        state.advance(0.0,0.4,0.0,c);
        assert!(state.broken_track_drive.iter().all(|side|side.load==0.0));
    }
    #[test]
    fn a_tank_left_alone_on_a_slope_stays_put_and_a_rolling_one_stops() {
        // Stopped on slopes up to 25 degrees, facing up or down, both tracks or one: no creep.
        for degrees in [-25.0_f32,-10.0,-3.0,3.0,10.0,25.0] {
            for one_track in [false,true] {
                let mut p=Powertrain::default();let mut speed=0.0;
                let mut input=controls(0.0);input.left_track=!one_track;
                let mut yaw=0.0;let mut distance=0.0;
                for _ in 0..600 {let m=p.advance(1.0/60.0,speed,degrees.to_radians(),input);
                    speed=m.speed;yaw+=m.yaw;distance+=m.distance;}
                assert_eq!((speed,distance,yaw),(0.0,0.0,0.0),"degrees={degrees} one_track={one_track}");
            }
        }
        // Let go of the drive at speed on level ground: it stops in a few seconds, not a coast
        // of half a minute, and not instantly.
        let mut p=Powertrain::default();let mut speed=0.0;
        run(&mut p,&mut speed,45.0,0.0,controls(1.0),1.0/60.0);
        let top=speed;let mut seconds=0.0;
        while speed>0.0 && seconds<60.0 {run(&mut p,&mut speed,0.1,0.0,controls(0.0),1.0/60.0);seconds+=0.1;}
        assert!(seconds>2.5 && seconds<8.0,"from {top} m/s took {seconds} s");
        // Rolling downhill with the drive released, it still comes to rest.
        let mut p=Powertrain::default();let mut speed=6.0;
        run(&mut p,&mut speed,20.0,(-12.0_f32).to_radians(),controls(0.0),1.0/60.0);
        assert_eq!(speed,0.0);
    }
    #[test]
    fn automatic_gears_accelerate_and_use_actual_ratio_rpm() {
        let mut p=Powertrain::default();let mut speed=0.0;
        run(&mut p,&mut speed,45.0,0.0,controls(1.0),1.0/60.0);
        assert!(p.gear>=7 && speed>8.0 && speed<13.0,"{p:?} speed={speed}");
        assert!((p.rpm-speed/gear_speed(p.gear)*RATED_RPM).abs()<150.0);
    }
    #[test]
    fn a_standing_start_on_a_grade_leaves_first_gear() {
        // Stopping to fire on a slope must not strand the Tiger at first-gear crawl (0.8 m/s).
        for (degrees,min_gear,min_speed) in [(5.0_f32,3,3.0_f32),(8.0,3,2.0),(12.0,2,1.0)] {
            let mut p=Powertrain::default();let mut speed=0.0;
            run(&mut p,&mut speed,60.0,degrees.to_radians(),controls(1.0),1.0/60.0);
            assert!(p.gear>=min_gear && speed>min_speed,"{degrees} degrees: gear {} speed {speed}",p.gear);
            // Settled, not hunting: the same gear a second later.
            let gear=p.gear;run(&mut p,&mut speed,1.0,degrees.to_radians(),controls(1.0),1.0/60.0);
            assert_eq!(p.gear,gear,"{degrees} degrees hunts between gears");
        }
    }
    #[test]
    fn climb_assist_eases_only_a_powered_climb() {
        // Powered climb at 10 degrees is faster than the unassisted model's 5 degree result
        // would allow at share 1.0, and never faster than level running.
        let mut p=Powertrain::default();let mut speed=0.0;
        run(&mut p,&mut speed,60.0,10.0_f32.to_radians(),controls(1.0),1.0/60.0);
        let mut flat=Powertrain::default();let mut flat_speed=0.0;
        run(&mut flat,&mut flat_speed,60.0,0.0,controls(1.0),1.0/60.0);
        assert!(speed>3.0 && speed<flat_speed,"climb {speed} flat {flat_speed}");
        // Throttle released on the same slope: the full grade acts and the tank loses speed
        // at least as fast as gravity along a 10 degree slope allows without any assist.
        let before=speed;
        run(&mut p,&mut speed,0.5,10.0_f32.to_radians(),controls(0.0),1.0/60.0);
        assert!(before-speed>0.5*9.81*10.0_f32.to_radians().sin()*0.9,"coasting kept the assist: {before} -> {speed}");
    }
    #[test]
    fn level_running_reaches_top_gear_and_the_calibrated_top_speed() {
        let mut p=Powertrain::default();let mut speed=0.0;
        run(&mut p,&mut speed,420.0,0.0,controls(1.0),1.0/60.0);
        assert!(p.gear==8 && speed>11.3 && speed<12.5,"gear {} speed {speed}",p.gear);
    }
    #[test]
    fn uphill_resistance_causes_downshift_and_lower_speed() {
        let mut flat=Powertrain::default();let mut speed=0.0;
        run(&mut flat,&mut speed,35.0,0.0,controls(1.0),1.0/60.0);
        let flat_speed=speed;let flat_gear=flat.gear;
        run(&mut flat,&mut speed,25.0,15.0_f32.to_radians(),controls(1.0),1.0/60.0);
        assert!(speed>0.1 && speed<flat_speed*0.5 && flat.gear<flat_gear,"{flat:?} speed={speed}");
        // 15 degrees through the owner-directed CLIMB_GRADE_SHARE: (sin 15 * share + rolling) * weight.
        assert!(flat.resistance_force_n>80_000.0 && flat.load>0.0,"{flat:?} speed={speed}");
        assert!((flat.load-flat.drive_force_n/wheel_force(flat.gear,flat.rpm)).abs()<1e-4,
            "audio load must report the torque actually transmitted, including governor reduction");
        assert!((flat.drive_force_n-flat.resistance_force_n).abs()<flat.resistance_force_n*0.02,
            "steady climbing must balance delivered traction against gravity/rolling resistance: {flat:?}");
    }
    #[test]
    fn manual_gear_stays_selected_and_rejects_moving_reversal() {
        let mut p=Powertrain::default();let mut speed=0.0;
        assert!(p.shift_manual(1));assert!(!p.automatic);assert_eq!(p.gear,2);
        run(&mut p,&mut speed,15.0,0.0,controls(1.0),1.0/60.0);
        assert_eq!(p.gear,2);assert!(speed<1.3 && speed>0.5);
        p.gear=0;
        assert!(!p.shift_manual(-1));
        p.select_automatic();assert!(p.automatic);
    }
    #[test]
    fn coast_reduces_speed_and_reverse_brakes_before_engaging() {
        let mut p=Powertrain::default();let mut speed=0.0;
        run(&mut p,&mut speed,15.0,0.0,controls(1.0),1.0/60.0);
        let powered=speed;
        run(&mut p,&mut speed,1.0,0.0,controls(0.0),1.0/60.0);
        assert!(speed>0.0 && speed<powered);
        let before=speed;p.advance(1.0/60.0,speed,0.0,controls(-1.0));
        assert!(p.gear>0 && p.observed_speed<before);
        run(&mut p,&mut speed,10.0,0.0,controls(-1.0),1.0/60.0);
        assert!(p.gear<0 && speed< -0.3,"{p:?} speed={speed}");
    }
    #[test]
    fn broken_track_pivots_but_engine_off_and_both_broken_cannot_drive() {
        for left in [false,true] {
            let mut p=Powertrain::default();let mut input=controls(1.0);input.left_track=left;input.right_track=!left;
            let motion=p.advance(0.5,0.0,0.0,input);
            assert!(motion.distance>0.0 && motion.yaw.signum()==if left {-1.0}else{1.0});
            assert!((motion.yaw.abs()*TIGER_TRACK_X-motion.distance).abs()<1e-5);
        }
        let mut p=Powertrain::default();let mut input=controls(1.0);input.engine_running=false;
        assert_eq!(p.advance(1.0,0.0,0.0,input).distance,0.0);
        input.engine_running=true;input.left_track=false;input.right_track=false;
        assert_eq!(p.advance(1.0,2.0,0.0,input).distance,0.0);
    }
    #[test]
    fn drivetrain_is_consistent_across_normal_frame_partitions() {
        let mut results=Vec::new();
        for dt in [1.0/15.0,1.0/30.0,1.0/60.0,1.0/120.0] {
            let mut p=Powertrain::default();let mut speed=0.0;
            let distance=run(&mut p,&mut speed,20.0,0.05,controls(1.0),dt);
            results.push((p.gear,p.rpm,speed,distance));
        }
        eprintln!("20s full throttle, 0.05rad slope, 15/30/60/120fps (gear,rpm,m/s,m): {results:?}");
        for r in &results[1..] {assert_eq!(r.0,results[0].0);assert!((r.1-results[0].1).abs()<2.0);
            assert!((r.2-results[0].2).abs()<0.01 && (r.3-results[0].3).abs()<0.05,"{results:?}");}
    }
}
