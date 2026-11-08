use super::*;
impl Range {
    pub(super) fn filled_circle(&mut self,p:[f32;2],diameter:f32,ink:[f32;4]) {
        let aspect=self.win[0]/self.win[1].max(1.0);
        for i in 0..12 {let y=(i as f32+0.5)/12.0*2.0-1.0;
            let half=(1.0-y*y).sqrt()*diameter*0.5/aspect;
            self.r.fill_rect(p[0]-half,p[1]+y*diameter*0.5,half*2.0,diameter/12.0,ink);
        }
    }
    pub(super) fn draw_action_icon(&mut self,label:&str,fallback:&str,rect:[f32;4],ink:[f32;4]) {
        let [x,y,w,h]=rect; let a=self.win[0]/self.win[1].max(1.0);
        let c=[x+w*0.5,y+h*0.5]; let d=h*0.52;
        match label {
            "REC" if self.recorder.is_none()=>self.filled_circle(c,d,RED),
            "REC"=>self.r.fill_rect(c[0]-d*0.5/a,c[1]-d*0.5,d/a,d,RED),
            "PLAY" if self.playing && !self.battle=>{for sign in [-1.0,1.0]{self.r.fill_rect(c[0]+sign*d*0.24/a-d*0.10/a,c[1]-d*0.5,d*0.20/a,d,ink);}},
            "PLAY"|"FIRE"=>{for i in 0..12 {let t=(i as f32+0.5)/12.0;let half=(1.0-(t*2.0-1.0).abs())*d*0.5;
                self.r.fill_rect(c[0]-d*0.30/a,c[1]+(t-0.5)*d,half*1.6/a,d/12.0,ink);}
                if label=="FIRE" {self.r.fill_rect(c[0]-d*0.48/a,c[1]-d*0.4,d*0.12/a,d*0.8,ink);}},
            "RESET"=>{for i in 0..22 {let t=i as f32/24.0*std::f32::consts::TAU;
                self.dot([c[0]+t.cos()*d*0.40/a,c[1]+t.sin()*d*0.40],d*0.14,ink);}
                self.r.fill_rect(c[0]+d*0.12/a,c[1]-d*0.44,d*0.42/a,d*0.17,ink);},
            _=>self.text_bold(fallback,c[0],c[1],TEXT*0.68,ink,crate::ui::Align::Center)
        }
    }
    pub(super) fn draw_damage_hud(&mut self,at:[f32;2],height:f32) {
        let Some(hud)=self.damage_hud.as_ref() else{return;};
        let aspect=self.win[0]/self.win[1].max(1.0);
        let tw=height*135.0/240.0/aspect;
        let crew=[CrewRole::Driver,CrewRole::Radioman,CrewRole::Gunner,CrewRole::Commander,CrewRole::Loader];
        let levels=crew.map(|role|{let m=self.g.crew_solo.man(role);if m.dead {2usize}else if !m.wounds.is_empty(){1}else{0}});
        let rect=[at[0],at[1],tw,height];
        self.r.draw_image(hud.tank,rect[0],rect[1],rect[2],rect[3],INK);
        use super::super::ModuleKind::*;
        for (name,handles) in &hud.layers {
            let role=match *name {"Driver"=>Some(0),"Radio"=>Some(1),"Gunner"=>Some(2),"Commander"=>Some(3),"Loader"=>Some(4),_=>None};
            let level=if let Some(i)=role{levels[i]}else{
                let kind=match *name{"LeftTrack"=>Some(LeftTrack),"RightTrack"=>Some(RightTrack),"Engine"=>Some(Engine),"Transmission"=>Some(Transmission),"TurretTraverse"=>Some(TurretTraverse),"MainGun"=>Some(MainGun),"AmmoRack"=>Some(AmmoRack),_=>None};
                kind.map_or(0,|kind|self.g.modules.iter().filter(|m|m.kind==kind).map(|m|if !m.functional {2}else if m.fault!=super::super::tiger_damage::Fault::None {1}else{0}).max().unwrap_or(0))
            };
            if level>0 {self.r.draw_image(handles[level-1],rect[0],rect[1],rect[2],rect[3],INK);}
        }
        // Authored figures beside the plan, lit and coloured; no symbolic stick bodies.
        let ch=height*0.48;let cw=ch*28.0/60.0/aspect;
        for i in 0..5 {
            self.r.draw_image(hud.crew[i][levels[i]],at[0]+tw+0.004+i as f32*cw,at[1]+height*0.25,cw,ch,INK);
        }
    }
}

// Generated from the authored Tiger and crew, orthographic cutaway with cabin lamps.
// Rebuild with web-playground/web/tools/export-native-hud.mjs, then mask changed pixels.
pub(super) struct DamageHud {
    pub tank: u32,
    pub layers: Vec<(&'static str, [u32;2])>,
    pub crew: [[u32;3];5],
}
impl DamageHud {
    pub fn load(r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut upload=|bytes:&[u8]| {
            let im=image::load_from_memory(bytes).expect("embedded HUD PNG").into_rgba8();
            r.upload_image_nearest(device,queue,&im,im.width(),im.height())
        };
        Self {
            tank:upload(include_bytes!("hud_assets/tank.png")),
            layers:vec![
                ("AmmoRack",[upload(include_bytes!("hud_assets/tank-AmmoRack-1.png")),upload(include_bytes!("hud_assets/tank-AmmoRack-2.png"))]),
                ("Commander",[upload(include_bytes!("hud_assets/tank-Commander-1.png")),upload(include_bytes!("hud_assets/tank-Commander-2.png"))]),
                ("Driver",[upload(include_bytes!("hud_assets/tank-Driver-1.png")),upload(include_bytes!("hud_assets/tank-Driver-2.png"))]),
                ("Engine",[upload(include_bytes!("hud_assets/tank-Engine-1.png")),upload(include_bytes!("hud_assets/tank-Engine-2.png"))]),
                ("Gunner",[upload(include_bytes!("hud_assets/tank-Gunner-1.png")),upload(include_bytes!("hud_assets/tank-Gunner-2.png"))]),
                ("Hull",[upload(include_bytes!("hud_assets/tank-Hull-1.png")),upload(include_bytes!("hud_assets/tank-Hull-2.png"))]),
                ("LeftTrack",[upload(include_bytes!("hud_assets/tank-LeftTrack-1.png")),upload(include_bytes!("hud_assets/tank-LeftTrack-2.png"))]),
                ("Loader",[upload(include_bytes!("hud_assets/tank-Loader-1.png")),upload(include_bytes!("hud_assets/tank-Loader-2.png"))]),
                ("MainGun",[upload(include_bytes!("hud_assets/tank-MainGun-1.png")),upload(include_bytes!("hud_assets/tank-MainGun-2.png"))]),
                ("Radio",[upload(include_bytes!("hud_assets/tank-Radio-1.png")),upload(include_bytes!("hud_assets/tank-Radio-2.png"))]),
                ("RightTrack",[upload(include_bytes!("hud_assets/tank-RightTrack-1.png")),upload(include_bytes!("hud_assets/tank-RightTrack-2.png"))]),
                ("Transmission",[upload(include_bytes!("hud_assets/tank-Transmission-1.png")),upload(include_bytes!("hud_assets/tank-Transmission-2.png"))]),
                ("TurretTraverse",[upload(include_bytes!("hud_assets/tank-TurretTraverse-1.png")),upload(include_bytes!("hud_assets/tank-TurretTraverse-2.png"))]),
            ],
            crew:[
                [upload(include_bytes!("hud_assets/crew-Driver-0.png")),upload(include_bytes!("hud_assets/crew-Driver-1.png")),upload(include_bytes!("hud_assets/crew-Driver-2.png"))],
                [upload(include_bytes!("hud_assets/crew-Radio-0.png")),upload(include_bytes!("hud_assets/crew-Radio-1.png")),upload(include_bytes!("hud_assets/crew-Radio-2.png"))],
                [upload(include_bytes!("hud_assets/crew-Gunner-0.png")),upload(include_bytes!("hud_assets/crew-Gunner-1.png")),upload(include_bytes!("hud_assets/crew-Gunner-2.png"))],
                [upload(include_bytes!("hud_assets/crew-Commander-0.png")),upload(include_bytes!("hud_assets/crew-Commander-1.png")),upload(include_bytes!("hud_assets/crew-Commander-2.png"))],
                [upload(include_bytes!("hud_assets/crew-Loader-0.png")),upload(include_bytes!("hud_assets/crew-Loader-1.png")),upload(include_bytes!("hud_assets/crew-Loader-2.png"))],
            ],
        }
    }
}
