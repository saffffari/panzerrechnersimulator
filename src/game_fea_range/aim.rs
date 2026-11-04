use super::*;
impl ShotSetup {
    pub(super) fn json(&self)->serde_json::Value {
        serde_json::json!({"shell":self.shell,"range_m":self.range_m,"from_az":self.from_az,"from_el":self.from_el,"seed":self.seed,
            "target":match self.target {Target::Point(p)=>serde_json::json!({"point":p.to_array()}),Target::Man(r)=>serde_json::json!({"man":r})}})
    }
    pub(super) fn read(v:&serde_json::Value,mut fallback:Self)->Self {
        if let Some(n)=v["shell"].as_u64(){fallback.shell=(n as usize).min(shells().len()-1);}
        if let Some(n)=v["range_m"].as_f64(){fallback.range_m=n as f32;}
        if let Some(n)=v["from_az"].as_f64(){fallback.from_az=n as f32;}
        if let Some(n)=v["from_el"].as_f64(){fallback.from_el=n as f32;}
        if let Some(n)=v["seed"].as_u64(){fallback.seed=n as u32;}
        if let Some(p)=v["target"]["point"].as_array().filter(|p|p.len()==3){fallback.target=Target::Point(Vec3::new(p[0].as_f64().unwrap_or(0.0) as f32,p[1].as_f64().unwrap_or(0.0) as f32,p[2].as_f64().unwrap_or(0.0) as f32));}
        if let Ok(r)=serde_json::from_value::<CrewRole>(v["target"]["man"].clone()){fallback.target=Target::Man(r);}
        fallback
    }
}
impl Range {
    pub(super) fn sync_placed_shot(&mut self) {
        if let Some(i)=self.selected_shot {if let Some(s)=self.placed_shots.get_mut(i){*s=self.shot;}}
    }
    pub(super) fn add_placed_shot(&mut self) {
        self.placed_shots.push(self.shot); self.selected_shot=Some(self.placed_shots.len()-1);
        self.status=format!("SHOT {} PLACED; HOLD AND DRAG TO AIM; F FIRES",self.placed_shots.len());
    }
    fn aim_radius(&self)->f32 {
        let (aim,_)=self.aim_line();
        (self.eye.distance(aim)*0.055).clamp(0.02,0.65)
    }
    pub(super) fn aim_handle_hit(&self,c:[f32;2])->bool {
        let (aim,dir)=self.aim_line();
        self.project(aim-dir*self.aim_radius()).is_some_and(|p| {
            let aspect=self.win[0]/self.win[1].max(1.0);
            ((p[0]-c[0])*aspect).hypot(p[1]-c[1])<0.022
        })
    }
    pub(super) fn drag_aim_sphere(&mut self,dx:f32,dy:f32) {
        let _ = (dx,dy);
        let (aim,dir)=self.aim_line();
        let radius=self.aim_radius();
        let (origin,ray)=self.ray(self.cursor);
        let oc=origin-aim; let b=oc.dot(ray);
        let discriminant=b*b-(oc.length_squared()-radius*radius);
        let previous=aim-dir*radius;
        let point=if discriminant>=0.0 {
            let root=discriminant.sqrt();
            let near=origin+ray*(-b-root); let far=origin+ray*(-b+root);
            if near.distance_squared(previous)<=far.distance_squared(previous){near}else{far}
        } else {
            let distance=oc.length().max(radius+0.0001); let axis=oc/distance;
            let closest=origin+ray*(-b).max(0.0)-aim;
            let radial=(closest-axis*closest.dot(axis)).normalize_or_zero();
            aim+axis*(radius*radius/distance)+radial*radius*(1.0-radius*radius/(distance*distance)).sqrt()
        };
        self.shot.set_dir_hull(self.g.player.hull_model().inverse().transform_vector3((aim-point).normalize_or_zero()));
        self.sync_placed_shot();
        self.status="AIM: HANDLE ON SPHERE; F FIRES ALONG HANDLE -> IMPACT".into();
    }
    pub(super) fn draw_aim_sphere(&mut self) {
        let (aim,dir)=self.aim_line();let radius=self.aim_radius();
        for (a,b) in [(Vec3::X,Vec3::Y),(Vec3::X,Vec3::Z),(Vec3::Y,Vec3::Z)] {
            for k in 0..96 {
                if k%2==1 {continue;}
                let t=k as f32*std::f32::consts::TAU/96.0;
                let q=(k+1) as f32*std::f32::consts::TAU/96.0;
                self.line(aim+(a*t.cos()+b*t.sin())*radius,aim+(a*q.cos()+b*q.sin())*radius,[0.65,0.65,0.65,0.45]);
                if k%12==0 {let radial=a*t.cos()+b*t.sin();self.line(aim+radial*radius*0.97,aim+radial*radius*1.03,INK);}
            }
        }
        let handle=aim-dir*radius;
        self.line(handle,aim,AMBER);
        if let Some(p)=self.project(aim){self.dot(p,0.005,INK);}
        if let Some(p)=self.project(handle){
            self.filled_circle(p,0.005,AMBER);
        }
    }
}
