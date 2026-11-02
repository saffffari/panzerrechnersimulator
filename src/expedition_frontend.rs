//! Explicit succession on the retained Game. This screen never rebuilds a world, refills
//! equipment or pretends an incomplete expedition snapshot can restore a live battle.
use super::*;
use winit::keyboard::KeyCode;

impl WindowState {
    pub(super) fn expedition_relief_key(&mut self, code: KeyCode) {
        let candidates = self.game.expedition_successor_candidates();
        if code == KeyCode::Escape {
            // Keep the casualty screen and battle in memory. Returning from the menu cannot
            // silently restart a run or resurrect the old controlled actor.
            self.game.expedition_cancel_medical_wait();
            if !self.flush_expedition_before_leave() {return;}
            self.enter_menu(true);
            return;
        }
        if code == KeyCode::F5 {
            self.save_live_expedition();
            self.expedition_relief_message=self.pause_menu.message.clone();return;
        }
        if code == KeyCode::KeyW {
            if self
                .game
                .expedition_medical_wait_report()
                .is_some_and(|r| r.phase == expedition::MedicalWaitPhase::Waiting)
            {
                self.game.expedition_cancel_medical_wait();
                self.expedition_relief_message =
                    "WAIT CANCELLED. ELAPSED BATTLE TIME IS KEPT.".into();
            } else {
                match self.game.expedition_wait_for_medical_report(10.0) {
                    Ok(_) => self.expedition_relief_message.clear(),
                    Err(error) => {
                        eprintln!("[expedition] medical wait refused: {error:#}");
                        self.expedition_relief_message =
                            "NO MEDICAL WAIT IS AVAILABLE. THE CURRENT RUN IS KEPT.".into();
                    }
                }
            }
            return;
        }
        if candidates.is_empty() {
            return;
        }
        let count = candidates.len();
        self.expedition_relief_selected = self.expedition_relief_selected.min(count - 1);
        match code {
            KeyCode::ArrowUp => {
                self.expedition_relief_selected =
                    (self.expedition_relief_selected + count - 1) % count
            }
            KeyCode::ArrowDown => {
                self.expedition_relief_selected = (self.expedition_relief_selected + 1) % count
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let id = candidates[self.expedition_relief_selected].id;
                if self.game.expedition_successor_requires_exit(id) {
                    match self.game.expedition_request_successor_exit(id) {
                        Ok(_)=>self.expedition_relief_message=self.game.expedition_successor_exit_notice().unwrap_or_default(),
                        Err(error)=>self.expedition_relief_message=format!("EXIT REQUEST REFUSED: {error}"),
                    }
                    return;
                }
                match self.game.expedition_take_successor(id, &mut self.renderer) {
                    Ok(()) => {
                        self.expedition_relief_message.clear();
                        self.game.set_audio_silenced(false);
                        self.game.set_audio_level(1.0);
                        self.state = AppState::Playing;
                        self.clock_reset = true;
                        self.set_play_cursor(true);
                    }
                    Err(error) => {
                        eprintln!("[expedition] successor rejected: {error:#}");
                        self.expedition_relief_message =
                            "RELIEF COULD NOT DEPART. THE CURRENT RUN IS KEPT.".into();
                    }
                }
            }
            _ => {}
        }
    }

    /// Called only by the expedition Death-screen branch. Ordinary selection stays paused;
    /// the player's explicit W request advances the retained battle in bounded real steps.
    pub(super) fn advance_expedition_relief_wait(&mut self, dt: f32) {
        if !self
            .game
            .expedition_medical_wait_report()
            .is_some_and(|r| r.phase == expedition::MedicalWaitPhase::Waiting)
        {
            return;
        }
        match self
            .game
            .advance_expedition_medical_wait(dt, &mut self.renderer)
        {
            Ok(report) => {
                use expedition::MedicalWaitPhase::*;
                self.expedition_relief_message = self.game.expedition_successor_exit_notice().unwrap_or_else(||match report.phase {
                    Waiting => String::new(),
                    SuccessorReady => {
                        "A SURVIVOR IS FIT TO TAKE CONTROL. SELECT THEM BELOW.".into()
                    }
                    Elapsed => "REPORT RECEIVED. NO ADDITIONAL FIT SOLDIER YET.".into(),
                    PersonnelLost => "NO ONE REMAINS. THE EXPEDITION IS LOST.".into(),
                    Cancelled => "WAIT CANCELLED. ELAPSED BATTLE TIME IS KEPT.".into(),
                    Interrupted => "WAIT STOPPED BY A WORLD-STATE ERROR. THE RUN IS KEPT.".into(),
                });
            }
            Err(error) => {
                eprintln!("[expedition] medical wait stopped: {error:#}");
                self.game.expedition_cancel_medical_wait();
                self.expedition_relief_message = "WAIT STOPPED. THE CURRENT RUN IS KEPT.".into();
            }
        }
    }

    pub(super) fn render_expedition_relief(&mut self) {
        self.maybe_autosave_expedition();
        let candidates = self.game.expedition_successor_candidates();
        self.expedition_relief_selected = self
            .expedition_relief_selected
            .min(candidates.len().saturating_sub(1));
        let Some((frame, view)) = self.acquire() else {
            return;
        };
        self.renderer.ui_reset();
        ui::draw_black(&mut self.renderer);
        self.draw_expedition_save_notice();
        let ink = [0.87, 0.86, 0.81, 1.0];
        let dim = [0.57, 0.58, 0.55, 1.0];
        self.renderer
            .draw_text_bold("RELIEF DETAIL", 0.5, 0.10, 0.038, ink, ui::Align::Center);
        self.renderer.draw_text(
            "A MAN IS LOST. THE TIGER REMAINS IN THE FIELD.",
            0.5,
            0.18,
            0.027,
            ink,
            ui::Align::Center,
        );
        self.renderer.draw_text(
            "CHOOSE WHO WILL CONTINUE THE RECOVERY.",
            0.5,
            0.225,
            0.023,
            dim,
            ui::Align::Center,
        );
        if candidates.is_empty() {
            let living = self
                .game
                .expedition_state()
                .map_or(0, |state| state.living_people());
            self.renderer.draw_text(
                if living == 0 {
                    "NO ONE REMAINS."
                } else {
                    "NO SURVIVOR IS FIT TO TAKE CONTROL YET."
                },
                0.5,
                0.43,
                0.032,
                ink,
                ui::Align::Center,
            );
            self.renderer.draw_text(
                "SURVIVORS AND PATIENTS REMAIN RECORDED IN THIS RUN.",
                0.5,
                0.49,
                0.022,
                dim,
                ui::Align::Center,
            );
        } else {
            for (i, person) in candidates.iter().enumerate() {
                let selected = i == self.expedition_relief_selected;
                let role = match person.trained_role {
                    expedition::Seat::Commander => "COMMANDER",
                    expedition::Seat::Gunner => "GUNNER",
                    expedition::Seat::Loader => "LOADER",
                    expedition::Seat::Driver => "DRIVER",
                    expedition::Seat::Radioman => "RADIO / BOW MG",
                };
                let y = 0.295 + i as f32 * 0.043;
                if selected {
                    self.renderer
                        .fill_rect(0.17, y - 0.005, 0.66, 0.037, [0.10, 0.12, 0.10, 1.0]);
                }
                self.renderer.draw_text(
                    &format!(
                        "{} {} {}",
                        if selected { ">" } else { " " },
                        person.rank,
                        person.surname
                    ),
                    0.20,
                    y,
                    0.026,
                    ink,
                    ui::Align::Left,
                );
                let location = match person.location {
                    expedition::PersonLocation::Camp { .. } => "CAMP",
                    expedition::PersonLocation::Treatment { .. } => "STABILIZED / BED",
                    expedition::PersonLocation::OnFoot { .. } => "IN THE FIELD",
                    _ => "ABOARD",
                };
                self.renderer.draw_text(
                    &format!("{role} / {location}"),
                    0.80,
                    y,
                    0.023,
                    if selected { ink } else { dim },
                    ui::Align::Right,
                );
            }
        }
        let waiting = self
            .game
            .expedition_medical_wait_report()
            .filter(|r| r.phase == expedition::MedicalWaitPhase::Waiting);
        let timing = waiting.map_or_else(
            || "THE BATTLE IS PAUSED. W WAITS UP TO 10 SECONDS; COMBAT CONTINUES.".into(),
            |r| {
                format!(
                    "AWAITING RELIEF {:.1}/{:.1}s - THE BATTLE CONTINUES.",
                    r.elapsed_seconds, r.requested_seconds
                )
            },
        );
        self.renderer
            .draw_text(&timing, 0.5, 0.82, 0.021, dim, ui::Align::Center);
        self.renderer.draw_text(
            &self.expedition_relief_message,
            0.5,
            0.865,
            0.022,
            ink,
            ui::Align::Center,
        );
        self.renderer.draw_text(
            "UP / DOWN SELECT   ENTER CONTROL / EXIT   W WAIT / CANCEL   F5 SAVE   ESC TITLE",
            0.5,
            0.92,
            0.024,
            ink,
            ui::Align::Center,
        );
        self.renderer.render_ui_to_surface(
            &self.device,
            &self.queue,
            &view,
            self.config.format,
            self.config.width,
            self.config.height,
        );
        frame.present();
    }
}

/// A completed expedition remains the same paused world. No path here runs simulation,
/// starts another dispatch, chooses a successor or constructs a replacement game.
impl WindowState {
    pub(super) fn expedition_finished(&self)->bool {
        self.play_mode==Some(PlayMode::Expedition)&&self.game.expedition_state()
            .is_some_and(|s|s.status==expedition::RunStatus::Victory)
    }
    pub(super) fn expedition_completion_panel(&self)->bool {
        self.expedition_finished()&&self.pause_menu.page==ui::PausePage::Main
    }
    pub(super) fn enter_expedition_completed(&mut self) {
        self.state=AppState::Paused;self.pause_menu.show(ui::PausePage::Main);
        self.expedition_complete_view=false;self.show_controls=false;self.pencil_down=false;
        self.game.set_audio_paused(true);self.set_play_cursor(false);
        self.pause_menu.message="FINAL ORDER COMPLETED. THE BATTLEFIELD IS PAUSED.".into();
        // Bypass the periodic throttle: the terminal result must be retained even if the last
        // ordinary autosave happened a fraction of a second before the objective completed.
        if self.flush_expedition_before_leave() {
            self.pause_menu.message="COMPLETED EXPEDITION SAVED.".into();
        }
    }
    pub(super) fn expedition_completion_row(&self,p:glam::Vec2)->Option<usize> {
        if !(0.22..=0.78).contains(&p.x){return None;}
        if self.expedition_complete_view {return (0.88..=0.96).contains(&p.y).then_some(0);}
        if (0.66..=0.725).contains(&p.y){Some(0)}else if (0.745..=0.81).contains(&p.y){Some(1)}else{None}
    }
    pub(super) fn expedition_completed_key(&mut self,code:KeyCode) {
        match code {
            KeyCode::F5=>self.save_live_expedition(),
            KeyCode::ArrowUp|KeyCode::ArrowDown if !self.expedition_complete_view=>self.pause_menu.selected=(self.pause_menu.selected+1)%2,
            KeyCode::Escape if self.expedition_complete_view=>self.expedition_complete_view=false,
            KeyCode::Escape=>{if self.flush_expedition_before_leave(){self.enter_menu(true);}},
            KeyCode::Enter|KeyCode::NumpadEnter|KeyCode::Space=>{
                if self.expedition_complete_view {self.expedition_complete_view=false;}
                else if self.pause_menu.selected==0 {self.expedition_complete_view=true;}
                else if self.flush_expedition_before_leave(){self.enter_menu(true);}
            },_=>{}
        }
    }
    pub(super) fn render_expedition_completed(&mut self) {
        let Some((frame,view))=self.acquire() else{return;};
        self.renderer.ui_reset();
        let ink=[0.87,0.86,0.81,1.];let dim=[0.60,0.61,0.57,1.];
        if self.expedition_complete_view {
            self.renderer.render_to_surface(&self.device,&self.queue,&view,self.config.format);
            self.renderer.fill_rect(0.17,0.875,0.66,0.085,[0.015,0.02,0.015,0.93]);
            self.renderer.draw_text("COMPLETED BATTLEFIELD / TIME STOPPED",0.5,0.89,0.022,ink,ui::Align::Center);
            self.renderer.draw_text("ENTER / ESC / LMB TO RETURN TO RESULTS",0.5,0.93,0.018,dim,ui::Align::Center);
            self.renderer.render_hud_over_surface(&self.device,&self.queue,&view,self.config.format,self.config.width,self.config.height);
        } else {
            ui::draw_black(&mut self.renderer);
            self.renderer.draw_text_bold("EXPEDITION COMPLETE",0.5,0.14,0.041,ink,ui::Align::Center);
            let summary=self.game.expedition_state().map(|s| {
                let final_site=s.dispatches.iter().rev().find_map(|d|match d.kind {
                    expedition::DispatchKind::Finale{site} if d.outcome==Some(expedition::OperationOutcome::Success)=>Some(site),_=>None});
                let name=final_site.and_then(|id|s.sites.iter().find(|site|site.id==id)).map_or("Final corridor",|site|site.name.as_str());
                let dead=s.people.iter().filter(|p|p.casualty.dead).count();
                (name.to_uppercase(),format!("{} PEOPLE SURVIVED / {} LOST",s.living_people(),dead),
                    format!("{} DISPATCHES / {:.0} MINUTES IN THE FIELD",s.dispatches.len(),s.simulation_seconds/60.))
            });
            if let Some((order,people,time))=summary {
                self.renderer.draw_text_bold(&format!("{order} / SECURED"),0.5,0.28,0.027,ink,ui::Align::Center);
                self.renderer.draw_text(&people,0.5,0.38,0.024,ink,ui::Align::Center);
                self.renderer.draw_text(&time,0.5,0.43,0.021,dim,ui::Align::Center);
            }
            self.renderer.draw_text("THE TIGER, SURVIVORS AND BATTLEFIELD REMAIN IN THIS SAVE.",0.5,0.53,0.020,dim,ui::Align::Center);
            for (i,label) in ["REVIEW THE PAUSED BATTLEFIELD","RETURN TO TITLE"].into_iter().enumerate() {
                let y=0.68+i as f32*0.085;
                if self.pause_menu.selected.min(1)==i {self.renderer.fill_rect(0.22,y-0.014,0.56,0.06,[0.10,0.12,0.10,1.]);}
                self.renderer.draw_text(label,0.5,y,0.025,ink,ui::Align::Center);
            }
            self.renderer.draw_text(&self.pause_menu.message,0.5,0.86,0.020,dim,ui::Align::Center);
            self.renderer.draw_text("UP / DOWN SELECT   ENTER CONFIRM   F5 SAVE   ESC TITLE",0.5,0.93,0.020,dim,ui::Align::Center);
            self.renderer.render_ui_to_surface(&self.device,&self.queue,&view,self.config.format,self.config.width,self.config.height);
        }
        frame.present();
    }
}
