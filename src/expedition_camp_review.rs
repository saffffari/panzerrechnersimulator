//! Explicit offscreen fixture of bedside arrival in the actual installed expedition camp.
//! The wound and delivered body are staged, never attributed to a simulated shell or journey.
//! Admission, named-medic travel, finite treatment and retained actor physiology use live paths.
use super::*;
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use std::{io::Write, path::Path};

fn photo(r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, path: &str,
    eye: Vec3, target: Vec3) -> anyhow::Result<()> {
    r.camera.pos=eye;
    r.camera.set_forward(target-eye);
    r.camera.fov_y=60f32.to_radians();
    r.reset_history();
    crate::converge_headless_screenshot(r,device,queue,24)?;
    r.render_to_png(device,queue,path,1280,720)
}

pub fn run(requested: &str) -> anyhow::Result<()> {
    let prefix=crate::capture_target(requested);
    let receipt_path=format!("{prefix}.json");
    let names=["camp", "overview", "medical_stock", "ammunition", "fuel", "approach",
        "arrival", "admitted", "treated"];
    ensure!(!Path::new(&receipt_path).exists()
        && names.iter().all(|name|!Path::new(&format!("{prefix}_{name}.png")).exists()),
        "medical review prefix exists");
    if let Some(parent)=Path::new(&prefix).parent(){std::fs::create_dir_all(parent)?;}
    std::env::set_var("KESSEL_AUDIO_CAPTURE","1");
    crate::act_manifest::select_runtime_manifest(Some(crate::act_manifest::playtest_manifest_path()))?;
    let instance=crate::create_instance();
    let adapter=crate::request_adapter(&instance,None);
    let (device,queue)=crate::request_device(&adapter);
    let (mut r,mut g)=Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;
    g.mute_audio();
    g.enable_expedition(1944,g.expedition_initial_camp_position()?)?;
    let anchors=g.installed_camp_anchors().context("camp not installed")?.clone();
    let bed=anchors.medical_beds[0];
    let root=g.support_camp.as_ref().unwrap().placement.root;
    g.mode=Mode::Foot;
    g.foot.pos=anchors.spawn;
    g.foot.yaw=std::f32::consts::PI;
    for _ in 0..120 {g.update(&TankInput::default(),1./60.,&mut r);}
    photo(&mut r,&device,&queue,&format!("{prefix}_camp.png"),
        root.transform_point3(Vec3::new(0.,1.8,4.)),anchors.rest+Vec3::Y)?;
    // These are observer-only views of the same installed Game, with its actual terrain,
    // placed stores, people and approach dressing. No inspection ground plane is substituted.
    for (name, eye, target) in [
        ("overview",root.transform_point3(Vec3::new(22.,18.,28.)),anchors.origin+Vec3::Y),
        ("medical_stock",anchors.medical_center+root.transform_vector3(Vec3::new(2.0,1.6,1.6)),
            anchors.medical_center+root.transform_vector3(Vec3::new(-0.7,1.05,-0.6))),
        ("ammunition",anchors.ammo+root.transform_vector3(Vec3::new(3.0,1.7,0.)),anchors.ammo+Vec3::Y),
        ("fuel",anchors.fuel+root.transform_vector3(Vec3::new(3.0,1.7,0.)),anchors.fuel+Vec3::Y),
    ] { photo(&mut r,&device,&queue,&format!("{prefix}_{name}.png"),eye,target)?; }
    let toward_spawn=(g.player.pos-anchors.origin).with_y(0.).normalize_or_zero();
    let approach_xz=anchors.origin+toward_spawn*55.;
    let approach_eye=Vec3::new(approach_xz.x,g.hf.height_at(approach_xz.x,approach_xz.z)+2.2,approach_xz.z);
    photo(&mut r,&device,&queue,&format!("{prefix}_approach.png"),approach_eye,anchors.origin+Vec3::Y)?;
    let camp_before=g.camp_people_receipt();
    let role=CrewRole::Driver;
    let actor=g.singleplayer_actor_id(role).context("missing driver")?;
    let person=g.singleplayer_person_id(role).context("driver not bound")?;
    let man=g.singleplayer_actor_casualty_mut(actor).unwrap();
    man.add_wound(crate::first_aid::Wound::new(crate::first_aid::WoundKind::Bleeding,
        crate::first_aid::Region::Leg,0.8));
    man.unconscious_s=45.;
    let identity=g.singleplayer_casualty_identity(role);
    let (scale,segments)=g.physical_crew_segments(identity,blood_frame_for_role(identity.role));
    let source=blood_parent_model(&g.player,blood_frame_for_role(identity.role))
        .transform_point3(crew_physics::center_of_mass(&segments));
    let impact=CrewRagdollImpact {hit_part:crew_physics::CrewBodyPart::Torso,
        point_world:source,velocity_world:Vec3::ZERO,energy_j:0.,blast_origin_world:source,
        explosive_energy_j:0.,behind_armor:0.,blast_proximity_m:0.};
    let mut body=new_crew_ragdoll_from_segments(&g.player,identity,impact,scale,segments);
    body.detach_into_world(&g.player);
    g.couch_ragdolls.push(body);
    let arrival=bed.patient_pose.transform_point3(Vec3::new(1.05,-0.30,0.));
    g.place_detached_crew_body(actor,Mat4::from_rotation_translation(
        Quat::from_rotation_y(Quat::from_mat4(&root).to_euler(glam::EulerRot::YXZ).0)
            *Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),arrival))?;
    g.foot.pos=bed.attend_from+root.transform_vector3(Vec3::new(1.1,0.,0.));
    g.foot.yaw=(arrival.x-g.foot.pos.x).atan2(-(arrival.z-g.foot.pos.z));
    g.update(&TankInput::default(),0.,&mut r);
    g.sync_expedition_now()?;
    let eye=g.foot.eye();
    let look=(arrival-eye).normalize();
    photo(&mut r,&device,&queue,&format!("{prefix}_arrival.png"),eye,arrival)?;
    let before=g.singleplayer_actor_casualty(actor).unwrap().clone();
    let picked=g.interact_crew_body(look,eye);
    let held=g.crew_body_held_actor();
    let bed_look=(bed.patient_pose.w_axis.truncate()-eye).normalize();
    let target=g.expedition_medical_target(eye,bed_look);
    let admitted=if picked {g.expedition_interact_medical(eye,bed_look)?}else{false};
    let mut checks=Vec::<Value>::new();
    checks.push(json!({"check":"actual anatomical patient pickup","pass":picked&&held==Some(actor),"held":held}));
    checks.push(json!({"check":"selected source bed admits held patient","pass":admitted
        &&g.expedition_state().unwrap().medical.occupies_bed(person),"target":target}));
    checks.push(json!({"check":"admission preserves wound and blood","pass":
        g.singleplayer_actor_casualty(actor).unwrap().blood_l==before.blood_l
        &&g.singleplayer_actor_casualty(actor).unwrap().wounds==before.wounds}));
    photo(&mut r,&device,&queue,&format!("{prefix}_admitted.png"),eye,bed.patient_pose.w_axis.truncate())?;
    let admission_state=g.expedition_status();
    let aid_before=serde_json::to_value(&g.expedition_state().unwrap().medical.supplies)?;
    let mut samples=Vec::new();
    if admitted {
        for tick in 0..3600 {
            g.update(&TankInput::default(),1./60.,&mut r);
            if tick%300==299 {
                g.sync_expedition_now()?;
                samples.push(json!({"seconds":(tick+1)as f32/60.,"medic_at_bed":g.camp_medic_at_bed(bed.id),
                    "people":g.camp_people_receipt(),"status":g.expedition_status()}));
            }
        }
    }
    g.sync_expedition_now()?;
    let after=g.singleplayer_actor_casualty(actor).unwrap();
    checks.push(json!({"check":"patient survives with original identity and wounds","pass":
        !after.dead&&!after.wounds.is_empty()&&g.singleplayer_person_for_actor(actor)==Some(person)}));
    checks.push(json!({"check":"care consumes finite stock without replacing blood","pass":
        admitted&&serde_json::to_value(&g.expedition_state().unwrap().medical.supplies)?!=aid_before
        &&after.blood_l<=before.blood_l}));
    checks.push(json!({"check":"named medic physically reaches bed","pass":samples.iter()
        .any(|s|s["medic_at_bed"]==true)}));
    photo(&mut r,&device,&queue,&format!("{prefix}_treated.png"),eye,bed.patient_pose.w_axis.truncate())?;
    let pass=checks.iter().all(|c|c["pass"]==true);
    let receipt=json!({"schema":"panzer.expedition-medical-review.v1","pass":pass,"checks":checks,
        "person":person,"actor":actor,"camp_before":camp_before,"admission":admission_state,
        "samples":samples,"final":g.expedition_status(),"bodies":g.crew_body_recovery_receipt(),
        "scope":"Real installed Act One camp and roster. Incapacitating wound and delivery beside bed explicitly staged, no shell or extraction/transport proof. Actual mesh pickup, bed targeting/admission, medic movement, finite treatment and persistent physiology. No automatic healing or save/resume proof."});
    let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(receipt_path)?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    ensure!(pass,"medical review failed; inspect receipt");
    Ok(())
}
