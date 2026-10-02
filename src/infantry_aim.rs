//! Bounded live rifle motor control. The policy still chooses whether to aim, move, fire or
//! reload; only the yaw servo uses a currently observed source point. Projectile spread,
//! body/source collision and the shared weapon cycle remain authoritative.
use crate::foot_ai::{FootAction, FootObservation};
use crate::infantry_combat::{Combatant, TURN_RATE_RAD_S};
/// Authored mechanical alignment tolerance, not a historical marksmanship claim. The existing
/// independent +/-0.011 rad standing spread still perturbs every discharged projectile.
pub const FIRE_ALIGNMENT_RAD: f32 = 0.02;

fn angle_error(from: f32, to: f32) -> f32 {
    (to - from).sin().atan2((to - from).cos())
}

pub(super) fn visible_target_motor(
    actor: &Combatant,
    observation: &FootObservation,
    mut action: FootAction,
    dt: f32,
) -> FootAction {
    // The public observation schema marks current sight at26; stale memory is not permission
    // to servo onto a hidden point. Movement turning remains under the navigation policy.
    if !action.aim || observation.values()[26] <= 0.0 {
        action.fire = false;
        return action;
    }
    let Some(target) = actor.last_observed_aim_point() else {
        action.fire = false;
        return action;
    };
    let delta = target - actor.eye();
    let range = delta.x.hypot(delta.z);
    if !dt.is_finite() || dt <= 0.0 || !range.is_finite() || range < 0.001 {
        action.fire = false;
        return action;
    }
    let error = angle_error(actor.yaw, delta.x.atan2(-delta.z));
    let maximum_step = TURN_RATE_RAD_S * dt;
    action.turn = (error / maximum_step).clamp(-1.0, 1.0);
    let after_turn = angle_error(actor.yaw + action.turn * maximum_step,
        delta.x.atan2(-delta.z));
    // apply_action may translate the body after turning. Reserve the angular amount a legal
    // 1.7m/s step could consume, so an almost-aligned moving shooter cannot bypass this gate.
    let movement_margin = if action.forward.abs() + action.strafe.abs() > 0.0 {
        (1.7 * dt / range).atan()
    } else { 0.0 };
    action.fire &= after_turn.abs() + movement_margin <= FIRE_ALIGNMENT_RAD;
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infantry_combat::{self as combat, Team};
    use glam::Vec3;

    fn seen(actor: &mut Combatant, point: Vec3) -> FootObservation {
        let mut target = Combatant::new(999, Team::German, point, 0.0);
        target.perception_point = Some(point);
        combat::observe(actor, &[target], &[], Vec3::ZERO, 1.0 / 60.0)
    }

    #[test]
    fn twenty_degree_policy_misalignment_converges_at_existing_rate_without_spending_rounds() {
        for hz in [30, 60, 144] {
            let dt = 1.0 / hz as f32;
            let mut actor = Combatant::new(6, Team::Soviet,
                Vec3::new(-781.1, -16.57, 343.16), 84.5_f32.to_radians());
            actor.cooldown = 0.0;
            let target = Vec3::new(-744.3, -16.42, 351.84);
            let mut discharged = None;
            for tick in 0..hz {
                let obs = seen(&mut actor, target);
                let base = FootAction { aim: true, fire: true, turn: -0.5, ..Default::default() };
                let action = visible_target_motor(&actor, &obs, base, dt);
                let before_yaw = actor.yaw;
                let before_ammo = actor.magazine;
                let shot = combat::apply_action(&mut actor, &obs, action, &[], dt, tick as u64);
                assert!((actor.yaw - before_yaw).abs() <= TURN_RATE_RAD_S * dt + 1e-6);
                if let Some(shot) = shot {
                    assert_eq!(actor.magazine + 1, before_ammo);
                    let desired = (target - shot.origin).normalize();
                    assert!(shot.direction.dot(desired).acos() < FIRE_ALIGNMENT_RAD + 0.017,
                        "only finite motor alignment and unchanged spread set the source direction");
                    let shot_yaw = shot.direction.x.atan2(-shot.direction.z);
                    assert!(angle_error(actor.yaw, shot_yaw).abs() < 0.012,
                        "bullet azimuth follows the presented body's yaw, never a target-snapped ray");
                    discharged = Some(tick);
                    break;
                }
                assert_eq!(actor.magazine, before_ammo, "misalignment cannot consume ammunition");
            }
            let tick = discharged.expect("current-sight servo should acquire the seen target");
            assert!(tick as f32 * dt > 0.06, "20deg error must not snap into alignment");
            assert!(tick as f32 * dt < 0.2);
        }
    }

    #[test]
    fn hidden_memory_and_unrequested_aim_cannot_gain_motor_lock_or_firing_permission() {
        let mut actor = Combatant::new(6, Team::Soviet, Vec3::ZERO, 0.0);
        actor.cooldown = 0.0;
        let visible = seen(&mut actor, Vec3::new(2.0, 1.6, -15.0));
        let base = FootAction { turn: -0.25, aim: true, fire: true, ..Default::default() };
        let hidden = combat::observe(&mut actor, &[], &[], Vec3::ZERO, 0.1);
        assert!(actor.last_observed_aim_point().is_some());
        let held = visible_target_motor(&actor, &hidden, base, 1.0 / 60.0);
        assert_eq!(held.turn, base.turn);
        assert!(!held.fire);
        let no_aim = visible_target_motor(&actor, &visible,
            FootAction { aim: false, ..base }, 1.0 / 60.0);
        assert_eq!(no_aim.turn, base.turn);
        assert!(!no_aim.fire);
        let rounds = actor.magazine;
        assert!(combat::apply_action(&mut actor, &hidden, held, &[], 1.0 / 60.0, 0).is_none());
        assert_eq!(actor.magazine, rounds);
    }

    #[test]
    fn motor_keeps_policy_movement_reload_and_trigger_intent_and_existing_bolt_cycle() {
        let mut actor = Combatant::new(6, Team::Soviet, Vec3::ZERO, 0.0);
        actor.cooldown = 0.0;
        let obs = seen(&mut actor, Vec3::new(0.0, 1.6, -30.0));
        let base = FootAction { forward: 0.3, strafe: -0.2, aim: true, fire: false,
            reload: true, crouch: true, ..Default::default() };
        let held = visible_target_motor(&actor, &obs, base, 1.0 / 60.0);
        assert_eq!((held.forward, held.strafe, held.reload, held.crouch, held.fire),
            (base.forward, base.strafe, base.reload, base.crouch, base.fire));
        let mut fired = 0;
        for tick in 0..120 {
            let obs = seen(&mut actor, Vec3::new(0.0, 1.6, -30.0));
            let action = visible_target_motor(&actor, &obs,
                FootAction { aim: true, fire: true, ..Default::default() }, 1.0 / 60.0);
            fired += usize::from(combat::apply_action(&mut actor, &obs, action,
                &[], 1.0 / 60.0, tick).is_some());
        }
        assert_eq!(fired, 2);
        assert_eq!(actor.magazine, 3);
    }
    #[test]
    fn policy_reload_retains_loaded_rounds_and_transfers_only_missing_cartridges() {
        let mut actor = Combatant::new(6, Team::Soviet, Vec3::ZERO, 0.0);
        actor.magazine = 4;
        actor.reserve = 30;
        let base = FootAction { aim: true, reload: true, ..Default::default() };
        for tick in 0..200 {
            let obs = seen(&mut actor, Vec3::new(0.0, 1.6, -30.0));
            let action = visible_target_motor(&actor, &obs, base, 1.0 / 60.0);
            assert!(action.reload);
            assert!(combat::apply_action(&mut actor, &obs, action,
                &[], 1.0 / 60.0, tick).is_none());
            assert_eq!(actor.magazine + actor.reserve, 34);
            assert!(actor.magazine >= 4);
        }
        assert_eq!((actor.magazine, actor.reserve), (5, 29));
    }

}
