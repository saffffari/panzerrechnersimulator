//! Embodied hatch input: a request names the operator's physical post, never the host camera.
//! Roof targets only articulate the authored lids. They do not stand a driver up or expose him.
use super::{Game, Mode, Station, TankInput};
use crate::lan::{CrewFrame, LanRole, F_DISMOUNTED, F_HATCH_TOGGLE};

const CUPOLA: u8 = 1;
const DRIVER: u8 = 2;
const LOADER: u8 = 4;

fn post_mask(post: LanRole) -> u8 {
    match post {
        LanRole::Commander => CUPOLA,
        LanRole::Driver => DRIVER,
        LanRole::Loader => LOADER,
        LanRole::Gunner | LanRole::Radio => 0,
    }
}

fn station_mask(station: Station) -> u8 {
    match station {
        Station::Commander => CUPOLA,
        Station::Driver => DRIVER,
        Station::Loader => LOADER,
        Station::Gunner | Station::Radioman => 0,
    }
}

/// Vacant-post cross-training is allowed. Neither copied fallback channels nor a claimed post
/// belonging to another human can operate that person's lid. Conflicting physical posts fail shut.
fn crew_requests(frame: CrewFrame) -> u8 {
    let mut requests = 0;
    for role in LanRole::ALL {
        let input = frame.input(role);
        if !frame.occupied(role) || !input.flag(F_HATCH_TOGGLE) || input.flag(F_DISMOUNTED) {
            continue;
        }
        let peer = frame.roster[role as usize];
        if frame.occupied(input.post) && frame.roster[input.post as usize] != peer {
            continue;
        }
        if LanRole::ALL.into_iter().any(|other| {
            frame.occupied(other)
                && frame.roster[other as usize] != peer
                && frame.input(other).post == input.post
                && !frame.input(other).flag(F_DISMOUNTED)
        }) {
            continue;
        }
        requests |= post_mask(input.post);
    }
    requests
}

#[derive(Default)]
pub(super) struct HatchEdges(u8);

impl HatchEdges {
    fn take(&mut self, requests: u8, allowed: bool) -> u8 {
        let rising = requests & !self.0;
        // Consume blocked presses too: finishing a climb/repair must not operate a held control.
        self.0 = requests;
        if allowed {
            rising
        } else {
            0
        }
    }
}

impl Game {
    pub(super) fn update_station_hatches(&mut self, input: &TankInput) {
        let (requests, authority_ready) = if let Some(frame) = input.coop {
            (crew_requests(frame), input.coop_host)
        } else {
            (
                if input.hatch_toggle {
                    station_mask(self.station)
                } else {
                    0
                },
                self.switch_t <= 0.0 && self.mode == Mode::Tank && !self.climb.active,
            )
        };
        let edges = self.hatch_edges.take(
            requests,
            authority_ready && self.player.alive && self.activity.is_none(),
        );
        if edges & CUPOLA != 0 {
            self.toggle_cupola_lid();
        }
        for (index, bit) in [DRIVER, LOADER].into_iter().enumerate() {
            if edges & bit != 0 {
                self.roof_hatches.open[index] = !self.roof_hatches.open[index];
                println!(
                    "[panzerrechnersystem] {} roof hatch {}",
                    if index == 0 { "driver" } else { "loader" },
                    if self.roof_hatches.open[index] {
                        "OPEN"
                    } else {
                        "CLOSED"
                    }
                );
            }
        }
    }

    /// The physical cupola lever remains reachable from the solo Loader's walk. In co-op the
    /// local operator must occupy Commander, so E cannot silently address a different player's lid.
    pub(super) fn local_can_work_cupola(&self) -> bool {
        self.mode == Mode::Tank
            && self.player.alive
            && !self.climb.active
            && self.station_ready(self.station)
            && if self.lan_local_role.is_some() || self.couch_crew_active {
                self.station == Station::Commander
                    && self.lan_post_mask & (1 << LanRole::Commander as u8) == 0
            } else {
                matches!(self.station, Station::Commander | Station::Loader)
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crew() -> CrewFrame {
        let mut frame = CrewFrame::default();
        for role in LanRole::ALL {
            frame.roster[role as usize] = role as u32 + 10;
            frame.inputs[role as usize].post = role;
        }
        frame
    }

    #[test]
    fn every_operator_addresses_only_their_physical_lid() {
        for (station, role, expected) in [
            (Station::Commander, LanRole::Commander, CUPOLA),
            (Station::Driver, LanRole::Driver, DRIVER),
            (Station::Loader, LanRole::Loader, LOADER),
            (Station::Gunner, LanRole::Gunner, 0),
            (Station::Radioman, LanRole::Radio, 0),
        ] {
            let mut frame = crew();
            frame.inputs[role as usize].flags = F_HATCH_TOGGLE;
            assert_eq!(crew_requests(frame), expected);
            assert_eq!(station_mask(station), expected);
        }
    }

    #[test]
    fn simultaneous_and_overlapping_presses_keep_independent_edges() {
        let mut edges = HatchEdges::default();
        assert_eq!(edges.take(DRIVER, true), DRIVER);
        assert_eq!(edges.take(DRIVER | LOADER, true), LOADER);
        assert_eq!(edges.take(DRIVER | LOADER | CUPOLA, true), CUPOLA);
        assert_eq!(edges.take(DRIVER | LOADER | CUPOLA, true), 0);
        assert_eq!(edges.take(0, true), 0);
        assert_eq!(edges.take(DRIVER | LOADER | CUPOLA, true), 7);
    }

    #[test]
    fn blocked_press_does_not_latch_for_later_climb_or_repair_completion() {
        let mut edges = HatchEdges::default();
        assert_eq!(edges.take(DRIVER, false), 0);
        assert_eq!(edges.take(DRIVER, true), 0);
        edges.take(0, true);
        assert_eq!(edges.take(DRIVER, true), DRIVER);
    }

    #[test]
    fn vacant_fallbacks_and_dismounted_players_cannot_operate_lids() {
        let mut frame = CrewFrame::default();
        frame
            .inputs
            .iter_mut()
            .for_each(|i| i.flags = F_HATCH_TOGGLE);
        assert_eq!(crew_requests(frame), 0);
        frame.roster[0] = 10;
        frame.inputs[0].flags |= F_DISMOUNTED;
        assert_eq!(crew_requests(frame), 0);
    }

    #[test]
    fn dismounted_host_does_not_remove_an_inside_peers_lid_authority() {
        let mut frame = crew();
        frame.inputs[LanRole::Commander as usize].flags = F_DISMOUNTED | F_HATCH_TOGGLE;
        frame.inputs[LanRole::Driver as usize].flags = F_HATCH_TOGGLE;
        assert_eq!(crew_requests(frame), DRIVER);
    }

    #[test]
    fn cross_training_requires_vacant_uncontested_physical_post() {
        let mut frame = crew();
        frame.inputs[LanRole::Gunner as usize].post = LanRole::Loader;
        frame.inputs[LanRole::Gunner as usize].flags = F_HATCH_TOGGLE;
        assert_eq!(crew_requests(frame), 0); // another human owns Loader
        frame.roster[LanRole::Loader as usize] = CrewFrame::default().roster[0];
        assert_eq!(crew_requests(frame), LOADER);
        frame.inputs[LanRole::Commander as usize].post = LanRole::Loader;
        assert_eq!(crew_requests(frame), 0); // conflicting physical occupant
    }
}
