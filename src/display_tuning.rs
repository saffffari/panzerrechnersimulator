//! Optional per-document display treatment. Missing profiles preserve the renderer's defaults.
use anyhow::{ensure, Context};
use serde_json::{json, Value};

pub const KEY: &str = "display_tuning";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayTuning {
    pub width: u32,
    pub height: u32,
    pub palette_levels: f32,
    pub ordered_dither: f32,
    pub retro_strength: f32,
    pub ldr_dither: f32,
    /// Camera reproduction only: balance to the diffuse environmental illuminant.
    pub ambient_white_balance: bool,
}

impl DisplayTuning {
    pub fn at_resolution(width: u32, height: u32) -> Self {
        Self { width, height, palette_levels: 31.0, ordered_dither: 1.0,
            retro_strength: 1.0, ldr_dither: 1.0, ambient_white_balance: false }
    }

    pub fn from_edits(edits: Option<&Value>) -> anyhow::Result<Option<Self>> {
        edits.and_then(|v| v.get(KEY)).filter(|v| !v.is_null()).map(Self::from_json).transpose()
    }

    pub fn from_json(value: &Value) -> anyhow::Result<Self> {
        let width = u32::try_from(value["width"].as_u64().context("display width required")?)?;
        let height = u32::try_from(value["height"].as_u64().context("display height required")?)?;
        ensure!(super::INTERNAL_RES_PRESETS.contains(&(width,height)), "unsupported display resolution");
        let read = |key: &str, lo: f32, hi: f32| -> anyhow::Result<f32> {
            let n = value[key].as_f64().with_context(|| format!("display {key} required"))? as f32;
            ensure!(n.is_finite() && n >= lo && n <= hi, "display {key} must be {lo}..{hi}");
            Ok(n)
        };
        let palette_levels = read("palette_levels", 2.0, 255.0)?;
        ensure!(palette_levels.fract() == 0.0, "palette levels must be an integer");
        Ok(Self { width, height, palette_levels,
            ordered_dither:read("ordered_dither",0.0,1.0)?,
            retro_strength:read("retro_strength",0.0,1.0)?,
            ldr_dither:read("ldr_dither",0.0,1.0)?,
            ambient_white_balance: value.get("ambient_white_balance")
                .map(|v| v.as_bool().context("display ambient_white_balance must be boolean"))
                .transpose()?.unwrap_or(false) })
    }

    pub fn to_json(self) -> Value {
        json!({"width":self.width,"height":self.height,"palette_levels":self.palette_levels,
            "ordered_dither":self.ordered_dither,"retro_strength":self.retro_strength,
            "ldr_dither":self.ldr_dither,"ambient_white_balance":self.ambient_white_balance})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a GPU and an explicitly selected saved winter study"]
    fn saved_winter_profile_survives_game_boot_and_updates() -> anyhow::Result<()> {
        let path=std::path::PathBuf::from(std::env::var("KESSEL_PLAYTEST_MANIFEST")?);
        let prefix=std::env::var("KESSEL_WINTER_REVIEW_PREFIX")?;
        ensure!(!std::path::Path::new(&format!("{prefix}.png")).exists(),"use a fresh capture prefix");
        let manifest=crate::act_manifest::ActManifest::load(&path)?;
        let saved=DisplayTuning::from_edits(manifest.edits())?.context("saved display required")?;
        let lighting=crate::environment_authoring::LightingTuning::from_edits(manifest.edits())?;
        let act=crate::act_manifest::select_runtime_manifest(Some(path))?.unwrap();
        let instance=crate::create_instance();
        let adapter=crate::request_adapter(&instance,None);
        let (device,queue)=crate::request_device(&adapter);
        let (mut r,mut g)=crate::game::Game::build_playtest_act(&device,&queue,false,&crate::world::act(act))?;
        for _ in 0..3 { g.update(&crate::game::TankInput::default(),1.0/60.0,&mut r); }
        assert_eq!(r.display_tuning,Some(saved));
        assert_eq!((r.iw,r.ih),(saved.width,saved.height));
        // The saved lighting record starts the sky clock; it cannot fix the camera's exposure.
        assert_eq!(r.exposure_override,None);
        if let Some(utc)=lighting.utc { assert_eq!(r.sky_clock.start,utc); }
        assert_eq!(r.fog_density,lighting.fog_density);
        assert_eq!(r.overcast,lighting.overcast);
        assert_eq!(r.storm,lighting.storm);
        r.camera.pos=glam::Vec3::new(1903.0,8.0,193.0);
        r.camera.set_forward(glam::Vec3::new(1880.0,7.0,167.0)-r.camera.pos);
        r.camera.fov_y=60f32.to_radians();r.reticle=false;r.optic_mask=0.0;
        r.interior_light=0.0;r.interior_haze=0.0;r.weather_shelter=0.0;
        r.set_interior_atmosphere(0.0,0.0,glam::Vec3::ZERO);
        r.reset_history();
        r.render_to_png(&device,&queue,&format!("{prefix}.png"),r.iw*2,r.ih*2)?;
        std::fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&json!({
            "scope":"Actual Play world boot and three updates; saved lighting and display asserted. Fixed exterior review camera.",
            "display":saved.to_json(),"lighting":lighting.to_json(),"adapter":adapter.get_info().name,
        }))?)?;
        crate::act_manifest::select_runtime_manifest(None)?;
        Ok(())
    }

    #[test]
    fn document_display_is_optional_and_round_trips() {
        assert_eq!(DisplayTuning::from_edits(None).unwrap(), None);
        let mut tuning = DisplayTuning::at_resolution(960,540);
        tuning.ordered_dither = 0.35;
        tuning.ambient_white_balance = true;
        assert_eq!(DisplayTuning::from_edits(Some(&json!({KEY:tuning.to_json()}))).unwrap(),Some(tuning));
        for (key, bad) in [("width",json!(777)),("ordered_dither",json!(1.5)),
            ("palette_levels",json!(2.5)),("retro_strength",json!(-0.1))] {
            let mut value=tuning.to_json(); value[key]=bad;
            assert!(DisplayTuning::from_json(&value).is_err());
        }
    }
}
