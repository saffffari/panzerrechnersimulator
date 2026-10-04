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
/// Seconds of a contact episode the percussion layer plays at full level, and its fade after.
pub const RHYTHM_SWELL_HOLD_S: f32 = 20.0;
pub const RHYTHM_SWELL_FADE_S: f32 = 15.0;
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
        // Percussion is a swell at the start of a contact episode, not a bed. The library's
        // percussion tops are four-second loops; held for the whole episode (which sustained
        // fire never lets end) one of them read as a metal clinking that would not stop.
        let swell = (1.0 - (self.age - RHYTHM_SWELL_HOLD_S) / RHYTHM_SWELL_FADE_S).clamp(0.0, 1.0);
        Gains {
            texture: rise * (1.0 - release / 60.0).clamp(0.0, 1.0),
            rhythm: rise * swell * (1.0 - release / 90.0).clamp(0.0, 1.0),
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
    fn one_short_incoming_contact_swells_drums_then_holds_texture() {
        let mut p = Layers::default();
        p.update(1.0, true);
        let swell = p.update(14.0, false);
        assert!(swell.texture > 0.99 && swell.rhythm > 0.99);
        // The percussion is gone well inside the episode; the texture and anchor hold on.
        let hold = p.update(75.0, false);
        assert!(hold.texture > 0.99 && hold.anchor_held);
        assert_eq!(hold.rhythm, 0.0);
        let middle = p.update(61.0, false);
        assert_eq!(middle.texture, 0.0);
        assert!(middle.anchor_held);
        let end = p.update(30.0, false);
        assert!(!end.anchor_held);
    }
    #[test]
    fn sustained_fire_cannot_keep_the_percussion_loop_going() {
        let mut p = Layers::default();
        let mut audible = 0.0;
        // Ten minutes with a hostile round every eight seconds: one unbroken episode.
        for second in 0..600 {
            if p.update(1.0, second % 8 == 0).rhythm > 0.0 { audible += 1.0; }
        }
        assert_eq!(p.episode, 1);
        assert!(audible <= RHYTHM_SWELL_HOLD_S + RHYTHM_SWELL_FADE_S, "percussion audible for {audible} s");
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
        // Same episode: the texture recovers at once, the spent percussion swell does not return.
        assert_eq!(again.texture, 1.0);
        assert_eq!(again.rhythm, 0.0);
    }
}
