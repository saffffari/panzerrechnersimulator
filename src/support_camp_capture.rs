//! Windowless source-camp review. This fixture intentionally does not run a battle, spawn a
//! roster or claim medical/vehicle functionality. Live expedition separately installs this source.
use crate::{
    gltf_load::{CpuImage, CpuMesh, Vertex},
    support_camp::SupportCamp,
};
use anyhow::{ensure, Result};
use glam::{Mat4, Vec3};
use serde_json::json;
use std::{fs, io::Write, path::Path};

fn inspection_ground() -> CpuMesh {
    CpuMesh::basic(
        [
            [-25., -0.01, -23.],
            [-25., -0.01, 26.],
            [26., -0.01, 26.],
            [26., -0.01, -23.],
        ]
        .into_iter()
        .map(|pos| Vertex {
            pos,
            normal: [0., 1., 0.],
            uv: [0.5, 0.5],
            tangent: [1., 0., 0., 1.],
        })
        .collect(),
        vec![0, 1, 2, 0, 2, 3],
        Some(CpuImage::new(1, 1, vec![104, 100, 85, 255])),
        Vec3::new(-25., -0.01, -23.),
        Vec3::new(26., -0.01, 26.),
    )
}

pub fn run(requested_prefix: &str) -> Result<()> {
    let prefix = crate::capture_target(requested_prefix);
    let views = [
        ("overview", Vec3::new(25., 21., 28.), Vec3::new(0., 0., 1.)),
        ("yard", Vec3::new(0., 1.7, 10.), Vec3::new(-2., 1.5, -3.)),
        (
            "medical",
            Vec3::new(-2.6, 1.65, 0.),
            Vec3::new(-7.25, 0.95, 0.),
        ),
        (
            "quarters",
            Vec3::new(0., 1.7, -4.4),
            Vec3::new(0., 0.9, -9.),
        ),
        ("workshop", Vec3::new(1.8, 2.0, 7.), Vec3::new(6., 1.0, 0.)),
        ("ammunition",Vec3::new(-2.5,1.7,5.8),Vec3::new(-6.5,1.,5.8)),
        ("fuel",Vec3::new(-2.6,1.7,11.2),Vec3::new(-6.,0.8,11.2)),
        (
            "transport",
            Vec3::new(11., 3.1, 17.),
            Vec3::new(6., 1.3, 11.2),
        ),
    ];
    ensure!(
        !Path::new(&format!("{prefix}.json")).exists()
            && views
                .iter()
                .all(|(name, ..)| !Path::new(&format!("{prefix}_{name}.png")).exists()),
        "camp-review prefix already exists; use a new capture prefix"
    );
    let mut camp = SupportCamp::load()?;
    camp.include_static_halftrack()?;
    let mut scene = vec![inspection_ground()];
    let placement = camp.append_to_scene(&mut scene, Mat4::IDENTITY)?;
    if let Some(parent) = Path::new(&prefix).parent() {
        fs::create_dir_all(parent)?;
    }
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let mut renderer = crate::renderer::Renderer::new(&device, &queue, &scene);
    // This is a source-layout inspection, not the live winter expedition. Use the existing
    // dated clear-day environment so the fifty-metre layout is actually inspectable.
    renderer.apply_campaign_atmosphere(3, false, crate::renderer::DayPhase::Day);
    let mut shots = Vec::new();
    for (name, eye, target) in views {
        renderer.camera.pos = eye;
        renderer.camera.set_forward(target - eye);
        renderer.camera.fov_y = 55f32.to_radians();
        renderer.optic_mask = 0.;
        renderer.reticle = false;
        renderer.reset_history();
        let settle = crate::converge_headless_screenshot(&mut renderer, &device, &queue, 24)?;
        ensure!(
            settle.stable,
            "camp review exposure did not converge for {name}"
        );
        let path = format!("{prefix}_{name}.png");
        renderer.render_to_png(&device, &queue, &path, renderer.iw, renderer.ih)?;
        shots.push(
            json!({"name":name,"file":path,"eye_m":eye.to_array(),"target_m":target.to_array(),
            "fov_y_degrees":55,"measured_exposure":settle.measured_exposure}),
        );
    }
    let a = &placement.anchors;
    let receipt = json!({"schema":"panzer.support-camp-source-review.v1",
        "shots":shots,"adapter":adapter.get_info().name,"internal_resolution":[renderer.iw,renderer.ih],
        "source_triangles":placement.triangles.len(),"source_meshes":placement.meshes.len(),
        "sources":placement.parts.iter().map(|p|json!({"part":p.name,"source":p.source,"scene_mesh":p.scene_mesh})).collect::<Vec<_>>(),
        "anchors":{"origin":a.origin.to_array(),"medic":a.medic.to_array(),
            "beds":a.medical_beds.map(|b|json!({"id":b.id,"patient_pose":b.patient_pose.to_cols_array(),"attend_from":b.attend_from.to_array()})),
            "quarters":a.quarters.map(|p|p.to_array()),"ammo":a.ammo.to_array(),"fuel":a.fuel.to_array(),
            "parts":a.parts.to_array(),"load":a.load.to_array(),"tiger_parking":a.tiger_parking.to_array(),"recovery_parking":a.recovery_parking.to_array()},
        "spawned_people":0,"game_constructed":false,"audio_device_opened":false,
        "inspection_weather":"existing Act Three clear-day profile; not live expedition weather",
        "visual_review_passed":false,"live_collision_tested":false,
        "scope":"Authored approximate field-camp canvas/frame geometry and existing source props; no claim of an exact historical tent. Empty furniture slots do not create roster members or supply quantities. Parked halftrack is the reviewed static prototype, with no driving/towing. Neutral level inspection ground; live map placement is a separate source-terrain admission. No image grading."});
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(format!("{prefix}.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    println!(
        "[camp-review] {} source views and {prefix}.json",
        shots.len()
    );
    Ok(())
}
