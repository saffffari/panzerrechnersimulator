//! Small repeating source-Pizza detail array. Linear reflectance ratios carry texture, while
//! the authored world drape supplies material colour. Every mip is area-averaged on the CPU.
const TILES: [&str; 4] = [
    "source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/grass_7.png",
    "source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/dirt_5.png",
    "source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/dirt_2.png",
    "source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/snow_1.png",
];

fn ratios(bytes: &[u8]) -> Vec<[f32; 4]> {
    ratios_filtered(bytes, true)
}

fn ratios_filtered(bytes: &[u8], high: bool) -> Vec<[f32; 4]> {
    let image = image::load_from_memory(bytes)
        .expect("source Pizza detail")
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
    // High-pass: remove the tile's structure broader than about half a metre (a three-pass box
    // blur, radius 24 texels, wrapped like the tile). That structure is what makes a repeat
    // readable; the grain that remains reads as material, not as a pattern.
    if high { high_pass(&mut field, 256, 24); }
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

/// Subtract a wrapped Gaussian-like blur (three box passes of `radius`) from the colour channels,
/// keeping each channel's mean.
fn high_pass(field: &mut [[f32; 4]], size: usize, radius: usize) {
    // One wrapped running-sum box pass along rows (`step` 1) or columns (`step` = size).
    let box_pass = |input: &[f32], output: &mut [f32], rows: bool| {
        let width = (2 * radius + 1) as f32;
        for line in 0..size {
            let at = |k: usize| if rows { line * size + k % size } else { (k % size) * size + line };
            let mut acc: f32 = (0..=2 * radius).map(|k| input[at(k + size - radius)]).sum();
            for k in 0..size {
                output[at(k)] = acc / width;
                acc += input[at(k + radius + 1)] - input[at(k + size - radius)];
            }
        }
    };
    for c in 0..3 {
        let mut blurred: Vec<f32> = field.iter().map(|p| p[c]).collect();
        let mut tmp = vec![0.0f32; size * size];
        for _ in 0..3 {
            box_pass(&blurred, &mut tmp, true);
            box_pass(&tmp, &mut blurred, false);
        }
        let mean = field.iter().map(|p| p[c] as f64).sum::<f64>() as f32 / field.len() as f32;
        for (p, b) in field.iter_mut().zip(&blurred) {
            p[c] = p[c] - b + mean;
        }
    }
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
    for (layer, path) in TILES.iter().enumerate() {
        let bytes = crate::resource::read(path).expect("load source Pizza detail from asset root");
        let mut size = 256;
        let mut field = ratios(&bytes);
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
    /// What repeats is grain: the high-pass at least halves the share of each tile's contrast
    /// held in structure broader than about half a metre, to under 6 %.
    #[test]
    fn source_detail_tiles_carry_no_broad_structure() {
        for path in super::TILES {
            let bytes = crate::resource::read(path).expect("load source Pizza detail");
            let std = |v: &[f32]| {
                let m = v.iter().sum::<f32>() / v.len() as f32;
                (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32).sqrt()
            };
            // Share of a field's contrast that the same blur keeps (broad structure).
            let broad_share = |field: &[[f32; 4]]| {
                let mut blurred_away = field.to_vec();
                super::high_pass(&mut blurred_away, 256, 24);
                let g: Vec<f32> = field.iter().map(|p| p[1]).collect();
                let broad: Vec<f32> = field.iter().zip(&blurred_away).map(|(a, b)| a[1] - b[1]).collect();
                std(&broad) / std(&g).max(1e-6)
            };
            let raw = broad_share(&super::ratios_filtered(&bytes, false));
            let filtered = broad_share(&super::ratios(&bytes));
            assert!(filtered < raw * 0.5 && filtered < 0.06, "{path}: broad structure {raw} -> {filtered}");
        }
    }

    #[test]
    fn source_detail_distant_average_is_neutral_and_finite() {
        for path in super::TILES {
            let bytes = crate::resource::read(path).expect("load source Pizza detail");
            let field = super::ratios(&bytes);
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
