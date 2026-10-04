//! Pure incoming-contact layer clock and conservative sample metadata admission.
//! No proximity, hidden enemy state, decoder, audio device or finite-cue looping here.
use crate::sound_library::{Cue, TempoCompatibility};
#[derive(Clone, Debug, Default)]
pub struct Layers {
    pub episode: u64,
    age: f32,
    quiet: f32,
    engaged: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gains {
    pub texture: f32,
    pub rhythm: f32,
    pub anchor_held: bool,
}
impl Layers {
    pub fn update(&mut self, dt: f32, incoming: bool) -> Gains {
        if !dt.is_finite() || dt <= 0.0 {
            return self.gains();
        }
        if incoming {
            if !self.engaged {
                self.episode = self.episode.wrapping_add(1);
                self.age = 0.0;
            }
            self.engaged = true;
            self.quiet = 0.0;
        }
        if self.engaged {
            self.age += dt;
            if !incoming {
                self.quiet += dt;
            }
        }
        let gains = self.gains();
        if self.engaged && self.age >= 180.0 && self.quiet >= 180.0 {
            self.engaged = false;
        }
        gains
    }
    pub fn gains(&self) -> Gains {
        if !self.engaged {
            return Gains::default();
        }
        let release = (self.quiet - 90.0).max(0.0);
        let rise = (self.age / 8.0).clamp(0.0, 1.0);
        Gains {
            texture: rise * (1.0 - release / 60.0).clamp(0.0, 1.0),
            rhythm: rise * (1.0 - release / 90.0).clamp(0.0, 1.0),
            anchor_held: release < 90.0,
        }
    }
}
pub fn explicit_percussion(c: &Cue) -> bool {
    // The authored filename, not the hashed playback clip, carries the declaration.
    let name = c.label();
    c.looping
        && c.musical_key.is_none()
        && (name.contains("drum")
            || name.contains("_drm")
            || name.contains("_perc_")
            || name.contains("corruptperc"))
}
pub fn compatible(anchor: &Cue, layer: &Cue) -> bool {
    if !anchor.looping || !layer.looping || anchor.id == layer.id {
        return false;
    }
    if anchor.musically_compatible(layer) {
        return true;
    }
    // Explicit unpitched percussion has no tonal requirement, but known anchor pulse
    // still must match/half/double BPM. No pitch or time-stretch is manufactured.
    if explicit_percussion(layer) {
        let name = anchor.label();
        let declared_atmosphere = anchor.bpm.is_none()
            && [
                "ambiance",
                "ambient",
                "atmos",
                "storm",
                "foggy",
                "rainfall",
                "trickling",
            ]
            .iter()
            .any(|tag| name.contains(tag));
        if declared_atmosphere {
            return true;
        }
        return matches!(
            anchor.compatibility(layer).tempo,
            TempoCompatibility::Same
                | TempoCompatibility::HalfTime
                | TempoCompatibility::DoubleTime
        );
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_short_incoming_contact_holds_then_drums_peel_last() {
        let mut p = Layers::default();
        p.update(1.0, true);
        let hold = p.update(89.0, false);
        assert!(hold.texture > 0.99 && hold.rhythm > 0.99);
        let middle = p.update(61.0, false);
        assert_eq!(middle.texture, 0.0);
        assert!(middle.rhythm > 0.3);
        assert!(middle.anchor_held);
        let end = p.update(30.0, false);
        assert_eq!(end.rhythm, 0.0);
        assert!(!end.anchor_held);
    }
    #[test]
    fn proximity_cannot_start_any_layer() {
        let mut p = Layers::default();
        for _ in 0..600 {
            assert_eq!(p.update(1.0, false), Gains::default());
        }
    }
    #[test]
    fn incoming_during_release_recovers_same_episode() {
        let mut p = Layers::default();
        p.update(1.0, true);
        p.update(130.0, false);
        let episode = p.episode;
        let again = p.update(1.0, true);
        assert_eq!(p.episode, episode);
        assert_eq!(again.rhythm, 1.0);
    }
}
