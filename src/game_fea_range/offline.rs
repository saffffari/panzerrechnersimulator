//! Offline camera-directed films: exact saved views, reference physics and no dropped frames.
use super::*;
use serde::Deserialize;
use serde_json::json;
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
#[derive(Deserialize)]
struct FilmPlan {
    settings: serde_json::Value,
    cameras: Vec<SavedCam>,
    jobs: Vec<Job>,
    video_dir: PathBuf,
    still_dir: PathBuf,
    report: PathBuf,
    #[serde(default)]
    previews_only: bool,
    #[serde(default = "fps")]
    fps: u32,
    #[serde(default = "scale")]
    scale: u32,
    #[serde(default = "samples")]
    samples: u32,
}
fn fps() -> u32 {
    60
}
fn scale() -> u32 {
    2
}
fn samples() -> u32 {
    2
}
#[derive(Deserialize)]
struct Job {
    name: String,
    shell: usize,
    camera: usize,
    target: [f32; 3],
    range_m: f32,
    #[serde(default)] section: bool,
    #[serde(default)] deformation: bool,
    #[serde(default)] phases: Vec<(f32,u32)>,
    #[serde(default)] cinematic: bool,
    #[serde(default)] real_time:bool,
}
impl Range {
    pub(super) fn offline_films(&mut self, path: &str) -> anyhow::Result<()> {
        let plan: FilmPlan = serde_json::from_slice(&std::fs::read(path)?)?;
        anyhow::ensure!(
            plan.fps > 0
                && plan.fps <= 120
                && plan.scale > 0
                && plan.scale <= 4
                && plan.samples > 0
                && plan.samples <= 32,
            "invalid film settings"
        );
        anyhow::ensure!(
            super::super::crew_tissue::reference_solve(),
            "offline films require KESSEL_TISSUE_SOLVE=reference"
        );
        std::fs::create_dir_all(&plan.video_dir)?;
        std::fs::create_dir_all(&plan.still_dir)?;
        self.cameras = plan.cameras;
        self.muted = true;
        self.g.mute_audio();
        self.battle = false;
        self.pilot = None;
        self.player_control = false;
        let mut report = json!({"schema":"panzer-offline-interiors/v1","complete":false,"fps":plan.fps,
            "internal_pixels":[self.r.iw,self.r.ih],"output_scale":plan.scale,"samples_per_frame":plan.samples,
            "simulation":{"mode":"reference","solver_commit_delay_seconds":0,"anatomy_cell_mm":10,"plate_particle_budget":6000,"projectile_cells_across":8,
                "substeps_per_frame":2,"timing":"See each clip's segments for exact time scales."},
            "audio":"ordinary gameplay mix on presentation clock; no artificial pitch reduction","source_settings":plan.settings,"clips":[]});
        for job in &plan.jobs {
            anyhow::ensure!(
                job.camera < self.cameras.len() && job.shell < shells().len(),
                "invalid camera or shell"
            );
            anyhow::ensure!(
                job.name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "unsafe clip name"
            );
            self.apply_state(&plan.settings);
            self.section.on = false;
            self.sky.running = false;
            self.reset();
            self.g.weapon_vfx.inspect_plate=job.deformation;
            self.view_camera(job.camera);
            self.section.on = job.section;
            if job.section {
                let side=self.cameras[job.camera].hull_eye.map_or(1.0,|p|p[0].signum());
                self.section.set_normal_hull(Vec3::X*side);
                self.section.offset=self.section.normal_hull().dot(Vec3::from_array(job.target)-SECTION_PIVOT);
            }
            super::super::game_fea::RECORD_PLATE_FILM.store(job.deformation,std::sync::atomic::Ordering::Relaxed);
            *super::super::game_fea::PLATE_FILM.lock().unwrap()=None;
            self.shot.shell = job.shell;
            self.shot.range_m = job.range_m;
            self.shot.from_az = 0.0;
            self.shot.from_el = 0.0;
            self.shot.target = Target::Point(Vec3::from_array(job.target));
            self.shot.seed = 0x5ec7_a001;
            self.placed_shots.clear();
            self.selected_shot = None;
            self.g.debug_set_strike_counters((47, 0));
            self.pose_view();
            self.r.ui_reset();
            self.r.reset_history();
            let convergence =
                crate::converge_headless_screenshot(&mut self.r, &self.device, &self.queue, 32)?;
            // Preserve the editor's exposure policy: an automatic eye stays automatic.
            // Freezing the first converged value washed the cool environment toward grey.
            let _=convergence;
            self.pose_view();
            self.r.ui_reset();
            let first = self
                .r
                .render_to_rgba(&self.device, &self.queue, self.r.iw, self.r.ih)?;
            let preview = plan.still_dir.join(format!("{}_before.png", job.name));
            image::save_buffer(
                &preview,
                &first,
                self.r.iw,
                self.r.ih,
                image::ColorType::Rgba8,
            )?;
            let cam = self.cameras[job.camera].clone();
            let mut entry = json!({"name":job.name,"camera_index":job.camera+1,"camera":cam,"shell":self.shot.spec().name,
                "range_m":job.range_m,"impact_velocity_mps":self.shot.impact_velocity(),"target_hull":job.target,
                "from_az":0,"from_el":0,"exposure":self.sky.exposure,"preview":preview,"complete":false});
            let contact = self
                .entry()
                .ok_or_else(|| anyhow::anyhow!("{} ray misses Tiger", job.name))?;
            entry["entry_hull"] = json!(self
                .g
                .player
                .hull_model()
                .inverse()
                .transform_point3(contact.0)
                .to_array());
            entry["entry_normal_world"] = json!(contact.1.to_array());
            let (aim,dir)=self.aim_line();
            entry["entry_source_frame"]=json!(self.g.cabin_fire.sweep(aim-dir*40.,aim+dir*0.02,&self.g.fire_surface_models()).map(|c|c.frame));
            println!(
                "[offline-film] BEGIN {} | {} | camera {} {} | entry {:?}",
                job.name,
                self.shot.spec().name,
                job.camera + 1,
                cam.name,
                entry["entry_hull"]
            );
            if plan.previews_only {
                entry["complete"] = json!(true);
                report["clips"].as_array_mut().unwrap().push(entry);
                std::fs::write(&plan.report, serde_json::to_vec_pretty(&report)?)?;
                continue;
            }
            let output = plan.video_dir.join(format!("{}.mp4", job.name));
            let partial = plan.video_dir.join(format!("{}.partial.mp4", job.name));
            anyhow::ensure!(
                !output.exists(),
                "completed video already exists: {}",
                output.display()
            );
            let size = format!("{}x{}", self.r.iw, self.r.ih);
            let rate = plan.fps.to_string();
            let filter = format!("scale=iw*{}:ih*{}:flags=neighbor", plan.scale, plan.scale);
            let mut encoder = Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-nostdin",
                    "-y",
                    "-f",
                    "rawvideo",
                    "-pixel_format",
                    "rgba",
                    "-video_size",
                    &size,
                    "-framerate",
                    &rate,
                    "-i",
                    "pipe:0",
                    "-an",
                    "-vf",
                    &filter,
                    "-c:v",
                    "libx264",
                    "-threads",
                    "4",
                    "-preset",
                    "fast",
                    "-crf",
                    "12",
                    "-pix_fmt",
                    "yuv444p",
                    "-movflags",
                    "+faststart",
                ])
                .arg(&partial)
                .stdin(Stdio::piped())
                .spawn()?;
            let mut plate=if job.deformation {Some(PlateView::new(&self.device,&self.queue,&mut self.r,self.shot.spec()))}else{None};
            let started = std::time::Instant::now();
            let mut frames = 0u32;
            self.g.begin_audio_capture();
            let mut sound:Vec<f32>=Vec::new();
            let audio_frames=(crate::acoustics::SAMPLE_RATE/plan.fps)as usize;
            let mut impact_frame:Option<u32>=None;
            let mut shell_min_z=f32::INFINITY;
            let mut rear_crossings=std::collections::BTreeMap::<u64,f32>::new();
            // Real simulation during the opening, including idle crew/lamps; no frozen hold.
            for _ in 0..if job.cinematic {plan.fps}else{plan.fps/2} {
                if job.cinematic {
                    self.step(1.0/plan.fps as f32);self.pose_view();self.r.ui_reset();
                    let rgba=self.r.render_to_rgba(&self.device,&self.queue,self.r.iw,self.r.ih)?;
                    encoder.stdin.as_mut().unwrap().write_all(&rgba)?;
                }else{encoder.stdin.as_mut().unwrap().write_all(&first)?;}
                sound.extend(self.g.render_audio_capture_frames(audio_frames)?);
                frames += 1;
            }
            let schedule=if job.real_time{vec![(1,vec![1.0/plan.fps as f32;(plan.fps as f32*5.65)as usize])]}else if job.cinematic{cinematic_schedule(plan.fps)}else{
                let phases=if job.phases.is_empty(){vec![(0.14f32,128u32),(0.45f32,16u32)]}else{job.phases.clone()};
                phases.into_iter().map(|(seconds,denominator)|{
                    assert!(seconds.is_finite()&&seconds>0.&&seconds<=2.&&denominator>0&&denominator<=16384);
                    (denominator,vec![1.0/(denominator*plan.fps)as f32;(seconds*denominator as f32*plan.fps as f32).round()as usize])
                }).collect()
            };
            if job.cinematic {
                let approach=if job.real_time{0.05}else{schedule[0].1.iter().chain(&schedule[1].1).sum::<f32>()+0.00015};
                let shell=self.shot.spec();
                let lead=lead_for_time(shell,self.shot.range_m,approach);
                let (aim,dir)=self.aim_line();let target=self.entry().map_or(aim,|(p,_)|p);
                self.g.debug_launch_round_with_lead(shell,false,target,dir,self.shot.range_m,self.shot.seed,lead);
                self.shot_t=0.0;
            }else{self.fire();}
            self.run = None;
            self.playing = false;
            let mut segments = Vec::new();
            for (phase,(denominator,steps)) in schedule.into_iter().enumerate() {
                let count=steps.len() as u32;
                segments.push(json!({"start_frame":frames,"frames":count,"speed_denominator":denominator,
                    "frame_simulation_seconds":steps,"speed_curve":if denominator==0 {"geometric smooth ramp"}else{"constant"}}));
                for (f,frame_dt) in steps.into_iter().enumerate() {
                    let f=f as u32;
                    let dt=frame_dt*0.5;
                    for _ in 0..2 {
                        self.step(dt);
                        self.shot_t += dt;
                        self.g.settle_casualty_tissue(true);
                    }
                    for body in self.g.hot_fragments.iter().filter(|b|b.active&&b.penetration_id==47&&b.tracer_share>0.0) {
                        let to_hull=self.g.player.hull_model().inverse()*super::super::blood_parent_model(&self.g.player,body.frame);
                        let p=to_hull.transform_point3(body.pos);shell_min_z=shell_min_z.min(p.z);
                        if p.z < -3.5 {rear_crossings.entry(body.id).or_insert(body.mass_kg);}
                    }
                    self.pose_view();
                    if impact_frame.is_none() && self.g.fea.temporal_view.is_some(){impact_frame=Some(frames);}
                    if let Some(at)=impact_frame {
                        // Presentation-only impulse, on output time: settled before inspection.
                        let age=(frames-at)as f32/plan.fps as f32;
                        if age<0.24 {
                            let amp=(-age*25.).exp()*(age*62.).sin();
                            self.r.camera.pos+=Vec3::Y*(amp*0.004)+self.r.camera.forward().cross(Vec3::Y)*(amp*0.006);
                        }
                    }
                    if let Some(plate)=plate.as_mut(){plate.draw(self);}
                    self.r.ui_reset();
                    for _ in 1..plan.samples {
                        self.r.render_offscreen(&self.device, &self.queue);
                    }
                    let rgba =
                        self.r
                            .render_to_rgba(&self.device, &self.queue, self.r.iw, self.r.ih)?;
                    encoder.stdin.as_mut().unwrap().write_all(&rgba)?;
                    sound.extend(self.g.render_audio_capture_frames(audio_frames)?);
                    frames += 1;
                    if f == count / 2 || f + 1 == count {
                        let still = plan
                            .still_dir
                            .join(format!("{}_phase{}_{}x_{:04}.png", job.name, phase+1, denominator, f));
                        image::save_buffer(
                            still,
                            &rgba,
                            self.r.iw,
                            self.r.ih,
                            image::ColorType::Rgba8,
                        )?;
                    }
                    if f % 120 == 0 {
                        println!(
                            "[offline-film] {} frame {} | sim {:.6}s | wall {:.1}s",
                            job.name,
                            frames,
                            self.shot_t,
                            started.elapsed().as_secs_f32()
                        );
                    }
                }
            }
            drop(encoder.stdin.take());
            anyhow::ensure!(encoder.wait()?.success(), "ffmpeg failed for {}", job.name);
            crate::acoustics::attenuate_over_peak(&mut sound,0.94);
            let wav=plan.video_dir.join(format!("{}.wav",job.name));
            crate::acoustics::write_wav_stereo(wav.to_str().unwrap(),&sound,crate::acoustics::SAMPLE_RATE)?;
            let status=Command::new("ffmpeg").args(["-v","error","-nostdin","-y","-i"]).arg(&partial)
                .arg("-i").arg(&wav).args(["-map","0:v:0","-map","1:a:0","-c:v","copy","-c:a","aac","-b:a","192k","-shortest","-movflags","+faststart"])
                .arg(&output).status()?;
            anyhow::ensure!(status.success(),"audio mux failed");
            std::fs::remove_file(&partial)?;std::fs::remove_file(&wav)?;
            entry["rear_observation"]=json!({"minimum_shell_fragment_hull_z_m":if shell_min_z.is_finite(){Some(shell_min_z)}else{None},
                "rear_plane_hull_z_m":-3.5,"crossing_fragment_ids":rear_crossings.keys().collect::<Vec<_>>(),
                "crossing_mass_kg":rear_crossings.values().sum::<f32>(),
                "meaning":"Observed current source-projectile fragment centers crossing a rear witness plane; no predicted penetration chain."});
            entry["real_time"]=json!(job.real_time);
            entry["impact_frame"]=json!(impact_frame);
            entry["audio"]=json!({"mode":"gameplay mix, presentation clock","sample_frames":sound.len()/2,"peak":sound.iter().copied().map(f32::abs).fold(0.0,f32::max)});
            entry["video"] = json!(output);
            entry["frames"] = json!(frames);
            entry["video_seconds"] = json!(frames as f32 / plan.fps as f32);
            entry["simulation_seconds"] = json!(self.shot_t);
            entry["segments"] = json!(segments);
            entry["wall_seconds"] = json!(started.elapsed().as_secs_f32());
            entry["damage_chains"] = json!(self
                .g
                .fea
                .receipts
                .iter()
                .map(|c| format!("{} {:?}: {}", c.shell, c.terminal, c.result.summary()))
                .collect::<Vec<_>>());
            entry["blood"] = self.g.debug_surface_blood();
            entry["sparks"]=json!({"contacts":self.g.weapon_vfx.cabin_sparks.contacts,"crew_contacts":self.g.weapon_vfx.cabin_sparks.crew_contacts});
            entry["section"]=json!(job.section);
            if let Some(plate)=plate.as_ref(){entry["plate_deformation"]=plate.receipt();plate.hide(&mut self.r);}
            entry["complete"] = json!(true);
            report["clips"].as_array_mut().unwrap().push(entry);
            std::fs::write(&plan.report, serde_json::to_vec_pretty(&report)?)?;
            println!(
                "[offline-film] DONE {} | {} frames | {:.1}s wall",
                output.display(),
                frames,
                started.elapsed().as_secs_f32()
            );
        }
        report["complete"] = json!(true);
        std::fs::write(&plan.report, serde_json::to_vec_pretty(&report)?)?;
        Ok(())
    }
}

/// The native scene's local plate patch is replaced by the lattice's recorded cells only in
/// explicitly requested section films. These are solver states, not a morph of the final scar.
fn cinematic_schedule(fps:u32)->Vec<(u32,Vec<f32>)> {
    let ramp=|from:f32,to:f32,frames:usize| (0..frames).map(|i|{
        let u=i as f32/(frames-1).max(1) as f32;
        (from.ln()+(to.ln()-from.ln())*u).exp()/fps as f32
    }).collect();
    let hold=|seconds:f32,den:u32|vec![1.0/(fps*den)as f32;(seconds*fps as f32*den as f32).round()as usize];
    vec![(1,vec![1.0/fps as f32]),(0,ramp(1.0,1.0/4096.0,(fps*3/5)as usize)),
        (4096,hold(0.0018,4096)),(0,ramp(1.0/4096.0,1.0/128.0,(fps/2)as usize)),
        (128,hold(0.065,128)),(0,ramp(1.0/128.0,1.0,fps as usize)),(1,hold(5.25,1))]
}
fn lead_for_time(shell:&crate::ballistics::ShellSpec,range:f32,seconds:f32)->f32 {
    let total=shell.time_of_flight(range);let mut lo=0.;let mut hi=range;
    for _ in 0..32 {let mid=(lo+hi)*0.5;if total-shell.time_of_flight(range-mid)<seconds{lo=mid;}else{hi=mid;}}
    (lo+hi)*0.5
}

struct PlateView {
    pages: [usize;6], first_positions:Vec<Vec3>, states:u64,
    maximum_displacement_m:f32, last:serde_json::Value,
}
impl PlateView {
    fn new(device:&wgpu::Device,queue:&wgpu::Queue,r:&mut Renderer,_shell:&crate::ballistics::ShellSpec)->Self{
        let colors=[[102,106,110],[132,76,48],[183,58,15],[240,100,18],[255,177,64],[255,225,150]];
        let pages=std::array::from_fn(|i|{
            let mesh=super::super::hot_fragment_mesh(colors[i]);
            let p=r.add_dynamic_instanced_pool(device,queue,&mesh,32768).expect("live material pool");
            r.set_batch_flags(p,2.0);
            r.set_dynamic_instanced_emissive(p,if i==0{0.0}else{[0.,0.08,0.3,0.7,1.4,2.8][i]});p
        });
        Self{pages,first_positions:Vec::new(),states:0,maximum_displacement_m:0.,last:json!({"recorded":false})}
    }
    fn draw(&mut self,range:&mut Range){
        let Some(f)=range.g.fea.temporal_view.as_ref() else{return;};
        if self.first_positions.is_empty(){self.first_positions=f.positions.clone();}
        self.states+=1;
        self.maximum_displacement_m=self.maximum_displacement_m.max(f.positions.iter().zip(&self.first_positions).map(|(a,b)|a.distance(*b)).fold(0.,f32::max));
        let hull=range.g.player.hull_model();
        let local=Mat4::from_cols(f.axes[0].extend(0.),f.axes[1].extend(0.),f.axes[2].extend(0.),f.origin.extend(1.));
        let transform=hull*local;let rotation=Quat::from_mat4(&transform);
        let plane=range.section_plane();
        let mut mats:[Vec<Mat4>;6]=std::array::from_fn(|_|Vec::new());
        for (positions,temperatures,retained,h) in [(&f.positions,&f.temperatures,&f.retained,f.h),
            (&f.shot_positions,&f.shot_temperatures,&f.shot_retained,f.shot_h)] {
            for ((&p,&temperature),&keep) in positions.iter().zip(temperatures).zip(retained){
                if !keep{continue;}
                let world=transform.transform_point3(p);
                if plane.is_some_and(|s|world.dot(s.truncate())>s.w){continue;}
                let stage=thermal_page(temperature);
                mats[stage].push(Mat4::from_scale_rotation_translation(Vec3::splat(h),rotation,world));
            }
        }
        for fragment in range.g.hot_fragments.iter().filter(|f|f.active){
            let Some((h,cells))=range.g.fea.fragment_shapes.get(&fragment.id) else{continue;};
            let parent=super::super::blood_parent_model(&range.g.player,fragment.frame);
            let (_,rot)=super::super::hot_fragment_render_pose(fragment);
            for &offset in cells {
                let world=parent.transform_point3(fragment.pos+rot*offset);
                if plane.is_some_and(|s|world.dot(s.truncate())>s.w){continue;}
                mats[thermal_page(fragment.temp_k)].push(Mat4::from_scale_rotation_translation(Vec3::splat(*h),rotation*rot,world));
            }
        }
        for (page,mats) in self.pages.into_iter().zip(mats.iter()){assert!(range.r.set_dynamic_instanced_mats(page,mats));}
        // Replace the source patch with the current cells, including before the first breach.
        // The dummy scar has zero damage and is a render-only boundary, never a ballistic hole.
        let scar=crate::armor_damage::ArmorScar{facing:crate::ballistics::Facing::HullFront,
            local_mm:f.origin.to_array().map(|x|(x*1000.)as i16),radius_mm:1,loss_mm:0,
            kind:crate::armor_damage::ScarKind::Dent,class:crate::armor_damage::ShotClass::Ap,
            normal:(-f.axes[2]).to_array().map(|x|(x*127.)as i8),dir:f.axes[2].to_array().map(|x|(x*127.)as i8),
            depth_dmm:0,plate_mm:(f.half[2]*1000.)as u16,aperture_mm:0,back_radius_mm:0,petals:0,petal_mm:0,plug:false};
        let mut draw=crate::renderer::ScarDraw::cold(scar);
        draw.review_patch=Some((f.origin,f.axes,f.half));
        for (slot,model) in [(range.g.slots.tiger[0],hull),(range.g.slots.tiger[1],range.g.player.turret_model()),
            (range.g.slots.mantlet,range.g.player.gun_model()),
            (range.g.slots.gun_cradle,range.g.player.gun_model())] {
            let into=model.inverse()*hull;
            draw.review_patch=Some((into.transform_point3(f.origin),f.axes.map(|a|into.transform_vector3(a)),f.half));
            range.r.set_mesh_scars(slot,&[draw]);
        }
        self.last=json!({"recorded":true,"source":"live sequential material states", "states":self.states,
            "cell_mm":f.h*1000.,"solve_seconds":f.t,"solver_steps":f.steps,
            "origin_hull":f.origin.to_array(),"axes_hull":f.axes.map(|a|a.to_array()),
            "maximum_cell_displacement_m":self.maximum_displacement_m,"birth_simulation_s":f.born,
            "shell_mass_kg":f.initial_shot_mass,"released_shell_mass_kg":f.released_shot_mass,
            "peak_plate_k":f.temperatures.iter().copied().fold(0.,f32::max),
            "peak_shell_k":f.shot_temperatures.iter().copied().fold(0.,f32::max),
            "note":"Sequential CFL material steps and current swept contacts; calibrated plate motion and approximate material laws, not experimental validation."});
    }
    fn hide(&self,r:&mut Renderer){for page in self.pages{r.set_dynamic_instanced_mats(page,&[]);}}
    fn receipt(&self)->serde_json::Value{self.last.clone()}
}
fn thermal_page(t:f32)->usize{if t<800.{0}else if t<1000.{1}else if t<1250.{2}else if t<1550.{3}else if t<2000.{4}else{5}}
