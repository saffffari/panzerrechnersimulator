//! Test bridge for the tracked semantic runtime-asset catalog and offline cook.

#[cfg(test)]
mod tests {
    #[test]
    fn python_runtime_asset_cook_suite_is_offline_and_deterministic() {
        let executable = if cfg!(windows) { "python" } else { "python3" };
        let output = std::process::Command::new(executable)
            .arg("tools/test_runtime_asset_cook.py")
            .output()
            .expect("Python is required by Panzerrechnersystem's existing worldgen toolchain");
        assert!(
            output.status.success(),
            "runtime asset cook regression suite failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}
