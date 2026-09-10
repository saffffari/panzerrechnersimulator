//! Transient feedback from completed simulation events, shared by tactile output devices.
//! Input presses, historical counters and nearby enemy sounds are not haptic events.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HapticCue {
    MainGun,
    OwnTankHit { penetrated: bool },
    MainGunMisfire,
    RoundLoaded,
}

/// Compare immediately before/after one live simulation update in the same world. Capture the
/// baseline from the game; a default/old-world snapshot must never stand in for that baseline.
/// Counter deltas collapse to one tactile cue, avoiding a burst of delayed historical feedback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HapticSnapshot {
    pub shots: u32,
    pub hits: u32,
    pub penetrations: u32,
    pub misfires: u32,
    pub loaded: bool,
}

fn advanced(before: u32, after: u32) -> bool {
    // Actual counters may wrap; resets and implausibly large catch-up jumps are discontinuities.
    matches!(after.wrapping_sub(before), 1..=64)
}

impl HapticSnapshot {
    pub fn cues_since(self, before: Self) -> Vec<HapticCue> {
        let mut cues = Vec::new();
        if advanced(before.shots, self.shots) {
            cues.push(HapticCue::MainGun);
        }
        if advanced(before.hits, self.hits) {
            cues.push(HapticCue::OwnTankHit {
                penetrated: advanced(before.penetrations, self.penetrations),
            });
        }
        if advanced(before.misfires, self.misfires) {
            cues.push(HapticCue::MainGunMisfire);
        }
        if !before.loaded && self.loaded {
            cues.push(HapticCue::RoundLoaded);
        }
        cues
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baselining_existing_world_does_not_replay_history_or_initial_loaded_round() {
        let snapshot = HapticSnapshot {
            shots: 7,
            hits: 9,
            penetrations: 2,
            misfires: 3,
            loaded: true,
        };
        assert!(snapshot.cues_since(snapshot).is_empty());
    }

    #[test]
    fn completed_simulation_events_emit_distinct_cues_and_collapse_catch_up_counts() {
        let before = HapticSnapshot::default();
        let after = HapticSnapshot {
            shots: 2,
            hits: 3,
            penetrations: 1,
            misfires: 1,
            loaded: true,
        };
        assert_eq!(
            after.cues_since(before),
            vec![
                HapticCue::MainGun,
                HapticCue::OwnTankHit { penetrated: true },
                HapticCue::MainGunMisfire,
                HapticCue::RoundLoaded,
            ]
        );
        let bounce = HapticSnapshot { hits: 1, ..before };
        assert_eq!(
            bounce.cues_since(before),
            vec![HapticCue::OwnTankHit { penetrated: false }]
        );
    }

    #[test]
    fn counter_wrap_is_a_new_event_but_resets_and_large_discontinuities_are_not() {
        let before = HapticSnapshot {
            shots: u32::MAX,
            misfires: 9,
            ..Default::default()
        };
        let after = HapticSnapshot {
            shots: 0,
            misfires: 0,
            ..before
        };
        assert_eq!(after.cues_since(before), vec![HapticCue::MainGun]);
        let jumped = HapticSnapshot {
            shots: 200,
            misfires: 200,
            ..after
        };
        assert!(jumped.cues_since(after).is_empty());
    }

    #[test]
    fn loaded_feedback_requires_a_newly_seated_round_and_penetration_requires_a_hit() {
        let loaded = HapticSnapshot {
            loaded: true,
            ..Default::default()
        };
        let empty = HapticSnapshot {
            loaded: false,
            ..loaded
        };
        assert!(empty.cues_since(loaded).is_empty());
        assert_eq!(loaded.cues_since(empty), vec![HapticCue::RoundLoaded]);
        let inconsistent = HapticSnapshot {
            penetrations: 1,
            ..loaded
        };
        assert!(inconsistent.cues_since(loaded).is_empty());
    }
}
