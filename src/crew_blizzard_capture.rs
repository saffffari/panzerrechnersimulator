//! The blizzard recorder's real crew viewpoints and automated co-op station inputs.
use crate::{
    game::{Game, TankInput},
    lan,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    External,
    Cupola,
    Inside,
}

impl View {
    pub fn label(self) -> &'static str {
        match self {
            Self::External => "external",
            Self::Cupola => "cupola",
            Self::Inside => "inside",
        }
    }

    pub fn is_crew(self) -> bool {
        self != Self::External
    }

    pub fn listener_alive(self, game: &Game) -> bool {
        let peer = match self {
            Self::Cupola => 1,
            Self::Inside => 4,
            Self::External => return true,
        };
        game.player.alive && !game.couch_peer_kia(peer)
    }

    pub fn stage(self, game: &mut Game) {
        if self.is_crew() {
            game.stage_capture_crew_view(self == Self::Cupola);
            game.begin_audio_capture();
        }
    }

    /// The listener stays in the photographed post. Separate occupied station channels operate
    /// the gun and breech through their ordinary authority and casualty gates.
    pub fn controls(self, game: &mut Game, tick: u32, fire_ready: bool) -> TankInput {
        self.controls_for_lay(game, tick, fire_ready, true)
    }

    /// Winter motion review uses a desired lay, leaving actual pitch/traverse to the ordinary
    /// rate-limited gun drives and their damage/crew gates. Older direct-lay films retain theirs.
    pub fn controls_with_slew(self, game: &mut Game, tick: u32, fire_ready: bool) -> TankInput {
        self.controls_for_lay(game, tick, fire_ready, false)
    }

    fn controls_for_lay(self, game: &mut Game, tick: u32, fire_ready: bool, direct: bool) -> TankInput {
        let mut crew = lan::CrewFrame::default();
        for (i, role) in [
            lan::LanRole::Commander,
            lan::LanRole::Gunner,
            lan::LanRole::Driver,
            lan::LanRole::Loader,
        ]
        .into_iter()
        .enumerate()
        {
            crew.roster[role as usize] = i as u32 + 1;
            crew.inputs[role as usize].post = role;
        }
        if game.player.alive {
            let laid = if direct {
                game.aim_at_enemy();
                true
            } else {
                // Keep one observed actor while it lives. Re-selecting the nearest actor every
                // frame made the demonstrator continually abandon its lay as tanks crossed.
                let target=game.enemies.iter().enumerate()
                    .find(|(i,e)|e.alive&&game.capture_enemy_active(*i)).map(|(i,_)|i);
                target.and_then(|i|game.capture_gaze_target(&serde_json::json!({"gaze":"enemy","enemy":i})))
                    .is_some_and(|target| game.campaign_ai_order_lay(target, 0.012))
            };
            crew.inputs[lan::LanRole::Gunner as usize].flags = if fire_ready && laid && game.auto_can_fire()
            {
                lan::F_FIRE_MAIN
            } else {
                0
            };
            if !game.breech_loaded() {
                crew.inputs[lan::LanRole::Loader as usize].flags = lan::F_SERVICE_BREECH;
                crew.inputs[lan::LanRole::Loader as usize].look_dx =
                    if tick % 2 == 0 { 28.0 } else { -28.0 };
                // The local inside camera is the loader, so its ordinary manual station owns
                // service rather than its remote role channel. Use the established auto-crew
                // state-machine helper here, still subject to the real station/breech gates.
                if self == Self::Inside && self.listener_alive(game) {
                    game.auto_service_breech(tick % 2 == 0, 1.0 / 60.0);
                }
            }
        }
        TankInput {
            coop: Some(crew),
            coop_host: true,
            couch_crew: true,
            extinguish: game.is_fire_active(),
            ..Default::default()
        }
    }
}
