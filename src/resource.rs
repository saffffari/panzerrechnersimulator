//! Finder-safe runtime resource discovery and path mapping.
//!
//! Development keeps using repository-relative paths. A packaged build instead discovers
//! `Kessel.app/Contents/Resources` from the executable, and an explicit `KESSEL_ASSET_ROOT`
//! override can point at either a repository/resources root or directly at an `assets` directory.
//! No mode changes the process working directory.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{anyhow, bail, ensure, Context};

pub const ASSET_ROOT_ENV: &str = "KESSEL_ASSET_ROOT";
const RUNTIME_MANIFEST: &str = "runtime-assets.manifest.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayoutKind {
    Override,
    Packaged,
    Development,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Layout {
    /// Base for ordinary repository-relative resources outside `assets/`.
    resource_root: PathBuf,
    /// Base replacing the leading `assets/` component.
    assets_root: PathBuf,
    kind: LayoutKind,
}

impl Layout {
    fn from_root_shape(root: PathBuf, has_assets_child: bool, kind: LayoutKind) -> Self {
        if has_assets_child {
            Self {
                assets_root: root.join("assets"),
                resource_root: root,
                kind,
            }
        } else {
            // `KESSEL_ASSET_ROOT=/some/assets` is intentionally supported. In this form both
            // ordinary relative resource paths and `assets/...` paths are rooted here.
            Self {
                assets_root: root.clone(),
                resource_root: root,
                kind,
            }
        }
    }
}

#[derive(Debug)]
struct ResourceResolver {
    layout: Layout,
    /// Catalog source path (`assets/...glb`) -> cooked path (`models/<stable-id>.glb`).
    cooked_models: HashMap<String, PathBuf>,
}

impl ResourceResolver {
    fn discover() -> anyhow::Result<Self> {
        let cwd = std::env::current_dir().context("discover current working directory")?;

        if let Some(raw) = std::env::var_os(ASSET_ROOT_ENV).filter(|value| !value.is_empty()) {
            let requested = PathBuf::from(raw);
            let root = if requested.is_absolute() {
                requested
            } else {
                cwd.join(requested)
            };
            ensure!(
                root.is_dir(),
                "{ASSET_ROOT_ENV} does not name a directory: {:?}",
                root
            );
            return Self::from_layout(Layout::from_root_shape(
                root.clone(),
                root.join("assets").is_dir(),
                LayoutKind::Override,
            ));
        }

        let executable = std::env::current_exe().context("discover current executable")?;
        if let Some(resources) = packaged_resources_for_executable(&executable) {
            // Once the executable has the .app/Contents/MacOS shape, do not silently fall back to
            // an unrelated working directory. A broken bundle should fail as a broken bundle.
            ensure!(
                resources.is_dir(),
                "packaged Resources directory is absent beside executable {:?}: {:?}",
                executable,
                resources
            );
            ensure!(
                resources.join("assets").is_dir(),
                "packaged asset root is absent: {:?}",
                resources.join("assets")
            );
            return Self::from_layout(Layout::from_root_shape(
                resources,
                true,
                LayoutKind::Packaged,
            ));
        }

        if let Some(repo) = find_development_root(&cwd)
            .or_else(|| executable.parent().and_then(find_development_root))
        {
            return Self::from_layout(Layout::from_root_shape(repo, true, LayoutKind::Development));
        }

        bail!(
            "could not discover Panzerrechnersystem resources from cwd {:?} or executable {:?}; set \
             {ASSET_ROOT_ENV} to the repository, Resources directory, or assets directory",
            cwd,
            executable
        )
    }

    fn from_layout(layout: Layout) -> anyhow::Result<Self> {
        let manifest_path = layout.assets_root.join(RUNTIME_MANIFEST);
        ensure!(
            layout.kind != LayoutKind::Packaged || manifest_path.is_file(),
            "packaged runtime asset manifest is absent: {manifest_path:?}"
        );
        let cooked_models = if manifest_path.is_file() {
            let bytes = fs::read(&manifest_path)
                .with_context(|| format!("read runtime asset manifest {manifest_path:?}"))?;
            parse_runtime_mapping(&bytes)
                .with_context(|| format!("parse runtime asset manifest {manifest_path:?}"))?
        } else {
            HashMap::new()
        };
        println!(
            "[panzerrechnersystem] resources: {:?} root={} assets={} cooked_models={}",
            layout.kind,
            layout.resource_root.display(),
            layout.assets_root.display(),
            cooked_models.len()
        );
        Ok(Self {
            layout,
            cooked_models,
        })
    }

    fn resolve(&self, logical: &Path) -> anyhow::Result<PathBuf> {
        if logical.is_absolute() {
            return Ok(logical.to_owned());
        }
        let key = normalized_relative(logical)?;

        if let Some(cooked) = self.cooked_models.get(&key) {
            return Ok(self.layout.assets_root.join(cooked));
        }

        let normalized = Path::new(&key);
        if let Ok(without_assets) = normalized.strip_prefix("assets") {
            if !self.cooked_models.is_empty()
                && normalized
                    .extension()
                    .and_then(OsStr::to_str)
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("glb"))
            {
                bail!(
                    "runtime GLB is absent from {RUNTIME_MANIFEST}: {key}; update and recook the \
                     reviewed runtime catalog"
                );
            }
            return Ok(self.layout.assets_root.join(without_assets));
        }

        // Runtime paths from the cooked manifest are also accepted directly. This is useful for
        // diagnostics/tools while source callers continue to use their stable `assets/...` names.
        if !self.cooked_models.is_empty() && normalized.starts_with("models") {
            return Ok(self.layout.assets_root.join(normalized));
        }

        Ok(self.layout.resource_root.join(normalized))
    }
}

static RESOLVER: OnceLock<Result<ResourceResolver, String>> = OnceLock::new();

fn resolver() -> anyhow::Result<&'static ResourceResolver> {
    match RESOLVER
        .get_or_init(|| ResourceResolver::discover().map_err(|error| format!("{error:#}")))
    {
        Ok(resolver) => Ok(resolver),
        Err(message) => Err(anyhow!(message.clone())),
    }
}

/// Resolve a logical resource path without changing the process current directory.
pub fn resolve(path: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let resolved = resolver()?.resolve(path.as_ref());
    if let Ok(physical) = &resolved {
        trace(path.as_ref(), physical);
    }
    resolved
}

/// `KESSEL_ASSET_TRACE=FILE`: append every distinct logical path this process resolves, with
/// whether it exists, one `present|absent<TAB>path` line each. `tools/windows_payload.py --trace`
/// adds the present ones to a package closure, so what a play session actually loads cannot be
/// missing from the copy. Off (one env read) unless set.
fn trace(logical: &Path, physical: &Path) {
    use std::io::Write;
    use std::sync::Mutex;
    static TRACE: OnceLock<Option<Mutex<(File, HashSet<PathBuf>)>>> = OnceLock::new();
    let Some(sink) = TRACE.get_or_init(|| {
        let path = std::env::var_os("KESSEL_ASSET_TRACE").filter(|value| !value.is_empty())?;
        let file = fs::OpenOptions::new().create(true).append(true).open(path).ok()?;
        Some(Mutex::new((file, HashSet::new())))
    }) else {
        return;
    };
    let Ok(mut sink) = sink.lock() else { return };
    let (file, seen) = &mut *sink;
    if seen.insert(logical.to_owned()) {
        let state = if physical.exists() { "present" } else { "absent" };
        let _ = writeln!(file, "{state}\t{}", logical.to_string_lossy().replace('\\', "/"));
    }
}

/// Read a logical resource into memory, reporting both its logical and resolved names on failure.
pub fn read(path: impl AsRef<Path>) -> anyhow::Result<Vec<u8>> {
    let logical = path.as_ref();
    let physical = resolve(logical)?;
    fs::read(&physical)
        .with_context(|| format!("read resource {logical:?} (resolved to {physical:?})"))
}

/// Read a UTF-8 logical resource.
pub fn read_to_string(path: impl AsRef<Path>) -> anyhow::Result<String> {
    let logical = path.as_ref();
    let physical = resolve(logical)?;
    fs::read_to_string(&physical)
        .with_context(|| format!("read resource {logical:?} (resolved to {physical:?})"))
}

/// Open a logical resource for streaming decoders.
pub fn open(path: impl AsRef<Path>) -> anyhow::Result<File> {
    let logical = path.as_ref();
    let physical = resolve(logical)?;
    File::open(&physical)
        .with_context(|| format!("open resource {logical:?} (resolved to {physical:?})"))
}

/// True when a logical resource resolves to a regular file. Mapping/discovery errors return false;
/// required callers should use [`resolve`] or [`read`] to retain the detailed error.
pub fn is_file(path: impl AsRef<Path>) -> bool {
    resolve(path).is_ok_and(|physical| physical.is_file())
}

/// True when a logical resource resolves to any filesystem entry.
pub fn exists(path: impl AsRef<Path>) -> bool {
    resolve(path).is_ok_and(|physical| physical.exists())
}

/// Written by `tools/windows_payload.py` at the root of the portable Windows folder.
const WINDOWS_PAYLOAD_MARKER: &str = "payload.windows.v1.json";

fn find_development_root(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|candidate| {
        // A checkout, or the portable Windows folder (same logical layout, no Cargo.toml), so a
        // friend who double-clicks the exe without the launcher still finds the assets.
        ((candidate.join("Cargo.toml").is_file() || candidate.join(WINDOWS_PAYLOAD_MARKER).is_file())
            && candidate.join("assets").is_dir())
            .then(|| candidate.to_owned())
    })
}

/// Return `Contents/Resources` only for the canonical `.app/Contents/MacOS/<executable>` shape.
fn packaged_resources_for_executable(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    if macos.file_name()? != OsStr::new("MacOS") {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name()? != OsStr::new("Contents") {
        return None;
    }
    Some(contents.join("Resources"))
}

fn normalized_relative(path: &Path) -> anyhow::Result<String> {
    ensure!(
        !path.as_os_str().is_empty(),
        "resource path may not be empty"
    );
    ensure!(
        !path.is_absolute(),
        "resource path must be relative: {path:?}"
    );
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| anyhow!("resource path is not UTF-8: {path:?}"))?;
                parts.push(part);
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("resource path may not escape its root: {path:?}")
            }
        }
    }
    ensure!(!parts.is_empty(), "resource path may not be empty");
    Ok(parts.join("/"))
}

fn parse_runtime_mapping(bytes: &[u8]) -> anyhow::Result<HashMap<String, PathBuf>> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    ensure!(
        value["schema"].as_str() == Some("kessel.runtime-assets")
            && value["version"].as_u64() == Some(1),
        "unsupported runtime asset manifest schema/version"
    );
    let assets = value["assets"]
        .as_array()
        .ok_or_else(|| anyhow!("runtime asset manifest assets must be an array"))?;
    ensure!(!assets.is_empty(), "runtime asset manifest is empty");
    let expected = value["asset_count"]
        .as_u64()
        .ok_or_else(|| anyhow!("runtime asset manifest asset_count must be an integer"))?;
    ensure!(
        expected as usize == assets.len(),
        "runtime asset manifest count is inconsistent"
    );
    ensure!(
        value["missing_dependencies"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "runtime asset manifest has missing external dependencies"
    );

    let mut mapping = HashMap::with_capacity(assets.len());
    let mut runtime_paths = HashSet::with_capacity(assets.len());
    for (index, asset) in assets.iter().enumerate() {
        let source = asset["source"]
            .as_str()
            .ok_or_else(|| anyhow!("runtime assets[{index}].source must be a string"))?;
        let runtime = asset["path"]
            .as_str()
            .ok_or_else(|| anyhow!("runtime assets[{index}].path must be a string"))?;
        let source_key = normalized_relative(Path::new(source))?;
        let runtime_key = normalized_relative(Path::new(runtime))?;
        ensure!(
            source_key.starts_with("assets/") && source_key.ends_with(".glb"),
            "runtime assets[{index}] source is not an assets/...glb path: {source_key}"
        );
        ensure!(
            runtime_key.starts_with("models/") && runtime_key.ends_with(".glb"),
            "runtime assets[{index}] path is not a models/...glb path: {runtime_key}"
        );
        ensure!(
            runtime_paths.insert(runtime_key.clone()),
            "duplicate cooked runtime path: {runtime_key}"
        );
        ensure!(
            mapping
                .insert(source_key.clone(), PathBuf::from(runtime_key))
                .is_none(),
            "duplicate runtime source mapping: {source_key}"
        );
    }
    Ok(mapping)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packaged(mapping: HashMap<String, PathBuf>) -> ResourceResolver {
        ResourceResolver {
            layout: Layout::from_root_shape(
                PathBuf::from("/Applications/Kessel.app/Contents/Resources"),
                true,
                LayoutKind::Packaged,
            ),
            cooked_models: mapping,
        }
    }

    #[test]
    fn finder_bundle_discovery_is_structural_and_does_not_depend_on_cwd() {
        assert_eq!(
            packaged_resources_for_executable(Path::new(
                "/Applications/Kessel.app/Contents/MacOS/kessel"
            )),
            Some(PathBuf::from("/Applications/Kessel.app/Contents/Resources"))
        );
        assert_eq!(
            packaged_resources_for_executable(Path::new("/tmp/target/release/kessel")),
            None
        );
    }

    #[test]
    fn override_accepts_a_root_or_the_assets_directory_itself() {
        let root =
            Layout::from_root_shape(PathBuf::from("/work/kessel"), true, LayoutKind::Override);
        assert_eq!(root.resource_root, Path::new("/work/kessel"));
        assert_eq!(root.assets_root, Path::new("/work/kessel/assets"));

        let assets = Layout::from_root_shape(
            PathBuf::from("/runtime/assets"),
            false,
            LayoutKind::Override,
        );
        assert_eq!(assets.resource_root, Path::new("/runtime/assets"));
        assert_eq!(assets.assets_root, Path::new("/runtime/assets"));
    }

    #[test]
    fn runtime_manifest_maps_source_glbs_but_preserves_other_resources() {
        let mapping = parse_runtime_mapping(
            br#"{
                "schema":"kessel.runtime-assets",
                "version":1,
                "asset_count":1,
                "missing_dependencies":[],
                "assets":[{
                    "source":"assets/pizza/Buildings/house.glb",
                    "path":"models/pizza.house.glb"
                }]
            }"#,
        )
        .unwrap();
        let resolver = packaged(mapping);
        assert_eq!(
            resolver
                .resolve(Path::new("assets/pizza/Buildings/house.glb"))
                .unwrap(),
            PathBuf::from(
                "/Applications/Kessel.app/Contents/Resources/assets/models/pizza.house.glb"
            )
        );
        assert_eq!(
            resolver
                .resolve(Path::new("assets/worldgen/act1/heights.npy"))
                .unwrap(),
            PathBuf::from(
                "/Applications/Kessel.app/Contents/Resources/assets/worldgen/act1/heights.npy"
            )
        );
        assert_eq!(
            resolver
                .resolve(Path::new("models/pizza.house.glb"))
                .unwrap(),
            PathBuf::from(
                "/Applications/Kessel.app/Contents/Resources/assets/models/pizza.house.glb"
            )
        );
    }

    #[test]
    fn packaged_uncataloged_source_glb_is_a_clear_error() {
        let mut mapping = HashMap::new();
        mapping.insert(
            "assets/cataloged.glb".to_owned(),
            PathBuf::from("models/cataloged.glb"),
        );
        let error = packaged(mapping)
            .resolve(Path::new("assets/not-cataloged.glb"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("absent from runtime-assets.manifest.json"));
    }

    #[test]
    fn absolute_paths_are_preserved_and_relative_escape_is_rejected() {
        let resolver = packaged(HashMap::new());
        assert_eq!(
            resolver.resolve(Path::new("/tmp/custom.glb")).unwrap(),
            PathBuf::from("/tmp/custom.glb")
        );
        assert!(resolver.resolve(Path::new("../secret")).is_err());
    }
}
