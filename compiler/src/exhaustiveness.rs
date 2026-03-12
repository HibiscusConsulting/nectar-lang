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
        | Expr::Stream { source: inner } | Expr::Spawn { body: inner }
        | Expr::Receive { channel: inner } | Expr::Navigate { path: inner } => {
            walk_expr(inner, enums, errors);
        }
        Expr::Assign { target, value } => {
            walk_expr(target, enums, errors);
            walk_expr(value, enums, errors);
        }
        Expr::Fetch { url, options } => {
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
        Expr::Parallel { exprs } => {
            for e in exprs {
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
        Item::Struct(_) | Item::Enum(_) | Item::Use(_) | Item::Router(_)
        | Item::LazyComponent(_) | Item::Trait(_) | Item::Mod(_) => {}
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
        MatchArm { pattern, body }
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
}
