//! Tank hits voiced by the owner's recordings (`audio::hit_recordings`): every penetration,
//! knockout and destruction of a tank takes a long close explosion, every non-penetrating strike a
//! short one high-passed by how hard it struck. World hits go through the ordinary propagation
//! queue at the event's own calibrated emission; a strike on the listener's own Tiger is placed at
//! its calibrated level at the ear (SL §5) through the hull and the cabin's room. Each helper
//! returns false where the recordings are not installed, and the caller keeps its old voice.
use super::*;
use crate::audio::HitFamily;

/// The family for a resolved impact on a tank: penetration, or a non-penetrating strike with
/// its hardness.
pub(super) fn hit_family(event: &crate::ballistics::ImpactEvent, penetrated: bool) -> HitFamily {
    if penetrated { HitFamily::Penetration }
    else { HitFamily::Stopped { hardness: crate::audio::hit_hardness_of(event) } }
}

/// The level at the ear of a strike on the own hull: the calibrated strike level
/// (`own_armour_strike_ear_level`, SL §5, which already falls with the round's energy and
/// obliquity and the station's distance) less the voice's filter loss, through the headset.
/// The struck plate is itself the radiator into the cabin, so the airborne insertion loss that
/// exterior sound takes crossing the plates (`Enclosure::insertion_loss_bands`) is not applied
/// to it; the cabin's room is (`play_own_hull_hit_recording`). Named approximation: the plate's
/// structure-borne radiation is stood in for by the recording's own spectrum.
pub(super) fn own_hull_hit_ear(mut ear: crate::sound_propagation::EarLevel, level_shift_db: f32)
    -> crate::sound_propagation::EarLevel
{
    ear.laf_db += level_shift_db;
    ear.lpk_db += level_shift_db;
    ear
}

impl Game {
    /// Schedule a tank hit heard in the world with its recording at `sfx`'s emission (shifted by
    /// the voice's filter loss). False if the recordings are not installed.
    pub(super) fn schedule_hit_recording(&mut self, family: HitFamily, sfx: crate::audio::Sfx,
        src: Vec3, seed: u32, path: &'static str) -> bool
    {
        let Some(voice) = self.audio.as_ref().and_then(|a| a.hit_voice(family, seed)) else { return false };
        log_hit_voice(path, family, &voice, src.distance(self.threat_center()));
        let emission = sound_emission_for(sfx).shifted(voice.level_shift_db);
        let stereo = voice.stereo();
        self.schedule_emission(sfx, src, emission, None, Some(stereo));
        true
    }

    /// A strike on the listener's own Tiger with its recording, at the ear now, through the
    /// hull's transfer and the cabin's traced room (the parametric steel interior until traced),
    /// on the impact bus, its impulse handed to the ears. False if the recordings are not
    /// installed.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn play_own_hull_hit_recording(&mut self, family: HitFamily, seed: u32,
        event: &crate::ballistics::ImpactEvent, penetrated: bool, internal_burst: bool,
        station_distance: f32, bearing: f32, impact_pos: Vec3) -> bool
    {
        let Some(voice) = self.audio.as_ref().and_then(|a| a.hit_voice(family, seed)) else { return false };
        log_hit_voice("own_hull", family, &voice, station_distance);
        let enclosure = self.listener_enclosure();
        let ear = own_hull_hit_ear(
            self.own_armour_strike_ear_level(event, penetrated, internal_burst, station_distance),
            voice.level_shift_db);
        let traced = self.acoustic_room_for(impact_pos);
        let inside = matches!(enclosure, crate::sound_propagation::Enclosure::Buttoned
            | crate::sound_propagation::Enclosure::HatchOpen);
        let room = match &traced {
            Some(ir) => crate::audio::Room::Traced(ir),
            None => crate::audio::Room::Parametric(if inside { crate::acoustics::ReverbParams::STEEL_INTERIOR }
                else { crate::acoustics::ReverbParams::DIRECT }),
        };
        let Some(stereo) = self.audio.as_ref().map(|a| a.render_world_mono_for_ear(&voice.mono, &ear, bearing, room))
        else { return false };
        self.play_stereo_and_hear_at_ear(stereo, &ear, bearing, crate::audio::EarBus::Impact);
        true
    }
}

fn log_hit_voice(path: &str, family: HitFamily, voice: &crate::audio::HitVoice, distance_m: f32) {
    let (kind, hardness) = match family {
        HitFamily::Penetration => ("penetration", None),
        HitFamily::Stopped { hardness } => ("nonpenetration", Some(hardness)),
    };
    println!("[hit-sound] path={path} family={kind} variant={} file={} hardness={} cutoff_hz={} level_shift_db={:.1} distance_m={distance_m:.1}",
        voice.variant, voice.file,
        hardness.map_or("-".into(), |h| format!("{h:.2}")),
        voice.cutoff_hz.map_or("-".into(), |c| format!("{c:.0}")),
        voice.level_shift_db);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_own_hull_hit_keeps_its_calibrated_level_less_the_filter_and_its_headset_transfer() {
        let ear = crate::sound_propagation::at_ear(150.0, 130.0, crate::sound_levels::SpectrumClass::LowFrequencyBlast, 5.0, true);
        let heard = own_hull_hit_ear(ear, -4.0);
        assert_eq!(heard.laf_db, ear.laf_db - 4.0);
        assert_eq!(heard.lpk_db, ear.lpk_db - 4.0);
        assert_eq!(heard.transfer_db, ear.transfer_db, "no airborne plate loss on the struck plate's own sound");
    }

    #[test]
    fn a_harder_strike_takes_a_lower_corner() {
        let plate = crate::ballistics::Plate { name: "test", thickness_mm: 80.0, slope_deg: 0.0,
            material: crate::ballistics::ArmorMaterial::Rha };
        let mut glancing = crate::ballistics::resolve(&crate::ballistics::BR_350B, 1500.0, plate, 70.0, 1);
        let mut square = crate::ballistics::resolve(&crate::ballistics::BR_350B, 300.0, plate, 0.0, 1);
        glancing.terminal = crate::ballistics::Terminal::Ricochet;
        square.terminal = crate::ballistics::Terminal::Bounce;
        let (HitFamily::Stopped { hardness: light }, HitFamily::Stopped { hardness: hard }) =
            (hit_family(&glancing, false), hit_family(&square, false)) else { panic!("non-penetrating") };
        assert!(hard > light + 0.2, "square {hard} vs glancing {light}");
        assert!(crate::audio::hit_hardness_of(&square) == hard);
        assert_eq!(hit_family(&square, true), HitFamily::Penetration);
    }
}
