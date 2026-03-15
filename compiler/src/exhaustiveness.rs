//! Match exhaustiveness and redundancy checking for the Nectar language.
//!
//! This module implements a simplified version of Rust's pattern usefulness
//! algorithm.  For enum types it verifies that every variant appears in at
//! least one arm; for booleans it checks both `true` and `false`; for
//! integers it warns when there is no wildcard/catch-all.  Redundant arms
//! (those that can never match because earlier arms already cover them) are
//! also detected.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::token::Span;

// ---------------------------------------------------------------------------
// Public error type
// ---------------------------------------------------------------------------

/// A warning/error produced by exhaustiveness checking.
#[derive(Debug, Clone)]
pub struct ExhaustivenessError {
    pub message: String,
    pub span: Span,
    pub missing_patterns: Vec<String>,
}

impl std::fmt::Display for ExhaustivenessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.span.line, self.span.col, self.message)
    }
}

// ---------------------------------------------------------------------------
// Enum registry — maps enum names to their variant names
// ---------------------------------------------------------------------------

/// Information about an enum definition, used for exhaustiveness analysis.
#[derive(Debug, Clone)]
struct EnumInfo {
    variant_names: Vec<String>,
    /// Maps variant name to the number of fields it carries.
    variant_field_counts: HashMap<String, usize>,
}

/// Build a map from enum name to its variant info by walking the program.
fn collect_enum_defs(program: &Program) -> HashMap<String, EnumInfo> {
    let mut enums = HashMap::new();
    for item in &program.items {
        if let Item::Enum(e) = item {
            let variant_names: Vec<String> =
                e.variants.iter().map(|v| v.name.clone()).collect();
            let variant_field_counts: HashMap<String, usize> = e
                .variants
                .iter()
                .map(|v| (v.name.clone(), v.fields.len()))
                .collect();
            enums.insert(
                e.name.clone(),
                EnumInfo {
                    variant_names,
                    variant_field_counts,
                },
            );
        }
    }
    // Register built-in Result<T,E> and Option<T> as enum-like types so that
    // match exhaustiveness checking requires Ok/Err and Some/None arms.
    enums.entry("Result".to_string()).or_insert_with(|| {
        let mut fc = HashMap::new();
        fc.insert("Ok".to_string(), 1);
        fc.insert("Err".to_string(), 1);
        EnumInfo {
            variant_names: vec!["Ok".to_string(), "Err".to_string()],
            variant_field_counts: fc,
        }
    });
    enums.entry("Option".to_string()).or_insert_with(|| {
        let mut fc = HashMap::new();
        fc.insert("Some".to_string(), 1);
        fc.insert("None".to_string(), 0);
        EnumInfo {
            variant_names: vec!["Some".to_string(), "None".to_string()],
            variant_field_counts: fc,
        }
    });

    enums
}

// ---------------------------------------------------------------------------
// Subject type inference (lightweight — no full type inference needed)
// ---------------------------------------------------------------------------

/// The type of a match subject, as far as exhaustiveness cares.
#[derive(Debug, Clone)]
enum SubjectKind {
    Enum(String),
    Bool,
    Integer,
    Other,
}

/// Try to determine what kind of value the match subject is by looking at the
/// patterns in the arms.  If any arm uses a `Variant` pattern whose name maps
/// to a known enum, the subject is an enum of that type.  If all literal
/// patterns are booleans, the subject is a bool, and so on.
fn infer_subject_kind(arms: &[MatchArm], enums: &HashMap<String, EnumInfo>) -> SubjectKind {
    for arm in arms {
        match &arm.pattern {
            Pattern::Variant { name, .. } => {
                // Find which enum owns this variant.
                for (enum_name, info) in enums {
                    if info.variant_names.contains(name) {
                        return SubjectKind::Enum(enum_name.clone());
                    }
                }
            }
            Pattern::Literal(Expr::Bool(_)) => return SubjectKind::Bool,
            Pattern::Literal(Expr::Integer(_)) => return SubjectKind::Integer,
            _ => {}
        }
    }
    SubjectKind::Other
}

// ---------------------------------------------------------------------------
// Core algorithm
// ---------------------------------------------------------------------------

/// Check whether the given set of patterns exhaustively covers the subject
/// type, and whether any pattern is redundant.
fn check_arms(
    arms: &[MatchArm],
    subject: &SubjectKind,
    enums: &HashMap<String, EnumInfo>,
    match_span: Span,
) -> Vec<ExhaustivenessError> {
    let mut errors = Vec::new();

    match subject {
        SubjectKind::Enum(enum_name) => {
            if let Some(info) = enums.get(enum_name) {
                check_enum_arms(arms, enum_name, info, match_span, enums, &mut errors);
            }
        }
        SubjectKind::Bool => {
            check_bool_arms(arms, match_span, &mut errors);
        }
        SubjectKind::Integer => {
            check_integer_arms(arms, match_span, &mut errors);
        }
        SubjectKind::Other => {
            // We can't check exhaustiveness for unknown types, but we can
            // still detect obviously redundant arms.
            check_redundancy_only(arms, &mut errors);
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Enum exhaustiveness
// ---------------------------------------------------------------------------

fn check_enum_arms(
    arms: &[MatchArm],
    enum_name: &str,
    info: &EnumInfo,
    match_span: Span,
    enums: &HashMap<String, EnumInfo>,
    errors: &mut Vec<ExhaustivenessError>,
) {
    let mut covered: HashSet<String> = HashSet::new();
    let mut has_wildcard = false;
    let mut wildcard_index: Option<usize> = None;

    for (i, arm) in arms.iter().enumerate() {
        match &arm.pattern {
            Pattern::Wildcard | Pattern::Ident(_) => {
                if has_wildcard || covered.len() == info.variant_names.len() {
                    // This arm is redundant.
                    errors.push(ExhaustivenessError {
                        message: format!(
                            "redundant pattern in match on `{}`: this arm will never be reached",
                            enum_name
                        ),
                        span: match_span,
                        missing_patterns: vec![],
                    });
                }
                has_wildcard = true;
                wildcard_index = Some(i);
            }
            Pattern::Variant { name, fields } => {
                if has_wildcard || covered.contains(name) {
                    errors.push(ExhaustivenessError {
                        message: format!(
                            "redundant pattern `{}` in match on `{}`: already covered",
                            name, enum_name
                        ),
                        span: match_span,
                        missing_patterns: vec![],
                    });
                }
                covered.insert(name.clone());

                // Recursively check nested patterns against variant fields
                if let Some(&field_count) = info.variant_field_counts.get(name) {
                    check_nested_patterns(name, fields, field_count, match_span, enums, errors);
                }
            }
            Pattern::Literal(_) => {
                // Literal in an enum match is unusual; treat as not covering
                // any variant.
            }
            _ => {}
        }
    }

    // Check if a wildcard after all variants are covered is redundant.
    if has_wildcard && covered.len() == info.variant_names.len() {
        // Only warn if the wildcard came after all variants were listed.
        if let Some(wi) = wildcard_index {
            // Check if all variants were covered by arms before the wildcard.
            let covered_before_wildcard: HashSet<String> = arms[..wi]
                .iter()
                .filter_map(|a| {
                    if let Pattern::Variant { name, .. } = &a.pattern {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
                .collect();
            if covered_before_wildcard.len() == info.variant_names.len() {
                // Already reported as redundant above; skip duplicate.
            }
        }
    }

    if !has_wildcard {
        let missing: Vec<String> = info
            .variant_names
            .iter()
            .filter(|v| !covered.contains(*v))
            .cloned()
            .collect();
        if !missing.is_empty() {
            let list = missing.join(", ");
            errors.push(ExhaustivenessError {
                message: format!(
                    "non-exhaustive match on `{}`: missing pattern(s): {}",
                    enum_name, list
                ),
                span: match_span,
                missing_patterns: missing,
            });
        }
    }
}

/// Check nested patterns inside a variant.
fn check_nested_patterns(
    variant_name: &str,
    patterns: &[Pattern],
    expected_field_count: usize,
    match_span: Span,
    _enums: &HashMap<String, EnumInfo>,
    errors: &mut Vec<ExhaustivenessError>,
) {
    if patterns.len() != expected_field_count && expected_field_count > 0 && !patterns.is_empty() {
        errors.push(ExhaustivenessError {
            message: format!(
                "variant `{}` has {} field(s) but pattern has {}",
                variant_name,
                expected_field_count,
                patterns.len()
            ),
            span: match_span,
            missing_patterns: vec![],
        });
    }
}

// ---------------------------------------------------------------------------
// Bool exhaustiveness
// ---------------------------------------------------------------------------

fn check_bool_arms(
    arms: &[MatchArm],
    match_span: Span,
    errors: &mut Vec<ExhaustivenessError>,
) {
    let mut has_true = false;
    let mut has_false = false;
    let mut has_wildcard = false;

    for (i, arm) in arms.iter().enumerate() {
        match &arm.pattern {
            Pattern::Wildcard | Pattern::Ident(_) => {
                if has_wildcard || (has_true && has_false) {
                    errors.push(ExhaustivenessError {
                        message: "redundant pattern in bool match: this arm will never be reached"
                            .to_string(),
                        span: match_span,
                        missing_patterns: vec![],
                    });
                }
                has_wildcard = true;
            }
            Pattern::Literal(Expr::Bool(true)) => {
                if has_true || has_wildcard {
                    errors.push(ExhaustivenessError {
                        message: "redundant pattern `true` in bool match: already covered"
                            .to_string(),
                        span: match_span,
                        missing_patterns: vec![],
                    });
                }
                has_true = true;
            }
            Pattern::Literal(Expr::Bool(false)) => {
                if has_false || has_wildcard {
                    errors.push(ExhaustivenessError {
                        message: "redundant pattern `false` in bool match: already covered"
                            .to_string(),
                        span: match_span,
                        missing_patterns: vec![],
                    });
                }
                has_false = true;
            }
            _ => {
                // Non-bool pattern in a bool match — ignore for now.
                let _ = i;
            }
        }
    }

    if !has_wildcard {
        let mut missing = Vec::new();
        if !has_true {
            missing.push("true".to_string());
        }
        if !has_false {
            missing.push("false".to_string());
        }
        if !missing.is_empty() {
            let list = missing.join(", ");
            errors.push(ExhaustivenessError {
                message: format!("non-exhaustive bool match: missing pattern(s): {}", list),
                span: match_span,
                missing_patterns: missing,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Integer exhaustiveness
// ---------------------------------------------------------------------------

fn check_integer_arms(
    arms: &[MatchArm],
    match_span: Span,
    errors: &mut Vec<ExhaustivenessError>,
) {
    let has_wildcard = arms.iter().any(|a| {
        matches!(&a.pattern, Pattern::Wildcard | Pattern::Ident(_))
    });

    if !has_wildcard {
        errors.push(ExhaustivenessError {
            message: "non-exhaustive integer match: consider adding a wildcard `_` arm".to_string(),
            span: match_span,
            missing_patterns: vec!["_".to_string()],
        });
    }

    // Check for redundant arms (duplicate literals or arms after wildcard).
    let mut seen_literals: HashSet<i64> = HashSet::new();
    let mut wildcard_seen = false;
    for arm in arms {
        match &arm.pattern {
            Pattern::Wildcard | Pattern::Ident(_) => {
                if wildcard_seen {
                    errors.push(ExhaustivenessError {
                        message:
                            "redundant pattern in integer match: this arm will never be reached"
                                .to_string(),
                        span: match_span,
                        missing_patterns: vec![],
                    });
                }
                wildcard_seen = true;
            }
            Pattern::Literal(Expr::Integer(n)) => {
                if wildcard_seen || !seen_literals.insert(*n) {
                    errors.push(ExhaustivenessError {
                        message: format!(
                            "redundant pattern `{}` in integer match: already covered",
                            n
                        ),
                        span: match_span,
                        missing_patterns: vec![],
                    });
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Redundancy-only check (for unknown subject types)
// ---------------------------------------------------------------------------

fn check_redundancy_only(arms: &[MatchArm], errors: &mut Vec<ExhaustivenessError>) {
    let mut wildcard_seen = false;
    for arm in arms {
        match &arm.pattern {
            Pattern::Wildcard | Pattern::Ident(_) => {
                if wildcard_seen {
                    errors.push(ExhaustivenessError {
                        message: "redundant pattern: this arm will never be reached".to_string(),
                        span: Span::new(0, 0, 0, 0),
                        missing_patterns: vec![],
                    });
                }
                wildcard_seen = true;
            }
            _ => {
                if wildcard_seen {
                    errors.push(ExhaustivenessError {
                        message: "redundant pattern: this arm follows a wildcard and will never be reached".to_string(),
                        span: Span::new(0, 0, 0, 0),
                        missing_patterns: vec![],
                    });
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Walking the AST to find all match expressions
// ---------------------------------------------------------------------------

fn walk_expr(expr: &Expr, enums: &HashMap<String, EnumInfo>, errors: &mut Vec<ExhaustivenessError>) {
    match expr {
        Expr::Match { subject, arms } => {
            // Determine subject kind from patterns.
            let kind = infer_subject_kind(arms, enums);
            // Use a dummy span; the match itself doesn't store a span
            // directly, but we can use the first arm or a zero span.
            let match_span = Span::new(0, 0, 0, 0);
            let mut arm_errors = check_arms(arms, &kind, enums, match_span);
            errors.append(&mut arm_errors);

            // Also walk into the subject and arm bodies.
            walk_expr(subject, enums, errors);
            for arm in arms {
                walk_expr(&arm.body, enums, errors);
            }
        }
        // Recursively walk sub-expressions.
        Expr::Binary { left, right, .. } => {
            walk_expr(left, enums, errors);
            walk_expr(right, enums, errors);
        }
        Expr::Unary { operand, .. } => walk_expr(operand, enums, errors),
        Expr::FieldAccess { object, .. } => walk_expr(object, enums, errors),
        Expr::MethodCall { object, args, .. } | Expr::FnCall { callee: object, args } => {
            walk_expr(object, enums, errors);
            for a in args {
                walk_expr(a, enums, errors);
            }
        }
        Expr::Index { object, index } => {
            walk_expr(object, enums, errors);
            walk_expr(index, enums, errors);
        }
        Expr::If { condition, then_block, else_block } => {
            walk_expr(condition, enums, errors);
            walk_block(then_block, enums, errors);
            if let Some(eb) = else_block {
                walk_block(eb, enums, errors);
            }
        }
        Expr::For { iterator, body, .. } => {
            walk_expr(iterator, enums, errors);
            walk_block(body, enums, errors);
        }
        Expr::While { condition, body } => {
            walk_expr(condition, enums, errors);
            walk_block(body, enums, errors);
        }
        Expr::Block(block) => walk_block(block, enums, errors),
        Expr::Borrow(inner) | Expr::BorrowMut(inner) | Expr::Await(inner)
        | Expr::Stream { source: inner }
        | Expr::Receive { channel: inner } | Expr::Navigate { path: inner } => {
            walk_expr(inner, enums, errors);
        }
        Expr::Spawn { body: blk, .. } => walk_block(blk, enums, errors),
        Expr::Assign { target, value } => {
            walk_expr(target, enums, errors);
            walk_expr(value, enums, errors);
        }
        Expr::Fetch { url, options, .. } => {
            walk_expr(url, enums, errors);
            if let Some(opts) = options {
                walk_expr(opts, enums, errors);
            }
        }
        Expr::Closure { body, .. } => walk_expr(body, enums, errors),
        Expr::Suspend { fallback, body } | Expr::Send { channel: fallback, value: body } => {
            walk_expr(fallback, enums, errors);
            walk_expr(body, enums, errors);
        }
        Expr::TryCatch { body, catch_body, .. } => {
            walk_expr(body, enums, errors);
            walk_expr(catch_body, enums, errors);
        }
        Expr::Parallel { tasks, .. } => {
            for e in tasks {
                walk_expr(e, enums, errors);
            }
        }
        Expr::Assert { condition, .. } => walk_expr(condition, enums, errors),
        Expr::AssertEq { left, right, .. } => {
            walk_expr(left, enums, errors);
            walk_expr(right, enums, errors);
        }
        Expr::StructInit { fields, .. } => {
            for (_, e) in fields {
                walk_expr(e, enums, errors);
            }
        }
        Expr::Animate { target, .. } => walk_expr(target, enums, errors),
        Expr::PromptTemplate { interpolations, .. } => {
            for (_, e) in interpolations {
                walk_expr(e, enums, errors);
            }
        }
        Expr::FormatString { parts } => {
            for part in parts {
                if let crate::ast::FormatPart::Expression(e) = part {
                    walk_expr(e, enums, errors);
                }
            }
        }
        Expr::Download { data, filename, .. } => {
            walk_expr(data, enums, errors);
            walk_expr(filename, enums, errors);
        }
        Expr::Env { name, .. } => {
            walk_expr(name, enums, errors);
        }
        Expr::Trace { label, body, .. } => {
            walk_expr(label, enums, errors);
            walk_block(body, enums, errors);
        }
        Expr::Flag { name, .. } => {
            walk_expr(name, enums, errors);
        }
        Expr::Channel { .. }
        | Expr::Integer(_) | Expr::Float(_) | Expr::StringLit(_)
        | Expr::Bool(_) | Expr::Ident(_) | Expr::SelfExpr => {}
        _ => {}
    }
}

fn walk_block(block: &Block, enums: &HashMap<String, EnumInfo>, errors: &mut Vec<ExhaustivenessError>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let { value, .. } | Stmt::Signal { value, .. } | Stmt::Expr(value) | Stmt::Yield(value) => {
                walk_expr(value, enums, errors);
            }
            Stmt::Return(Some(e)) => walk_expr(e, enums, errors),
            Stmt::Return(None) => {}
            _ => {}
        }
    }
}

fn walk_item(item: &Item, enums: &HashMap<String, EnumInfo>, errors: &mut Vec<ExhaustivenessError>) {
    match item {
        Item::Function(f) => walk_block(&f.body, enums, errors),
        Item::Component(c) => {
            for method in &c.methods {
                walk_block(&method.body, enums, errors);
            }
        }
        Item::Impl(imp) => {
            for method in &imp.methods {
                walk_block(&method.body, enums, errors);
            }
        }
        Item::Store(store) => {
            for action in &store.actions {
                walk_block(&action.body, enums, errors);
            }
            for computed in &store.computed {
                walk_block(&computed.body, enums, errors);
            }
            for effect in &store.effects {
                walk_block(&effect.body, enums, errors);
            }
        }
        Item::Agent(agent) => {
            for tool in &agent.tools {
                walk_block(&tool.body, enums, errors);
            }
            for method in &agent.methods {
                walk_block(&method.body, enums, errors);
            }
        }
        Item::Test(t) => walk_block(&t.body, enums, errors),
            Item::Contract(_) => {}
            Item::App(_) => {}
            Item::Page(_) => {}
            Item::Form(_) => {}
            Item::Channel(ch) => {
                if let Some(ref handler) = ch.on_message {
                    walk_block(&handler.body, enums, errors);
                }
                if let Some(ref handler) = ch.on_connect {
                    walk_block(&handler.body, enums, errors);
                }
                if let Some(ref handler) = ch.on_disconnect {
                    walk_block(&handler.body, enums, errors);
                }
                for method in &ch.methods {
                    walk_block(&method.body, enums, errors);
                }
            }
        Item::Struct(_) | Item::Enum(_) | Item::Use(_) | Item::Router(_)
        | Item::LazyComponent(_) | Item::Trait(_) | Item::Mod(_) => {}
        Item::Embed(_) => {}
        Item::Pdf(_) => {}
        Item::Payment(_) => {}
        Item::Banking(_) => {}
        Item::Map(_) => {}
        Item::Auth(_) => {}
        Item::Upload(_) => {}
        Item::Db(_) => {}
        Item::Cache(_) => {}
        Item::Breakpoints(_) => {}
        Item::Theme(_) => {}
        Item::Animation(_) => {}
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Check all match expressions in the program for exhaustiveness and
/// redundancy.  Returns a (possibly empty) list of warnings.
pub fn check_exhaustiveness(program: &Program) -> Vec<ExhaustivenessError> {
    let enums = collect_enum_defs(program);
    let mut errors = Vec::new();

    for item in &program.items {
        walk_item(item, &enums, &mut errors);
    }

    errors
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Span;

    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    fn block(stmts: Vec<Stmt>) -> Block {
        Block {
            stmts,
            span: span(),
        }
    }

    fn make_enum(name: &str, variants: Vec<(&str, usize)>) -> Item {
        Item::Enum(EnumDef {
            name: name.to_string(),
            type_params: vec![],
            variants: variants
                .into_iter()
                .map(|(vname, field_count)| Variant {
                    name: vname.to_string(),
                    fields: (0..field_count)
                        .map(|_| Type::Named("i32".to_string()))
                        .collect(),
                })
                .collect(),
            is_pub: false,
            span: span(),
        })
    }

    fn match_expr(arms: Vec<MatchArm>) -> Expr {
        Expr::Match {
            subject: Box::new(Expr::Ident("x".to_string())),
            arms,
        }
    }

    fn arm(pattern: Pattern, body: Expr) -> MatchArm {
        MatchArm { pattern, guard: None, body }
    }

    fn unit_body() -> Expr {
        Expr::Integer(0)
    }

    fn wrap_in_fn(expr: Expr) -> Item {
        Item::Function(Function {
            name: "test_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            lifetimes: vec![],
            body: block(vec![Stmt::Expr(expr)]),
            is_pub: false,
            is_async: false,
            must_use: false,
            span: span(),
        })
    }

    // -----------------------------------------------------------------------
    // Test 1: All enum variants covered — no warning
    // -----------------------------------------------------------------------
    #[test]
    fn all_variants_covered_no_warning() {
        let program = Program {
            items: vec![
                make_enum("Color", vec![("Red", 0), ("Green", 0), ("Blue", 0)]),
                wrap_in_fn(match_expr(vec![
                    arm(Pattern::Variant { name: "Red".into(), fields: vec![] }, unit_body()),
                    arm(Pattern::Variant { name: "Green".into(), fields: vec![] }, unit_body()),
                    arm(Pattern::Variant { name: "Blue".into(), fields: vec![] }, unit_body()),
                ])),
            ],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    // -----------------------------------------------------------------------
    // Test 2: Missing variant — error with missing variant name
    // -----------------------------------------------------------------------
    #[test]
    fn missing_variant_produces_error() {
        let program = Program {
            items: vec![
                make_enum("Color", vec![("Red", 0), ("Green", 0), ("Blue", 0)]),
                wrap_in_fn(match_expr(vec![
                    arm(Pattern::Variant { name: "Red".into(), fields: vec![] }, unit_body()),
                    arm(Pattern::Variant { name: "Green".into(), fields: vec![] }, unit_body()),
                ])),
            ],
        };
        let errors = check_exhaustiveness(&program);
        assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
        assert!(
            errors[0].message.contains("non-exhaustive"),
            "expected non-exhaustive error, got: {}",
            errors[0].message,
        );
        assert!(
            errors[0].missing_patterns.contains(&"Blue".to_string()),
            "expected missing 'Blue', got: {:?}",
            errors[0].missing_patterns,
        );
    }

    // -----------------------------------------------------------------------
    // Test 3: Wildcard covers remaining — no warning
    // -----------------------------------------------------------------------
    #[test]
    fn wildcard_covers_remaining_no_warning() {
        let program = Program {
            items: vec![
                make_enum("Color", vec![("Red", 0), ("Green", 0), ("Blue", 0)]),
                wrap_in_fn(match_expr(vec![
                    arm(Pattern::Variant { name: "Red".into(), fields: vec![] }, unit_body()),
                    arm(Pattern::Wildcard, unit_body()),
                ])),
            ],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    // -----------------------------------------------------------------------
    // Test 4: Bool match missing `false` — error
    // -----------------------------------------------------------------------
    #[test]
    fn bool_match_missing_false() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Literal(Expr::Bool(true)), unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
        assert!(
            errors[0].message.contains("non-exhaustive"),
            "expected non-exhaustive, got: {}",
            errors[0].message,
        );
        assert!(
            errors[0].missing_patterns.contains(&"false".to_string()),
            "expected missing 'false', got: {:?}",
            errors[0].missing_patterns,
        );
    }

    // -----------------------------------------------------------------------
    // Test 5: Redundant pattern after wildcard — warning
    // -----------------------------------------------------------------------
    #[test]
    fn redundant_pattern_after_wildcard() {
        let program = Program {
            items: vec![
                make_enum("Color", vec![("Red", 0), ("Green", 0), ("Blue", 0)]),
                wrap_in_fn(match_expr(vec![
                    arm(Pattern::Wildcard, unit_body()),
                    arm(Pattern::Variant { name: "Red".into(), fields: vec![] }, unit_body()),
                ])),
            ],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("redundant")),
            "expected redundant pattern warning, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test 6: Nested variant patterns checked (field count mismatch)
    // -----------------------------------------------------------------------
    #[test]
    fn nested_variant_field_count_mismatch() {
        let program = Program {
            items: vec![
                make_enum("Shape", vec![("Circle", 1), ("Rect", 2)]),
                wrap_in_fn(match_expr(vec![
                    arm(
                        Pattern::Variant {
                            name: "Circle".into(),
                            fields: vec![Pattern::Ident("r".into()), Pattern::Ident("extra".into())],
                        },
                        unit_body(),
                    ),
                    arm(
                        Pattern::Variant {
                            name: "Rect".into(),
                            fields: vec![Pattern::Ident("w".into()), Pattern::Ident("h".into())],
                        },
                        unit_body(),
                    ),
                ])),
            ],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("Circle") && e.message.contains("field")),
            "expected field count mismatch for Circle, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test 7: Ident pattern covers everything (catch-all)
    // -----------------------------------------------------------------------
    #[test]
    fn ident_pattern_is_catch_all() {
        let program = Program {
            items: vec![
                make_enum("Color", vec![("Red", 0), ("Green", 0), ("Blue", 0)]),
                wrap_in_fn(match_expr(vec![
                    arm(Pattern::Ident("x".into()), unit_body()),
                ])),
            ],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    // -----------------------------------------------------------------------
    // Test 8: Integer match without wildcard warns
    // -----------------------------------------------------------------------
    #[test]
    fn integer_match_without_wildcard_warns() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Literal(Expr::Integer(1)), unit_body()),
                arm(Pattern::Literal(Expr::Integer(2)), unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("non-exhaustive")),
            "expected non-exhaustive warning for integer match, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: Integer match with wildcard is OK
    // -----------------------------------------------------------------------
    #[test]
    fn integer_match_with_wildcard_ok() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Literal(Expr::Integer(1)), unit_body()),
                arm(Pattern::Wildcard, unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    // -----------------------------------------------------------------------
    // Test: Integer match with duplicate literals detects redundancy
    // -----------------------------------------------------------------------
    #[test]
    fn integer_match_duplicate_literal() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Literal(Expr::Integer(1)), unit_body()),
                arm(Pattern::Literal(Expr::Integer(1)), unit_body()), // duplicate
                arm(Pattern::Wildcard, unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("redundant")),
            "expected redundant pattern warning, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: Bool match with both true and false is exhaustive
    // -----------------------------------------------------------------------
    #[test]
    fn bool_match_both_branches_no_warning() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Literal(Expr::Bool(true)), unit_body()),
                arm(Pattern::Literal(Expr::Bool(false)), unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    // -----------------------------------------------------------------------
    // Test: Bool match missing true
    // -----------------------------------------------------------------------
    #[test]
    fn bool_match_missing_true() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Literal(Expr::Bool(false)), unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.missing_patterns.contains(&"true".to_string())),
            "expected missing 'true', got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: Bool match with wildcard
    // -----------------------------------------------------------------------
    #[test]
    fn bool_match_wildcard_covers_all() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Literal(Expr::Bool(true)), unit_body()),
                arm(Pattern::Wildcard, unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    // -----------------------------------------------------------------------
    // Test: Bool match with redundant true after wildcard
    // -----------------------------------------------------------------------
    #[test]
    fn bool_match_redundant_after_wildcard() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Wildcard, unit_body()),
                arm(Pattern::Literal(Expr::Bool(true)), unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("redundant")),
            "expected redundant warning, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: Redundancy-only check for unknown types
    // -----------------------------------------------------------------------
    #[test]
    fn redundancy_only_double_wildcard() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Wildcard, unit_body()),
                arm(Pattern::Wildcard, unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("redundant")),
            "expected redundant warning for double wildcard, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: Pattern after wildcard in unknown type
    // -----------------------------------------------------------------------
    #[test]
    fn redundancy_pattern_after_wildcard() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Wildcard, unit_body()),
                arm(Pattern::Literal(Expr::StringLit("x".into())), unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("redundant") && e.message.contains("wildcard")),
            "expected redundant pattern follows wildcard, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: Enum with all variants + redundant wildcard
    // -----------------------------------------------------------------------
    #[test]
    fn enum_all_variants_plus_wildcard_redundant() {
        let program = Program {
            items: vec![
                make_enum("Dir", vec![("Up", 0), ("Down", 0)]),
                wrap_in_fn(match_expr(vec![
                    arm(Pattern::Variant { name: "Up".into(), fields: vec![] }, unit_body()),
                    arm(Pattern::Variant { name: "Down".into(), fields: vec![] }, unit_body()),
                    arm(Pattern::Wildcard, unit_body()), // redundant
                ])),
            ],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("redundant")),
            "expected redundant wildcard warning, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: Enum with duplicate variant
    // -----------------------------------------------------------------------
    #[test]
    fn enum_duplicate_variant_redundant() {
        let program = Program {
            items: vec![
                make_enum("Dir", vec![("Up", 0), ("Down", 0)]),
                wrap_in_fn(match_expr(vec![
                    arm(Pattern::Variant { name: "Up".into(), fields: vec![] }, unit_body()),
                    arm(Pattern::Variant { name: "Up".into(), fields: vec![] }, unit_body()), // duplicate
                    arm(Pattern::Variant { name: "Down".into(), fields: vec![] }, unit_body()),
                ])),
            ],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("redundant") && e.message.contains("Up")),
            "expected redundant 'Up' warning, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: Tuple pattern (falls through to Other)
    // -----------------------------------------------------------------------
    #[test]
    fn tuple_pattern_match() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Tuple(vec![Pattern::Ident("a".into()), Pattern::Ident("b".into())]), unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        // Tuple patterns resolve to SubjectKind::Other, so no exhaustiveness check
        assert!(errors.is_empty(), "expected no errors for tuple pattern, got: {:?}", errors);
    }

    // -----------------------------------------------------------------------
    // Test: String pattern (falls through to Other)
    // -----------------------------------------------------------------------
    #[test]
    fn string_pattern_match() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Literal(Expr::StringLit("hello".into())), unit_body()),
                arm(Pattern::Wildcard, unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.is_empty(), "expected no errors for string match with wildcard, got: {:?}", errors);
    }

    // -----------------------------------------------------------------------
    // Test: Multiple missing variants
    // -----------------------------------------------------------------------
    #[test]
    fn multiple_missing_variants() {
        let program = Program {
            items: vec![
                make_enum("Dir", vec![("Up", 0), ("Down", 0), ("Left", 0), ("Right", 0)]),
                wrap_in_fn(match_expr(vec![
                    arm(Pattern::Variant { name: "Up".into(), fields: vec![] }, unit_body()),
                ])),
            ],
        };
        let errors = check_exhaustiveness(&program);
        assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
        assert!(errors[0].missing_patterns.contains(&"Down".to_string()));
        assert!(errors[0].missing_patterns.contains(&"Left".to_string()));
        assert!(errors[0].missing_patterns.contains(&"Right".to_string()));
    }

    // -----------------------------------------------------------------------
    // Test: ExhaustivenessError Display trait
    // -----------------------------------------------------------------------
    #[test]
    fn test_error_display() {
        let err = ExhaustivenessError {
            message: "test error".to_string(),
            span: Span::new(0, 10, 5, 3),
            missing_patterns: vec![],
        };
        let display = format!("{}", err);
        assert_eq!(display, "5:3: test error");
    }

    // -----------------------------------------------------------------------
    // Test: Built-in Result<T,E> type exhaustiveness
    // -----------------------------------------------------------------------
    #[test]
    fn builtin_result_missing_err() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Variant { name: "Ok".into(), fields: vec![Pattern::Ident("v".into())] }, unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("non-exhaustive") && e.missing_patterns.contains(&"Err".to_string())),
            "expected missing 'Err', got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: Built-in Option<T> type exhaustiveness
    // -----------------------------------------------------------------------
    #[test]
    fn builtin_option_complete() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Variant { name: "Some".into(), fields: vec![Pattern::Ident("v".into())] }, unit_body()),
                arm(Pattern::Variant { name: "None".into(), fields: vec![] }, unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.is_empty(), "expected no errors for complete Option match, got: {:?}", errors);
    }

    // -----------------------------------------------------------------------
    // Test: Integer arm after wildcard is redundant
    // -----------------------------------------------------------------------
    #[test]
    fn integer_arm_after_wildcard_redundant() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Wildcard, unit_body()),
                arm(Pattern::Literal(Expr::Integer(1)), unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("redundant")),
            "expected redundant, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: walk_expr coverage for various expression types
    // -----------------------------------------------------------------------

    #[allow(dead_code)]
    fn make_match_inside(expr: Expr) -> Item {
        // Wraps an expression in a function so walk_item processes it
        wrap_in_fn(expr)
    }

    #[test]
    fn walk_expr_binary() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Binary {
                op: BinOp::Add,
                left: Box::new(match_expr(vec![
                    arm(Pattern::Wildcard, unit_body()),
                ])),
                right: Box::new(Expr::Integer(1)),
            })],
        };
        let errors = check_exhaustiveness(&program);
        // No error for wildcard-only match
        assert!(errors.is_empty());
    }

    #[test]
    fn walk_expr_unary() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Unary {
                op: UnaryOp::Neg,
                operand: Box::new(match_expr(vec![
                    arm(Pattern::Wildcard, unit_body()),
                ])),
            })],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.is_empty());
    }

    #[test]
    fn walk_expr_field_access() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::FieldAccess {
                object: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
                field: "f".to_string(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_method_call() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::MethodCall {
                object: Box::new(Expr::Ident("x".to_string())),
                method: "m".to_string(),
                args: vec![match_expr(vec![arm(Pattern::Wildcard, unit_body())])],
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_fn_call() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::FnCall {
                callee: Box::new(Expr::Ident("f".to_string())),
                args: vec![match_expr(vec![arm(Pattern::Wildcard, unit_body())])],
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_index() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Index {
                object: Box::new(Expr::Ident("arr".to_string())),
                index: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_if() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::If {
                condition: Box::new(Expr::Bool(true)),
                then_block: block(vec![Stmt::Expr(match_expr(vec![
                    arm(Pattern::Wildcard, unit_body()),
                ]))]),
                else_block: Some(block(vec![Stmt::Expr(match_expr(vec![
                    arm(Pattern::Wildcard, unit_body()),
                ]))])),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_for() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::For {
                binding: "i".to_string(),
                iterator: Box::new(Expr::Ident("items".to_string())),
                body: block(vec![Stmt::Expr(match_expr(vec![
                    arm(Pattern::Wildcard, unit_body()),
                ]))]),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_while() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::While {
                condition: Box::new(Expr::Bool(true)),
                body: block(vec![Stmt::Expr(match_expr(vec![
                    arm(Pattern::Wildcard, unit_body()),
                ]))]),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_block() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Block(block(vec![
                Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
            ])))],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_borrow_and_variants() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Borrow(Box::new(
                match_expr(vec![arm(Pattern::Wildcard, unit_body())]),
            )))],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_spawn() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Spawn {
                body: block(vec![Stmt::Expr(match_expr(vec![
                    arm(Pattern::Wildcard, unit_body()),
                ]))]),
                span: span(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_assign() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Assign {
                target: Box::new(Expr::Ident("x".to_string())),
                value: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_fetch() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Fetch {
                url: Box::new(Expr::StringLit("u".to_string())),
                options: Some(Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))),
                contract: None,
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_closure() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Closure {
                params: vec![],
                body: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_suspend_and_send() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Suspend {
                fallback: Box::new(Expr::Integer(0)),
                body: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_try_catch() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::TryCatch {
                body: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
                error_binding: "e".to_string(),
                catch_body: Box::new(Expr::Integer(0)),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_parallel() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Parallel {
                tasks: vec![match_expr(vec![arm(Pattern::Wildcard, unit_body())])],
                span: span(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_assert() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Assert {
                condition: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
                message: None,
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_assert_eq() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::AssertEq {
                left: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
                right: Box::new(Expr::Integer(1)),
                message: None,
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_struct_init() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::StructInit {
                name: "S".to_string(),
                fields: vec![("x".to_string(), match_expr(vec![arm(Pattern::Wildcard, unit_body())]))],
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_animate() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Animate {
                target: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
                animation: "fade".to_string(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_prompt_template() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::PromptTemplate {
                template: "{x}".to_string(),
                interpolations: vec![("x".to_string(), match_expr(vec![arm(Pattern::Wildcard, unit_body())]))],
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_format_string() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::FormatString {
                parts: vec![crate::ast::FormatPart::Expression(Box::new(
                    match_expr(vec![arm(Pattern::Wildcard, unit_body())]),
                ))],
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_download() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Download {
                data: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
                filename: Box::new(Expr::StringLit("f".to_string())),
                span: span(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_env() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Env {
                name: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
                span: span(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_trace() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Trace {
                label: Box::new(Expr::StringLit("t".to_string())),
                body: block(vec![Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))]),
                span: span(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_expr_flag() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Flag {
                name: Box::new(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
                span: span(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    // -----------------------------------------------------------------------
    // walk_item coverage for Component, Impl, Store, Agent, Test, Channel
    // -----------------------------------------------------------------------

    fn wrap_in_component_method(expr: Expr) -> Item {
        Item::Component(Component {
            name: "C".to_string(), type_params: vec![], props: vec![],
            state: vec![],
            methods: vec![Function {
                name: "m".to_string(), lifetimes: vec![], type_params: vec![],
                params: vec![], return_type: None, trait_bounds: vec![],
                body: block(vec![Stmt::Expr(expr)]),
                is_pub: false, is_async: false, must_use: false, span: span(),
            }],
            styles: vec![], transitions: vec![], trait_bounds: vec![],
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            permissions: None, gestures: vec![], skeleton: None,
            error_boundary: None, chunk: None, on_destroy: None,
            a11y: None, shortcuts: vec![], span: span(),
        })
    }

    #[test]
    fn walk_item_component() {
        let program = Program {
            items: vec![wrap_in_component_method(
                match_expr(vec![arm(Pattern::Wildcard, unit_body())]),
            )],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_item_impl() {
        let program = Program {
            items: vec![Item::Impl(ImplBlock {
                target: "Foo".to_string(), trait_impls: vec![],
                methods: vec![Function {
                    name: "m".to_string(), lifetimes: vec![], type_params: vec![],
                    params: vec![], return_type: None, trait_bounds: vec![],
                    body: block(vec![Stmt::Expr(match_expr(vec![
                        arm(Pattern::Wildcard, unit_body()),
                    ]))]),
                    is_pub: false, is_async: false, must_use: false, span: span(),
                }],
                span: span(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_item_store() {
        let program = Program {
            items: vec![Item::Store(StoreDef {
                name: "S".to_string(), signals: vec![],
                actions: vec![ActionDef {
                    name: "a".to_string(), params: vec![],
                    body: block(vec![Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))]),
                    is_async: false, span: span(),
                }],
                computed: vec![ComputedDef {
                    name: "c".to_string(), return_type: None,
                    body: block(vec![Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))]),
                    span: span(),
                }],
                effects: vec![EffectDef {
                    name: "e".to_string(),
                    body: block(vec![Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))]),
                    span: span(),
                }],
                selectors: vec![], is_pub: false, span: span(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_item_agent() {
        let program = Program {
            items: vec![Item::Agent(AgentDef {
                name: "A".to_string(), system_prompt: None,
                tools: vec![ToolDef {
                    name: "t".to_string(), description: None, params: vec![],
                    return_type: None,
                    body: block(vec![Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))]),
                    span: span(),
                }],
                state: vec![],
                methods: vec![Function {
                    name: "m".to_string(), lifetimes: vec![], type_params: vec![],
                    params: vec![], return_type: None, trait_bounds: vec![],
                    body: block(vec![Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))]),
                    is_pub: false, is_async: false, must_use: false, span: span(),
                }],
                render: None, span: span(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_item_test() {
        let program = Program {
            items: vec![Item::Test(TestDef {
                name: "test".to_string(),
                body: block(vec![Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))]),
                span: span(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    #[test]
    fn walk_item_channel() {
        let program = Program {
            items: vec![Item::Channel(ChannelDef {
                name: "Ch".to_string(),
                url: Expr::StringLit("/ws".to_string()),
                provider: None,
                contract: None,
                on_message: Some(Function {
                    name: "on_msg".to_string(), lifetimes: vec![], type_params: vec![],
                    params: vec![], return_type: None, trait_bounds: vec![],
                    body: block(vec![Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))]),
                    is_pub: false, is_async: false, must_use: false, span: span(),
                }),
                on_connect: Some(Function {
                    name: "on_conn".to_string(), lifetimes: vec![], type_params: vec![],
                    params: vec![], return_type: None, trait_bounds: vec![],
                    body: block(vec![Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))]),
                    is_pub: false, is_async: false, must_use: false, span: span(),
                }),
                on_disconnect: Some(Function {
                    name: "on_disc".to_string(), lifetimes: vec![], type_params: vec![],
                    params: vec![], return_type: None, trait_bounds: vec![],
                    body: block(vec![Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))]),
                    is_pub: false, is_async: false, must_use: false, span: span(),
                }),
                reconnect: false, heartbeat_interval: None,
                methods: vec![Function {
                    name: "send".to_string(), lifetimes: vec![], type_params: vec![],
                    params: vec![], return_type: None, trait_bounds: vec![],
                    body: block(vec![Stmt::Expr(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))]),
                    is_pub: false, is_async: false, must_use: false, span: span(),
                }],
                is_pub: false, span: span(),
            })],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    // -----------------------------------------------------------------------
    // walk_block coverage for Signal, Let, Return stmts
    // -----------------------------------------------------------------------

    #[test]
    fn walk_block_let_and_signal() {
        let program = Program {
            items: vec![wrap_in_fn(Expr::Block(block(vec![
                Stmt::Let {
                    name: "x".to_string(), ty: None, mutable: false, secret: false,
                    value: match_expr(vec![arm(Pattern::Wildcard, unit_body())]),
                    ownership: Ownership::Owned,
                },
                Stmt::Signal {
                    name: "s".to_string(), ty: None, secret: false, atomic: false,
                    value: match_expr(vec![arm(Pattern::Wildcard, unit_body())]),
                },
                Stmt::Return(Some(match_expr(vec![arm(Pattern::Wildcard, unit_body())]))),
                Stmt::Return(None),
                Stmt::Yield(match_expr(vec![arm(Pattern::Wildcard, unit_body())])),
            ])))],
        };
        assert!(check_exhaustiveness(&program).is_empty());
    }

    // -----------------------------------------------------------------------
    // Test: Bool match with duplicate true
    // -----------------------------------------------------------------------

    #[test]
    fn bool_match_duplicate_true() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Literal(Expr::Bool(true)), unit_body()),
                arm(Pattern::Literal(Expr::Bool(true)), unit_body()), // duplicate
                arm(Pattern::Literal(Expr::Bool(false)), unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("redundant")),
            "expected redundant true, got: {:?}",
            errors,
        );
    }

    #[test]
    fn bool_match_duplicate_false() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Literal(Expr::Bool(false)), unit_body()),
                arm(Pattern::Literal(Expr::Bool(false)), unit_body()), // duplicate
                arm(Pattern::Literal(Expr::Bool(true)), unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("redundant")),
            "expected redundant false, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: Enum variant after wildcard
    // -----------------------------------------------------------------------

    #[test]
    fn enum_variant_after_wildcard_redundant() {
        let program = Program {
            items: vec![
                make_enum("Color", vec![("Red", 0), ("Green", 0)]),
                wrap_in_fn(match_expr(vec![
                    arm(Pattern::Variant { name: "Red".into(), fields: vec![] }, unit_body()),
                    arm(Pattern::Wildcard, unit_body()),
                    arm(Pattern::Variant { name: "Green".into(), fields: vec![] }, unit_body()), // redundant
                ])),
            ],
        };
        let errors = check_exhaustiveness(&program);
        assert!(
            errors.iter().any(|e| e.message.contains("redundant") && e.message.contains("Green")),
            "expected redundant Green after wildcard, got: {:?}",
            errors,
        );
    }

    // -----------------------------------------------------------------------
    // Test: Integer match with redundant wildcard after wildcard
    // -----------------------------------------------------------------------

    #[test]
    fn integer_match_double_wildcard() {
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Wildcard, unit_body()),
                arm(Pattern::Wildcard, unit_body()), // redundant
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.iter().any(|e| e.message.contains("redundant")));
    }

    #[test]
    fn match_with_guard_field() {
        // Ensure MatchArm with guard field works in exhaustiveness
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                MatchArm {
                    pattern: Pattern::Literal(Expr::Integer(1)),
                    guard: Some(Expr::Bool(true)),
                    body: unit_body(),
                },
                arm(Pattern::Wildcard, unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.is_empty(), "Match with guard should be exhaustive with wildcard: {:?}", errors);
    }

    #[test]
    fn qualified_variant_pattern() {
        // Ensure qualified names like "Enum::Variant" in patterns work
        let program = Program {
            items: vec![wrap_in_fn(match_expr(vec![
                arm(Pattern::Variant { name: "Color::Red".to_string(), fields: vec![] }, unit_body()),
                arm(Pattern::Wildcard, unit_body()),
            ]))],
        };
        let errors = check_exhaustiveness(&program);
        assert!(errors.is_empty(), "Qualified variant with wildcard should be exhaustive: {:?}", errors);
    }
}
