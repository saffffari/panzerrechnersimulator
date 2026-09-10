//! Authored gameplay consequence of sustained fire reaching a live round.
//! The exposure input is not a temperature. Once the round ignites, cooling nearby surfaces
//! does not reset its fuse; removing that exact round from the live inventory removes the risk.
pub const IGNITION_EXPOSURE: f32 = 0.65;
pub const COOKOFF_SECONDS: f32 = 3.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct AmmunitionFire {
    burning_round: Option<u32>,
    remaining: f32,
}
impl AmmunitionFire {
    pub fn reset(&mut self) { *self = Self::default(); }
    pub fn remaining(&self) -> Option<f32> { self.burning_round.map(|_| self.remaining) }
    pub fn burning_round(&self) -> Option<u32> { self.burning_round }
    /// Move the same cartridge between inventory locations without refreshing its countdown.
    /// Call before the old location is cleared; an unrelated cartridge never inherits ignition.
    pub fn transfer(&mut self, old_location_id: u32, new_location_id: u32) -> bool {
        if self.burning_round != Some(old_location_id) { return false; }
        self.burning_round = Some(new_location_id);
        true
    }
    /// True exactly when a still-present ignited round reaches cookoff.
    pub fn advance(&mut self, dt: f32, live_ids: &[u32], exposed: Option<u32>, exposure: f32) -> bool {
        if self.burning_round.is_some_and(|id| !live_ids.contains(&id)) { self.reset(); }
        if self.burning_round.is_none() && exposure >= IGNITION_EXPOSURE {
            if let Some(id) = exposed.filter(|id| live_ids.contains(id)) {
                self.burning_round = Some(id);
                self.remaining = COOKOFF_SECONDS;
            }
        }
        if self.burning_round.is_some() {
            self.remaining = (self.remaining - dt.clamp(0.0, 0.05)).max(0.0);
            if self.remaining == 0.0 { self.reset(); return true; }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn burning_round_cooks_off_despite_extinguishing_nearby_surfaces() {
        let mut fire = AmmunitionFire::default();
        assert!(!fire.advance(0.05, &[7, 8], Some(7), 0.8));
        assert_eq!(fire.burning_round(), Some(7));
        for _ in 0..58 { assert!(!fire.advance(0.05, &[7, 8], None, 0.0)); }
        assert!(fire.remaining().unwrap() < 0.051);
        assert!(fire.advance(0.05, &[7, 8], None, 0.0) || fire.advance(0.05, &[7, 8], None, 0.0));
        assert_eq!(fire.remaining(), None);
    }
    #[test]
    fn early_suppression_prevents_ignition_and_empty_inventory_cannot_cook_off() {
        let mut fire = AmmunitionFire::default();
        for _ in 0..100 {
            assert!(!fire.advance(0.05, &[7], Some(7), 0.6));
            assert!(!fire.advance(0.05, &[], Some(7), 1.4));
        }
        assert_eq!(fire.remaining(), None);
    }
    #[test]
    fn moving_a_live_round_does_not_cancel_it_but_removing_that_round_does() {
        let mut fire = AmmunitionFire::default();
        fire.advance(0.05, &[7, 8], Some(7), 0.8);
        fire.advance(0.05, &[8, 7], None, 0.0);
        assert_eq!(fire.burning_round(), Some(7));
        fire.advance(0.05, &[8], None, 0.0);
        assert_eq!(fire.remaining(), None);
    }
    #[test]
    fn transfer_preserves_countdown_through_drop_load_and_unload() {
        let mut fire = AmmunitionFire::default();
        fire.advance(0.05, &[7], Some(7), 0.8);
        for (old, new) in [(7, 0x2_0000), (0x2_0000, 0x1_0000), (0x1_0000, 0x2_0001)] {
            let remaining = fire.remaining();
            assert!(fire.transfer(old, new));
            assert_eq!(fire.remaining(), remaining, "handling must not refresh the fuse");
            assert!(!fire.advance(0.05, &[new], None, 0.0));
            assert_eq!(fire.burning_round(), Some(new));
        }
        let mut elapsed = 0.20;
        while !fire.advance(0.05, &[0x2_0001], None, 0.0) {
            elapsed += 0.05;
            assert!(elapsed < COOKOFF_SECONDS + 0.1);
        }
        assert!(elapsed >= COOKOFF_SECONDS - 0.1);
    }
    #[test]
    fn transfer_does_not_ignite_a_replacement_or_follow_an_unrelated_round() {
        let mut fire = AmmunitionFire::default();
        fire.advance(0.05, &[7, 8], Some(7), 0.8);
        let remaining = fire.remaining();
        assert!(!fire.transfer(8, 0x1_0000));
        assert_eq!(fire.burning_round(), Some(7));
        assert_eq!(fire.remaining(), remaining);
        assert!(fire.transfer(7, 0x2_0000));
        // Refill the old rack slot with a different cartridge: the floor round still owns the fuse.
        fire.advance(0.05, &[7, 0x2_0000], None, 0.0);
        assert_eq!(fire.burning_round(), Some(0x2_0000));
        fire.advance(0.05, &[7], None, 0.0);
        assert_eq!(fire.remaining(), None);
        assert!(!fire.transfer(7, 0x1_0000));
        assert!(!fire.advance(0.05, &[0x1_0000], None, 0.0));
        assert_eq!(fire.remaining(), None);
    }
}
