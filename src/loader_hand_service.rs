//! Human hands at the breech: existing finite ammunition/casing handling, shared by
//! normal operation and a living keeper servicing the disabled recoverable Tiger.
//! Driving, gun firing and autonomous loading stay in their original owners.
use super::*;

impl Game {
    pub(super) fn update_manual_ammunition_hands(&mut self,input:&TankInput,acting:Option<Station>,
        primary_was_held:bool,dt:f32,r:&mut Renderer)->(bool,bool) {
        let at=|station:Station|acting==Some(station);
        let mut ram_consumed_look=false;let mut casing_consumed_look=false;
        // Inside: one LMB click works the lever; held LMB carries physical ammunition.
        if at(Station::Loader) {
            self.lever_pulling = false;
            let target = (self.hover == Some(Hotspot::GunLever)).then(|| {
                    breech_lever_click(self.lever_open, input.mg_coax, primary_was_held,
                    self.carry_mesh.is_none() && self.carried_casing.is_none()
                        && !self.ext_equipped)
                }).flatten();
            if let Some(target) = target {
                let before = self.lever_open;
                self.lever_open = target;
                let crossed_open = before < LEVER_OPEN && self.lever_open >= LEVER_OPEN;
                let crossed_shut = before > LEVER_SHUT && self.lever_open <= LEVER_SHUT;
                // The lever pulled and the block closing: one of the three recorded
                // closes (never the last one twice); the opening keeps its recording.
                let closed_recorded = crossed_shut && {
                    let at = self.breech_pos();
                    self.recorded(crate::audio::RecordedFx::BreechClose, 0.0, at)
                };
                if (crossed_open || crossed_shut) && !closed_recorded {
                    if let Some(a) = &self.audio {
                        a.play_mechanical(
                            crate::acoustics::MechanicalParams {
                                cue: if crossed_open {
                                    crate::acoustics::MechanicalCue::BreechOpen
                                } else {
                                    crate::acoustics::MechanicalCue::BreechLock
                                },
                                intensity: 0.88,
                                bearing_deg: 28.0,
                                seed: self.armor_tick ^ 0x1E7E_0001,
                            },
                            0.72,
                        );
                    }
                    if crossed_open {
                        // Exactly one vent per fired case. Re-working an empty block does
                        // not conjure repeated orange puffs into the compartment.
                        self.vent_bore_smoke();
                    }
                }
            }
            // ── THE HOT CASE: after a shot it SITS in the breech. Hold LMB on it to work it
            // out (CASING_PULL_TIME of grip), then it rides your hands until you let go —
            // the toss inherits the carry swing and the case clatters off the basket floor.
            let can_extract = self.can_spawn_casing() && casing_extract_eligible(self.breech, self.lever_open,
                self.carried_casing.is_none() && self.carry_mesh.is_none() && !self.ext_equipped);
            (self.casing_pull, casing_consumed_look) = casing_pull_step(
                self.casing_pull, dt, input.mg_coax, can_extract,
                self.hover == Some(Hotspot::SpentCasing));
            if can_extract && self.casing_pull >= 1.0 {
                let case_model = spent_case_model(self.player.visual_gun_model(), 1.0);
                self.casing_pull = 0.0;
                self.breech = Breech::Empty;
                let (_, rotation, position) = case_model.to_scale_rotation_translation();
                self.set_carry_pose(position, rotation, Vec3::ZERO, Vec3::ZERO);
                let k = self.spawn_casing_from(position, Vec3::ZERO)
                    .expect("the extraction gate reserved an unused casing body");
                self.casings[k].fired_s = Some(0.0);
                self.cabin_fumes.case_freed(k);
                self.casings[k].carried = true;
                self.carried_casing = Some(k);
                // The case drawn out: its main transient where it clears the breech.
                let at = self.breech_pos();
                let extracted = self.recorded_aligned(crate::audio::RecordedFx::BreechExtract, 0.0, at,
                    game_recorded_fx::Align::Loudest { lead_s: 0.05 });
                if !extracted { if let Some(a) = &self.audio {
                    a.play_mechanical(
                        crate::acoustics::MechanicalParams {
                            cue: crate::acoustics::MechanicalCue::CasingExtract,
                            intensity: 0.76,
                            bearing_deg: 64.0,
                            seed: self.kill_tick ^ 0xCA5E_0002,
                        },
                        0.58,
                    );
                } }
            }
            if self.carried_casing.is_some() {
                self.update_carry(r, dt, CARRY_K_CASE, CARRY_D_CASE);
                if !input.mg_coax { self.release_carried_casing(); }
            }
            // A steel fragment in the hand rides the same light spring as an empty case.
            if self.fea.carried_fragment.is_some() {
                self.update_carry(r, dt, CARRY_K_CASE, CARRY_D_CASE);
                self.fea_follow_carried_fragment();
                if !input.mg_coax { self.fea_release_fragment(None); }
            }

            if self.carried_casing.is_none() {
                // A fresh held press picks the exact hovered rack/floor/chamber round.
                if input.mg_coax && !primary_was_held && self.carry_mesh.is_none()
                    && !self.primary_pickup_press
                {
                    self.interact_round(r.camera.forward());
                }
                if round_grip_released(self.carry_mesh.is_some(), input.mg_coax) {
                    self.try_stow_carried_round(r.camera.pos, r.camera.forward());
                    self.grab = false;
                    if self.drop_carried_round(true) {
                        self.ram = 0.0;
                        // A release is not a floor contact. The swept body emits
                        // its own sound when it actually strikes a surface.
                    }
                }
                if self.grab {
                    self.update_carry(r, dt, CARRY_K_ROUND, CARRY_D_ROUND);
                    // Hold the round at the open tray, then flick mouse/right stick to ram.
                    let mouth = self.reload_mouth_world();
                    let line = (mouth - r.camera.pos).normalize_or_zero();
                    let eligible = manual_ram_eligible(self.breech, self.lever_open,
                        self.carry_pos.distance(mouth), if self.ram > 0.0 { 1.0 } else { r.camera.forward().dot(line) });
                    (self.ram, ram_consumed_look) = manual_ram_step(
                        self.ram, dt, input.mg_coax, eligible, Vec2::new(input.look_dx, input.look_dy));
                    self.ram_ready = eligible;
                    if self.auto_ram {
                        if eligible {
                            // manual_ram_step bled a little off; the committed ram outruns it.
                            self.ram = (self.ram + dt * (1.0 / AUTO_RAM_SECONDS + RAM_DECAY)).min(1.0);
                            ram_consumed_look = true;
                            // The committed ram's end is known: start the recording so its
                            // final strike lands as the round seats.
                            if !self.acoustics.ram_sound_started {
                                let in_s = (1.0 - self.ram).max(0.0) * AUTO_RAM_SECONDS;
                                let at = self.breech_pos();
                                self.acoustics.ram_sound_started = self.recorded_aligned(crate::audio::RecordedFx::BreechRam, 0.0,
                                    at, game_recorded_fx::Align::StrikeIn { in_s });
                            }
                        } else {
                            self.auto_ram = false;
                        }
                    }
                    if eligible {
                        let seated = chambered_round_model(self.player.visual_gun_model());
                        let carried_pose = self.carried_round_model();
                        let physically_seated = carried_pose.transform_point3(Vec3::ZERO)
                            .distance(seated.transform_point3(Vec3::ZERO)) <= 0.035
                            && carried_pose.transform_vector3(Vec3::Z).normalize_or_zero()
                                .dot(seated.transform_vector3(Vec3::Z).normalize_or_zero()) > 0.995;
                        if self.ram >= 1.0 && physically_seated {
                            // The guided round has reached the open, empty chamber.
                            if let Some(carried) = self.carry_mesh {
                                self.he_loaded = carried.nature_he(&self.slot_nature_he, &self.loose);
                                self.transfer_cartridge_state(carried.ammunition_location_id(), 0x1_0000);
                            }
                            self.breech = Breech::Loaded;
                            self.ram = 0.0;
                            self.grab = false;
                            self.carry_mesh = None;
                            self.auto_ram = false;
                            self.ram_ready = false;
                            // A hand ram's end is not known ahead: the recording's head is cut
                            // so its strike lands 30 ms after the seat.
                            let started = std::mem::take(&mut self.acoustics.ram_sound_started);
                            let at = self.breech_pos();
                            let rammed = started || self.recorded_aligned(crate::audio::RecordedFx::BreechRam, 0.0,
                                at, game_recorded_fx::Align::StrikeIn { in_s: 0.03 });
                            if !rammed { if let Some(a) = &self.audio {
                                a.play_mechanical(
                                    crate::acoustics::MechanicalParams {
                                        cue: crate::acoustics::MechanicalCue::ShellRam,
                                        intensity: 1.0,
                                        bearing_deg: 350.0,
                                        seed: self.rounds_spent ^ 0x5E11_0006,
                                    },
                                    0.82,
                                );
                            } }
                        }
                    }
                }
            }
        } else if self.station == Station::Commander && self.grab {
            self.update_carry(r, dt, CARRY_K_ROUND, CARRY_D_ROUND);
            if !input.mg_coax { self.drop_carried_round(true); }
        } else if self.grab || self.carried_casing.is_some() || self.fea.carried_fragment.is_some() {
            // Left the station with full hands: the round/case drops where you stood.
            self.release_carried_casing();
            self.fea_release_fragment(None);
            if self.grab {
                self.drop_carried_round(false);
            }
        }

        (ram_consumed_look,casing_consumed_look)
    }
}
