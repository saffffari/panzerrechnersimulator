//! Fictional memorial identity and terminal-clock reveal and audio-completion presentation for the scored death sequence.
//! This module describes presentation only; it cannot change a campaign death or retry.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub first_name: &'static str,
    pub surname: &'static str,
    pub birth_year: i32,
    pub death_year: i32,
}

/// A generated fictional character, never an attribution to a historical tanker.
/// A local hash leaves simulation, audio-selection and campaign random streams untouched.
pub fn identity(seed: u64, death_year: i32) -> Identity {
    const FIRST: &[&str] = &[
        "Anton",
        "Emil",
        "Erich",
        "Felix",
        "Friedrich",
        "Georg",
        "Heinrich",
        "Karl",
        "Kurt",
        "Ludwig",
        "Otto",
        "Paul",
        "Walter",
        "Wilhelm",
        "Albert",
        "Franz",
    ];
    const LAST: &[&str] = &[
        "Bergmann",
        "Brenner",
        "Eckert",
        "Falk",
        "Fischer",
        "Hartmann",
        "Keller",
        "Krause",
        "Lindner",
        "Mertens",
        "Neumann",
        "Reuter",
        "Scholz",
        "Sommer",
        "Vogt",
        "Wagner",
        "Winter",
        "Wolff",
        "Zimmermann",
    ];
    let a = mix(seed ^ 0x4445_4154_4843_4152);
    let b = mix(a);
    let age = 18 + (mix(b) % 28) as i32;
    Identity {
        first_name: FIRST[(a % FIRST.len() as u64) as usize],
        surname: LAST[(b % LAST.len() as u64) as usize],
        birth_year: death_year.saturating_sub(age),
        death_year,
    }
}

fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Black,
    Identity,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Presentation {
    pub identity: Identity,
    duration_s: f32,
    /// A silent card on a fixed clock: (seconds after the fatal frame at which the card appears,
    /// seconds it is held). `None` is the older scored card that follows its audio cue.
    schedule: Option<(f32, f32)>,
}

impl Presentation {
    pub fn new(seed: u64, death_year: i32, duration_s: f32) -> Option<Self> {
        (duration_s.is_finite() && duration_s > 0.0).then(|| Self {
            identity: identity(seed, death_year),
            duration_s,
            schedule: None,
        })
    }

    /// The card with no music: black until `reveal_at_s`, the card for `hold_s`, then finished.
    pub fn silent(seed: u64, death_year: i32, reveal_at_s: f32, hold_s: f32) -> Self {
        Self { identity: identity(seed, death_year), duration_s: hold_s.max(0.0),
            schedule: Some((reveal_at_s.max(0.0), hold_s.max(0.0))) }
    }
    pub fn is_silent(self) -> bool { self.schedule.is_some() }
    /// Seconds after the fatal frame at which a silent card is put away.
    pub fn card_end_s(self) -> Option<f32> { self.schedule.map(|(reveal, hold)| reveal + hold) }

    /// Preserve the former midpoint delay relative to fatal entry, even though the cue
    /// now begins at text reveal. The stopped audio clock cannot drive this deadline.
    pub fn reveal_at_s(self) -> f32 { self.schedule.map_or(self.duration_s * 0.5, |(reveal, _)| reveal) }

    pub fn phase(self, terminal_elapsed_s: f32, audio_elapsed_s: Option<f32>, audio_finished: bool) -> Phase {
        if let Some((reveal, hold)) = self.schedule {
            if !terminal_elapsed_s.is_finite() || terminal_elapsed_s < reveal { return Phase::Black; }
            return if terminal_elapsed_s < reveal + hold { Phase::Identity } else { Phase::Finished };
        }
        let started = audio_elapsed_s.is_some_and(|t| t.is_finite() && t >= 0.0);
        if started && audio_finished {
            Phase::Finished
        } else if started || (terminal_elapsed_s.is_finite() && terminal_elapsed_s >= self.reveal_at_s()) {
            Phase::Identity
        } else {
            Phase::Black
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fictional_identity_is_stable_and_has_plausible_adult_years() {
        let mut names = std::collections::BTreeSet::new();
        for year in 1942..=1945 {
            for seed in 0..200 {
                let person = identity(seed, year);
                assert_eq!(person, identity(seed, year));
                assert_eq!(person.death_year, year);
                assert!((18..=45).contains(&(year - person.birth_year)));
                assert!(person.first_name.is_ascii() && person.surname.is_ascii());
                names.insert((person.first_name, person.surname));
            }
        }
        assert!(
            names.len() > 80,
            "identities vary without consuming game randomness"
        );
        let earlier = identity(42, 1943);
        let later = identity(42, 1944);
        assert_eq!(earlier.first_name, later.first_name);
        assert_eq!(earlier.surname, later.surname);
        assert_eq!(earlier.birth_year + 1, later.birth_year);
    }

    #[test]
    fn reveal_preserves_midpoint_delay_without_waiting_for_stopped_audio() {
        let card = Presentation::new(3, 1943, 36.0).unwrap();
        assert_eq!(card.reveal_at_s(), 18.0);
        assert_eq!(card.phase(0.0, None, true), Phase::Black);
        assert_eq!(card.phase(17.999, None, false), Phase::Black);
        assert_eq!(card.phase(18.0, None, false), Phase::Identity);
        assert_eq!(card.phase(300.0, None, true), Phase::Identity,
            "a missing first presented frame cannot complete an unstarted cue");
        assert_eq!(card.phase(18.0, Some(0.0), false), Phase::Identity);
        assert_eq!(card.phase(54.0, Some(36.0), false), Phase::Identity,
            "the entire source and buffered output must finish before release");
        assert_eq!(card.phase(54.1, Some(36.0), true), Phase::Finished);
    }

    #[test]
    fn silent_card_runs_on_its_own_clock_with_no_audio() {
        let card = Presentation::silent(3, 1943, 3.5, 5.0);
        assert!(card.is_silent());
        assert_eq!(card.phase(0.0, None, false), Phase::Black);
        assert_eq!(card.phase(3.49, None, false), Phase::Black);
        assert_eq!(card.phase(3.5, None, false), Phase::Identity);
        assert_eq!(card.phase(8.49, Some(1.0), true), Phase::Identity, "audio state is ignored");
        assert_eq!(card.phase(8.5, None, false), Phase::Finished);
        assert_eq!(card.phase(f32::NAN, None, false), Phase::Black);
    }

    #[test]
    fn paused_or_missing_playback_cannot_skip_the_full_chord() {
        let card = Presentation::new(8, 1944, 4.0).unwrap();
        for _ in 0..100 {
            assert_eq!(card.phase(1.0, None, false), Phase::Black);
            assert_eq!(card.phase(100.0, Some(1.0), false), Phase::Identity);
        }
        for elapsed in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
            assert_eq!(card.phase(elapsed, None, false), Phase::Black);
        }
        for duration in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(Presentation::new(0, 1943, duration).is_none());
        }
    }
}
