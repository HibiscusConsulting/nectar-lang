//! Dependency resolver for Nectar packages.
//!
//! Performs semver-aware version resolution, detects circular dependencies, and
//! picks the highest compatible version when multiple constraints overlap.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use semver::{Version, VersionReq};

use crate::package::{NectarManifest, Dependency, collect_dependencies};
use crate::registry::RegistryClient;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A fully resolved dependency — name, exact version, and download location.
#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    pub name: String,
    pub version: Version,
    pub source: DependencySource,
    pub features: Vec<String>,
}

/// Where a resolved package comes from.
#[derive(Debug, Clone)]
pub enum DependencySource {
    /// From the registry (or local cache).
    Registry { cache_path: PathBuf },
    /// A path dependency on the local filesystem.
    Local { path: PathBuf },
}

// ---------------------------------------------------------------------------
// Resolver
// ---------------------------------------------------------------------------

/// Resolves an entire dependency graph starting from a root manifest.
pub struct Resolver<'a> {
    client: &'a RegistryClient,
}

impl<'a> Resolver<'a> {
    pub fn new(client: &'a RegistryClient) -> Self {
        Self { client }
    }

    /// Walk the dependency graph of `manifest` and return a flat, de-duplicated
    /// list of resolved dependencies in topological order.
    pub fn resolve(&self, manifest: &NectarManifest) -> Result<Vec<ResolvedDependency>> {
        let deps = collect_dependencies(manifest);

        // Track which packages we have resolved so far: name -> resolved info.
        let mut resolved: BTreeMap<String, ResolvedDependency> = BTreeMap::new();

        // Track the visitation state for cycle detection.
        let mut visiting: HashSet<String> = HashSet::new();
        let mut visited: HashSet<String> = HashSet::new();

        for dep in &deps {
            self.resolve_dep(dep, &mut resolved, &mut visiting, &mut visited)?;
        }

        // Return in sorted order (topological by name for determinism).
        Ok(resolved.into_values().collect())
    }

    // Recursive DFS resolution with cycle detection.
    fn resolve_dep(
        &self,
        dep: &Dependency,
        resolved: &mut BTreeMap<String, ResolvedDependency>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> Result<()> {
        if visited.contains(&dep.name) {
            // Already fully resolved — check version compatibility.
            if let Some(existing) = resolved.get(&dep.name) {
                let req = parse_version_req(&dep.version_req)?;
                if !req.matches(&existing.version) {
                    bail!(
                        "version conflict for `{}`: already resolved {} but {} is required",
                        dep.name,
                        existing.version,
                        dep.version_req
                    );
                }
            }
            return Ok(());
        }

        if visiting.contains(&dep.name) {
            bail!("circular dependency detected involving `{}`", dep.name);
        }

        visiting.insert(dep.name.clone());

        // Resolve this package.
        let resolved_dep = self.resolve_single(dep)?;

        // If the resolved package itself has an Nectar.toml (e.g. local path dep),
        // recurse into its transitive dependencies.
        let transitive_manifest = match &resolved_dep.source {
            DependencySource::Local { path } => {
                let manifest_path = path.join("Nectar.toml");
                if manifest_path.exists() {
                    Some(crate::package::parse_manifest(
                        &manifest_path.to_string_lossy(),
                    )?)
                } else {
                    None
                }
            }
            DependencySource::Registry { cache_path } => {
                let manifest_path = cache_path.join("Nectar.toml");
                if manifest_path.exists() {
                    Some(crate::package::parse_manifest(
                        &manifest_path.to_string_lossy(),
                    )?)
                } else {
                    None
                }
            }
        };

        if let Some(trans) = transitive_manifest {
            let trans_deps = collect_dependencies(&trans);
            for td in &trans_deps {
                self.resolve_dep(td, resolved, visiting, visited)?;
            }
        }

        visiting.remove(&dep.name);
        visited.insert(dep.name.clone());
        resolved.insert(dep.name.clone(), resolved_dep);

        Ok(())
    }

    /// Resolve a single dependency to a concrete version.
    fn resolve_single(&self, dep: &Dependency) -> Result<ResolvedDependency> {
        // Path dependency — use as-is.
        if let Some(ref local_path) = dep.path {
            let p = Path::new(local_path);
            if !p.exists() {
                bail!("path dependency `{}` not found at {}", dep.name, local_path);
            }
            // Try to read the local manifest to get the version.
            let version = local_manifest_version(p).unwrap_or_else(|| Version::new(0, 0, 0));
            return Ok(ResolvedDependency {
                name: dep.name.clone(),
                version,
                source: DependencySource::Local {
                    path: p.to_path_buf(),
                },
                features: dep.features.clone(),
            });
        }

        // Registry dependency — ask the client for available versions.
        let metadata = self.client.fetch_package_metadata(&dep.name)?;
        let req = parse_version_req(&dep.version_req)?;

        // Pick the highest version that satisfies the requirement.
        let best = pick_best_version(&metadata.versions, &req)
            .with_context(|| {
                format!(
                    "no version of `{}` satisfies requirement `{}`",
                    dep.name, dep.version_req
                )
            })?;

        // Download / ensure cached.
        let cache_path = self
            .client
            .download_package(&dep.name, &best.to_string())?;

        Ok(ResolvedDependency {
            name: dep.name.clone(),
            version: best,
            source: DependencySource::Registry { cache_path },
            features: dep.features.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Semver helpers
// ---------------------------------------------------------------------------

/// Parse a version requirement string. Supports:
/// - `"1.0"` -> `^1.0` (caret is the default, same as Cargo)
/// - `"^1.0"`, `"~1.0"`, `">=1.0"`, `"*"`, etc.
pub fn parse_version_req(req: &str) -> Result<VersionReq> {
    // The `semver` crate already handles all the common prefixes.
    let vr = VersionReq::parse(req)
        .with_context(|| format!("invalid version requirement: `{}`", req))?;
    Ok(vr)
}

/// Parse a concrete version string.
pub fn parse_version(v: &str) -> Result<Version> {
    let version = Version::parse(v)
        .with_context(|| format!("invalid version: `{}`", v))?;
    Ok(version)
}

/// Given a set of available versions, pick the highest that satisfies `req`.
pub fn pick_best_version(versions: &[String], req: &VersionReq) -> Option<Version> {
    let mut candidates: Vec<Version> = versions
        .iter()
        .filter_map(|v| Version::parse(v).ok())
        .filter(|v| req.matches(v))
        .collect();

    candidates.sort();
    candidates.into_iter().last()
}

/// Try to read the version from a local package's `Nectar.toml`.
fn local_manifest_version(path: &Path) -> Option<Version> {
    let manifest_path = path.join("Nectar.toml");
    let content = std::fs::read_to_string(manifest_path).ok()?;
    let manifest: crate::package::NectarManifest = toml::from_str(&content).ok()?;
    Version::parse(&manifest.package.version).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // parse_version_req
    // -----------------------------------------------------------------------

    #[test]
    fn parse_version_req_caret() {
        let req = parse_version_req("^1.2").unwrap();
        assert!(req.matches(&Version::new(1, 3, 0)));
        assert!(!req.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn parse_version_req_tilde() {
        let req = parse_version_req("~1.2.0").unwrap();
        assert!(req.matches(&Version::new(1, 2, 5)));
        assert!(!req.matches(&Version::new(1, 3, 0)));
    }

    #[test]
    fn parse_version_req_exact() {
        let req = parse_version_req("=1.0.0").unwrap();
        assert!(req.matches(&Version::new(1, 0, 0)));
        assert!(!req.matches(&Version::new(1, 0, 1)));
    }

    #[test]
    fn parse_version_req_wildcard() {
        let req = parse_version_req("*").unwrap();
        assert!(req.matches(&Version::new(99, 99, 99)));
    }

    #[test]
    fn parse_version_req_gte() {
        let req = parse_version_req(">=2.0.0").unwrap();
        assert!(req.matches(&Version::new(3, 0, 0)));
        assert!(!req.matches(&Version::new(1, 9, 9)));
    }

    #[test]
    fn parse_version_req_bare_version() {
        // Bare version "1.0" should be treated as "^1.0"
        let req = parse_version_req("1.0").unwrap();
        assert!(req.matches(&Version::new(1, 5, 0)));
        assert!(!req.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn parse_version_req_invalid() {
        assert!(parse_version_req("not_a_version").is_err());
    }

    // -----------------------------------------------------------------------
    // parse_version
    // -----------------------------------------------------------------------

    #[test]
    fn parse_version_valid() {
        let v = parse_version("1.2.3").unwrap();
        assert_eq!(v, Version::new(1, 2, 3));
    }

    #[test]
    fn parse_version_with_pre() {
        let v = parse_version("1.0.0-alpha.1").unwrap();
        assert_eq!(v.major, 1);
        assert!(!v.pre.is_empty());
    }

    #[test]
    fn parse_version_invalid() {
        assert!(parse_version("abc").is_err());
        assert!(parse_version("1.2").is_err());
        assert!(parse_version("").is_err());
    }

    // -----------------------------------------------------------------------
    // pick_best_version
    // -----------------------------------------------------------------------

    #[test]
    fn pick_best_version_matching() {
        let versions = vec![
            "1.0.0".to_string(),
            "1.1.0".to_string(),
            "1.2.0".to_string(),
            "2.0.0".to_string(),
        ];
        let req = VersionReq::parse("^1.0").unwrap();
        let best = pick_best_version(&versions, &req).unwrap();
        assert_eq!(best, Version::new(1, 2, 0));
    }

    #[test]
    fn pick_best_version_no_match() {
        let versions = vec!["1.0.0".to_string(), "1.1.0".to_string()];
        let req = VersionReq::parse(">=2.0.0").unwrap();
        assert!(pick_best_version(&versions, &req).is_none());
    }

    #[test]
    fn pick_best_version_empty() {
        let versions: Vec<String> = vec![];
        let req = VersionReq::parse("*").unwrap();
        assert!(pick_best_version(&versions, &req).is_none());
    }

    #[test]
    fn pick_best_version_wildcard() {
        let versions = vec![
            "0.1.0".to_string(),
            "3.0.0".to_string(),
            "2.5.0".to_string(),
        ];
        let req = VersionReq::parse("*").unwrap();
        let best = pick_best_version(&versions, &req).unwrap();
        assert_eq!(best, Version::new(3, 0, 0));
    }

    #[test]
    fn pick_best_version_with_invalid_entries() {
        let versions = vec![
            "1.0.0".to_string(),
            "not-a-version".to_string(),
            "1.5.0".to_string(),
        ];
        let req = VersionReq::parse("^1.0").unwrap();
        let best = pick_best_version(&versions, &req).unwrap();
        assert_eq!(best, Version::new(1, 5, 0));
    }

    // -----------------------------------------------------------------------
    // local_manifest_version
    // -----------------------------------------------------------------------

    #[test]
    fn local_manifest_version_reads_version() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Nectar.toml"),
            r#"
[package]
name = "local"
version = "3.2.1"
"#,
        )
        .unwrap();
        let v = local_manifest_version(tmp.path()).unwrap();
        assert_eq!(v, Version::new(3, 2, 1));
    }

    #[test]
    fn local_manifest_version_missing_manifest() {
        let tmp = TempDir::new().unwrap();
        assert!(local_manifest_version(tmp.path()).is_none());
    }

    #[test]
    fn local_manifest_version_invalid_version() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Nectar.toml"),
            r#"
[package]
name = "local"
version = "not-semver"
"#,
        )
        .unwrap();
        assert!(local_manifest_version(tmp.path()).is_none());
    }

    // -----------------------------------------------------------------------
    // Resolver with path dependencies
    // -----------------------------------------------------------------------

    fn make_registry_client(tmp: &TempDir) -> RegistryClient {
        let config = crate::registry::RegistryConfig {
            registry_url: "https://test.example".to_string(),
            cache_dir: tmp.path().join("cache"),
        };
        RegistryClient::new(config)
    }

    fn write_manifest(dir: &Path, name: &str, version: &str, deps: &str) {
        let content = format!(
            r#"[package]
name = "{name}"
version = "{version}"

[dependencies]
{deps}
"#
        );
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("Nectar.toml"), content).unwrap();
    }

    #[test]
    fn resolve_single_path_dependency() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("my-lib");
        write_manifest(&dep_dir, "my-lib", "1.0.0", "");

        let root_manifest_str = format!(
            r#"
[package]
name = "app"
version = "0.1.0"

[dependencies]
my-lib = {{ path = "{}" }}
"#,
            dep_dir.display()
        );
        let manifest: crate::package::NectarManifest =
            toml::from_str(&root_manifest_str).unwrap();

        let cache_tmp = TempDir::new().unwrap();
        let client = make_registry_client(&cache_tmp);
        let resolver = Resolver::new(&client);

        let resolved = resolver.resolve(&manifest).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "my-lib");
        assert_eq!(resolved[0].version, Version::new(1, 0, 0));
        assert!(matches!(resolved[0].source, DependencySource::Local { .. }));
    }

    #[test]
    fn resolve_path_dependency_missing_dir() {
        let root_manifest_str = r#"
[package]
name = "app"
version = "0.1.0"

[dependencies]
missing = { path = "/nonexistent/path/to/lib" }
"#;
        let manifest: crate::package::NectarManifest =
            toml::from_str(root_manifest_str).unwrap();

        let cache_tmp = TempDir::new().unwrap();
        let client = make_registry_client(&cache_tmp);
        let resolver = Resolver::new(&client);

        let result = resolver.resolve(&manifest);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not found"));
    }

    #[test]
    fn resolve_path_dep_without_manifest_gets_zero_version() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("no-manifest-lib");
        std::fs::create_dir_all(&dep_dir).unwrap();
        // No Nectar.toml -- should still resolve with 0.0.0

        let root_manifest_str = format!(
            r#"
[package]
name = "app"
version = "0.1.0"

[dependencies]
no-manifest-lib = {{ path = "{}" }}
"#,
            dep_dir.display()
        );
        let manifest: crate::package::NectarManifest =
            toml::from_str(&root_manifest_str).unwrap();

        let cache_tmp = TempDir::new().unwrap();
        let client = make_registry_client(&cache_tmp);
        let resolver = Resolver::new(&client);

        let resolved = resolver.resolve(&manifest).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].version, Version::new(0, 0, 0));
    }

    #[test]
    fn resolve_transitive_path_deps() {
        let tmp = TempDir::new().unwrap();

        // lib-b has no deps
        let lib_b_dir = tmp.path().join("lib-b");
        write_manifest(&lib_b_dir, "lib-b", "2.0.0", "");

        // lib-a depends on lib-b
        let lib_a_dir = tmp.path().join("lib-a");
        write_manifest(
            &lib_a_dir,
            "lib-a",
            "1.0.0",
            &format!("lib-b = {{ path = \"{}\" }}", lib_b_dir.display()),
        );

        // root depends on lib-a
        let root_manifest_str = format!(
            r#"
[package]
name = "root"
version = "0.1.0"

[dependencies]
lib-a = {{ path = "{}" }}
"#,
            lib_a_dir.display()
        );
        let manifest: crate::package::NectarManifest =
            toml::from_str(&root_manifest_str).unwrap();

        let cache_tmp = TempDir::new().unwrap();
        let client = make_registry_client(&cache_tmp);
        let resolver = Resolver::new(&client);

        let resolved = resolver.resolve(&manifest).unwrap();
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().any(|r| r.name == "lib-a"));
        assert!(resolved.iter().any(|r| r.name == "lib-b"));
    }

    // -----------------------------------------------------------------------
    // Cycle detection
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_detects_cycle() {
        let tmp = TempDir::new().unwrap();

        let lib_a_dir = tmp.path().join("lib-a");
        let lib_b_dir = tmp.path().join("lib-b");

        // lib-a depends on lib-b
        write_manifest(
            &lib_a_dir,
            "lib-a",
            "1.0.0",
            &format!("lib-b = {{ path = \"{}\" }}", lib_b_dir.display()),
        );
        // lib-b depends on lib-a (cycle!)
        write_manifest(
            &lib_b_dir,
            "lib-b",
            "1.0.0",
            &format!("lib-a = {{ path = \"{}\" }}", lib_a_dir.display()),
        );

        let root_manifest_str = format!(
            r#"
[package]
name = "root"
version = "0.1.0"

[dependencies]
lib-a = {{ path = "{}" }}
"#,
            lib_a_dir.display()
        );
        let manifest: crate::package::NectarManifest =
            toml::from_str(&root_manifest_str).unwrap();

        let cache_tmp = TempDir::new().unwrap();
        let client = make_registry_client(&cache_tmp);
        let resolver = Resolver::new(&client);

        let result = resolver.resolve(&manifest);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("circular dependency"));
    }

    // -----------------------------------------------------------------------
    // Resolver with registry deps (via seeded cache)
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_registry_dependency_from_cache() {
        let cache_tmp = TempDir::new().unwrap();
        let source_tmp = TempDir::new().unwrap();

        // Create a source package
        write_manifest(source_tmp.path(), "cached-lib", "1.5.0", "");

        let config = crate::registry::RegistryConfig {
            registry_url: "https://test.example".to_string(),
            cache_dir: cache_tmp.path().to_path_buf(),
        };
        let client = RegistryClient::new(config);
        client
            .cache_local_package("cached-lib", "1.5.0", source_tmp.path())
            .unwrap();

        let root_manifest_str = r#"
[package]
name = "app"
version = "0.1.0"

[dependencies]
cached-lib = "^1.0"
"#;
        let manifest: crate::package::NectarManifest =
            toml::from_str(root_manifest_str).unwrap();

        let resolver = Resolver::new(&client);
        let resolved = resolver.resolve(&manifest).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "cached-lib");
        assert_eq!(resolved[0].version, Version::new(1, 5, 0));
        assert!(matches!(
            resolved[0].source,
            DependencySource::Registry { .. }
        ));
    }

    #[test]
    fn resolve_empty_dependencies() {
        let root_manifest_str = r#"
[package]
name = "app"
version = "0.1.0"
"#;
        let manifest: crate::package::NectarManifest =
            toml::from_str(root_manifest_str).unwrap();

        let cache_tmp = TempDir::new().unwrap();
        let client = make_registry_client(&cache_tmp);
        let resolver = Resolver::new(&client);

        let resolved = resolver.resolve(&manifest).unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn resolve_duplicate_dep_compatible_version() {
        let tmp = TempDir::new().unwrap();

        let lib_dir = tmp.path().join("shared");
        write_manifest(&lib_dir, "shared", "1.5.0", "");

        // Both lib-a and lib-b depend on shared
        let lib_a_dir = tmp.path().join("lib-a");
        write_manifest(
            &lib_a_dir,
            "lib-a",
            "1.0.0",
            &format!("shared = {{ path = \"{}\" }}", lib_dir.display()),
        );
        let lib_b_dir = tmp.path().join("lib-b");
        write_manifest(
            &lib_b_dir,
            "lib-b",
            "1.0.0",
            &format!("shared = {{ path = \"{}\" }}", lib_dir.display()),
        );

        let root_manifest_str = format!(
            r#"
[package]
name = "root"
version = "0.1.0"

[dependencies]
lib-a = {{ path = "{}" }}
lib-b = {{ path = "{}" }}
"#,
            lib_a_dir.display(),
            lib_b_dir.display()
        );
        let manifest: crate::package::NectarManifest =
            toml::from_str(&root_manifest_str).unwrap();

        let cache_tmp = TempDir::new().unwrap();
        let client = make_registry_client(&cache_tmp);
        let resolver = Resolver::new(&client);

        let resolved = resolver.resolve(&manifest).unwrap();
        // shared should appear only once
        let shared_count = resolved.iter().filter(|r| r.name == "shared").count();
        assert_eq!(shared_count, 1);
        assert_eq!(resolved.len(), 3); // lib-a, lib-b, shared
    }
}
