//! Window entry points for complete, versioned battlefield saves. The candidate is imported
//! before the live Game is replaced. No save operation grants equipment or advances time.
use super::*;
use anyhow::{Context,Result,ensure};
use crate::expedition_world_save::{self as storage,FrontendState};

#[derive(Clone,Debug,PartialEq,Eq)]
struct EventMark { dispatches:usize, resolved:usize, active:Option<u64>, controlled:Option<u64>,
    dead:u16, camp:[u64;3], status:expedition::RunStatus }
impl EventMark {
    fn read(s:&expedition::Expedition)->Self {Self {
        dispatches:s.dispatches.len(),resolved:s.dispatches.iter().filter(|d|d.outcome.is_some()).count(),
        active:s.active_dispatch.map(|id|id.0),controlled:s.controlled_person.map(|id|id.0),
        dead:s.people.iter().enumerate().fold(0,|bits,(i,p)|bits|((p.casualty.dead as u16)<<i)),
        camp:[s.camp.position.x.to_bits(),s.camp.position.y.to_bits(),s.camp.position.z.to_bits()],status:s.status,
    }}
}
#[derive(Default)]
pub(super) struct SaveSession {
    last_simulation:f64,last_attempt:Option<Instant>,mark:Option<EventMark>,
    notice:Option<(Instant,String)>,
}
impl SaveSession {
    fn due(&self,now:Instant,seconds:f64,mark:&EventMark)->bool {
        if self.last_attempt.is_some_and(|t|now.duration_since(t).as_secs_f32()<15.) {return false;}
        self.mark.as_ref()!=Some(mark)||seconds-self.last_simulation>=120.
    }
}

impl WindowState {
    fn expedition_frontend_snapshot(&self)->FrontendState {FrontendState {
        map_cursor:self.map_cursor,pencil_marks:self.pencil_marks.clone(),map_pins:self.map_pins.clone(),
        show_map:self.game.map_held,relief_selected:self.expedition_relief_selected.min(15),
    }}
    fn write_expedition_save(&mut self,automatic:bool)->Result<()> {
        ensure!(self.play_mode==Some(PlayMode::Expedition),"this is not an expedition");
        let world=self.game.expedition_world_recipe()?;
        let mut snapshot=self.game.export_expedition_world(&self.renderer,world)?;
        snapshot.frontend=Some(self.expedition_frontend_snapshot());
        let path=if automatic {storage::autosave_path()?}else{storage::default_path()?};
        storage::save(&path,&snapshot)?;
        let state=self.game.expedition_state().context("expedition disappeared during save")?;
        self.expedition_io.last_simulation=state.simulation_seconds;
        self.expedition_io.mark=Some(EventMark::read(state));
        self.menu.expedition_save_available=true;
        println!("[expedition save] {} at {:.3}s",path.display(),snapshot.simulation_seconds);
        Ok(())
    }
    /// Explicitly leaving a retained expedition flushes once even inside the periodic throttle.
    /// Failure keeps the current session and offers the existing Save/Exit actions again.
    pub(super) fn flush_expedition_before_leave(&mut self)->bool {
        if self.play_mode!=Some(PlayMode::Expedition)||self.game.expedition_state().is_none() {return true;}
        self.expedition_io.last_attempt=Some(Instant::now());
        let result=self.write_expedition_save(true);self.io_clock_reset=true;
        if let Err(error)=result {
            eprintln!("[expedition exit save] {error:#}");
            let message=format!("SAVE FAILED - SESSION KEPT: {}",brief_error(&error));
            self.pause_menu.message=message.clone();self.expedition_relief_message=message.clone();
            self.expedition_io.notice=Some((Instant::now(),message));return false;
        }
        true
    }
    pub(super) fn save_live_expedition(&mut self) {
        self.expedition_io.last_attempt=Some(Instant::now());
        let result=self.write_expedition_save(false);self.io_clock_reset=true;
        self.pause_menu.message=match result {Ok(())=>"EXPEDITION SAVED - THE BATTLEFIELD IS RETAINED".into(),Err(error)=>{
            eprintln!("[expedition save] {error:#}");
            format!("SAVE FAILED - CURRENT GAME KEPT: {}",brief_error(&error))
        }};
        self.expedition_io.notice=Some((Instant::now(),self.pause_menu.message.clone()));
    }
    /// Called once after each ordinary live update, including held-map and medical-wait paths.
    /// A failed attempt is rate-limited too; it never replaces an older valid generation.
    pub(super) fn maybe_autosave_expedition(&mut self) {
        if self.play_mode!=Some(PlayMode::Expedition) {return;}
        let Some(state)=self.game.expedition_state() else{return;};
        let mark=EventMark::read(state);let now=Instant::now();
        if !self.expedition_io.due(now,state.simulation_seconds,&mark) {return;}
        self.expedition_io.last_attempt=Some(now);
        let result=self.write_expedition_save(true);self.io_clock_reset=true;
        let notice=match result {Ok(())=>"EXPEDITION AUTOSAVED".into(),Err(error)=>{
            eprintln!("[expedition autosave] {error:#}");
            let text=format!("AUTOSAVE FAILED - USE PAUSE TO RETRY: {}",brief_error(&error));
            self.pause_menu.message=text.clone();text
        }};
        self.expedition_io.notice=Some((Instant::now(),notice));
    }
    pub(super) fn draw_expedition_save_notice(&mut self) {
        if let Some((time,text))=&self.expedition_io.notice {
            if time.elapsed().as_secs()<10 {self.renderer.draw_text(text,0.5,0.045,0.019,[0.88,0.85,0.72,1.],ui::Align::Center);}
        }
    }
    /// Manual pause load uses the deliberate manual save. Title Resume chooses the newest
    /// readable complete generation, including autosaves, and reports fallback explicitly.
    pub(super) fn load_live_expedition(&mut self,from_title:bool) {
        if self.lan.is_some() {self.pause_menu.message="LEAVE THE NETWORK SESSION BEFORE LOADING".into();return;}
        let prior_manifest=act_manifest::selected_runtime_manifest().map(|(_,path)|path);
        let result=(||->Result<(Renderer,Game,FrontendState,bool)> {
            let mut failures=Vec::new();
            let (saved,recovered)=if from_title {
                let mut chosen=None;
                for path in storage::resume_paths()? {
                    match storage::load(&path).and_then(|save|{save.verify_for_resume()?;ensure!(save.frontend.is_some(),"save has no window/map record");Ok(save)}) {
                        Ok(save)=>{chosen=Some((save,!failures.is_empty()||path.extension().is_some_and(|ext|ext=="bak")));break;}
                        Err(error)=>failures.push(format!("{}: {error:#}",path.display())),
                    }
                }
                chosen.with_context(||if failures.is_empty(){"No expedition has been saved".into()}else{failures.join("; ")})?
            } else {
                let loaded=storage::load_with_recovery(&storage::default_path()?)?;
                (loaded.save,loaded.recovered_from_backup)
            };
            saved.verify_for_resume()?;
            let frontend=saved.frontend.clone().context("This diagnostic save has no window/map record")?;
            frontend.validate()?;
            let (r,g)=Game::build_expedition_resume_candidate(&self.device,&self.queue,&saved,true)?;
            ensure!(frontend.show_map==g.map_held,"saved paper/window state differs");
            Ok((r,g,frontend,recovered))
        })();
        self.clock_reset=true;
        let (mut renderer,mut game,frontend,recovered)=match result {
            Ok(candidate)=>candidate,
            Err(error)=>{
                let _=act_manifest::select_runtime_manifest(prior_manifest);
                eprintln!("[expedition resume] {error:#}");
                self.pause_menu.message=format!("RESUME FAILED - CURRENT GAME KEPT: {}",brief_error(&error));
                return;
            }
        };
        // Commit point. Construction/import above could fail without replacing the current run.
        self.game.set_audio_silenced(true);
        renderer.master_retro=self.renderer.master_retro;
        // A previously authorized wait is never automatically restarted by opening a save.
        game.expedition_cancel_medical_wait();game.set_audio_paused(true);
        self.renderer=renderer;self.game=game;
        self.play_mode=Some(PlayMode::Expedition);self.current_act=self.game.act.id;
        self.campaign=None;self.pending_final_defeat=None;self.campaign_total_kills=0;
        self.lan=None;self.lan_browser=None;self.lan_scene=None;self.lan_status.clear();
        self.lan_crew=lan::CrewFrame::default();self.lan_main_gun_cursor.clear();self.lan_applied_role=None;
        self.couch_active=false;self.couch_connected.clear();self.couch_kia_started.clear();self.couch_crew.reset(self.couch_input_mode);
        self.show_controls=false;self.controls_audio_paused=false;self.show_map=frontend.show_map;
        self.map_cursor=frontend.map_cursor;self.pencil_marks=frontend.pencil_marks;self.map_pins=frontend.map_pins;self.pencil_down=false;
        self.expedition_relief_selected=frontend.relief_selected;self.expedition_relief_message.clear();self.expedition_complete_view=false;
        self.menu.resume_available=false;self.menu.expedition_save_available=true;
        self.pause_menu.expedition=true;self.pause_menu.show(ui::PausePage::Main);
        self.pause_menu.message=if recovered {"RECOVERED AN EARLIER VALID SAVE - REVIEW THE RETAINED WORLD"}else{"EXPEDITION RESTORED - SELECT RESUME WHEN READY"}.into();
        self.state=if self.game.expedition_succession_pending()||self.game.expedition_state().is_some_and(|s|s.status==expedition::RunStatus::PersonnelLost) {
            self.expedition_relief_message=self.pause_menu.message.clone();AppState::Death{t:0.,card:None}
        }else{AppState::Paused};
        if self.expedition_finished() {self.state=AppState::Paused;self.pause_menu.message="COMPLETED EXPEDITION RESTORED. BATTLE TIME REMAINS STOPPED.".into();}
        self.expedition_io=SaveSession::default();
        if let Some(s)=self.game.expedition_state() {self.expedition_io.mark=Some(EventMark::read(s));self.expedition_io.last_simulation=s.simulation_seconds;}
        self.expedition_io.last_attempt=Some(Instant::now());
        self.death_fade=None;self.ft_last=None;self.x_exit_frac=0.;self.autopilot=None;
        self.apply_graphics_preferences(false);self.set_play_cursor(false);
    }
}
fn brief_error(error:&anyhow::Error)->String {
    let message=error.chain().last().unwrap_or(error.as_ref()).to_string();
    message.chars().take(100).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn autosave_uses_real_simulation_progress_and_bounded_event_attempts() {
        let mut state=expedition::Expedition::new(22);let mark=EventMark::read(&state);
        let now=Instant::now();let mut scheduler=SaveSession::default();
        assert!(scheduler.due(now,0.,&mark));
        scheduler.mark=Some(mark.clone());scheduler.last_attempt=Some(now);
        assert!(!scheduler.due(now+std::time::Duration::from_secs(120),0.,&mark));
        assert!(scheduler.due(now+std::time::Duration::from_secs(120),120.,&mark));
        state.people[0].casualty.dead=true;let death=EventMark::read(&state);
        assert!(!scheduler.due(now+std::time::Duration::from_secs(1),1.,&death));
        assert!(scheduler.due(now+std::time::Duration::from_secs(15),1.,&death));
        scheduler.last_attempt=Some(now+std::time::Duration::from_secs(15));
        assert!(!scheduler.due(now+std::time::Duration::from_secs(16),1.,&death));
    }
}
