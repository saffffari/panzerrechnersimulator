//! Small-unit cohesion above individual combat policies. Membership survives casualties;
//! reports are nearby visible signals, never shared hidden enemy state or firing permission.
use glam::{Vec2, Vec3};

pub const GARRISON_GROUP_SIZE: usize = 6;
pub const COHESION_RADIUS_M: f32 = 18.0;

#[derive(Clone, Debug)]
pub struct Squad {
    pub members: Vec<usize>,
}

#[derive(Clone, Copy)]
pub struct Member {
    pub pos: Vec3,
    pub alive: bool,
    pub yaw: f32,
    /// Only a freshly observed contact may be pointed out to a nearby squadmate.
    pub contact: Option<Vec3>,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Order {
    pub regroup: Option<Vec3>,
    pub look: Option<Vec3>,
}

pub fn orders(squads: &[Squad], members: &[Member], mut visible: impl FnMut(Vec3, Vec3) -> bool) -> Vec<Order> {
    let mut result = vec![Order::default(); members.len()];
    for squad in squads {
        let live: Vec<usize> = squad.members.iter().copied()
            .filter(|&i| members.get(i).is_some_and(|m| m.alive)).collect();
        // A medoid keeps a separated soldier from pulling the whole group after him.
        // Re-elect from the survivors; the first casualty cannot strand every follower.
        let Some(&anchor) = live.iter().min_by(|&&a, &&b| {
            let cost = |i: usize| live.iter().map(|&j| {
                horizontal(members[i].pos - members[j].pos).length().min(40.0)
            }).sum::<f32>();
            cost(a).total_cmp(&cost(b)).then(a.cmp(&b))
        }) else { continue; };
        for (slot, &i) in live.iter().enumerate() {
            let own = members[i];
            let delta = horizontal(own.pos - members[anchor].pos);
            if delta.length() > COHESION_RADIUS_M {
                let angle = slot as f32 * 2.399963;
                result[i].regroup = Some(members[anchor].pos + Vec3::new(angle.cos(), 0.0, angle.sin()) * 3.5);
            }
            // This is a look direction only. Each receiver must acquire the enemy through
            // ordinary FOV/LOS before it can aim or fire; no target memory is copied.
            result[i].look = live.iter().copied().filter(|&j| j != i)
                .filter(|&j| members[j].contact.is_some()
                    && own.pos.distance(members[j].pos) <= 25.0
                    && Vec2::new(own.yaw.sin(),-own.yaw.cos())
                        .dot(horizontal(members[j].pos-own.pos).normalize_or_zero()) > 0.342
                    && visible(own.pos + Vec3::Y * 1.5, members[j].pos + Vec3::Y * 1.5))
                .min_by(|&a, &b| own.pos.distance_squared(members[a].pos)
                    .total_cmp(&own.pos.distance_squared(members[b].pos)))
                .and_then(|j| members[j].contact);
        }
    }
    result
}

/// Manifest posts look along (sin h,+cos h); foot actors use (sin y,-cos y).
pub fn actor_yaw(post_heading:f32)->f32 { std::f32::consts::PI-post_heading }

fn horizontal(v: Vec3) -> Vec2 { Vec2::new(v.x, v.z) }

/// Find an entire loose group on measured clear ground near one authored post. It either
/// fits together or rejects the site; it never scatters members across unrelated posts.
pub fn deployment(anchor: Vec2, heading: f32, count: usize, mut clear: impl FnMut(Vec2) -> bool) -> Option<Vec<Vec2>> {
    let right = Vec2::new(heading.cos(), -heading.sin());
    let forward = Vec2::new(heading.sin(), heading.cos());
    let mut candidates = Vec::new();
    for z in -5_i32..=5 {
        for x in -5_i32..=5 {
            let local = Vec2::new(x as f32, z as f32) * 2.8;
            if local.length() <= 14.0 { candidates.push(local); }
        }
    }
    candidates.sort_by(|a,b| a.length_squared().total_cmp(&b.length_squared())
        .then(a.y.total_cmp(&b.y)).then(a.x.total_cmp(&b.x)));
    let mut points = Vec::new();
    for local in candidates {
        let at = anchor + right * local.x + forward * local.y;
        if clear(at) && points.iter().all(|p: &Vec2| p.distance(at) >= 2.5) {
            points.push(at);
            if points.len() == count { return Some(points); }
        }
    }
    None
}

/// An authored hearing-radius model, using actual discharge time and the atmosphere's sound
/// speed. It grants attention only; it never creates visual target memory or permission to fire.
#[derive(Clone)]
struct Report {
    origin: Vec3,
    range: f32,
    emitted_s: f32,
    speed_mps: f32,
    delivered: Vec<bool>,
}
#[derive(Clone, Copy)]
struct Attention { area: Vec3, until_s: f32 }
#[derive(Default)]
pub struct Hearing {
    reports: std::collections::VecDeque<Report>,
    attention: Vec<Option<Attention>>,
    pub admitted: u64,
    pub omitted: u64,
}
impl Hearing {
    pub fn report(&mut self, origin: Vec3, range: f32, emitted_s: f32, speed_mps: f32) {
        if !origin.is_finite() || !range.is_finite() || range <= 0.0
            || !emitted_s.is_finite() || !speed_mps.is_finite() || speed_mps <= 0.0 { return; }
        if self.reports.len() == 128 { self.reports.pop_front(); self.omitted += 1; }
        self.reports.push_back(Report {origin,range,emitted_s,speed_mps,delivered:Vec::new()});
    }
    pub fn expire(&mut self, now_s: f32) {
        self.reports.retain(|r| now_s-r.emitted_s <= r.range/r.speed_mps+1.0);
    }
    pub fn listen(&mut self, id: usize, ear: Vec3, now_s: f32,
        mut clear: impl FnMut(Vec3,Vec3)->bool)->Option<Vec3> {
        if self.attention.len()<=id {self.attention.resize(id+1,None);}
        for report in &mut self.reports {
            if report.delivered.len()<=id {report.delivered.resize(id+1,false);}
            if report.delivered[id] {continue;}
            let age=now_s-report.emitted_s;
            let distance=ear.distance(report.origin);
            if age<0.0 || age*report.speed_mps<distance {continue;}
            report.delivered[id]=true;
            if distance>report.range || !clear(ear,report.origin) {continue;}
            // Deliberately coarser than sight: an uncertain source area, not hidden live position.
            // Stable per listener/source grid; a volley does not jitter the goal every frame.
            let offset=[Vec3::new(-2.0,0.0,1.0),Vec3::new(2.0,0.0,-1.0),
                Vec3::new(1.0,0.0,2.0),Vec3::new(-1.0,0.0,-2.0)][id%4];
            let area=Vec3::new((report.origin.x/8.0).round()*8.0, report.origin.y,
                (report.origin.z/8.0).round()*8.0)+offset;
            self.attention[id]=Some(Attention {area,until_s:now_s+4.0});
            self.admitted+=1;
        }
        let cue=self.attention[id].filter(|a|now_s<=a.until_s);
        self.attention[id]=cue;
        cue.map(|a|a.area)
    }
}

/// The movement role is independent of weapon decisions. Covering men retain their actual
/// sight, rifle cycle, magazine and suppression; movers cannot inherit another man's fire gate.
#[derive(Clone, Copy)]
pub struct Fighter { pub member:Member, pub ready:bool }
#[derive(Clone, Copy)]
struct Maneuver { goal:Vec3, observed_area:Vec3, until_s:f32 }
#[derive(Clone, Copy, Default)]
pub struct TacticalOrder { pub goal:Option<Vec3>, pub cover:bool }
#[derive(Default)]
pub struct Tactics {
    next_plan_s:f32,
    phase:usize,
    moves:Vec<Option<Maneuver>>,
    pub plans:u64,
}
impl Tactics {
    pub fn orders(&mut self, squads:&[Squad], fighters:&[Fighter], now_s:f32,
        mut walkable:impl FnMut(Vec3,Vec3)->bool)->Vec<TacticalOrder> {
        self.moves.resize(fighters.len(),None);
        for (i,plan) in self.moves.iter_mut().enumerate() {
            if plan.is_some_and(|p| now_s>=p.until_s || !fighters[i].member.alive
                || !fighters[i].ready || fighters[i].member.contact
                    .is_none_or(|c|c.distance(p.observed_area)>8.0)) { *plan=None; }
        }
        if now_s>=self.next_plan_s {
            self.next_plan_s=now_s+6.0;
            for squad in squads {
                let engaged:Vec<_>=squad.members.iter().copied().filter(|&i|fighters.get(i)
                    .is_some_and(|f|f.member.alive&&f.ready&&f.member.contact.is_some())).collect();
                // Keep at least two active riflemen in place. No solo flank after casualties.
                if engaged.len()<4 {continue;}
                let reference=fighters[engaged[0]].member.contact.unwrap();
                let engaged:Vec<_>=engaged.into_iter().filter(|&i|
                    fighters[i].member.contact.unwrap().distance(reference)<=8.0).collect();
                if engaged.len()<4 {continue;}
                let center=engaged.iter().map(|&i|fighters[i].member.pos).sum::<Vec3>()/engaged.len() as f32;
                let toward=Vec3::new(reference.x-center.x,0.0,reference.z-center.z);
                let range=toward.length();
                if !(18.0..=60.0).contains(&range) {continue;}
                let direction=toward/range;
                let side=Vec3::new(-direction.z,0.0,direction.x);
                for k in 0..2 {
                    let i=engaged[(self.phase*2+k)%engaged.len()];
                    let own=fighters[i].member.pos;
                    if own.distance(center)>COHESION_RADIUS_M {continue;}
                    let goal=center+side*if k==0 {-8.0} else {8.0}+direction*2.0;
                    if goal.distance(own)>1.5 && walkable(own,goal) {
                        self.moves[i]=Some(Maneuver {goal,observed_area:reference,until_s:now_s+5.0});
                        self.plans+=1;
                    }
                }
            }
            self.phase=self.phase.wrapping_add(1);
        }
        let mut result:Vec<_>=self.moves.iter().map(|m|TacticalOrder {goal:m.map(|p|p.goal),cover:false}).collect();
        for squad in squads {
            if squad.members.iter().any(|&i|result.get(i).is_some_and(|o|o.goal.is_some())) {
                for &i in &squad.members {
                    if let Some(f)=fighters.get(i) {
                        result[i].cover=result[i].goal.is_none()&&f.member.alive&&f.ready&&f.member.contact.is_some();
                    }
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn member(x:f32,z:f32)->Member { Member {pos:Vec3::new(x,0.0,z),alive:true,yaw:std::f32::consts::FRAC_PI_2,contact:None} }
    #[test]
    fn hearing_waits_for_wavefront_and_never_grants_exact_target_memory() {
        let mut h=Hearing::default();let source=Vec3::new(3.1,1.4,7.2);let ear=source+Vec3::X*34.0;
        h.report(source,100.0,10.0,340.0);
        assert!(h.listen(0,ear,10.099,|_,_|true).is_none());
        let area=h.listen(0,ear,10.101,|_,_|true).unwrap();
        assert!(area.distance(source)>0.1 && area.distance(source)<8.0);
        assert_eq!(h.admitted,1);
        assert_eq!(h.listen(0,ear,11.0,|_,_|true),Some(area));
        assert_eq!(h.admitted,1,"the same wave is admitted only once per listener");
        assert!(h.listen(0,ear,14.2,|_,_|true).is_none());
        assert!(h.listen(1,ear,11.0,|_,_|false).is_none());
        assert!(h.listen(1,ear,11.1,|_,_|true).is_none(),"opening cover later cannot replay the passed wave");
        assert!(h.listen(2,source+Vec3::X*150.0,11.0,|_,_|true).is_none());
        h.expire(20.0);assert!(h.reports.is_empty());
    }
    #[test]
    fn hearing_accounts_for_listener_motion_and_has_a_fixed_event_budget() {
        let mut h=Hearing::default();h.report(Vec3::ZERO,200.0,0.0,340.0);
        assert!(h.listen(0,Vec3::X*100.0,0.1,|_,_|true).is_none());
        assert!(h.listen(0,Vec3::X*20.0,0.11,|_,_|true).is_some());
        for k in 0..200 {h.report(Vec3::ZERO,100.0,k as f32,340.0);}
        assert_eq!(h.reports.len(),128);assert_eq!(h.omitted,73);
    }
    #[test]
    fn two_members_flank_while_four_cover_with_their_own_contact() {
        let target=Vec3::new(0.,0.,-35.);
        let mut f:Vec<_>=(0..6).map(|i|Fighter {member:Member {
            pos:Vec3::new((i as f32-2.5)*2.5,0.,0.),alive:true,yaw:0.,contact:Some(target)},ready:true}).collect();
        let squads=[Squad {members:(0..6).collect()}];let mut t=Tactics::default();
        let o=t.orders(&squads,&f,0.0,|_,_|true);
        assert_eq!(o.iter().filter(|o|o.goal.is_some()).count(),2);
        assert_eq!(o.iter().filter(|o|o.cover).count(),4);
        let goals:Vec<_>=o.iter().filter_map(|o|o.goal).collect();
        assert!(goals[0].x*goals[1].x<0.0);
        assert!(goals.iter().all(|g|g.length()<COHESION_RADIUS_M));
        let held=t.orders(&squads,&f,1.0,|_,_|true);
        assert_eq!(o.iter().map(|o|o.goal).collect::<Vec<_>>(),held.iter().map(|o|o.goal).collect::<Vec<_>>());
        let mover=o.iter().position(|o|o.goal.is_some()).unwrap();
        f[mover].member.contact=None;
        assert!(t.orders(&squads,&f,1.1,|_,_|true)[mover].goal.is_none());
        f.iter_mut().take(3).for_each(|f|f.ready=false);
        assert!(t.orders(&squads,&f,6.1,|_,_|true).iter().all(|o|o.goal.is_none()));
        let mut blocked=Tactics::default();
        f.iter_mut().for_each(|f|{f.ready=true;f.member.contact=Some(target)});
        assert!(blocked.orders(&squads,&f,0.,|_,_|false).iter().all(|o|o.goal.is_none()));
    }
    #[test]
    fn separated_man_returns_to_group_without_dragging_group_away() {
        let members = [member(0.,0.),member(3.,0.),member(-3.,0.),member(1.,3.),member(0.,-3.),member(70.,0.)];
        let squads = [Squad{members:(0..6).collect()}];
        let o=orders(&squads,&members,|_,_|true);
        assert!(o[..5].iter().all(|o|o.regroup.is_none()));
        assert!(o[5].regroup.unwrap().length()<8.0);
    }
    #[test]
    fn casualties_re_elect_anchor_without_reviving_or_moving_dead_members() {
        let mut m=[member(0.,0.),member(30.,0.),member(33.,0.),member(70.,0.)];
        m[0].alive=false;
        let o=orders(&[Squad{members:vec![0,1,2,3]}],&m,|_,_|true);
        assert!(o[0].regroup.is_none());
        assert!((o[3].regroup.unwrap().x-33.0).abs()<4.0);
        m[1].alive=false;m[2].alive=false;
        assert!(orders(&[Squad{members:vec![0,1,2,3]}],&m,|_,_|true)[3].regroup.is_none());
    }
    #[test]
    fn contact_signals_require_living_nearby_visible_squadmate() {
        let mut m=[member(0.,0.),member(5.,0.),member(80.,0.)];
        m[1].contact=Some(Vec3::new(100.,0.,100.));
        let squads=[Squad{members:vec![0,1,2]}];
        assert_eq!(orders(&squads,&m,|_,_|true)[0].look,m[1].contact);
        assert!(orders(&squads,&m,|_,_|false)[0].look.is_none());
        assert!(orders(&squads,&m,|_,_|true)[2].look.is_none());
        m[0].yaw=-std::f32::consts::FRAC_PI_2;
        assert!(orders(&squads,&m,|_,_|true)[0].look.is_none());
        m[0].yaw=std::f32::consts::FRAC_PI_2;m[1].alive=false;
        assert!(orders(&squads,&m,|_,_|true)[0].look.is_none());
    }
    #[test]
    fn authored_post_facing_converts_to_actor_facing() {
        for h in [0.0_f32,0.7,std::f32::consts::FRAC_PI_2,std::f32::consts::PI] {
            let y=actor_yaw(h);
            assert!(Vec2::new(y.sin(),-y.cos()).distance(Vec2::new(h.sin(),h.cos()))<1e-6);
        }
    }
    #[test]
    fn deployment_respects_obstacles_spacing_and_group_extent() {
        let center=Vec2::new(12.,-6.);
        let p=deployment(center,0.7,6,|p|p.x<14.).unwrap();
        for (i,a) in p.iter().enumerate() {
            assert!(a.x<14. && a.distance(center)<=14.001);
            assert!(p[..i].iter().all(|b|a.distance(*b)>=2.5));
        }
        assert!(deployment(center,0.,6,|_|false).is_none());
    }
}
