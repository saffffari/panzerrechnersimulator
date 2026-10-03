//! Fictional memorial identity and audio-clock presentation for the scored death sequence.
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
}

impl Presentation {
    pub fn new(seed: u64, death_year: i32, duration_s: f32) -> Option<Self> {
        (duration_s.is_finite() && duration_s > 0.0).then(|| Self {
            identity: identity(seed, death_year),
            duration_s,
        })
    }

    /// The playback clock reveals the identity; only audio completion releases the screen.
    /// Missing clock data holds black instead of falling back to an unrelated wall-clock timer.
    pub fn phase(self, audio_elapsed_s: Option<f32>, audio_finished: bool) -> Phase {
        if audio_finished {
            Phase::Finished
        } else if audio_elapsed_s.is_some_and(|t| t.is_finite() && t >= self.duration_s * 0.5) {
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
    fn halfway_reveal_uses_audio_time_and_waits_for_actual_completion() {
        let card = Presentation::new(3, 1943, 36.0).unwrap();
        assert_eq!(card.phase(Some(0.0), false), Phase::Black);
        assert_eq!(card.phase(Some(17.999), false), Phase::Black);
        assert_eq!(card.phase(Some(18.0), false), Phase::Identity);
        assert_eq!(card.phase(Some(36.0), false), Phase::Identity);
        assert_eq!(card.phase(Some(300.0), false), Phase::Identity);
        assert_eq!(card.phase(Some(36.0), true), Phase::Finished);
        assert_eq!(card.phase(None, true), Phase::Finished);
    }

    #[test]
    fn missing_or_paused_clock_cannot_advance_the_card() {
        let card = Presentation::new(8, 1944, 4.0).unwrap();
        for _ in 0..100 {
            assert_eq!(card.phase(Some(1.0), false), Phase::Black);
            assert_eq!(card.phase(None, false), Phase::Black);
        }
        for elapsed in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
            assert_eq!(card.phase(Some(elapsed), false), Phase::Black);
        }
        for duration in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(Presentation::new(0, 1943, duration).is_none());
        }
    }
}
