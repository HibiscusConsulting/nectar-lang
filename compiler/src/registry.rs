//! Registry client for Nectar packages.
//!
//! Currently supports local (path) dependencies and a stub for a future
//! HTTP-based registry. Downloaded packages are cached under `~/.nectar/cache/`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Metadata about a package as returned by the registry.
#[derive(Debug, Clone)]
pub struct PackageMetadata {
    pub name: String,
    /// All published version strings (e.g. `["1.0.0", "1.1.0", "2.0.0"]`).
    pub versions: Vec<String>,
    pub description: Option<String>,
}

/// Configuration for the registry client.
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Base URL of the Nectar package registry.
    pub registry_url: String,
    /// Local cache directory (defaults to `~/.nectar/cache`).
    pub cache_dir: PathBuf,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        let cache_dir = dirs::home_dir()
            .map(|h| h.join(".nectar").join("cache"))
            .unwrap_or_else(|| PathBuf::from(".nectar/cache"));
        Self {
            registry_url: "https://registry.nectarlang.org".to_string(),
            cache_dir,
        }
    }
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Client for interacting with the Nectar package registry and local cache.
pub struct RegistryClient {
    config: RegistryConfig,
}

impl RegistryClient {
    pub fn new(config: RegistryConfig) -> Self {
        Self { config }
    }

    /// Create a client with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(RegistryConfig::default())
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Fetch package metadata from the registry.
    ///
    /// In the current implementation this checks the local cache index first.
    /// A future version will HTTP GET `{registry_url}/api/v1/packages/{name}`.
    pub fn fetch_package_metadata(&self, name: &str) -> Result<PackageMetadata> {
        // 1. Check local index cache.
        if let Some(meta) = self.read_cached_index(name)? {
            return Ok(meta);
        }

        // 2. In the future this would perform an HTTP request:
        //    GET {self.config.registry_url}/api/v1/packages/{name}
        //
        //    For now, return an error indicating the registry is not yet
        //    available, guiding the user toward path dependencies.
        bail!(
            "package `{}` not found in local cache and the remote registry \
             ({}) is not yet available.\n\
             Hint: use a path dependency instead:\n  \
             [dependencies]\n  \
             {} = {{ path = \"../{}\" }}",
            name,
            self.config.registry_url,
            name,
            name,
        )
    }

    /// Download (or locate in cache) a specific version of a package.
    ///
    /// Returns the path to the cached package directory.
    pub fn download_package(&self, name: &str, version: &str) -> Result<PathBuf> {
        let pkg_dir = self.package_cache_path(name, version);

        if pkg_dir.exists() {
            // Already cached.
            return Ok(pkg_dir);
        }

        // Future: HTTP GET the tarball, verify checksum, extract.
        //   let url = format!("{}/api/v1/packages/{}/{}/download",
        //       self.config.registry_url, name, version);
        //   let bytes = http_get(&url)?;
        //   verify_checksum(&bytes, expected)?;
        //   extract_tar_gz(&bytes, &pkg_dir)?;

        bail!(
            "package `{}@{}` is not cached and the remote registry is not yet available",
            name,
            version,
        )
    }

    /// Return the cache path for a given package + version.
    pub fn package_cache_path(&self, name: &str, version: &str) -> PathBuf {
        self.config.cache_dir.join(name).join(version)
    }

    /// Ensure the cache directory exists.
    pub fn ensure_cache_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.config.cache_dir)
            .with_context(|| {
                format!(
                    "failed to create cache directory at {}",
                    self.config.cache_dir.display()
                )
            })?;
        Ok(())
    }

    /// Install a local package into the cache (useful for `nectar publish` in the
    /// future or for seeding the cache in tests).
    pub fn cache_local_package(
        &self,
        name: &str,
        version: &str,
        source_dir: &Path,
    ) -> Result<PathBuf> {
        let dest = self.package_cache_path(name, version);
        if dest.exists() {
            return Ok(dest);
        }
        fs::create_dir_all(&dest)?;

        // Copy all files from source_dir into the cache directory.
        copy_dir_recursive(source_dir, &dest)
            .with_context(|| format!("failed to cache package {} from {}", name, source_dir.display()))?;

        // Write a metadata index entry so `fetch_package_metadata` finds it.
        self.write_cached_index(name, version)?;

        Ok(dest)
    }

    // -----------------------------------------------------------------------
    // Index helpers
    // -----------------------------------------------------------------------

    /// Read the local index file for a package. The index is a simple text file
    /// at `~/.nectar/cache/{name}/index` with one version per line.
    fn read_cached_index(&self, name: &str) -> Result<Option<PackageMetadata>> {
        let index_path = self.config.cache_dir.join(name).join("index");
        if !index_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&index_path)?;
        let versions: Vec<String> = content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if versions.is_empty() {
            return Ok(None);
        }
        Ok(Some(PackageMetadata {
            name: name.to_string(),
            versions,
            description: None,
        }))
    }

    /// Append a version to the local index for a package.
    fn write_cached_index(&self, name: &str, version: &str) -> Result<()> {
        let index_dir = self.config.cache_dir.join(name);
        fs::create_dir_all(&index_dir)?;
        let index_path = index_dir.join("index");

        let mut versions = if index_path.exists() {
            fs::read_to_string(&index_path)?
        } else {
            String::new()
        };

        // Avoid duplicates.
        if !versions.lines().any(|l| l.trim() == version) {
            versions.push_str(version);
            versions.push('\n');
            fs::write(&index_path, versions)?;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute the SHA-256 hex digest of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}
