//! The ignited round's latch. Whether a live round ignites is the compartment's heat model's
//! (`cabin_heat`, driven by `game_cookoff`): the case's own temperature under the hot gas, the
//! flames in sight of it and its burning neighbours (2026-11-02; this replaces an exposure fuse).
//! This keeps which round went first for the systems that ask whether the ammunition burns, and
//! follows that cartridge's identity through handling. Its saved layout is unchanged.

#[derive(Clone, Copy, Debug, Default)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct AmmunitionFire {
    burning_round: Option<u32>,
    remaining: f32,
}
impl AmmunitionFire {
    pub fn reset(&mut self) { *self = Self::default(); }
    /// `Some(0.0)` once a round has ignited (the heat model has no fuse to count).
    pub fn remaining(&self) -> Option<f32> { self.burning_round.map(|_| self.remaining) }
    pub fn burning_round(&self) -> Option<u32> { self.burning_round }
    /// The compartment's heat model has ignited this round: latch it as the burning round.
    pub fn latch(&mut self, id: u32) {
        if self.burning_round.is_none() {
            self.burning_round = Some(id);
            self.remaining = 0.0;
        }
    }
    /// Move the same cartridge between inventory locations; an unrelated cartridge never
    /// inherits the latch.
    pub fn transfer(&mut self, old_location_id: u32, new_location_id: u32) -> bool {
        if self.burning_round != Some(old_location_id) { return false; }
        self.burning_round = Some(new_location_id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_latch_holds_the_first_round_and_follows_it_through_handling() {
        let mut fire = AmmunitionFire::default();
        assert_eq!(fire.remaining(), None);
        fire.latch(7);
        fire.latch(8);
        assert_eq!(fire.burning_round(), Some(7), "the first round stays the burning one");
        assert_eq!(fire.remaining(), Some(0.0));
        assert!(!fire.transfer(8, 0x1_0000), "an unrelated round never inherits it");
        assert!(fire.transfer(7, 0x2_0000));
        assert_eq!(fire.burning_round(), Some(0x2_0000));
        fire.reset();
        assert_eq!(fire.burning_round(), None);
    }
}
