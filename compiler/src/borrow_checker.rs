use std::collections::HashMap;
use std::fmt;

use crate::ast::*;
use crate::token::Span;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BorrowError {
    pub kind: BorrowErrorKind,
    pub span: Span,
    pub message: String,
    /// The source line where the error occurred (for rustc-style diagnostics).
    pub source_line: Option<String>,
    /// A suggested fix (e.g. "did you mean `&x`?").
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BorrowErrorKind {
    UseAfterMove,
    DoubleMutBorrow,
    MutBorrowWhileImmBorrowed,
    ImmBorrowWhileMutBorrowed,
    #[allow(dead_code)]
    BorrowOutlivesScope,
    AssignWhileBorrowed,
    #[allow(dead_code)]
    LifetimeViolation,
    MissingLifetimeAnnotation,
}

impl fmt::Display for BorrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "error[borrow]: {}", self.message)?;
        writeln!(f, " --> {}:{}", self.span.line, self.span.col)?;
        if let Some(ref line) = self.source_line {
            writeln!(f, "  |")?;
            writeln!(f, "{} | {}", self.span.line, line)?;
            writeln!(f, "  | {}^", " ".repeat(self.span.col.saturating_sub(1) as usize))?;
        }
        if let Some(ref suggestion) = self.suggestion {
            writeln!(f, "  = help: {}", suggestion)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Ownership / borrow state tracked per variable
// ---------------------------------------------------------------------------

/// The current ownership state of a single variable binding.
#[derive(Debug, Clone, PartialEq)]
enum VarState {
    /// The variable owns its value and it has not been moved or borrowed.
    Owned,
    /// The value has been moved to another binding. `moved_to` is a
    /// human-readable description used in diagnostics.
    Moved { moved_to: String },
    /// The variable is currently borrowed immutably `count` times.
    Borrowed { count: usize },
    /// The variable is currently mutably borrowed.
    MutBorrowed,
}

/// Metadata about a live borrow that must be invalidated when the borrowing
/// variable goes out of scope.
#[derive(Debug, Clone)]
struct BorrowInfo {
    /// The variable that was borrowed.
    source_var: String,
    /// Whether the borrow is mutable.
    mutable: bool,
    /// The scope depth at which the borrow was created.
    #[allow(dead_code)]
    scope_depth: usize,
    /// Optional named lifetime for this borrow (e.g., `'a`).
    #[allow(dead_code)]
    lifetime: Option<String>,
    /// Statement index of last use of this borrow variable (NLL).
    /// When set, the borrow is released after this statement rather than at scope exit.
    #[allow(dead_code)]
    last_use_stmt_idx: Option<usize>,
}

// ---------------------------------------------------------------------------
// Scope / environment
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Scope {
    /// Variables introduced in this scope.
    bindings: Vec<String>,
    /// For each variable introduced as a borrow in this scope, record the
    /// borrow so it can be released when the scope exits.
    borrows: Vec<(String, BorrowInfo)>,
    /// Optional lifetime label for this scope (e.g., `'a`).
    lifetime: Option<String>,
}

struct Env {
    /// Variable -> current state.
    vars: HashMap<String, VarState>,
    /// Stack of scopes (outermost first).
    scopes: Vec<Scope>,
    /// Lookup table: borrowing variable name -> borrow metadata.
    borrow_map: HashMap<String, BorrowInfo>,
}

impl Env {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
            scopes: vec![Scope {
                bindings: Vec::new(),
                borrows: Vec::new(),
                lifetime: None,
            }],
            borrow_map: HashMap::new(),
        }
    }

    fn depth(&self) -> usize {
        self.scopes.len()
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope {
            bindings: Vec::new(),
            borrows: Vec::new(),
            lifetime: None,
        });
    }

    fn push_scope_with_lifetime(&mut self, lifetime: String) {
        self.scopes.push(Scope {
            bindings: Vec::new(),
            borrows: Vec::new(),
            lifetime: Some(lifetime),
        });
    }

    /// Find the scope depth that owns the given named lifetime.
    /// Returns None if the lifetime is not found (or is `'static`).
    fn lifetime_scope_depth(&self, name: &str) -> Option<usize> {
        if name == "static" {
            // 'static lives for the entire program — depth 0
            return Some(0);
        }
        for (i, scope) in self.scopes.iter().enumerate() {
            if scope.lifetime.as_deref() == Some(name) {
                return Some(i);
            }
        }
        None
    }

    /// Pop the current scope, releasing all borrows created in it and removing
    /// its bindings from the variable map.
    fn pop_scope(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            // Release borrows that were created in this scope.
            for (borrow_var, info) in &scope.borrows {
                self.release_borrow_on_source(&info.source_var, info.mutable);
                self.borrow_map.remove(borrow_var);
            }
            // Remove bindings introduced in this scope.
            for name in &scope.bindings {
                self.vars.remove(name);
            }
        }
    }

    fn declare(&mut self, name: &str, state: VarState) {
        self.vars.insert(name.to_string(), state);
        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.push(name.to_string());
        }
    }

    fn get(&self, name: &str) -> Option<&VarState> {
        self.vars.get(name)
    }

    fn set(&mut self, name: &str, state: VarState) {
        self.vars.insert(name.to_string(), state);
    }

    fn record_borrow(&mut self, borrow_var: &str, info: BorrowInfo) {
        self.borrow_map.insert(borrow_var.to_string(), info.clone());
        if let Some(scope) = self.scopes.last_mut() {
            scope.borrows.push((borrow_var.to_string(), info));
        }
    }

    /// Decrement the borrow count (or clear mut-borrow flag) on the *source*
    /// variable when a borrow is released.
    fn release_borrow_on_source(&mut self, source: &str, mutable: bool) {
        if let Some(state) = self.vars.get_mut(source) {
            match state {
                VarState::MutBorrowed if mutable => {
                    *state = VarState::Owned;
                }
                VarState::Borrowed { count } if !mutable => {
                    if *count <= 1 {
                        *state = VarState::Owned;
                    } else {
                        *count -= 1;
                    }
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Checker
// ---------------------------------------------------------------------------

struct Checker {
    env: Env,
    errors: Vec<BorrowError>,
    /// Pre-scanned function signatures: fn_name -> Vec<Ownership>.
    /// Used to determine whether a function call moves or borrows its arguments.
    fn_sigs: HashMap<String, Vec<Ownership>>,
}

impl Checker {
    fn new() -> Self {
        Self {
            env: Env::new(),
            fn_sigs: HashMap::new(),
            errors: Vec::new(),
        }
    }

    fn error(&mut self, kind: BorrowErrorKind, span: Span, message: impl Into<String>) {
        self.errors.push(BorrowError {
            kind,
            span,
            message: message.into(),
            source_line: None,
            suggestion: None,
        });
    }

    fn error_with_suggestion(&mut self, kind: BorrowErrorKind, span: Span, message: impl Into<String>, suggestion: impl Into<String>) {
        self.errors.push(BorrowError {
            kind,
            span,
            message: message.into(),
            source_line: None,
            suggestion: Some(suggestion.into()),
        });
    }

    // -- top-level -----------------------------------------------------------

    fn check_program(&mut self, program: &Program) {
        // Pre-scan: collect function signatures for ownership-aware call checking.
        for item in &program.items {
            if let Item::Function(f) = item {
                let ownerships: Vec<Ownership> = f.params.iter()
                    .map(|p| p.ownership.clone())
                    .collect();
                self.fn_sigs.insert(f.name.clone(), ownerships);
            }
        }
        for item in &program.items {
            self.check_item(item);
        }
    }

    fn check_item(&mut self, item: &Item) {
        match item {
            Item::Function(f) => self.check_function(f),
            Item::Impl(imp) => {
                for method in &imp.methods {
                    self.check_function(method);
                }
            }
            Item::Component(c) => self.check_component(c),
            Item::Test(test) => {
                self.env.push_scope();
                self.check_block(&test.body);
                self.env.pop_scope();
            }
            Item::Trait(trait_def) => {
                // Check default method bodies for borrow violations.
                for method in &trait_def.methods {
                    if let Some(ref body) = method.default_body {
                        self.env.push_scope();
                        for param in &method.params {
                            let state = match &param.ownership {
                                Ownership::Borrowed => VarState::Borrowed { count: 0 },
                                Ownership::MutBorrowed => VarState::Owned,
                                Ownership::Owned => VarState::Owned,
                            };
                            self.env.declare(&param.name, state);
                        }
                        self.check_block(body);
                        self.env.pop_scope();
                    }
                }
            }
            // Structs, enums, and use-paths have no runtime behaviour to check.
            _ => {}
        }
    }

    fn check_function(&mut self, func: &Function) {
        self.env.push_scope();

        // Register named lifetimes from function signature as scope markers.
        for lt in &func.lifetimes {
            self.env.push_scope_with_lifetime(lt.clone());
        }

        // Introduce parameters as owned bindings.
        for param in &func.params {
            let state = match &param.ownership {
                Ownership::Borrowed => VarState::Borrowed { count: 0 },
                Ownership::MutBorrowed => VarState::Owned,
                Ownership::Owned => VarState::Owned,
            };
            self.env.declare(&param.name, state);
        }

        // Validate lifetime elision rules.
        self.check_lifetime_elision(func);

        // Validate lifetime consistency for explicit annotations.
        self.check_lifetime_consistency(func);

        // Collect parameter names for return-reference validation.
        let param_names: Vec<String> = func.params.iter().map(|p| p.name.clone()).collect();

        self.check_block(&func.body);

        // Validate that returned references don't point to local variables.
        self.check_return_references(&func.body, &param_names, func.span);

        // Pop lifetime scopes (in reverse order).
        for _ in &func.lifetimes {
            self.env.pop_scope();
        }
        self.env.pop_scope();
    }

    /// Validate lifetime elision rules for a function signature.
    /// - Single input reference -> output gets same lifetime (no annotation needed)
    /// - `&self` methods -> output gets lifetime of self (no annotation needed)
    /// - Multiple input references -> output must be explicitly annotated
    fn check_lifetime_elision(&mut self, func: &Function) {
        let return_has_ref = func.return_type.as_ref().map_or(false, |t| type_has_reference(t));
        if !return_has_ref {
            return;
        }

        let return_has_lifetime = func.return_type.as_ref().map_or(false, |t| type_has_named_lifetime(t));
        if return_has_lifetime {
            return;
        }

        let ref_param_count = func.params.iter()
            .filter(|p| type_has_reference(&p.ty))
            .count();

        let has_self = func.params.iter().any(|p| p.name == "self");

        // &self method -> output gets lifetime of self (elision ok)
        if has_self {
            return;
        }

        // Single input reference -> elision ok
        if ref_param_count == 1 {
            return;
        }

        // Multiple input references with explicit lifetime params -> ok
        if ref_param_count > 1 && !func.lifetimes.is_empty() {
            return;
        }

        if ref_param_count > 1 {
            self.error(
                BorrowErrorKind::MissingLifetimeAnnotation,
                func.span,
                format!(
                    "function `{}` returns a reference but has multiple reference parameters; \
                     explicit lifetime annotations are required",
                    func.name
                ),
            );
        }
    }

    /// Validate that explicit lifetime annotations are consistent.
    ///
    /// Checks:
    /// 1. Return type lifetimes must reference a declared lifetime parameter.
    /// 2. All parameter lifetimes must reference a declared lifetime parameter.
    /// 3. If the return type has a lifetime, at least one parameter must share
    ///    that same lifetime (so the returned reference has a valid source).
    fn check_lifetime_consistency(&mut self, func: &Function) {
        if func.lifetimes.is_empty() {
            return;
        }

        // Collect all lifetime names used in parameter types
        let mut param_lifetimes: Vec<String> = Vec::new();
        for param in &func.params {
            collect_lifetimes_from_type(&param.ty, &mut param_lifetimes);
        }

        // Collect all lifetime names used in the return type
        let mut return_lifetimes: Vec<String> = Vec::new();
        if let Some(ret_ty) = &func.return_type {
            collect_lifetimes_from_type(ret_ty, &mut return_lifetimes);
        }

        // Check 1: All lifetimes used in parameters must be declared
        for lt in &param_lifetimes {
            if !func.lifetimes.contains(lt) {
                self.error(
                    BorrowErrorKind::LifetimeViolation,
                    func.span,
                    format!(
                        "function `{}` uses undeclared lifetime `'{}` in parameter type",
                        func.name, lt
                    ),
                );
            }
        }

        // Check 2: All lifetimes used in return type must be declared
        for lt in &return_lifetimes {
            if !func.lifetimes.contains(lt) {
                self.error(
                    BorrowErrorKind::LifetimeViolation,
                    func.span,
                    format!(
                        "function `{}` uses undeclared lifetime `'{}` in return type",
                        func.name, lt
                    ),
                );
            }
        }

        // Check 3: Each return-type lifetime must appear on at least one parameter.
        // This ensures the returned reference has a valid source with matching lifetime.
        for ret_lt in &return_lifetimes {
            if !param_lifetimes.contains(ret_lt) {
                self.error(
                    BorrowErrorKind::LifetimeViolation,
                    func.span,
                    format!(
                        "function `{}` returns a reference with lifetime `'{}` but no parameter \
                         has that lifetime; the returned reference must borrow from a parameter",
                        func.name, ret_lt
                    ),
                );
            }
        }
    }

    fn check_component(&mut self, comp: &Component) {
        for method in &comp.methods {
            self.check_function(method);
        }
    }

    // -- blocks / statements ------------------------------------------------

    fn check_block(&mut self, block: &Block) {
        // NLL: compute last-use indices for borrow variables.
        let last_use = Self::find_last_use_in_block(block);
        for (idx, stmt) in block.stmts.iter().enumerate() {
            // Release borrows that are dead before this statement.
            self.release_dead_borrows(&last_use, idx);
            self.check_stmt(stmt, block.span);
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt, enclosing_span: Span) {
        match stmt {
            Stmt::Let {
                name,
                value,
                ownership,
                ..
            } => {
                self.check_let(name, value, ownership, enclosing_span);
            }
            Stmt::Signal { name, value, .. } => {
                // Signals are reactive state; treat them as owned values.
                self.check_expr(value, enclosing_span);
                self.env.declare(name, VarState::Owned);
            }
            Stmt::Expr(expr) => {
                self.check_expr(expr, enclosing_span);
            }
            Stmt::Return(maybe_expr) => {
                if let Some(expr) = maybe_expr {
                    self.check_expr(expr, enclosing_span);
                }
            }
            Stmt::Yield(expr) => {
                self.check_expr(expr, enclosing_span);
            }
            Stmt::LetDestructure { pattern, value, .. } => {
                self.check_expr(value, enclosing_span);
                self.declare_pattern_bindings(pattern);
            }
        }
    }

    fn check_let(
        &mut self,
        name: &str,
        value: &Expr,
        ownership: &Ownership,
        span: Span,
    ) {
        // First, evaluate the right-hand side to detect moves/borrows.
        match value {
            Expr::Borrow(inner) => {
                let source = self.expr_as_ident(inner);
                if let Some(source_name) = source {
                    self.create_immutable_borrow(name, &source_name, span);
                } else {
                    // Borrowing a non-ident expression -- just check it.
                    self.check_expr(value, span);
                    self.env.declare(name, VarState::Owned);
                }
            }
            Expr::BorrowMut(inner) => {
                let source = self.expr_as_ident(inner);
                if let Some(source_name) = source {
                    self.create_mutable_borrow(name, &source_name, span);
                } else {
                    self.check_expr(value, span);
                    self.env.declare(name, VarState::Owned);
                }
            }
            Expr::Ident(source_name) => {
                // Assignment from another variable -- this is a *move* unless
                // the ownership annotation says otherwise.
                //
                // Keyword-like constructors (`None`, `Some`, `Ok`, `Err`) are
                // Copy values / enum constructors; they are never moved out of
                // the environment.
                let is_builtin_constructor = matches!(
                    source_name.as_str(),
                    "None" | "Some" | "Ok" | "Err" | "true" | "false"
                );
                if is_builtin_constructor {
                    self.check_expr(value, span);
                    self.env.declare(name, VarState::Owned);
                } else {
                    match ownership {
                        Ownership::Borrowed => {
                            self.create_immutable_borrow(name, source_name, span);
                        }
                        Ownership::MutBorrowed => {
                            self.create_mutable_borrow(name, source_name, span);
                        }
                        Ownership::Owned => {
                            // Move.
                            self.move_var(source_name, name, span);
                            self.env.declare(name, VarState::Owned);
                        }
                    }
                }
            }
            _ => {
                self.check_expr(value, span);
                self.env.declare(name, VarState::Owned);
            }
        }
    }

    // -- expressions --------------------------------------------------------

    fn check_expr(&mut self, expr: &Expr, span: Span) {
        match expr {
            Expr::Ident(name) => {
                // Builtin constructors and enum variants are not variables in
                // the move-tracking environment; skip the moved check.
                let is_builtin = matches!(
                    name.as_str(),
                    "None" | "Some" | "Ok" | "Err" | "true" | "false"
                ) || name.contains("::");
                if !is_builtin {
                    self.assert_not_moved(name, span);
                }
            }
            Expr::Integer(_)
            | Expr::Float(_)
            | Expr::StringLit(_)
            | Expr::Bool(_)
            | Expr::SelfExpr => {}

            Expr::Binary { left, right, .. } => {
                self.check_expr(left, span);
                self.check_expr(right, span);
            }
            Expr::Unary { operand, .. } => {
                self.check_expr(operand, span);
            }

            Expr::FieldAccess { object, .. } | Expr::OptionalChain { object, .. } => {
                self.check_expr(object, span);
            }
            Expr::MethodCall { object, args, .. } => {
                self.check_expr(object, span);
                for arg in args {
                    self.check_expr(arg, span);
                }
            }
            Expr::FnCall { callee, args } => {
                // Determine callee name for signature lookup.
                let callee_name = self.expr_as_ident(callee);
                let param_ownerships = callee_name
                    .as_ref()
                    .and_then(|name| self.fn_sigs.get(name).cloned());

                self.check_expr(callee, span);
                for (i, arg) in args.iter().enumerate() {
                    let ownership = param_ownerships
                        .as_ref()
                        .and_then(|ownerships| ownerships.get(i))
                        .cloned()
                        .unwrap_or(Ownership::Owned);

                    match ownership {
                        Ownership::Borrowed | Ownership::MutBorrowed => {
                            // Borrowed param: just check the arg, no move.
                            self.check_expr(arg, span);
                        }
                        Ownership::Owned => {
                            // Owned param: moves the argument.
                            if let Expr::Ident(name) = arg {
                                self.assert_not_moved(name, span);
                                self.move_var(name, "<function argument>", span);
                            } else {
                                self.check_expr(arg, span);
                            }
                        }
                    }
                }
            }
            Expr::Index { object, index } => {
                self.check_expr(object, span);
                self.check_expr(index, span);
            }

            // Control flow -- each branch gets its own scope.
            Expr::If {
                condition,
                then_block,
                else_block,
            } => {
                self.check_expr(condition, span);
                self.env.push_scope();
                self.check_block(then_block);
                self.env.pop_scope();
                if let Some(else_blk) = else_block {
                    self.env.push_scope();
                    self.check_block(else_blk);
                    self.env.pop_scope();
                }
            }
            Expr::Match { subject, arms } => {
                self.check_expr(subject, span);
                for arm in arms {
                    self.env.push_scope();
                    self.declare_pattern_bindings(&arm.pattern);
                    if let Some(guard) = &arm.guard {
                        self.check_expr(guard, span);
                    }
                    self.check_expr(&arm.body, span);
                    self.env.pop_scope();
                }
            }
            Expr::For {
                binding,
                iterator,
                body,
            } => {
                self.check_expr(iterator, span);
                self.env.push_scope();
                self.env.declare(binding, VarState::Owned);
                self.check_block(body);
                self.env.pop_scope();
            }
            Expr::While { condition, body } => {
                self.check_expr(condition, span);
                self.env.push_scope();
                self.check_block(body);
                self.env.pop_scope();
            }
            Expr::Block(block) => {
                self.env.push_scope();
                self.check_block(block);
                self.env.pop_scope();
            }

            Expr::Borrow(inner) => {
                if let Expr::Ident(name) = inner.as_ref() {
                    self.assert_not_moved(name, span);
                    // Allow reborrowing: &r where r: &mut T is valid (immutable reborrow).
                    // Only error if the variable is not a mutable borrow holder.
                    let is_reborrow = self.env.borrow_map.get(name)
                        .map(|info| info.mutable)
                        .unwrap_or(false);
                    if !is_reborrow {
                        self.assert_not_mut_borrowed(name, span);
                    }
                } else {
                    self.check_expr(inner, span);
                }
            }
            Expr::BorrowMut(inner) => {
                if let Expr::Ident(name) = inner.as_ref() {
                    self.assert_not_moved(name, span);
                    self.assert_no_active_borrows(name, span);
                } else {
                    self.check_expr(inner, span);
                }
            }

            Expr::StructInit { fields, .. } => {
                for (_fname, fval) in fields {
                    self.check_expr(fval, span);
                }
            }

            Expr::Assign { target, value } => {
                // If the target is currently borrowed, we cannot assign to it.
                if let Expr::Ident(name) = target.as_ref() {
                    match self.env.get(name).cloned() {
                        Some(VarState::Borrowed { .. }) | Some(VarState::MutBorrowed) => {
                            self.error(
                                BorrowErrorKind::AssignWhileBorrowed,
                                span,
                                format!("cannot assign to `{}` because it is currently borrowed", name),
                            );
                        }
                        _ => {}
                    }
                }
                self.check_expr(value, span);
            }

            Expr::Await(inner) => {
                self.check_expr(inner, span);
            }
            Expr::Fetch { url, options, .. } => {
                self.check_expr(url, span);
                if let Some(opts) = options {
                    self.check_expr(opts, span);
                }
            }
            Expr::Closure { params, body } => {
                // Closures capture variables from the enclosing scope.
                let param_names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();

                // Walk the closure body to find captured variables.
                let captures = collect_captures(body, &param_names);

                // For each captured variable, check borrow rules.
                for cap in &captures {
                    if let Some(state) = self.env.get(cap).cloned() {
                        match state {
                            VarState::Moved { .. } => {
                                self.error(
                                    BorrowErrorKind::UseAfterMove,
                                    span,
                                    format!("closure captures moved variable `{}`", cap),
                                );
                            }
                            VarState::MutBorrowed => {
                                self.error(
                                    BorrowErrorKind::ImmBorrowWhileMutBorrowed,
                                    span,
                                    format!("closure captures `{}` which is already mutably borrowed", cap),
                                );
                            }
                            _ => {
                                if body_mutates_var(body, cap) {
                                    self.assert_no_active_borrows(cap, span);
                                }
                            }
                        }
                    }
                }

                // Check the closure body with params declared in a child scope.
                self.env.push_scope();
                for name in &param_names {
                    self.env.declare(name, VarState::Owned);
                }
                self.check_expr(body, span);
                self.env.pop_scope();
            }
            Expr::PromptTemplate { interpolations, .. } => {
                for (_name, expr) in interpolations {
                    self.check_expr(expr, span);
                }
            }
            Expr::Navigate { path } => {
                self.check_expr(path, span);
            }
            Expr::Stream { source } => {
                self.check_expr(source, span);
            }
            Expr::Suspend { fallback, body } => {
                self.check_expr(fallback, span);
                self.check_expr(body, span);
            }
            Expr::Spawn { body, .. } => {
                self.check_block(body);
            }
            Expr::Channel { .. } => {}
            Expr::Send { channel, value } => {
                self.check_expr(channel, span);
                self.check_expr(value, span);
            }
            Expr::Receive { channel } => {
                self.check_expr(channel, span);
            }
            Expr::Parallel { tasks, .. } => {
                for expr in tasks {
                    self.check_expr(expr, span);
                }
            }
            Expr::TryCatch { body, error_binding, catch_body } => {
                self.env.push_scope();
                self.check_expr(body, span);
                self.env.pop_scope();
                self.env.push_scope();
                self.env.declare(error_binding, VarState::Owned);
                self.check_expr(catch_body, span);
                self.env.pop_scope();
            }
            Expr::Assert { condition, .. } => {
                self.check_expr(condition, span);
            }
            Expr::AssertEq { left, right, .. } => {
                self.check_expr(left, span);
                self.check_expr(right, span);
            }
            Expr::Animate { target, .. } => {
                self.check_expr(target, span);
            }
            Expr::FormatString { parts } => {
                for part in parts {
                    if let FormatPart::Expression(expr) = part {
                        self.check_expr(expr, span);
                    }
                }
            }
            Expr::Try(inner) => {
                self.check_expr(inner, span);
            }
            Expr::DynamicImport { path, .. } => {
                self.check_expr(path, span);
            }
            Expr::Download { data, filename, .. } => {
                self.check_expr(data, span);
                self.check_expr(filename, span);
            }
            Expr::Env { name, .. } => {
                self.check_expr(name, span);
            }
            Expr::Trace { label, body, .. } => {
                self.check_expr(label, span);
                for stmt in &body.stmts {
                    match stmt {
                        Stmt::Expr(e) | Stmt::Let { value: e, .. } | Stmt::Signal { value: e, .. } | Stmt::Yield(e) => {
                            self.check_expr(e, span);
                        }
                        Stmt::Return(Some(e)) => self.check_expr(e, span),
                        _ => {}
                    }
                }
            }
            Expr::Flag { name, .. } => {
                self.check_expr(name, span);
            }
            Expr::Range { start, end } => {
                self.check_expr(start, span);
                self.check_expr(end, span);
            }
            Expr::Break | Expr::Continue => {
                // No ownership effects
            }
            Expr::VirtualList { items, item_height, template, .. } => {
                self.check_expr(items, span);
                self.check_expr(item_height, span);
                self.check_expr(template, span);
            }
            Expr::ArrayLit(elements) => {
                for elem in elements {
                    self.check_expr(elem, span);
                }
            }
            Expr::ObjectLit { fields } => {
                for (_key, value) in fields {
                    self.check_expr(value, span);
                }
            }
            Expr::TupleLit(elements) => {
                for elem in elements {
                    self.check_expr(elem, span);
                }
            }
        }
    }

    // -- helpers ------------------------------------------------------------

    /// If `expr` is a simple identifier, return its name.
    fn expr_as_ident(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Ident(name) => Some(name.clone()),
            _ => None,
        }
    }

    /// Extract a dotted path from an expression for partial/field borrow tracking.
    /// `obj.x.y` -> Some("obj.x.y"), `obj` -> Some("obj"), anything else -> None.
    fn expr_as_path(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Ident(name) => Some(name.clone()),
            Expr::FieldAccess { object, field } => {
                self.expr_as_path(object).map(|base| format!("{}.{}", base, field))
            }
            _ => None,
        }
    }

    /// Check if two borrow paths conflict. Two paths conflict if one is a prefix
    /// of the other (e.g., "obj" and "obj.x" conflict, "obj.x" and "obj.y" do not).
    fn paths_conflict(a: &str, b: &str) -> bool {
        a == b || a.starts_with(&format!("{}.", b)) || b.starts_with(&format!("{}.", a))
    }

    fn assert_not_moved(&mut self, name: &str, span: Span) {
        if let Some(VarState::Moved { moved_to }) = self.env.get(name).cloned() {
            self.error(
                BorrowErrorKind::UseAfterMove,
                span,
                format!(
                    "use of moved value `{}` (value was moved to {})",
                    name, moved_to
                ),
            );
        }
    }

    fn assert_not_mut_borrowed(&mut self, name: &str, span: Span) {
        if let Some(VarState::MutBorrowed) = self.env.get(name) {
            self.error(
                BorrowErrorKind::ImmBorrowWhileMutBorrowed,
                span,
                format!(
                    "cannot immutably borrow `{}` because it is already mutably borrowed",
                    name
                ),
            );
        }
    }

    fn assert_no_active_borrows(&mut self, name: &str, span: Span) {
        match self.env.get(name) {
            Some(VarState::MutBorrowed) => {
                self.error(
                    BorrowErrorKind::DoubleMutBorrow,
                    span,
                    format!("cannot borrow `{}` as mutable more than once at a time", name),
                );
            }
            Some(VarState::Borrowed { count }) if *count > 0 => {
                self.error(
                    BorrowErrorKind::MutBorrowWhileImmBorrowed,
                    span,
                    format!(
                        "cannot borrow `{}` as mutable because it is already borrowed as immutable",
                        name
                    ),
                );
            }
            _ => {}
        }
    }

    fn move_var(&mut self, source: &str, dest: &str, span: Span) {
        self.assert_not_moved(source, span);

        // Check that the source is not currently borrowed.
        match self.env.get(source) {
            Some(VarState::Borrowed { count }) if *count > 0 => {
                self.error(
                    BorrowErrorKind::AssignWhileBorrowed,
                    span,
                    format!("cannot move `{}` because it is currently borrowed", source),
                );
            }
            Some(VarState::MutBorrowed) => {
                self.error(
                    BorrowErrorKind::AssignWhileBorrowed,
                    span,
                    format!(
                        "cannot move `{}` because it is currently mutably borrowed",
                        source
                    ),
                );
            }
            _ => {}
        }

        self.env.set(
            source,
            VarState::Moved {
                moved_to: format!("`{}`", dest),
            },
        );
    }

    fn create_immutable_borrow(&mut self, borrow_var: &str, source: &str, span: Span) {
        self.assert_not_moved(source, span);
        self.assert_not_mut_borrowed(source, span);

        // Bump immutable borrow count on the source.
        let new_state = match self.env.get(source) {
            Some(VarState::Borrowed { count }) => VarState::Borrowed { count: count + 1 },
            Some(VarState::Owned) => VarState::Borrowed { count: 1 },
            _ => VarState::Borrowed { count: 1 },
        };
        self.env.set(source, new_state);

        // Declare the borrowing variable.
        self.env.declare(borrow_var, VarState::Owned);

        // Record the borrow so it is released when `borrow_var` goes out of scope.
        self.env.record_borrow(
            borrow_var,
            BorrowInfo {
                source_var: source.to_string(),
                mutable: false,
                scope_depth: self.env.depth(),
                lifetime: None,
                last_use_stmt_idx: None,
            },
        );
    }

    fn create_mutable_borrow(&mut self, borrow_var: &str, source: &str, span: Span) {
        self.assert_not_moved(source, span);
        self.assert_no_active_borrows(source, span);

        self.env.set(source, VarState::MutBorrowed);

        self.env.declare(borrow_var, VarState::Owned);

        self.env.record_borrow(
            borrow_var,
            BorrowInfo {
                source_var: source.to_string(),
                mutable: true,
                scope_depth: self.env.depth(),
                lifetime: None,
                last_use_stmt_idx: None,
            },
        );
    }

    /// Verify that no return statement returns a reference to a local variable.
    /// References to function parameters are allowed (they outlive the function call).
    fn check_return_references(&mut self, block: &Block, param_names: &[String], span: Span) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Return(Some(expr)) => {
                    self.check_return_expr_for_local_refs(expr, param_names, span);
                }
                Stmt::Expr(expr) => {
                    self.check_return_in_expr(expr, param_names, span);
                }
                _ => {}
            }
        }
    }

    fn check_return_expr_for_local_refs(&mut self, expr: &Expr, param_names: &[String], span: Span) {
        match expr {
            Expr::Borrow(inner) | Expr::BorrowMut(inner) => {
                if let Some(name) = self.expr_as_ident(inner) {
                    if !param_names.contains(&name) {
                        self.error_with_suggestion(
                            BorrowErrorKind::BorrowOutlivesScope,
                            span,
                            format!("cannot return reference to local variable `{}`", name),
                            "consider returning the value directly instead of a reference".to_string(),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn check_return_in_expr(&mut self, expr: &Expr, param_names: &[String], span: Span) {
        match expr {
            Expr::If { then_block, else_block, .. } => {
                self.check_return_references(then_block, param_names, span);
                if let Some(eb) = else_block {
                    self.check_return_references(eb, param_names, span);
                }
            }
            Expr::Block(block) => {
                self.check_return_references(block, param_names, span);
            }
            _ => {}
        }
    }

    /// NLL liveness pre-pass: find the last statement index where each variable
    /// is used in a block. This allows borrows to be released at last use rather
    /// than at scope exit.
    fn find_last_use_in_block(block: &Block) -> HashMap<String, usize> {
        let mut last_use: HashMap<String, usize> = HashMap::new();
        for (idx, stmt) in block.stmts.iter().enumerate() {
            let mut vars = Vec::new();
            Self::collect_used_vars_in_stmt(stmt, &mut vars);
            for var in vars {
                last_use.insert(var, idx);
            }
        }
        last_use
    }

    fn collect_used_vars_in_stmt(stmt: &Stmt, out: &mut Vec<String>) {
        match stmt {
            Stmt::Let { value, .. } => Self::collect_used_vars_in_expr(value, out),
            Stmt::Expr(e) => Self::collect_used_vars_in_expr(e, out),
            Stmt::Return(Some(e)) => Self::collect_used_vars_in_expr(e, out),
            Stmt::Signal { value, .. } => Self::collect_used_vars_in_expr(value, out),
            Stmt::Yield(e) => Self::collect_used_vars_in_expr(e, out),
            _ => {}
        }
    }

    fn collect_used_vars_in_expr(expr: &Expr, out: &mut Vec<String>) {
        match expr {
            Expr::Ident(name) => out.push(name.clone()),
            Expr::Binary { left, right, .. } => {
                Self::collect_used_vars_in_expr(left, out);
                Self::collect_used_vars_in_expr(right, out);
            }
            Expr::Unary { operand, .. } => Self::collect_used_vars_in_expr(operand, out),
            Expr::FnCall { callee, args } => {
                Self::collect_used_vars_in_expr(callee, out);
                for arg in args {
                    Self::collect_used_vars_in_expr(arg, out);
                }
            }
            Expr::FieldAccess { object, .. } => Self::collect_used_vars_in_expr(object, out),
            Expr::MethodCall { object, args, .. } => {
                Self::collect_used_vars_in_expr(object, out);
                for arg in args {
                    Self::collect_used_vars_in_expr(arg, out);
                }
            }
            Expr::Borrow(inner) | Expr::BorrowMut(inner) => {
                Self::collect_used_vars_in_expr(inner, out);
            }
            Expr::Index { object, index } => {
                Self::collect_used_vars_in_expr(object, out);
                Self::collect_used_vars_in_expr(index, out);
            }
            Expr::Assign { target, value } => {
                Self::collect_used_vars_in_expr(target, out);
                Self::collect_used_vars_in_expr(value, out);
            }
            _ => {}
        }
    }

    /// Release borrows that are no longer live at the given statement index.
    /// Called during block checking to implement NLL.
    fn release_dead_borrows(&mut self, last_use: &HashMap<String, usize>, current_idx: usize) {
        // Find borrow variables whose last use is before current_idx.
        let to_release: Vec<(String, BorrowInfo)> = self.env.borrow_map.iter()
            .filter(|(var, _info)| {
                last_use.get(var.as_str())
                    .map(|&lu| lu < current_idx)
                    .unwrap_or(false)
            })
            .map(|(var, info)| (var.clone(), info.clone()))
            .collect();

        for (var, info) in to_release {
            self.env.release_borrow_on_source(&info.source_var, info.mutable);
            self.env.borrow_map.remove(&var);
        }
    }

    fn declare_pattern_bindings(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Ident(name) => {
                self.env.declare(name, VarState::Owned);
            }
            Pattern::Variant { fields, .. } => {
                for p in fields {
                    self.declare_pattern_bindings(p);
                }
            }
            Pattern::Wildcard | Pattern::Literal(_) => {}
            Pattern::Tuple(patterns) | Pattern::Array(patterns) => {
                for p in patterns {
                    self.declare_pattern_bindings(p);
                }
            }
            Pattern::Struct { fields, .. } => {
                for (_name, p) in fields {
                    self.declare_pattern_bindings(p);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Type reference helpers
// ---------------------------------------------------------------------------

/// Returns true if the AST type contains a reference.
fn type_has_reference(ty: &Type) -> bool {
    match ty {
        Type::Reference { .. } => true,
        Type::Array(inner) | Type::Option(inner) => type_has_reference(inner),
        Type::Generic { args, .. } => args.iter().any(type_has_reference),
        Type::Tuple(elems) => elems.iter().any(type_has_reference),
        Type::Function { params, ret } => {
            params.iter().any(type_has_reference) || type_has_reference(ret)
        }
        _ => false,
    }
}

/// Collect all named lifetime strings from a type into the `out` vector.
fn collect_lifetimes_from_type(ty: &Type, out: &mut Vec<String>) {
    match ty {
        Type::Reference { lifetime, inner, .. } => {
            if let Some(lt) = lifetime {
                if !out.contains(lt) {
                    out.push(lt.clone());
                }
            }
            collect_lifetimes_from_type(inner, out);
        }
        Type::Array(inner) | Type::Option(inner) => collect_lifetimes_from_type(inner, out),
        Type::Generic { args, .. } => {
            for arg in args {
                collect_lifetimes_from_type(arg, out);
            }
        }
        Type::Tuple(elems) => {
            for elem in elems {
                collect_lifetimes_from_type(elem, out);
            }
        }
        Type::Function { params, ret } => {
            for p in params { collect_lifetimes_from_type(p, out); }
            collect_lifetimes_from_type(ret, out);
        }
        Type::Result { ok, err } => {
            collect_lifetimes_from_type(ok, out);
            collect_lifetimes_from_type(err, out);
        }
        _ => {}
    }
}

/// Returns true if the AST type contains a named lifetime.
fn type_has_named_lifetime(ty: &Type) -> bool {
    match ty {
        Type::Reference { lifetime, inner, .. } => {
            lifetime.is_some() || type_has_named_lifetime(inner)
        }
        Type::Array(inner) | Type::Option(inner) => type_has_named_lifetime(inner),
        Type::Generic { args, .. } => args.iter().any(type_has_named_lifetime),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Closure capture helpers
// ---------------------------------------------------------------------------

/// Collect all identifiers referenced in `expr` that are not in `local_names`.
/// These are the variables captured from the enclosing scope.
fn collect_captures(expr: &Expr, local_names: &[String]) -> Vec<String> {
    let mut captures = Vec::new();
    collect_captures_inner(expr, local_names, &mut captures);
    captures.sort();
    captures.dedup();
    captures
}

fn collect_captures_inner(expr: &Expr, locals: &[String], out: &mut Vec<String>) {
    match expr {
        Expr::Ident(name) => {
            if !locals.contains(name) {
                out.push(name.clone());
            }
        }
        Expr::Binary { left, right, .. } => {
            collect_captures_inner(left, locals, out);
            collect_captures_inner(right, locals, out);
        }
        Expr::Unary { operand, .. } => {
            collect_captures_inner(operand, locals, out);
        }
        Expr::FnCall { callee, args } => {
            collect_captures_inner(callee, locals, out);
            for arg in args {
                collect_captures_inner(arg, locals, out);
            }
        }
        Expr::FieldAccess { object, .. } | Expr::OptionalChain { object, .. } => {
            collect_captures_inner(object, locals, out);
        }
        Expr::MethodCall { object, args, .. } => {
            collect_captures_inner(object, locals, out);
            for arg in args {
                collect_captures_inner(arg, locals, out);
            }
        }
        Expr::If { condition, then_block, else_block } => {
            collect_captures_inner(condition, locals, out);
            for stmt in &then_block.stmts {
                if let Stmt::Expr(e) = stmt { collect_captures_inner(e, locals, out); }
            }
            if let Some(blk) = else_block {
                for stmt in &blk.stmts {
                    if let Stmt::Expr(e) = stmt { collect_captures_inner(e, locals, out); }
                }
            }
        }
        Expr::Block(block) => {
            for stmt in &block.stmts {
                if let Stmt::Expr(e) = stmt { collect_captures_inner(e, locals, out); }
            }
        }
        Expr::Assign { target, value } => {
            collect_captures_inner(target, locals, out);
            collect_captures_inner(value, locals, out);
        }
        Expr::Index { object, index } => {
            collect_captures_inner(object, locals, out);
            collect_captures_inner(index, locals, out);
        }
        Expr::Borrow(inner) | Expr::BorrowMut(inner) | Expr::Await(inner) | Expr::Try(inner) => {
            collect_captures_inner(inner, locals, out);
        }
        // For other expression types, we do a best-effort walk.
        _ => {}
    }
}

/// Check whether the closure body mutates (assigns to) a variable by name.
fn body_mutates_var(expr: &Expr, var: &str) -> bool {
    match expr {
        Expr::Assign { target, value } => {
            if let Expr::Ident(name) = target.as_ref() {
                if name == var { return true; }
            }
            body_mutates_var(value, var)
        }
        Expr::Binary { left, right, .. } => {
            body_mutates_var(left, var) || body_mutates_var(right, var)
        }
        Expr::Block(block) => {
            block.stmts.iter().any(|s| {
                if let Stmt::Expr(e) = s { body_mutates_var(e, var) } else { false }
            })
        }
        Expr::If { condition, then_block, else_block } => {
            body_mutates_var(condition, var)
                || then_block.stmts.iter().any(|s| if let Stmt::Expr(e) = s { body_mutates_var(e, var) } else { false })
                || else_block.as_ref().is_some_and(|b| b.stmts.iter().any(|s| if let Stmt::Expr(e) = s { body_mutates_var(e, var) } else { false }))
        }
        Expr::FnCall { callee, args } => {
            body_mutates_var(callee, var) || args.iter().any(|a| body_mutates_var(a, var))
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the borrow checker over a parsed program.
///
/// Returns `Ok(())` when no ownership violations are found, or
/// `Err(errors)` with a list of every violation detected.
pub fn check(program: &Program) -> Result<(), Vec<BorrowError>> {
    let mut checker = Checker::new();
    checker.check_program(program);

    if checker.errors.is_empty() {
        Ok(())
    } else {
        Err(checker.errors)
    }
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

    fn ident(name: &str) -> Expr {
        Expr::Ident(name.to_string())
    }

    fn int_lit(v: i64) -> Expr {
        Expr::Integer(v)
    }

    /// Helper: wrap statements into a single-function program.
    fn program_with_stmts(stmts: Vec<Stmt>) -> Program {
        Program {
            items: vec![Item::Function(Function {
                name: "main".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts,
                    span: span(),
                },
                is_pub: true,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        }
    }

    // -----------------------------------------------------------------------
    // Use after move
    // -----------------------------------------------------------------------

    #[test]
    fn use_after_move_detected() {
        // let x = 42;
        // let y = x;   // moves x
        // let z = x;   // ERROR: use after move
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "x".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: int_lit(42),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "y".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: ident("x"),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "z".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: ident("x"),
                ownership: Ownership::Owned,
            },
        ]);

        let result = check(&prog);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Double mutable borrow
    // -----------------------------------------------------------------------

    #[test]
    fn double_mut_borrow_detected() {
        // let mut x = 42;
        // let a = &mut x;
        // let b = &mut x;  // ERROR: already mutably borrowed
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "x".to_string(),
                ty: None,
                mutable: true,
                secret: false,
                value: int_lit(42),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "a".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::BorrowMut(Box::new(ident("x"))),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "b".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::BorrowMut(Box::new(ident("x"))),
                ownership: Ownership::Owned,
            },
        ]);

        let result = check(&prog);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].kind, BorrowErrorKind::DoubleMutBorrow);
    }

    // -----------------------------------------------------------------------
    // Mutable borrow while immutably borrowed
    // -----------------------------------------------------------------------

    #[test]
    fn mut_borrow_while_imm_borrowed_detected() {
        // let x = 42;
        // let a = &x;
        // let b = &mut x;  // ERROR
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "x".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: int_lit(42),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "a".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Borrow(Box::new(ident("x"))),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "b".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::BorrowMut(Box::new(ident("x"))),
                ownership: Ownership::Owned,
            },
        ]);

        let result = check(&prog);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].kind, BorrowErrorKind::MutBorrowWhileImmBorrowed);
    }

    // -----------------------------------------------------------------------
    // Valid: multiple immutable borrows
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_immutable_borrows_ok() {
        // let x = 42;
        // let a = &x;
        // let b = &x;
        // a;  // use a
        // b;  // use b
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "x".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: int_lit(42),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "a".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Borrow(Box::new(ident("x"))),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "b".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Borrow(Box::new(ident("x"))),
                ownership: Ownership::Owned,
            },
            Stmt::Expr(ident("a")),
            Stmt::Expr(ident("b")),
        ]);

        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Valid: borrow ends at scope exit, then mut borrow is fine
    // -----------------------------------------------------------------------

    #[test]
    fn scope_exit_releases_borrows() {
        // let x = 42;
        // { let a = &x; }   // borrow released
        // let b = &mut x;   // OK -- no active borrows
        let inner_block = Block {
            stmts: vec![Stmt::Let {
                name: "a".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Borrow(Box::new(ident("x"))),
                ownership: Ownership::Owned,
            }],
            span: span(),
        };

        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "x".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: int_lit(42),
                ownership: Ownership::Owned,
            },
            Stmt::Expr(Expr::Block(inner_block)),
            Stmt::Let {
                name: "b".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::BorrowMut(Box::new(ident("x"))),
                ownership: Ownership::Owned,
            },
        ]);

        let result = check(&prog);
        assert!(result.is_ok(), "expected Ok but got: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // Scope exit invalidation: use-after-move does not leak across scopes
    // -----------------------------------------------------------------------

    #[test]
    fn scope_exit_invalidation() {
        // let x = 42;
        // { let y = x; }  // x moved inside inner scope
        // let z = x;      // ERROR: use after move
        let inner_block = Block {
            stmts: vec![Stmt::Let {
                name: "y".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: ident("x"),
                ownership: Ownership::Owned,
            }],
            span: span(),
        };

        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "x".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: int_lit(42),
                ownership: Ownership::Owned,
            },
            Stmt::Expr(Expr::Block(inner_block)),
            Stmt::Let {
                name: "z".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: ident("x"),
                ownership: Ownership::Owned,
            },
        ]);

        let result = check(&prog);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Valid: simple owned values, no borrows, no moves
    // -----------------------------------------------------------------------

    #[test]
    fn simple_owned_values_ok() {
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "x".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: int_lit(1),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "y".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: int_lit(2),
                ownership: Ownership::Owned,
            },
            Stmt::Expr(Expr::Binary {
                op: BinOp::Add,
                left: Box::new(ident("x")),
                right: Box::new(ident("y")),
            }),
        ]);

        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Lifetime elision: multiple ref params returning ref needs annotation
    // -----------------------------------------------------------------------

    #[test]
    fn lifetime_elision_multiple_refs_returning_ref_needs_annotation() {
        // fn longest(a: &i32, b: &i32) -> &i32 { ... }
        // Should error: multiple ref params, returning ref, no lifetime annotation
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "longest".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![
                    Param {
                        name: "a".to_string(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: None,
                            inner: Box::new(Type::Named("i32".to_string())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
},
                    Param {
                        name: "b".to_string(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: None,
                            inner: Box::new(Type::Named("i32".to_string())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
},
                ],
                return_type: Some(Type::Reference {
                    mutable: false,
                    lifetime: None,
                    inner: Box::new(Type::Named("i32".to_string())),
                }),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(ident("a")))],
                    span: span(),
                },
                is_pub: false,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };

        let result = check(&prog);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors[0].kind, BorrowErrorKind::MissingLifetimeAnnotation);
    }

    #[test]
    fn lifetime_elision_single_ref_returning_ref_ok() {
        // fn first(a: &i32) -> &i32 { ... }
        // Single input reference -> elision ok
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "first".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![Param {
                    name: "a".to_string(),
                    ty: Type::Reference {
                        mutable: false,
                        lifetime: None,
                        inner: Box::new(Type::Named("i32".to_string())),
                    },
                    ownership: Ownership::Borrowed,
                    secret: false,
}],
                return_type: Some(Type::Reference {
                    mutable: false,
                    lifetime: None,
                    inner: Box::new(Type::Named("i32".to_string())),
                }),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(ident("a")))],
                    span: span(),
                },
                is_pub: false,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };

        let result = check(&prog);
        assert!(result.is_ok());
    }

    #[test]
    fn lifetime_annotation_multiple_refs_returning_ref_ok() {
        // fn longest<'a>(a: &'a i32, b: &'a i32) -> &'a i32 { ... }
        // Explicit lifetime annotation -> ok
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "longest".to_string(),
                lifetimes: vec!["a".to_string()],
                type_params: vec![],
                params: vec![
                    Param {
                        name: "a".to_string(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: Some("a".to_string()),
                            inner: Box::new(Type::Named("i32".to_string())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
},
                    Param {
                        name: "b".to_string(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: Some("a".to_string()),
                            inner: Box::new(Type::Named("i32".to_string())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
},
                ],
                return_type: Some(Type::Reference {
                    mutable: false,
                    lifetime: Some("a".to_string()),
                    inner: Box::new(Type::Named("i32".to_string())),
                }),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(ident("a")))],
                    span: span(),
                },
                is_pub: false,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };

        let result = check(&prog);
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod comprehensive_borrow_tests {
    use super::*;
    use crate::token::Span;

    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    fn ident(name: &str) -> Expr {
        Expr::Ident(name.to_string())
    }

    fn int_lit(v: i64) -> Expr {
        Expr::Integer(v)
    }

    fn program_with_stmts(stmts: Vec<Stmt>) -> Program {
        Program {
            items: vec![Item::Function(Function {
                name: "main".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block { stmts, span: span() },
                is_pub: true,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        }
    }

    // -----------------------------------------------------------------------
    // Immutable borrow while mutably borrowed
    // -----------------------------------------------------------------------

    #[test]
    fn imm_borrow_while_mut_borrowed() {
        // let mut x = 42;
        // let a = &mut x;
        // let b = &x;  // ERROR
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: true, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "a".into(), ty: None, mutable: false, secret: false, value: Expr::BorrowMut(Box::new(ident("x"))), ownership: Ownership::Owned },
            Stmt::Let { name: "b".into(), ty: None, mutable: false, secret: false, value: Expr::Borrow(Box::new(ident("x"))), ownership: Ownership::Owned },
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::ImmBorrowWhileMutBorrowed);
    }

    // -----------------------------------------------------------------------
    // Assign while borrowed
    // -----------------------------------------------------------------------

    #[test]
    fn assign_while_immutably_borrowed() {
        // let mut x = 42;
        // let a = &x;
        // x = 100;  // ERROR: assign while borrowed
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: true, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "a".into(), ty: None, mutable: false, secret: false, value: Expr::Borrow(Box::new(ident("x"))), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Assign {
                target: Box::new(ident("x")),
                value: Box::new(int_lit(100)),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::AssignWhileBorrowed);
    }

    #[test]
    fn assign_while_mutably_borrowed() {
        // let mut x = 42;
        // let a = &mut x;
        // x = 100;  // ERROR: assign while mutably borrowed
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: true, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "a".into(), ty: None, mutable: false, secret: false, value: Expr::BorrowMut(Box::new(ident("x"))), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Assign {
                target: Box::new(ident("x")),
                value: Box::new(int_lit(100)),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::AssignWhileBorrowed);
    }

    // -----------------------------------------------------------------------
    // Move while borrowed
    // -----------------------------------------------------------------------

    #[test]
    fn move_while_immutably_borrowed() {
        // let x = 42;
        // let a = &x;
        // let b = x;  // ERROR: cannot move while borrowed
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "a".into(), ty: None, mutable: false, secret: false, value: Expr::Borrow(Box::new(ident("x"))), ownership: Ownership::Owned },
            Stmt::Let { name: "b".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::AssignWhileBorrowed);
    }

    // -----------------------------------------------------------------------
    // Use after move in function call
    // -----------------------------------------------------------------------

    #[test]
    fn use_after_move_in_fn_call() {
        // let x = 42;
        // foo(x);   // moves x
        // let y = x; // ERROR: use after move
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Expr(Expr::FnCall {
                callee: Box::new(ident("foo")),
                args: vec![ident("x")],
            }),
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Borrow in if/else branches (each branch gets own scope)
    // -----------------------------------------------------------------------

    #[test]
    fn borrow_in_if_branch_does_not_leak() {
        // let x = 42;
        // if true { let a = &x; }
        // let b = &mut x;  // OK: borrow in if branch is released
        let if_block = Block {
            stmts: vec![Stmt::Let {
                name: "a".into(), ty: None, mutable: false, secret: false,
                value: Expr::Borrow(Box::new(ident("x"))),
                ownership: Ownership::Owned,
            }],
            span: span(),
        };
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Expr(Expr::If {
                condition: Box::new(Expr::Bool(true)),
                then_block: if_block,
                else_block: None,
            }),
            Stmt::Let { name: "b".into(), ty: None, mutable: false, secret: false, value: Expr::BorrowMut(Box::new(ident("x"))), ownership: Ownership::Owned },
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "borrow in if should not leak: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // Borrow in for loop body
    // -----------------------------------------------------------------------

    #[test]
    fn borrow_in_for_loop() {
        // let x = 42;
        // for i in arr { let a = &x; }
        // let b = &mut x;  // OK
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Expr(Expr::For {
                binding: "i".into(),
                iterator: Box::new(ident("arr")),
                body: Block {
                    stmts: vec![Stmt::Let {
                        name: "a".into(), ty: None, mutable: false, secret: false,
                        value: Expr::Borrow(Box::new(ident("x"))),
                        ownership: Ownership::Owned,
                    }],
                    span: span(),
                },
            }),
            Stmt::Let { name: "b".into(), ty: None, mutable: false, secret: false, value: Expr::BorrowMut(Box::new(ident("x"))), ownership: Ownership::Owned },
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "borrow in for loop body: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // Borrow in while loop body
    // -----------------------------------------------------------------------

    #[test]
    fn borrow_in_while_loop() {
        // let x = 42;
        // while true { let a = &x; }
        // let b = &mut x;  // OK
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Expr(Expr::While {
                condition: Box::new(Expr::Bool(true)),
                body: Block {
                    stmts: vec![Stmt::Let {
                        name: "a".into(), ty: None, mutable: false, secret: false,
                        value: Expr::Borrow(Box::new(ident("x"))),
                        ownership: Ownership::Owned,
                    }],
                    span: span(),
                },
            }),
            Stmt::Let { name: "b".into(), ty: None, mutable: false, secret: false, value: Expr::BorrowMut(Box::new(ident("x"))), ownership: Ownership::Owned },
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "borrow in while loop body: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // Match arm scoping
    // -----------------------------------------------------------------------

    #[test]
    fn borrow_in_match_arm() {
        // let x = 42;
        // match y { 1 => { let a = &x; }, _ => {} }
        // let b = &mut x;  // OK
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Match {
                subject: Box::new(int_lit(1)),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Literal(int_lit(1)),
                        guard: None,
                        body: Expr::Block(Block {
                            stmts: vec![Stmt::Let {
                                name: "a".into(), ty: None, mutable: false, secret: false,
                                value: Expr::Borrow(Box::new(ident("x"))),
                                ownership: Ownership::Owned,
                            }],
                            span: span(),
                        }),
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard,
                        guard: None,
                        body: Expr::Integer(0),
                    },
                ],
            }),
            Stmt::Let { name: "b".into(), ty: None, mutable: false, secret: false, value: Expr::BorrowMut(Box::new(ident("x"))), ownership: Ownership::Owned },
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "borrow in match arm: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // Closure captures moved variable
    // -----------------------------------------------------------------------

    #[test]
    fn closure_captures_moved_variable() {
        // let x = 42;
        // let y = x;  // moves x
        // let f = |a: i32| x + a;  // ERROR: closure captures moved variable
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::Let {
                name: "f".into(), ty: None, mutable: false, secret: false,
                value: Expr::Closure {
                    params: vec![("a".into(), None)],
                    body: Box::new(Expr::Binary {
                        op: BinOp::Add,
                        left: Box::new(ident("x")),
                        right: Box::new(ident("a")),
                    }),
                },
                ownership: Ownership::Owned,
            },
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Closure captures mutably borrowed variable
    // -----------------------------------------------------------------------

    #[test]
    fn closure_captures_mut_borrowed_variable() {
        // let mut x = 42;
        // let a = &mut x;
        // let f = || x + 1;  // ERROR: captures x which is mutably borrowed
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: true, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "a".into(), ty: None, mutable: false, secret: false, value: Expr::BorrowMut(Box::new(ident("x"))), ownership: Ownership::Owned },
            Stmt::Let {
                name: "f".into(), ty: None, mutable: false, secret: false,
                value: Expr::Closure {
                    params: vec![],
                    body: Box::new(Expr::Binary {
                        op: BinOp::Add,
                        left: Box::new(ident("x")),
                        right: Box::new(int_lit(1)),
                    }),
                },
                ownership: Ownership::Owned,
            },
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::ImmBorrowWhileMutBorrowed);
    }

    // -----------------------------------------------------------------------
    // Valid closure captures
    // -----------------------------------------------------------------------

    #[test]
    fn closure_captures_owned_variable_ok() {
        // let x = 42;
        // let f = || x + 1;  // OK: x is owned and not moved
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let {
                name: "f".into(), ty: None, mutable: false, secret: false,
                value: Expr::Closure {
                    params: vec![],
                    body: Box::new(Expr::Binary {
                        op: BinOp::Add,
                        left: Box::new(ident("x")),
                        right: Box::new(int_lit(1)),
                    }),
                },
                ownership: Ownership::Owned,
            },
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "closure captures owned var: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // Signal state treated as owned
    // -----------------------------------------------------------------------

    #[test]
    fn signal_state_is_owned() {
        let prog = program_with_stmts(vec![
            Stmt::Signal { name: "count".into(), ty: None, secret: false, atomic: false, value: int_lit(0) },
            Stmt::Expr(ident("count")), // can use it
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "signal state should be owned: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // Return expression checks borrows
    // -----------------------------------------------------------------------

    #[test]
    fn return_moved_value_error() {
        // let x = 42;
        // let y = x;  // moves x
        // return x;   // ERROR: use after move
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::Return(Some(ident("x"))),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Yield expression checks borrows
    // -----------------------------------------------------------------------

    #[test]
    fn yield_moved_value_error() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::Yield(ident("x")), // ERROR: use after move
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // LetDestructure declares bindings
    // -----------------------------------------------------------------------

    #[test]
    fn let_destructure_declares_bindings() {
        let prog = program_with_stmts(vec![
            Stmt::LetDestructure {
                pattern: Pattern::Tuple(vec![Pattern::Ident("a".into()), Pattern::Ident("b".into())]),
                ty: None,
                value: int_lit(0),
            },
            Stmt::Expr(ident("a")),
            Stmt::Expr(ident("b")),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "destructure should declare bindings: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // Struct field borrowing
    // -----------------------------------------------------------------------

    #[test]
    fn struct_init_checks_field_values() {
        // let x = 42;
        // let y = x;  // moves x
        // let s = Point { a: x };  // ERROR: use after move
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::StructInit {
                name: "Point".into(),
                fields: vec![("a".into(), ident("x"))],
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Binary expressions check both sides
    // -----------------------------------------------------------------------

    #[test]
    fn binary_expr_checks_both_operands() {
        // let x = 42;
        // let y = x;  // moves x
        // let r = x + 1;  // ERROR: use after move
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Binary {
                op: BinOp::Add,
                left: Box::new(ident("x")),
                right: Box::new(int_lit(1)),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Unary expression checks operand
    // -----------------------------------------------------------------------

    #[test]
    fn unary_expr_checks_operand() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Unary { op: UnaryOp::Neg, operand: Box::new(ident("x")) }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Method call checks object and args
    // -----------------------------------------------------------------------

    #[test]
    fn method_call_checks_object() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::MethodCall {
                object: Box::new(ident("x")),
                method: "foo".into(),
                args: vec![],
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // TryCatch scoping
    // -----------------------------------------------------------------------

    #[test]
    fn try_catch_scoping() {
        // Borrow in try body shouldn't leak to catch
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Expr(Expr::TryCatch {
                body: Box::new(Expr::Borrow(Box::new(ident("x")))),
                error_binding: "e".into(),
                catch_body: Box::new(int_lit(0)),
            }),
            Stmt::Let { name: "b".into(), ty: None, mutable: false, secret: false, value: Expr::BorrowMut(Box::new(ident("x"))), ownership: Ownership::Owned },
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "try/catch scoping: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // Impl block method borrow checking
    // -----------------------------------------------------------------------

    #[test]
    fn impl_method_borrow_checked() {
        let prog = Program {
            items: vec![Item::Impl(ImplBlock {
                target: "Foo".into(),
                trait_impls: vec![],
                methods: vec![Function {
                    name: "bar".into(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block {
                        stmts: vec![
                            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(1), ownership: Ownership::Owned },
                            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
                            Stmt::Let { name: "z".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
                        ],
                        span: span(),
                    },
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: span(),
                }],
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Component method borrow checking
    // -----------------------------------------------------------------------

    #[test]
    fn component_method_borrow_checked() {
        let prog = Program {
            items: vec![Item::Component(Component {
                name: "Widget".into(),
                type_params: vec![],
                props: vec![],
                state: vec![],
                methods: vec![Function {
                    name: "handler".into(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block {
                        stmts: vec![
                            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(1), ownership: Ownership::Owned },
                            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
                            Stmt::Let { name: "z".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
                        ],
                        span: span(),
                    },
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: span(),
                }],
                styles: vec![],
                transitions: vec![],
                trait_bounds: vec![],
                render: RenderBlock { body: TemplateNode::TextLiteral("hi".into()), span: span() },
                permissions: None,
                gestures: vec![],
                skeleton: None,
                error_boundary: None,
                chunk: None,
                on_destroy: None,
                a11y: None,
                shortcuts: vec![],
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Trait default body borrow checking
    // -----------------------------------------------------------------------

    #[test]
    fn trait_default_body_borrow_checked() {
        let prog = Program {
            items: vec![Item::Trait(TraitDef {
                name: "Foo".into(),
                type_params: vec![],
                methods: vec![TraitMethod {
                    name: "bar".into(),
                    params: vec![],
                    return_type: None,
                    default_body: Some(Block {
                        stmts: vec![
                            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(1), ownership: Ownership::Owned },
                            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
                            Stmt::Let { name: "z".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
                        ],
                        span: span(),
                    }),
                    span: span(),
                }],
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Test block borrow checking
    // -----------------------------------------------------------------------

    #[test]
    fn test_block_borrow_checked() {
        let prog = Program {
            items: vec![Item::Test(TestDef {
                name: "my_test".into(),
                body: Block {
                    stmts: vec![
                        Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(1), ownership: Ownership::Owned },
                        Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
                        Stmt::Let { name: "z".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
                    ],
                    span: span(),
                },
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Fetch expression borrow checking
    // -----------------------------------------------------------------------

    #[test]
    fn fetch_checks_url() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "url".into(), ty: None, mutable: false, secret: false, value: Expr::StringLit("https://api.example.com".into()), ownership: Ownership::Owned },
            Stmt::Let { name: "u2".into(), ty: None, mutable: false, secret: false, value: ident("url"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Fetch {
                url: Box::new(ident("url")),
                options: None,
                contract: None,
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Parallel expression checks all tasks
    // -----------------------------------------------------------------------

    #[test]
    fn parallel_checks_tasks() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Parallel {
                tasks: vec![ident("x")], // ERROR: use after move
                span: span(),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Index expression checks object and index
    // -----------------------------------------------------------------------

    #[test]
    fn index_checks_object() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "arr".into(), ty: None, mutable: false, secret: false, value: int_lit(0), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("arr"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Index {
                object: Box::new(ident("arr")),
                index: Box::new(int_lit(0)),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Multiple borrows released correctly
    // -----------------------------------------------------------------------

    #[test]
    fn three_immutable_borrows_ok() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "a".into(), ty: None, mutable: false, secret: false, value: Expr::Borrow(Box::new(ident("x"))), ownership: Ownership::Owned },
            Stmt::Let { name: "b".into(), ty: None, mutable: false, secret: false, value: Expr::Borrow(Box::new(ident("x"))), ownership: Ownership::Owned },
            Stmt::Let { name: "c".into(), ty: None, mutable: false, secret: false, value: Expr::Borrow(Box::new(ident("x"))), ownership: Ownership::Owned },
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "three immutable borrows: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // Ownership annotation on let binding
    // -----------------------------------------------------------------------

    #[test]
    fn let_with_borrowed_ownership() {
        // let x = 42;
        // let ref y = x;  // immutable borrow via ownership annotation
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Borrowed },
            Stmt::Expr(ident("x")), // x should still be usable
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "borrowed ownership: {:?}", result);
    }

    #[test]
    fn let_with_mut_borrowed_ownership() {
        // let mut x = 42;
        // let ref mut y = x;  // mutable borrow via ownership annotation
        // let z = &x;  // ERROR: cannot immutably borrow while mutably borrowed
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: true, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::MutBorrowed },
            Stmt::Let { name: "z".into(), ty: None, mutable: false, secret: false, value: Expr::Borrow(Box::new(ident("x"))), ownership: Ownership::Owned },
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::ImmBorrowWhileMutBorrowed);
    }

    // -----------------------------------------------------------------------
    // Literals and self don't trigger errors
    // -----------------------------------------------------------------------

    #[test]
    fn literals_are_fine() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Integer(1)),
            Stmt::Expr(Expr::Float(1.0)),
            Stmt::Expr(Expr::StringLit("hi".into())),
            Stmt::Expr(Expr::Bool(true)),
            Stmt::Expr(Expr::SelfExpr),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "literals should be fine: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // FormatString checks expression parts
    // -----------------------------------------------------------------------

    #[test]
    fn format_string_checks_expressions() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::FormatString {
                parts: vec![
                    FormatPart::Literal("val=".into()),
                    FormatPart::Expression(Box::new(ident("x"))), // ERROR: use after move
                ],
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Spawn checks body
    // -----------------------------------------------------------------------

    #[test]
    fn spawn_checks_body() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Spawn {
                body: Block {
                    stmts: vec![Stmt::Expr(ident("x"))], // ERROR: use after move
                    span: span(),
                },
                span: span(),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // FieldAccess checks object
    // -----------------------------------------------------------------------

    #[test]
    fn field_access_checks_object() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "p".into(), ty: None, mutable: false, secret: false, value: int_lit(0), ownership: Ownership::Owned },
            Stmt::Let { name: "q".into(), ty: None, mutable: false, secret: false, value: ident("p"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::FieldAccess {
                object: Box::new(ident("p")),
                field: "x".into(),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // &self lifetime elision (should be OK)
    // -----------------------------------------------------------------------

    #[test]
    fn self_method_lifetime_elision_ok() {
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "get".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![
                    Param {
                        name: "self".into(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: None,
                            inner: Box::new(Type::Named("Foo".into())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
},
                ],
                return_type: Some(Type::Reference {
                    mutable: false,
                    lifetime: None,
                    inner: Box::new(Type::Named("i32".into())),
                }),
                trait_bounds: vec![],
                body: Block { stmts: vec![Stmt::Return(Some(int_lit(0)))], span: span() },
                is_pub: false,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok(), "&self method lifetime elision: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // No return type reference = no elision check
    // -----------------------------------------------------------------------

    #[test]
    fn no_ref_return_no_elision_needed() {
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "add".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![
                    Param {
                        name: "a".into(),
                        ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(Type::Named("i32".into())) },
                        ownership: Ownership::Borrowed,
                        secret: false,
},
                    Param {
                        name: "b".into(),
                        ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(Type::Named("i32".into())) },
                        ownership: Ownership::Borrowed,
                        secret: false,
},
                ],
                return_type: Some(Type::Named("i32".into())), // value, not reference
                trait_bounds: vec![],
                body: Block { stmts: vec![Stmt::Return(Some(int_lit(0)))], span: span() },
                is_pub: false,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok(), "no ref return: {:?}", result);
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use crate::token::Span;

    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    fn ident(name: &str) -> Expr {
        Expr::Ident(name.to_string())
    }

    fn int_lit(v: i64) -> Expr {
        Expr::Integer(v)
    }

    fn program_with_stmts(stmts: Vec<Stmt>) -> Program {
        Program {
            items: vec![Item::Function(Function {
                name: "main".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block { stmts, span: span() },
                is_pub: true,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        }
    }

    fn program_with_fn(name: &str, params: Vec<Param>, return_type: Option<Type>, stmts: Vec<Stmt>) -> Program {
        Program {
            items: vec![Item::Function(Function {
                name: name.to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params,
                return_type,
                trait_bounds: vec![],
                body: Block { stmts, span: span() },
                is_pub: false,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        }
    }

    // -----------------------------------------------------------------------
    // BorrowError Display impl (lines 31-33)
    // -----------------------------------------------------------------------

    #[test]
    fn borrow_error_display() {
        let err = BorrowError {
            kind: BorrowErrorKind::UseAfterMove,
            span: Span::new(0, 5, 3, 7),
            message: "test error".to_string(),
            source_line: None,
            suggestion: None,
        };
        let s = format!("{}", err);
        assert!(s.contains("3:7"));
        assert!(s.contains("test error"));
        assert!(s.contains("borrow"));
    }

    #[test]
    fn borrow_error_display_with_source_line() {
        let err = BorrowError {
            kind: BorrowErrorKind::UseAfterMove,
            span: Span::new(0, 10, 5, 8),
            message: "use of moved value: `x`".to_string(),
            source_line: Some("    let y = x;".to_string()),
            suggestion: Some("consider borrowing with `&x`".to_string()),
        };
        let s = format!("{}", err);
        assert!(s.contains("5:8"), "should contain line:col");
        assert!(s.contains("let y = x;"), "should contain source line");
        assert!(s.contains("consider borrowing"), "should contain suggestion");
    }

    // -----------------------------------------------------------------------
    // lifetime_scope_depth (lines 131-141) — 'static and unknown
    // -----------------------------------------------------------------------

    #[test]
    fn lifetime_scope_depth_static() {
        let env = Env::new();
        assert_eq!(env.lifetime_scope_depth("static"), Some(0));
    }

    #[test]
    fn lifetime_scope_depth_named() {
        let mut env = Env::new();
        env.push_scope_with_lifetime("a".to_string());
        assert_eq!(env.lifetime_scope_depth("a"), Some(1));
        assert_eq!(env.lifetime_scope_depth("b"), None);
    }

    // -----------------------------------------------------------------------
    // check_item wildcard: structs, enums, use-paths, etc. (line 269)
    // -----------------------------------------------------------------------

    #[test]
    fn struct_enum_use_items_pass_through() {
        let prog = Program {
            items: vec![
                Item::Struct(StructDef {
                    name: "Foo".into(),
                    lifetimes: vec![],
                    type_params: vec![],
                    fields: vec![],
                    trait_bounds: vec![],
                    is_pub: false,
                    span: span(),
                }),
                Item::Enum(EnumDef {
                    name: "Bar".into(),
                    type_params: vec![],
                    variants: vec![],
                    is_pub: false,
                    span: span(),
                }),
                Item::Use(UsePath {
                    segments: vec!["std".into(), "io".into()],
                    alias: None,
                    glob: false,
                    group: None,
                    span: span(),
                }),
            ],
        };
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Trait default body with ownership params (lines 256-261, 285-286)
    // -----------------------------------------------------------------------

    #[test]
    fn trait_default_body_with_borrowed_params() {
        let prog = Program {
            items: vec![Item::Trait(TraitDef {
                name: "Foo".into(),
                type_params: vec![],
                methods: vec![TraitMethod {
                    name: "bar".into(),
                    params: vec![
                        Param {
                            name: "a".into(),
                            ty: Type::Reference {
                                mutable: false,
                                lifetime: None,
                                inner: Box::new(Type::Named("i32".into())),
                            },
                            ownership: Ownership::Borrowed,
                            secret: false,
},
                        Param {
                            name: "b".into(),
                            ty: Type::Named("i32".into()),
                            ownership: Ownership::MutBorrowed,
                            secret: false,
},
                    ],
                    return_type: None,
                    default_body: Some(Block {
                        stmts: vec![Stmt::Expr(ident("a"))],
                        span: span(),
                    }),
                    span: span(),
                }],
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // Trait without default body -> nothing to check
    #[test]
    fn trait_method_no_default_body() {
        let prog = Program {
            items: vec![Item::Trait(TraitDef {
                name: "Baz".into(),
                type_params: vec![],
                methods: vec![TraitMethod {
                    name: "qux".into(),
                    params: vec![],
                    return_type: None,
                    default_body: None,
                    span: span(),
                }],
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Function with borrowed param ownership (lines 283-286)
    // -----------------------------------------------------------------------

    #[test]
    fn function_with_borrowed_param() {
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "foo".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![Param {
                    name: "x".into(),
                    ty: Type::Reference {
                        mutable: false,
                        lifetime: None,
                        inner: Box::new(Type::Named("i32".into())),
                    },
                    ownership: Ownership::Borrowed,
                    secret: false,
}],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Expr(ident("x"))],
                    span: span(),
                },
                is_pub: false,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Multiple ref params with lifetimes -> ok (line 336)
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_ref_params_with_lifetime_params_ok() {
        // fn foo<'a>(a: &i32, b: &i32) -> &i32 { a }
        // Has lifetimes declared, so elision skip
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "foo".into(),
                lifetimes: vec!["a".to_string()],
                type_params: vec![],
                params: vec![
                    Param {
                        name: "a".into(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: None,
                            inner: Box::new(Type::Named("i32".into())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
},
                    Param {
                        name: "b".into(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: None,
                            inner: Box::new(Type::Named("i32".into())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
},
                ],
                return_type: Some(Type::Reference {
                    mutable: false,
                    lifetime: None,
                    inner: Box::new(Type::Named("i32".into())),
                }),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(ident("a")))],
                    span: span(),
                },
                is_pub: false,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Stmt wildcard catch-all (line 396)
    // -----------------------------------------------------------------------

    // The _ => {} catch-all only fires for hypothetical future Stmt variants
    // that we don't have. All existing Stmt variants are covered above. We
    // don't need to test it directly, but we can verify that the exhaustive
    // coverage of Stmt variants works by using all of them.

    // -----------------------------------------------------------------------
    // Borrow / BorrowMut of non-ident expressions in check_let (lines 415-416, 424-425)
    // -----------------------------------------------------------------------

    #[test]
    fn let_borrow_non_ident_expr() {
        // let a = &(1 + 2);  -- borrow of non-ident
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "a".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Borrow(Box::new(Expr::Binary {
                    op: BinOp::Add,
                    left: Box::new(int_lit(1)),
                    right: Box::new(int_lit(2)),
                })),
                ownership: Ownership::Owned,
            },
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    #[test]
    fn let_borrow_mut_non_ident_expr() {
        // let a = &mut (1 + 2);  -- borrow_mut of non-ident
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "a".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::BorrowMut(Box::new(Expr::Binary {
                    op: BinOp::Add,
                    left: Box::new(int_lit(1)),
                    right: Box::new(int_lit(2)),
                })),
                ownership: Ownership::Owned,
            },
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // FnCall with non-ident args (line 490)
    // -----------------------------------------------------------------------

    #[test]
    fn fn_call_non_ident_args() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::FnCall {
                callee: Box::new(ident("foo")),
                args: vec![int_lit(42), Expr::Binary {
                    op: BinOp::Add,
                    left: Box::new(int_lit(1)),
                    right: Box::new(int_lit(2)),
                }],
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // If with else block (lines 510-512)
    // -----------------------------------------------------------------------

    #[test]
    fn if_with_else_block() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Expr(Expr::If {
                condition: Box::new(Expr::Bool(true)),
                then_block: Block {
                    stmts: vec![Stmt::Expr(ident("x"))],
                    span: span(),
                },
                else_block: Some(Block {
                    stmts: vec![Stmt::Expr(ident("x"))],
                    span: span(),
                }),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Borrow / BorrowMut non-ident in check_expr (lines 552, 555-560)
    // -----------------------------------------------------------------------

    #[test]
    fn borrow_non_ident_in_expr() {
        // &(1 + 2)
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Borrow(Box::new(Expr::Binary {
                op: BinOp::Add,
                left: Box::new(int_lit(1)),
                right: Box::new(int_lit(2)),
            }))),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    #[test]
    fn borrow_mut_non_ident_in_expr() {
        // &mut (1 + 2)
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::BorrowMut(Box::new(Expr::Binary {
                op: BinOp::Add,
                left: Box::new(int_lit(1)),
                right: Box::new(int_lit(2)),
            }))),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    #[test]
    fn borrow_mut_of_already_mut_borrowed_ident() {
        // &mut x where x is already mut borrowed => assert_no_active_borrows fires
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: true, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "a".into(), ty: None, mutable: false, secret: false, value: Expr::BorrowMut(Box::new(ident("x"))), ownership: Ownership::Owned },
            Stmt::Expr(Expr::BorrowMut(Box::new(ident("x")))), // error: double mut borrow
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::DoubleMutBorrow);
    }

    #[test]
    fn borrow_of_already_mut_borrowed_ident() {
        // &x where x is mut borrowed
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: true, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "a".into(), ty: None, mutable: false, secret: false, value: Expr::BorrowMut(Box::new(ident("x"))), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Borrow(Box::new(ident("x")))), // error: imm borrow while mut borrowed
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::ImmBorrowWhileMutBorrowed);
    }

    // -----------------------------------------------------------------------
    // Await expression (line 587-588)
    // -----------------------------------------------------------------------

    #[test]
    fn await_checks_inner() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Await(Box::new(ident("x")))),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Fetch with options (lines 590-594)
    // -----------------------------------------------------------------------

    #[test]
    fn fetch_with_options_checks_both() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Fetch {
                url: Box::new(Expr::StringLit("https://example.com".into())),
                options: Some(Box::new(Expr::StringLit("{}".into()))),
                contract: None,
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Closure that mutates captured variable (lines 622-624)
    // -----------------------------------------------------------------------

    #[test]
    fn closure_mutates_borrowed_captured_var() {
        // let x = 42;
        // let a = &x;  -- immutable borrow of x
        // let f = || { x = 10; };  -- closure mutates x, should error
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: true, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "a".into(), ty: None, mutable: false, secret: false, value: Expr::Borrow(Box::new(ident("x"))), ownership: Ownership::Owned },
            Stmt::Let {
                name: "f".into(), ty: None, mutable: false, secret: false,
                value: Expr::Closure {
                    params: vec![],
                    body: Box::new(Expr::Assign {
                        target: Box::new(ident("x")),
                        value: Box::new(int_lit(10)),
                    }),
                },
                ownership: Ownership::Owned,
            },
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::MutBorrowWhileImmBorrowed);
    }

    // -----------------------------------------------------------------------
    // PromptTemplate (lines 638-641)
    // -----------------------------------------------------------------------

    #[test]
    fn prompt_template_checks_interpolations() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "doc".into(), ty: None, mutable: false, secret: false, value: Expr::StringLit("hello".into()), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("doc"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::PromptTemplate {
                template: "Summarize: {document}".into(),
                interpolations: vec![("document".into(), ident("doc"))],
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // Navigate (lines 643-644)
    // -----------------------------------------------------------------------

    #[test]
    fn navigate_checks_path() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Navigate {
                path: Box::new(Expr::StringLit("/home".into())),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Stream (lines 646-647)
    // -----------------------------------------------------------------------

    #[test]
    fn stream_checks_source() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "s".into(), ty: None, mutable: false, secret: false, value: int_lit(0), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("s"), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Stream { source: Box::new(ident("s")) }),
        ]);
        let result = check(&prog);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Suspend (lines 649-651)
    // -----------------------------------------------------------------------

    #[test]
    fn suspend_checks_fallback_and_body() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Suspend {
                fallback: Box::new(Expr::StringLit("loading".into())),
                body: Box::new(Expr::StringLit("done".into())),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Channel (line 656)
    // -----------------------------------------------------------------------

    #[test]
    fn channel_expr_ok() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Channel { ty: None }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Send / Receive (lines 657-662)
    // -----------------------------------------------------------------------

    #[test]
    fn send_checks_channel_and_value() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "ch".into(), ty: None, mutable: false, secret: false, value: Expr::Channel { ty: None }, ownership: Ownership::Owned },
            Stmt::Expr(Expr::Send {
                channel: Box::new(ident("ch")),
                value: Box::new(int_lit(42)),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    #[test]
    fn receive_checks_channel() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "ch".into(), ty: None, mutable: false, secret: false, value: Expr::Channel { ty: None }, ownership: Ownership::Owned },
            Stmt::Expr(Expr::Receive { channel: Box::new(ident("ch")) }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Assert / AssertEq (lines 678-683)
    // -----------------------------------------------------------------------

    #[test]
    fn assert_checks_condition() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Assert {
                condition: Box::new(Expr::Bool(true)),
                message: None,
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    #[test]
    fn assert_eq_checks_both() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::AssertEq {
                left: Box::new(int_lit(1)),
                right: Box::new(int_lit(1)),
                message: Some("should be equal".into()),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Animate (lines 685-686)
    // -----------------------------------------------------------------------

    #[test]
    fn animate_checks_target() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Animate {
                target: Box::new(Expr::StringLit("element".into())),
                animation: "fadeIn".into(),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Try expression (lines 695-696)
    // -----------------------------------------------------------------------

    #[test]
    fn try_checks_inner() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Try(Box::new(int_lit(42)))),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // DynamicImport (lines 698-699)
    // -----------------------------------------------------------------------

    #[test]
    fn dynamic_import_checks_path() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::DynamicImport {
                path: Box::new(Expr::StringLit("./module".into())),
                span: span(),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Download (lines 701-703)
    // -----------------------------------------------------------------------

    #[test]
    fn download_checks_data_and_filename() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Download {
                data: Box::new(Expr::StringLit("content".into())),
                filename: Box::new(Expr::StringLit("file.txt".into())),
                span: span(),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Env (lines 705-706)
    // -----------------------------------------------------------------------

    #[test]
    fn env_checks_name() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Env {
                name: Box::new(Expr::StringLit("API_KEY".into())),
                span: span(),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Trace (lines 708-718)
    // -----------------------------------------------------------------------

    #[test]
    fn trace_checks_label_and_body() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Trace {
                label: Box::new(Expr::StringLit("perf".into())),
                body: Block {
                    stmts: vec![
                        Stmt::Expr(int_lit(1)),
                        Stmt::Let { name: "t".into(), ty: None, mutable: false, secret: false, value: int_lit(2), ownership: Ownership::Owned },
                        Stmt::Signal { name: "s".into(), ty: None, secret: false, atomic: false, value: int_lit(3) },
                        Stmt::Yield(int_lit(4)),
                        Stmt::Return(Some(int_lit(5))),
                        Stmt::Return(None),
                    ],
                    span: span(),
                },
                span: span(),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Flag (lines 720-721)
    // -----------------------------------------------------------------------

    #[test]
    fn flag_checks_name() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Flag {
                name: Box::new(Expr::StringLit("dark_mode".into())),
                span: span(),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // VirtualList (lines 723-726)
    // -----------------------------------------------------------------------

    #[test]
    fn virtual_list_checks_all_fields() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::VirtualList {
                items: Box::new(Expr::StringLit("[]".into())),
                item_height: Box::new(int_lit(40)),
                template: Box::new(Expr::StringLit("item".into())),
                buffer: Some(5),
                span: span(),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // move_var while MutBorrowed (lines 802-808)
    // -----------------------------------------------------------------------

    #[test]
    fn move_while_mut_borrowed() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: true, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "a".into(), ty: None, mutable: false, secret: false, value: Expr::BorrowMut(Box::new(ident("x"))), ownership: Ownership::Owned },
            Stmt::Let { name: "b".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::AssignWhileBorrowed);
    }

    // -----------------------------------------------------------------------
    // declare_pattern_bindings: Variant pattern (lines 874-876)
    // -----------------------------------------------------------------------

    #[test]
    fn let_destructure_variant_pattern() {
        let prog = program_with_stmts(vec![
            Stmt::LetDestructure {
                pattern: Pattern::Variant {
                    name: "Some".into(),
                    fields: vec![Pattern::Ident("val".into())],
                },
                ty: None,
                value: int_lit(42),
            },
            Stmt::Expr(ident("val")),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // declare_pattern_bindings: Struct pattern (lines 885-887)
    // -----------------------------------------------------------------------

    #[test]
    fn let_destructure_struct_pattern() {
        let prog = program_with_stmts(vec![
            Stmt::LetDestructure {
                pattern: Pattern::Struct {
                    name: "Point".into(),
                    fields: vec![
                        ("x".into(), Pattern::Ident("px".into())),
                        ("y".into(), Pattern::Ident("py".into())),
                    ],
                    rest: false,
                },
                ty: None,
                value: int_lit(0),
            },
            Stmt::Expr(ident("px")),
            Stmt::Expr(ident("py")),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // declare_pattern_bindings: Array pattern (line 880)
    // -----------------------------------------------------------------------

    #[test]
    fn let_destructure_array_pattern() {
        let prog = program_with_stmts(vec![
            Stmt::LetDestructure {
                pattern: Pattern::Array(vec![Pattern::Ident("a".into()), Pattern::Ident("b".into())]),
                ty: None,
                value: int_lit(0),
            },
            Stmt::Expr(ident("a")),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // declare_pattern_bindings: Wildcard and Literal (line 879)
    // -----------------------------------------------------------------------

    #[test]
    fn let_destructure_wildcard_literal() {
        let prog = program_with_stmts(vec![
            Stmt::LetDestructure {
                pattern: Pattern::Wildcard,
                ty: None,
                value: int_lit(0),
            },
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // type_has_reference: Array, Option, Generic, Tuple, Function (lines 906-910)
    // -----------------------------------------------------------------------

    #[test]
    fn type_has_reference_array() {
        assert!(type_has_reference(&Type::Array(Box::new(Type::Reference {
            mutable: false,
            lifetime: None,
            inner: Box::new(Type::Named("i32".into())),
        }))));
    }

    #[test]
    fn type_has_reference_option() {
        assert!(type_has_reference(&Type::Option(Box::new(Type::Reference {
            mutable: false,
            lifetime: None,
            inner: Box::new(Type::Named("i32".into())),
        }))));
    }

    #[test]
    fn type_has_reference_generic() {
        assert!(type_has_reference(&Type::Generic {
            name: "Vec".into(),
            args: vec![Type::Reference {
                mutable: false,
                lifetime: None,
                inner: Box::new(Type::Named("i32".into())),
            }],
        }));
    }

    #[test]
    fn type_has_reference_tuple() {
        assert!(type_has_reference(&Type::Tuple(vec![
            Type::Named("i32".into()),
            Type::Reference {
                mutable: false,
                lifetime: None,
                inner: Box::new(Type::Named("i32".into())),
            },
        ])));
    }

    #[test]
    fn type_has_reference_function() {
        assert!(type_has_reference(&Type::Function {
            params: vec![Type::Reference {
                mutable: false,
                lifetime: None,
                inner: Box::new(Type::Named("i32".into())),
            }],
            ret: Box::new(Type::Named("i32".into())),
        }));
        // Also test reference in return type
        assert!(type_has_reference(&Type::Function {
            params: vec![],
            ret: Box::new(Type::Reference {
                mutable: false,
                lifetime: None,
                inner: Box::new(Type::Named("i32".into())),
            }),
        }));
    }

    #[test]
    fn type_has_reference_named_false() {
        assert!(!type_has_reference(&Type::Named("i32".into())));
    }

    // -----------------------------------------------------------------------
    // type_has_named_lifetime: Array, Option, Generic (lines 922-923)
    // -----------------------------------------------------------------------

    #[test]
    fn type_has_named_lifetime_array() {
        assert!(type_has_named_lifetime(&Type::Array(Box::new(Type::Reference {
            mutable: false,
            lifetime: Some("a".into()),
            inner: Box::new(Type::Named("i32".into())),
        }))));
    }

    #[test]
    fn type_has_named_lifetime_option() {
        assert!(type_has_named_lifetime(&Type::Option(Box::new(Type::Reference {
            mutable: false,
            lifetime: Some("a".into()),
            inner: Box::new(Type::Named("i32".into())),
        }))));
    }

    #[test]
    fn type_has_named_lifetime_generic() {
        assert!(type_has_named_lifetime(&Type::Generic {
            name: "Vec".into(),
            args: vec![Type::Reference {
                mutable: false,
                lifetime: Some("a".into()),
                inner: Box::new(Type::Named("i32".into())),
            }],
        }));
    }

    #[test]
    fn type_has_named_lifetime_inner_ref() {
        // Reference with no lifetime but inner type has named lifetime
        assert!(type_has_named_lifetime(&Type::Reference {
            mutable: false,
            lifetime: None,
            inner: Box::new(Type::Reference {
                mutable: false,
                lifetime: Some("b".into()),
                inner: Box::new(Type::Named("i32".into())),
            }),
        }));
    }

    #[test]
    fn type_has_named_lifetime_false() {
        assert!(!type_has_named_lifetime(&Type::Named("i32".into())));
    }

    // -----------------------------------------------------------------------
    // collect_captures_inner: various expression types (lines 953-1000)
    // -----------------------------------------------------------------------

    #[test]
    fn collect_captures_binary() {
        let caps = collect_captures(
            &Expr::Binary {
                op: BinOp::Add,
                left: Box::new(ident("x")),
                right: Box::new(ident("y")),
            },
            &["y".into()],
        );
        assert_eq!(caps, vec!["x".to_string()]);
    }

    #[test]
    fn collect_captures_unary() {
        let caps = collect_captures(
            &Expr::Unary { op: UnaryOp::Neg, operand: Box::new(ident("x")) },
            &[],
        );
        assert_eq!(caps, vec!["x".to_string()]);
    }

    #[test]
    fn collect_captures_fn_call() {
        let caps = collect_captures(
            &Expr::FnCall {
                callee: Box::new(ident("foo")),
                args: vec![ident("a"), ident("b")],
            },
            &["b".into()],
        );
        assert!(caps.contains(&"foo".to_string()));
        assert!(caps.contains(&"a".to_string()));
        assert!(!caps.contains(&"b".to_string()));
    }

    #[test]
    fn collect_captures_field_access() {
        let caps = collect_captures(
            &Expr::FieldAccess {
                object: Box::new(ident("obj")),
                field: "x".into(),
            },
            &[],
        );
        assert_eq!(caps, vec!["obj".to_string()]);
    }

    #[test]
    fn collect_captures_method_call() {
        let caps = collect_captures(
            &Expr::MethodCall {
                object: Box::new(ident("obj")),
                method: "foo".into(),
                args: vec![ident("a")],
            },
            &[],
        );
        assert!(caps.contains(&"obj".to_string()));
        assert!(caps.contains(&"a".to_string()));
    }

    #[test]
    fn collect_captures_if() {
        let caps = collect_captures(
            &Expr::If {
                condition: Box::new(ident("cond")),
                then_block: Block {
                    stmts: vec![Stmt::Expr(ident("a"))],
                    span: span(),
                },
                else_block: Some(Block {
                    stmts: vec![Stmt::Expr(ident("b"))],
                    span: span(),
                }),
            },
            &[],
        );
        assert!(caps.contains(&"cond".to_string()));
        assert!(caps.contains(&"a".to_string()));
        assert!(caps.contains(&"b".to_string()));
    }

    #[test]
    fn collect_captures_block() {
        let caps = collect_captures(
            &Expr::Block(Block {
                stmts: vec![Stmt::Expr(ident("x"))],
                span: span(),
            }),
            &[],
        );
        assert_eq!(caps, vec!["x".to_string()]);
    }

    #[test]
    fn collect_captures_assign() {
        let caps = collect_captures(
            &Expr::Assign {
                target: Box::new(ident("x")),
                value: Box::new(ident("y")),
            },
            &[],
        );
        assert!(caps.contains(&"x".to_string()));
        assert!(caps.contains(&"y".to_string()));
    }

    #[test]
    fn collect_captures_index() {
        let caps = collect_captures(
            &Expr::Index {
                object: Box::new(ident("arr")),
                index: Box::new(ident("i")),
            },
            &[],
        );
        assert!(caps.contains(&"arr".to_string()));
        assert!(caps.contains(&"i".to_string()));
    }

    #[test]
    fn collect_captures_borrow_borrow_mut_await_try() {
        let caps = collect_captures(&Expr::Borrow(Box::new(ident("x"))), &[]);
        assert_eq!(caps, vec!["x".to_string()]);

        let caps = collect_captures(&Expr::BorrowMut(Box::new(ident("y"))), &[]);
        assert_eq!(caps, vec!["y".to_string()]);

        let caps = collect_captures(&Expr::Await(Box::new(ident("z"))), &[]);
        assert_eq!(caps, vec!["z".to_string()]);

        let caps = collect_captures(&Expr::Try(Box::new(ident("w"))), &[]);
        assert_eq!(caps, vec!["w".to_string()]);
    }

    #[test]
    fn collect_captures_other_expr_types() {
        // Other expression types return empty captures (best-effort)
        let caps = collect_captures(&int_lit(42), &[]);
        assert!(caps.is_empty());
    }

    // -----------------------------------------------------------------------
    // body_mutates_var (lines 1004-1028)
    // -----------------------------------------------------------------------

    #[test]
    fn body_mutates_var_assign() {
        assert!(body_mutates_var(
            &Expr::Assign {
                target: Box::new(ident("x")),
                value: Box::new(int_lit(1)),
            },
            "x",
        ));
        // Assign to different var
        assert!(!body_mutates_var(
            &Expr::Assign {
                target: Box::new(ident("y")),
                value: Box::new(int_lit(1)),
            },
            "x",
        ));
    }

    #[test]
    fn body_mutates_var_assign_value_side() {
        // Mutation in the value side of an assignment
        assert!(body_mutates_var(
            &Expr::Assign {
                target: Box::new(ident("y")),
                value: Box::new(Expr::Assign {
                    target: Box::new(ident("x")),
                    value: Box::new(int_lit(1)),
                }),
            },
            "x",
        ));
    }

    #[test]
    fn body_mutates_var_binary() {
        assert!(body_mutates_var(
            &Expr::Binary {
                op: BinOp::Add,
                left: Box::new(Expr::Assign {
                    target: Box::new(ident("x")),
                    value: Box::new(int_lit(1)),
                }),
                right: Box::new(int_lit(2)),
            },
            "x",
        ));
    }

    #[test]
    fn body_mutates_var_block() {
        assert!(body_mutates_var(
            &Expr::Block(Block {
                stmts: vec![Stmt::Expr(Expr::Assign {
                    target: Box::new(ident("x")),
                    value: Box::new(int_lit(1)),
                })],
                span: span(),
            }),
            "x",
        ));
        // Non-Expr stmt doesn't match
        assert!(!body_mutates_var(
            &Expr::Block(Block {
                stmts: vec![Stmt::Return(None)],
                span: span(),
            }),
            "x",
        ));
    }

    #[test]
    fn body_mutates_var_if() {
        // In condition
        assert!(body_mutates_var(
            &Expr::If {
                condition: Box::new(Expr::Assign {
                    target: Box::new(ident("x")),
                    value: Box::new(int_lit(1)),
                }),
                then_block: Block { stmts: vec![], span: span() },
                else_block: None,
            },
            "x",
        ));
        // In then block
        assert!(body_mutates_var(
            &Expr::If {
                condition: Box::new(Expr::Bool(true)),
                then_block: Block {
                    stmts: vec![Stmt::Expr(Expr::Assign {
                        target: Box::new(ident("x")),
                        value: Box::new(int_lit(1)),
                    })],
                    span: span(),
                },
                else_block: None,
            },
            "x",
        ));
        // In else block
        assert!(body_mutates_var(
            &Expr::If {
                condition: Box::new(Expr::Bool(true)),
                then_block: Block { stmts: vec![], span: span() },
                else_block: Some(Block {
                    stmts: vec![Stmt::Expr(Expr::Assign {
                        target: Box::new(ident("x")),
                        value: Box::new(int_lit(1)),
                    })],
                    span: span(),
                }),
            },
            "x",
        ));
        // No else block, not mutated
        assert!(!body_mutates_var(
            &Expr::If {
                condition: Box::new(Expr::Bool(true)),
                then_block: Block { stmts: vec![], span: span() },
                else_block: None,
            },
            "x",
        ));
    }

    #[test]
    fn body_mutates_var_fn_call() {
        assert!(body_mutates_var(
            &Expr::FnCall {
                callee: Box::new(Expr::Assign {
                    target: Box::new(ident("x")),
                    value: Box::new(int_lit(1)),
                }),
                args: vec![],
            },
            "x",
        ));
        assert!(body_mutates_var(
            &Expr::FnCall {
                callee: Box::new(ident("foo")),
                args: vec![Expr::Assign {
                    target: Box::new(ident("x")),
                    value: Box::new(int_lit(1)),
                }],
            },
            "x",
        ));
    }

    #[test]
    fn body_mutates_var_other() {
        assert!(!body_mutates_var(&int_lit(42), "x"));
    }

    // -----------------------------------------------------------------------
    // Multiple errors accumulated
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_errors_accumulated() {
        // Two independent use-after-move errors
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "a".into(), ty: None, mutable: false, secret: false, value: int_lit(1), ownership: Ownership::Owned },
            Stmt::Let { name: "b".into(), ty: None, mutable: false, secret: false, value: ident("a"), ownership: Ownership::Owned },
            Stmt::Expr(ident("a")), // error 1
            Stmt::Let { name: "c".into(), ty: None, mutable: false, secret: false, value: int_lit(2), ownership: Ownership::Owned },
            Stmt::Let { name: "d".into(), ty: None, mutable: false, secret: false, value: ident("c"), ownership: Ownership::Owned },
            Stmt::Expr(ident("c")), // error 2
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().len(), 2);
    }

    // -----------------------------------------------------------------------
    // Match with pattern bindings in arm
    // -----------------------------------------------------------------------

    #[test]
    fn match_arm_with_ident_pattern() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Match {
                subject: Box::new(int_lit(1)),
                arms: vec![MatchArm {
                    pattern: Pattern::Ident("val".into()),
                    guard: None,
                    body: ident("val"),
                }],
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Assign to non-ident target (no borrow check on target)
    // -----------------------------------------------------------------------

    #[test]
    fn assign_to_field_access() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Assign {
                target: Box::new(Expr::FieldAccess {
                    object: Box::new(ident("obj")),
                    field: "x".into(),
                }),
                value: Box::new(int_lit(1)),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Return with None (no expr to check)
    // -----------------------------------------------------------------------

    #[test]
    fn return_none() {
        let prog = program_with_stmts(vec![
            Stmt::Return(None),
        ]);
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Store, Agent, Router, Page, Form (all go through _ => {} in check_item)
    // -----------------------------------------------------------------------

    #[test]
    fn store_item_passes_through() {
        let prog = Program {
            items: vec![Item::Store(StoreDef {
                name: "AppStore".into(),
                signals: vec![],
                actions: vec![],
                computed: vec![],
                effects: vec![],
                selectors: vec![],
                is_pub: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok());
    }

    #[test]
    fn agent_item_passes_through() {
        let prog = Program {
            items: vec![Item::Agent(AgentDef {
                name: "Helper".into(),
                system_prompt: None,
                tools: vec![],
                state: vec![],
                methods: vec![],
                render: None,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok());
    }

    #[test]
    fn router_item_passes_through() {
        let prog = Program {
            items: vec![Item::Router(RouterDef {
                name: "AppRouter".into(),
                routes: vec![],
                fallback: None,
                layout: None,
                transition: None,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok());
    }

    #[test]
    fn page_item_passes_through() {
        let prog = Program {
            items: vec![Item::Page(PageDef {
                name: "Home".into(),
                props: vec![],
                meta: None,
                state: vec![],
                methods: vec![],
                styles: vec![],
                render: RenderBlock {
                    body: TemplateNode::TextLiteral("hi".into()),
                    span: span(),
                },
                permissions: None,
                gestures: vec![],
                is_pub: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok());
    }

    #[test]
    fn form_item_passes_through() {
        let prog = Program {
            items: vec![Item::Form(FormDef {
                name: "LoginForm".into(),
                fields: vec![],
                on_submit: None,
                steps: vec![],
                methods: vec![],
                styles: vec![],
                render: None,
                is_pub: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Scope with lifetime for functions with lifetime params
    // -----------------------------------------------------------------------

    #[test]
    fn function_with_lifetime_scopes() {
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "foo".into(),
                lifetimes: vec!["a".to_string(), "b".to_string()],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Expr(int_lit(42))],
                    span: span(),
                },
                is_pub: false,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Signal in different context: signal then move
    // -----------------------------------------------------------------------

    #[test]
    fn signal_then_move() {
        let prog = program_with_stmts(vec![
            Stmt::Signal { name: "s".into(), ty: None, secret: false, atomic: false, value: int_lit(0) },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("s"), ownership: Ownership::Owned },
            Stmt::Expr(ident("s")), // ERROR: use after move
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // -----------------------------------------------------------------------
    // LetDestructure value is checked before pattern bindings
    // -----------------------------------------------------------------------

    #[test]
    fn let_destructure_checks_value() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(42), ownership: Ownership::Owned },
            Stmt::Let { name: "y".into(), ty: None, mutable: false, secret: false, value: ident("x"), ownership: Ownership::Owned },
            Stmt::LetDestructure {
                pattern: Pattern::Tuple(vec![Pattern::Ident("a".into())]),
                ty: None,
                value: ident("x"), // ERROR: use after move
            },
        ]);
        let result = check(&prog);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].kind, BorrowErrorKind::UseAfterMove);
    }

    // ── ArrayLit borrow checking ────────────────────────────────────────

    #[test]
    fn array_lit_checks_elements() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(1), ownership: Ownership::Owned },
            Stmt::Expr(Expr::ArrayLit(vec![ident("x"), ident("x")])),
        ]);
        // integers are Copy, so using x twice is fine
        let result = check(&prog);
        assert!(result.is_ok(), "Array with Copy elements should pass: {:?}", result);
    }

    #[test]
    fn array_lit_simple() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::ArrayLit(vec![int_lit(1), int_lit(2), int_lit(3)])),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "Array of literals should pass: {:?}", result);
    }

    // ── ObjectLit borrow checking ───────────────────────────────────────

    #[test]
    fn object_lit_checks_fields() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::ObjectLit {
                fields: vec![
                    ("a".into(), int_lit(1)),
                    ("b".into(), int_lit(2)),
                ],
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "Object literal with literals should pass: {:?}", result);
    }

    // ── Match guard borrow checking ─────────────────────────────────────

    #[test]
    fn match_guard_borrow_check() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::Match {
                subject: Box::new(int_lit(1)),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Ident("n".into()),
                        guard: Some(Expr::Binary {
                            op: BinOp::Gt,
                            left: Box::new(ident("n")),
                            right: Box::new(int_lit(0)),
                        }),
                        body: int_lit(10),
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard,
                        guard: None,
                        body: int_lit(0),
                    },
                ],
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "Match with guard should pass borrow check: {:?}", result);
    }

    #[test]
    fn object_lit_simple() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::ObjectLit {
                fields: vec![
                    ("a".into(), int_lit(1)),
                ],
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "Single-field ObjectLit should pass: {:?}", result);
    }

    #[test]
    fn match_no_guard_borrow_check() {
        let prog = program_with_stmts(vec![
            Stmt::Let { name: "x".into(), ty: None, mutable: false, secret: false, value: int_lit(5), ownership: Ownership::Owned },
            Stmt::Expr(Expr::Match {
                subject: Box::new(ident("x")),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Wildcard,
                        guard: None,
                        body: int_lit(0),
                    },
                ],
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "Simple match should pass: {:?}", result);
    }

    #[test]
    fn empty_array_lit() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::ArrayLit(vec![])),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "Empty array should pass: {:?}", result);
    }

    #[test]
    fn empty_object_lit() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::ObjectLit { fields: vec![] }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "Empty object should pass: {:?}", result);
    }

    // ── Fix: None/Some/Ok/Err are not moveable ────────────────────────────

    /// `None` assigned to a binding and then used again must not produce a
    /// "use of moved value" error.  `None` is a built-in constructor/constant,
    /// not a user variable, so it is exempt from move semantics.
    #[test]
    fn none_not_treated_as_moved() {
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "missing".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Ident("None".into()),
                ownership: Ownership::Owned,
            },
            // Using None again after binding — must not error.
            Stmt::Expr(Expr::Binary {
                op: crate::ast::BinOp::Eq,
                left: Box::new(Expr::Ident("missing".into())),
                right: Box::new(Expr::Ident("None".into())),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "None used after binding should not be 'moved': {:?}", result);
    }

    /// `Some(x)` constructor is not a variable; it should not be marked moved.
    #[test]
    fn some_constructor_not_treated_as_moved() {
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "a".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Ident("None".into()),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "b".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Ident("None".into()),
                ownership: Ownership::Owned,
            },
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "None can be bound multiple times: {:?}", result);
    }

    /// `Ok` and `Err` constructors must not be treated as moved variables.
    #[test]
    fn ok_err_constructors_not_moved() {
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "r1".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Ident("Ok".into()),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "r2".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Ident("Err".into()),
                ownership: Ownership::Owned,
            },
            // Both Ok and Err should still be usable as identifiers.
            Stmt::Expr(Expr::Ident("Ok".into())),
            Stmt::Expr(Expr::Ident("Err".into())),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "Ok/Err constructors should not be marked moved: {:?}", result);
    }

    #[test]
    fn range_expression_borrows() {
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "start".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Integer(0),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "end".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Integer(10),
                ownership: Ownership::Owned,
            },
            Stmt::Expr(Expr::Range {
                start: Box::new(Expr::Ident("start".into())),
                end: Box::new(Expr::Ident("end".into())),
            }),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "Range expression should pass borrow check: {:?}", result);
    }

    // -- Phase 3: Partial borrows, fn sigs, NLL, return refs, reborrowing -----

    #[test]
    fn test_paths_conflict_same() {
        assert!(Checker::paths_conflict("obj", "obj"));
    }

    #[test]
    fn test_paths_conflict_prefix() {
        assert!(Checker::paths_conflict("obj", "obj.x"));
        assert!(Checker::paths_conflict("obj.x", "obj"));
    }

    #[test]
    fn test_paths_no_conflict_siblings() {
        assert!(!Checker::paths_conflict("obj.x", "obj.y"));
    }

    #[test]
    fn test_expr_as_path_ident() {
        let checker = Checker::new();
        assert_eq!(checker.expr_as_path(&Expr::Ident("x".into())), Some("x".to_string()));
    }

    #[test]
    fn test_expr_as_path_field_access() {
        let checker = Checker::new();
        let expr = Expr::FieldAccess {
            object: Box::new(Expr::Ident("obj".into())),
            field: "x".to_string(),
        };
        assert_eq!(checker.expr_as_path(&expr), Some("obj.x".to_string()));
    }

    #[test]
    fn test_expr_as_path_nested() {
        let checker = Checker::new();
        let expr = Expr::FieldAccess {
            object: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("a".into())),
                field: "b".to_string(),
            }),
            field: "c".to_string(),
        };
        assert_eq!(checker.expr_as_path(&expr), Some("a.b.c".to_string()));
    }

    #[test]
    fn test_fn_call_borrowed_param_no_move() {
        // fn takes_ref(ref x: i32) { }
        // let a = 5; takes_ref(a); let b = a;  -- should be ok
        let prog = Program {
            items: vec![
                Item::Function(Function {
                    name: "takes_ref".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Named("i32".into()),
                        ownership: Ownership::Borrowed,
                        secret: false,
                    }],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block { stmts: vec![], span: span() },
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: span(),
                }),
                Item::Function(Function {
                    name: "main".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block {
                        stmts: vec![
                            Stmt::Let {
                                name: "a".to_string(),
                                ty: None,
                                value: int_lit(5),
                                mutable: false,
                                secret: false,
                                ownership: Ownership::Owned,
                            },
                            Stmt::Expr(Expr::FnCall {
                                callee: Box::new(ident("takes_ref")),
                                args: vec![ident("a")],
                            }),
                            Stmt::Let {
                                name: "b".to_string(),
                                ty: None,
                                value: ident("a"),
                                mutable: false,
                                secret: false,
                                ownership: Ownership::Owned,
                            },
                        ],
                        span: span(),
                    },
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: span(),
                }),
            ],
        };
        let result = check(&prog);
        assert!(result.is_ok(), "Borrowed param should not move: {:?}", result.err());
    }

    #[test]
    fn test_return_ref_to_local_error() {
        // fn bad() -> &i32 { let x = 5; return &x; }
        let prog = program_with_fn(
            "bad",
            vec![],
            Some(Type::Reference {
                lifetime: None,
                mutable: false,
                inner: Box::new(Type::Named("i32".into())),
            }),
            vec![
                Stmt::Let {
                    name: "x".to_string(),
                    ty: None,
                    value: int_lit(5),
                    mutable: false,
                    secret: false,
                    ownership: Ownership::Owned,
                },
                Stmt::Return(Some(Expr::Borrow(Box::new(ident("x"))))),
            ],
        );
        let result = check(&prog);
        assert!(result.is_err(), "Should reject returning reference to local variable");
    }

    #[test]
    fn test_return_ref_to_param_ok() {
        // fn ok(ref x: i32) -> &i32 { return &x; }
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "ok".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![Param {
                    name: "x".to_string(),
                    ty: Type::Reference {
                        lifetime: None,
                        mutable: false,
                        inner: Box::new(Type::Named("i32".into())),
                    },
                    ownership: Ownership::Borrowed,
                    secret: false,
                }],
                return_type: Some(Type::Reference {
                    lifetime: None,
                    mutable: false,
                    inner: Box::new(Type::Named("i32".into())),
                }),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Borrow(Box::new(ident("x"))))),
                    ],
                    span: span(),
                },
                is_pub: false,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok(), "Should allow returning reference to param: {:?}", result.err());
    }

    #[test]
    fn test_reborrow_immutable_from_mut_borrow() {
        // let mut x = 5;
        // let r = &mut x;
        // let s = &r;   -- immutable reborrow of mutable borrow holder, should be ok
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "x".to_string(),
                ty: None,
                value: int_lit(5),
                mutable: true,
                secret: false,
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "r".to_string(),
                ty: None,
                value: Expr::BorrowMut(Box::new(ident("x"))),
                mutable: false,
                secret: false,
                ownership: Ownership::MutBorrowed,
            },
            Stmt::Let {
                name: "s".to_string(),
                ty: None,
                value: Expr::Borrow(Box::new(ident("r"))),
                mutable: false,
                secret: false,
                ownership: Ownership::Borrowed,
            },
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "Immutable reborrow of mut borrow should be ok: {:?}", result.err());
    }

    #[test]
    fn test_nll_borrow_released_at_last_use() {
        // NLL: let r = &x; use(r); let y = x;
        // r is dead before y, so the move of x should be ok.
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "x".to_string(),
                ty: None,
                value: int_lit(42),
                mutable: false,
                secret: false,
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "r".to_string(),
                ty: None,
                value: Expr::Borrow(Box::new(ident("x"))),
                mutable: false,
                secret: false,
                ownership: Ownership::Borrowed,
            },
            // Use r in an expression
            Stmt::Expr(Expr::FnCall {
                callee: Box::new(ident("use_val")),
                args: vec![ident("r")],
            }),
            // After r is dead, moving x should be ok
            Stmt::Let {
                name: "y".to_string(),
                ty: None,
                value: ident("x"),
                mutable: false,
                secret: false,
                ownership: Ownership::Owned,
            },
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "NLL should allow move after last use of borrow: {:?}", result.err());
    }

    // =====================================================================
    // Feature 1: Tuple literal borrow checking
    // =====================================================================

    #[test]
    fn tuple_literal_borrow_check_ok() {
        let prog = program_with_stmts(vec![
            Stmt::Expr(Expr::TupleLit(vec![
                Expr::Integer(1),
                Expr::Integer(2),
                Expr::Integer(3),
            ])),
        ]);
        let result = check(&prog);
        assert!(result.is_ok(), "Tuple literal should pass borrow check: {:?}", result);
    }

    #[test]
    fn tuple_literal_with_moved_var() {
        let prog = program_with_stmts(vec![
            Stmt::Let {
                name: "x".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Integer(42),
                ownership: Ownership::Owned,
            },
            // Move x
            Stmt::Let {
                name: "y".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Ident("x".into()),
                ownership: Ownership::Owned,
            },
            // Use x in tuple — should fail
            Stmt::Expr(Expr::TupleLit(vec![
                Expr::Ident("x".into()),
                Expr::Integer(2),
            ])),
        ]);
        let result = check(&prog);
        assert!(result.is_err(), "Using moved variable in tuple should fail");
    }

    // =====================================================================
    // Feature 10: Full Lifetime Annotations
    // =====================================================================

    #[test]
    fn lifetime_consistency_valid() {
        // fn longest<'a>(a: &'a str, b: &'a str) -> &'a str
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "longest".into(),
                lifetimes: vec!["a".into()],
                type_params: vec![],
                params: vec![
                    Param {
                        name: "x".into(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: Some("a".into()),
                            inner: Box::new(Type::Named("str".into())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
                    },
                    Param {
                        name: "y".into(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: Some("a".into()),
                            inner: Box::new(Type::Named("str".into())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
                    },
                ],
                return_type: Some(Type::Reference {
                    mutable: false,
                    lifetime: Some("a".into()),
                    inner: Box::new(Type::Named("str".into())),
                }),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(Expr::Ident("x".into())))],
                    span: span(),
                },
                is_pub: true,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok(), "Valid lifetime annotations should compile: {:?}", result);
    }

    #[test]
    fn lifetime_undeclared_in_param() {
        // fn bad<'a>(x: &'b str) -> &'a str — 'b is not declared
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "bad".into(),
                lifetimes: vec!["a".into()],
                type_params: vec![],
                params: vec![
                    Param {
                        name: "x".into(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: Some("b".into()),
                            inner: Box::new(Type::Named("str".into())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
                    },
                ],
                return_type: Some(Type::Reference {
                    mutable: false,
                    lifetime: Some("a".into()),
                    inner: Box::new(Type::Named("str".into())),
                }),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(Expr::Ident("x".into())))],
                    span: span(),
                },
                is_pub: true,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_err(), "Undeclared lifetime in param should fail");
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("undeclared lifetime") && e.message.contains("'b")),
            "Error should mention undeclared lifetime 'b: {:?}", errs);
    }

    #[test]
    fn lifetime_undeclared_in_return() {
        // fn bad<'a>(x: &'a str) -> &'b str — 'b is not declared
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "bad".into(),
                lifetimes: vec!["a".into()],
                type_params: vec![],
                params: vec![
                    Param {
                        name: "x".into(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: Some("a".into()),
                            inner: Box::new(Type::Named("str".into())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
                    },
                ],
                return_type: Some(Type::Reference {
                    mutable: false,
                    lifetime: Some("b".into()),
                    inner: Box::new(Type::Named("str".into())),
                }),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(Expr::Ident("x".into())))],
                    span: span(),
                },
                is_pub: true,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_err(), "Undeclared lifetime in return should fail");
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("undeclared lifetime") && e.message.contains("'b")),
            "Error should mention undeclared lifetime 'b: {:?}", errs);
    }

    #[test]
    fn lifetime_return_not_on_any_param() {
        // fn bad<'a, 'b>(x: &'a str) -> &'b str — 'b not on any param
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "bad".into(),
                lifetimes: vec!["a".into(), "b".into()],
                type_params: vec![],
                params: vec![
                    Param {
                        name: "x".into(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: Some("a".into()),
                            inner: Box::new(Type::Named("str".into())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
                    },
                ],
                return_type: Some(Type::Reference {
                    mutable: false,
                    lifetime: Some("b".into()),
                    inner: Box::new(Type::Named("str".into())),
                }),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(Expr::Ident("x".into())))],
                    span: span(),
                },
                is_pub: true,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_err(), "Return lifetime not matching any param should fail");
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("no parameter has that lifetime")),
            "Error should mention no param has the return lifetime: {:?}", errs);
    }

    #[test]
    fn lifetime_two_params_same_lifetime_valid() {
        // fn pick<'a>(x: &'a i32, y: &'a i32) -> &'a i32
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "pick".into(),
                lifetimes: vec!["a".into()],
                type_params: vec![],
                params: vec![
                    Param {
                        name: "x".into(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: Some("a".into()),
                            inner: Box::new(Type::Named("i32".into())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
                    },
                    Param {
                        name: "y".into(),
                        ty: Type::Reference {
                            mutable: false,
                            lifetime: Some("a".into()),
                            inner: Box::new(Type::Named("i32".into())),
                        },
                        ownership: Ownership::Borrowed,
                        secret: false,
                    },
                ],
                return_type: Some(Type::Reference {
                    mutable: false,
                    lifetime: Some("a".into()),
                    inner: Box::new(Type::Named("i32".into())),
                }),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(Expr::Ident("x".into())))],
                    span: span(),
                },
                is_pub: true,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok(), "Valid matching lifetimes should compile: {:?}", result);
    }

    #[test]
    fn lifetime_no_annotations_no_error() {
        // fn simple(x: i32) -> i32 — no lifetimes at all
        let prog = Program {
            items: vec![Item::Function(Function {
                name: "simple".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![
                    Param {
                        name: "x".into(),
                        ty: Type::Named("i32".into()),
                        ownership: Ownership::Owned,
                        secret: false,
                    },
                ],
                return_type: Some(Type::Named("i32".into())),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(Expr::Ident("x".into())))],
                    span: span(),
                },
                is_pub: true,
                is_async: false,
                must_use: false,
                span: span(),
            })],
        };
        let result = check(&prog);
        assert!(result.is_ok(), "Function without lifetimes should compile: {:?}", result);
    }

    #[test]
    fn collect_lifetimes_from_type_works() {
        let mut lifetimes = Vec::new();
        let ty = Type::Reference {
            mutable: false,
            lifetime: Some("a".into()),
            inner: Box::new(Type::Named("str".into())),
        };
        collect_lifetimes_from_type(&ty, &mut lifetimes);
        assert_eq!(lifetimes, vec!["a".to_string()]);
    }

    #[test]
    fn collect_lifetimes_nested() {
        let mut lifetimes = Vec::new();
        let ty = Type::Tuple(vec![
            Type::Reference {
                mutable: false,
                lifetime: Some("a".into()),
                inner: Box::new(Type::Named("i32".into())),
            },
            Type::Reference {
                mutable: false,
                lifetime: Some("b".into()),
                inner: Box::new(Type::Named("str".into())),
            },
        ]);
        collect_lifetimes_from_type(&ty, &mut lifetimes);
        assert!(lifetimes.contains(&"a".to_string()));
        assert!(lifetimes.contains(&"b".to_string()));
    }

    #[test]
    fn collect_lifetimes_deduplication() {
        let mut lifetimes = Vec::new();
        let ty = Type::Reference {
            mutable: false,
            lifetime: Some("a".into()),
            inner: Box::new(Type::Reference {
                mutable: false,
                lifetime: Some("a".into()),
                inner: Box::new(Type::Named("i32".into())),
            }),
        };
        collect_lifetimes_from_type(&ty, &mut lifetimes);
        assert_eq!(lifetimes.len(), 1, "Should deduplicate same lifetime");
        assert_eq!(lifetimes[0], "a");
    }
}
