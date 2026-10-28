//! Source-triangle contact authority. BVHs are created once with the source mesh, not per shot.
use super::*;
#[derive(Clone,Copy,Debug)]
pub(super) struct SurfacePlate {
    pub plate:crate::ballistics::Plate,
    pub coverage:&'static str,
    pub manufacture:&'static str,
    pub provenance:&'static str,
}
/// cos(20 degrees): a plate this close to horizontal is roof or belly, not sloped armor.
const HORIZONTAL_PLATE_NORMAL_Y:f32=0.94;
/// Hull-local Z of the Tiger driver's front plate (visor aperture at z=2.386). Upward-facing
/// hull plate ahead of it is the 60 mm glacis, not the 25 mm hull roof.
const TIGER_GLACIS_MIN_Z:f32=2.45;
fn plate(scheme:crate::ballistics::ArmorScheme,part:ArmorPart,point:Vec3,normal:Vec3)->SurfacePlate {
    use crate::ballistics::{ArmorScheme as A,ArmorMaterial as M};
    let facing=facing_from_local_normal(normal,part);
    let mut p=scheme.plate(facing);p.slope_deg=0.0; // angle is measured from the actual source normal
    let (mut coverage,mut provenance)=("plate-class mapping; source triangle verified, local variation unresolved","existing armor scheme; not a per-triangle measured thickness map");
    // Roof and belly are the plates within about 20 degrees of horizontal. The former 0.65
    // cut (49 degrees) also swallowed real sloped armor: a T-34 glacis at 60 degrees from the
    // vertical has normal.y 0.87 and resolved as a 20 mm roof instead of its 45 mm plate.
    if scheme==A::TigerI && part==ArmorPart::Hull && normal.y>HORIZONTAL_PLATE_NORMAL_Y
        && point.z>TIGER_GLACIS_MIN_Z {
        p.thickness_mm=60.0;p.name="Tiger glacis";
        coverage="Tiger glacis nominal class; fore-and-aft boundary authored at the driver's front plate";
        provenance="US War Department TT&T34 Sep23 1943 glacis 2.44in (~62mm) near horizontal; nominal60mm";
    }else if normal.y.abs()>HORIZONTAL_PLATE_NORMAL_Y {
        p=crate::ballistics::Plate{name:if normal.y>0.0 {"source roof plate"}else{"source belly plate"},
            thickness_mm:match scheme {A::TigerI=>25.0,A::T34_76|A::T34_85=>20.0,_=>25.0},slope_deg:0.0,material:M::Rha};
        if scheme==A::TigerI {
            coverage="1943 Tiger roof/belly nominal measured class";
            provenance="US War Department TT&T34 Sep23 1943 armor table 1.02in (~25.9mm); nominal25mm";
        }else{coverage="unverified conservative horizontal plate estimate";provenance="T34 report located but horizontal distribution not yet verified;20mm authored fallback";}
    }else if scheme==A::TigerI && part==ArmorPart::Hull && normal.x.abs()>0.65 && point.y<1.05 {
        p.thickness_mm=60.0;p.name="Tiger lower hull side";
        coverage="Tiger lower-side nominal class; source height boundary authored";
        provenance="US War Department TT&T34 Sep23 1943 lower-side2.44in (~62mm), nominal60mm";
    }
    SurfacePlate{manufacture:if p.material==M::Cast {"cast"}else{"rolled nominal; hardness/lot not measured"},plate:p,coverage,provenance}
}
pub(super) fn contact(source:&surface_fire::CabinFire,models:&[Mat4;surface_fire::FRAMES],tank:&Tank,
    vehicle:ProjectileVehicle,scheme:crate::ballistics::ArmorScheme,start:Vec3,end:Vec3)->Option<ProjectileContact> {
    let hit=source.sweep(start,end,models)?;
    let part=if matches!(hit.frame,0|3|5|9|10|11|14) {ArmorPart::Hull}else{ArmorPart::Turret};
    let pos=start.lerp(end,hit.fraction);
    let world_normal=models[hit.frame].inverse().transpose().transform_vector3(hit.normal).normalize_or_zero();
    let model=if part==ArmorPart::Hull {tank.hull_model()}else{tank.turret_model()};
    let inverse=model.inverse();let local_point=inverse.transform_point3(pos);
    let local_normal=model.transpose().transform_vector3(world_normal).normalize_or_zero();
    Some(ProjectileContact{t_m:start.distance(end)*hit.fraction,pos,kind:ProjectileContactKind::Vehicle(VehicleArmorContact{
        vehicle,part,facing:facing_from_local_normal(local_normal,part),local_point,world_normal,
        surface_plate:Some(plate(scheme,part,local_point,local_normal)),})})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_roof_belly_and_lower_side_retain_distinct_nominal_plates() {
        use crate::ballistics::ArmorScheme as A;
        let roof=plate(A::TigerI,ArmorPart::Hull,Vec3::Y*2.0,Vec3::Y);
        let belly=plate(A::TigerI,ArmorPart::Hull,Vec3::ZERO,Vec3::NEG_Y);
        let lower=plate(A::TigerI,ArmorPart::Hull,Vec3::new(1.5,0.9,0.0),Vec3::X);
        let upper=plate(A::TigerI,ArmorPart::Hull,Vec3::new(1.5,1.5,0.0),Vec3::X);
        assert_eq!(roof.plate.thickness_mm,25.0);assert_eq!(belly.plate.thickness_mm,25.0);
        assert_eq!(lower.plate.thickness_mm,60.0);assert_eq!(upper.plate.thickness_mm,80.0);
        for p in [roof,belly,lower,upper] {assert_eq!(p.plate.slope_deg,0.0);}
        assert!(plate(A::T34_76,ArmorPart::Hull,Vec3::Y,Vec3::Y).coverage.contains("unverified"));
    }
    #[test]
    fn sloped_front_plates_are_not_classified_as_roof() {
        use crate::ballistics::ArmorScheme as A;
        // T-34 glacis: 60 degrees from the vertical, so normal.y is sin(60 deg) = 0.866.
        let glacis_normal=Vec3::new(0.0,0.866,0.5);
        let t34=plate(A::T34_76,ArmorPart::Hull,Vec3::new(0.0,1.1,2.4),glacis_normal);
        let front=A::T34_76.plate(facing_from_local_normal(glacis_normal,ArmorPart::Hull));
        assert_eq!(t34.plate.thickness_mm,front.thickness_mm);
        assert!(t34.plate.thickness_mm>=45.0,"T-34 glacis resolved as {} mm",t34.plate.thickness_mm);
        // T-34 rear upper plate, about 47 degrees: also sloped armor, not a 20 mm roof.
        assert!(plate(A::T34_76,ArmorPart::Hull,Vec3::new(0.0,1.2,-2.6),Vec3::new(0.0,0.73,-0.68)).plate.thickness_mm>20.0);
        // Tiger near-horizontal glacis ahead of the driver's plate is 60 mm; hull roof stays 25 mm.
        let tiger_glacis=plate(A::TigerI,ArmorPart::Hull,Vec3::new(0.0,1.45,2.8),Vec3::new(0.0,0.985,0.17));
        assert_eq!(tiger_glacis.plate.thickness_mm,60.0);
        assert_eq!(plate(A::TigerI,ArmorPart::Hull,Vec3::new(0.0,1.8,0.5),Vec3::Y).plate.thickness_mm,25.0);
        // A truly horizontal T-34 roof keeps the authored horizontal fallback.
        assert_eq!(plate(A::T34_76,ArmorPart::Hull,Vec3::Y,Vec3::Y).plate.thickness_mm,20.0);
    }
    #[test]
    fn swept_source_horizontal_armor_is_not_discarded_and_follows_hull_rotation() {
        let hf=crate::terrain::Heightfield {res:2,spacing:1000.0,heights:vec![0.0;4],min_h:0.0,max_h:0.0,openings:Default::default(),detail:None};
        let mut tank=Tank::new(0.0,0.0,0.0,&hf,crate::tank::TIGER_PARTS);
        tank.pos=Vec3::new(12.0,3.0,-7.0);tank.heading=0.71;tank.pitch=0.32;tank.roll=-0.18;
        let mesh=crate::prims::box_mesh(Vec3::new(-1.0,0.0,-2.0),Vec3::new(1.0,2.0,2.0),[100,100,100]);
        let source=surface_fire::CabinFire::new(&[(0,&mesh)]);
        let mut models=[Mat4::IDENTITY;surface_fire::FRAMES];models[0]=tank.hull_model();
        for (from,to,expected) in [(Vec3::new(0.0,4.0,0.0),Vec3::new(0.0,1.0,0.0),Vec3::Y),
            (Vec3::new(0.0,-2.0,0.0),Vec3::Y,Vec3::NEG_Y)] {
            let a=models[0].transform_point3(from);let b=models[0].transform_point3(to);
            let hit=contact(&source,&models,&tank,ProjectileVehicle::Player,crate::ballistics::ArmorScheme::TigerI,a,b).unwrap();
            let ProjectileContactKind::Vehicle(armor)=hit.kind else{panic!("wrong source contact")};
            assert!((armor.world_normal-models[0].transform_vector3(expected)).length()<1e-5);
            assert_eq!(armor.surface_plate.unwrap().plate.thickness_mm,25.0);
            assert!(hit.t_m>0.0 && hit.t_m<a.distance(b));
        }
        // A path beside the source has no hit, even within a generous whole-vehicle envelope.
        assert!(contact(&source,&models,&tank,ProjectileVehicle::Player,crate::ballistics::ArmorScheme::TigerI,
            models[0].transform_point3(Vec3::new(1.2,4.0,0.0)),models[0].transform_point3(Vec3::new(1.2,-2.0,0.0))).is_none());
    }
}
