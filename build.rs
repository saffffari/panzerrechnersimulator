//! Bakes the build's git identity into the binary for the window title, so a running game says
//! which commit it is: `KESSEL_BUILD_ID` = "<branch> <short hash> (<commit date>)", or "unknown"
//! when the source has no git (a packaged tree).
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !text.is_empty()).then_some(text)
}

fn main() {
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
