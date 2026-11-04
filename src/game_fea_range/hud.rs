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
        use crate::first_aid::Region;
        let aspect=self.win[0]/self.win[1].max(1.0);let px=height/30.0;
        for (i,role) in [CrewRole::Driver,CrewRole::Radioman,CrewRole::Gunner,CrewRole::Commander,CrewRole::Loader].into_iter().enumerate(){
            let man=self.g.crew_solo.man(role);let dead=man.dead;
            let damage=[Region::Head,Region::Torso,Region::Arm,Region::Leg].map(|region|man.wounds.iter().any(|w|w.region==region));
            let x=at[0]+i as f32*7.0*px/aspect;
            for (u,v,w,h,region) in [(2.,0.,2.,2.,0),(1.,3.,4.,4.,1),(0.,3.,1.,5.,2),(5.,3.,1.,5.,2),(1.,7.,1.5,5.,3),(3.5,7.,1.5,5.,3)] {
                self.r.fill_rect(x+u*px/aspect,at[1]+v*px,w*px/aspect,h*px,if damage[region]||dead{RED}else{INK});
            }
            if dead {for k in 0..12 {let t=k as f32;for sign in [false,true]{self.r.fill_rect(x+(if sign{5.0-t*0.4}else{t*0.4})*px/aspect,at[1]+t*px,px/aspect,px,BLACK);}}}
        }
        let x=at[0]+10.0*px/aspect;let y=at[1]+14.0*px;
        self.r.fill_rect(x+2.0*px/aspect,y,10.0*px/aspect,15.0*px,[0.22,0.22,0.22,1.0]);
        use super::super::ModuleKind::*;
        for (kind,u,v,w,h) in [(LeftTrack,0.,1.,2.,13.),(RightTrack,12.,1.,2.,13.),(Engine,3.,11.,8.,3.),(Transmission,3.,1.,8.,2.),(TurretTraverse,4.,5.,6.,5.),(MainGun,6.,-2.,2.,8.),(AmmoRack,2.,5.,2.,6.),(Fuel,10.,10.,2.,3.),(Radio,9.,3.,2.,2.),(GunnerSight,5.,4.,1.,2.),(CoaxMg,8.,4.,1.,2.),(HullMg,9.,1.,1.,2.)] {
            let damaged=self.g.modules.iter().any(|m|m.kind==kind&&(!m.functional || m.fault!=super::super::tiger_damage::Fault::None));
            self.r.fill_rect(x+u*px/aspect,y+v*px,w*px/aspect,h*px,if damaged{RED}else{INK});
        }
    }
}
