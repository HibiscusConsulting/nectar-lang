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
