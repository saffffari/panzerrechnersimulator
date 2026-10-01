//! Invalid training jobs must fail before allocating a renderer or touching active weights.
use std::process::Command;

#[test]
fn invalid_training_jobs_fail_before_gpu_initialization() {
    for args in [
        vec!["train", "0", "3", "1", "0"],
        vec!["train", "1", "0", "1", "0"],
        vec!["train", "1", "3", "NaN", "0"],
        vec!["train", "1", "3", "1", ""],
        vec!["train", "1", "3", "1", "0", "-1"],
        vec!["train", "1", "3", "1", "0", "1", "../tiger"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_panzerrechnersystem"))
            .args(&args).output().unwrap();
        assert!(!result.status.success(), "accepted invalid job: {args:?}");
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(!stdout.contains("adapter:"), "initialized GPU for {args:?}");
        assert!(!stdout.contains("wgpu instance"), "initialized GPU for {args:?}");
    }
}

#[test]
fn foot_training_refuses_existing_candidate_without_partial_write() {
    let dir = std::env::temp_dir().join(format!("panzer-foot-guard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sentinel = dir.join("weights_foot_soviet.bin");
    std::fs::write(&sentinel, b"existing candidate").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_panzerrechnersystem"))
        .args(["train-foot-ai", "1", dir.to_str().unwrap()]).output().unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"existing candidate");
    assert!(!dir.join("weights_foot_tiger.bin").exists());
    std::fs::remove_file(sentinel).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
