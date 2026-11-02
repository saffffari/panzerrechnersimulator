//! Bakes the build's git identity into the binary for the window title, so a running game says
//! which commit it is: `KESSEL_BUILD_ID` = "<branch> <short hash> (<commit date>)", or "unknown"
//! when the source has no git (a packaged tree).
use std::process::Command;

fn simulation_fingerprint() -> std::io::Result<u64> {
    fn gather(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() { gather(&entry.path(), files)?; }
            else if entry.file_type()?.is_file() { files.push(entry.path()); }
        }
        Ok(())
    }
    let mut files = vec![std::path::PathBuf::from("Cargo.lock"), std::path::PathBuf::from("Cargo.toml"), std::path::PathBuf::from("build.rs")];
    for root in ["src", "shaders", "catalog"] { gather(std::path::Path::new(root), &mut files)?; }
    files.sort();
    let mut hash = 0xcbf29ce484222325_u64;
    let mut feed = |bytes: &[u8]| { for &byte in bytes { hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3); } };
    for file in files {
        println!("cargo:rerun-if-changed={}", file.display());
        let name = file.to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(&file)?;
        feed(&(name.len() as u64).to_le_bytes()); feed(name.as_bytes());
        feed(&(bytes.len() as u64).to_le_bytes()); feed(&bytes);
    }
    // Directory watches also detect added/removed source owners, not only edits to known files.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=shaders");
    println!("cargo:rerun-if-changed=catalog");
    Ok(hash)
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !text.is_empty()).then_some(text)
}

fn main() {
    let fingerprint = simulation_fingerprint().expect("fingerprint complete simulation source");
    println!("cargo:rustc-env=KESSEL_SIMULATION_FINGERPRINT={fingerprint:016x}");
    let id = match (git(&["rev-parse", "--short", "HEAD"]),
        git(&["log", "-1", "--format=%cd", "--date=format:%Y-%m-%d %H:%M"])) {
        (Some(hash), Some(date)) => {
            let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD");
            match branch {
                Some(branch) => format!("{branch} {hash} ({date})"),
                None => format!("{hash} ({date})"),
            }
        }
        _ => "unknown".to_string(),
    };
    println!("cargo:rustc-env=KESSEL_BUILD_ID={id}");
    // Rebuild the stamp on every commit or checkout of this worktree (its reflog moves with both).
    for path in ["HEAD", "logs/HEAD"] {
        if let Some(p) = git(&["rev-parse", "--git-path", path]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
