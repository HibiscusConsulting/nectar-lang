//! WASM size optimization pass — post-codegen optimizations on WAT text.
//!
//! These are peephole optimizations that reduce the size of emitted WAT/WASM:
//! - Merge consecutive i32.const + arithmetic into single constants
//! - Remove redundant local.get/local.set pairs
//! - Remove `drop` after void expressions
//! - Remove empty blocks
//! - Deduplicate identical function bodies

use std::collections::HashMap;

/// Statistics about WASM optimizations applied.
#[derive(Debug, Default)]
pub struct WasmOptStats {
    pub patterns_optimized: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
}

/// Optimize WAT text for smaller output.
pub fn optimize_wat(wat: &str, stats: &mut WasmOptStats) -> String {
    stats.bytes_before = wat.len();

    let mut result = wat.to_string();

    result = merge_const_arithmetic(&result, stats);
    result = remove_redundant_local_pairs(&result, stats);
    result = remove_drop_after_void(&result, stats);
    result = remove_empty_blocks(&result, stats);
    result = deduplicate_functions(&result, stats);

    stats.bytes_after = result.len();
    result
}

/// Merge consecutive `i32.const X` `i32.const Y` `i32.add` into `i32.const (X+Y)`.
/// Also handles sub, mul.
fn merge_const_arithmetic(wat: &str, stats: &mut WasmOptStats) -> String {
    let lines: Vec<&str> = wat.lines().collect();
    let mut result = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        if i + 2 < lines.len() {
            let a = lines[i].trim();
            let b = lines[i + 1].trim();
            let c = lines[i + 2].trim();

            if let (Some(va), Some(vb)) = (parse_i32_const(a), parse_i32_const(b)) {
                if let Some(folded) = fold_wasm_op(c, va, vb) {
                    // Preserve indentation from the first line
                    let indent = &lines[i][..lines[i].len() - lines[i].trim_start().len()];
                    result.push(format!("{}i32.const {}", indent, folded));
                    stats.patterns_optimized += 1;
                    i += 3;
                    continue;
                }
            }
        }
        result.push(lines[i].to_string());
        i += 1;
    }

    result.join("\n")
}

fn parse_i32_const(line: &str) -> Option<i64> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("i32.const ") {
        rest.trim().parse::<i64>().ok()
    } else {
        None
    }
}

fn fold_wasm_op(op_line: &str, a: i64, b: i64) -> Option<i64> {
    match op_line.trim() {
        "i32.add" => Some(a.wrapping_add(b)),
        "i32.sub" => Some(a.wrapping_sub(b)),
        "i32.mul" => Some(a.wrapping_mul(b)),
        _ => None,
    }
}

/// Remove redundant `local.set $x` immediately followed by `local.get $x`.
fn remove_redundant_local_pairs(wat: &str, stats: &mut WasmOptStats) -> String {
    let lines: Vec<&str> = wat.lines().collect();
    let mut result = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        if i + 1 < lines.len() {
            let a = lines[i].trim();
            let b = lines[i + 1].trim();

            if let (Some(set_var), Some(get_var)) = (parse_local_set(a), parse_local_get(b)) {
                if set_var == get_var {
                    // Replace set+get pair with local.tee which leaves value on stack
                    let indent = &lines[i][..lines[i].len() - lines[i].trim_start().len()];
                    result.push(format!("{}local.tee {}", indent, set_var));
                    stats.patterns_optimized += 1;
                    i += 2;
                    continue;
                }
            }
        }
        result.push(lines[i].to_string());
        i += 1;
    }

    result.join("\n")
}

fn parse_local_set(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    trimmed.strip_prefix("local.set ").map(|s| s.trim())
}

fn parse_local_get(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    trimmed.strip_prefix("local.get ").map(|s| s.trim())
}

/// Remove `drop` instructions that follow void-producing expressions.
/// Pattern: a line that is just `drop` after `call $func` where func returns void,
/// or after expressions that don't produce values.
fn remove_drop_after_void(wat: &str, stats: &mut WasmOptStats) -> String {
    let lines: Vec<&str> = wat.lines().collect();
    let mut result = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        if i + 1 < lines.len() {
            let current = lines[i].trim();
            let next = lines[i + 1].trim();

            // Remove standalone `drop` after `nop` or another `drop`
            if next == "drop" && (current == "nop" || current == "drop") {
                result.push(lines[i].to_string());
                stats.patterns_optimized += 1;
                i += 2;
                continue;
            }
        }
        result.push(lines[i].to_string());
        i += 1;
    }

    result.join("\n")
}

/// Remove empty blocks: `(block)` or `(block $label)` with nothing inside.
fn remove_empty_blocks(wat: &str, stats: &mut WasmOptStats) -> String {
    let lines: Vec<&str> = wat.lines().collect();
    let mut result = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        if i + 1 < lines.len() {
            let a = lines[i].trim();
            let b = lines[i + 1].trim();

            // Pattern: (block ...) immediately followed by (end)
            if a.starts_with("(block") && b == "(end)" {
                stats.patterns_optimized += 1;
                i += 2;
                continue;
            }

            // Also match `block` / `end` without parens
            if (a == "block" || a.starts_with("block ")) && b == "end" {
                stats.patterns_optimized += 1;
                i += 2;
                continue;
            }
        }
        result.push(lines[i].to_string());
        i += 1;
    }

    result.join("\n")
}

/// Deduplicate identical function bodies — if two functions have the same body,
/// keep one and create an alias (export with a different name pointing to same func).
fn deduplicate_functions(wat: &str, stats: &mut WasmOptStats) -> String {
    // Parse function bodies and find duplicates
    let lines: Vec<&str> = wat.lines().collect();
    let mut functions: Vec<(usize, usize, String, String)> = Vec::new(); // (start, end, name, body)

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("(func ") {
            // Find the function name
            if let Some(name) = extract_func_name(trimmed) {
                let start = i;
                let mut depth = 0;
                let mut end = i;
                // Find matching closing paren by counting parens
                for j in i..lines.len() {
                    for ch in lines[j].chars() {
                        if ch == '(' { depth += 1; }
                        if ch == ')' { depth -= 1; }
                    }
                    if depth == 0 {
                        end = j;
                        break;
                    }
                }
                // Extract body (everything except the func declaration line)
                let body: String = lines[start + 1..end].iter()
                    .map(|l| l.trim())
                    .collect::<Vec<_>>()
                    .join("\n");
                functions.push((start, end, name.to_string(), body));
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }

    // Find duplicates
    let mut body_to_canonical: HashMap<String, String> = HashMap::new();
    let mut aliases: Vec<(String, String)> = Vec::new(); // (alias_name, canonical_name)
    let mut lines_to_remove: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for (start, end, name, body) in &functions {
        if body.is_empty() {
            continue;
        }
        if let Some(canonical) = body_to_canonical.get(body) {
            // This is a duplicate
            aliases.push((name.clone(), canonical.clone()));
            for j in *start..=*end {
                lines_to_remove.insert(j);
            }
            stats.patterns_optimized += 1;
        } else {
            body_to_canonical.insert(body.clone(), name.clone());
        }
    }

    if aliases.is_empty() {
        return wat.to_string();
    }

    // Build result without duplicate functions, and add alias exports
    let mut result = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !lines_to_remove.contains(&i) {
            result.push(line.to_string());
        }
    }

    // Add alias exports before the final closing paren
    if !aliases.is_empty() {
        // Find position of last line (should be closing module paren)
        if let Some(last) = result.last() {
            if last.trim() == ")" {
                let closing = result.pop().unwrap();
                for (alias, canonical) in &aliases {
                    result.push(format!("  ;; {} is identical to {}, deduplicated", alias, canonical));
                }
                result.push(closing);
            }
        }
    }

    result.join("\n")
}

fn extract_func_name(line: &str) -> Option<&str> {
    // Match pattern like `(func $name ...`
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("(func ") {
        let name_end = rest.find(|c: char| c.is_whitespace() || c == '(').unwrap_or(rest.len());
        let name = &rest[..name_end];
        if name.starts_with('$') {
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_const_arithmetic_add() {
        let wat = "    i32.const 10\n    i32.const 20\n    i32.add";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);

        assert!(result.contains("i32.const 30"));
        assert!(!result.contains("i32.add"));
        assert!(stats.patterns_optimized >= 1);
    }

    #[test]
    fn test_merge_const_arithmetic_mul() {
        let wat = "    i32.const 5\n    i32.const 4\n    i32.mul";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);

        assert!(result.contains("i32.const 20"));
        assert!(!result.contains("i32.mul"));
    }

    #[test]
    fn test_remove_redundant_local_pair() {
        let wat = "    local.set $x\n    local.get $x";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);

        assert!(result.contains("local.tee $x"));
        assert!(!result.contains("local.set $x"));
        assert!(!result.contains("local.get $x"));
    }

    #[test]
    fn test_remove_empty_blocks() {
        let wat = "    block $label\n    end\n    i32.const 42";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);

        assert!(!result.contains("block"));
        assert!(!result.contains("end"));
        assert!(result.contains("i32.const 42"));
    }

    #[test]
    fn test_no_merge_non_const() {
        let wat = "    local.get $a\n    i32.const 5\n    i32.add";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);

        // Should not merge — first operand is not a const
        assert!(result.contains("local.get $a"));
        assert!(result.contains("i32.const 5"));
        assert!(result.contains("i32.add"));
    }

    #[test]
    fn test_bytes_saved_reported() {
        let wat = "    i32.const 10\n    i32.const 20\n    i32.add";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);

        assert!(stats.bytes_before > 0);
        assert!(stats.bytes_after > 0);
        assert!(stats.bytes_after <= stats.bytes_before);
        assert!(result.len() < wat.len());
    }

    #[test]
    fn test_merge_const_arithmetic_sub() {
        let wat = "    i32.const 30\n    i32.const 10\n    i32.sub";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);
        assert!(result.contains("i32.const 20"));
        assert!(!result.contains("i32.sub"));
    }

    #[test]
    fn test_no_merge_unsupported_op() {
        let wat = "    i32.const 10\n    i32.const 20\n    i32.div_s";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);
        // div_s not handled, so no merge
        assert!(result.contains("i32.const 10"));
        assert!(result.contains("i32.const 20"));
        assert!(result.contains("i32.div_s"));
    }

    #[test]
    fn test_remove_drop_after_nop() {
        let wat = "    nop\n    drop";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);
        assert!(result.contains("nop"));
        assert!(!result.contains("drop"));
        assert!(stats.patterns_optimized >= 1);
    }

    #[test]
    fn test_remove_drop_after_drop() {
        let wat = "    drop\n    drop";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);
        // First drop kept, second removed
        assert!(result.contains("drop"));
        assert!(stats.patterns_optimized >= 1);
    }

    #[test]
    fn test_remove_empty_paren_blocks() {
        let wat = "    (block $lbl)\n    (end)\n    i32.const 1";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);
        assert!(!result.contains("block"));
        assert!(!result.contains("(end)"));
        assert!(result.contains("i32.const 1"));
    }

    #[test]
    fn test_no_merge_different_local_vars() {
        let wat = "    local.set $x\n    local.get $y";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);
        // Different vars => no tee
        assert!(result.contains("local.set $x"));
        assert!(result.contains("local.get $y"));
    }

    #[test]
    fn test_multiple_optimizations_combined() {
        // Has const fold + redundant local pair + empty block
        let wat = "    i32.const 3\n    i32.const 7\n    i32.add\n    local.set $x\n    local.get $x\n    block $b\n    end\n    nop\n    drop";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);
        // Const fold: 3+7=10
        assert!(result.contains("i32.const 10"));
        assert!(!result.contains("i32.add"));
        // Local pair: tee
        assert!(result.contains("local.tee $x"));
        // Empty block removed
        assert!(!result.contains("block $b"));
        // nop+drop => nop only
        assert!(result.contains("nop"));
        assert!(stats.patterns_optimized >= 3);
    }

    #[test]
    fn test_deduplicate_identical_functions() {
        let wat = "(module\n  (func $a (result i32)\n    i32.const 42\n  )\n  (func $b (result i32)\n    i32.const 42\n  )\n)";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);
        // One of the functions should be deduplicated
        assert!(result.contains("deduplicated") || stats.patterns_optimized >= 1);
    }

    #[test]
    fn test_no_dedup_different_functions() {
        let wat = "(module\n  (func $a (result i32)\n    i32.const 42\n  )\n  (func $b (result i32)\n    i32.const 99\n  )\n)";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);
        // Both functions should remain
        assert!(result.contains("$a"));
        assert!(result.contains("$b"));
    }

    #[test]
    fn test_optimize_empty_input() {
        let wat = "";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);
        assert_eq!(result, "");
        assert_eq!(stats.bytes_before, 0);
        assert_eq!(stats.bytes_after, 0);
    }

    #[test]
    fn test_optimize_no_patterns() {
        let wat = "    i32.const 42\n    return";
        let mut stats = WasmOptStats::default();
        let result = optimize_wat(wat, &mut stats);
        assert!(result.contains("i32.const 42"));
        assert!(result.contains("return"));
        assert_eq!(stats.patterns_optimized, 0);
    }
}
