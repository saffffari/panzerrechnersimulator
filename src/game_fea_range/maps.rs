//! Persistent map selection and per-map view state. Geometry/authoring still belongs to the
//! ordinary manifest worldbuilder; look tuning uses the same live renderer as RANGE/BATTLE.
use super::*;
use std::path::PathBuf;
pub(super) const MAPS:[(&str,&str);2]=[
    ("CANYON",crate::canyon::DEFAULT_MANIFEST),
    ("BRIDGE FIELD","assets/worldgen/workshops/bridge-look-editor-20261105/manifest.v1.json")];
fn stored_selection()->Option<PathBuf> {
    std::fs::read_to_string("target/editor_map.json").ok()
        .and_then(|s|serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v|v["manifest"].as_str().map(PathBuf::from))
}
pub(super) fn selected()->PathBuf {
    if let Some(path)=std::env::var_os("KESSEL_FEA_RANGE_MANIFEST") {return PathBuf::from(path);}
    if let Some(path)=stored_selection() {
        match crate::act_manifest::ActManifest::load(&path) {
            Ok(_)=>return path,
            Err(e)=>eprintln!("[editor-map] saved map is unavailable; opening Canyon: {e:#}"),
        }
    }
    crate::canyon::manifest_path()
}
/// Selection is a receipt of a successfully built world, never a pending load request.
pub(super) fn confirm_selected(manifest:&str) {
    if let Err(e)=write_json("target/editor_map.json",&serde_json::json!({"manifest":manifest})) {
        eprintln!("[editor-map] could not save successful selection: {e:#}");
    }
}
fn previous_selection(failed:&std::path::Path,previous:Option<PathBuf>)->Option<PathBuf> {
    let key=|p:&std::path::Path|p.canonicalize().unwrap_or_else(|_|p.to_path_buf());
    previous.filter(|p|key(p)!=key(failed))
}
/// If a selected map fails after manifest validation, rebuild the last successful map once.
/// A failure of that map cannot recursively retry itself.
pub(super) fn recover_selection()->bool {
    let Some(previous)=previous_selection(&selected(),stored_selection()) else{return false;};
    if crate::act_manifest::ActManifest::load(&previous).is_err(){return false;}
    eprintln!("[editor-map] restoring last successful map: {}",previous.display());
    std::env::set_var("KESSEL_FEA_RANGE_MANIFEST",previous);
    true
}
fn per_map(path:&str)->PathBuf {
    let hash=path.bytes().fold(0xcbf29ce484222325u64,|h,b|(h^b as u64).wrapping_mul(0x100000001b3));
    PathBuf::from(format!("target/fea_range_map_{hash:016x}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_map_can_return_to_previous_but_never_retry_itself() {
        let good=PathBuf::from("/saved/good/manifest.v1.json");
        let bad=std::path::Path::new("/pending/bad/manifest.v1.json");
        assert_eq!(previous_selection(bad,Some(good.clone())),Some(good.clone()));
        assert_eq!(previous_selection(&good,Some(good.clone())),None);
        assert_eq!(previous_selection(bad,None),None);
    }
}
pub(super) fn write_json(path:impl AsRef<std::path::Path>,v:&serde_json::Value)->anyhow::Result<()> {
    let path=path.as_ref();if let Some(parent)=path.parent(){std::fs::create_dir_all(parent)?;}
    let tmp=path.with_extension("json.tmp");std::fs::write(&tmp,serde_json::to_vec_pretty(v)?)?;std::fs::rename(tmp,path)?;Ok(())
}
pub(super) fn write_state(state:&serde_json::Value)->anyhow::Result<()> {
    write_json(STATE_PATH,state)?;
    if let Some(manifest)=state["manifest"].as_str(){write_json(per_map(manifest),state)?;}
    Ok(())
}
impl Range {
    pub(super) fn map_label(&self)->&str {MAPS.iter().find(|(_,p)|self.manifest.ends_with(p.trim_start_matches("assets/"))).map_or("CUSTOM MAP",|(s,_)|s)}
    pub(super) fn request_map(&mut self,index:usize) {
        let result=(||->anyhow::Result<String>{
            let path=crate::resource::resolve(MAPS[index%MAPS.len()].1)?;
            crate::act_manifest::ActManifest::load(&path)?;
            Ok(path.to_string_lossy().into_owned())
        })();
        match result {
            Ok(path)=>{self.pending_map=Some(path);self.status="LOADING MAP".into();},
            Err(e)=>self.status=format!("MAP NOT LOADED: {e:#}")
        }
    }
    pub(super) fn save_map_state(&self) {
        let state=self.state_json();
        if let Err(e)=write_state(&state) {eprintln!("[range-save] {e:#}");}
    }
    pub(super) fn load_map_state(&mut self) {
        let own=per_map(&self.manifest);
        let path=if own.exists(){own}else{PathBuf::from(STATE_PATH)};
        let Some(mut v)=std::fs::read_to_string(path).ok().and_then(|s|serde_json::from_str::<serde_json::Value>(&s).ok())else{return;};
        let foreign=v["manifest"].as_str().map_or(self.map_label()=="BRIDGE FIELD",|p|p!=self.manifest);
        if foreign {
            if let Some(o)=v.as_object_mut(){for k in ["camera","tiger","sky","battle","section","snow","wear"] {o.remove(k);}}
        }
        self.apply_state(&v);
        if foreign {self.frame_tiger();}
    }
}
