//! Opt-in, fenced benchmark instrumentation. Never reads GPU data in normal gameplay.
use std::cell::{Cell, RefCell};

pub const ENV: &str = "KESSEL_GPU_PROFILE";
const MAX_PASSES: u32 = 1024;

pub fn requested() -> bool {
    std::env::var(ENV).is_ok_and(|value| value == "1")
}

pub struct GpuProfile {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    active: Cell<bool>,
    labels: RefCell<Vec<String>>,
}

impl GpuProfile {
    pub fn new(device: &wgpu::Device) -> Option<Self> {
        if !requested() || !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("benchmark-pass-timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: MAX_PASSES * 2,
        });
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: u64::from(MAX_PASSES) * 16,
                usage,
                mapped_at_creation: false,
            })
        };
        Some(Self {
            queries,
            resolve: buffer(
                "benchmark-query-resolve",
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            ),
            readback: buffer(
                "benchmark-query-readback",
                wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            ),
            active: Cell::new(false),
            labels: RefCell::new(Vec::new()),
        })
    }

    pub fn begin_frame(&self) {
        self.labels.borrow_mut().clear();
        self.active.set(true);
    }

    fn allocate(&self, label: &str) -> Option<u32> {
        if !self.active.get() {
            return None;
        }
        let mut labels = self.labels.borrow_mut();
        assert!(
            labels.len() < MAX_PASSES as usize,
            "GPU profile query capacity exceeded"
        );
        let first = labels.len() as u32 * 2;
        labels.push(label.to_owned());
        Some(first)
    }

    pub fn render(&self, label: &str) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let first = self.allocate(label)?;
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(first),
            end_of_pass_write_index: Some(first + 1),
        })
    }

    pub fn compute(&self, label: &str) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let first = self.allocate(label)?;
        Some(wgpu::ComputePassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(first),
            end_of_pass_write_index: Some(first + 1),
        })
    }

    /// Called after the benchmark's ordinary frame fence/timer, excluding readback overhead.
    /// Pass durations are GPU measurements, not CPU submit time; they need not sum to wall time.
    pub fn read_frame(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<Vec<(String, f64)>> {
        self.active.set(false);
        let labels = self.labels.borrow();
        if labels.is_empty() {
            return Ok(Vec::new());
        }
        let count = labels.len() as u32 * 2;
        let bytes = u64::from(count) * 8;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("benchmark-query-readback"),
        });
        encoder.resolve_query_set(&self.queries, 0..count, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, bytes);
        queue.submit([encoder.finish()]);
        let slice = self.readback.slice(..bytes);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let mapped = slice.get_mapped_range();
        let timestamps: &[u64] = bytemuck::cast_slice(&mapped);
        let ms_per_tick = f64::from(queue.get_timestamp_period()) / 1_000_000.0;
        let samples = labels
            .iter()
            .enumerate()
            .map(|(i, label)| {
                (
                    label.clone(),
                    timestamps[2 * i + 1].wrapping_sub(timestamps[2 * i]) as f64 * ms_per_tick,
                )
            })
            .collect();
        drop(mapped);
        self.readback.unmap();
        Ok(samples)
    }
}

/// Aggregate repeated views by pass label within each simulation frame before averaging.
#[derive(Default)]
pub struct GpuProfileSummary {
    frames: u32,
    totals: std::collections::BTreeMap<String, (f64, u64)>,
}

impl GpuProfileSummary {
    pub fn record(&mut self, samples: Vec<(String, f64)>) {
        if samples.is_empty() {
            return;
        }
        self.frames += 1;
        for (label, ms) in samples {
            let entry = self.totals.entry(label).or_default();
            entry.0 += ms;
            entry.1 += 1;
        }
    }

    pub fn report(&self) -> serde_json::Value {
        let mut rows: Vec<_> = self.totals.iter().map(|(label, (total, calls))| {
            serde_json::json!({"pass": label, "mean_ms_per_frame": total / f64::from(self.frames.max(1)), "calls_per_frame": *calls as f64 / f64::from(self.frames.max(1))})
        }).collect();
        rows.sort_by(|a, b| {
            b["mean_ms_per_frame"]
                .as_f64()
                .unwrap()
                .total_cmp(&a["mean_ms_per_frame"].as_f64().unwrap())
        });
        if self.frames > 0 {
            for row in &rows {
                println!(
                    "[gpuprof] {:<22} {:8.3} ms/frame ({:.1} passes/frame)",
                    row["pass"].as_str().unwrap(),
                    row["mean_ms_per_frame"].as_f64().unwrap(),
                    row["calls_per_frame"].as_f64().unwrap()
                );
            }
        }
        serde_json::json!({"measured_frames": self.frames, "passes": rows, "note": "GPU pass timestamps; readback excluded from frame timer; passes may overlap; scene preparation and TLAS build not timed by pass queries"})
    }
}
