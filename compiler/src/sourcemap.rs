//! Source map generation (v3 format) for Nectar -> WAT/WASM compilation.
//!
//! Produces a JSON source map that maps generated WebAssembly text positions
//! back to the original `.nectar` source files, enabling browser devtools to
//! show Nectar source during debugging.

use serde::Serialize;

// ---------------------------------------------------------------------------
// VLQ encoding (Base64-VLQ as specified by the source map v3 spec)
// ---------------------------------------------------------------------------

const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode a single signed integer as a Base64-VLQ string.
fn vlq_encode(value: i64) -> String {
    let mut encoded = String::new();
    // Convert to unsigned with sign in the LSB.
    let mut vlq = if value < 0 {
        ((-value) << 1) | 1
    } else {
        value << 1
    } as u64;

    loop {
        let mut digit = (vlq & 0b11111) as u8;
        vlq >>= 5;
        if vlq > 0 {
            digit |= 0b100000; // continuation bit
        }
        encoded.push(BASE64_CHARS[digit as usize] as char);
        if vlq == 0 {
            break;
        }
    }

    encoded
}

// ---------------------------------------------------------------------------
// Mapping — a single source location mapping
// ---------------------------------------------------------------------------

/// A single mapping from a generated position to a source position.
#[derive(Debug, Clone)]
pub struct Mapping {
    pub generated_line: u32,
    pub generated_col: u32,
    pub source_line: u32,
    pub source_col: u32,
    pub source_idx: u32,
    pub name_idx: Option<u32>,
}

// ---------------------------------------------------------------------------
// SourceMap
// ---------------------------------------------------------------------------

/// Accumulates mappings during codegen and serializes to Source Map v3 JSON.
#[derive(Debug)]
pub struct SourceMap {
    /// List of original source file paths.
    pub sources: Vec<String>,
    /// List of original identifier names referenced in the mappings.
    pub names: Vec<String>,
    /// All recorded mappings, in the order they were added.
    pub mappings: Vec<Mapping>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            names: Vec::new(),
            mappings: Vec::new(),
        }
    }

    /// Register a source file and return its index.
    pub fn add_source(&mut self, path: &str) -> u32 {
        if let Some(idx) = self.sources.iter().position(|s| s == path) {
            return idx as u32;
        }
        let idx = self.sources.len() as u32;
        self.sources.push(path.to_string());
        idx
    }

    /// Register a name and return its index.
    pub fn add_name(&mut self, name: &str) -> u32 {
        if let Some(idx) = self.names.iter().position(|n| n == name) {
            return idx as u32;
        }
        let idx = self.names.len() as u32;
        self.names.push(name.to_string());
        idx
    }

    /// Record a mapping from generated position to original source position.
    pub fn add_mapping(
        &mut self,
        generated_line: u32,
        generated_col: u32,
        source_line: u32,
        source_col: u32,
        source_file: &str,
    ) {
        let source_idx = self.add_source(source_file);
        self.mappings.push(Mapping {
            generated_line,
            generated_col,
            source_line,
            source_col,
            source_idx,
            name_idx: None,
        });
    }

    /// Record a mapping that also references a named identifier.
    pub fn add_mapping_with_name(
        &mut self,
        generated_line: u32,
        generated_col: u32,
        source_line: u32,
        source_col: u32,
        source_file: &str,
        name: &str,
    ) {
        let source_idx = self.add_source(source_file);
        let name_idx = self.add_name(name);
        self.mappings.push(Mapping {
            generated_line,
            generated_col,
            source_line,
            source_col,
            source_idx,
            name_idx: Some(name_idx),
        });
    }

    /// Encode all mappings into the VLQ-encoded `mappings` string used in
    /// source map v3.
    fn encode_mappings(&self) -> String {
        if self.mappings.is_empty() {
            return String::new();
        }

        // Sort mappings by generated line, then column.
        let mut sorted: Vec<&Mapping> = self.mappings.iter().collect();
        sorted.sort_by(|a, b| {
            a.generated_line
                .cmp(&b.generated_line)
                .then(a.generated_col.cmp(&b.generated_col))
        });

        let mut result = String::new();
        let mut prev_gen_line: u32 = 0;
        let mut prev_gen_col: i64 = 0;
        let mut prev_source: i64 = 0;
        let mut prev_source_line: i64 = 0;
        let mut prev_source_col: i64 = 0;
        let mut prev_name: i64 = 0;

        for mapping in &sorted {
            // Emit semicolons for skipped lines.
            while prev_gen_line < mapping.generated_line {
                result.push(';');
                prev_gen_line += 1;
                prev_gen_col = 0;
            }

            if !result.is_empty() && !result.ends_with(';') {
                result.push(',');
            }

            // Field 1: generated column (relative)
            let gen_col = mapping.generated_col as i64;
            result.push_str(&vlq_encode(gen_col - prev_gen_col));
            prev_gen_col = gen_col;

            // Field 2: source file index (relative)
            let src = mapping.source_idx as i64;
            result.push_str(&vlq_encode(src - prev_source));
            prev_source = src;

            // Field 3: source line (relative)
            let src_line = mapping.source_line as i64;
            result.push_str(&vlq_encode(src_line - prev_source_line));
            prev_source_line = src_line;

            // Field 4: source column (relative)
            let src_col = mapping.source_col as i64;
            result.push_str(&vlq_encode(src_col - prev_source_col));
            prev_source_col = src_col;

            // Field 5 (optional): name index (relative)
            if let Some(name_idx) = mapping.name_idx {
                let n = name_idx as i64;
                result.push_str(&vlq_encode(n - prev_name));
                prev_name = n;
            }
        }

        result
    }

    /// Serialize the source map to a JSON string (source map v3 format).
    pub fn to_json(&self) -> String {
        let obj = SourceMapJson {
            version: 3,
            file: String::new(),
            source_root: String::new(),
            sources: &self.sources,
            names: &self.names,
            mappings: self.encode_mappings(),
        };
        serde_json::to_string_pretty(&obj).unwrap_or_else(|_| "{}".to_string())
    }
}

#[derive(Serialize)]
struct SourceMapJson<'a> {
    version: u32,
    file: String,
    #[serde(rename = "sourceRoot")]
    source_root: String,
    sources: &'a [String],
    names: &'a [String],
    mappings: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vlq_encode_zero() {
        assert_eq!(vlq_encode(0), "A");
    }

    #[test]
    fn test_vlq_encode_positive() {
        assert_eq!(vlq_encode(1), "C");
        assert_eq!(vlq_encode(5), "K");
    }

    #[test]
    fn test_vlq_encode_negative() {
        assert_eq!(vlq_encode(-1), "D");
    }

    #[test]
    fn test_vlq_encode_large() {
        // 16 -> binary 10000 -> VLQ: needs two digits
        let encoded = vlq_encode(16);
        assert_eq!(encoded.len(), 2);
    }

    #[test]
    fn test_source_map_empty() {
        let sm = SourceMap::new();
        let json = sm.to_json();
        assert!(json.contains("\"version\": 3"));
        assert!(json.contains("\"mappings\": \"\""));
    }

    #[test]
    fn test_add_source_deduplicates() {
        let mut sm = SourceMap::new();
        let idx1 = sm.add_source("app.nectar");
        let idx2 = sm.add_source("app.nectar");
        assert_eq!(idx1, idx2);
        assert_eq!(sm.sources.len(), 1);
    }

    #[test]
    fn test_add_mapping_basic() {
        let mut sm = SourceMap::new();
        sm.add_mapping(0, 0, 0, 0, "app.nectar");
        sm.add_mapping(1, 4, 3, 2, "app.nectar");

        let json = sm.to_json();
        assert!(json.contains("\"version\": 3"));
        assert!(json.contains("\"sources\""));
        assert!(json.contains("app.nectar"));
        // Mappings should be non-empty
        assert!(!json.contains("\"mappings\": \"\""));
    }

    #[test]
    fn test_to_json_valid_structure() {
        let mut sm = SourceMap::new();
        sm.add_mapping(0, 0, 1, 0, "main.nectar");
        sm.add_mapping_with_name(0, 10, 1, 4, "main.nectar", "counter");

        let json = sm.to_json();
        // Should parse as valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["version"], 3);
        assert!(parsed["sources"].is_array());
        assert!(parsed["names"].is_array());
        assert!(parsed["mappings"].is_string());
    }
}
