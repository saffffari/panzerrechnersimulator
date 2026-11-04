//! Two independent caches: simulation output for a free camera, and images for a fixed view.
use super::*;
use std::path::PathBuf;
const MAX_BYTES: usize = 4_000_000_000;

#[derive(Default)]
pub(super) struct Cache {
    frames: Vec<Arc<crate::renderer::SceneFrame>>,
    baking: bool, pending_frame: Option<u32>, bytes: usize,
    key: serde_json::Value, lamp_gain: f32,
    index: usize, clock: f32, applied: Option<(usize, u64)>,
    export_after_bake: bool, play_after_bake: bool,
    render: Option<Render>,
    image_dir: Option<PathBuf>, image_key: serde_json::Value, preview: bool,
}
struct Render {
    dir: PathBuf, key: serde_json::Value, next: usize,
    recorder: Recorder,
}
impl Cache {
    pub(super) fn label(&self) -> String {
        let state = if self.baking { "CACHING" } else if self.frames.is_empty() { "CACHE EMPTY" } else { "CACHED" };
        format!("{state}: {} FRAMES / {:.0} MB | RESET CLEARS CACHE", self.frames.len(), self.bytes as f64 / 1e6)
    }
    pub(super) fn busy(&self) -> bool { self.baking || self.render.is_some() }
    pub(super) fn rendering(&self) -> bool { self.render.is_some() }
    fn ready(&self) -> bool { !self.baking && !self.frames.is_empty() }
}
impl Range {
    pub(super) fn step_cached_frame(&mut self, forward: bool, count: u32) -> bool {
        if !self.cache.ready() { return false; }
        let index=if forward {self.cache.index.saturating_add(count as usize)}
            else {self.cache.index.saturating_sub(count as usize)};
        self.cached_seek(index)
    }
    pub(super) fn start_cached_shot(&mut self) {
        if self.cached_seek(0) { self.playing = false; self.cache.clock = 0.0; }
        else { self.reset(); }
    }
    pub(super) fn play_cached_shot(&mut self) {
        if self.cache.baking { self.playing = !self.playing; return; }
        if self.cache.ready() && self.cache.index + 1 == self.cache.frames.len() && !self.playing {
            self.cache.index = 0; self.cache.clock = 0.0;
        }
        if self.cache.ready() { self.playing = !self.playing; }
        else { self.cache_simulation(); self.cache.play_after_bake = true; }
    }
    fn simulation_key(&self) -> serde_json::Value {
        let state = self.state_json();
        let mut scene = state["scene"].clone();
        scene.as_object_mut().unwrap().remove("cabin_lamps");
        serde_json::json!({"shot":state["shot"], "scene":scene, "tissue":state["tissue"],
            "tiger":state["tiger"], "engine":state["engine"]})
    }
    fn image_key(&self) -> serde_json::Value {
        let state = self.state_json();
        serde_json::json!({"camera":state["camera"],"section":state["section"],"sky":state["sky"],
            "look":state["look"],"lamp_gain":state["lamp_gain"],"lamps":self.scene.cabin_lamps,
            "wear":state["wear"],"snow":state["snow"],"effects":state["effects"],
            "size":[self.r.iw,self.r.ih],"crop":self.crop()})
    }
    pub(super) fn clear_caches(&mut self) {
        self.finish_cached_render();
        self.cache = Cache::default();
        self.r.enable_scene_cache_capture(false);
    }
    pub(super) fn cache_simulation(&mut self) {
        if self.cache.busy() {
            self.status = "CACHE KEPT: RESUME WITH PLAY; RESET DISCARDS IT".into();
            return;
        }
        self.free_fire = false;
        if self.cache.frames.is_empty() {
            // The current fresh scene becomes the first shot; no implicit reset.
            self.fire_now();
        } else {
            // The physical world remains at the calculated end while cached views replay.
            // New shots extend that world and keep all earlier cached frames.
            let last = self.cache.frames.len() as u32 - 1;
            let elapsed = self.frame_time(last);
            self.apply_tuning();
            self.fire();
            self.shot_t = elapsed;
            let extra = self.total_frames(&self.scene);
            if let Some(run) = &mut self.run {
                run.total = last.saturating_add(1).saturating_add(extra);
                self.run_tape.resize(run.total as usize, usize::MAX);
            }
            self.frame_at = Some(last);
            self.past_run = false;
            self.mode = Mode::Live;
            self.cache.applied = None;
        }
        self.r.enable_scene_cache_capture(true);
        self.cache.baking = true;
        self.cache.key = self.simulation_key();
        self.cache.lamp_gain = review_lamp_gain().max(1e-6);
        self.playing = true;
        self.status = "FIRST PASS: CALCULATING AND CACHING; CAMERA STAYS FREE".into();
    }
    // Cache the picture shown during calculation too. Camera/look changes begin a new
    // image sequence; missing pictures are rendered from the unchanged simulation cache.
    fn cache_first_pass_image(&mut self, index: usize) {
        let key = self.image_key();
        if self.cache.image_key != key || self.cache.image_dir.is_none() {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
            let dir = PathBuf::from(format!("target/fea_range_frames/{stamp}"));
            if std::fs::create_dir_all(&dir).is_err() { return; }
            self.cache.image_dir = Some(dir);
            self.cache.image_key = key;
        }
        self.pose_view();
        if let Ok(raw) = self.r.render_to_rgba(&self.device, &self.queue, self.r.iw, self.r.ih) {
            let (rgba,w,h) = self.crop_rgba(&raw);
            if let Some(dir) = &self.cache.image_dir {
                let _ = image::save_buffer(dir.join(format!("{index:06}.png")), &rgba,w,h,image::ColorType::Rgba8);
            }
            self.upload_image(&rgba,w,h);
            self.last_rgba = Some((rgba,w,h));
            self.captured = true;
        }
    }
    pub(super) fn cached_seek(&mut self, index: usize) -> bool {
        if !self.cache.ready() { return false; }
        self.cache.index = index.min(self.cache.frames.len()-1);
        self.frame_at = Some(self.cache.index as u32);
        self.shot_t = self.frame_time(self.cache.index as u32);
        self.mode = Mode::Live;
        self.past_run = false;
        self.status = format!("CACHED SIMULATION FRAME {}/{}: NO PHYSICS STEP",self.cache.index+1,self.cache.frames.len());
        true
    }
    /// Returns true when the cache owns this tick. No call to Game::update exists on playback.
    pub(super) fn tick_cache(&mut self, dt: f32) -> bool {
        if self.cache.baking {
            self.captured = false;
            if !self.playing { return true; }
            self.g.settle_casualty_tissue(false);
            if self.g.casualty_tissue_pending() { return true; }
            if self.cache.pending_frame.is_none() {
                if let Some(k) = self.next_run_frame() {
                    self.run_frame(k);
                    self.cache.pending_frame = Some(k);
                }
            }
            if self.g.casualty_tissue_pending() { return true; }
            if let Some(k) = self.cache.pending_frame.take() {
                // All crew must be in the cache, including a body hidden by the live station view.
                self.g.drive_singleplayer_crew_rigs_for_view(None,&mut self.r);
                self.g.drive_physical_crew_casualties(&mut self.r);
                let previous = self.cache.frames.last().map(Arc::as_ref);
                match self.r.capture_scene_frame(previous) {
                    Ok(frame) if self.cache.bytes.saturating_add(frame.bytes) <= MAX_BYTES => {
                        self.cache.bytes += frame.bytes;
                        self.cache.frames.push(Arc::new(frame));
                        self.cache_first_pass_image(k as usize);
                        self.status = format!("CACHING SIMULATION {}/{} ({:.0} MB)",k+1,
                            self.run.map_or(0,|r|r.total),self.cache.bytes as f64/1e6);
                    }
                    result => {
                        self.cache.baking = false; self.playing = false;
                        self.status = match result { Err(e)=>format!("CACHE FAILED: {e:#}"),
                            _=>"CACHE FULL (4 GB): EXISTING FRAMES KEPT; RESET TO CLEAR".into() };
                        return true;
                    }
                }
            }
            if self.next_run_frame().is_none() {
                self.cache.baking = false; self.cache.index = self.cache.frames.len().saturating_sub(1); self.playing = false;
                self.status = format!("SHOT CACHED: {} FRAMES; PLAY REPLAYS; CAMERA AND LOOK STAY FREE",self.cache.frames.len());
                if self.cache.export_after_bake { self.start_cached_render(); }
            }
            return true;
        }
        if !self.cache.ready() { return false; }
        if self.cache.render.is_some() {
            self.render_cache_tick(); return true;
        }
        if let Mode::Scrub(i) = self.mode {
            self.cache.index = i.min(self.cache.frames.len()-1);
        } else if self.playing {
            self.cache.clock += dt * 30.0 * self.time_speed;
            let steps = self.cache.clock.floor() as usize;
            self.cache.clock -= steps as f32;
            let start = self.clip.map_or(0,|c|c.0 as usize).min(self.cache.frames.len()-1);
            let end = self.clip.map_or(self.cache.frames.len()-1,|c|c.1 as usize).min(self.cache.frames.len()-1);
            let next = self.cache.index.saturating_add(steps);
            if next > end {
                if self.looping { self.cache.index = start + (next-start) % (end-start+1); }
                else { self.cache.index = end; self.playing = false; }
            } else { self.cache.index = next; }
        }
        // A valid image needs only a blit: no scene uploads and no ray tracing.
        if self.cache.image_key == self.image_key() {
            if let Some(dir) = &self.cache.image_dir {
                if self.shown_frame == Some(self.cache.index) {
                    self.captured = true; return true;
                }
                let path = dir.join(format!("{:06}.png",self.cache.index));
                if let Ok(image) = image::open(path) {
                    let rgba = image.into_rgba8(); let (w,h)=rgba.dimensions();
                    self.upload_image(&rgba,w,h); self.captured = true;
                    self.last_rgba = Some((rgba.into_raw(),w,h));
                    self.shown_frame = Some(self.cache.index);
                    self.frame_at = Some(self.cache.index as u32);
                    self.shot_t = self.frame_time(self.cache.index as u32);
                    return true;
                }
            }
        }
        self.apply_cached_frame();
        true
    }
    fn apply_cached_frame(&mut self) {
        let frame = self.cache.frames[self.cache.index].clone();
        let gain = if self.scene.cabin_lamps { review_lamp_gain()/self.cache.lamp_gain } else { 0.0 };
        let key = (self.cache.index,((gain.to_bits() as u64)<<2)
            | u64::from(self.eye_fx.concussion) | (u64::from(self.eye_fx.flash)<<1));
        if self.cache.applied == Some(key) { return; }
        if let Err(e) = self.r.restore_scene_frame(&frame,gain) {
            self.status = format!("CACHE RESTORE FAILED: {e:#}"); self.playing = false;
        }
        self.cache.applied = Some(key);
        self.frame_at = Some(self.cache.index as u32);
        self.shot_t = self.frame_time(self.cache.index as u32);
        self.past_run = false;
    }
    pub(super) fn pose_cached_view(&mut self) -> bool {
        if !self.cache.ready() { return false; }
        self.apply_sky(); self.place_camera();
        self.r.optic_mask = 0.0; self.r.reticle = false;
        let local=self.g.player.hull_model().inverse().transform_point3(self.eye);
        let inside=local.x.abs()<1.6 && local.y>0.4 && local.y<2.9 && local.z> -3.2 && local.z<3.4;
        self.r.interior_light=if inside {1.0} else {0.0};
        self.r.weather_shelter=if inside {1.0} else {0.0};
        self.r.set_interior_atmosphere_enabled(inside);
        self.r.set_hovered_meshes(&[]);
        let meshes = self.section_meshes();
        self.r.set_film_section(self.section_plane(),&meshes);
        if let Some(w)=self.wear { self.r.wear=w; }
        if let Some([tank,world])=self.snow { self.r.player_snow_load=tank; self.r.world_snow_load=world; }
        self.apply_eye_fx();
        true
    }
    pub(super) fn render_cached_shot(&mut self) {
        if self.cache.render.is_some() { self.finish_cached_render(); return; }
        if self.cache.baking {
            self.cache.export_after_bake=true;
            self.status="RENDER QUEUED: SIMULATION CACHE IS STILL BUILDING".into();return;
        }
        if !self.cache.ready() {
            self.cache_simulation(); self.cache.export_after_bake=true;
        } else { self.start_cached_render(); }
    }
    fn start_cached_render(&mut self) {
        self.playing=false;
        let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        let dir=if self.cache.image_key == self.image_key() {
            self.cache.image_dir.clone().unwrap_or_else(||PathBuf::from(format!("target/fea_range_frames/{stamp}")))
        } else {PathBuf::from(format!("target/fea_range_frames/{stamp}"))};
        let result=(|| -> anyhow::Result<Render> {
            std::fs::create_dir_all(&dir)?;
            let key=self.image_key();
            std::fs::write(dir.join("view.json"),serde_json::to_vec_pretty(&key)?)?;
            let path=crate::capture_target(&format!("fea_range_render_{stamp}.mp4"));
            let (_,w,h)=self.crop_rgba(&vec![0;(self.r.iw*self.r.ih*4) as usize]);
            let size=format!("{w}x{h}"); let k=display_scale(self.r.ih);
            let scale=format!("scale=iw*{k}:ih*{k}:flags=neighbor,crop=trunc(iw/2)*2:trunc(ih/2)*2");
            let encoder=std::process::Command::new("ffmpeg").args(["-v","error","-nostdin","-y","-f","rawvideo",
                "-pixel_format","rgba","-video_size",&size,"-framerate","30","-i","pipe:0","-an","-vf",&scale,
                "-c:v","libx264","-threads","4","-preset","medium","-crf","10","-pix_fmt","yuv444p","-movflags","+faststart",&path])
                .stdin(std::process::Stdio::piped()).spawn()?;
            Ok(Render {dir,key,next:0,recorder:Recorder {path,size:(w,h),encoder,frames:0}})
        })();
        match result { Ok(job)=>{ self.cache.render=Some(job); self.r.reset_history(); },
            Err(e)=>self.status=format!("RENDER FAILED: {e:#}") }
    }
    fn render_cache_tick(&mut self) {
        let mut job=self.cache.render.take().unwrap();
        if job.key != self.image_key() {
            self.cache.render=Some(job); self.finish_cached_render();
            self.status="RENDER STOPPED: VIEW SETTINGS CHANGED; RENDER AGAIN FOR THE NEW VIEW".into(); return;
        }
        self.cache.index=job.next;
        self.apply_cached_frame(); self.pose_cached_view();
        let result=(|| -> anyhow::Result<()> {
            let path=job.dir.join(format!("{:06}.png",job.next));
            let (rgba,w,h)=if let Ok(image)=image::open(&path) {
                let rgba=image.into_rgba8();let (w,h)=rgba.dimensions();(rgba.into_raw(),w,h)
            } else {
                let raw=self.r.render_to_rgba(&self.device,&self.queue,self.r.iw,self.r.ih)?;
                self.crop_rgba(&raw)
            };
            anyhow::ensure!((w,h)==job.recorder.size,"viewport resized during render");
            image::save_buffer(job.dir.join(format!("{:06}.png",job.next)),&rgba,w,h,image::ColorType::Rgba8)?;
            use std::io::Write;
            job.recorder.encoder.stdin.as_mut().ok_or_else(||anyhow::anyhow!("encoder closed"))?.write_all(&rgba)?;
            self.upload_image(&rgba,w,h); self.captured=true; self.last_rgba=Some((rgba,w,h));
            Ok(())
        })();
        match result {
            Ok(())=>{job.next+=1;job.recorder.frames+=1;
                self.status=format!("RENDERING FRAME {}/{} (OFFLINE; EVERY FRAME KEPT)",job.next,self.cache.frames.len());},
            Err(e)=>{self.cache.render=Some(job);self.finish_cached_render();self.status=format!("RENDER FAILED: {e:#}");return;}
        }
        let done=job.next==self.cache.frames.len();
        if done {self.cache.image_dir=Some(job.dir.clone());self.cache.image_key=job.key.clone();}
        self.cache.render=Some(job);
        if done {self.finish_cached_render();}
    }
    pub(super) fn finish_cached_render(&mut self) {
        if let Some(mut job)=self.cache.render.take() {
            drop(job.recorder.encoder.stdin.take());
            let ok=job.recorder.encoder.wait().is_ok_and(|s|s.success());
            self.status=format!("{} {} FRAMES TO {}",if ok {"RENDERED"} else {"ENCODER FAILED:"},job.recorder.frames,job.recorder.path);
            println!("[fea-range] {}",self.status);
        }
    }
}

impl Range {
    pub(super) fn cache_probe(&mut self) -> anyhow::Result<()> {
        self.scene.slow_s=0.02;self.scene.run_s=0.60;
        self.cache_simulation();
        let started=std::time::Instant::now();
        while self.cache.baking {
            anyhow::ensure!(started.elapsed().as_secs()<120,"simulation cache probe timed out");
            self.tick(1.0/30.0);
            if self.g.casualty_tissue_pending() { std::thread::sleep(std::time::Duration::from_millis(2)); }
        }
        anyhow::ensure!(self.cache.ready(),"simulation cache is empty: {}",self.status);
        // Exercise 3D restoration rather than the first-pass image fast path.
        self.cache.image_key = serde_json::Value::Null;
        let clock=self.g.armor_tick; let digest=self.digest();
        let indices=[0,self.cache.frames.len()-1,0,self.cache.frames.len()/2];
        for index in indices {
            self.cached_seek(index);self.tick(1.0/30.0);
            let expected=self.cache.frames[index].content_hash();
            let actual=self.r.capture_scene_frame(None)?.content_hash();
            anyhow::ensure!(expected==actual,"scene roundtrip differs at frame {index}: {expected:x} != {actual:x}");
            self.eye.x+=0.05;self.pose_cached_view();
            anyhow::ensure!(self.g.armor_tick==clock && self.digest()==digest,"cache playback advanced simulation");
        }
        self.step_frame(true,1);self.tick(1.0/30.0);
        self.step_frame(false,1);self.tick(1.0/30.0);
        self.section.on=!self.section.on;
        self.tick(1.0/30.0);self.pose_cached_view();
        anyhow::ensure!(self.g.armor_tick==clock && self.digest()==digest,"cached arrow stepping or view edits advanced simulation");
        let rendered=self.r.render_to_rgba(&self.device,&self.queue,self.r.iw,self.r.ih)?;
        anyhow::ensure!(!rendered.is_empty(),"cached scene did not render");
        println!("[cache-probe] {} frames, {:.1} MB; forward/backward scene hashes match; moved camera renders; simulation clock and damage unchanged",
            self.cache.frames.len(),self.cache.bytes as f64/1e6);
        self.render_cached_shot();
        while self.cache.render.is_some() {self.tick(1.0/30.0);}
        anyhow::ensure!(self.cache.image_dir.is_some(),"offline frame export did not finish: {}",self.status);
        self.cached_seek(0);self.tick(1.0/30.0);
        anyhow::ensure!(self.captured,"rendered frame was not served from the image cache");
        println!("[cache-probe] PNG frame cache and offline MP4 completed; cached image playback skips rendering");
        self.playing = false;
        let kept = self.cache.frames.len();
        self.shot.range_m += 1.0;
        self.tick(1.0/30.0);
        anyhow::ensure!(self.cache.frames.len() == kept && self.g.armor_tick == clock,
            "editing shot settings discarded the cache or advanced physics");
        let first = self.cache.frames[0].content_hash();
        self.cache_simulation();
        anyhow::ensure!(self.cache.frames.len() == kept && self.cache.baking,
            "firing again discarded existing frames");
        while self.cache.baking {
            self.tick(1.0/30.0);
            if self.g.casualty_tissue_pending() { std::thread::sleep(std::time::Duration::from_millis(2)); }
            anyhow::ensure!(started.elapsed().as_secs()<120,"append cache probe timed out");
        }
        anyhow::ensure!(self.cache.frames.len() > kept && self.cache.frames[0].content_hash() == first,
            "second shot did not append while preserving the first shot");
        self.button("RESET");
        anyhow::ensure!(self.cache.frames.is_empty() && !self.playing && self.run.is_none(),
            "RESET did not clear and pause the shot");
        println!("[cache-probe] shot edits retain frames; second shot appends; RESET alone clears and pauses");
        Ok(())
    }
}
