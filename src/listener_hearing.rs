//! The listener's ear in the game: where it is acoustically (enclosure, headset), the levels of
//! the vehicle's own sources at the ear, and the hooks through which every impulse that reaches
//! the ear is handed to the hearing model.
//!
//! Own-vehicle sources (own gun, own machine gun, strikes on the listener's own armour) are
//! calibrated at the ear directly (SL §1 rows 1–3, 10, 16–19); only the crew headset lies
//! between them and the eardrum.

use super::*;
use crate::sound_levels::SpectrumClass;
use crate::sound_propagation::{propagate, EarLevel, Emission, Path};

// Pure source-level bridge shared by the live listener and its regression checks. The resolved
// outcome decides whether explosive energy is inside; carrying filler alone does not establish it.
fn resolved_armour_strike_level(event: &crate::ballistics::ImpactEvent, penetrated: bool,
    internal_burst: bool, station_distance_m: f32) -> crate::sound_levels::StrikeLevel
{
    let fill_kg = event.shell.filler_g / 1000.0;
    let he = matches!(event.shell.kind, crate::ballistics::ShellKind::He | crate::ballistics::ShellKind::Heat);
    crate::sound_levels::own_armour_strike(event.shell.mass_kg, event.striking_v,
        event.impact_angle_deg, station_distance_m, penetrated,
        penetrated && internal_burst, fill_kg, he)
}

impl Game {
    /// Level at the ear of an own-vehicle source given its calibration at the ear position.
    pub(super) fn own_vehicle_ear_level(&self, lpk_db: f32, laf_db: f32, class: SpectrumClass,
        a_duration_ms: f32) -> EarLevel
    {
        crate::sound_propagation::at_ear(lpk_db, laf_db, class, a_duration_ms, self.listener_headset())
    }

    /// Physical level at the ear of a cannon strike on the listener's own vehicle
    /// (`sound_levels::own_armour_strike`, SL §5), through the headset.
    pub(super) fn own_armour_strike_ear_level(&self, event: &crate::ballistics::ImpactEvent,
        penetrated: bool, internal_burst: bool, station_distance_m: f32) -> EarLevel
    {
        let strike = resolved_armour_strike_level(event, penetrated, internal_burst, station_distance_m);
        self.own_vehicle_ear_level(strike.lpk_db, strike.laf_db, strike.spectrum, strike.a_duration_ms)
    }

    /// The own machine gun at the ear: each round's peak and the burst level (LAeq, dB(A)), and
    /// whether the closed-hull clip applies. Buttoned: SL §1 row 10; a hatch open lets the muzzle
    /// blast in (+6 dB [E]); head-out: the open-air round (row 9) at the coax muzzle distance.
    /// Through the headset.
    fn own_mg_ear(&self) -> (bool, EarLevel) {
        use crate::sound_levels as L;
        use crate::sound_propagation::Enclosure;
        let (interior, lpk, laf) = match self.listener_enclosure() {
            Enclosure::HeadOut | Enclosure::OnFoot => {
                let spread = 20.0 * crate::audio::MG_COAX_HEAD_OUT_DISTANCE_M.log10();
                (false, L::MG34_OUTSIDE_1M.lpk_db - spread, L::MG34_OUTSIDE_1M.laf_db - spread)
            }
            Enclosure::HatchOpen => (true, L::MG34_INSIDE.lpk_db + 6.0, L::MG34_INSIDE.laf_db + 6.0),
            Enclosure::Buttoned => (true, L::MG34_INSIDE.lpk_db, L::MG34_INSIDE.laf_db),
        };
        (interior, self.own_vehicle_ear_level(lpk, laf, SpectrumClass::SmallArms,
            L::MG34_INSIDE.a_duration_ms))
    }

    /// The own machine gun's burst level at the ear (LAeq, dB(A)) and whether the closed-hull
    /// clip applies.
    pub(super) fn own_mg_ear_laf(&self) -> (bool, f32) {
        let (interior, ear) = self.own_mg_ear();
        (interior, ear.laf_db)
    }

    /// One real own-MG cycle at the cabin crew reference. The published project table is an
    /// estimated combined mount/receiver path (±10dB), not an identified3Dsource measurement.
    /// Keep it distinct from the separately propagated exterior muzzle, with no invented locator.
    pub(super) fn hear_own_mg_round(&mut self) {
        use crate::sound_propagation::Enclosure;
        if !matches!(self.listener_enclosure(),Enclosure::Buttoned|Enclosure::HatchOpen) {return;}
        let level=&crate::sound_levels::MG34_INSIDE;
        // Burst LAeq -> exposure per actual MG34 cycle -> fast maximum.
        let per_round=level.laf_db-10.0*(crate::mounted_mg::ROUNDS_PER_SECOND as f32).log10()
            +crate::sound_levels::LAF_MINUS_LAE_DB;
        let ear=self.own_vehicle_ear_level(level.lpk_db,per_round,level.spectrum,level.a_duration_ms);
        self.hear_own_vehicle_impulse(&ear,0.0);
        if let Some(audio)=&self.audio {
            // Local receiver/mount vibration is already a finite damped mechanical response.
            // Repeating a29–44ms Schroeder comb tail on every round made the13Hz burst ring
            // like a hollow chamber. Keep the ordinary headset transfer and calibration once;
            // the separate exterior muzzle wave still crosses armor and the room normally.
            audio.play_mg_receiver_at_ear(self.mg_round,&ear);
        }
    }

    /// The own Maybach and tracks at the listener's ear (dB(A)). Buttoned: the report's interior
    /// law (SL §7: 93 idle, 110 cross-country, 115 road). An open hatch: 8 dB quieter (buttoned is
    /// 5–12 dB louder, SL §7/§8: the closed cabin becomes a reverberant box around the drivetrain).
    /// Head-out: outside the cabin, ~4 m from the engine deck: the exterior level (10 dB below the
    /// interior: Tiger 100 dB(A) at 10 m moving against 110 inside) carried from 10 m to 4 m [E].
    /// On foot: that exterior level propagated from the parked Tiger to the ear.
    pub(super) fn own_engine_ear_laf(&self, telemetry: crate::audio::EngineTelemetry,
        listener: Vec3) -> f32
    {
        use crate::sound_propagation::{headset_loss_dba, own_engine_enclosure_offset_db, Enclosure,
            EXTERIOR_BELOW_INTERIOR_DB};
        let interior = crate::audio::tiger_engine_interior_dba(telemetry);
        let headset = headset_loss_dba(SpectrumClass::Engine);
        match own_engine_enclosure_offset_db(self.listener_enclosure()) {
            Some(offset) => interior + offset - headset,
            None => {
                let exterior = Emission::from_level(&crate::sound_levels::TIGER_EXTERIOR_10M)
                    .shifted(interior - EXTERIOR_BELOW_INTERIOR_DB
                        - crate::sound_levels::TIGER_EXTERIOR_10M.laf_db);
                let source = self.player.center() + Vec3::Y * 1.0;
                let path = Path {
                    distance_m: source.distance(listener),
                    atmosphere: crate::sound_propagation::active_atmosphere(),
                    occluded: !self.los_clear(source, listener),
                    enclosure: Enclosure::OnFoot,
                    headset: false,
                };
                propagate(&exterior, &path).laf_db
            }
        }
    }

    /// Enemy vehicles' engines and tracks at the listener's ear. The research's T-34 exterior
    /// level (106 dB(A) at 10 m moving, straight exhaust, louder than the Tiger; SL §7) scaled to
    /// the vehicle's speed by the same engine/track law (idle 17 dB lower) and propagated like any
    /// other source. Every enemy type uses the T-34 calibration and voice [E].
    pub(super) fn enemy_vehicle_voices(&self, listener: Vec3, facing: Vec3)
        -> Vec<crate::audio::VehicleVoice>
    {
        use crate::sound_levels::{exterior_dba_at_10m, T34_EXTERIOR_10M};
        let enclosure = self.listener_enclosure();
        let headset = self.listener_headset();
        let atmosphere = crate::sound_propagation::active_atmosphere();
        let mut nearest: Vec<(usize, f32)> = self.enemies.iter().enumerate()
            .filter(|(_, e)| e.alive)
            .map(|(i, e)| (i, e.center().distance(listener)))
            .filter(|(_, d)| *d < 3_000.0)
            .collect();
        nearest.sort_by(|a, b| a.1.total_cmp(&b.1));
        nearest.truncate(6);
        nearest.into_iter().map(|(i, distance)| {
            let enemy = &self.enemies[i];
            let speed_frac = (enemy.speed.abs() / tank::TOP_SPEED).clamp(0.0, 1.0);
            let rpm = 0.08 + 0.8 * speed_frac;
            let kmh = speed_frac * tank::TOP_SPEED * 3.6;
            let level = exterior_dba_at_10m(T34_EXTERIOR_10M.laf_db, rpm, kmh);
            let emission = Emission::from_level(&T34_EXTERIOR_10M).shifted(level - T34_EXTERIOR_10M.laf_db);
            let source = enemy.center() + Vec3::Y * 0.8;
            let path = Path { distance_m: distance, atmosphere,
                occluded: !self.los_clear(source, listener), enclosure, headset };
            let ear = propagate(&emission, &path);
            crate::audio::VehicleVoice {
                id: i,
                ear_laf_dba: ear.laf_db,
                bearing_deg: bearing_deg_from(facing, source - listener),
                lowpass_hz: crate::audio::lowpass_for_transfer(&ear.transfer_db),
                // V-2 idles near 600 rpm and is governed near 1,800 (SL §7); the loop is at ~720.
                playback_speed: 0.83 + 1.6 * speed_frac,
            }
        }).collect()
    }

    /// The manual elevation handwheel/gearbox at its actual retained turret-local source.
    /// 75dBA at1m/full speed is an authored estimate, not a historical measurement. The
    /// existing octave-band path supplies distance, air, obstruction, enclosure and headset
    /// losses; the shared listener bus subsequently applies current hearing state.
    pub(super) fn gun_elevation_voice(&self, listener: Vec3, facing: Vec3, rate: f32)
        -> Option<crate::audio::VehicleVoice>
    {
        use crate::sound_propagation::{Enclosure, class_spectrum};
        if !self.player.alive || !rate.is_finite() || rate < 0.002 { return None; }
        let source = self.player.turret_model().transform_point3(crate::interior_rig::WHEEL1_POS);
        // Contact energy grows with lay rate; square-root amplitude keeps gentle handwheel
        // movement audible without assigning full-speed power to an almost stationary gear.
        let level = 78.0 + 10.0 * rate.min(1.0).log10();
        let emission = Emission {lpk_db:level+9.0,laf_db:level,ref_m:1.0,
            law:crate::sound_levels::DistanceLaw::Spherical,class:SpectrumClass::Mechanical,
            spectrum:class_spectrum(SpectrumClass::Mechanical),a_duration_ms:1000.0,
            tnt_kg:0.0,directivity_db:0.0};
        let enclosure = self.listener_enclosure();
        let inside = matches!(self.mode,Mode::Tank)
            && matches!(enclosure,Enclosure::Buttoned|Enclosure::HatchOpen);
        // An internal source crosses no armor to another seat. A listener outside receives
        // the existing open/closed-hull transfer in the reverse direction (reciprocal estimate).
        let path = Path {distance_m:source.distance(listener).max(0.35),
            atmosphere:crate::sound_propagation::active_atmosphere(),
            occluded:!inside && !self.los_clear(source,listener),
            enclosure:if inside {Enclosure::OnFoot} else if self.hatch_open || self.driver_hatch {
                Enclosure::HatchOpen } else {Enclosure::Buttoned},
            headset:self.listener_headset()};
        let ear = propagate(&emission,&path);
        Some(crate::audio::VehicleVoice {id:0,ear_laf_dba:ear.laf_db,
            bearing_deg:bearing_deg_from(facing,source-listener),
            lowpass_hz:crate::audio::lowpass_for_transfer(&ear.transfer_db),
            playback_speed:0.55+0.65*rate.min(1.0)})
    }

    /// Loose-run contacts at the modeled broken link, with the normal exterior propagation.
    /// Cadence/effort comes from the drivetrain; neither hull travel nor a damage flag creates it.
    pub(super) fn loose_track_voices(&self,listener:Vec3,facing:Vec3)->Vec<crate::audio::VehicleVoice> {
        use crate::sound_propagation::class_spectrum;
        if !self.player.alive {return Vec::new();}
        self.player_powertrain.broken_track_drive.iter().enumerate().filter_map(|(index,drive)| {
            let level=crate::audio::loose_track_source_level(drive.belt_speed_mps.abs(),drive.load)?;
            let kind=if index==0 {ModuleKind::LeftTrack}else{ModuleKind::RightTrack};
            if self.module_ok(kind) {return None;}
            let source=self.track_repair_point(kind);
            let emission=Emission {lpk_db:level+12.0,laf_db:level,ref_m:1.0,
                law:crate::sound_levels::DistanceLaw::Spherical,class:SpectrumClass::Mechanical,
                spectrum:class_spectrum(SpectrumClass::Mechanical),a_duration_ms:1000.0,
                tnt_kg:0.0,directivity_db:0.0};
            let path=Path {distance_m:source.distance(listener).max(0.35),
                atmosphere:crate::sound_propagation::active_atmosphere(),
                occluded:!self.los_clear(source,listener) || (self.mode==Mode::Foot
                    && self.small_arm_player_armor(source,listener).is_some()),
                enclosure:self.listener_enclosure(),headset:self.listener_headset()};
            let ear=propagate(&emission,&path);
            Some(crate::audio::VehicleVoice {id:index,ear_laf_dba:ear.laf_db,
                bearing_deg:bearing_deg_from(facing,source-listener),
                lowpass_hz:crate::audio::lowpass_for_transfer(&ear.transfer_db),
                playback_speed:drive.belt_speed_mps.abs()/crate::audio::LOOSE_TRACK_REFERENCE_BELT_MPS})
        }).collect()
    }

    /// Continuous combustion follows the same patches/wreck states as the flames.
    /// Levels and timbre are authored estimates, not historical measurements.
    pub(super) fn burning_fire_voices(&self, listener: Vec3, facing: Vec3)
        -> Vec<crate::audio::VehicleVoice>
    {
        use crate::sound_propagation::{Enclosure, class_spectrum};
        let mut sources=Vec::new();
        let models=self.fire_surface_models();
        let mut sum=0.0;
        let mut weighted=Vec3::ZERO;
        for k in 0..surface_fire::MAX_PATCHES {
            if let Some((pos,intensity))=self.cabin_fire.patch_world(k,&models) {
                if intensity>0.02 {sum+=intensity;weighted+=pos*intensity;}
            }
        }
        if sum>0.02 {sources.push((0,weighted/sum,sum,true));}
        for (i,wreck) in self.wrecks.iter().enumerate() {
            if self.enemy_active[i] && !self.enemies[i].alive && wreck.burning {
                sources.push((100+i,self.enemies[i].center(),1.0,false));
            }
        }
        for (i,ally) in self.allies.iter().enumerate() {
            if !ally.tank.alive {sources.push((1000+i,ally.tank.center(),0.88,false));}
        }
        for (i,hulk) in self.campaign_hulks.iter().enumerate() {
            if hulk.burning {sources.push((2000+i,hulk.models[0].w_axis.truncate(),0.82,false));}
        }
        sources.into_iter().filter(|(_,p,_,_)|p.distance_squared(listener)<500.0*500.0)
            .map(|(id,source,strength,inside)| {
                // User-selected stronger combustion presence: an authored86dBA at1m
                // reference (+12dB from the old understated bed), not a historical measurement.
                let level=86.0+10.0*strength.max(0.01_f32).log10();
                let emission=Emission {lpk_db:level+12.0,laf_db:level,ref_m:1.0,
                    law:crate::sound_levels::DistanceLaw::Spherical,class:SpectrumClass::Wind,
                    spectrum:class_spectrum(SpectrumClass::Wind),a_duration_ms:1000.0,
                    tnt_kg:0.0,directivity_db:0.0};
                let same_compartment=inside && self.mode==Mode::Tank;
                let path=Path {distance_m:source.distance(listener).max(0.4),
                    atmosphere:crate::sound_propagation::active_atmosphere(),
                    occluded:!same_compartment && !self.los_clear(source,listener),
                    enclosure:if same_compartment {Enclosure::OnFoot} else {self.listener_enclosure()},
                    headset:self.listener_headset()};
                let ear=propagate(&emission,&path);
                crate::audio::VehicleVoice {id,ear_laf_dba:ear.laf_db,
                    bearing_deg:bearing_deg_from(facing,source-listener),
                    lowpass_hz:crate::audio::lowpass_for_transfer(&ear.transfer_db),playback_speed:1.0}
            }).collect()
    }

    /// Actual first fuel ignition. Destruction voices already contain their own catch; this
    /// covers the non-catastrophic fuel/engine transition without replaying every loop update.
    pub(super) fn hear_fuel_ignition(&mut self, source: Vec3, seed: u32) {
        let emission=Emission {lpk_db:106.0,laf_db:92.0,ref_m:1.0,
            law:crate::sound_levels::DistanceLaw::Spherical,class:SpectrumClass::Wind,
            spectrum:crate::sound_propagation::class_spectrum(SpectrumClass::Wind),
            a_duration_ms:180.0,tnt_kg:0.0,directivity_db:0.0};
        let mut voice=crate::acoustics::synth_fire_ignition(seed);
        if let Some(audio)=&self.audio {audio.blend_named_ignition(&mut voice);}
        self.scheduled.push(ScheduledSound {
            delay:self.threat_center().distance(source)/speed_of_sound(),
            sfx:crate::audio::Sfx::ImpactGround, emission, muzzle:None,source,
            source_in_player:true,buf:Some(voice),wave: None, enemy_report:None,
        });
    }

    /// A passing supersonic round's conical shock at the listener (SL §3), tested for every round
    /// (shells and bullets alike) once per flight step against the segment it actually travelled
    /// (`passing_crack`), then carried by the ordinary delay queue (hull, headset, air, the ears).
    pub(super) fn hear_passing_round(&mut self, projectile: &mut AuthoritativeProjectile,
        start: Vec3, end: Vec3, speed_m_s: f32, listener: Vec3)
    {
        if projectile.crack_heard {
            return;
        }
        let Some(crack) = passing_crack(start, end, speed_m_s, listener, projectile.shell.caliber_mm,
            speed_of_sound()) else { return };
        projectile.crack_heard = true;
        // Below ~90 dB peak a crack is far under any bed the listener is in; no voice.
        if crack.lpk_db < 90.0 {
            return;
        }
        let shock = crate::acoustics::synth_n_wave(crack.period_s);
        let wave = crate::acoustics::synth_shell_pass(crack.period_s,
            projectile.shell.caliber_mm,speed_m_s,crack.miss_m);
        let mut emission=Emission::crack(crack.lpk_db,crack.period_s);
        // Preserve the shock's peak; account for the wake's additional energy before the common
        // level-calibrated playback stage. Armor/headset/LOS filtering still happen at arrival.
        let energy=|samples:&[f32]| samples.iter().map(|v|v*v).sum::<f32>();
        let ratio=energy(&wave)/energy(&shock).max(1e-9);
        emission.laf_db+=10.0*ratio.log10();
        emission.a_duration_ms*=ratio;
        self.scheduled.push(ScheduledSound {
            delay: crack.delay_s,
            sfx: crate::audio::Sfx::NearMiss,
            emission,
            muzzle: None,
            source: crack.emission_point,
            source_in_player: false,
            buf: Some(crate::acoustics::spatialize_mono(&wave, 0.0, 0.0)),
            wave: None, enemy_report: None,
        });
    }

    /// Hand an impulse from the world (a propagated gun report, burst, crack or contact) to the
    /// ears (`hearing.rs`): each ear's incident peak, and whether it faces the source.
    pub(super) fn hear_world_impulse(&mut self, ear: &EarLevel, bearing_deg: f32) {
        self.hear_impulse(ear, bearing_deg);
    }

    /// Hand an impulse from the listener's own vehicle to the ears.
    pub(super) fn hear_own_vehicle_impulse(&mut self, ear: &EarLevel, bearing_deg: f32) {
        self.hear_impulse_with(ear, bearing_deg, 0.0, None);
    }

    /// Play a direct recording at the ear (the own gun, the listener's own weapons) and hand the
    /// impulse it carries to the ears where its sound begins (the verified gun recordings open
    /// with ~50 ms of near-silence); the recording, as heard, drives the middle-ear reflex.
    pub(super) fn play_and_hear_at_ear(&mut self, sfx: crate::audio::Sfx, ear: &EarLevel,
        bearing_deg: f32)
    {
        if sfx==crate::audio::Sfx::LugerShot {
            if let Some(mono)=self.audio.as_ref().and_then(|a|a.next_small_arm_direct(sfx)) {
                let stereo=mono.iter().flat_map(|v|[*v,*v]).collect();
                self.play_stereo_and_hear_at_ear(stereo,ear,bearing_deg,crate::audio::EarBus::World);
                return;
            }
        }
        let (onset, envelope) = match self.audio.as_ref().and_then(|a| a.direct_drive(sfx)) {
            Some((onset, envelope)) => (onset, Some(envelope)),
            None => (0.0, None),
        };
        self.hear_impulse_with(ear, bearing_deg, onset, envelope);
        if let Some(audio) = &self.audio {
            audio.play_at_ear(sfx, ear.laf_db);
        }
    }

    /// Hand the impulse a recording carries to the ears where its sound begins, without playing
    /// it (the caller plays it its own way).
    pub(super) fn hear_recording_at_ear(&mut self, sfx: crate::audio::Sfx, ear: &EarLevel, bearing_deg: f32) {
        let (onset, envelope) = match self.audio.as_ref().and_then(|a| a.direct_drive(sfx)) {
            Some((onset, envelope)) => (onset, Some(envelope)),
            None => (0.0, None),
        };
        self.hear_impulse_with(ear, bearing_deg, onset, envelope);
    }

    /// Play a recording through the world renderer: the path's loss in each band, the bearing of
    /// its source from the live head, and the reverberation of where the listener is. Its impulse
    /// goes to the ears where its sound begins, each ear by whether it faces the source.
    pub(super) fn play_world_and_hear_at_ear(&mut self, sfx: crate::audio::Sfx, ear: &EarLevel,
        bearing_deg: f32, reverb: crate::acoustics::ReverbParams)
    {
        let (onset, envelope) = match self.audio.as_ref().and_then(|a| a.direct_drive(sfx)) {
            Some((onset, envelope)) => (onset, Some(envelope)),
            None => (0.0, None),
        };
        self.hear_impulse_with(ear, bearing_deg, onset, envelope);
        if let Some(audio) = &self.audio {
            audio.play_world_at_ear(sfx, ear, bearing_deg, reverb);
        }
    }

    /// Play a synthesized voice at the ear on `bus` (a strike on the own hull, a bullet on its
    /// steel) and hand its impulse to the ears where its sound begins; the voice, as heard,
    /// drives the middle-ear reflex.
    pub(super) fn play_stereo_and_hear_at_ear(&mut self, samples: Vec<f32>, ear: &EarLevel,
        bearing_deg: f32, bus: crate::audio::EarBus)
    {
        let (onset, envelope) = crate::hearing::DriveEnvelope::of_voice(&samples);
        self.hear_impulse_with(ear, bearing_deg, onset, Some(envelope));
        if let Some(audio) = &self.audio {
            audio.play_stereo_at_ear(samples, ear.laf_db, bus);
        }
    }

    fn hear_impulse(&mut self, ear: &EarLevel, bearing_deg: f32) {
        self.hear_impulse_with(ear, bearing_deg, 0.0, None);
    }

    fn hear_impulse_with(&mut self, ear: &EarLevel, bearing_deg: f32, delay_s: f32,
        envelope: Option<crate::hearing::DriveEnvelope>)
    {
        if !matches!(self.mode, Mode::Tank | Mode::Foot) || !ear.lpk_db.is_finite() {
            return;
        }
        self.hearing.impulse_with_envelope(crate::hearing::ear_impulses(ear, bearing_deg), delay_s,
            envelope);
    }
}

/// A passing round's crack at a listener.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PassingCrack {
    /// Whitham N-wave peak at the miss distance, dB re 20 µPa.
    pub lpk_db: f32,
    /// N-wave duration, s.
    pub period_s: f32,
    /// From the segment's start (the round at `start`) to the shock's arrival at the listener, s.
    pub delay_s: f32,
    /// Where on the trajectory the shock that reaches the listener was emitted.
    pub emission_point: Vec3,
    pub miss_m: f32,
}

/// The conical shock of a round travelling `start` -> `end` at `speed_m_s` as it reaches
/// `listener` (SL §3), if it is emitted on this segment. The shock that reaches a listener at miss
/// distance b left the trajectory b/sqrt(M^2-1) before the abeam point; if the round was not on
/// that point during this segment (behind the muzzle, or stopped short of it), no crack arrives
/// from it. The arrival is the round's time to that point plus the normal distance
/// b*M/sqrt(M^2-1) at the speed of sound, i.e. b*sqrt(M^2-1)/(M*c) after the closest approach.
/// None for a round below Mach 1.05.
pub(super) fn passing_crack(start: Vec3, end: Vec3, speed_m_s: f32, listener: Vec3,
    caliber_mm: f32, sound_m_s: f32) -> Option<PassingCrack>
{
    use crate::sound_levels::{projectile_length_m, whitham_n_wave};
    if !listener.is_finite() || !start.is_finite() || !end.is_finite() || !speed_m_s.is_finite() {
        return None;
    }
    let segment = end - start;
    let length = segment.length();
    if !(length > 1.0e-6) {
        return None;
    }
    let direction = segment / length;
    let mach = speed_m_s / sound_m_s.max(1.0);
    if !(mach > 1.05) {
        return None;
    }
    let relative = listener - start;
    let abeam = relative.dot(direction);
    let miss = (relative - direction * abeam).length();
    let cone = (mach * mach - 1.0).sqrt();
    let emitted = abeam - miss / cone;
    if emitted < 0.0 || emitted >= length {
        return None;
    }
    let (lpk_db, period_s) = whitham_n_wave(speed_m_s, sound_m_s, caliber_mm / 1000.0,
        projectile_length_m(caliber_mm), miss)?;
    Some(PassingCrack {
        lpk_db,
        period_s,
        delay_s: emitted / speed_m_s + miss * mach / (cone * sound_m_s),
        emission_point: start + direction * emitted,
        miss_m: miss,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: f32 = 319.0;

    #[test]
    fn internal_ammunition_source_keeps_headset_without_crossing_its_own_hull() {
        use crate::sound_propagation::{Atmosphere, Enclosure, Ground};
        let sound = |source_in_player| ScheduledSound {
            delay: 0.01, sfx: crate::audio::Sfx::TurretPop,
            emission: sound_emission_for(crate::audio::Sfx::TurretPop), muzzle: None,
            source: Vec3::ZERO, source_in_player, buf: None, wave: None, enemy_report: None,
        };
        let internal = sound(true);
        let exterior = sound(false); // identical Sfx and position; provenance decides the path
        for enclosure in [Enclosure::Buttoned, Enclosure::HatchOpen] {
            let base = Path { distance_m: 2.0, atmosphere: Atmosphere::WINTER,
                occluded: true, enclosure, headset: true };
            let (inside, same_compartment) = internal.receiver_path(base);
            assert!(same_compartment);
            assert_eq!(inside.distance_m, base.distance_m);
            assert_eq!(inside.atmosphere.temperature_c, base.atmosphere.temperature_c);
            assert_eq!(inside.atmosphere.ground, Ground::Hard);
            assert!(!inside.occluded && inside.headset);
            assert_eq!(inside.enclosure, Enclosure::OnFoot);
            assert_eq!(exterior.receiver_path(base), (base, false));
            let heard = propagate(&internal.emission, &inside);
            let bare = propagate(&internal.emission, &Path { headset: false, ..inside });
            let outside = propagate(&exterior.emission, &base);
            assert!(heard.laf_db > outside.laf_db + 10.0,
                "an internal burst inherited exterior cover/hull attenuation");
            assert!(heard.laf_db < bare.laf_db - 2.0 && heard.lpk_db < bare.lpk_db,
                "internal provenance bypassed the listener's headset");
            // Physical hearing still receives this very high incident level; no damage/reflex
            // bypass is part of the path correction.
            assert!(crate::hearing::ear_impulses(&heard, 0.0).iter()
                .all(|ear| ear.lpk_db > crate::hearing::MIN_EFFECTIVE_DB));
        }
        for enclosure in [Enclosure::HeadOut, Enclosure::OnFoot] {
            let base = Path { distance_m: 4.0, atmosphere: Atmosphere::WINTER,
                occluded: true, enclosure, headset: enclosure == Enclosure::HeadOut };
            assert_eq!(internal.receiver_path(base), (base, false));
            assert_eq!(exterior.receiver_path(base), (base, false));
        }
    }

    #[test]
    fn armour_source_requires_a_resolved_internal_burst_including_heat_and_ammunition() {
        use crate::ballistics as b;
        use crate::acoustics as a;
        let facing = b::Facing::HullSide;
        let plate = b::ArmorScheme::TigerI.plate(facing);
        for shell in [&b::PANZERFAUST_60, &b::BR_471, &b::PZGR_40] {
            let event = b::resolve(shell, 25.0, plate, 0.0, 0xA660_2026);
            assert_eq!(event.terminal, b::Terminal::Penetration);
            let params = a::params_from_event(&event, facing, 0.0, 0.9, 0xA660_2026);
            let dry = resolved_armour_strike_level(&event, true, false, 0.9);
            let internal = resolved_armour_strike_level(&event, true, true, 0.9);
            assert_eq!(dry.spectrum, SpectrumClass::ArmourRing,
                "{} filler alone selected the confined internal source", shell.name);
            assert_eq!(internal.spectrum, SpectrumClass::LowFrequencyBlast,
                "a resolved ammunition/filler burst was lost for {}", shell.name);
            assert_ne!(internal, dry);
            // The PzGr 40 has no filler: its internal=true case represents the damage solver's
            // separate live-ammunition outcome, not an invented explosive projectile.
            if shell.kind == b::ShellKind::Apcr { assert_eq!(shell.filler_g, 0.0); }
            for (confirmed, source) in [(false, dry), (true, internal)] {
                let ear = crate::sound_propagation::at_ear(source.lpk_db, source.laf_db,
                    source.spectrum, source.a_duration_ms, true);
                let offline = a::impact_ear_level(params,
                    a::DamageSoundContext { internal_detonation: confirmed, ..Default::default() },
                    shell.filler_g / 1000.0);
                assert_eq!((ear.lpk_db, ear.laf_db), (offline.lpk_db, offline.laf_db),
                    "live listener and offline strike disagree about resolved source state");
            }
            assert_eq!(resolved_armour_strike_level(&event, false, true, 0.9).spectrum,
                SpectrumClass::ArmourRing, "no penetration means no internal source");
        }
    }

    #[test]
    fn a_round_passing_abeam_cracks_after_its_closest_approach() {
        // A 76 mm round at 650 m/s passing 5 m to the side of the listener.
        let listener = Vec3::new(5.0, 0.0, 100.0);
        let crack = passing_crack(Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0), 650.0, listener, 76.2, C)
            .expect("a supersonic round passing at 5 m cracks");
        let (lpk, period) = crate::sound_levels::whitham_n_wave(650.0, C, 0.0762,
            crate::sound_levels::projectile_length_m(76.2), 5.0).unwrap();
        assert!((crack.lpk_db - lpk).abs() < 1e-3 && (crack.period_s - period).abs() < 1e-7);
        assert!((crack.miss_m - 5.0).abs() < 1e-3);
        // Closest approach at 100 m / 650 m/s; the shock follows by b*sqrt(M^2-1)/(M*c).
        let mach = 650.0 / C;
        let expected = 100.0 / 650.0
            + crate::sound_levels::crack_delay_after_closest_s(mach, 5.0, C);
        assert!((crack.delay_s - expected).abs() < 1e-4, "{} vs {expected}", crack.delay_s);
        // It was emitted before the abeam point, so it seems to come from up-range.
        assert!(crack.emission_point.z < 100.0);
    }

    #[test]
    fn no_crack_behind_the_muzzle_short_of_the_emission_point_or_below_mach_one() {
        // The firer: 3 m behind and 1 m beside the muzzle of his own round.
        let firer = Vec3::new(1.0, 0.0, -3.0);
        assert!(passing_crack(Vec3::ZERO, Vec3::new(0.0, 0.0, 50.0), 930.0, firer, 88.0, C).is_none());
        // A round that stops (hits the ground) before the point whose shock would reach the
        // listener 20 m aside at z = 60: emitted at 60 - 20/sqrt(M^2-1), after the 45 m stop.
        let aside = Vec3::new(20.0, 0.0, 60.0);
        assert!(passing_crack(Vec3::ZERO, Vec3::new(0.0, 0.0, 45.0), 650.0, aside, 76.2, C).is_none());
        // ... while on a later segment that reaches it, the crack is emitted.
        assert!(passing_crack(Vec3::new(0.0, 0.0, 45.0), Vec3::new(0.0, 0.0, 90.0), 650.0, aside,
            76.2, C).is_some());
        // A subsonic round has no shock.
        assert!(passing_crack(Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0), 300.0, aside, 7.92, C).is_none());
    }

    #[test]
    fn the_crack_falls_with_miss_distance_by_the_whitham_law() {
        let at = |miss: f32| passing_crack(Vec3::ZERO, Vec3::new(0.0, 0.0, 400.0), 760.0,
            Vec3::new(miss, 0.0, 200.0), 7.92, C).unwrap().lpk_db;
        // p ~ b^-3/4: 15 dB of peak level per decade of miss distance.
        assert!(((at(2.0) - at(20.0)) - 15.0).abs() < 0.05, "{}", at(2.0) - at(20.0));
    }
}
