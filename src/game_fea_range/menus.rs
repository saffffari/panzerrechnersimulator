use super::*;

pub(super) struct Popup {
    pub knob: Knob,
    pub items: Vec<String>,
    pub selected: usize,
    pub scroll: usize,
    rect: [f32;4],
}
const MENU_ROW:f32=0.029;
pub(super) const MENU_ROWS:usize=18;
pub(super) fn is_menu(k:Knob)->bool {
    matches!(k,Knob::PlacedShot|Knob::Shell|Knob::SavedShot|Knob::SavedCam|Knob::AddShot|Knob::TargetKind|Knob::SectionPreset|Knob::Look|Knob::DitherPattern)
}
impl Range {
    pub(super) fn open_menu(&mut self,k:Knob,cursor:[f32;2]) {
        self.sync_placed_shot();
        let (items,selected):(Vec<String>,usize)=match k {
            Knob::PlacedShot=>(self.placed_shots.iter().enumerate().map(|(i,s)|format!("Shot {} - {}",i+1,s.spec().name)).collect(),self.selected_shot.unwrap_or(0)),
            Knob::Shell=>(shells().iter().map(|s|s.name.into()).collect(),self.shot.shell),
            Knob::SavedShot=>(self.shot_presets.iter().map(|p|p["name"].as_str().unwrap_or("Unnamed").into()).collect(),self.shot_preset_at.unwrap_or(0)),
            Knob::SavedCam=>(self.cameras.iter().map(|c|c.name.clone()).collect(),self.camera_at.unwrap_or(0).max(0) as usize),
            Knob::AddShot=>{let mut names=vec!["Save as new preset...".into()];names.extend(self.shot_presets.iter().map(|p|format!("Overwrite {}",p["name"].as_str().unwrap_or("Unnamed"))));(names,0)},
            Knob::TargetKind=>(["Picked point","Commander","Gunner","Loader","Driver","Radioman"].iter().map(|s|s.to_string()).collect(),0),
            Knob::SectionPreset=>(["Top","Left","Right","Front","Rear"].iter().map(|s|s.to_string()).collect(),0),
            Knob::Look=>(LOOKS.iter().map(|(_,s)|s.to_string()).collect(),LOOKS.iter().position(|(v,_)|(*v-self.r.look).abs()<0.1).unwrap_or(0)),
            Knob::DitherPattern=>(DITHERS.iter().map(|s|s.to_string()).collect(),self.r.display_dither_pattern.round().max(0.0) as usize),
            _=>return,
        };
        let height=(items.len().clamp(1,MENU_ROWS) as f32+1.7)*MENU_ROW;
        let rect=[(LEFT_X+COLS_W+0.008).min(0.44), (cursor[1]-height+MENU_ROW).clamp(0.035,0.96-height),0.54,height];
        let selected=selected.min(items.len().saturating_sub(1));
        let scroll=selected.saturating_sub(MENU_ROWS/2).min(items.len().saturating_sub(MENU_ROWS));
        self.popup=Some(Popup {knob:k,items,selected,scroll,rect});
    }
    pub(super) fn click_menu(&mut self,cursor:[f32;2])->bool {
        let Some(p)=self.popup.as_ref() else{return false};
        let [x,y,w,h]=p.rect;
        let row=((cursor[1]-y-MENU_ROW)/MENU_ROW).floor() as i32;
        if cursor[0]>=x && cursor[0]<=x+w && cursor[1]>=y+MENU_ROW && cursor[1]<y+h-MENU_ROW*0.4 && row>=0 {
            let index=p.scroll+row as usize;
            if index<p.items.len() {self.choose_menu(index);return true;}
        }
        self.popup=None;
        true
    }
    pub(super) fn menu_key(&mut self,code:KeyCode)->bool {
        let Some(p)=self.popup.as_mut() else{return false};
        match code {
            KeyCode::Escape=>self.popup=None,
            KeyCode::ArrowDown=>{p.selected=(p.selected+1).min(p.items.len().saturating_sub(1));if p.selected>=p.scroll+MENU_ROWS {p.scroll=p.selected+1-MENU_ROWS;}},
            KeyCode::ArrowUp=>{p.selected=p.selected.saturating_sub(1);p.scroll=p.scroll.min(p.selected);},
            KeyCode::Enter|KeyCode::NumpadEnter=>{let i=p.selected;self.choose_menu(i);},
            _=>{}
        }
        true
    }
    fn choose_menu(&mut self,index:usize) {
        let Some(menu)=self.popup.take() else{return};
        if index>=menu.items.len(){return;}
        match menu.knob {
            Knob::PlacedShot=>{self.shot=self.placed_shots[index];self.selected_shot=Some(index);},
            Knob::Shell=>{self.shot.shell=index;self.sync_placed_shot();},
            Knob::SavedCam=>self.view_camera(index),
            Knob::SavedShot=>{
                let p=self.shot_presets[index].clone();
                self.apply_state(&p["settings"]);self.apply_sky();self.shot_preset_at=Some(index);
                if !p["settings"]["shots"].is_array(){self.placed_shots=vec![self.shot];self.selected_shot=Some(0);}
                self.status=format!("PRESET {} LOADED",p["name"].as_str().unwrap_or(""));
            },
            Knob::AddShot=>{
                if index==0 {
                    let mut n=1;while self.shot_presets.iter().any(|p|p["name"].as_str()==Some(format!("shot{n}").as_str())){n+=1;}
                    self.preset_name=Some(format!("shot{n}"));self.preset_name_selected=true;
                } else {let i=index-1;let name=self.shot_presets[i]["name"].as_str().unwrap_or("preset").to_string();self.save_named_preset(name,Some(i));}
            },
            Knob::Look=>self.r.look=LOOKS[index].0,
            Knob::DitherPattern=>self.r.display_dither_pattern=index as f32,
            Knob::SectionPreset=>{self.section.set_normal_hull([Vec3::Y,Vec3::X,Vec3::NEG_X,Vec3::Z,Vec3::NEG_Z][index]);self.section.offset=if index==0{0.5}else{0.0};self.section.on=true;},
            Knob::TargetKind=>{
                let roles=[CrewRole::Commander,CrewRole::Gunner,CrewRole::Loader,CrewRole::Driver,CrewRole::Radioman];
                self.shot.target=if index==0 {match self.shot.target {Target::Point(p)=>Target::Point(p),Target::Man(r)=>Target::Point(self.g.player.hull_model().inverse().transform_point3(self.g.crew_solo_man_point(r)))}}else{Target::Man(roles[index-1])};
            },
            _=>{}
        }
        self.save_state();
    }
    pub(super) fn save_named_preset(&mut self,name:String,overwrite:Option<usize>) {
        self.sync_placed_shot();
        let name=name.trim();
        if overwrite.is_none() && self.shot_presets.iter().any(|p|p["name"].as_str()==Some(name)) {self.status="NAME EXISTS: CHOOSE OVERWRITE IN SAVE PRESET".into();return;}
        if name.is_empty(){self.status="PRESET NEEDS A NAME".into();return;}
        let preset=serde_json::json!({"name":name,"settings":self.state_json()});
        let mut all=self.shot_presets.clone();
        let index=if let Some(i)=overwrite.filter(|&i|i<all.len()){all[i]=preset.clone();i}else{all.push(preset.clone());all.len()-1};
        let result=(||->anyhow::Result<()>{
            std::fs::create_dir_all("target")?;
            let tmp=format!("{PRESETS_PATH}.tmp");
            std::fs::write(&tmp,serde_json::to_vec_pretty(&all)?)?;std::fs::rename(tmp,PRESETS_PATH)?;
            std::fs::write("target/playground_preset.json",serde_json::to_vec_pretty(&preset)?)?;
            Ok(())
        })();
        match result {Ok(())=>{self.shot_presets=all;self.shot_preset_at=Some(index);self.preset_name=None;self.status=format!("PRESET {name} SAVED; WEB EXPORT: TARGET/PLAYGROUND_PRESET.JSON");},Err(e)=>self.status=format!("PRESET SAVE FAILED: {e:#}")}
    }
    pub(super) fn draw_popup(&mut self,cursor:[f32;2]) {
        if let Some(name)=self.preset_name.clone() {
            self.r.fill_rect(0.24,0.33,0.52,0.18,[0.02,0.02,0.02,0.98]);
            self.text_bold("SAVE PRESET",0.26,0.36,TEXT,AMBER,crate::ui::Align::Left);
            self.r.fill_rect(0.26,0.39,0.48,0.045,[0.10,0.10,0.10,1.0]);
            self.text(&format!("{name}_"),0.27,0.413,TEXT,INK,crate::ui::Align::Left);
            self.text("TYPE A NAME; ENTER SAVES; ESC CANCELS",0.26,0.47,TEXT*0.8,DIM,crate::ui::Align::Left);
            return;
        }
        let Some(p)=self.popup.as_ref() else{return};
        let [x,y,w,h]=p.rect;let items=p.items.clone();let scroll=p.scroll;let selected=p.selected;let title=self.knob_label(p.knob).0;
        self.r.fill_rect(x-0.002,y-0.002,w+0.004,h+0.004,[0.6,0.4,0.15,1.0]);
        self.r.fill_rect(x,y,w,h,[0.02,0.02,0.02,0.98]);
        self.text_bold(&title,x+0.012,y+MENU_ROW*0.5,TEXT*0.85,AMBER,crate::ui::Align::Left);
        if items.is_empty(){self.text("NONE SAVED YET",x+0.012,y+MENU_ROW*1.5,TEXT*0.8,DIM,crate::ui::Align::Left);}
        for (i,label) in items.iter().enumerate().skip(scroll).take(MENU_ROWS) {
            let row=y+MENU_ROW*(1+(i-scroll)) as f32;
            let hover=cursor[0]>=x && cursor[0]<=x+w && cursor[1]>=row && cursor[1]<row+MENU_ROW;
            if hover || i==selected{self.r.fill_rect(x+0.003,row,w-0.006,MENU_ROW,if hover{[0.24,0.18,0.08,1.0]}else{[0.10,0.10,0.10,1.0]});}
            self.text(label,x+0.012,row+MENU_ROW*0.5,TEXT*0.85,if i==selected{AMBER}else{INK},crate::ui::Align::Left);
        }
        if items.len()>MENU_ROWS{self.text("SCROLL FOR MORE; ESC CLOSES",x+0.012,y+h-0.009,TEXT*0.65,DIM,crate::ui::Align::Left);}
    }
}
