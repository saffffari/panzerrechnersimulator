//! Baked gun nodes are siblings, not an articulated recoil hierarchy. The shield, its rear
//! flange, trunnion block and barrel collar elevate together, but do not slide into the cabin.
use crate::gltf_load::CpuMesh;
pub const SOURCE: &str = "assets/tanks/tiger/baked/tiger_gun.glb";
pub const ARMOR: &[&str] = &["Gun_0008", "Gun_0007", "Gun_0009", "Gun_0018", "Gun_0025"];
/// Barrel/breech and their attached rods/handle. Everything else is the non-recoiling cradle,
/// coaxial assembly and elevation fittings. The two rods run inside the fixed brake cylinders.
pub const RECOILING: &[&str] = &["Gun_0001", "Breech", "Gun_0110", "Gun_0119", "Gun_0114", "Gun_0115", "Gun_0116"];
pub fn load() -> anyhow::Result<[CpuMesh;3]> {
    let mut pages=crate::gltf_load::load_glb_grouped_roots(SOURCE,&[ARMOR,RECOILING],crate::tank::tiger_paint_finish)?;
    let moving=pages.pop().unwrap();let armor=pages.pop().unwrap();let cradle=pages.pop().unwrap();
    Ok([moving,armor,cradle])
}
