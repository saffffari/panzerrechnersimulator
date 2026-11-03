//! `Game::hot_surfaces`: the hot bodies of the fire, spark and fume side for the heat shimmer's
//! refraction (`hot_surfaces`).
use super::*;
use crate::hot_surfaces::{HotKind, HotSurface};

impl Game {
    /// Every hot body this side owns this frame: the penetrations' glowing rims, the hot fragments,
    /// the cabin fire's burning patches, the burning wrecks and the spent cases still warm.
    pub fn hot_surfaces(&self) -> Vec<HotSurface> {
        let mut out = Vec::new();
        let warm = |k: f32| k > crate::hot_surfaces::AMBIENT_K + crate::hot_surfaces::REPORT_ABOVE_K;
        for (world, normal, heat, age) in self.weapon_vfx.hot_rims() {
            let surface_k = crate::renderer::renderer_weapon_particles::math::rim_temperature(heat.initial_kelvin, age, heat.tau_s);
            if !warm(surface_k) { continue; }
            let outer = heat.radius + heat.width;
            out.push(HotSurface { kind: HotKind::PenetrationRim, world, normal, size_m: outer,
                area_m2: std::f32::consts::PI * (outer * outer - heat.radius * heat.radius), surface_k });
        }
        for fragment in self.hot_fragments.iter().filter(|f| f.active && warm(f.temp_k)) {
            let world = blood_parent_model(&self.player, fragment.frame).transform_point3(fragment.pos);
            out.push(HotSurface { kind: HotKind::Fragment, world, normal: Vec3::ZERO, size_m: fragment.size,
                area_m2: 6.0 * fragment.size * fragment.size, surface_k: fragment.temp_k });
        }
        let models = self.fire_surface_models();
        for k in 0..surface_fire::MAX_PATCHES {
            let Some(patch) = self.cabin_fire.patch_surface(k, &models).filter(|p| p.intensity > 0.02) else { continue; };
            let r = crate::hot_surfaces::FIRE_PATCH_RADIUS_M;
            out.push(HotSurface { kind: HotKind::FirePatch, world: patch.position, normal: patch.normal, size_m: r,
                area_m2: std::f32::consts::PI * r * r * patch.intensity.clamp(0.0, 1.0), surface_k: crate::hot_surfaces::FLAME_K });
        }
        let r = crate::hot_surfaces::WRECK_FIRE_RADIUS_M;
        let mut wreck = |world: Vec3, strength: f32| if strength > 0.02 {
            out.push(HotSurface { kind: HotKind::WreckFire, world, normal: Vec3::Y, size_m: r,
                area_m2: std::f32::consts::PI * r * r * strength.clamp(0.0, 1.0), surface_k: crate::hot_surfaces::FLAME_K });
        };
        for i in 0..self.wrecks.len() {
            if !(self.enemy_active[i] && !self.enemies[i].alive && self.wrecks[i].burning) { continue; }
            let at = self.wreck_deck_anchors[wreck_kind_index(self.enemies[i].kind)]
                .map(|p| self.enemies[i].hull_model().transform_point3(p)).or(self.wrecks[i].fire_entry_world);
            if let Some(at) = at { wreck(at, self.wrecks[i].fire_strength()); }
        }
        for ally in self.allies.iter().filter(|a| !a.tank.alive) { wreck(ally.tank.center() + Vec3::Y * 0.30, 0.88); }
        for hulk in self.campaign_hulks.iter().filter(|h| h.burning) {
            if let Some(anchor) = self.wreck_deck_anchors[wreck_kind_index(hulk.kind)] { wreck(hulk.models[0].transform_point3(anchor), 0.82); }
        }
        if !self.player.alive && self.fire_active {
            wreck(self.last_fatal.as_ref().map_or(self.player.center(), |f| f.entry_point), self.fire.clamp(0.0, 1.0));
        }
        for c in self.casings.iter().filter(|c| c.active) {
            let Some(age) = c.fired_s else { continue; };
            let surface_k = crate::hot_surfaces::case_wall_k(age);
            if !warm(surface_k) { continue; }
            out.push(HotSurface { kind: HotKind::SpentCase, world: c.pos, normal: Vec3::ZERO,
                size_m: crate::hot_surfaces::CASE_LENGTH_M, area_m2: crate::hot_surfaces::CASE_AREA_M2, surface_k });
        }
        out
    }
}
