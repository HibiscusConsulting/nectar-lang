//! Nectar package manifest parsing and lockfile support.
//!
//! Handles `Nectar.toml` manifests (similar to Cargo.toml) and `Nectar.lock` lockfiles
//! for reproducible builds.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Nectar.toml manifest
// ---------------------------------------------------------------------------

/// Top-level manifest parsed from `Nectar.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NectarManifest {
    pub package: PackageInfo,

    #[serde(default)]
    pub dependencies: BTreeMap<String, DependencySpec>,

    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: BTreeMap<String, DependencySpec>,
}

/// The `[package]` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
}

/// A dependency can be specified as a plain version string or as a table with
/// extra fields (features, path, registry, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencySpec {
    /// Short form: `nectar-ui = "1.0"`
    Simple(String),
    /// Table form: `nectar-ui = { version = "1.0", features = ["3d"] }`
    Detailed(DetailedDependency),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedDependency {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub registry_url: Option<String>,
}

/// Normalized dependency information used by the resolver.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version_req: String,
    pub features: Vec<String>,
    pub registry_url: Option<String>,
    /// If set, the dependency lives on the local filesystem.
    pub path: Option<String>,
}

impl Dependency {
    /// Convert a `(name, DependencySpec)` pair into a `Dependency`.
    pub fn from_spec(name: &str, spec: &DependencySpec) -> Self {
        match spec {
            DependencySpec::Simple(version) => Dependency {
                name: name.to_string(),
                version_req: version.clone(),
                features: Vec::new(),
                registry_url: None,
                path: None,
            },
            DependencySpec::Detailed(d) => Dependency {
                name: name.to_string(),
                version_req: d.version.clone().unwrap_or_else(|| "*".to_string()),
                features: d.features.clone(),
                registry_url: d.registry_url.clone(),
                path: d.path.clone(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Nectar.lock lockfile
// ---------------------------------------------------------------------------

/// Lockfile that pins exact resolved versions for reproducible builds.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NectarLockfile {
    /// Lockfile format version.
    #[serde(default = "default_lock_version")]
    pub version: u32,

    #[serde(default, rename = "package")]
    pub packages: Vec<LockedPackage>,
}

fn default_lock_version() -> u32 {
    1
}

/// A single locked package entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse an `Nectar.toml` manifest from a file path.
pub fn parse_manifest(path: &str) -> Result<NectarManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest at {}", path))?;
    let manifest: NectarManifest =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path))?;
    Ok(manifest)
}

/// Parse an `Nectar.lock` lockfile from a file path.  Returns `Ok(None)` if the
/// file does not exist.
pub fn parse_lockfile(path: &str) -> Result<Option<NectarLockfile>> {
    let p = Path::new(path);
    if !p.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(p)
        .with_context(|| format!("failed to read lockfile at {}", path))?;
    let lockfile: NectarLockfile =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path))?;
    Ok(Some(lockfile))
}

/// Serialize an `NectarLockfile` and write it to disk.
pub fn write_lockfile(path: &str, lockfile: &NectarLockfile) -> Result<()> {
    let content =
        toml::to_string_pretty(lockfile).context("failed to serialize lockfile")?;
    fs::write(path, content).with_context(|| format!("failed to write lockfile to {}", path))?;
    Ok(())
}

/// Return a list of `Dependency` values from the manifest (regular deps only).
pub fn collect_dependencies(manifest: &NectarManifest) -> Vec<Dependency> {
    manifest
        .dependencies
        .iter()
        .map(|(name, spec)| Dependency::from_spec(name, spec))
        .collect()
}

/// Return a list of `Dependency` values from the manifest (dev deps only).
pub fn collect_dev_dependencies(manifest: &NectarManifest) -> Vec<Dependency> {
    manifest
        .dev_dependencies
        .iter()
        .map(|(name, spec)| Dependency::from_spec(name, spec))
        .collect()
}

/// Generate a default `Nectar.toml` for `nectar init`.
pub fn default_manifest(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"

[dependencies]

[dev-dependencies]
"#
    )
}
