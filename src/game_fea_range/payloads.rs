use super::*;
impl Range {
    pub(super) fn draw_experimental_payloads(&mut self) {
        let tank=self.g.projectiles.iter().rev().find(|p|p.shell.name==crate::ballistics::T34_MACH1_PLAYGROUND.name);
        if let Some(p)=tank {
            let parts=crate::tank::T34_PARTS;
            let nose=parts.turret_pivot+parts.gun_trunnion+Vec3::Z*parts.gun_len;
            let rotation=Quat::from_rotation_arc(Vec3::Z,p.velocity.normalize_or_zero());
            let base=Mat4::from_rotation_translation(rotation,p.pos)*Mat4::from_translation(-nose);
            let turret=base*Mat4::from_translation(parts.turret_pivot);
            for (slot,model) in self.payload_t34.into_iter().zip([base,turret,turret*Mat4::from_translation(parts.gun_trunnion)]) {
                self.r.set_visible(slot,true);self.r.set_model(slot,model);
            }
        } else {for slot in self.payload_t34 {self.r.set_visible(slot,false);}}
        let cannon=self.g.projectiles.iter().rev().find(|p|p.shell.name==crate::ballistics::GUSTAV_800_PLAYGROUND.name);
        let models=cannon.map(|p|vec![Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::Z,p.velocity.normalize_or_zero()),p.pos)]).unwrap_or_default();
        self.r.set_moving_instanced_mats(self.payload_gustav,&models);
    }
}
