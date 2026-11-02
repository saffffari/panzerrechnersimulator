//! Authored field dispatch table. Contact is the exact visible paper mesh, not an area trigger.
//! The order acknowledgement is an explicit player action; no invented radio transmission.
use super::*;

#[derive(Clone,Debug,Serialize,Deserialize)]
pub(in crate::game) struct Ledger {pub site:u64,pub position:Vec3,pub paper_slot:usize}

fn paper_mesh()->CpuMesh {
 crate::prims::box_mesh(Vec3::new(-0.25,0.812,-0.18),Vec3::new(0.25,0.821,0.18),[219,211,186])
}
pub(super) fn install(scene:&mut Vec<CpuMesh>,occluders:&mut Vec<stealth::Occluder>,spec:&Spec,hf:&terrain::Heightfield)->Vec<Ledger>{
 let mut result=Vec::new();
 for site in spec.sites.iter().filter(|s|s.kind==Kind::Dispatch){
  let x=site.position[0];let z=site.position[1]-9.;let position=Vec3::new(x,hf.height_at(x,z),z);
  let mut add=|lo:Vec3,hi:Vec3,color|{scene.push(crate::prims::box_mesh(position+lo,position+hi,color));occluders.push(stealth::Occluder{min:position+lo,max:position+hi});};
  add(Vec3::new(-0.70,0.76,-0.36),Vec3::new(0.70,0.81,0.36),[93,77,54]);
  for x in [-0.61,0.61]{for z in [-0.27,0.27]{add(Vec3::new(x-0.035,0.,z-0.035),Vec3::new(x+0.035,0.76,z+0.035),[76,62,43]);}}
  let paper_slot=scene.len();let mut paper=paper_mesh();for v in &mut paper.vertices{v.pos=(Vec3::from_array(v.pos)+position).to_array();}paper.min+=position;paper.max+=position;scene.push(paper);
  // Black ink is small actual coplanar-inset geometry on the paper, with no emission.
  for n in 0..6{let z=-0.12+n as f32*0.043;scene.push(crate::prims::box_mesh(position+Vec3::new(-0.20,0.8211,z),position+Vec3::new(if n==0{0.14}else{0.20},0.8212,z+0.004),[43,40,34]));}
  result.push(Ledger{site:site.id,position,paper_slot});
 }result
}
impl Game {
 fn expedition_relay_pick(&self,eye:Vec3,dir:Vec3)->Option<u64>{
  if self.mode!=Mode::Foot||!self.controlled_soldier_alive()||self.halftrack_occupied(){return None;}
  let t=self.theatre.as_ref()?;
  t.relays.iter().find_map(|l|{
   if l.position.distance(eye)>3.{return None;}
   let d=ray_loader_ammunition_mesh(eye,dir,Mat4::from_translation(l.position),2.2,&paper_mesh())?;
   self.equipment_path_clear(eye,eye+dir*d,0.02).then_some(l.site)
  })
 }
 pub fn expedition_relay_hover(&self,eye:Vec3,dir:Vec3)->Option<String>{
  let id=self.expedition_relay_pick(eye,dir)?;let t=self.theatre.as_ref()?;
  Some(if t.sites.iter().any(|s|s.id==id&&s.relay_delivered){"DISPATCH LEDGER / ORDER ACKNOWLEDGED"}else if t.active==Some(id){"DISPATCH LEDGER / LMB ACKNOWLEDGE ORDER"}else{"DISPATCH LEDGER / ACCEPT THE RELAY ORDER ON YOUR MAP"}.into())
 }
 pub fn expedition_relay_interact(&mut self,eye:Vec3,dir:Vec3)->bool{
  let Some(id)=self.expedition_relay_pick(eye,dir)else{return false};let t=self.theatre.as_mut().unwrap();
  if t.active==Some(id){if let Some(s)=t.sites.iter_mut().find(|s|s.id==id){s.relay_delivered=true;t.notice="Written order acknowledged. Secure the relay before reporting completion.".into();}}
  self.consume_primary_press();true
 }
}
