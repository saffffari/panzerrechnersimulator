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
    pub(super) fn aim_handle_hit(&self,c:[f32;2])->bool {
        let (aim,_)=self.aim_line();
        self.project(aim).is_some_and(|p| ((p[0]-c[0])*self.win[0]).hypot((p[1]-c[1])*self.win[1]) < 14.0)
    }
    /// Full discs are hit targets; the nearest rim disambiguates their overlap.
    pub(super) fn aim_disc_hit(&self,c:[f32;2])->Option<usize> {
        if !self.aim_rings {return None;}
        let (aim,dir)=self.aim_line();
        let (_, (h,up,_,_))=self.shot_frames(aim,dir);
        let (origin,ray)=self.ray(c);
        let mut best=(f32::INFINITY,None);
        for (which,normal) in [up,h.cross(up).normalize_or_zero()].into_iter().enumerate() {
            let denom=ray.dot(normal); if denom.abs()<1e-5 {continue;}
            let t=(aim-origin).dot(normal)/denom; if t<=0.0 {continue;}
            let radius=(origin+ray*t).distance(aim);
            if radius<=SHOT_HANDLE_M && SHOT_HANDLE_M-radius<best.0 {best=(SHOT_HANDLE_M-radius,Some(which));}
        }
        best.1
    }
    fn game_pixel_span(&mut self,x:i32,y:i32,width:i32,ink:[f32;4]) {
        if y<0 || y>=self.r.ih as i32 || width<=0 {return;}
        let crop=self.crop();
        let dx=(1.0-self.view_x())/(self.r.iw as f32*crop[2]);
        let dy=self.view_h()/(self.r.ih as f32*crop[3]);
        let x0=self.view_x()+(x as f32/self.r.iw as f32-crop[0])/crop[2]*(1.0-self.view_x());
        let y0=(y as f32/self.r.ih as f32-crop[1])/crop[3]*self.view_h();
        let left=x0.max(self.view_x());let right=(x0+width as f32*dx).min(1.0);
        if right>left && y0>=0.0 && y0+dy<=self.view_h()+1e-5 {self.r.fill_rect(left,y0,right-left,dy,ink);}
    }
    fn game_point(&self, p:Vec3)->Option<(i32,i32)> {
        self.project(p).and_then(|p|self.to_frame(p)).map(|p|((p[0]*self.r.iw as f32).floor() as i32,(p[1]*self.r.ih as f32).floor() as i32))
    }
    fn game_line(&mut self,a:Vec3,b:Vec3,ink:[f32;4]) {
        let (Some((mut x,mut y)),Some((bx,by)))=(self.game_point(a),self.game_point(b)) else{return;};
        let dx=(bx-x).abs();let sx=if x<bx {1}else{-1};let dy=-(by-y).abs();let sy=if y<by {1}else{-1};let mut err=dx+dy;
        loop {self.game_pixel_span(x,y,1,ink);if x==bx && y==by {break;}let e=err*2;if e>=dy{err+=dy;x+=sx;}if e<=dx{err+=dx;y+=sy;}}
    }
    pub(super) fn draw_aim_sphere(&mut self) {
        let red=[0.92,0.06,0.06,1.0];let white=[1.0,1.0,1.0,1.0];
        let (aim,dir)=self.aim_line();
        self.game_line(aim,aim-dir*1.3,[red[0],red[1],red[2],0.6]);
        if self.aim_rings {
            for which in 0..2 {
                let points=self.shot_curve(which);let colour=if which==0 {red}else{white};
                if self.aim_drag==Some(which) {
                    let polygon:Vec<_>=points.iter().filter_map(|p|self.game_point(*p)).collect();
                    if polygon.len()==points.len() {
                        let lo=polygon.iter().map(|p|p.1).min().unwrap_or(0);let hi=polygon.iter().map(|p|p.1).max().unwrap_or(0);
                        for y in lo..=hi {
                            let mut xs=Vec::new();let sample=y as f32+0.5;
                            for edge in polygon.windows(2) {let (a,b)=(edge[0],edge[1]);
                                if (a.1 as f32<=sample && b.1 as f32>sample)||(b.1 as f32<=sample && a.1 as f32>sample) {
                                    xs.push(a.0 as f32+(sample-a.1 as f32)*(b.0-a.0) as f32/(b.1-a.1) as f32);
                                }
                            }
                            xs.sort_by(f32::total_cmp);
                            if let (Some(a),Some(b))=(xs.first(),xs.last()){self.game_pixel_span(a.ceil() as i32,y,(b.floor()-a.ceil()) as i32+1,[colour[0],colour[1],colour[2],0.12]);}
                        }
                    }
                }
                for p in points.windows(2){self.game_line(p[0],p[1],colour);}
            }
        }
        if let Some((x,y))=self.game_point(aim){self.game_pixel_span(x-3,y,7,red);for row in y-3..=y+3{self.game_pixel_span(x,row,1,red);}}
    }
}
