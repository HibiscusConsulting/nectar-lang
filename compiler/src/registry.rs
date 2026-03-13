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
    #[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_client(tmp: &TempDir) -> RegistryClient {
        let config = RegistryConfig {
            registry_url: "https://test.registry.example".to_string(),
            cache_dir: tmp.path().to_path_buf(),
        };
        RegistryClient::new(config)
    }

    // -----------------------------------------------------------------------
    // RegistryConfig::default
    // -----------------------------------------------------------------------

    #[test]
    fn registry_config_default_url() {
        let config = RegistryConfig::default();
        assert_eq!(config.registry_url, "https://registry.nectarlang.org");
        // cache_dir should end with .nectar/cache
        let cache_str = config.cache_dir.to_string_lossy();
        assert!(
            cache_str.ends_with(".nectar/cache"),
            "expected cache_dir to end with .nectar/cache, got: {}",
            cache_str
        );
    }

    // -----------------------------------------------------------------------
    // RegistryClient::new / with_defaults
    // -----------------------------------------------------------------------

    #[test]
    fn client_new_stores_config() {
        let tmp = TempDir::new().unwrap();
        let client = make_client(&tmp);
        assert_eq!(client.config.registry_url, "https://test.registry.example");
    }

    #[test]
    fn client_with_defaults() {
        let client = RegistryClient::with_defaults();
        assert_eq!(
            client.config.registry_url,
            "https://registry.nectarlang.org"
        );
    }

    // -----------------------------------------------------------------------
    // package_cache_path
    // -----------------------------------------------------------------------

    #[test]
    fn package_cache_path_structure() {
        let tmp = TempDir::new().unwrap();
        let client = make_client(&tmp);
        let path = client.package_cache_path("my-pkg", "1.2.3");
        assert_eq!(path, tmp.path().join("my-pkg").join("1.2.3"));
    }

    // -----------------------------------------------------------------------
    // ensure_cache_dir
    // -----------------------------------------------------------------------

    #[test]
    fn ensure_cache_dir_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("a").join("b").join("cache");
        let config = RegistryConfig {
            registry_url: String::new(),
            cache_dir: nested.clone(),
        };
        let client = RegistryClient::new(config);
        client.ensure_cache_dir().unwrap();
        assert!(nested.exists());
    }

    // -----------------------------------------------------------------------
    // cache_local_package
    // -----------------------------------------------------------------------

    #[test]
    fn cache_local_package_copies_files() {
        let cache_tmp = TempDir::new().unwrap();
        let source_tmp = TempDir::new().unwrap();

        // Create source files
        fs::write(source_tmp.path().join("Nectar.toml"), "[package]\nname = \"x\"\nversion = \"1.0.0\"").unwrap();
        let sub = source_tmp.path().join("src");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("main.nec"), "fn main() {}").unwrap();

        let client = make_client(&cache_tmp);
        let dest = client
            .cache_local_package("x", "1.0.0", source_tmp.path())
            .unwrap();

        assert!(dest.exists());
        assert!(dest.join("Nectar.toml").exists());
        assert!(dest.join("src").join("main.nec").exists());

        // Index file should exist with the version
        let index = cache_tmp.path().join("x").join("index");
        let content = fs::read_to_string(index).unwrap();
        assert!(content.contains("1.0.0"));
    }

    #[test]
    fn cache_local_package_idempotent() {
        let cache_tmp = TempDir::new().unwrap();
        let source_tmp = TempDir::new().unwrap();
        fs::write(source_tmp.path().join("file.txt"), "hello").unwrap();

        let client = make_client(&cache_tmp);
        let dest1 = client
            .cache_local_package("pkg", "0.1.0", source_tmp.path())
            .unwrap();
        let dest2 = client
            .cache_local_package("pkg", "0.1.0", source_tmp.path())
            .unwrap();
        assert_eq!(dest1, dest2);
    }

    #[test]
    fn cache_local_package_multiple_versions() {
        let cache_tmp = TempDir::new().unwrap();
        let source_tmp = TempDir::new().unwrap();
        fs::write(source_tmp.path().join("lib.nec"), "").unwrap();

        let client = make_client(&cache_tmp);
        client
            .cache_local_package("multi", "1.0.0", source_tmp.path())
            .unwrap();
        client
            .cache_local_package("multi", "2.0.0", source_tmp.path())
            .unwrap();

        let index = cache_tmp.path().join("multi").join("index");
        let content = fs::read_to_string(index).unwrap();
        assert!(content.contains("1.0.0"));
        assert!(content.contains("2.0.0"));
    }

    // -----------------------------------------------------------------------
    // fetch_package_metadata
    // -----------------------------------------------------------------------

    #[test]
    fn fetch_package_metadata_cache_hit() {
        let cache_tmp = TempDir::new().unwrap();
        let source_tmp = TempDir::new().unwrap();
        fs::write(source_tmp.path().join("lib.nec"), "").unwrap();

        let client = make_client(&cache_tmp);
        client
            .cache_local_package("cached-pkg", "1.0.0", source_tmp.path())
            .unwrap();
        client
            .cache_local_package("cached-pkg", "1.1.0", source_tmp.path())
            .unwrap();

        let meta = client.fetch_package_metadata("cached-pkg").unwrap();
        assert_eq!(meta.name, "cached-pkg");
        assert_eq!(meta.versions.len(), 2);
        assert!(meta.versions.contains(&"1.0.0".to_string()));
        assert!(meta.versions.contains(&"1.1.0".to_string()));
    }

    #[test]
    fn fetch_package_metadata_cache_miss() {
        let cache_tmp = TempDir::new().unwrap();
        let client = make_client(&cache_tmp);
        let result = client.fetch_package_metadata("nonexistent");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("nonexistent"));
        assert!(err_msg.contains("not found"));
    }

    // -----------------------------------------------------------------------
    // download_package
    // -----------------------------------------------------------------------

    #[test]
    fn download_package_cached() {
        let cache_tmp = TempDir::new().unwrap();
        let source_tmp = TempDir::new().unwrap();
        fs::write(source_tmp.path().join("lib.nec"), "").unwrap();

        let client = make_client(&cache_tmp);
        client
            .cache_local_package("dl-pkg", "1.0.0", source_tmp.path())
            .unwrap();

        let path = client.download_package("dl-pkg", "1.0.0").unwrap();
        assert!(path.exists());
        assert_eq!(path, client.package_cache_path("dl-pkg", "1.0.0"));
    }

    #[test]
    fn download_package_not_cached() {
        let cache_tmp = TempDir::new().unwrap();
        let client = make_client(&cache_tmp);
        let result = client.download_package("missing", "1.0.0");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("missing@1.0.0"));
    }

    // -----------------------------------------------------------------------
    // sha256_hex
    // -----------------------------------------------------------------------

    #[test]
    fn sha256_hex_empty() {
        // Known SHA-256 of empty input
        let hash = sha256_hex(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_hex_hello() {
        let hash = sha256_hex(b"hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn sha256_hex_deterministic() {
        let a = sha256_hex(b"test data");
        let b = sha256_hex(b"test data");
        assert_eq!(a, b);
    }

    // -----------------------------------------------------------------------
    // read_cached_index / write_cached_index (via cache_local_package)
    // -----------------------------------------------------------------------

    #[test]
    fn empty_index_returns_none() {
        let cache_tmp = TempDir::new().unwrap();
        // Write an empty index file
        let pkg_dir = cache_tmp.path().join("empty-pkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("index"), "").unwrap();

        let client = make_client(&cache_tmp);
        let meta = client.fetch_package_metadata("empty-pkg");
        // Empty index => falls through to "not found" error
        assert!(meta.is_err());
    }

    #[test]
    fn index_with_whitespace_only_returns_none() {
        let cache_tmp = TempDir::new().unwrap();
        let pkg_dir = cache_tmp.path().join("ws-pkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("index"), "  \n\n  \n").unwrap();

        let client = make_client(&cache_tmp);
        let meta = client.fetch_package_metadata("ws-pkg");
        assert!(meta.is_err());
    }

    #[test]
    fn duplicate_version_not_written_twice() {
        let cache_tmp = TempDir::new().unwrap();
        let source_tmp = TempDir::new().unwrap();
        fs::write(source_tmp.path().join("lib.nec"), "").unwrap();

        let client = make_client(&cache_tmp);

        // Cache once
        client
            .cache_local_package("dup", "1.0.0", source_tmp.path())
            .unwrap();

        // Manually remove the cached directory so cache_local_package runs again
        let dest = client.package_cache_path("dup", "1.0.0");
        fs::remove_dir_all(&dest).unwrap();

        client
            .cache_local_package("dup", "1.0.0", source_tmp.path())
            .unwrap();

        let index = cache_tmp.path().join("dup").join("index");
        let content = fs::read_to_string(index).unwrap();
        let count = content.lines().filter(|l| l.trim() == "1.0.0").count();
        assert_eq!(count, 1, "version should appear exactly once");
    }

    // -----------------------------------------------------------------------
    // copy_dir_recursive
    // -----------------------------------------------------------------------

    #[test]
    fn copy_dir_recursive_nested() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        fs::write(src.path().join("root.txt"), "root").unwrap();
        let nested = src.path().join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("deep.txt"), "deep").unwrap();

        let dst_dir = dst.path().join("output");
        copy_dir_recursive(src.path(), &dst_dir).unwrap();

        assert_eq!(fs::read_to_string(dst_dir.join("root.txt")).unwrap(), "root");
        assert_eq!(
            fs::read_to_string(dst_dir.join("a").join("b").join("deep.txt")).unwrap(),
            "deep"
        );
    }
}
