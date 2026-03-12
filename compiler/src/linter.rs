/// Static analysis linter for the Nectar language.
///
/// Runs a configurable set of lint rules over a parsed AST and returns
/// a list of warnings with file positions.

use std::collections::{HashMap, HashSet};
use crate::ast::*;
use crate::token::Span;

// =========================================================================
// Public types
// =========================================================================

/// Severity level for a lint warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// A single lint diagnostic.
#[derive(Debug, Clone)]
pub struct LintWarning {
    /// Machine-readable rule id (e.g. `"unused-variable"`).
    pub rule: String,
    /// Human-readable explanation.
    pub message: String,
    /// Location in source.
    pub span: Span,
    /// Severity level.
    pub severity: Severity,
}

/// Which rules are enabled (all default to `true`).
#[derive(Debug, Clone)]
pub struct LintConfig {
    pub unused_variable: bool,
    pub unused_function: bool,
    pub unused_import: bool,
    pub mutable_not_mutated: bool,
    pub empty_block: bool,
    pub snake_case_functions: bool,
    pub pascal_case_types: bool,
    pub unreachable_code: bool,
    pub single_match: bool,
    pub redundant_clone: bool,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            unused_variable: true,
            unused_function: true,
            unused_import: true,
            mutable_not_mutated: true,
            empty_block: true,
            snake_case_functions: true,
            pascal_case_types: true,
            unreachable_code: true,
            single_match: true,
            redundant_clone: true,
        }
    }
}

// =========================================================================
// Linter
// =========================================================================

struct Linter {
    config: LintConfig,
    warnings: Vec<LintWarning>,
}

impl Linter {
    fn new(config: LintConfig) -> Self {
        Self {
            config,
            warnings: Vec::new(),
        }
    }

    fn warn(&mut self, rule: &str, message: String, span: Span, severity: Severity) {
        self.warnings.push(LintWarning {
            rule: rule.to_string(),
            message,
            span,
            severity,
        });
    }

    // ------------------------------------------------------------------
    // Main traversal
    // ------------------------------------------------------------------

    fn lint_program(&mut self, program: &Program) {
        // Collect all defined private function names and all called function names
        // for the unused-function rule.
        let mut defined_fns: HashMap<String, Span> = HashMap::new();
        let mut called_fns: HashSet<String> = HashSet::new();

        // Collect use-path names for unused-import rule
        let mut imported_names: HashMap<String, Span> = HashMap::new();
        let mut referenced_names: HashSet<String> = HashSet::new();

        // First pass: collect definitions & references
        for item in &program.items {
            match item {
                Item::Function(f) => {
                    if !f.is_pub {
                        defined_fns.insert(f.name.clone(), f.span);
                    }
                    self.collect_calls_in_block(&f.body, &mut called_fns);
                    self.collect_refs_in_block(&f.body, &mut referenced_names);
                }
                Item::Use(u) => {
                    if let Some(last) = u.segments.last() {
                        imported_names.insert(last.clone(), u.span);
                    }
                }
                Item::Component(c) => {
                    self.collect_calls_in_component(c, &mut called_fns);
                    self.collect_refs_in_component(c, &mut referenced_names);
                }
                Item::Impl(im) => {
                    for m in &im.methods {
                        self.collect_calls_in_block(&m.body, &mut called_fns);
                        self.collect_refs_in_block(&m.body, &mut referenced_names);
                    }
                }
                Item::Store(s) => {
                    for a in &s.actions {
                        self.collect_calls_in_block(&a.body, &mut called_fns);
                        self.collect_refs_in_block(&a.body, &mut referenced_names);
                    }
                    for c in &s.computed {
                        self.collect_calls_in_block(&c.body, &mut called_fns);
                        self.collect_refs_in_block(&c.body, &mut referenced_names);
                    }
                    for e in &s.effects {
                        self.collect_calls_in_block(&e.body, &mut called_fns);
                        self.collect_refs_in_block(&e.body, &mut referenced_names);
                    }
                }
                Item::Agent(a) => {
                    for t in &a.tools {
                        self.collect_calls_in_block(&t.body, &mut called_fns);
                        self.collect_refs_in_block(&t.body, &mut referenced_names);
                    }
                    for m in &a.methods {
                        self.collect_calls_in_block(&m.body, &mut called_fns);
                        self.collect_refs_in_block(&m.body, &mut referenced_names);
                    }
                }
                Item::Test(t) => {
                    self.collect_calls_in_block(&t.body, &mut called_fns);
                    self.collect_refs_in_block(&t.body, &mut referenced_names);
                }
                _ => {}
            }
        }

        // Emit unused-function warnings
        if self.config.unused_function {
            for (name, span) in &defined_fns {
                if !called_fns.contains(name) {
                    self.warn(
                        "unused-function",
                        format!("function `{}` is defined but never called", name),
                        *span,
                        Severity::Warning,
                    );
                }
            }
        }

        // Emit unused-import warnings
        if self.config.unused_import {
            for (name, span) in &imported_names {
                if !referenced_names.contains(name) && !called_fns.contains(name) {
                    self.warn(
                        "unused-import",
                        format!("imported name `{}` is never used", name),
                        *span,
                        Severity::Warning,
                    );
                }
            }
        }

        // Second pass: per-item lints
        for item in &program.items {
            self.lint_item(item);
        }
    }

    fn lint_item(&mut self, item: &Item) {
        match item {
            Item::Function(f) => {
                if self.config.snake_case_functions {
                    self.check_snake_case(&f.name, f.span, "function");
                }
                self.lint_function_body(f);
            }
            Item::Component(c) => {
                if self.config.pascal_case_types {
                    self.check_pascal_case(&c.name, c.span, "component");
                }
                self.lint_component(c);
            }
            Item::Struct(s) => {
                if self.config.pascal_case_types {
                    self.check_pascal_case(&s.name, s.span, "struct");
                }
            }
            Item::Enum(e) => {
                if self.config.pascal_case_types {
                    self.check_pascal_case(&e.name, e.span, "enum");
                }
            }
            Item::Impl(im) => {
                for m in &im.methods {
                    if self.config.snake_case_functions {
                        self.check_snake_case(&m.name, m.span, "method");
                    }
                    self.lint_function_body(m);
                }
            }
            Item::Trait(t) => {
                if self.config.pascal_case_types {
                    self.check_pascal_case(&t.name, t.span, "trait");
                }
                for m in &t.methods {
                    if self.config.snake_case_functions {
                        self.check_snake_case(&m.name, m.span, "trait method");
                    }
                }
            }
            Item::Store(s) => {
                if self.config.pascal_case_types {
                    self.check_pascal_case(&s.name, s.span, "store");
                }
                for a in &s.actions {
                    self.lint_block(&a.body);
                }
                for c in &s.computed {
                    self.lint_block(&c.body);
                }
                for e in &s.effects {
                    self.lint_block(&e.body);
                }
            }
            Item::Agent(a) => {
                for t in &a.tools {
                    self.lint_block(&t.body);
                }
                for m in &a.methods {
                    self.lint_function_body(m);
                }
            }
            Item::Test(t) => {
                self.lint_block(&t.body);
            }
            Item::LazyComponent(lc) => {
                if self.config.pascal_case_types {
                    self.check_pascal_case(&lc.component.name, lc.span, "component");
                }
                self.lint_component(&lc.component);
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Per-function linting
    // ------------------------------------------------------------------

    fn lint_function_body(&mut self, f: &Function) {
        // Check empty block
        if self.config.empty_block && f.body.stmts.is_empty() {
            self.warn(
                "empty-block",
                format!("function `{}` has an empty body", f.name),
                f.span,
                Severity::Warning,
            );
        }

        self.lint_block(&f.body);
    }

    fn lint_component(&mut self, c: &Component) {
        for m in &c.methods {
            if self.config.snake_case_functions {
                self.check_snake_case(&m.name, m.span, "method");
            }
            self.lint_function_body(m);
        }
    }

    // ------------------------------------------------------------------
    // Block-level linting
    // ------------------------------------------------------------------

    fn lint_block(&mut self, block: &Block) {
        // Collect declared variables and track usage for unused-variable
        // and mutable-not-mutated.
        let mut declared_vars: HashMap<String, (Span, bool)> = HashMap::new(); // name -> (span, mutable)
        let mut used_vars: HashSet<String> = HashSet::new();
        let mut mutated_vars: HashSet<String> = HashSet::new();

        for stmt in &block.stmts {
            match stmt {
                Stmt::Let {
                    name,
                    mutable,
                    value,
                    ..
                } => {
                    declared_vars.insert(name.clone(), (block.span, *mutable));
                    self.collect_idents_in_expr(value, &mut used_vars);
                    self.collect_assignments_in_expr(value, &mut mutated_vars);
                }
                Stmt::Signal { value, .. } => {
                    self.collect_idents_in_expr(value, &mut used_vars);
                }
                Stmt::Expr(expr) => {
                    self.collect_idents_in_expr(expr, &mut used_vars);
                    self.collect_assignments_in_expr(expr, &mut mutated_vars);
                }
                Stmt::Return(Some(expr)) => {
                    self.collect_idents_in_expr(expr, &mut used_vars);
                }
                Stmt::Return(None) => {}
                Stmt::Yield(expr) => {
                    self.collect_idents_in_expr(expr, &mut used_vars);
                }
                Stmt::LetDestructure { value, .. } => {
                    self.collect_idents_in_expr(value, &mut used_vars);
                    self.collect_assignments_in_expr(value, &mut mutated_vars);
                }
            }
        }

        // Unused variables
        if self.config.unused_variable {
            for (name, (span, _mutable)) in &declared_vars {
                if !name.starts_with('_') && !used_vars.contains(name) {
                    self.warn(
                        "unused-variable",
                        format!("variable `{}` is declared but never used", name),
                        *span,
                        Severity::Warning,
                    );
                }
            }
        }

        // Mutable but never mutated
        if self.config.mutable_not_mutated {
            for (name, (span, mutable)) in &declared_vars {
                if *mutable && !mutated_vars.contains(name) {
                    self.warn(
                        "mutable-not-mutated",
                        format!(
                            "variable `{}` is declared as `mut` but is never mutated",
                            name
                        ),
                        *span,
                        Severity::Warning,
                    );
                }
            }
        }

        // Unreachable code: any statement after a return
        if self.config.unreachable_code {
            let mut saw_return = false;
            for stmt in &block.stmts {
                if saw_return {
                    let span = match stmt {
                        Stmt::Let { .. } => block.span,
                        Stmt::Signal { .. } => block.span,
                        Stmt::Expr(_) => block.span,
                        Stmt::Return(_) => block.span,
                        Stmt::Yield(_) => block.span,
                        Stmt::LetDestructure { .. } => block.span,
                    };
                    self.warn(
                        "unreachable-code",
                        "unreachable code after return statement".to_string(),
                        span,
                        Severity::Warning,
                    );
                    break; // Only report once per block
                }
                if matches!(stmt, Stmt::Return(_)) {
                    saw_return = true;
                }
            }
        }

        // Recurse into nested expressions for expression-level lints
        for stmt in &block.stmts {
            match stmt {
                Stmt::Expr(expr) | Stmt::Return(Some(expr)) | Stmt::Yield(expr) => {
                    self.lint_expr(expr);
                }
                Stmt::Let { value, .. }
                | Stmt::Signal { value, .. }
                | Stmt::LetDestructure { value, .. } => {
                    self.lint_expr(value);
                }
                Stmt::Return(None) => {}
            }
        }
    }

    // ------------------------------------------------------------------
    // Expression-level linting
    // ------------------------------------------------------------------

    fn lint_expr(&mut self, expr: &Expr) {
        match expr {
            // Empty block inside if
            Expr::If {
                then_block,
                else_block,
                condition,
                ..
            } => {
                if self.config.empty_block && then_block.stmts.is_empty() {
                    self.warn(
                        "empty-block",
                        "if block has an empty body".to_string(),
                        then_block.span,
                        Severity::Warning,
                    );
                }
                if let Some(eb) = else_block {
                    if self.config.empty_block && eb.stmts.is_empty() {
                        self.warn(
                            "empty-block",
                            "else block has an empty body".to_string(),
                            eb.span,
                            Severity::Warning,
                        );
                    }
                    self.lint_block(eb);
                }
                self.lint_expr(condition);
                self.lint_block(then_block);
            }

            // Single-match: match with exactly one arm + wildcard
            Expr::Match { arms, subject, .. } => {
                if self.config.single_match && arms.len() == 2 {
                    let has_wildcard = arms.iter().any(|a| matches!(a.pattern, Pattern::Wildcard));
                    if has_wildcard {
                        self.warn(
                            "single-match",
                            "this `match` has a single non-wildcard arm; consider using `if let`"
                                .to_string(),
                            Span::new(0, 0, 1, 1), // best-effort span
                            Severity::Info,
                        );
                    }
                }
                self.lint_expr(subject);
                for arm in arms {
                    self.lint_expr(&arm.body);
                }
            }

            Expr::For { body, iterator, .. } => {
                self.lint_expr(iterator);
                self.lint_block(body);
            }
            Expr::While { condition, body } => {
                self.lint_expr(condition);
                self.lint_block(body);
            }
            Expr::Block(block) => self.lint_block(block),

            // Redundant clone: x.clone() where x is never used again.
            // This is a heuristic — we flag any .clone() call and suggest review.
            Expr::MethodCall {
                method, object, args, ..
            } => {
                if self.config.redundant_clone && method == "clone" && args.is_empty() {
                    if let Expr::Ident(name) = object.as_ref() {
                        self.warn(
                            "redundant-clone",
                            format!(
                                "`{}.clone()` may be redundant if `{}` is not used afterwards — consider moving instead",
                                name, name
                            ),
                            Span::new(0, 0, 1, 1),
                            Severity::Info,
                        );
                    }
                }
                self.lint_expr(object);
                for a in args {
                    self.lint_expr(a);
                }
            }

            // Recurse into sub-expressions
            Expr::Binary { left, right, .. } => {
                self.lint_expr(left);
                self.lint_expr(right);
            }
            Expr::Unary { operand, .. } => self.lint_expr(operand),
            Expr::FieldAccess { object, .. } => self.lint_expr(object),
            Expr::FnCall { callee, args } => {
                self.lint_expr(callee);
                for a in args {
                    self.lint_expr(a);
                }
            }
            Expr::Index { object, index } => {
                self.lint_expr(object);
                self.lint_expr(index);
            }
            Expr::Assign { target, value } => {
                self.lint_expr(target);
                self.lint_expr(value);
            }
            Expr::Closure { body, .. } => self.lint_expr(body),
            Expr::Await(inner)
            | Expr::Borrow(inner)
            | Expr::BorrowMut(inner)
            | Expr::Spawn { body: inner }
            | Expr::Navigate { path: inner }
            | Expr::Stream { source: inner } => self.lint_expr(inner),
            Expr::StructInit { fields, .. } => {
                for (_, v) in fields {
                    self.lint_expr(v);
                }
            }
            Expr::TryCatch {
                body, catch_body, ..
            } => {
                self.lint_expr(body);
                self.lint_expr(catch_body);
            }
            Expr::Fetch { url, options } => {
                self.lint_expr(url);
                if let Some(o) = options {
                    self.lint_expr(o);
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Naming conventions
    // ------------------------------------------------------------------

    fn check_snake_case(&mut self, name: &str, span: Span, kind: &str) {
        if !is_snake_case(name) {
            self.warn(
                "snake-case-functions",
                format!(
                    "{} `{}` should use snake_case naming",
                    kind, name
                ),
                span,
                Severity::Warning,
            );
        }
    }

    fn check_pascal_case(&mut self, name: &str, span: Span, kind: &str) {
        if !is_pascal_case(name) {
            self.warn(
                "pascal-case-types",
                format!(
                    "{} `{}` should use PascalCase naming",
                    kind, name
                ),
                span,
                Severity::Warning,
            );
        }
    }

    // ------------------------------------------------------------------
    // Reference / call collection helpers
    // ------------------------------------------------------------------

    fn collect_calls_in_block(&self, block: &Block, calls: &mut HashSet<String>) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Let { value, .. } | Stmt::Signal { value, .. } => {
                    self.collect_calls_in_expr(value, calls);
                }
                Stmt::Expr(e) | Stmt::Return(Some(e)) | Stmt::Yield(e) => {
                    self.collect_calls_in_expr(e, calls);
                }
                Stmt::Return(None) => {}
                _ => {}
            }
        }
    }

    fn collect_calls_in_expr(&self, expr: &Expr, calls: &mut HashSet<String>) {
        match expr {
            Expr::FnCall { callee, args } => {
                if let Expr::Ident(name) = callee.as_ref() {
                    calls.insert(name.clone());
                }
                for a in args {
                    self.collect_calls_in_expr(a, calls);
                }
            }
            Expr::MethodCall { object, args, .. } => {
                self.collect_calls_in_expr(object, calls);
                for a in args {
                    self.collect_calls_in_expr(a, calls);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.collect_calls_in_expr(left, calls);
                self.collect_calls_in_expr(right, calls);
            }
            Expr::Unary { operand, .. } => self.collect_calls_in_expr(operand, calls),
            Expr::If {
                condition,
                then_block,
                else_block,
            } => {
                self.collect_calls_in_expr(condition, calls);
                self.collect_calls_in_block(then_block, calls);
                if let Some(eb) = else_block {
                    self.collect_calls_in_block(eb, calls);
                }
            }
            Expr::Match { subject, arms } => {
                self.collect_calls_in_expr(subject, calls);
                for arm in arms {
                    self.collect_calls_in_expr(&arm.body, calls);
                }
            }
            Expr::For { iterator, body, .. } => {
                self.collect_calls_in_expr(iterator, calls);
                self.collect_calls_in_block(body, calls);
            }
            Expr::While { condition, body } => {
                self.collect_calls_in_expr(condition, calls);
                self.collect_calls_in_block(body, calls);
            }
            Expr::Block(block) => self.collect_calls_in_block(block, calls),
            Expr::Closure { body, .. } => self.collect_calls_in_expr(body, calls),
            Expr::Assign { target, value } => {
                self.collect_calls_in_expr(target, calls);
                self.collect_calls_in_expr(value, calls);
            }
            Expr::Await(inner)
            | Expr::Borrow(inner)
            | Expr::BorrowMut(inner)
            | Expr::Spawn { body: inner }
            | Expr::Navigate { path: inner }
            | Expr::Stream { source: inner } => {
                self.collect_calls_in_expr(inner, calls);
            }
            Expr::FieldAccess { object, .. } | Expr::Index { object, .. } => {
                self.collect_calls_in_expr(object, calls);
            }
            Expr::StructInit { fields, .. } => {
                for (_, v) in fields {
                    self.collect_calls_in_expr(v, calls);
                }
            }
            Expr::TryCatch {
                body, catch_body, ..
            } => {
                self.collect_calls_in_expr(body, calls);
                self.collect_calls_in_expr(catch_body, calls);
            }
            Expr::Fetch { url, options } => {
                self.collect_calls_in_expr(url, calls);
                if let Some(o) = options {
                    self.collect_calls_in_expr(o, calls);
                }
            }
            _ => {}
        }
    }

    fn collect_calls_in_component(&self, c: &Component, calls: &mut HashSet<String>) {
        for m in &c.methods {
            self.collect_calls_in_block(&m.body, calls);
        }
    }

    fn collect_refs_in_block(&self, block: &Block, refs: &mut HashSet<String>) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Let { value, .. } | Stmt::Signal { value, .. } => {
                    self.collect_idents_in_expr(value, refs);
                }
                Stmt::Expr(e) | Stmt::Return(Some(e)) | Stmt::Yield(e) => {
                    self.collect_idents_in_expr(e, refs);
                }
                Stmt::Return(None) => {}
                _ => {}
            }
        }
    }

    fn collect_refs_in_component(&self, c: &Component, refs: &mut HashSet<String>) {
        for m in &c.methods {
            self.collect_refs_in_block(&m.body, refs);
        }
    }

    fn collect_idents_in_expr(&self, expr: &Expr, idents: &mut HashSet<String>) {
        match expr {
            Expr::Ident(name) => {
                idents.insert(name.clone());
            }
            Expr::FnCall { callee, args } => {
                self.collect_idents_in_expr(callee, idents);
                for a in args {
                    self.collect_idents_in_expr(a, idents);
                }
            }
            Expr::MethodCall { object, args, .. } => {
                self.collect_idents_in_expr(object, idents);
                for a in args {
                    self.collect_idents_in_expr(a, idents);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.collect_idents_in_expr(left, idents);
                self.collect_idents_in_expr(right, idents);
            }
            Expr::Unary { operand, .. } => self.collect_idents_in_expr(operand, idents),
            Expr::FieldAccess { object, .. } | Expr::Index { object, .. } => {
                self.collect_idents_in_expr(object, idents);
            }
            Expr::If {
                condition,
                then_block,
                else_block,
            } => {
                self.collect_idents_in_expr(condition, idents);
                self.collect_refs_in_block(then_block, idents);
                if let Some(eb) = else_block {
                    self.collect_refs_in_block(eb, idents);
                }
            }
            Expr::Match { subject, arms } => {
                self.collect_idents_in_expr(subject, idents);
                for arm in arms {
                    self.collect_idents_in_expr(&arm.body, idents);
                }
            }
            Expr::For { iterator, body, .. } => {
                self.collect_idents_in_expr(iterator, idents);
                self.collect_refs_in_block(body, idents);
            }
            Expr::While { condition, body } => {
                self.collect_idents_in_expr(condition, idents);
                self.collect_refs_in_block(body, idents);
            }
            Expr::Block(block) => self.collect_refs_in_block(block, idents),
            Expr::Closure { body, .. } => self.collect_idents_in_expr(body, idents),
            Expr::Assign { target, value } => {
                self.collect_idents_in_expr(target, idents);
                self.collect_idents_in_expr(value, idents);
            }
            Expr::StructInit { name, fields } => {
                idents.insert(name.clone());
                for (_, v) in fields {
                    self.collect_idents_in_expr(v, idents);
                }
            }
            Expr::Await(inner)
            | Expr::Borrow(inner)
            | Expr::BorrowMut(inner)
            | Expr::Spawn { body: inner }
            | Expr::Navigate { path: inner }
            | Expr::Stream { source: inner } => {
                self.collect_idents_in_expr(inner, idents);
            }
            Expr::TryCatch {
                body, catch_body, ..
            } => {
                self.collect_idents_in_expr(body, idents);
                self.collect_idents_in_expr(catch_body, idents);
            }
            Expr::Fetch { url, options } => {
                self.collect_idents_in_expr(url, idents);
                if let Some(o) = options {
                    self.collect_idents_in_expr(o, idents);
                }
            }
            _ => {}
        }
    }

    fn collect_assignments_in_expr(&self, expr: &Expr, mutated: &mut HashSet<String>) {
        match expr {
            Expr::Assign { target, value } => {
                if let Expr::Ident(name) = target.as_ref() {
                    mutated.insert(name.clone());
                }
                self.collect_assignments_in_expr(value, mutated);
            }
            Expr::Binary { left, right, .. } => {
                self.collect_assignments_in_expr(left, mutated);
                self.collect_assignments_in_expr(right, mutated);
            }
            Expr::If {
                condition,
                then_block,
                else_block,
            } => {
                self.collect_assignments_in_expr(condition, mutated);
                for stmt in &then_block.stmts {
                    if let Stmt::Expr(e) = stmt {
                        self.collect_assignments_in_expr(e, mutated);
                    }
                }
                if let Some(eb) = else_block {
                    for stmt in &eb.stmts {
                        if let Stmt::Expr(e) = stmt {
                            self.collect_assignments_in_expr(e, mutated);
                        }
                    }
                }
            }
            Expr::Block(block) => {
                for stmt in &block.stmts {
                    if let Stmt::Expr(e) = stmt {
                        self.collect_assignments_in_expr(e, mutated);
                    }
                }
            }
            Expr::FnCall { callee, args } => {
                self.collect_assignments_in_expr(callee, mutated);
                for a in args {
                    self.collect_assignments_in_expr(a, mutated);
                }
            }
            Expr::MethodCall { object, args, .. } => {
                self.collect_assignments_in_expr(object, mutated);
                for a in args {
                    self.collect_assignments_in_expr(a, mutated);
                }
            }
            Expr::Closure { body, .. } => {
                self.collect_assignments_in_expr(body, mutated);
            }
            _ => {}
        }
    }
}

// =========================================================================
// Naming convention helpers
// =========================================================================

fn is_snake_case(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // snake_case: lowercase letters, digits, underscores; no uppercase; doesn't start with digit
    name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}

fn is_pascal_case(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // PascalCase: starts with uppercase letter, no underscores
    name.starts_with(|c: char| c.is_ascii_uppercase())
        && !name.contains('_')
}

// =========================================================================
// Public entry point
// =========================================================================

/// Lint a program with default configuration. Returns all warnings.
pub fn lint_program(program: &Program) -> Vec<LintWarning> {
    lint_program_with_config(program, LintConfig::default())
}

/// Lint a program with a specific configuration.
pub fn lint_program_with_config(program: &Program, config: LintConfig) -> Vec<LintWarning> {
    let mut linter = Linter::new(config);
    linter.lint_program(program);
    linter.warnings
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    #[test]
    fn test_unused_variable_detected() {
        let program = Program {
            items: vec![Item::Function(Function {
                name: "foo".to_string(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![
                        Stmt::Let {
                            name: "x".to_string(),
                            ty: None,
                            mutable: false,
                            value: Expr::Integer(42),
                            ownership: Ownership::Owned,
                        },
                        // x is never used after declaration
                    ],
                    span: dummy_span(),
                },
                is_pub: false,
                span: dummy_span(),
            })],
        };

        let warnings = lint_program(&program);
        let has_unused = warnings
            .iter()
            .any(|w| w.rule == "unused-variable" && w.message.contains("`x`"));
        assert!(has_unused, "Expected unused-variable warning for `x`, got: {:?}", warnings);
    }

    #[test]
    fn test_snake_case_violation_detected() {
        let program = Program {
            items: vec![Item::Function(Function {
                name: "myFunction".to_string(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(None)],
                    span: dummy_span(),
                },
                is_pub: false,
                span: dummy_span(),
            })],
        };

        let warnings = lint_program(&program);
        let has_snake = warnings
            .iter()
            .any(|w| w.rule == "snake-case-functions" && w.message.contains("`myFunction`"));
        assert!(has_snake, "Expected snake-case-functions warning, got: {:?}", warnings);
    }

    #[test]
    fn test_empty_block_detected() {
        let program = Program {
            items: vec![Item::Function(Function {
                name: "empty".to_string(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![], // empty!
                    span: dummy_span(),
                },
                is_pub: false,
                span: dummy_span(),
            })],
        };

        let warnings = lint_program(&program);
        let has_empty = warnings
            .iter()
            .any(|w| w.rule == "empty-block" && w.message.contains("`empty`"));
        assert!(has_empty, "Expected empty-block warning, got: {:?}", warnings);
    }

    #[test]
    fn test_pascal_case_type_names_enforced() {
        let program = Program {
            items: vec![Item::Struct(StructDef {
                name: "my_struct".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                fields: vec![],
                trait_bounds: vec![],
                is_pub: false,
                span: dummy_span(),
            })],
        };

        let warnings = lint_program(&program);
        let has_pascal = warnings
            .iter()
            .any(|w| w.rule == "pascal-case-types" && w.message.contains("`my_struct`"));
        assert!(has_pascal, "Expected pascal-case-types warning, got: {:?}", warnings);
    }

    #[test]
    fn test_unreachable_code_detected() {
        let program = Program {
            items: vec![Item::Function(Function {
                name: "early_return".to_string(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(1))),
                        Stmt::Expr(Expr::Integer(2)), // unreachable
                    ],
                    span: dummy_span(),
                },
                is_pub: false,
                span: dummy_span(),
            })],
        };

        let warnings = lint_program(&program);
        let has_unreachable = warnings.iter().any(|w| w.rule == "unreachable-code");
        assert!(has_unreachable, "Expected unreachable-code warning, got: {:?}", warnings);
    }

    #[test]
    fn test_mutable_not_mutated() {
        let program = Program {
            items: vec![Item::Function(Function {
                name: "no_mutate".to_string(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![
                        Stmt::Let {
                            name: "x".to_string(),
                            ty: None,
                            mutable: true,
                            value: Expr::Integer(0),
                            ownership: Ownership::Owned,
                        },
                        // x is used but never assigned to after declaration
                        Stmt::Expr(Expr::Ident("x".to_string())),
                    ],
                    span: dummy_span(),
                },
                is_pub: false,
                span: dummy_span(),
            })],
        };

        let warnings = lint_program(&program);
        let has_mut = warnings
            .iter()
            .any(|w| w.rule == "mutable-not-mutated" && w.message.contains("`x`"));
        assert!(has_mut, "Expected mutable-not-mutated warning, got: {:?}", warnings);
    }

    #[test]
    fn test_single_match_suggestion() {
        let program = Program {
            items: vec![Item::Function(Function {
                name: "check".to_string(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Expr(Expr::Match {
                        subject: Box::new(Expr::Ident("x".to_string())),
                        arms: vec![
                            MatchArm {
                                pattern: Pattern::Variant {
                                    name: "Some".to_string(),
                                    fields: vec![Pattern::Ident("v".to_string())],
                                },
                                body: Expr::Ident("v".to_string()),
                            },
                            MatchArm {
                                pattern: Pattern::Wildcard,
                                body: Expr::Integer(0),
                            },
                        ],
                    })],
                    span: dummy_span(),
                },
                is_pub: false,
                span: dummy_span(),
            })],
        };

        let warnings = lint_program(&program);
        let has_single = warnings.iter().any(|w| w.rule == "single-match");
        assert!(has_single, "Expected single-match suggestion, got: {:?}", warnings);
    }
}
