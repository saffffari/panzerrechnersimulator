//! The player's own body catching fire. A game rule set, not a burn-injury model: the numbers
//! below are authored for play and are named as such.
//!
//! Standing in a cabin fire for a moment sets the clothing alight. Once alight the fire feeds
//! itself and hurts every second until it is beaten out. Space pats it down: slowly from a seat
//! inside the tank, much faster outside by dropping to the ground, in the open and exposed.
//! While alight the body cannot jump or climb.

/// Fire intensity at the body (0..1) above which clothing starts to catch.
pub const CATCH_EXPOSURE: f32 = 0.30;
/// Seconds of continuous exposure before the clothing catches.
pub const CATCH_SECONDS: f32 = 1.5;
/// Flame level right after catching.
pub const CATCH_HEAT: f32 = 0.35;
/// How fast unattended flames grow per second with no outside fire feeding them.
pub const SELF_GROWTH_PER_S: f32 = 0.06;
/// Extra growth per second at full exposure to a surrounding fire.
pub const FED_GROWTH_PER_S: f32 = 0.30;
/// Body damage per second at full flame, in the same units as `foot::FOOT_HP`.
pub const DAMAGE_PER_S: f32 = 16.0;
/// Flame removed by one pat from a seat, and by one roll on the ground outside.
pub const PAT_SEATED: f32 = 0.11;
pub const PAT_PRONE: f32 = 0.34;
/// A roll keeps the body on the ground this long.
pub const PRONE_HOLD_S: f32 = 0.7;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerBurn {
    /// Flames on the body, 0 (none) to 1.
    pub heat: f32,
    catching_s: f32,
    prone_s: f32,
    /// Total pats this fire, for telemetry and tests.
    pub pats: u32,
}

impl PlayerBurn {
    pub fn burning(&self) -> bool { self.heat > 0.0 }
    /// The body is held on the ground by a recent roll.
    pub fn prone(&self) -> bool { self.prone_s > 0.0 }

    /// Advance one step. `exposure` is the surrounding fire's intensity at the body (0..1).
    /// Returns the body damage dealt this step.
    pub fn update(&mut self, dt: f32, exposure: f32) -> f32 {
        if !dt.is_finite() || dt <= 0.0 { return 0.0; }
        let exposure = if exposure.is_finite() { exposure.clamp(0.0, 1.0) } else { 0.0 };
        self.prone_s = (self.prone_s - dt).max(0.0);
        if !self.burning() {
            if exposure >= CATCH_EXPOSURE {
                self.catching_s += dt;
                if self.catching_s >= CATCH_SECONDS {
                    self.heat = CATCH_HEAT;
                    self.catching_s = 0.0;
                    self.pats = 0;
                }
            } else {
                self.catching_s = (self.catching_s - dt).max(0.0);
            }
            return 0.0;
        }
        self.heat = (self.heat + (SELF_GROWTH_PER_S + FED_GROWTH_PER_S * exposure) * dt).min(1.0);
        self.heat * DAMAGE_PER_S * dt
    }

    /// One press of Space. Outside the tank the body drops and rolls, which is far quicker.
    pub fn pat(&mut self, outside: bool) {
        if !self.burning() { return; }
        self.pats += 1;
        self.heat -= if outside { PAT_PRONE } else { PAT_SEATED };
        if outside { self.prone_s = PRONE_HOLD_S; }
        if self.heat <= 0.02 {
            self.heat = 0.0;
            self.catching_s = 0.0;
        }
    }

    pub fn extinguish(&mut self) { *self = Self::default(); }

    /// The small centre-bottom prompt, or nothing when the body is not alight.
    pub fn prompt(&self, outside: bool) -> Option<&'static str> {
        self.burning().then_some(if outside { "ON FIRE - HIT SPACE TO DROP AND ROLL" }
            else { "ON FIRE - PRESS SPACE TO PAT DOWN" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(burn: &mut PlayerBurn, seconds: f32, exposure: f32) -> f32 {
        let mut damage = 0.0;
        for _ in 0..(seconds * 60.0).round() as usize { damage += burn.update(1.0 / 60.0, exposure); }
        damage
    }

    #[test]
    fn a_brief_brush_with_fire_does_not_catch_but_standing_in_it_does() {
        let mut burn = PlayerBurn::default();
        run(&mut burn, CATCH_SECONDS * 0.6, 1.0);
        assert!(!burn.burning());
        run(&mut burn, 5.0, 0.0);
        run(&mut burn, CATCH_SECONDS * 0.6, 1.0);
        assert!(!burn.burning(), "exposure must be continuous, not accumulated across a gap");
        run(&mut burn, CATCH_SECONDS, 1.0);
        assert!(burn.burning());
        // Weak fire below the threshold never catches.
        let mut cool = PlayerBurn::default();
        run(&mut cool, 60.0, CATCH_EXPOSURE * 0.9);
        assert!(!cool.burning());
    }

    #[test]
    fn burning_keeps_hurting_after_leaving_the_fire_until_patted_out() {
        let mut burn = PlayerBurn::default();
        run(&mut burn, CATCH_SECONDS + 0.1, 1.0);
        let away = run(&mut burn, 3.0, 0.0);
        assert!(burn.burning() && away > 10.0, "damage away from the fire {away}");
        let mut pats = 0;
        while burn.burning() && pats < 50 { burn.pat(false); pats += 1; }
        assert!(!burn.burning() && pats >= 4);
        assert_eq!(run(&mut burn, 2.0, 0.0), 0.0);
    }

    #[test]
    fn rolling_outside_is_much_faster_than_patting_inside_and_holds_the_body_down() {
        let lit = PlayerBurn { heat: 0.9, ..Default::default() };
        let (mut seated, mut prone) = (lit, lit);
        let mut seated_pats = 0;
        while seated.burning() { seated.pat(false); seated_pats += 1; }
        let mut prone_pats = 0;
        while prone.burning() { prone.pat(true); prone_pats += 1; }
        assert!(prone_pats * 2 < seated_pats, "prone {prone_pats} seated {seated_pats}");
        let mut rolling = lit;
        rolling.pat(true);
        assert!(rolling.prone());
        run(&mut rolling, PRONE_HOLD_S + 0.1, 0.0);
        assert!(!rolling.prone());
        assert!(lit.prompt(false).unwrap().contains("PAT DOWN"));
        assert!(lit.prompt(true).unwrap().contains("DROP AND ROLL"));
        assert!(PlayerBurn::default().prompt(false).is_none());
    }

    #[test]
    fn frame_rate_does_not_change_the_outcome() {
        let mut a = PlayerBurn::default();
        let mut b = PlayerBurn::default();
        let mut da = 0.0;
        let mut db = 0.0;
        for _ in 0..300 { da += a.update(1.0 / 30.0, 0.8); }
        for _ in 0..1200 { db += b.update(1.0 / 120.0, 0.8); }
        assert!((a.heat - b.heat).abs() < 0.01 && (da - db).abs() < da * 0.03, "{da} {db}");
    }
}
