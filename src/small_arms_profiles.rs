//! Live weapon/cartridge families, not a validated exterior-ballistics table.
//! Muzzle-reference provenance and estimated masses/drag are in SMALL_ARMS_FLIGHT_20261001.md.
use crate::ballistics::{ShellKind, ShellSpec};
use crate::infantry_combat::{Team, Weapon};

pub(super) const PISTOL: ShellSpec = ShellSpec {
    name:"9 mm P08 ball (estimated flight)", kind:ShellKind::Ap, caliber_mm:9.0,
    mass_kg:0.008, muzzle_velocity:350.0, filler_g:0.0, drag_k:0.0018,
    pen_ref_mm:0.0, ref_range_m:0.0,
};
pub(super) const GERMAN_RIFLE: ShellSpec = ShellSpec {
    name:"7.92 mm German rifle ball (estimated flight)", caliber_mm:7.92,
    mass_kg:0.0128, muzzle_velocity:760.0, drag_k:0.0012, ..PISTOL
};
pub(super) const GERMAN_MG: ShellSpec = ShellSpec {
    name:"7.92 mm MG34 ball (estimated flight)", ..GERMAN_RIFLE
};
pub(super) const SOVIET_RIFLE: ShellSpec = ShellSpec {
    name:"7.62 mm M1891/30 light ball (reference launch, estimated drag)", caliber_mm:7.62,
    mass_kg:0.0096, muzzle_velocity:2838.0*0.3048, drag_k:0.0015, ..PISTOL
};
pub(super) const SOVIET_MG: ShellSpec = ShellSpec {
    name:"7.62 mm DT ball (DP launch proxy, estimated drag)", muzzle_velocity:2771.0*0.3048,
    ..SOVIET_RIFLE
};
pub(super) fn profile(weapon:Weapon,team:Team)->&'static ShellSpec {
    match (weapon,team) {
        (Weapon::P08,_)=>&PISTOL,
        (Weapon::Rifle,Team::German)=>&GERMAN_RIFLE,
        (Weapon::MachineGun,Team::German)=>&GERMAN_MG,
        (Weapon::Rifle,Team::Soviet)=>&SOVIET_RIFLE,
        (Weapon::MachineGun,Team::Soviet)=>&SOVIET_MG,
    }
}
/// Explicit authored belt mix. Ball and tracer share the family flight approximation until a
/// cartridge-specific tracer drag/charge table is available. Rifles and pistols carry ball only.
pub(super) fn tracer_round(weapon:Weapon,round:u32)->bool {
    weapon==Weapon::MachineGun && round>0 && round%5==0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn weapon_families_have_separate_launch_and_no_tank_penetration() {
        for (w,t) in [(Weapon::P08,Team::German),(Weapon::Rifle,Team::German),
            (Weapon::MachineGun,Team::German),(Weapon::Rifle,Team::Soviet),
            (Weapon::MachineGun,Team::Soviet)] {
            let s=profile(w,t); assert_eq!(s.pen_ref_mm,0.0);assert_eq!(s.filler_g,0.0);
            assert!(s.velocity_at(300.0)<s.muzzle_velocity);
            assert!(s.time_of_flight(300.0)>300.0/s.muzzle_velocity);
        }
        assert_eq!(profile(Weapon::Rifle,Team::Soviet).caliber_mm,7.62);
        assert!((SOVIET_RIFLE.muzzle_velocity-865.0224).abs()<0.001);
        assert!((SOVIET_MG.muzzle_velocity-844.6008).abs()<0.001);
        assert!(SOVIET_RIFLE.time_of_flight(100.0)<GERMAN_RIFLE.time_of_flight(100.0));
    }
    #[test]
    fn mounted_cyclic_rounds_and_visible_belt_selection_share_one_count() {
        fn timeline(dt: f32, frames: usize) -> (usize, Vec<f64>) {
            let mut phase = 0.0;
            let mut round = 0_u32;
            let mut clock = 0.0;
            let mut tracers = Vec::new();
            for _ in 0..frames {
                for offset in crate::mounted_mg::shot_offsets(&mut phase, dt) {
                    round += 1;
                    if tracer_round(Weapon::MachineGun, round) {
                        tracers.push(clock + f64::from(offset));
                    }
                }
                clock += f64::from(dt);
            }
            (round as usize, tracers)
        }
        let (rounds, tracers) = timeline(0.25, 40);
        assert_eq!(rounds, 134); // first cycle at t=0, half-open ten-second interval
        assert_eq!(tracers.len(), 26);
        for pair in tracers.windows(2) {
            assert!((pair[1] - pair[0] - 0.375).abs() < 2.0e-6);
        }
        for (dt, frames) in [(1.0/60.0, 600), (0.5, 20)] {
            let (actual_rounds, actual_tracers) = timeline(dt, frames);
            assert_eq!(actual_rounds, rounds);
            assert_eq!(actual_tracers.len(), tracers.len());
            for (a,b) in actual_tracers.iter().zip(&tracers) { assert!((a-b).abs()<2e-6); }
        }
    }

    #[test] fn ball_is_unlit_and_only_each_fifth_mg_round_is_selected() {
        for w in [Weapon::P08,Weapon::Rifle] {assert!((1..101).all(|n|!tracer_round(w,n)));}
        assert_eq!((1..101).filter(|&n|tracer_round(Weapon::MachineGun,n)).count(),20);
        assert!(!tracer_round(Weapon::MachineGun,0));
    }
}
