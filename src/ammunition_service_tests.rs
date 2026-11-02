//! Recovery regressions use the live traced-fragment solver and the actual finite intake helpers.
use super::*;

fn live_round() -> LiveAmmoRound {
    // A finite loose AP cartridge in the cabin, using the same rendered-body dimensions as
    // live_ammunition_in_frame. Its position is an explicit test fixture, not an authored rack.
    let radius = crate::procedural_ordnance::LIVE_MAX_DIAMETER_M * 0.5;
    LiveAmmoRound {
        stable_id: 0x2_0000,
        center: Vec3::new(0.0, 1.65, 0.0),
        rotation: Quat::from_rotation_y(0.36),
        half_extents: Vec3::new(
            radius,
            radius,
            crate::procedural_ordnance::LIVE_LENGTH_M * 0.5,
        ),
        explosive_energy_j: crate::ballistics::PZGR_39.filler_g * 4_184.0,
    }
}

fn crossing_fragment() -> HotFragment {
    HotFragment {
        id: 91,
        active: true,
        frame: BloodFrame::Turret,
        entered_compartment: true,
        pos: live_round().center - Vec3::X * 0.4,
        vel: Vec3::X * 100.0,
        size: 0.01,
        life: 0.01,
        ..Default::default()
    }
}

#[test]
fn failed_rack_does_not_shield_actual_live_cartridges_or_duplicate_detonation() {
    for initially_serviceable in [true, false] {
        let mut modules = default_modules();
        let rack = modules
            .iter_mut()
            .find(|m| m.kind == ModuleKind::AmmoRack)
            .unwrap();
        rack.functional = initially_serviceable;
        rack.fault = if initially_serviceable {
            tiger_damage::Fault::None
        } else {
            tiger_damage::Fault::Spall
        };
        let unaffected: Vec<_> = modules
            .iter()
            .filter(|m| m.kind != ModuleKind::AmmoRack)
            .map(|m| (m.kind, m.functional, m.fault))
            .collect();
        let round = live_round();
        let chip = crossing_fragment();
        let fragments = [chip, HotFragment { id: 92, ..chip }];
        // No component stands in the way: the real cartridge owns contact.
        let (result, casualties, contacts) = resolve_fragment_damage(
            &fragments,
            None,
            &[],
            &FragmentWorld::bare(),
            &[round],
            &mut modules,
        );
        assert!(result.ammo);
        assert_eq!(result.hit_modules, vec![ModuleKind::AmmoRack]);
        assert!(casualties.is_empty() && !result.hit_body && !result.fire);
        assert_eq!(
            contacts.len(),
            1,
            "one physical ammunition blast authority per solve"
        );
        assert_eq!(contacts[0].internal_blast_origin_local, Some(round.center));
        assert_eq!(
            contacts[0].target,
            FragmentDamageTarget::Module(ModuleKind::AmmoRack)
        );
        assert!(contacts[0].energy_j > module_fragment_resistance_j(ModuleKind::AmmoRack));
        assert_eq!(
            unaffected,
            modules
                .iter()
                .filter(|m| m.kind != ModuleKind::AmmoRack)
                .map(|m| (m.kind, m.functional, m.fault))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn failed_rack_without_a_crossed_live_round_cannot_manufacture_ammunition_blast() {
    let round = live_round();
    let displaced = LiveAmmoRound {
        center: round.center + Vec3::Y * 5.0,
        ..round
    };
    for (rounds, size) in [
        (vec![], 0.01),
        (vec![displaced], 0.01),
        (vec![round], 0.001),
    ] {
        let mut modules = default_modules();
        modules
            .iter_mut()
            .find(|m| m.kind == ModuleKind::AmmoRack)
            .unwrap()
            .functional = false;
        let chip = HotFragment {
            size,
            ..crossing_fragment()
        };
        let (result, _, contacts) = resolve_fragment_damage(
            &[chip],
            None,
            &[],
            &FragmentWorld::bare(),
            &rounds,
            &mut modules,
        );
        assert!(!result.ammo && result.hit_modules.is_empty() && contacts.is_empty());
    }
}

fn supply_counts(s: &crate::foot::Supplies) -> [u32; 7] {
    [
        s.ammo,
        s.ap_88mm,
        s.he_88mm,
        s.fuel,
        s.medical,
        s.cigarettes,
        s.rations,
    ]
}

#[test]
fn uncertified_rack_preserves_rounds_and_rejected_stock_through_remount_banking() {
    let mut taken = [false, true, true, true, true, true];
    let mut nature = [false, false, true, false, true, false];
    let initial_taken = taken;
    let initial_nature = nature;
    let mut carried = crate::foot::Supplies {
        ammo: 1,
        ap_88mm: 2,
        he_88mm: 3,
        fuel: 1,
        medical: 2,
        cigarettes: 4,
        rations: 5,
    };
    let initial = supply_counts(&carried);
    let mut banked = crate::foot::Supplies::default();
    stow_recovered_88mm(false, &mut taken, &mut nature, &mut carried, Some(4));
    assert_eq!(
        taken, initial_taken,
        "already-present and carried round identities stay intact"
    );
    assert_eq!(nature, initial_nature);
    assert_eq!(supply_counts(&carried), initial);
    bank_recovered_supplies(&mut banked, &mut carried, false);
    assert_eq!(supply_counts(&carried), [1, 2, 3, 0, 0, 0, 0]);
    assert_eq!(supply_counts(&banked), [0, 0, 0, 1, 2, 4, 5]);
    bank_recovered_supplies(&mut banked, &mut carried, false);
    assert_eq!(supply_counts(&carried), [1, 2, 3, 0, 0, 0, 0]);
    assert_eq!(
        supply_counts(&banked),
        [0, 0, 0, 1, 2, 4, 5],
        "repeated remount cannot bank twice"
    );

    // Certification creates no rounds. The ordinary next transfer spends this same finite stock.
    stow_recovered_88mm(true, &mut taken, &mut nature, &mut carried, Some(4));
    assert_eq!(taken, [false, false, false, false, true, false]);
    assert_eq!(nature, initial_nature);
    assert_eq!((carried.ammo, carried.ap_88mm, carried.he_88mm), (0, 5, 2));
    assert_eq!(
        taken.iter().filter(|empty| !**empty).count() as u32 + carried.ap_88mm + carried.he_88mm,
        1 + 6 + 2 + 3
    );
}
