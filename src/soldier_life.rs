//! A human casualty and a disabled vehicle are separate events. The legacy front end still
//! ends its attempt when the controlled man dies; keeping the machine intact makes later
//! expedition succession possible without resurrecting a tank or resetting its damage.
use super::*;

impl Game {
    /// Integration harness staging only; does not claim a shell or create an artificial wound.
    pub fn debug_controlled_soldier_loss(&mut self) {
        self.lose_controlled_soldier(DeathCause::CrewLost);
    }

    pub fn controlled_soldier_alive(&self) -> bool {
        if let Some(role) = self.singleplayer_embodied_role() {
            singleplayer_crew::role_alive(&self.crew, role)
                && !self.crew_solo.man(role).dead && self.foot.hp > 0.0
        } else {
            match self.mode {
                Mode::Tank => self.player.alive,
                Mode::Foot => self.foot.hp > 0.0,
            }
        }
    }

    pub(super) fn lose_controlled_soldier(&mut self, cause: DeathCause) {
        self.retain_controlled_corpse();
        if let Some(role) = self.singleplayer_embodied_role() {
            // Record the same actor in the existing roster and wound model. Never touch
            // another person's state, the engine switch, vehicle damage or ammunition.
            if let Some(person) = self.crew.iter_mut().find(|person| person.role == role) {
                person.ok = false;
            }
            let man = self.crew_solo.man_mut(role);
            man.dead = true;
            if man.cause.is_none() { man.cause = Some("killed"); }
            self.crew_ko_t = CREW_KO_DUR;
        } else {
            // Legacy uncrewed/couch modes retain their established terminal behavior.
            self.player.alive = false;
            self.cue_engine_death(0.7);
        }
        self.death_cause = Some(cause);
        self.death_replay = None;
        self.recompute_hp();
    }
}
