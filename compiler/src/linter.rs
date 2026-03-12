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
    pub resource_leak: bool,
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
            resource_leak: true,
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
            Item::Page(page) => {
                if self.config.pascal_case_types {
                    self.check_pascal_case(&page.name, page.span, "page");
                }
                self.lint_page(page);
            }
            Item::Form(form) => {
                if self.config.pascal_case_types {
                    self.check_pascal_case(&form.name, form.span, "form");
                }
                for m in &form.methods {
                    if self.config.snake_case_functions {
                        self.check_snake_case(&m.name, m.span, "method");
                    }
                    self.lint_function_body(m);
                }
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

        // Semantic HTML / a11y linting on component render blocks
        self.lint_semantic_html(&c.render.body, c.span, false);

        // Resource leak detection (Feature 3)
        if self.config.resource_leak {
            self.lint_resource_leaks(c);
        }
    }

    /// Check for potential memory leaks: event listeners, intervals, timeouts,
    /// and subscriptions that are acquired but never cleaned up in on_destroy.
    fn lint_resource_leaks(&mut self, component: &Component) {
        let acquisition_patterns = ["addEventListener", "setInterval", "setTimeout", "subscribe"];
        let release_patterns = ["removeEventListener", "clearInterval", "clearTimeout", "unsubscribe"];

        let mut acquisitions: Vec<(&str, Span)> = Vec::new();

        // Walk methods (excluding on_destroy) for acquisitions
        for method in &component.methods {
            if method.name == "on_destroy" { continue; }
            self.find_acquisition_calls_in_block(&method.body, &acquisition_patterns, &mut acquisitions);
        }

        if acquisitions.is_empty() { return; }

        // Check on_destroy exists
        let has_on_destroy = component.on_destroy.is_some()
            || component.methods.iter().any(|m| m.name == "on_destroy");

        if !has_on_destroy {
            for (acq, span) in &acquisitions {
                self.warn(
                    "resource_leak",
                    format!("component `{}` uses `{}` but has no on_destroy — potential memory leak", component.name, acq),
                    *span,
                    Severity::Warning,
                );
            }
            return;
        }

        // Check that each acquisition has a corresponding release in on_destroy
        let destroy_body = if let Some(ref destroy_fn) = component.on_destroy {
            Some(&destroy_fn.body)
        } else {
            component.methods.iter()
                .find(|m| m.name == "on_destroy")
                .map(|m| &m.body)
        };

        if let Some(destroy_body) = destroy_body {
            let mut releases: Vec<(&str, Span)> = Vec::new();
            self.find_acquisition_calls_in_block(destroy_body, &release_patterns, &mut releases);
            let release_names: Vec<&str> = releases.iter().map(|(n, _)| *n).collect();

            for (acq, span) in &acquisitions {
                let expected_release = match *acq {
                    "addEventListener" => "removeEventListener",
                    "setInterval" => "clearInterval",
                    "setTimeout" => "clearTimeout",
                    "subscribe" => "unsubscribe",
                    _ => continue,
                };
                if !release_names.contains(&expected_release) {
                    self.warn(
                        "resource_leak",
                        format!(
                            "component `{}` calls `{}` but on_destroy does not call `{}` — potential memory leak",
                            component.name, acq, expected_release
                        ),
                        *span,
                        Severity::Warning,
                    );
                }
            }
        }
    }

    /// Walk a block looking for method calls matching any of the given patterns.
    fn find_acquisition_calls_in_block<'a>(
        &self,
        block: &Block,
        patterns: &[&'a str],
        results: &mut Vec<(&'a str, Span)>,
    ) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Expr(expr) | Stmt::Let { value: expr, .. } | Stmt::Signal { value: expr, .. } => {
                    self.find_acquisition_calls_in_expr(expr, patterns, results);
                }
                Stmt::Return(Some(expr)) | Stmt::Yield(expr) => {
                    self.find_acquisition_calls_in_expr(expr, patterns, results);
                }
                _ => {}
            }
        }
    }

    /// Walk an expression tree looking for method calls matching any of the given patterns.
    fn find_acquisition_calls_in_expr<'a>(
        &self,
        expr: &Expr,
        patterns: &[&'a str],
        results: &mut Vec<(&'a str, Span)>,
    ) {
        match expr {
            Expr::MethodCall { object, method, args, .. } => {
                for pattern in patterns {
                    if method == pattern {
                        // Use a dummy span since Expr doesn't carry spans directly
                        results.push((pattern, Span::new(0, 0, 0, 0)));
                    }
                }
                self.find_acquisition_calls_in_expr(object, patterns, results);
                for arg in args {
                    self.find_acquisition_calls_in_expr(arg, patterns, results);
                }
            }
            Expr::FnCall { callee, args } => {
                // Check if callee is an identifier matching a pattern
                if let Expr::Ident(name) = callee.as_ref() {
                    for pattern in patterns {
                        if name == pattern {
                            results.push((pattern, Span::new(0, 0, 0, 0)));
                        }
                    }
                }
                self.find_acquisition_calls_in_expr(callee, patterns, results);
                for arg in args {
                    self.find_acquisition_calls_in_expr(arg, patterns, results);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.find_acquisition_calls_in_expr(left, patterns, results);
                self.find_acquisition_calls_in_expr(right, patterns, results);
            }
            Expr::If { condition, then_block, else_block, .. } => {
                self.find_acquisition_calls_in_expr(condition, patterns, results);
                self.find_acquisition_calls_in_block(then_block, patterns, results);
                if let Some(blk) = else_block {
                    self.find_acquisition_calls_in_block(blk, patterns, results);
                }
            }
            Expr::Block(block) => {
                self.find_acquisition_calls_in_block(block, patterns, results);
            }
            Expr::Assign { target, value } => {
                self.find_acquisition_calls_in_expr(target, patterns, results);
                self.find_acquisition_calls_in_expr(value, patterns, results);
            }
            Expr::Closure { body, .. } => {
                self.find_acquisition_calls_in_expr(body, patterns, results);
            }
            _ => {}
        }
    }

    fn lint_page(&mut self, page: &PageDef) {
        for m in &page.methods {
            if self.config.snake_case_functions {
                self.check_snake_case(&m.name, m.span, "method");
            }
            self.lint_function_body(m);
        }
        // Semantic HTML linting for page render blocks
        self.lint_semantic_html(&page.render.body, page.span, true);
    }

    /// Check the render template tree for semantic HTML issues.
    /// `is_page` indicates whether this is a page component (triggers h1 check).
    fn lint_semantic_html(&mut self, node: &TemplateNode, span: Span, is_page: bool) {
        let mut has_h1 = false;
        self.walk_template_for_semantics(node, &mut has_h1, true);

        if is_page && !has_h1 {
            self.warn(
                "semantic_html",
                "page component should contain an <h1> element for SEO".to_string(),
                span,
                Severity::Warning,
            );
        }
    }

    fn walk_template_for_semantics(&mut self, node: &TemplateNode, has_h1: &mut bool, is_top_level: bool) {
        match node {
            TemplateNode::Element(el) => {
                if el.tag == "h1" {
                    *has_h1 = true;
                }

                // Warn if <div> is used as a top-level wrapper when semantic tags would be better
                if is_top_level && el.tag == "div" {
                    self.warn(
                        "semantic_html",
                        "consider using <main>, <article>, <section>, or <nav> instead of <div> as a top-level wrapper".to_string(),
                        el.span,
                        Severity::Warning,
                    );
                }

                // Warn if <img> has no alt attribute
                if el.tag == "img" {
                    let has_alt = el.attributes.iter().any(|attr| {
                        match attr {
                            Attribute::Static { name, .. } => name == "alt",
                            Attribute::Dynamic { name, .. } => name == "alt",
                            _ => false,
                        }
                    });
                    if !has_alt {
                        self.warn(
                            "semantic_html",
                            "<img> element should have an `alt` attribute for accessibility and SEO".to_string(),
                            el.span,
                            Severity::Warning,
                        );
                    }
                }

                // Warn if <input>, <select>, or <textarea> has no label or aria-label
                if matches!(el.tag.as_str(), "input" | "select" | "textarea") {
                    let has_label = el.attributes.iter().any(|attr| {
                        matches!(attr,
                            Attribute::Aria { name, .. } if name == "aria-label" || name == "aria-labelledby"
                        ) || matches!(attr,
                            Attribute::Static { name, .. } if name == "aria-label" || name == "id"
                        )
                    });
                    if !has_label {
                        self.warn(
                            "a11y_label",
                            format!("<{}> element should have an `aria-label`, `aria-labelledby`, or associated `<label>` for accessibility", el.tag),
                            el.span,
                            Severity::Warning,
                        );
                    }
                }

                // Warn if <button> or <a> has no text content or aria-label
                if matches!(el.tag.as_str(), "button" | "a") {
                    let has_text = el.children.iter().any(|c| {
                        matches!(c, TemplateNode::TextLiteral(s) if !s.trim().is_empty())
                            || matches!(c, TemplateNode::Expression(_))
                    });
                    let has_aria_label = el.attributes.iter().any(|attr| {
                        matches!(attr, Attribute::Aria { name, .. } if name == "aria-label")
                    });
                    if !has_text && !has_aria_label {
                        self.warn(
                            "a11y_label",
                            format!("<{}> element should have text content or `aria-label` for accessibility", el.tag),
                            el.span,
                            Severity::Warning,
                        );
                    }
                }

                // Warn if non-interactive element has on:click without role or tabindex
                let interactive_tags = ["button", "a", "input", "select", "textarea", "details", "summary"];
                if !interactive_tags.contains(&el.tag.as_str()) {
                    let has_click = el.attributes.iter().any(|attr| {
                        matches!(attr, Attribute::EventHandler { event, .. } if event == "click")
                    });
                    if has_click {
                        let has_role = el.attributes.iter().any(|attr| matches!(attr, Attribute::Role { .. }));
                        let has_tabindex = el.attributes.iter().any(|attr| {
                            matches!(attr, Attribute::Static { name, .. } | Attribute::Dynamic { name, .. } if name == "tabindex")
                        });
                        if !has_role {
                            self.warn(
                                "a11y_interactive",
                                format!("<{}> with on:click should have a `role` attribute (e.g. role=\"button\")", el.tag),
                                el.span,
                                Severity::Warning,
                            );
                        }
                        if !has_tabindex {
                            self.warn(
                                "a11y_interactive",
                                format!("<{}> with on:click should have `tabindex=\"0\"` for keyboard accessibility", el.tag),
                                el.span,
                                Severity::Warning,
                            );
                        }
                    }
                }

                // Warn if aria-hidden="true" is on a focusable element
                let is_aria_hidden = el.attributes.iter().any(|attr| {
                    matches!(attr, Attribute::Aria { name, value } if name == "aria-hidden" && matches!(value, Expr::StringLit(s) if s == "true"))
                        || matches!(attr, Attribute::Static { name, value } if name == "aria-hidden" && value == "true")
                });
                if is_aria_hidden {
                    let is_focusable = interactive_tags.contains(&el.tag.as_str())
                        || el.attributes.iter().any(|attr| {
                            matches!(attr, Attribute::Static { name, .. } | Attribute::Dynamic { name, .. } if name == "tabindex")
                        });
                    if is_focusable {
                        self.warn(
                            "a11y_hidden",
                            "element with `aria-hidden=\"true\"` should not be focusable".to_string(),
                            el.span,
                            Severity::Warning,
                        );
                    }
                }

                for child in &el.children {
                    self.walk_template_for_semantics(child, has_h1, false);
                }
            }
            TemplateNode::Fragment(children) => {
                for child in children {
                    self.walk_template_for_semantics(child, has_h1, is_top_level);
                }
            }
            TemplateNode::Link { children, .. } => {
                for child in children {
                    self.walk_template_for_semantics(child, has_h1, false);
                }
            }
            TemplateNode::Layout(layout_node) => {
                let children = match layout_node {
                    LayoutNode::Stack { children, .. }
                    | LayoutNode::Row { children, .. }
                    | LayoutNode::Grid { children, .. }
                    | LayoutNode::Center { children, .. }
                    | LayoutNode::Cluster { children, .. }
                    | LayoutNode::Sidebar { children, .. }
                    | LayoutNode::Switcher { children, .. } => children,
                };
                for child in children {
                    self.walk_template_for_semantics(child, has_h1, false);
                }
            }
            _ => {}
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
            | Expr::Navigate { path: inner }
            | Expr::Stream { source: inner } => self.lint_expr(inner),
            Expr::Spawn { body, .. } => self.lint_block(body),
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
            Expr::Fetch { url, options, .. } => {
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
            | Expr::Navigate { path: inner }
            | Expr::Stream { source: inner } => {
                self.collect_calls_in_expr(inner, calls);
            }
            Expr::Spawn { body, .. } => {
                self.collect_calls_in_block(body, calls);
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
            Expr::Fetch { url, options, .. } => {
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
            | Expr::Navigate { path: inner }
            | Expr::Stream { source: inner } => {
                self.collect_idents_in_expr(inner, idents);
            }
            Expr::Spawn { body, .. } => {
                for s in &body.stmts {
                    match s {
                        Stmt::Expr(e) | Stmt::Let { value: e, .. } | Stmt::Signal { value: e, .. } | Stmt::Yield(e) => {
                            self.collect_idents_in_expr(e, idents);
                        }
                        Stmt::Return(Some(e)) => self.collect_idents_in_expr(e, idents),
                        _ => {}
                    }
                }
            }
            Expr::TryCatch {
                body, catch_body, ..
            } => {
                self.collect_idents_in_expr(body, idents);
                self.collect_idents_in_expr(catch_body, idents);
            }
            Expr::Fetch { url, options, .. } => {
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

    fn make_fn(name: &str, stmts: Vec<Stmt>) -> Function {
        Function {
            name: name.to_string(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: Block { stmts, span: dummy_span() },
            is_pub: false,
            must_use: false,
            span: dummy_span(),
        }
    }

    fn make_pub_fn(name: &str, stmts: Vec<Stmt>) -> Function {
        let mut f = make_fn(name, stmts);
        f.is_pub = true;
        f
    }

    fn has_rule(warnings: &[LintWarning], rule: &str) -> bool {
        warnings.iter().any(|w| w.rule == rule)
    }

    fn has_rule_containing(warnings: &[LintWarning], rule: &str, msg_part: &str) -> bool {
        warnings.iter().any(|w| w.rule == rule && w.message.contains(msg_part))
    }

    // ===================================================================
    // Existing tests
    // ===================================================================

    #[test]
    fn test_unused_variable_detected() {
        let program = Program {
            items: vec![Item::Function(make_fn("foo", vec![
                Stmt::Let {
                    name: "x".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(42),
                    ownership: Ownership::Owned,
                },
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "unused-variable", "`x`"),
            "Expected unused-variable warning for `x`, got: {:?}", warnings);
    }

    #[test]
    fn test_snake_case_violation_detected() {
        let program = Program {
            items: vec![Item::Function(make_fn("myFunction", vec![Stmt::Return(None)]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "snake-case-functions", "`myFunction`"),
            "Expected snake-case-functions warning, got: {:?}", warnings);
    }

    #[test]
    fn test_empty_block_detected() {
        let program = Program {
            items: vec![Item::Function(make_fn("empty", vec![]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "empty-block", "`empty`"),
            "Expected empty-block warning, got: {:?}", warnings);
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
        assert!(has_rule_containing(&warnings, "pascal-case-types", "`my_struct`"),
            "Expected pascal-case-types warning, got: {:?}", warnings);
    }

    #[test]
    fn test_unreachable_code_detected() {
        let program = Program {
            items: vec![Item::Function(make_fn("early_return", vec![
                Stmt::Return(Some(Expr::Integer(1))),
                Stmt::Expr(Expr::Integer(2)),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "unreachable-code"),
            "Expected unreachable-code warning, got: {:?}", warnings);
    }

    #[test]
    fn test_mutable_not_mutated() {
        let program = Program {
            items: vec![Item::Function(make_fn("no_mutate", vec![
                Stmt::Let {
                    name: "x".to_string(),
                    ty: None,
                    mutable: true,
                    secret: false,
                    value: Expr::Integer(0),
                    ownership: Ownership::Owned,
                },
                Stmt::Expr(Expr::Ident("x".to_string())),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "mutable-not-mutated", "`x`"),
            "Expected mutable-not-mutated warning, got: {:?}", warnings);
    }

    #[test]
    fn test_single_match_suggestion() {
        let program = Program {
            items: vec![Item::Function(make_fn("check", vec![
                Stmt::Expr(Expr::Match {
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
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "single-match"),
            "Expected single-match suggestion, got: {:?}", warnings);
    }

    // ===================================================================
    // Unused variable - additional cases
    // ===================================================================

    #[test]
    fn test_unused_variable_underscore_prefix_no_warning() {
        let program = Program {
            items: vec![Item::Function(make_fn("foo", vec![
                Stmt::Let {
                    name: "_unused".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(42),
                    ownership: Ownership::Owned,
                },
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-variable", "`_unused`"),
            "Underscore-prefixed variables should not trigger unused-variable, got: {:?}", warnings);
    }

    #[test]
    fn test_used_variable_no_warning() {
        let program = Program {
            items: vec![Item::Function(make_fn("foo", vec![
                Stmt::Let {
                    name: "x".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(42),
                    ownership: Ownership::Owned,
                },
                Stmt::Expr(Expr::Ident("x".to_string())),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-variable", "`x`"),
            "Used variable should not trigger warning, got: {:?}", warnings);
    }

    #[test]
    fn test_unused_variable_in_nested_expr() {
        // Variable used inside a binary expression should count as used
        let program = Program {
            items: vec![Item::Function(make_fn("foo", vec![
                Stmt::Let {
                    name: "a".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(1),
                    ownership: Ownership::Owned,
                },
                Stmt::Expr(Expr::Binary {
                    op: BinOp::Add,
                    left: Box::new(Expr::Ident("a".to_string())),
                    right: Box::new(Expr::Integer(2)),
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-variable", "`a`"),
            "Variable used in binary expr should not be unused, got: {:?}", warnings);
    }

    // ===================================================================
    // Unused function
    // ===================================================================

    #[test]
    fn test_unused_private_function() {
        let program = Program {
            items: vec![
                Item::Function(make_fn("helper", vec![Stmt::Return(None)])),
                Item::Function(make_fn("main_fn", vec![Stmt::Return(None)])),
            ],
        };
        let warnings = lint_program(&program);
        // Both functions are private and neither calls the other
        assert!(has_rule_containing(&warnings, "unused-function", "`helper`"),
            "Expected unused-function for helper, got: {:?}", warnings);
    }

    #[test]
    fn test_used_private_function_no_warning() {
        let program = Program {
            items: vec![
                Item::Function(make_fn("helper", vec![Stmt::Return(None)])),
                Item::Function(make_fn("main_fn", vec![
                    Stmt::Expr(Expr::FnCall {
                        callee: Box::new(Expr::Ident("helper".to_string())),
                        args: vec![],
                    }),
                ])),
            ],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-function", "`helper`"),
            "Called function should not be unused, got: {:?}", warnings);
    }

    #[test]
    fn test_pub_function_not_flagged_unused() {
        let program = Program {
            items: vec![Item::Function(make_pub_fn("public_api", vec![Stmt::Return(None)]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-function", "`public_api`"),
            "Public function should not be flagged as unused, got: {:?}", warnings);
    }

    // ===================================================================
    // Unused import
    // ===================================================================

    #[test]
    fn test_unused_import() {
        let program = Program {
            items: vec![
                Item::Use(UsePath {
                    segments: vec!["std".to_string(), "HashMap".to_string()],
                    alias: None,
                    glob: false,
                    group: None,
                    span: dummy_span(),
                }),
                Item::Function(make_fn("foo", vec![Stmt::Return(None)])),
            ],
        };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "unused-import", "`HashMap`"),
            "Expected unused-import for HashMap, got: {:?}", warnings);
    }

    #[test]
    fn test_used_import_no_warning() {
        let program = Program {
            items: vec![
                Item::Use(UsePath {
                    segments: vec!["std".to_string(), "HashMap".to_string()],
                    alias: None,
                    glob: false,
                    group: None,
                    span: dummy_span(),
                }),
                Item::Function(make_fn("foo", vec![
                    Stmt::Expr(Expr::Ident("HashMap".to_string())),
                ])),
            ],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-import", "`HashMap`"),
            "Used import should not be flagged, got: {:?}", warnings);
    }

    // ===================================================================
    // Snake case - additional cases
    // ===================================================================

    #[test]
    fn test_snake_case_valid_no_warning() {
        let program = Program {
            items: vec![Item::Function(make_fn("valid_name", vec![Stmt::Return(None)]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "snake-case-functions", "`valid_name`"),
            "Valid snake_case should not trigger warning, got: {:?}", warnings);
    }

    #[test]
    fn test_snake_case_method_in_impl() {
        let im = ImplBlock {
            target: "Point".to_string(),
            trait_impls: vec![],
            methods: vec![make_fn("badMethod", vec![Stmt::Return(None)])],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Impl(im)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "snake-case-functions", "`badMethod`"),
            "Expected snake-case warning for impl method, got: {:?}", warnings);
    }

    #[test]
    fn test_snake_case_trait_method() {
        let t = TraitDef {
            name: "MyTrait".to_string(),
            type_params: vec![],
            methods: vec![TraitMethod {
                name: "badName".to_string(),
                params: vec![],
                return_type: None,
                default_body: None,
                span: dummy_span(),
            }],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Trait(t)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "snake-case-functions", "`badName`"),
            "Expected snake-case warning for trait method, got: {:?}", warnings);
    }

    #[test]
    fn test_snake_case_component_method() {
        let c = Component {
            name: "MyComp".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![make_fn("badHandler", vec![Stmt::Return(None)])],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element { tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span() }),
                span: dummy_span(),
            },
            trait_bounds: vec![],
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Component(c)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "snake-case-functions", "`badHandler`"),
            "Expected snake-case warning in component method, got: {:?}", warnings);
    }

    // ===================================================================
    // Pascal case - additional cases
    // ===================================================================

    #[test]
    fn test_pascal_case_valid_no_warning() {
        let s = StructDef {
            name: "ValidName".to_string(),
            lifetimes: vec![],
            type_params: vec![],
            fields: vec![],
            trait_bounds: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Struct(s)] };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "pascal-case-types", "`ValidName`"),
            "Valid PascalCase should not trigger warning, got: {:?}", warnings);
    }

    #[test]
    fn test_pascal_case_enum() {
        let e = EnumDef {
            name: "bad_enum".to_string(),
            type_params: vec![],
            variants: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Enum(e)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "pascal-case-types", "`bad_enum`"),
            "Expected pascal-case warning for enum, got: {:?}", warnings);
    }

    #[test]
    fn test_pascal_case_component() {
        let c = Component {
            name: "bad_component".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element { tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span() }),
                span: dummy_span(),
            },
            trait_bounds: vec![],
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Component(c)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "pascal-case-types", "`bad_component`"),
            "Expected pascal-case warning for component, got: {:?}", warnings);
    }

    #[test]
    fn test_pascal_case_trait() {
        let t = TraitDef {
            name: "bad_trait".to_string(),
            type_params: vec![],
            methods: vec![],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Trait(t)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "pascal-case-types", "`bad_trait`"),
            "Expected pascal-case warning for trait, got: {:?}", warnings);
    }

    #[test]
    fn test_pascal_case_store() {
        let s = StoreDef {
            name: "bad_store".to_string(),
            signals: vec![],
            actions: vec![],
            computed: vec![],
            effects: vec![],
            selectors: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Store(s)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "pascal-case-types", "`bad_store`"),
            "Expected pascal-case warning for store, got: {:?}", warnings);
    }

    // ===================================================================
    // Empty block - additional cases
    // ===================================================================

    #[test]
    fn test_empty_if_block() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::If {
                    condition: Box::new(Expr::Bool(true)),
                    then_block: Block { stmts: vec![], span: dummy_span() },
                    else_block: None,
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "empty-block", "if block"),
            "Expected empty-block warning for if, got: {:?}", warnings);
    }

    #[test]
    fn test_empty_else_block() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::If {
                    condition: Box::new(Expr::Bool(true)),
                    then_block: Block {
                        stmts: vec![Stmt::Expr(Expr::Integer(1))],
                        span: dummy_span(),
                    },
                    else_block: Some(Block { stmts: vec![], span: dummy_span() }),
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "empty-block", "else block"),
            "Expected empty-block warning for else, got: {:?}", warnings);
    }

    #[test]
    fn test_non_empty_function_no_empty_block_warning() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Return(Some(Expr::Integer(42))),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule(&warnings, "empty-block"),
            "Non-empty function should not trigger empty-block, got: {:?}", warnings);
    }

    // ===================================================================
    // Unreachable code - additional cases
    // ===================================================================

    #[test]
    fn test_no_unreachable_when_return_is_last() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Integer(1)),
                Stmt::Return(Some(Expr::Integer(2))),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule(&warnings, "unreachable-code"),
            "Return as last statement should not trigger unreachable, got: {:?}", warnings);
    }

    #[test]
    fn test_unreachable_code_only_reported_once() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Return(Some(Expr::Integer(1))),
                Stmt::Expr(Expr::Integer(2)),
                Stmt::Expr(Expr::Integer(3)),
                Stmt::Expr(Expr::Integer(4)),
            ]))],
        };
        let warnings = lint_program(&program);
        let unreachable_count = warnings.iter().filter(|w| w.rule == "unreachable-code").count();
        assert_eq!(unreachable_count, 1,
            "Expected exactly 1 unreachable-code warning, got {}: {:?}", unreachable_count, warnings);
    }

    // ===================================================================
    // Mutable not mutated - additional cases
    // ===================================================================

    #[test]
    fn test_mutable_actually_mutated_no_warning() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Let {
                    name: "x".to_string(),
                    ty: None,
                    mutable: true,
                    secret: false,
                    value: Expr::Integer(0),
                    ownership: Ownership::Owned,
                },
                Stmt::Expr(Expr::Assign {
                    target: Box::new(Expr::Ident("x".to_string())),
                    value: Box::new(Expr::Integer(1)),
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "mutable-not-mutated", "`x`"),
            "Mutated variable should not trigger warning, got: {:?}", warnings);
    }

    #[test]
    fn test_immutable_no_mutable_not_mutated_warning() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Let {
                    name: "x".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(0),
                    ownership: Ownership::Owned,
                },
                Stmt::Expr(Expr::Ident("x".to_string())),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "mutable-not-mutated", "`x`"),
            "Immutable variable should not trigger mutable-not-mutated, got: {:?}", warnings);
    }

    // ===================================================================
    // Single match - additional cases
    // ===================================================================

    #[test]
    fn test_match_with_three_arms_no_single_match() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Match {
                    subject: Box::new(Expr::Ident("x".to_string())),
                    arms: vec![
                        MatchArm { pattern: Pattern::Literal(Expr::Integer(1)), body: Expr::StringLit("one".to_string()) },
                        MatchArm { pattern: Pattern::Literal(Expr::Integer(2)), body: Expr::StringLit("two".to_string()) },
                        MatchArm { pattern: Pattern::Wildcard, body: Expr::StringLit("other".to_string()) },
                    ],
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule(&warnings, "single-match"),
            "Match with 3 arms should not trigger single-match, got: {:?}", warnings);
    }

    #[test]
    fn test_match_two_arms_no_wildcard_no_single_match() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Match {
                    subject: Box::new(Expr::Ident("x".to_string())),
                    arms: vec![
                        MatchArm {
                            pattern: Pattern::Variant { name: "Some".to_string(), fields: vec![Pattern::Ident("v".to_string())] },
                            body: Expr::Ident("v".to_string()),
                        },
                        MatchArm {
                            pattern: Pattern::Variant { name: "None".to_string(), fields: vec![] },
                            body: Expr::Integer(0),
                        },
                    ],
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule(&warnings, "single-match"),
            "Match with 2 non-wildcard arms should not trigger single-match, got: {:?}", warnings);
    }

    // ===================================================================
    // Redundant clone
    // ===================================================================

    #[test]
    fn test_redundant_clone_detected() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::MethodCall {
                    object: Box::new(Expr::Ident("x".to_string())),
                    method: "clone".to_string(),
                    args: vec![],
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "redundant-clone", "`x.clone()`"),
            "Expected redundant-clone warning, got: {:?}", warnings);
    }

    #[test]
    fn test_clone_with_args_not_flagged() {
        // .clone(something) is not the same as .clone()
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::MethodCall {
                    object: Box::new(Expr::Ident("x".to_string())),
                    method: "clone".to_string(),
                    args: vec![Expr::Integer(1)],
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule(&warnings, "redundant-clone"),
            "clone() with args should not be flagged, got: {:?}", warnings);
    }

    // ===================================================================
    // Lint config - disabling rules
    // ===================================================================

    #[test]
    fn test_config_disable_unused_variable() {
        let program = Program {
            items: vec![Item::Function(make_fn("foo", vec![
                Stmt::Let {
                    name: "x".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(42),
                    ownership: Ownership::Owned,
                },
            ]))],
        };
        let config = LintConfig { unused_variable: false, ..Default::default() };
        let warnings = lint_program_with_config(&program, config);
        assert!(!has_rule(&warnings, "unused-variable"),
            "Disabled rule should not fire, got: {:?}", warnings);
    }

    #[test]
    fn test_config_disable_empty_block() {
        let program = Program {
            items: vec![Item::Function(make_fn("empty", vec![]))],
        };
        let config = LintConfig { empty_block: false, ..Default::default() };
        let warnings = lint_program_with_config(&program, config);
        assert!(!has_rule(&warnings, "empty-block"),
            "Disabled empty-block should not fire, got: {:?}", warnings);
    }

    #[test]
    fn test_config_disable_snake_case() {
        let program = Program {
            items: vec![Item::Function(make_fn("myBadName", vec![Stmt::Return(None)]))],
        };
        let config = LintConfig { snake_case_functions: false, ..Default::default() };
        let warnings = lint_program_with_config(&program, config);
        assert!(!has_rule(&warnings, "snake-case-functions"),
            "Disabled snake-case should not fire, got: {:?}", warnings);
    }

    #[test]
    fn test_config_disable_pascal_case() {
        let s = StructDef {
            name: "bad_name".to_string(),
            lifetimes: vec![],
            type_params: vec![],
            fields: vec![],
            trait_bounds: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Struct(s)] };
        let config = LintConfig { pascal_case_types: false, ..Default::default() };
        let warnings = lint_program_with_config(&program, config);
        assert!(!has_rule(&warnings, "pascal-case-types"),
            "Disabled pascal-case should not fire, got: {:?}", warnings);
    }

    #[test]
    fn test_config_disable_unreachable() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Return(Some(Expr::Integer(1))),
                Stmt::Expr(Expr::Integer(2)),
            ]))],
        };
        let config = LintConfig { unreachable_code: false, ..Default::default() };
        let warnings = lint_program_with_config(&program, config);
        assert!(!has_rule(&warnings, "unreachable-code"),
            "Disabled unreachable should not fire, got: {:?}", warnings);
    }

    #[test]
    fn test_config_disable_single_match() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Match {
                    subject: Box::new(Expr::Ident("x".to_string())),
                    arms: vec![
                        MatchArm {
                            pattern: Pattern::Variant { name: "Some".to_string(), fields: vec![Pattern::Ident("v".to_string())] },
                            body: Expr::Ident("v".to_string()),
                        },
                        MatchArm { pattern: Pattern::Wildcard, body: Expr::Integer(0) },
                    ],
                }),
            ]))],
        };
        let config = LintConfig { single_match: false, ..Default::default() };
        let warnings = lint_program_with_config(&program, config);
        assert!(!has_rule(&warnings, "single-match"),
            "Disabled single-match should not fire, got: {:?}", warnings);
    }

    #[test]
    fn test_config_disable_redundant_clone() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::MethodCall {
                    object: Box::new(Expr::Ident("x".to_string())),
                    method: "clone".to_string(),
                    args: vec![],
                }),
            ]))],
        };
        let config = LintConfig { redundant_clone: false, ..Default::default() };
        let warnings = lint_program_with_config(&program, config);
        assert!(!has_rule(&warnings, "redundant-clone"),
            "Disabled redundant-clone should not fire, got: {:?}", warnings);
    }

    // ===================================================================
    // Severity levels
    // ===================================================================

    #[test]
    fn test_severity_display() {
        assert_eq!(format!("{}", Severity::Info), "info");
        assert_eq!(format!("{}", Severity::Warning), "warning");
        assert_eq!(format!("{}", Severity::Error), "error");
    }

    #[test]
    fn test_single_match_is_info_severity() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Match {
                    subject: Box::new(Expr::Ident("x".to_string())),
                    arms: vec![
                        MatchArm {
                            pattern: Pattern::Variant { name: "Some".to_string(), fields: vec![Pattern::Ident("v".to_string())] },
                            body: Expr::Ident("v".to_string()),
                        },
                        MatchArm { pattern: Pattern::Wildcard, body: Expr::Integer(0) },
                    ],
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        let single_match = warnings.iter().find(|w| w.rule == "single-match");
        assert!(single_match.is_some());
        assert_eq!(single_match.unwrap().severity, Severity::Info);
    }

    #[test]
    fn test_unused_variable_is_warning_severity() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Let {
                    name: "x".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(1),
                    ownership: Ownership::Owned,
                },
            ]))],
        };
        let warnings = lint_program(&program);
        let w = warnings.iter().find(|w| w.rule == "unused-variable");
        assert!(w.is_some());
        assert_eq!(w.unwrap().severity, Severity::Warning);
    }

    #[test]
    fn test_redundant_clone_is_info_severity() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::MethodCall {
                    object: Box::new(Expr::Ident("x".to_string())),
                    method: "clone".to_string(),
                    args: vec![],
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        let rc = warnings.iter().find(|w| w.rule == "redundant-clone");
        assert!(rc.is_some());
        assert_eq!(rc.unwrap().severity, Severity::Info);
    }

    // ===================================================================
    // Naming convention helper functions
    // ===================================================================

    #[test]
    fn test_is_snake_case() {
        assert!(is_snake_case("hello_world"));
        assert!(is_snake_case("foo"));
        assert!(is_snake_case("x"));
        assert!(is_snake_case("_private"));
        assert!(is_snake_case(""));
        assert!(!is_snake_case("HelloWorld"));
        assert!(!is_snake_case("camelCase"));
        assert!(!is_snake_case("UPPER"));
    }

    #[test]
    fn test_is_pascal_case() {
        assert!(is_pascal_case("HelloWorld"));
        assert!(is_pascal_case("Foo"));
        assert!(is_pascal_case("X"));
        assert!(is_pascal_case(""));
        assert!(!is_pascal_case("hello_world"));
        assert!(!is_pascal_case("camelCase"));
        assert!(!is_pascal_case("Bad_Name"));
    }

    // ===================================================================
    // Store linting
    // ===================================================================

    #[test]
    fn test_store_pascal_case_check() {
        let s = StoreDef {
            name: "AppStore".to_string(),
            signals: vec![],
            actions: vec![ActionDef {
                name: "increment".to_string(),
                params: vec![],
                body: Block { stmts: vec![Stmt::Return(None)], span: dummy_span() },
                is_async: false,
                span: dummy_span(),
            }],
            computed: vec![ComputedDef {
                name: "double".to_string(),
                return_type: None,
                body: Block { stmts: vec![Stmt::Return(None)], span: dummy_span() },
                span: dummy_span(),
            }],
            effects: vec![EffectDef {
                name: "logger".to_string(),
                body: Block { stmts: vec![Stmt::Return(None)], span: dummy_span() },
                span: dummy_span(),
            }],
            selectors: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Store(s)] };
        let warnings = lint_program(&program);
        // AppStore is valid PascalCase, should not trigger
        assert!(!has_rule_containing(&warnings, "pascal-case-types", "`AppStore`"),
            "Valid PascalCase store should not trigger, got: {:?}", warnings);
    }

    #[test]
    fn test_store_lint_action_block() {
        // Empty action body doesn't trigger "empty-block" (only functions do for empty-block),
        // but unreachable code in action should be caught
        let s = StoreDef {
            name: "TestStore".to_string(),
            signals: vec![],
            actions: vec![ActionDef {
                name: "act".to_string(),
                params: vec![],
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(1))),
                        Stmt::Expr(Expr::Integer(2)), // unreachable
                    ],
                    span: dummy_span(),
                },
                is_async: false,
                span: dummy_span(),
            }],
            computed: vec![],
            effects: vec![],
            selectors: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Store(s)] };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "unreachable-code"),
            "Expected unreachable-code in store action, got: {:?}", warnings);
    }

    // ===================================================================
    // Impl linting
    // ===================================================================

    #[test]
    fn test_impl_method_empty_block() {
        let im = ImplBlock {
            target: "Foo".to_string(),
            trait_impls: vec![],
            methods: vec![make_fn("do_nothing", vec![])],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Impl(im)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "empty-block", "`do_nothing`"),
            "Expected empty-block for impl method, got: {:?}", warnings);
    }

    // ===================================================================
    // Nested expression linting
    // ===================================================================

    #[test]
    fn test_lint_nested_for_loop() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::For {
                    binding: "i".to_string(),
                    iterator: Box::new(Expr::Ident("items".to_string())),
                    body: Block {
                        stmts: vec![
                            Stmt::Return(Some(Expr::Integer(1))),
                            Stmt::Expr(Expr::Integer(2)), // unreachable
                        ],
                        span: dummy_span(),
                    },
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "unreachable-code"),
            "Expected unreachable-code in for body, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_nested_while_loop() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::While {
                    condition: Box::new(Expr::Bool(true)),
                    body: Block {
                        stmts: vec![
                            Stmt::Let {
                                name: "unused_in_while".to_string(),
                                ty: None,
                                mutable: false,
                                secret: false,
                                value: Expr::Integer(0),
                                ownership: Ownership::Owned,
                            },
                        ],
                        span: dummy_span(),
                    },
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "unused-variable", "`unused_in_while`"),
            "Expected unused-variable in while body, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_nested_block_expr() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Block(Block {
                    stmts: vec![
                        Stmt::Let {
                            name: "nested_unused".to_string(),
                            ty: None,
                            mutable: false,
                            secret: false,
                            value: Expr::Integer(99),
                            ownership: Ownership::Owned,
                        },
                    ],
                    span: dummy_span(),
                })),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "unused-variable", "`nested_unused`"),
            "Expected unused-variable in nested block, got: {:?}", warnings);
    }

    // ===================================================================
    // Expression-level recursion in lint_expr
    // ===================================================================

    #[test]
    fn test_lint_binary_expr_recurse() {
        // Single-match inside a binary expression body
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Let {
                    name: "r".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Match {
                        subject: Box::new(Expr::Ident("x".to_string())),
                        arms: vec![
                            MatchArm {
                                pattern: Pattern::Variant { name: "Some".to_string(), fields: vec![Pattern::Ident("v".to_string())] },
                                body: Expr::Ident("v".to_string()),
                            },
                            MatchArm { pattern: Pattern::Wildcard, body: Expr::Integer(0) },
                        ],
                    },
                    ownership: Ownership::Owned,
                },
                Stmt::Expr(Expr::Ident("r".to_string())),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "single-match"),
            "Expected single-match in let value, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_closure_recurse() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Closure {
                    params: vec![("x".to_string(), None)],
                    body: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::Ident("x".to_string())),
                        method: "clone".to_string(),
                        args: vec![],
                    }),
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "redundant-clone"),
            "Expected redundant-clone inside closure, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_fn_call_recurse() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("foo".to_string())),
                    args: vec![Expr::MethodCall {
                        object: Box::new(Expr::Ident("y".to_string())),
                        method: "clone".to_string(),
                        args: vec![],
                    }],
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "redundant-clone"),
            "Expected redundant-clone inside fn call arg, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_index_recurse() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Index {
                    object: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::Ident("v".to_string())),
                        method: "clone".to_string(),
                        args: vec![],
                    }),
                    index: Box::new(Expr::Integer(0)),
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "redundant-clone"),
            "Expected redundant-clone inside index object, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_assign_recurse() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Assign {
                    target: Box::new(Expr::Ident("x".to_string())),
                    value: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::Ident("y".to_string())),
                        method: "clone".to_string(),
                        args: vec![],
                    }),
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "redundant-clone"),
            "Expected redundant-clone inside assignment, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_unary_recurse() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::Ident("z".to_string())),
                        method: "clone".to_string(),
                        args: vec![],
                    }),
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "redundant-clone"),
            "Expected redundant-clone inside unary, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_field_access_recurse() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::FieldAccess {
                    object: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::Ident("a".to_string())),
                        method: "clone".to_string(),
                        args: vec![],
                    }),
                    field: "x".to_string(),
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "redundant-clone"),
            "Expected redundant-clone inside field access, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_struct_init_recurse() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::StructInit {
                    name: "Foo".to_string(),
                    fields: vec![
                        ("x".to_string(), Expr::MethodCall {
                            object: Box::new(Expr::Ident("v".to_string())),
                            method: "clone".to_string(),
                            args: vec![],
                        }),
                    ],
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "redundant-clone"),
            "Expected redundant-clone inside struct init, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_try_catch_recurse() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::TryCatch {
                    body: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::Ident("a".to_string())),
                        method: "clone".to_string(),
                        args: vec![],
                    }),
                    error_binding: "e".to_string(),
                    catch_body: Box::new(Expr::Integer(0)),
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "redundant-clone"),
            "Expected redundant-clone in try body, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_fetch_recurse() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Fetch {
                    url: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::Ident("url".to_string())),
                        method: "clone".to_string(),
                        args: vec![],
                    }),
                    options: Some(Box::new(Expr::Integer(1))),
                    contract: None,
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "redundant-clone"),
            "Expected redundant-clone in fetch url, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_await_recurse() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Await(Box::new(Expr::MethodCall {
                    object: Box::new(Expr::Ident("f".to_string())),
                    method: "clone".to_string(),
                    args: vec![],
                }))),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "redundant-clone"),
            "Expected redundant-clone inside await, got: {:?}", warnings);
    }

    #[test]
    fn test_lint_spawn_recurse() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Expr(Expr::Spawn {
                    body: Block {
                        stmts: vec![
                            Stmt::Let {
                                name: "spawn_unused".to_string(),
                                ty: None,
                                mutable: false,
                                secret: false,
                                value: Expr::Integer(1),
                                ownership: Ownership::Owned,
                            },
                        ],
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                }),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "unused-variable", "`spawn_unused`"),
            "Expected unused-variable in spawn block, got: {:?}", warnings);
    }

    // ===================================================================
    // Agent, Test, LazyComponent linting
    // ===================================================================

    #[test]
    fn test_agent_tool_lint() {
        let agent = AgentDef {
            name: "Bot".to_string(),
            system_prompt: None,
            tools: vec![ToolDef {
                name: "search".to_string(),
                description: None,
                params: vec![],
                return_type: None,
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(1))),
                        Stmt::Expr(Expr::Integer(2)),
                    ],
                    span: dummy_span(),
                },
                span: dummy_span(),
            }],
            state: vec![],
            methods: vec![],
            render: None,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Agent(agent)] };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "unreachable-code"),
            "Expected unreachable-code in agent tool, got: {:?}", warnings);
    }

    #[test]
    fn test_agent_method_empty_block_lint() {
        let agent = AgentDef {
            name: "Bot".to_string(),
            system_prompt: None,
            tools: vec![],
            state: vec![],
            methods: vec![make_fn("do_nothing", vec![])],
            render: None,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Agent(agent)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "empty-block", "`do_nothing`"),
            "Expected empty-block warning for agent method, got: {:?}", warnings);
    }

    #[test]
    fn test_test_block_lint() {
        let test = TestDef {
            name: "my test".to_string(),
            body: Block {
                stmts: vec![
                    Stmt::Let {
                        name: "unused_test_var".to_string(),
                        ty: None,
                        mutable: false,
                        secret: false,
                        value: Expr::Integer(1),
                        ownership: Ownership::Owned,
                    },
                ],
                span: dummy_span(),
            },
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Test(test)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "unused-variable", "`unused_test_var`"),
            "Expected unused-variable in test, got: {:?}", warnings);
    }

    #[test]
    fn test_lazy_component_pascal_case_check() {
        let lc = LazyComponentDef {
            component: Component {
                name: "bad_lazy".to_string(),
                type_params: vec![],
                props: vec![],
                state: vec![],
                methods: vec![],
                styles: vec![],
                transitions: vec![],
                render: RenderBlock {
                    body: TemplateNode::Element(Element { tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span() }),
                    span: dummy_span(),
                },
                trait_bounds: vec![],
                permissions: None,
                gestures: vec![],
                skeleton: None,
                error_boundary: None,
                chunk: None,
                on_destroy: None,
                a11y: None,
                shortcuts: vec![],
                span: dummy_span(),
            },
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::LazyComponent(lc)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "pascal-case-types", "`bad_lazy`"),
            "Expected pascal-case warning for lazy component, got: {:?}", warnings);
    }

    // ===================================================================
    // Page linting
    // ===================================================================

    #[test]
    fn test_page_pascal_case() {
        let page = PageDef {
            name: "bad_page".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![TemplateNode::Element(Element {
                        tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                    })],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "pascal-case-types", "`bad_page`"),
            "Expected pascal-case warning for page, got: {:?}", warnings);
    }

    #[test]
    fn test_page_method_snake_case() {
        let page = PageDef {
            name: "TestPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![make_fn("badHandler", vec![Stmt::Return(None)])],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![TemplateNode::Element(Element {
                        tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                    })],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "snake-case-functions", "`badHandler`"),
            "Expected snake-case warning for page method, got: {:?}", warnings);
    }

    // ===================================================================
    // Semantic HTML linting
    // ===================================================================

    #[test]
    fn test_page_missing_h1() {
        let page = PageDef {
            name: "NoH1Page".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![TemplateNode::Element(Element {
                        tag: "p".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                    })],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "semantic_html", "h1"),
            "Expected semantic_html warning for missing h1, got: {:?}", warnings);
    }

    #[test]
    fn test_page_with_h1_no_warning() {
        let page = PageDef {
            name: "GoodPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![TemplateNode::Element(Element {
                        tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                    })],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "semantic_html", "h1"),
            "Page with h1 should not trigger warning, got: {:?}", warnings);
    }

    #[test]
    fn test_page_div_wrapper_warning() {
        let page = PageDef {
            name: "DivPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(),
                    attributes: vec![],
                    children: vec![TemplateNode::Element(Element {
                        tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                    })],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "semantic_html", "div"),
            "Expected semantic_html warning for div wrapper, got: {:?}", warnings);
    }

    #[test]
    fn test_img_without_alt() {
        let page = PageDef {
            name: "ImgPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![
                        TemplateNode::Element(Element {
                            tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                        }),
                        TemplateNode::Element(Element {
                            tag: "img".to_string(),
                            attributes: vec![
                                Attribute::Static { name: "src".to_string(), value: "pic.png".to_string() },
                            ],
                            children: vec![],
                            span: dummy_span(),
                        }),
                    ],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "semantic_html", "alt"),
            "Expected semantic_html warning for img without alt, got: {:?}", warnings);
    }

    #[test]
    fn test_img_with_alt_no_warning() {
        let page = PageDef {
            name: "GoodImgPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![
                        TemplateNode::Element(Element {
                            tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                        }),
                        TemplateNode::Element(Element {
                            tag: "img".to_string(),
                            attributes: vec![
                                Attribute::Static { name: "alt".to_string(), value: "description".to_string() },
                            ],
                            children: vec![],
                            span: dummy_span(),
                        }),
                    ],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "semantic_html", "alt"),
            "img with alt should not trigger warning, got: {:?}", warnings);
    }

    // ===================================================================
    // Accessibility linting
    // ===================================================================

    #[test]
    fn test_input_without_label_warns() {
        let page = PageDef {
            name: "FormPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![
                        TemplateNode::Element(Element {
                            tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                        }),
                        TemplateNode::Element(Element {
                            tag: "input".to_string(),
                            attributes: vec![Attribute::Static { name: "type".into(), value: "text".into() }],
                            children: vec![],
                            span: dummy_span(),
                        }),
                    ],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "a11y_label", "input"),
            "Expected a11y_label warning for input without label, got: {:?}", warnings);
    }

    #[test]
    fn test_input_with_aria_label_no_warning() {
        let page = PageDef {
            name: "LabeledPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![
                        TemplateNode::Element(Element {
                            tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                        }),
                        TemplateNode::Element(Element {
                            tag: "input".to_string(),
                            attributes: vec![
                                Attribute::Aria { name: "aria-label".into(), value: Expr::StringLit("Search".into()) },
                            ],
                            children: vec![],
                            span: dummy_span(),
                        }),
                    ],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "a11y_label", "input"),
            "Input with aria-label should not warn, got: {:?}", warnings);
    }

    #[test]
    fn test_button_without_text_warns() {
        let page = PageDef {
            name: "BtnPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![
                        TemplateNode::Element(Element {
                            tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                        }),
                        TemplateNode::Element(Element {
                            tag: "button".to_string(),
                            attributes: vec![],
                            children: vec![],
                            span: dummy_span(),
                        }),
                    ],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "a11y_label", "button"),
            "Expected a11y_label warning for empty button, got: {:?}", warnings);
    }

    #[test]
    fn test_button_with_text_no_warning() {
        let page = PageDef {
            name: "GoodBtnPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![
                        TemplateNode::Element(Element {
                            tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                        }),
                        TemplateNode::Element(Element {
                            tag: "button".to_string(),
                            attributes: vec![],
                            children: vec![TemplateNode::TextLiteral("Click me".into())],
                            span: dummy_span(),
                        }),
                    ],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "a11y_label", "button"),
            "Button with text should not warn, got: {:?}", warnings);
    }

    #[test]
    fn test_clickable_div_without_role_warns() {
        let page = PageDef {
            name: "ClickPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![
                        TemplateNode::Element(Element {
                            tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                        }),
                        TemplateNode::Element(Element {
                            tag: "div".to_string(),
                            attributes: vec![
                                Attribute::EventHandler { event: "click".into(), handler: Expr::Ident("handle".into()) },
                            ],
                            children: vec![],
                            span: dummy_span(),
                        }),
                    ],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "a11y_interactive", "role"),
            "Expected a11y_interactive warning for clickable div without role, got: {:?}", warnings);
        assert!(has_rule_containing(&warnings, "a11y_interactive", "tabindex"),
            "Expected a11y_interactive warning for clickable div without tabindex, got: {:?}", warnings);
    }

    #[test]
    fn test_clickable_div_with_role_and_tabindex_no_warning() {
        let page = PageDef {
            name: "GoodClickPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![
                        TemplateNode::Element(Element {
                            tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                        }),
                        TemplateNode::Element(Element {
                            tag: "div".to_string(),
                            attributes: vec![
                                Attribute::EventHandler { event: "click".into(), handler: Expr::Ident("handle".into()) },
                                Attribute::Role { value: "button".into() },
                                Attribute::Static { name: "tabindex".into(), value: "0".into() },
                            ],
                            children: vec![],
                            span: dummy_span(),
                        }),
                    ],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "a11y_interactive", "role"),
            "Div with role should not warn about missing role, got: {:?}", warnings);
        assert!(!has_rule_containing(&warnings, "a11y_interactive", "tabindex"),
            "Div with tabindex should not warn about missing tabindex, got: {:?}", warnings);
    }

    #[test]
    fn test_aria_hidden_on_button_warns() {
        let page = PageDef {
            name: "HiddenBtnPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![
                        TemplateNode::Element(Element {
                            tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                        }),
                        TemplateNode::Element(Element {
                            tag: "button".to_string(),
                            attributes: vec![
                                Attribute::Aria { name: "aria-hidden".into(), value: Expr::StringLit("true".into()) },
                            ],
                            children: vec![TemplateNode::TextLiteral("Click".into())],
                            span: dummy_span(),
                        }),
                    ],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "a11y_hidden", "aria-hidden"),
            "Expected a11y_hidden warning for aria-hidden on button, got: {:?}", warnings);
    }

    #[test]
    fn test_aria_hidden_on_div_no_warning() {
        let page = PageDef {
            name: "HiddenDivPage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "main".to_string(),
                    attributes: vec![],
                    children: vec![
                        TemplateNode::Element(Element {
                            tag: "h1".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                        }),
                        TemplateNode::Element(Element {
                            tag: "div".to_string(),
                            attributes: vec![
                                Attribute::Aria { name: "aria-hidden".into(), value: Expr::StringLit("true".into()) },
                            ],
                            children: vec![],
                            span: dummy_span(),
                        }),
                    ],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "a11y_hidden", "aria-hidden"),
            "aria-hidden on non-focusable div should not warn, got: {:?}", warnings);
    }

    // ===================================================================
    // Form linting
    // ===================================================================

    #[test]
    fn test_form_pascal_case() {
        let form = FormDef {
            name: "bad_form".to_string(),
            fields: vec![],
            on_submit: None,
            steps: vec![],
            methods: vec![],
            styles: vec![],
            render: None,
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Form(form)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "pascal-case-types", "`bad_form`"),
            "Expected pascal-case warning for form, got: {:?}", warnings);
    }

    #[test]
    fn test_form_method_snake_case() {
        let form = FormDef {
            name: "LoginForm".to_string(),
            fields: vec![],
            on_submit: None,
            steps: vec![],
            methods: vec![make_fn("handleSubmit", vec![Stmt::Return(None)])],
            styles: vec![],
            render: None,
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Form(form)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "snake-case-functions", "`handleSubmit`"),
            "Expected snake-case warning for form method, got: {:?}", warnings);
    }

    // ===================================================================
    // Resource leak detection
    // ===================================================================

    #[test]
    fn test_resource_leak_no_on_destroy() {
        let c = Component {
            name: "Leaky".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![Function {
                name: "setup".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Expr(Expr::MethodCall {
                        object: Box::new(Expr::Ident("window".to_string())),
                        method: "addEventListener".to_string(),
                        args: vec![Expr::StringLit("click".to_string()), Expr::Ident("handler".to_string())],
                    })],
                    span: dummy_span(),
                },
                is_pub: false,
                must_use: false,
                span: dummy_span(),
            }],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element { tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span() }),
                span: dummy_span(),
            },
            trait_bounds: vec![],
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Component(c)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "resource_leak", "addEventListener"),
            "Expected resource_leak warning, got: {:?}", warnings);
    }

    #[test]
    fn test_resource_leak_with_proper_cleanup() {
        let c = Component {
            name: "Clean".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![
                Function {
                    name: "setup".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block {
                        stmts: vec![Stmt::Expr(Expr::MethodCall {
                            object: Box::new(Expr::Ident("window".to_string())),
                            method: "addEventListener".to_string(),
                            args: vec![Expr::StringLit("click".to_string())],
                        })],
                        span: dummy_span(),
                    },
                    is_pub: false,
                    must_use: false,
                    span: dummy_span(),
                },
                Function {
                    name: "on_destroy".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block {
                        stmts: vec![Stmt::Expr(Expr::MethodCall {
                            object: Box::new(Expr::Ident("window".to_string())),
                            method: "removeEventListener".to_string(),
                            args: vec![Expr::StringLit("click".to_string())],
                        })],
                        span: dummy_span(),
                    },
                    is_pub: false,
                    must_use: false,
                    span: dummy_span(),
                },
            ],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element { tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span() }),
                span: dummy_span(),
            },
            trait_bounds: vec![],
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Component(c)] };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "resource_leak", "addEventListener"),
            "Properly cleaned up resource should not trigger warning, got: {:?}", warnings);
    }

    #[test]
    fn test_resource_leak_missing_matching_cleanup() {
        let c = Component {
            name: "BadClean".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![
                Function {
                    name: "setup".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block {
                        stmts: vec![Stmt::Expr(Expr::MethodCall {
                            object: Box::new(Expr::Ident("x".to_string())),
                            method: "setInterval".to_string(),
                            args: vec![],
                        })],
                        span: dummy_span(),
                    },
                    is_pub: false,
                    must_use: false,
                    span: dummy_span(),
                },
                Function {
                    name: "on_destroy".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block {
                        stmts: vec![Stmt::Expr(Expr::Integer(1))], // does NOT call clearInterval
                        span: dummy_span(),
                    },
                    is_pub: false,
                    must_use: false,
                    span: dummy_span(),
                },
            ],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element { tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span() }),
                span: dummy_span(),
            },
            trait_bounds: vec![],
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Component(c)] };
        let warnings = lint_program(&program);
        assert!(has_rule_containing(&warnings, "resource_leak", "setInterval"),
            "Expected resource_leak for missing clearInterval, got: {:?}", warnings);
    }

    #[test]
    fn test_no_resource_leak_when_no_acquisitions() {
        let c = Component {
            name: "Plain".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![make_fn("do_stuff", vec![Stmt::Return(None)])],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element { tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span() }),
                span: dummy_span(),
            },
            trait_bounds: vec![],
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Component(c)] };
        let warnings = lint_program(&program);
        assert!(!has_rule(&warnings, "resource_leak"),
            "No acquisitions should not trigger resource_leak, got: {:?}", warnings);
    }

    // ===================================================================
    // Call / reference collection from various item types
    // ===================================================================

    #[test]
    fn test_function_call_in_component_counts_as_used() {
        let c = Component {
            name: "Comp".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![Function {
                name: "render_data".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Expr(Expr::FnCall {
                        callee: Box::new(Expr::Ident("helper".to_string())),
                        args: vec![],
                    })],
                    span: dummy_span(),
                },
                is_pub: false,
                must_use: false,
                span: dummy_span(),
            }],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element { tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span() }),
                span: dummy_span(),
            },
            trait_bounds: vec![],
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: dummy_span(),
        };
        let program = Program {
            items: vec![
                Item::Function(make_fn("helper", vec![Stmt::Return(None)])),
                Item::Component(c),
            ],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-function", "`helper`"),
            "Function called from component should not be unused, got: {:?}", warnings);
    }

    #[test]
    fn test_function_call_in_impl_counts_as_used() {
        let im = ImplBlock {
            target: "Foo".to_string(),
            trait_impls: vec![],
            methods: vec![Function {
                name: "method".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Expr(Expr::FnCall {
                        callee: Box::new(Expr::Ident("helper".to_string())),
                        args: vec![],
                    })],
                    span: dummy_span(),
                },
                is_pub: false,
                must_use: false,
                span: dummy_span(),
            }],
            span: dummy_span(),
        };
        let program = Program {
            items: vec![
                Item::Function(make_fn("helper", vec![Stmt::Return(None)])),
                Item::Impl(im),
            ],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-function", "`helper`"),
            "Function called from impl should not be unused, got: {:?}", warnings);
    }

    #[test]
    fn test_function_call_in_store_counts_as_used() {
        let store = StoreDef {
            name: "MyStore".to_string(),
            signals: vec![],
            actions: vec![ActionDef {
                name: "act".to_string(),
                params: vec![],
                body: Block {
                    stmts: vec![Stmt::Expr(Expr::FnCall {
                        callee: Box::new(Expr::Ident("helper".to_string())),
                        args: vec![],
                    })],
                    span: dummy_span(),
                },
                is_async: false,
                span: dummy_span(),
            }],
            computed: vec![],
            effects: vec![],
            selectors: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program {
            items: vec![
                Item::Function(make_fn("helper", vec![Stmt::Return(None)])),
                Item::Store(store),
            ],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-function", "`helper`"),
            "Function called from store should not be unused, got: {:?}", warnings);
    }

    // ===================================================================
    // Yield / LetDestructure in lint_block
    // ===================================================================

    #[test]
    fn test_yield_collects_idents() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Let {
                    name: "x".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(1),
                    ownership: Ownership::Owned,
                },
                Stmt::Yield(Expr::Ident("x".to_string())),
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-variable", "`x`"),
            "Variable used in yield should not be unused, got: {:?}", warnings);
    }

    #[test]
    fn test_let_destructure_collects_idents() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Let {
                    name: "data".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(1),
                    ownership: Ownership::Owned,
                },
                Stmt::LetDestructure {
                    pattern: Pattern::Tuple(vec![Pattern::Ident("a".to_string()), Pattern::Ident("b".to_string())]),
                    ty: None,
                    value: Expr::Ident("data".to_string()),
                },
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-variable", "`data`"),
            "Variable used in destructure should not be unused, got: {:?}", warnings);
    }

    // ===================================================================
    // Default lint config
    // ===================================================================

    #[test]
    fn test_default_config_all_enabled() {
        let config = LintConfig::default();
        assert!(config.unused_variable);
        assert!(config.unused_function);
        assert!(config.unused_import);
        assert!(config.mutable_not_mutated);
        assert!(config.empty_block);
        assert!(config.snake_case_functions);
        assert!(config.pascal_case_types);
        assert!(config.unreachable_code);
        assert!(config.single_match);
        assert!(config.redundant_clone);
        assert!(config.resource_leak);
    }

    // ===================================================================
    // Signal statement in lint_block
    // ===================================================================

    #[test]
    fn test_signal_value_idents_collected() {
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![
                Stmt::Let {
                    name: "initial".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(0),
                    ownership: Ownership::Owned,
                },
                Stmt::Signal {
                    name: "sig".to_string(),
                    ty: None,
                    secret: false,
                    atomic: false,
                    value: Expr::Ident("initial".to_string()),
                },
            ]))],
        };
        let warnings = lint_program(&program);
        assert!(!has_rule_containing(&warnings, "unused-variable", "`initial`"),
            "Variable used as signal initial value should not be unused, got: {:?}", warnings);
    }

    // ===================================================================
    // Multiple warnings from single program
    // ===================================================================

    #[test]
    fn test_multiple_warnings() {
        let program = Program {
            items: vec![
                Item::Function(make_fn("myBadFn", vec![
                    Stmt::Let {
                        name: "unused_var".to_string(),
                        ty: None,
                        mutable: true,
                        secret: false,
                        value: Expr::Integer(0),
                        ownership: Ownership::Owned,
                    },
                ])),
                Item::Struct(StructDef {
                    name: "bad_struct".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    fields: vec![],
                    trait_bounds: vec![],
                    is_pub: false,
                    span: dummy_span(),
                }),
            ],
        };
        let warnings = lint_program(&program);
        assert!(has_rule(&warnings, "snake-case-functions"), "missing snake-case, got: {:?}", warnings);
        assert!(has_rule(&warnings, "unused-variable"), "missing unused-variable, got: {:?}", warnings);
        assert!(has_rule(&warnings, "mutable-not-mutated"), "missing mutable-not-mutated, got: {:?}", warnings);
        assert!(has_rule(&warnings, "pascal-case-types"), "missing pascal-case-types, got: {:?}", warnings);
        // Note: empty-block only fires when body.stmts is empty; this function has one stmt
    }
}
