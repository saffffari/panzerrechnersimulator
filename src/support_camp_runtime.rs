//! Opt-in scene installation. Camp contacts use exactly the appended visible source triangles;
//! bounds below are site-admission broad phases only, never invisible gameplay walls.
use super::*;
use crate::support_camp::{CampAnchors, CampCollision, CampPlacement, SupportCamp};
use anyhow::{ensure, Result};
#[path = "camp_terrain_pad.rs"]
mod camp_terrain_pad;

pub(super) struct CampRuntime {
    pub(super) placement: CampPlacement,
    pub(super) approach_dressing: Option<crate::worldgen::GenWorld>,
    contacts: surface_fire::CabinFire,
    contact_model: Mat4,
    bounds: (Vec3, Vec3),
    grading: Option<camp_terrain_pad::GradeReceipt>,
}

impl CampRuntime {
    #[cfg(test)]
    pub(super) fn source_fixture(root: Mat4) -> Result<Self> {
        let camp = SupportCamp::load()?;
        let mut scene = Vec::new();
        let placement = camp.append_to_scene(&mut scene, root)?;
        let refs: Vec<_> = camp
            .parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                (
                    usize::from(part.collision == CampCollision::Canvas),
                    &scene[i],
                )
            })
            .collect();
        let bounds = camp.bounds();
        Ok(Self {
            contacts: surface_fire::CabinFire::new(&refs),
            contact_model: Mat4::IDENTITY,
            placement,
            approach_dressing: None,
            bounds: crate::building_render::transformed_bounds(root, bounds.0, bounds.1),
            grading: None,
        })
    }

    pub(super) fn install(
        scene: &mut Vec<CpuMesh>,
        hf: &mut terrain::Heightfield,
        act: &crate::world::ActMap,
        manifest: Option<&crate::act_manifest::ActManifest>,
        authored_grid: &[f32],
        detail_ground: bool,
        occluders: &[stealth::Occluder],
        additional_cover: &[(Vec3, Vec3)],
        swarm: &[(f32, f32, SpawnZone, EnemyKind, bool)],
    ) -> Result<Self> {
        let mut camp = SupportCamp::load()?;
        let mut obstacles: Vec<_> = occluders.iter().map(|o| (o.min, o.max)).collect();
        obstacles.extend_from_slice(additional_cover);
        // A tent cannot be installed through a source tree, furniture or rendered building
        // merely because that asset is not an armor occluder. Reuse the manifest compiler.
        if let Some(manifest) = manifest {
            let paging = crate::editor::manifest_pages_from(manifest)?;
            for asset in &paging.prototypes {
                let mesh = match crate::world_doors::load_frame(&asset.path)? {
                    Some(frame) => frame,
                    None => crate::worldgen::load_module(&asset.path).ok_or_else(|| {
                        anyhow::anyhow!("camp admission cannot read {}", asset.path)
                    })?,
                };
                for page in &asset.pages {
                    for authored in &page.transforms {
                        let mut model = *authored;
                        if detail_ground {
                            let (x, z) = (model.w_axis.x, model.w_axis.z);
                            model.w_axis.y += hf.height_at(x, z)
                                - terrain::grid_grade(authored_grid, hf.res, hf.spacing, x, z);
                        }
                        let (lo, hi) =
                            crate::building_render::transformed_bounds(model, mesh.min, mesh.max);
                        // Ground-only road surfaces may support camp; every raised source is excluded.
                        let center = (lo + hi) * 0.5;
                        if hi.y > hf.height_at(center.x, center.z) + 0.10 {
                            obstacles.push((lo, hi));
                        }
                    }
                }
            }
        }
        let (local_lo, local_hi) = camp.bounds();
        let forbidden: Vec<Vec3> = swarm
            .iter()
            .map(|(x, z, ..)| Vec3::new(*x, hf.height_at(*x, *z), *z))
            .collect();
        // The support camp is a compact set of independently seated shelters on existing
        // terrain. Do not flatten a rectangular campground or erase its original material.
        // A course may set where its camp stands (`canyon_course.camp_at` [x, z], e.g. just over
        // the first rise behind the start): the site search starts there instead of at the spawn.
        let sited_act = manifest.and_then(|m| m.edits())
            .and_then(|e| e["canyon_course"]["camp_at"].as_array().cloned())
            .and_then(|a| Some(Vec2::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32)))
            .map(|at| { let mut a = act.clone(); a.spawn = at; a });
        let root = select_site(hf,sited_act.as_ref().unwrap_or(act),local_lo,local_hi,&obstacles,&forbidden,0.60,1.0)?;
        camp.conform_to_terrain(root,hf)?;
        let grading = None;
        let (local_lo,local_hi)=camp.bounds();
        let placement = camp.append_to_scene(scene, root)?;
        let refs: Vec<_> = placement
            .parts
            .iter()
            .map(|part| {
                let kind = camp.parts[part.scene_mesh - placement.meshes.start].collision;
                (
                    usize::from(kind == CampCollision::Canvas),
                    &scene[part.scene_mesh],
                )
            })
            .collect();
        let contacts = surface_fire::CabinFire::new(&refs);
        let bounds = crate::building_render::transformed_bounds(root, local_lo, local_hi);
        obstacles.extend(forbidden.iter().map(|p| (*p - Vec3::new(5., 1., 5.), *p + Vec3::new(5., 4., 5.))));
        let approach_dressing = Some(crate::worldgen::generate_camp_dressing(
            hf, &placement.anchors, bounds, &obstacles, act.spawn, act.seed,
        ));
        Ok(Self {
            placement,
            approach_dressing,
            contacts,
            contact_model: Mat4::IDENTITY,
            bounds,
            grading,
        })
    }

    pub(super) fn contact(&self, from: Vec3, to: Vec3) -> Option<(Vec3, Vec3, CampCollision)> {
        self.contacts
            .sweep(from, to, &[self.contact_model; surface_fire::FRAMES])
            .map(|hit| {
                (
                    self.contact_model.transform_point3(hit.point),
                    self.contact_model.transform_vector3(hit.normal).normalize_or_zero(),
                    if hit.frame == 1 {
                        CampCollision::Canvas
                    } else {
                        CampCollision::Solid
                    },
                )
            })
    }

    pub(super) fn blocks_sight(&self, from: Vec3, to: Vec3) -> bool {
        self.contact(from, to).is_some()
    }

    pub(super) fn standing_y(&self,point:Vec3,ground:f32)->f32 {
        self.contact(Vec3::new(point.x,ground+0.65,point.z),Vec3::new(point.x,ground-0.02,point.z))
            .filter(|(_,n,kind)|*kind==CampCollision::Solid&&n.y>0.7)
            .map_or(ground,|(p,_,_)|p.y.max(ground))
    }

    pub(super) fn near(&self, position: Vec3, margin: f32) -> bool {
        position.cmpge(self.bounds.0 - Vec3::splat(margin)).all()
            && position.cmple(self.bounds.1 + Vec3::splat(margin)).all()
    }

    pub(super) fn validate_final_world(
        &self,
        cover: &[(Vec3, Vec3)],
        enemies: &[Tank],
    ) -> Result<()> {
        let (lo, hi) = self.bounds;
        ensure!(
            !cover
                .iter()
                .any(|&(a, b)| a.cmple(hi).all() && b.cmpge(lo).all()),
            "late world dressing intersects support camp; author a clear camp pad"
        );
        ensure!(
            !enemies.iter().any(|e| self.near(e.pos, 16.0)),
            "enemy initial placement intersects support camp; author a clear camp pad"
        );
        Ok(())
    }
}

fn select_site(
    hf: &terrain::Heightfield,
    act: &crate::world::ActMap,
    lo: Vec3,
    hi: Vec3,
    obstacles: &[(Vec3, Vec3)],
    enemies: &[Vec3],
    relief_limit: f32,
    clearance: f32,
) -> Result<Mat4> {
    let half = (hf.res.saturating_sub(1)) as f32 * hf.spacing * 0.5;
    let direction = act.goal - act.spawn;
    let heading = direction.x.atan2(direction.y);
    for radius in [40., 64., 96., 128., 192., 256., 384., 512.] {
        for sector in 0..64 {
            let angle =
                heading + std::f32::consts::PI + sector as f32 * std::f32::consts::TAU / 64.;
            let x = act.spawn.x + angle.sin() * radius;
            let z = act.spawn.y + angle.cos() * radius;
            if x + lo.x - clearance - 1. <= -half
                || x + hi.x + clearance + 1. >= half
                || z + lo.z - clearance - 1. <= -half
                || z + hi.z + clearance + 1. >= half
            {
                continue;
            }
            let y = hf.height_at(x, z);
            if !y.is_finite() {
                continue;
            }
            let mut low = y;
            let mut high = y;
            // Sample the full source footprint, not merely a marker's centre and four corners.
            let mut level = true;
            'pad: for dz in (lo.z.floor() as i32 - 1)..=(hi.z.ceil() as i32 + 1) {
                for dx in (lo.x.floor() as i32 - 1)..=(hi.x.ceil() as i32 + 1) {
                    let h = hf.height_at(x + dx as f32, z + dz as f32);
                    low = low.min(h);
                    high = high.max(h);
                    if !h.is_finite() || high - low > relief_limit {
                        level = false;
                        break 'pad;
                    }
                }
            }
            if !level {
                continue;
            }
            // Seat frame feet at the lowest observed ground, allowing at most 12 cm of soil
            // embedment. No floating platform, heightfield edit or invisible leveling collider.
            let origin = Vec3::new(
                x,
                if relief_limit > 0.12 {
                    (low + high) * 0.5
                } else {
                    low
                },
                z,
            );
            let a = origin + lo - Vec3::new(clearance, relief_limit, clearance);
            let b = origin + hi + Vec3::new(clearance, relief_limit, clearance);
            if !hf
                .openings
                .meeting(Vec2::new(a.x, a.z), Vec2::new(b.x, b.z))
                .is_empty()
            {
                continue;
            }
            if obstacles
                .iter()
                .any(|&(p, q)| p.cmple(b).all() && q.cmpge(a).all())
                || enemies.iter().any(|p| {
                    p.cmpge(a - Vec3::splat(24.)).all() && p.cmple(b + Vec3::splat(24.)).all()
                })
            {
                continue;
            }
            return Ok(Mat4::from_translation(origin));
        }
    }
    anyhow::bail!("no unobstructed gentle terrain for the compact support camp; author a clear site rather than flattening the countryside")
}

impl Game {
    /// Logistics transition used only offscreen at a cleared, admitted camp site. Retains the
    /// actual camp meshes and collision; no replacement stocks or people are generated.
    pub(super) fn relocate_camp_geometry(&mut self, root: Mat4, r: &mut Renderer) -> Result<()> {
        let camp=self.support_camp.as_mut().context("no installed support camp")?;
        let delta=root*camp.placement.root.inverse();
        for (slot,model) in camp.placement.relocate(root)? {r.set_model(slot,model);}
        camp.contact_model=delta*camp.contact_model;
        camp.bounds=crate::building_render::transformed_bounds(delta,camp.bounds.0,camp.bounds.1);
        Ok(())
    }

    pub(super) fn expedition_relocate_camp(&mut self, site:crate::expedition::SiteId, r:&mut Renderer)->Result<bool>{
        let Some(e)=self.expedition_state() else{return Ok(false)};
        // Admitted patients are retained bodies owned by the beds, not walking CampPeople.
        // Keep the medical detail in place until all beds are cleared; never move its tent
        // away from a patient or invent an offscreen medical transport that does not exist.
        if e.medical.beds.iter().any(|bed|bed.patient.is_some()){return Ok(false)}
        let target=e.sites.iter().find(|s|s.id==site&&s.friendly).context("camp target is not secured")?.position;
        let old=Vec3::new(e.camp.position.x as f32,e.camp.position.y as f32,e.camp.position.z as f32);
        let actor=if self.mode==Mode::Foot{self.foot.pos}else{self.player.pos};
        if actor.distance(old)<250. {return Ok(false)}
        let camp=self.support_camp.as_ref().context("no physical camp")?;
        let inv=camp.placement.root.inverse();
        let (lo,hi)=crate::building_render::transformed_bounds(inv,camp.bounds.0,camp.bounds.1);
        let mut act=self.act.clone();act.spawn=Vec2::new(target.x as f32,target.z as f32);act.goal=act.spawn+Vec2::Y;
        let obstacles:Vec<_>=self.occluders.iter().map(|o|(o.min,o.max)).collect();
        let enemies:Vec<_>=self.enemies.iter().enumerate().filter(|(i,t)|t.alive&&self.enemy_active.get(*i)==Some(&true)).map(|(_,t)|t.pos).collect();
        let root=select_site(&self.hf,&act,lo,hi,&obstacles,&enemies,0.12,1.)?;
        if root.w_axis.truncate().distance(actor)<250. {return Ok(false)}
        // Terrain-conforming tent feet may differ by at most the admitted 12 cm. The concrete
        // workshop retains its authored slab; no rectangular ground plate is added.
        let delta=root*camp.placement.root.inverse();
        let ids:Vec<_>=e.people.iter().filter(|p|p.alive()&&matches!(p.location,crate::expedition::PersonLocation::Camp{..}|crate::expedition::PersonLocation::Treatment{..})).map(|p|p.id).collect();
        let mut state=e.clone();
        let event=crate::expedition::EventId(state.events.iter().map(|e|e.id.0).max().unwrap_or(0)+1);
        let pos=root.w_axis.truncate();
        state.relocate_camp(event,site,crate::expedition::WorldPosition{x:pos.x as f64,y:pos.y as f64,z:pos.z as f64})?;
        self.relocate_camp_geometry(root,r)?;
        self.relocate_camp_people(delta,&ids,r)?;
        self.relocate_parked_recovery_transport(old,delta,r)?;
        self.expedition_runtime.as_mut().unwrap().state=state;
        Ok(true)
    }

    pub fn installed_camp_receipt(&self) -> Option<serde_json::Value> {
        let camp = self.support_camp.as_ref()?;
        Some(
            serde_json::json!({"root":camp.placement.root.to_cols_array(),
            "source_triangles":camp.placement.triangles.len(),"source_meshes":camp.placement.meshes.len(),
            "grading":camp.grading,"people_spawned_by_geometry":0,
            "terrain_conforming":true,"only_raised_surface":"7 x 10 m workshop concrete service slab",
            "display_stocks_are_not_an_inventory_refill":true,"active_medical_beds":2,"spare_stretcher_stations":2,
            "static_transport_only":false,"transport_is_separate_runtime":true,"projectile_material_damage_integrated":false}),
        )
    }
    pub fn installed_camp_anchors(&self) -> Option<&CampAnchors> {
        self.support_camp
            .as_ref()
            .map(|camp| &camp.placement.anchors)
    }
    pub(super) fn camp_blocks_sight(&self, from: Vec3, to: Vec3) -> bool {
        self.support_camp
            .as_ref()
            .is_some_and(|camp| camp.blocks_sight(from, to))
    }
    pub(super) fn resolve_camp_foot(&mut self, previous: Vec3, airborne: bool) {
        let Some(camp) = self
            .support_camp
            .as_ref()
            .filter(|camp| camp.near(previous, 3.) || camp.near(self.foot.pos, 3.))
        else {
            return;
        };
        self.foot
            .resolve_solid_motion(previous, airborne, |from, to| {
                camp.contact(from, to)
                    .filter(|(_, n, kind)| !(*kind == CampCollision::Canvas && n.y > 0.55))
                    .map(|(p, n, _)| (p, n))
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_admission_rejects_occupied_or_uneven_footprints() {
        let mut hf = terrain::Heightfield {
            res: 129,
            spacing: 4.,
            heights: vec![0.; 129 * 129],
            min_h: 0.,
            max_h: 0.,
            openings: Default::default(),
            detail: None,
        };
        let mut act = crate::world::act(0);
        act.spawn = Vec2::ZERO;
        act.goal = Vec2::new(0., 100.);
        let lo = Vec3::new(-15., 0., -17.);
        let hi = Vec3::new(18., 4., 20.);
        let root = select_site(&hf, &act, lo, hi, &[], &[], 0.12, 1.).unwrap();
        assert!(root.transform_point3(Vec3::ZERO).z < 0.);
        assert!(select_site(
            &hf,
            &act,
            lo,
            hi,
            &[(Vec3::splat(-1000.), Vec3::splat(1000.))],
            &[],
            0.12,
            1.
        )
        .is_err());
        for (i, height) in hf.heights.iter_mut().enumerate() {
            *height = (i % 129) as f32 * 0.2;
        }
        assert!(select_site(&hf, &act, lo, hi, &[], &[], 0.12, 1.).is_err());
        assert!(select_site(&hf, &act, lo, hi, &[], &[], 1.6, 12.).is_err());
    }

    #[test]
    #[ignore = "private camp source assets; CPU-only exact-contact admission"]
    fn actual_visible_tent_has_open_door_and_material_specific_contacts() {
        let camp = SupportCamp::load().unwrap();
        let mut scene = Vec::new();
        let placement = camp.append_to_scene(&mut scene, Mat4::IDENTITY).unwrap();
        let refs: Vec<_> = camp
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| (usize::from(p.collision == CampCollision::Canvas), &scene[i]))
            .collect();
        let runtime = CampRuntime {
            contacts: surface_fire::CabinFire::new(&refs),
            contact_model: Mat4::IDENTITY,
            bounds: camp.bounds(),
            placement,
            approach_dressing: None,
            grading: None,
        };
        assert!(
            !runtime.blocks_sight(Vec3::new(-2.6, 1., 0.), Vec3::new(-6., 1., 0.)),
            "medical doorway must stay open"
        );
        assert_eq!(
            runtime
                .contact(Vec3::new(-4., 1., 3.4), Vec3::new(-4., 1., 2.7))
                .unwrap()
                .2,
            CampCollision::Canvas
        );
        assert_eq!(
            runtime
                .contact(Vec3::new(-8.25, 1.6, 0.), Vec3::new(-8.25, 0.7, 0.))
                .unwrap()
                .2,
            CampCollision::Solid
        );
    }
}
