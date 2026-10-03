//! RMB eye relief derived from the two preserved direct turret glass blocks.
//! No optical relay, new hole, hidden armor, or exterior camera is introduced.
use crate::gltf_load::{load_glb_partitioned_roots,CpuMesh};
use glam::Vec3;
use std::sync::OnceLock;

pub const FIRST_INDEX:usize=8;
pub const NAMES:[&str;2]=["GLASS RIGHT PORT","Left Port Glass"];
#[derive(Clone,Copy,Debug)]
pub struct Port {pub inner_center:Vec3,pub outward:Vec3,pub eye:Vec3}

fn source_port(mesh:&CpuMesh)->anyhow::Result<Port> {
    let center=mesh.center();
    let mut largest=0.0f32;let mut outward=Vec3::ZERO;
    for tri in mesh.indices.chunks_exact(3) {
        let [a,b,c]=[0,1,2].map(|i|Vec3::from(mesh.vertices[tri[i] as usize].pos));
        let cross=(b-a).cross(c-a);let area=cross.length();
        if area>largest {largest=area;outward=cross/area;}
    }
    anyhow::ensure!(largest>0.001,"side-port broad source face missing");
    // Positive/negative lateral center is the source's exterior side. Use its real slanted face,
    // not a guessed world X direction or a camera position beyond the outer armor.
    if outward.dot(Vec3::new(center.x,0.0,0.0))<0.0 {outward=-outward;}
    let inner=mesh.vertices.iter().map(|v|Vec3::from(v.pos).dot(outward)).fold(f32::INFINITY,f32::min);
    let inner_center=center+outward*(inner-center.dot(outward));
    anyhow::ensure!(center.x.abs()>0.8 && center.x.abs()<1.2 && outward.x.abs()>0.8,
        "side-port source registration changed");
    let eye=inner_center-outward*0.025;
    Ok(Port{inner_center,outward,eye})
}
fn load()->anyhow::Result<[Port;2]> {
    // The remainder is the separate gunner sight lens and stays in its existing renderer route.
    let pages=load_glb_partitioned_roots("assets/tanks/tiger/baked/tiger_port_turret.glb",&NAMES)?;
    Ok([source_port(&pages[1])?,source_port(&pages[2])?])
}
pub fn ports()->Option<&'static [Port;2]> {
    static PORTS:OnceLock<Option<[Port;2]>>=OnceLock::new();
    PORTS.get_or_init(||match load() {Ok(p)=>Some(p),Err(e)=>{eprintln!("[side-port-focus] source unavailable: {e}");None}}).as_ref()
}
pub fn close_view(index:usize)->Option<(Vec3,Vec3,f32)> {
    let relative=index.checked_sub(FIRST_INDEX)?;
    let port=ports()?.get(relative)?;
    Some((port.eye,port.outward,14_f32.to_radians()))
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn both_source_slits_have_interior_eye_relief_and_distinct_outward_bearings(){
        let p=load().unwrap();
        assert!(p[0].outward.x< -0.8&&p[1].outward.x>0.8);
        for (i,port) in p.iter().enumerate(){
            assert!((port.inner_center-port.eye).dot(port.outward)>0.0249);
            assert!((port.inner_center-port.eye).cross(port.outward).length()<1e-5);
            let (eye,axis,fov)=close_view(FIRST_INDEX+i).unwrap();
            assert_eq!(eye,port.eye);assert_eq!(axis,port.outward);assert!(fov<0.3);
        }
        assert!(close_view(7).is_none());assert!(close_view(10).is_none());
    }
}
