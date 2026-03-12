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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // NectarManifest deserialization
    // -----------------------------------------------------------------------

    #[test]
    fn deserialize_manifest_simple_deps() {
        let toml_str = r#"
[package]
name = "my-app"
version = "1.0.0"

[dependencies]
foo = "0.2"
bar = "3.1.0"
"#;
        let manifest: NectarManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.package.name, "my-app");
        assert_eq!(manifest.package.version, "1.0.0");
        assert_eq!(manifest.dependencies.len(), 2);
        assert!(matches!(
            manifest.dependencies.get("foo").unwrap(),
            DependencySpec::Simple(v) if v == "0.2"
        ));
        assert!(manifest.dev_dependencies.is_empty());
    }

    #[test]
    fn deserialize_manifest_detailed_deps() {
        let toml_str = r#"
[package]
name = "my-app"
version = "0.1.0"

[dependencies]
nectar-ui = { version = "1.0", features = ["3d", "animation"] }
local-lib = { path = "../local-lib" }
"#;
        let manifest: NectarManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.dependencies.len(), 2);
        match manifest.dependencies.get("nectar-ui").unwrap() {
            DependencySpec::Detailed(d) => {
                assert_eq!(d.version.as_deref(), Some("1.0"));
                assert_eq!(d.features, vec!["3d", "animation"]);
                assert!(d.path.is_none());
            }
            _ => panic!("expected Detailed"),
        }
        match manifest.dependencies.get("local-lib").unwrap() {
            DependencySpec::Detailed(d) => {
                assert_eq!(d.path.as_deref(), Some("../local-lib"));
                assert!(d.version.is_none());
            }
            _ => panic!("expected Detailed"),
        }
    }

    #[test]
    fn deserialize_manifest_empty_deps() {
        let toml_str = r#"
[package]
name = "empty"
version = "0.0.1"
"#;
        let manifest: NectarManifest = toml::from_str(toml_str).unwrap();
        assert!(manifest.dependencies.is_empty());
        assert!(manifest.dev_dependencies.is_empty());
    }

    // -----------------------------------------------------------------------
    // Dependency::from_spec
    // -----------------------------------------------------------------------

    #[test]
    fn dependency_from_spec_simple() {
        let spec = DependencySpec::Simple("1.2.3".to_string());
        let dep = Dependency::from_spec("foo", &spec);
        assert_eq!(dep.name, "foo");
        assert_eq!(dep.version_req, "1.2.3");
        assert!(dep.features.is_empty());
        assert!(dep.registry_url.is_none());
        assert!(dep.path.is_none());
    }

    #[test]
    fn dependency_from_spec_detailed() {
        let spec = DependencySpec::Detailed(DetailedDependency {
            version: Some(">=2.0".to_string()),
            features: vec!["ssl".to_string()],
            path: None,
            registry_url: Some("https://custom.reg".to_string()),
        });
        let dep = Dependency::from_spec("bar", &spec);
        assert_eq!(dep.name, "bar");
        assert_eq!(dep.version_req, ">=2.0");
        assert_eq!(dep.features, vec!["ssl"]);
        assert_eq!(dep.registry_url.as_deref(), Some("https://custom.reg"));
        assert!(dep.path.is_none());
    }

    #[test]
    fn dependency_from_spec_detailed_no_version() {
        let spec = DependencySpec::Detailed(DetailedDependency {
            version: None,
            features: vec![],
            path: Some("/some/path".to_string()),
            registry_url: None,
        });
        let dep = Dependency::from_spec("local", &spec);
        assert_eq!(dep.version_req, "*");
        assert_eq!(dep.path.as_deref(), Some("/some/path"));
    }

    // -----------------------------------------------------------------------
    // parse_manifest
    // -----------------------------------------------------------------------

    #[test]
    fn parse_manifest_from_file() {
        let tmp = TempDir::new().unwrap();
        let manifest_path = tmp.path().join("Nectar.toml");
        std::fs::write(
            &manifest_path,
            r#"
[package]
name = "test-pkg"
version = "0.5.0"
description = "A test"
authors = ["Alice"]
license = "MIT"

[dependencies]
dep-a = "1.0"

[dev-dependencies]
dep-b = { version = "2.0", features = ["test"] }
"#,
        )
        .unwrap();

        let manifest = parse_manifest(manifest_path.to_str().unwrap()).unwrap();
        assert_eq!(manifest.package.name, "test-pkg");
        assert_eq!(manifest.package.description.as_deref(), Some("A test"));
        assert_eq!(manifest.package.authors, vec!["Alice"]);
        assert_eq!(manifest.package.license.as_deref(), Some("MIT"));
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(manifest.dev_dependencies.len(), 1);
    }

    #[test]
    fn parse_manifest_missing_file() {
        let result = parse_manifest("/nonexistent/Nectar.toml");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // parse_lockfile / write_lockfile round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn parse_lockfile_nonexistent_returns_none() {
        let result = parse_lockfile("/nonexistent/Nectar.lock").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_lockfile_existing() {
        let tmp = TempDir::new().unwrap();
        let lock_path = tmp.path().join("Nectar.lock");
        std::fs::write(
            &lock_path,
            r#"
version = 1

[[package]]
name = "foo"
version = "1.0.0"
source = "registry"
checksum = "abc123"
dependencies = ["bar"]
"#,
        )
        .unwrap();

        let lockfile = parse_lockfile(lock_path.to_str().unwrap())
            .unwrap()
            .expect("lockfile should exist");
        assert_eq!(lockfile.version, 1);
        assert_eq!(lockfile.packages.len(), 1);
        assert_eq!(lockfile.packages[0].name, "foo");
        assert_eq!(lockfile.packages[0].version, "1.0.0");
        assert_eq!(lockfile.packages[0].source.as_deref(), Some("registry"));
        assert_eq!(lockfile.packages[0].checksum.as_deref(), Some("abc123"));
        assert_eq!(lockfile.packages[0].dependencies, vec!["bar"]);
    }

    #[test]
    fn write_and_read_lockfile_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let lock_path = tmp.path().join("Nectar.lock");

        let lockfile = NectarLockfile {
            version: 1,
            packages: vec![
                LockedPackage {
                    name: "alpha".to_string(),
                    version: "2.0.0".to_string(),
                    source: Some("registry".to_string()),
                    checksum: Some("deadbeef".to_string()),
                    dependencies: vec!["beta".to_string()],
                },
                LockedPackage {
                    name: "beta".to_string(),
                    version: "0.1.0".to_string(),
                    source: None,
                    checksum: None,
                    dependencies: vec![],
                },
            ],
        };

        write_lockfile(lock_path.to_str().unwrap(), &lockfile).unwrap();
        let read_back = parse_lockfile(lock_path.to_str().unwrap())
            .unwrap()
            .expect("should read back");
        assert_eq!(read_back.version, 1);
        assert_eq!(read_back.packages.len(), 2);
        assert_eq!(read_back.packages[0].name, "alpha");
        assert_eq!(read_back.packages[1].name, "beta");
    }

    // -----------------------------------------------------------------------
    // collect_dependencies / collect_dev_dependencies
    // -----------------------------------------------------------------------

    #[test]
    fn collect_deps_and_dev_deps() {
        let toml_str = r#"
[package]
name = "app"
version = "1.0.0"

[dependencies]
foo = "1.0"
bar = { version = "2.0", features = ["x"] }

[dev-dependencies]
test-lib = "0.1"
"#;
        let manifest: NectarManifest = toml::from_str(toml_str).unwrap();

        let deps = collect_dependencies(&manifest);
        assert_eq!(deps.len(), 2);
        let bar = deps.iter().find(|d| d.name == "bar").unwrap();
        assert_eq!(bar.version_req, "2.0");
        assert_eq!(bar.features, vec!["x"]);

        let dev_deps = collect_dev_dependencies(&manifest);
        assert_eq!(dev_deps.len(), 1);
        assert_eq!(dev_deps[0].name, "test-lib");
        assert_eq!(dev_deps[0].version_req, "0.1");
    }

    #[test]
    fn collect_deps_empty() {
        let toml_str = r#"
[package]
name = "empty"
version = "0.0.1"
"#;
        let manifest: NectarManifest = toml::from_str(toml_str).unwrap();
        assert!(collect_dependencies(&manifest).is_empty());
        assert!(collect_dev_dependencies(&manifest).is_empty());
    }

    // -----------------------------------------------------------------------
    // default_manifest
    // -----------------------------------------------------------------------

    #[test]
    fn default_manifest_output() {
        let output = default_manifest("my-project");
        assert!(output.contains("name = \"my-project\""));
        assert!(output.contains("version = \"0.1.0\""));
        assert!(output.contains("[dependencies]"));
        assert!(output.contains("[dev-dependencies]"));
    }

    #[test]
    fn default_manifest_is_valid_toml() {
        let output = default_manifest("valid-pkg");
        let manifest: NectarManifest = toml::from_str(&output).unwrap();
        assert_eq!(manifest.package.name, "valid-pkg");
        assert_eq!(manifest.package.version, "0.1.0");
    }

    // -----------------------------------------------------------------------
    // default_lock_version
    // -----------------------------------------------------------------------

    #[test]
    fn default_lockfile_version() {
        let lockfile = NectarLockfile::default();
        assert_eq!(lockfile.version, 0); // Default trait gives 0, serde default gives 1
        assert!(lockfile.packages.is_empty());
    }

    #[test]
    fn lockfile_deserialize_without_version_uses_default() {
        let toml_str = r#"
[[package]]
name = "x"
version = "1.0.0"
"#;
        let lockfile: NectarLockfile = toml::from_str(toml_str).unwrap();
        assert_eq!(lockfile.version, 1); // default_lock_version()
    }
}
