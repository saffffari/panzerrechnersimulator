//! Bounded grazing kinetic contact response. These coefficients are authored approximations,
//! not measured projectile/plate ricochet data. The normal is the struck source surface normal;
//! the model conserves projectile mass, dissipates kinetic energy, and never supplies interior work.
use super::{ArmorMaterial,ImpactEvent,ShellKind,Terminal};
use glam::Vec3;

pub const MAX_DEFLECTIONS:u8=4;
pub const MAX_FLIGHT_SECONDS:f32=30.0;
pub const SEPARATION_M:f32=0.002;
#[derive(Clone,Copy,Debug)]
pub struct Response {
    pub velocity:Vec3,
    pub outward:Vec3,
    pub incoming_j:f32,
    pub retained_j:f32,
    pub deposited_j:f32,
}

/// Only a surviving, grazing cannon AP/APCBC/APCR defeat can turn into a continuation.
/// Shatter, penetration, chemical events and near-normal stopped impacts remain terminal.
/// 65deg (AP/APCBC) /75deg (APCR) and the restitution coefficients are explicit uncalibrated
/// gameplay response parameters. No random direction or target-dependent correction is used.
pub fn response(event:&ImpactEvent,incoming:Vec3,outward:Vec3,count:u8)->Option<Response> {
    if event.terminal!=Terminal::Bounce || count>=MAX_DEFLECTIONS
        || event.shell.caliber_mm<20.0 || !incoming.is_finite() || !outward.is_finite()
        || !event.shell.mass_kg.is_finite() || event.shell.mass_kg<=0.0
        || !event.plate.thickness_mm.is_finite() || event.plate.thickness_mm<=0.0 {return None;}
    let minimum_angle=match event.shell.kind {ShellKind::Ap|ShellKind::Apcbc=>65.0_f32,
        ShellKind::Apcr=>75.0, _=>return None};
    let speed=incoming.length();let n=outward.normalize_or_zero();
    if speed<30.0 || n.length_squared()<0.5 {return None;}
    let inward=-incoming.dot(n);
    if inward<=0.0 || inward/speed>minimum_angle.to_radians().cos() {return None;}
    // Tangential sliding loses momentum to gouging. The normal component is strongly inelastic.
    let tangential=match event.plate.material {ArmorMaterial::Cast=>0.78,
        ArmorMaterial::FaceHardened=>0.86,ArmorMaterial::Rha|ArmorMaterial::LateGerman=>0.82};
    let normal_restitution=0.20;
    let velocity=(incoming+n*inward)*tangential+n*(inward*normal_restitution);
    let incoming_j=0.5*event.shell.mass_kg*incoming.length_squared();
    let retained_j=0.5*event.shell.mass_kg*velocity.length_squared();
    if !velocity.is_finite() || velocity.length()<25.0 || velocity.dot(n)<=0.0
        || !(retained_j>0.0 && retained_j<incoming_j) {return None;}
    Some(Response {velocity,outward:n,incoming_j,retained_j,deposited_j:incoming_j-retained_j})
}

pub fn separated_origin(point:Vec3,outward:Vec3)->Vec3 {
    // Two millimetres at normal world coordinates; at large coordinates cover float roundoff.
    // No shooter/vehicle exemption and no large forward teleport through adjacent armor.
    let epsilon=SEPARATION_M.max(point.abs().max_element()*f32::EPSILON*4.0);
    point+outward*epsilon
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stopped()->ImpactEvent {
        let mut event=super::super::resolve(&super::super::PZGR_39,200.0,
            super::super::ArmorScheme::TigerI.plate(super::super::Facing::HullFront),75.0,7);
        event.terminal=Terminal::Bounce;event
    }
    #[test]
    fn grazing_response_dissipates_energy_and_exits_rotated_source_plane() {
        for normal in [Vec3::Y,Vec3::Z,Vec3::new(0.3,0.7,-0.2).normalize()] {
            let tangent=normal.cross(if normal.y.abs()<0.9 {Vec3::Y}else{Vec3::X}).normalize();
            for angle in [65.1_f32,72.0,82.0,89.5] {
                let incoming=(tangent*angle.to_radians().sin()-normal*angle.to_radians().cos())*600.0;
                let result=response(&stopped(),incoming,normal,0).unwrap();
                assert!(result.velocity.dot(normal)>0.0);
                assert!(result.velocity.dot(tangent)>0.0);
                assert!((result.incoming_j-result.retained_j-result.deposited_j).abs()<0.2);
                assert!(result.retained_j<result.incoming_j*0.8);
                for point in [Vec3::ZERO,Vec3::new(201.0,5.0,-411.0)] {
                    let start=separated_origin(point,normal);
                    let end=start+result.velocity/240.0;
                    assert!((start-point).dot(normal)>0.0 && (end-point).dot(normal)>0.0,
                        "same source plane cannot be crossed on the outgoing segment");
                }
            }
        }
    }
    #[test]
    fn stopped_shattered_chemical_and_exhausted_rounds_do_not_continue() {
        let mut event=stopped();let grazing=Vec3::new(500.0,-50.0,0.0);
        assert!(response(&event,grazing,Vec3::Y,0).is_some());
        for incoming in [-Vec3::Y*500.0,Vec3::Y*500.0,Vec3::ZERO,Vec3::splat(f32::NAN)] {
            assert!(response(&event,incoming,Vec3::Y,0).is_none());
        }
        assert!(response(&event,grazing,Vec3::Y,MAX_DEFLECTIONS).is_none());
        for terminal in [Terminal::Shatter,Terminal::Penetration,Terminal::HeBlast,Terminal::Ricochet] {
            event.terminal=terminal;assert!(response(&event,grazing,Vec3::Y,0).is_none());
        }
        event.terminal=Terminal::Bounce;event.shell=&super::super::SPRGR_88;
        assert!(response(&event,grazing,Vec3::Y,0).is_none());
        event.shell=&super::super::PANZERFAUST_60;
        assert!(response(&event,grazing,Vec3::Y,0).is_none());
    }
}
