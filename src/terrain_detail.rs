//! Small repeating source-Pizza detail array. Linear reflectance ratios carry texture, while
//! the authored world drape supplies material colour. Every mip is area-averaged on the CPU.
const TILES: [&[u8]; 4] = [
    include_bytes!("../source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/grass_7.png"),
    include_bytes!("../source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/dirt_5.png"),
    include_bytes!("../source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/dirt_2.png"),
    include_bytes!("../source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/snow_1.png"),
];

fn ratios(bytes: &[u8]) -> Vec<[f32; 4]> {
    let image = image::load_from_memory(bytes)
        .expect("embedded source Pizza detail")
        .into_rgb8();
    assert_eq!(image.dimensions(), (256, 256));
    let linear: Vec<[f32; 3]> = image
        .pixels()
        .map(|p| {
            std::array::from_fn(|c| {
                let x = p[c] as f32 / 255.0;
                if x <= 0.04045 {
                    x / 12.92
                } else {
                    ((x + 0.055) / 1.055).powf(2.4)
                }
            })
        })
        .collect();
    // A full tile has 65,536 samples: f32 sequential sums measurably bias the neutral mip.
    let mean: [f32; 3] = std::array::from_fn(|c| {
        (linear.iter().map(|p| p[c] as f64).sum::<f64>() / linear.len() as f64) as f32
    });
    // Bounded source variation avoids treating isolated source highlights as bright emitters.
    // Recenter after clipping so the last mip is neutral and distant soil does not change colour.
    let mut field: Vec<[f32; 4]> = linear
        .iter()
        .map(|p| {
            [0, 1, 2, 3].map(|c| {
                if c == 3 {
                    1.0
                } else {
                    (p[c] / mean[c].max(0.00001)).clamp(0.35, 1.8)
                }
            })
        })
        .collect();
    let clipped: [f32; 3] = std::array::from_fn(|c| {
        (field.iter().map(|p| p[c] as f64).sum::<f64>() / field.len() as f64) as f32
    });
    for p in &mut field {
        for c in 0..3 {
            p[c] = (1.0 + (p[c] - clipped[c]) * 0.60) * 0.5;
        }
    }
    field
}

pub fn upload(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("source-pizza-terrain-detail"),
        size: wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 4,
        },
        mip_level_count: 9,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, bytes) in TILES.iter().enumerate() {
        let mut size = 256;
        let mut field = ratios(bytes);
        for mip in 0..9 {
            let packed: Vec<u8> = field
                .iter()
                .flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
                .collect();
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: mip,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &packed,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
            if size == 1 {
                break;
            }
            let next = size / 2;
            let mut down = Vec::with_capacity((next * next) as usize);
            for y in 0..next {
                for x in 0..next {
                    down.push(std::array::from_fn(|c| {
                        let i = (y * 2 * size + x * 2) as usize;
                        (field[i][c]
                            + field[i + 1][c]
                            + field[i + size as usize][c]
                            + field[i + size as usize + 1][c])
                            * 0.25
                    }));
                }
            }
            size = next;
            field = down;
        }
    }
    tex.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_detail_distant_average_is_neutral_and_finite() {
        for bytes in super::TILES {
            let field = super::ratios(bytes);
            for c in 0..3 {
                let mean = field.iter().map(|p| p[c] as f64).sum::<f64>() / field.len() as f64;
                assert!(
                    (mean - 0.5).abs() < 0.0001,
                    "far mip changed material colour: {mean}"
                );
                assert!(field
                    .iter()
                    .all(|p| p[c].is_finite() && (0.0..=1.0).contains(&p[c])));
            }
        }
    }
}
