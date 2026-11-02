//! Preserve authoring ownership before editor paging discards it. This does not ground geometry.
//! Only positively identified complete independent sources enter the support solver. Index rules
//! mirror editor::placements_from_manifest_value/apply_placement_edits: skipped malformed base
//! rows do not allocate an index; adds append; deletes retain their slot; sets never change asset.
use std::collections::HashSet;
use serde_json::Value;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Policy {Independent,Preserve(&'static str)}
pub struct Ownership {records:Vec<Record>,block_membership_unresolved:bool,horse_scenes:HashSet<String>}
struct Record {asset:String,known_independent:bool,deleted:bool,reason:Option<&'static str>,scene:Option<String>}

impl Ownership {
    pub fn from_manifest(manifest:&crate::act_manifest::ActManifest)->Self {
        Self::from_values(manifest.value(),manifest.edits())
    }
    pub fn policy(&self,source_index:usize,asset_path:&str)->Option<Policy> {
        if !eligible(asset_path){return None;}
        let Some(record)=self.records.get(source_index) else{return Some(Policy::Preserve("effective source index missing"));};
        if record.asset!=asset_path{return Some(Policy::Preserve("effective source asset mismatch"));}
        if record.deleted{return Some(Policy::Preserve("deleted source slot"));}
        if let Some(reason)=record.reason{return Some(Policy::Preserve(reason));}
        if separate_component(asset_path){return Some(Policy::Preserve("separate mechanical hull/turret/gun component"));}
        if self.block_membership_unresolved{return Some(Policy::Preserve("rigid scene-block membership is not retained per placement"));}
        if horse_or_cart(asset_path)&&record.scene.as_ref().is_some_and(|s|self.horse_scenes.contains(s)) {
            return Some(Policy::Preserve("cart/sledge and horses share an authored traces scene"));
        }
        if record.known_independent&&complete_source(asset_path){Some(Policy::Independent)}
        else{Some(Policy::Preserve("independent complete-source ownership not established"))}
    }
    fn from_values(base:&Value,edits:Option<&Value>)->Self {
        // The exact generator owns these base whole vehicles independently. This authoring
        // identity is stronger than recognizing a word such as `wreck` in a filename.
        let canyon_base=base["authoring"]["tool"].as_str()==Some("tools/worldgen/gaea_canyon_v2.py")
            &&base["authoring"]["variant"].as_str()==Some("canyon-v2");
        let mut assets:Vec<String>=base["assets"].as_array().into_iter().flatten()
            .filter_map(|v|v.as_str().map(str::to_owned)).collect();
        let mut records=Vec::new();
        for row in base["placements"].as_array().into_iter().flatten() {
            let Some(row)=row.as_array().filter(|r|r.len()>=6) else{continue;};
            let asset=row[0].as_u64().unwrap_or(0) as usize;
            records.push(Record{asset:assets.get(asset).cloned().unwrap_or_default(),known_independent:canyon_base,
                deleted:false,reason:None,scene:None});
        }
        if let Some(edits)=edits {for op in edits["ops"].as_array().into_iter().flatten() {
            match op["op"].as_str() {
                Some("add")=>{
                    let path=op["asset"].as_str().unwrap_or("");
                    if !assets.iter().any(|p|p==path){assets.push(path.to_owned());}
                    records.push(Record{asset:path.to_owned(),known_independent:generated_wreck_op(op),
                        deleted:false,reason:explicit_preservation(op),scene:op["scene"].as_str().map(str::to_owned)});
                }
                Some("delete")=>{if let Some(r)=op["i"].as_u64().and_then(|i|records.get_mut(i as usize)){r.deleted=true;}}
                Some("set")=>{if let Some(r)=op["i"].as_u64().and_then(|i|records.get_mut(i as usize)) {
                    // An untagged hand edit is not overwritten simply because its original
                    // generator was known. Keep earlier explicit burial/assembly constraints.
                    r.known_independent=generated_wreck_op(op);
                    r.reason=r.reason.or_else(||explicit_preservation(op));
                    if let Some(s)=op["scene"].as_str(){r.scene=Some(s.to_owned());}
                }}
                _=>{}
            }
        }}
        if let Some(edits)=edits {
            for hull in edits["winter_wreck_aftermath_20260925"]["hulls"].as_array().into_iter().flatten() {
                if let Some(index)=hull["base_source_index"].as_u64().or_else(||hull["effective_source_index"].as_u64()) {
                    if let Some(r)=records.get_mut(index as usize){r.reason=Some("registered multipart winter wreck assembly");}
                }
            }
        }
        let horse_scenes=records.iter().filter(|r|!r.deleted&&r.asset.contains("draught_horse_"))
            .filter_map(|r|r.scene.clone()).collect();
        let block_membership_unresolved=[Some(base),edits].into_iter().flatten().any(|v|
            ["scene_blocks","scene_recipe_instances"].iter().any(|k|v[*k].as_array().is_some_and(|a|!a.is_empty())));
        Self{records,horse_scenes,block_membership_unresolved}
    }
}

fn generated_wreck_op(op:&Value)->bool {
    op["tag"].as_str()==Some("canyon_v2_wreckage")
        &&op["scene"].as_str().is_some_and(|s|s.starts_with("canyon_v2_wreckage."))
}
fn explicit_preservation(op:&Value)->Option<&'static str> {
    if ["mired","intentionally_buried","preserve_ground_pose"].iter().any(|k|op[*k].as_bool()==Some(true))
        ||op["burial_m"].as_f64().is_some_and(|v|v>0.)||op["sink_m"].as_f64().is_some_and(|v|v<0.)
        ||matches!(op["grounding"].as_str(),Some("mired"|"buried"|"preserve")) {
        return Some("explicit intentional burial or preserved ground pose");
    }
    if ["host","assembly","parent","rigid_anchor","scene_block"].iter().any(|k|op.get(*k).is_some_and(|v|!v.is_null())) {
        return Some("explicit mechanical or rigid placement owner");
    }
    None
}
fn complete_source(path:&str)->bool {
    matches!(path,
        "assets/authoring/battlefield/pz3_wreck_burnt.glb"|"assets/authoring/battlefield/pz3_wreck_detail.glb"
        |"assets/authoring/battlefield/kv1_wreck_burnt.glb"|"assets/authoring/battlefield/kv1_wreck_detail.glb"
        |"assets/authoring/battlefield/lorry_wreck_burnt.glb"|"assets/authoring/battlefield/lorry_wreck_detail.glb"
        |"assets/authoring/battlefield/panje_sledge_wreck_detail.glb"|"assets/authoring/battlefield/panje_sledge_overturned_wreck_detail.glb"
        |"assets/authoring/canyon-v2/wreck_cart.glb"|"assets/authoring/canyon-v2/wreck_gun45.glb")
}
fn separate_component(path:&str)->bool {
    let component=["_hull.glb","_turret.glb","_gun.glb"].iter().any(|s|path.ends_with(s));
    component&&(path.contains("/tanks/")||path.contains("/battlefield/tiger_wreck_"))
}
fn eligible(path:&str)->bool {complete_source(path)||separate_component(path)||path.contains("wreck")}
fn horse_or_cart(path:&str)->bool {path.contains("wreck_cart")||path.contains("panje_sledge")||path.contains("draught_horse_")}

#[cfg(test)] mod tests {
    use super::*;
    use serde_json::json;
    const W:&str="assets/authoring/battlefield/pz3_wreck_burnt.glb";
    fn base()->Value{json!({"assets":[W],"placements":[[0,0,0,0,0,1],[0,5,0,0,0,1]],
        "authoring":{"tool":"tools/worldgen/gaea_canyon_v2.py","variant":"canyon-v2"}})}
    fn add(path:&str)->Value{json!({"op":"add","asset":path,"x":0,"y":0,"z":0,"deg":0,"s":1,
        "tag":"canyon_v2_wreckage","scene":"canyon_v2_wreckage.retreat.00"})}
    #[test] fn delete_keeps_index_add_appends_and_untagged_set_revokes_independence() {
        let mut set=add(W);set["op"]=json!("set");set["i"]=json!(1);set.as_object_mut().unwrap().remove("tag");
        let e=json!({"ops":[{"op":"delete","i":0},add(W),set]});let o=Ownership::from_values(&base(),Some(&e));
        assert_eq!(o.records.len(),3);assert_eq!(o.policy(0,W),Some(Policy::Preserve("deleted source slot")));
        assert_eq!(o.policy(2,W),Some(Policy::Independent));assert!(matches!(o.policy(1,W),Some(Policy::Preserve(_))));
    }
    #[test] fn explicit_burial_and_mechanical_components_are_preserved() {
        let mut buried=add(W);buried["burial_m"]=json!(0.3);
        let hull="assets/tanks/tiger/baked/half/tiger_hull.glb";
        let e=json!({"ops":[buried,add(hull)]});let o=Ownership::from_values(&base(),Some(&e));
        assert_eq!(o.policy(2,W),Some(Policy::Preserve("explicit intentional burial or preserved ground pose")));
        assert_eq!(o.policy(3,hull),Some(Policy::Preserve("separate mechanical hull/turret/gun component")));
    }
    #[test] fn horse_traces_preserve_cart_but_not_unconnected_lorry_in_same_scene() {
        let cart="assets/authoring/canyon-v2/wreck_cart.glb";let lorry="assets/authoring/battlefield/lorry_wreck_detail.glb";
        let e=json!({"ops":[add(cart),add("assets/authoring/battlefield/draught_horse_a_wreck_detail.glb"),add(lorry)]});
        let o=Ownership::from_values(&base(),Some(&e));assert!(matches!(o.policy(2,cart),Some(Policy::Preserve(_))));
        assert_eq!(o.policy(4,lorry),Some(Policy::Independent));
    }
    #[test] fn native_canyon_indices_match_real_editor_pages_and_admit_actual_whole_wrecks() {
        let path=std::path::Path::new("assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json");
        if !path.exists(){return;}
        let m=crate::act_manifest::ActManifest::load(path).unwrap();let o=Ownership::from_manifest(&m);
        let pages=crate::editor::manifest_pages_from(&m).unwrap();let (mut independent,mut preserved)=(0,0);
        for asset in pages.prototypes {for page in asset.pages {for i in page.source_indices {
            assert_eq!(o.records[i].asset,asset.path,"effective index {i} must mean the same source to both owners");
            match o.policy(i,&asset.path) {Some(Policy::Independent)=>independent+=1,Some(Policy::Preserve(_))=>preserved+=1,None=>{}}
        }}}
        assert!(independent>=200,"actual launcher wrecks must reach admission: {independent}");
        assert!(preserved>0,"horse/cart compounds remain explicit, not independently moved");
    }
}
