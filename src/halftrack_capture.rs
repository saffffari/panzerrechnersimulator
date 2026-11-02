//! Optional windowless source-asset review. No Game or Audio is constructed, and no vehicle
//! simulation runs. The plain ground plane and observer cameras are inspection fixtures.
use crate::{
    gltf_load::{CpuImage, CpuMesh, Vertex},
    halftrack::{Articulation, HalftrackAsset},
};
use anyhow::{ensure, Context};
use glam::{Mat4, Vec3};
use serde_json::json;
use std::{fs, io::Write, path::Path};

fn ground() -> CpuMesh {
    let positions = [
        [-16., -0.01, -16.],
        [-16., -0.01, 16.],
        [16., -0.01, 16.],
        [16., -0.01, -16.],
    ];
    CpuMesh::basic(
        positions
            .into_iter()
            .map(|pos| Vertex {
                pos,
                normal: [0., 1., 0.],
                uv: [0.5, 0.5],
                tangent: [1., 0., 0., 1.],
            })
            .collect(),
        vec![0, 1, 2, 0, 2, 3],
        Some(CpuImage::new(1, 1, vec![110, 110, 110, 255])),
        Vec3::new(-16., -0.01, -16.),
        Vec3::new(16., -0.01, 16.),
    )
}

/// CPU admission path, useful before obtaining a GPU slot. No standard startup dependency.
pub fn inspect(receipt_path: &Path) -> anyhow::Result<()> {
    ensure!(
        !receipt_path.exists(),
        "preserving existing halftrack inspection receipt"
    );
    let asset = HalftrackAsset::load()?;
    let mut receipt = asset.inspection();
    receipt["schema"] = json!("panzer.halftrack-cpu-inspection.v1");
    receipt["gpu_visual_verification"] = json!(false);
    receipt["game_integration"] = json!(false);
    if let Some(parent) = receipt_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(receipt_path)?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    println!(
        "[halftrack-inspect] {}: {} source triangles in {} textured pages; CPU admission only",
        receipt_path.display(),
        asset.triangles,
        asset.meshes.len()
    );
    Ok(())
}

pub fn run(requested_prefix: &str, interiors: bool) -> anyhow::Result<()> {
    let prefix = crate::capture_target(requested_prefix);
    let mut names = vec!["front_quarter", "rear_quarter", "side", "steered"];
    if interiors {
        names.extend(["cab_detail", "cargo_overhead"]);
    }
    let mut paths = names
        .iter()
        .map(|name| format!("{prefix}_{name}.png"))
        .collect::<Vec<_>>();
    paths.push(format!("{prefix}.json"));
    ensure!(
        paths.iter().all(|p| !Path::new(p).exists()),
        "halftrack review prefix already exists; choose a fresh prefix"
    );
    // Load and validate BEFORE obtaining the GPU. This is an optional private asset, not a
    // prerequisite of normal game startup or distribution.
    let asset = HalftrackAsset::load()?;
    let mut scene = vec![ground()];
    let vehicle = asset.append_to_scene(&mut scene);
    if let Some(parent) = Path::new(&prefix).parent() {
        fs::create_dir_all(parent)?;
    }
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let mut renderer = crate::renderer::Renderer::new(&device, &queue, &scene);
    for index in vehicle.first_mesh..vehicle.first_mesh + crate::halftrack::PART_COUNT {
        renderer.mark_temporal_dynamic(index);
    }
    let neutral = Articulation::default();
    let steered = Articulation {
        steer_left_rad: 24f32.to_radians(),
        steer_right_rad: 24f32.to_radians(),
        front_left_spin_rad: 42f32.to_radians(),
        front_right_spin_rad: 42f32.to_radians(),
        sprocket_spin_rad: 30f32.to_radians(),
        idler_spin_rad: 30f32.to_radians(),
    };
    // These are observer positions derived from the reviewed six-metre envelope; the close
    // cameras inspect the source surface and do not promise a finished or traversable interior.
    let views = [
        (
            "front_quarter",
            Vec3::new(7., 4.1, 9.),
            Vec3::new(0., 1.15, 0.15),
            neutral,
        ),
        (
            "rear_quarter",
            Vec3::new(-7., 4.4, -9.),
            Vec3::new(0., 1.15, -0.2),
            neutral,
        ),
        (
            "side",
            Vec3::new(10.5, 2.9, 0.),
            Vec3::new(0., 1.2, 0.),
            neutral,
        ),
        (
            "steered",
            Vec3::new(4.3, 2.5, 6.6),
            Vec3::new(0., 0.8, 1.6),
            steered,
        ),
        (
            "cab_detail",
            Vec3::new(2.5, 3.8, 3.6),
            Vec3::new(0., 1.6, 0.8),
            neutral,
        ),
        (
            "cargo_overhead",
            Vec3::new(1.7, 5.6, -3.6),
            Vec3::new(0., 1.25, -1.25),
            neutral,
        ),
    ];
    let mut shots = Vec::new();
    for (name, eye, target, pose) in views.into_iter().take(names.len()) {
        vehicle.apply(&mut renderer, Mat4::IDENTITY, pose)?;
        renderer.camera.pos = eye;
        renderer.camera.set_forward(target - eye);
        renderer.camera.fov_y = 48f32.to_radians();
        renderer.optic_mask = 0.;
        renderer.reticle = false;
        renderer.ui_reset();
        renderer.reset_history();
        let settled = crate::converge_headless_screenshot(&mut renderer, &device, &queue, 24)?;
        ensure!(
            settled.stable,
            "halftrack review exposure did not converge for {name}"
        );
        let path = format!("{prefix}_{name}.png");
        renderer
            .render_to_png(&device, &queue, &path, renderer.iw, renderer.ih)
            .with_context(|| format!("capture halftrack view {name}"))?;
        shots.push(
            json!({"name":name,"file":path,"eye_m":eye.to_array(),"target_m":target.to_array(),
            "fov_y_degrees":48,"measured_exposure":settled.measured_exposure,
            "model_matrices":vehicle.rig.models(Mat4::IDENTITY, pose)?.map(|m|m.to_cols_array())}),
        );
    }
    let receipt = json!({
        "schema":"panzer.halftrack-asset-review.v1", "asset":asset.inspection(),
        "adapter":adapter.get_info().name, "renderer":format!("{:?}",crate::graphics::renderer_backend()),
        "internal_resolution":[renderer.iw,renderer.ih], "sky_utc":renderer.sky_clock.now().to_iso(),
        "weather":{"storm":renderer.storm,"fog_density_m_inv":renderer.fog_density},
        "shots":shots, "audio_device_opened":false,"game_constructed":false,
        "driving_tested":false,"towing_tested":false,"human_visual_approval":false,
        "disclosure":"Original low-poly transport geometry and palette, standard engine renderer and its default authored sky. Neutral 32 m inspection ground plane. Static observer views and source-pivot articulation only; no gameplay, collision, transport or recovery simulation. Cab/cargo views inspect source detail, not implemented seats. No image grading."
    });
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(format!("{prefix}.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    println!(
        "[halftrack-review] {} views and {prefix}.json; source-asset review only",
        shots.len()
    );
    Ok(())
}
