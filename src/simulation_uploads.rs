//! Render-free simulation still queues instance/vertex updates. wgpu retains their staging
//! allocations until a submission completes; polling alone does not submit pending writes.
//! Drain at a bounded cadence without drawing a scene or changing authoritative simulation.
pub fn drain(device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<()> {
    let submission = queue.submit([]);
    device.poll(wgpu::PollType::Wait {
        submission_index: Some(submission),
        timeout: Some(std::time::Duration::from_secs(30)),
    })?;
    Ok(())
}
