//! Dead code elimination pass — removes unreachable and unused code.
//!
//! This pass performs several eliminations:
//! - Removes statements after `return` in a block
//! - Removes `if false { ... }` branches (after constant folding)
//! - Removes unused local variables (assigned but never read)
//! - Removes unused private functions (not called and not exported)

use std::collections::HashSet;

use crate::ast::*;

/// Statistics about what dead code elimination accomplished.
#[derive(Debug, Default)]
pub struct DceStats {
    pub stmts_removed: usize,
    pub functions_removed: usize,
    pub unused_vars_removed: usize,
}

/// Eliminate dead code from the entire program.
pub fn eliminate_dead_code(program: &mut Program, stats: &mut DceStats) {
    // Phase 1: Remove unreachable statements within function bodies
    for item in &mut program.items {
        eliminate_in_item(item, stats);
    }

    // Phase 2: Remove unused private functions
    remove_unused_functions(program, stats);
}

fn eliminate_in_item(item: &mut Item, stats: &mut DceStats) {
    match item {
        Item::Function(f) => {
            eliminate_in_block(&mut f.body, stats);
            remove_unused_locals_in_block(&mut f.body, stats);
        }
        Item::Component(c) => {
            for method in &mut c.methods {
                eliminate_in_block(&mut method.body, stats);
                remove_unused_locals_in_block(&mut method.body, stats);
            }
        }
        Item::Impl(imp) => {
            for method in &mut imp.methods {
                eliminate_in_block(&mut method.body, stats);
                remove_unused_locals_in_block(&mut method.body, stats);
            }
        }
        Item::Store(store) => {
            for action in &mut store.actions {
                eliminate_in_block(&mut action.body, stats);
            }
            for computed in &mut store.computed {
                eliminate_in_block(&mut computed.body, stats);
            }
            for effect in &mut store.effects {
                eliminate_in_block(&mut effect.body, stats);
            }
        }
        Item::Agent(agent) => {
            for method in &mut agent.methods {
                eliminate_in_block(&mut method.body, stats);
            }
            for tool in &mut agent.tools {
                eliminate_in_block(&mut tool.body, stats);
            }
        }
        Item::Page(page) => {
            for method in &mut page.methods {
                eliminate_in_block(&mut method.body, stats);
                remove_unused_locals_in_block(&mut method.body, stats);
            }
        }
        Item::Form(form) => {
            for method in &mut form.methods {
                eliminate_in_block(&mut method.body, stats);
                remove_unused_locals_in_block(&mut method.body, stats);
            }
        }
        _ => {}
    }
}

/// Remove statements after a `return` in a block.
fn eliminate_in_block(block: &mut Block, stats: &mut DceStats) {
    // Recurse into nested blocks first
    for stmt in &mut block.stmts {
        eliminate_in_stmt(stmt, stats);
    }

    // Find first return statement and truncate after it
    if let Some(pos) = block.stmts.iter().position(|s| matches!(s, Stmt::Return(_))) {
        let removed = block.stmts.len() - (pos + 1);
        if removed > 0 {
            block.stmts.truncate(pos + 1);
            stats.stmts_removed += removed;
        }
    }

    // Remove `if false { ... }` with no else (these are dead after const folding)
    let before_len = block.stmts.len();
    block.stmts.retain(|stmt| {
        !matches!(stmt,
            Stmt::Expr(Expr::If {
                condition,
                else_block: None,
                ..
            }) if matches!(condition.as_ref(), Expr::Bool(false))
        )
    });
    let removed = before_len - block.stmts.len();
    stats.stmts_removed += removed;
}

fn eliminate_in_stmt(stmt: &mut Stmt, stats: &mut DceStats) {
    match stmt {
        Stmt::Expr(expr) => eliminate_in_expr(expr, stats),
        Stmt::Let { value, .. } => eliminate_in_expr(value, stats),
        Stmt::Signal { value, .. } => eliminate_in_expr(value, stats),
        Stmt::Return(Some(expr)) => eliminate_in_expr(expr, stats),
        Stmt::Return(None) => {}
        Stmt::Yield(expr) => eliminate_in_expr(expr, stats),
        _ => {}
    }
}

fn eliminate_in_expr(expr: &mut Expr, stats: &mut DceStats) {
    match expr {
        Expr::If { condition, then_block, else_block, .. } => {
            eliminate_in_expr(condition, stats);
            eliminate_in_block(then_block, stats);
            if let Some(eb) = else_block {
                eliminate_in_block(eb, stats);
            }
        }
        Expr::Block(block) => eliminate_in_block(block, stats),
        Expr::For { iterator, body, .. } => {
            eliminate_in_expr(iterator, stats);
            eliminate_in_block(body, stats);
        }
        Expr::While { condition, body, .. } => {
            eliminate_in_expr(condition, stats);
            eliminate_in_block(body, stats);
        }
        Expr::Closure { body, .. } => {
            eliminate_in_expr(body, stats);
        }
        _ => {}
    }
}

/// Remove unused local variables — variables assigned in `let` but never referenced.
fn remove_unused_locals_in_block(block: &mut Block, stats: &mut DceStats) {
    // Collect all variable names that are read
    let mut referenced = HashSet::new();
    for stmt in &block.stmts {
        collect_references_in_stmt(stmt, &mut referenced);
    }

    // Remove `let` bindings for variables that are never referenced,
    // but only if the value has no side effects.
    let before_len = block.stmts.len();
    block.stmts.retain(|stmt| {
        match stmt {
            Stmt::Let { name, value, .. } => {
                if referenced.contains(name.as_str()) {
                    true
                } else if is_pure(value) {
                    false // safe to remove
                } else {
                    true // keep — value may have side effects
                }
            }
            _ => true,
        }
    });
    let removed = before_len - block.stmts.len();
    stats.unused_vars_removed += removed;
    stats.stmts_removed += removed;
}

/// Collect all identifiers referenced in a statement (for usage analysis).
fn collect_references_in_stmt(stmt: &Stmt, refs: &mut HashSet<String>) {
    match stmt {
        Stmt::Let { value, .. } => collect_references_in_expr(value, refs),
        Stmt::Signal { value, .. } => collect_references_in_expr(value, refs),
        Stmt::Expr(expr) => collect_references_in_expr(expr, refs),
        Stmt::Return(Some(expr)) => collect_references_in_expr(expr, refs),
        Stmt::Return(None) => {}
        Stmt::Yield(expr) => collect_references_in_expr(expr, refs),
        _ => {}
    }
}

fn collect_references_in_expr(expr: &Expr, refs: &mut HashSet<String>) {
    match expr {
        Expr::Ident(name) => { refs.insert(name.clone()); }
        Expr::Binary { left, right, .. } => {
            collect_references_in_expr(left, refs);
            collect_references_in_expr(right, refs);
        }
        Expr::Unary { operand, .. } => collect_references_in_expr(operand, refs),
        Expr::FnCall { callee, args, .. } => {
            collect_references_in_expr(callee, refs);
            for arg in args { collect_references_in_expr(arg, refs); }
        }
        Expr::MethodCall { object, args, .. } => {
            collect_references_in_expr(object, refs);
            for arg in args { collect_references_in_expr(arg, refs); }
        }
        Expr::FieldAccess { object, .. } => collect_references_in_expr(object, refs),
        Expr::Index { object, index, .. } => {
            collect_references_in_expr(object, refs);
            collect_references_in_expr(index, refs);
        }
        Expr::If { condition, then_block, else_block, .. } => {
            collect_references_in_expr(condition, refs);
            for s in &then_block.stmts { collect_references_in_stmt(s, refs); }
            if let Some(eb) = else_block {
                for s in &eb.stmts { collect_references_in_stmt(s, refs); }
            }
        }
        Expr::Block(block) => {
            for s in &block.stmts { collect_references_in_stmt(s, refs); }
        }
        Expr::For { iterator, body, .. } => {
            collect_references_in_expr(iterator, refs);
            for s in &body.stmts { collect_references_in_stmt(s, refs); }
        }
        Expr::While { condition, body, .. } => {
            collect_references_in_expr(condition, refs);
            for s in &body.stmts { collect_references_in_stmt(s, refs); }
        }
        Expr::Assign { target, value, .. } => {
            collect_references_in_expr(target, refs);
            collect_references_in_expr(value, refs);
        }
        Expr::Closure { body, .. } => collect_references_in_expr(body, refs),
        Expr::StructInit { fields, .. } => {
            for (_, v) in fields { collect_references_in_expr(v, refs); }
        }
        Expr::Match { subject, arms, .. } => {
            collect_references_in_expr(subject, refs);
            for arm in arms { collect_references_in_expr(&arm.body, refs); }
        }
        Expr::Borrow(e) | Expr::BorrowMut(e) | Expr::Await(e)
        | Expr::Stream { source: e } | Expr::Navigate { path: e }
        | Expr::Receive { channel: e } => {
            collect_references_in_expr(e, refs);
        }
        Expr::Spawn { body, .. } => {
            for s in &body.stmts { collect_references_in_stmt(s, refs); }
        }
        Expr::Send { channel, value } => {
            collect_references_in_expr(channel, refs);
            collect_references_in_expr(value, refs);
        }
        Expr::Suspend { fallback, body } => {
            collect_references_in_expr(fallback, refs);
            collect_references_in_expr(body, refs);
        }
        Expr::TryCatch { body, catch_body, .. } => {
            collect_references_in_expr(body, refs);
            collect_references_in_expr(catch_body, refs);
        }
        Expr::Fetch { url, options, .. } => {
            collect_references_in_expr(url, refs);
            if let Some(opts) = options { collect_references_in_expr(opts, refs); }
        }
        Expr::Parallel { tasks, .. } => {
            for e in tasks { collect_references_in_expr(e, refs); }
        }
        Expr::PromptTemplate { interpolations, .. } => {
            for (_, e) in interpolations { collect_references_in_expr(e, refs); }
        }
        Expr::Env { name, .. } => {
            collect_references_in_expr(name, refs);
        }
        Expr::Trace { label, body, .. } => {
            collect_references_in_expr(label, refs);
            for s in &body.stmts { collect_references_in_stmt(s, refs); }
        }
        Expr::Flag { name, .. } => {
            collect_references_in_expr(name, refs);
        }
        _ => {}
    }
}

/// Check if an expression is pure (has no side effects).
fn is_pure(expr: &Expr) -> bool {
    match expr {
        Expr::Integer(_) | Expr::Float(_) | Expr::StringLit(_)
        | Expr::Bool(_) | Expr::Ident(_) | Expr::SelfExpr => true,
        Expr::Binary { left, right, .. } => is_pure(left) && is_pure(right),
        Expr::Unary { operand, .. } => is_pure(operand),
        Expr::StructInit { fields, .. } => fields.iter().all(|(_, v)| is_pure(v)),
        Expr::FieldAccess { object, .. } => is_pure(object),
        Expr::Borrow(e) | Expr::BorrowMut(e) => is_pure(e),
        // Function calls, method calls, await, fetch, etc. are impure
        _ => false,
    }
}

/// Remove unused private functions that are never called from anywhere.
fn remove_unused_functions(program: &mut Program, stats: &mut DceStats) {
    // Collect all function names that are referenced
    let mut called_fns = HashSet::new();
    for item in &program.items {
        collect_called_functions_in_item(item, &mut called_fns);
    }

    let before_len = program.items.len();
    program.items.retain(|item| {
        match item {
            Item::Function(f) => {
                if f.is_pub {
                    true // keep public functions
                } else {
                    called_fns.contains(f.name.as_str())
                }
            }
            _ => true,
        }
    });
    let removed = before_len - program.items.len();
    stats.functions_removed += removed;
}

fn collect_called_functions_in_item(item: &Item, called: &mut HashSet<String>) {
    match item {
        Item::Function(f) => {
            for s in &f.body.stmts { collect_references_in_stmt(s, called); }
        }
        Item::Component(c) => {
            for method in &c.methods {
                for s in &method.body.stmts { collect_references_in_stmt(s, called); }
            }
            // Mark component name as used
            called.insert(c.name.clone());
        }
        Item::Impl(imp) => {
            for method in &imp.methods {
                for s in &method.body.stmts { collect_references_in_stmt(s, called); }
            }
        }
        Item::Store(store) => {
            for action in &store.actions {
                for s in &action.body.stmts { collect_references_in_stmt(s, called); }
            }
            for computed in &store.computed {
                for s in &computed.body.stmts { collect_references_in_stmt(s, called); }
            }
            for effect in &store.effects {
                for s in &effect.body.stmts { collect_references_in_stmt(s, called); }
            }
        }
        Item::Agent(agent) => {
            for method in &agent.methods {
                for s in &method.body.stmts { collect_references_in_stmt(s, called); }
            }
            for tool in &agent.tools {
                for s in &tool.body.stmts { collect_references_in_stmt(s, called); }
            }
        }
        Item::Router(router) => {
            for route in &router.routes {
                called.insert(route.component.clone());
                if let Some(ref guard) = route.guard {
                    collect_references_in_expr(guard, called);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Span;

    fn dummy_span() -> Span {
        Span { start: 0, end: 0, line: 0, col: 0 }
    }

    fn make_fn(name: &str, is_pub: bool, stmts: Vec<Stmt>) -> Item {
        Item::Function(Function {
            name: name.to_string(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: Block { stmts, span: dummy_span() },
            is_pub,
            must_use: false,
            span: dummy_span(),
        })
    }

    #[test]
    fn test_remove_after_return() {
        let stmts = vec![
            Stmt::Return(Some(Expr::Integer(42))),
            Stmt::Expr(Expr::Integer(99)),   // dead
            Stmt::Expr(Expr::Integer(100)),  // dead
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);

        match &program.items[0] {
            Item::Function(f) => {
                assert_eq!(f.body.stmts.len(), 1);
                assert_eq!(f.body.stmts[0], Stmt::Return(Some(Expr::Integer(42))));
            }
            _ => panic!("expected function"),
        }
        assert_eq!(stats.stmts_removed, 2);
    }

    #[test]
    fn test_remove_if_false() {
        let stmts = vec![
            Stmt::Expr(Expr::If {
                condition: Box::new(Expr::Bool(false)),
                then_block: Block {
                    stmts: vec![Stmt::Expr(Expr::Integer(42))],
                    span: dummy_span(),
                },
                else_block: None,
            }),
            Stmt::Expr(Expr::Integer(10)),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);

        match &program.items[0] {
            Item::Function(f) => {
                // The `if false` should be removed, leaving only the 10
                assert_eq!(f.body.stmts.len(), 1);
                assert_eq!(f.body.stmts[0], Stmt::Expr(Expr::Integer(10)));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_remove_unused_private_function() {
        let items = vec![
            make_fn("main", true, vec![
                Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("helper".to_string())),
                    args: vec![],
                }),
            ]),
            make_fn("helper", false, vec![Stmt::Return(Some(Expr::Integer(1)))]),
            make_fn("unused", false, vec![Stmt::Return(Some(Expr::Integer(2)))]),
        ];
        let mut program = Program { items };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);

        // `unused` should be removed, `helper` and `main` kept
        assert_eq!(program.items.len(), 2);
        let names: Vec<_> = program.items.iter().map(|item| {
            match item {
                Item::Function(f) => f.name.as_str(),
                _ => "",
            }
        }).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"helper"));
        assert!(!names.contains(&"unused"));
        assert_eq!(stats.functions_removed, 1);
    }

    #[test]
    fn test_remove_unused_local_variable() {
        let stmts = vec![
            Stmt::Let {
                name: "unused_var".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Integer(42),
                ownership: Ownership::Owned,
            },
            Stmt::Expr(Expr::Integer(10)),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);

        match &program.items[0] {
            Item::Function(f) => {
                assert_eq!(f.body.stmts.len(), 1);
                assert_eq!(f.body.stmts[0], Stmt::Expr(Expr::Integer(10)));
            }
            _ => panic!("expected function"),
        }
        assert_eq!(stats.unused_vars_removed, 1);
    }

    #[test]
    fn test_keep_used_local_variable() {
        let stmts = vec![
            Stmt::Let {
                name: "x".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Integer(42),
                ownership: Ownership::Owned,
            },
            Stmt::Expr(Expr::Ident("x".to_string())),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);

        match &program.items[0] {
            Item::Function(f) => {
                assert_eq!(f.body.stmts.len(), 2); // both kept
            }
            _ => panic!("expected function"),
        }
        assert_eq!(stats.unused_vars_removed, 0);
    }

    // --- Dead code after return in nested blocks ---

    #[test]
    fn test_remove_after_return_in_nested_if() {
        let stmts = vec![
            Stmt::Expr(Expr::If {
                condition: Box::new(Expr::Bool(true)),
                then_block: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(1))),
                        Stmt::Expr(Expr::Integer(99)), // dead
                    ],
                    span: dummy_span(),
                },
                else_block: None,
            }),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 1);
    }

    #[test]
    fn test_remove_after_return_in_else_block() {
        let stmts = vec![
            Stmt::Expr(Expr::If {
                condition: Box::new(Expr::Ident("x".to_string())),
                then_block: Block {
                    stmts: vec![Stmt::Expr(Expr::Integer(1))],
                    span: dummy_span(),
                },
                else_block: Some(Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(2))),
                        Stmt::Expr(Expr::Integer(99)), // dead
                    ],
                    span: dummy_span(),
                }),
            }),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 1);
    }

    #[test]
    fn test_dead_code_in_for_loop() {
        let stmts = vec![
            Stmt::Expr(Expr::For {
                binding: "i".to_string(),
                iterator: Box::new(Expr::Ident("items".to_string())),
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(1))),
                        Stmt::Expr(Expr::Integer(99)), // dead
                    ],
                    span: dummy_span(),
                },
            }),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 1);
    }

    #[test]
    fn test_dead_code_in_while_loop() {
        let stmts = vec![
            Stmt::Expr(Expr::While {
                condition: Box::new(Expr::Bool(true)),
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(1))),
                        Stmt::Expr(Expr::Integer(99)), // dead
                    ],
                    span: dummy_span(),
                },
            }),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 1);
    }

    #[test]
    fn test_dead_code_in_closure() {
        let stmts = vec![
            Stmt::Expr(Expr::Closure {
                params: vec![],
                body: Box::new(Expr::Block(Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(1))),
                        Stmt::Expr(Expr::Integer(99)), // dead
                    ],
                    span: dummy_span(),
                })),
            }),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 1);
    }

    // --- If false with else not removed (has else) ---

    #[test]
    fn test_if_false_with_else_not_removed() {
        // `if false { ... } else { ... }` should NOT be removed (it has an else)
        let stmts = vec![
            Stmt::Expr(Expr::If {
                condition: Box::new(Expr::Bool(false)),
                then_block: Block {
                    stmts: vec![Stmt::Expr(Expr::Integer(1))],
                    span: dummy_span(),
                },
                else_block: Some(Block {
                    stmts: vec![Stmt::Expr(Expr::Integer(2))],
                    span: dummy_span(),
                }),
            }),
            Stmt::Expr(Expr::Integer(10)),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        // Should still have 2 statements (if false + else is kept, plus the 10)
        match &program.items[0] {
            Item::Function(f) => assert_eq!(f.body.stmts.len(), 2),
            _ => panic!("expected function"),
        }
    }

    // --- Multiple unused local variables ---

    #[test]
    fn test_remove_multiple_unused_locals() {
        let stmts = vec![
            Stmt::Let {
                name: "a".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Integer(1),
                ownership: Ownership::Owned,
            },
            Stmt::Let {
                name: "b".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Integer(2),
                ownership: Ownership::Owned,
            },
            Stmt::Expr(Expr::Integer(10)),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.unused_vars_removed, 2);
        match &program.items[0] {
            Item::Function(f) => assert_eq!(f.body.stmts.len(), 1),
            _ => panic!("expected function"),
        }
    }

    // --- Keep impure unused vars ---

    #[test]
    fn test_keep_impure_unused_var() {
        let stmts = vec![
            Stmt::Let {
                name: "result".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::FnCall {
                    callee: Box::new(Expr::Ident("side_effect".to_string())),
                    args: vec![],
                },
                ownership: Ownership::Owned,
            },
            Stmt::Expr(Expr::Integer(10)),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        // Should NOT be removed because value is a function call (side effect)
        assert_eq!(stats.unused_vars_removed, 0);
    }

    // --- Unused private functions (multiple) ---

    #[test]
    fn test_remove_multiple_unused_private_functions() {
        let items = vec![
            make_fn("main", true, vec![Stmt::Return(Some(Expr::Integer(0)))]),
            make_fn("unused1", false, vec![Stmt::Return(Some(Expr::Integer(1)))]),
            make_fn("unused2", false, vec![Stmt::Return(Some(Expr::Integer(2)))]),
            make_fn("unused3", false, vec![Stmt::Return(Some(Expr::Integer(3)))]),
        ];
        let mut program = Program { items };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.functions_removed, 3);
        assert_eq!(program.items.len(), 1);
    }

    // --- Pub functions are never removed ---

    #[test]
    fn test_keep_all_pub_functions() {
        let items = vec![
            make_fn("a", true, vec![Stmt::Return(Some(Expr::Integer(1)))]),
            make_fn("b", true, vec![Stmt::Return(Some(Expr::Integer(2)))]),
        ];
        let mut program = Program { items };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.functions_removed, 0);
        assert_eq!(program.items.len(), 2);
    }

    // --- No return in block ---

    #[test]
    fn test_no_dead_code_without_return() {
        let stmts = vec![
            Stmt::Expr(Expr::Integer(1)),
            Stmt::Expr(Expr::Integer(2)),
            Stmt::Expr(Expr::Integer(3)),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        match &program.items[0] {
            Item::Function(f) => assert_eq!(f.body.stmts.len(), 3),
            _ => panic!("expected function"),
        }
        assert_eq!(stats.stmts_removed, 0);
    }

    // --- DCE inside component/impl/store/agent/page/form ---

    #[test]
    fn test_dce_inside_component() {
        let items = vec![Item::Component(Component {
            name: "Test".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![Function {
                name: "m".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(1))),
                        Stmt::Expr(Expr::Integer(99)), // dead
                    ],
                    span: dummy_span(),
                },
                is_pub: false,
                must_use: false,
                span: dummy_span(),
            }],
            styles: vec![],
            transitions: vec![],
            trait_bounds: vec![],
            render: RenderBlock {
                body: TemplateNode::Fragment(vec![]),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: dummy_span(),
        })];
        let mut program = Program { items };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 1);
    }

    #[test]
    fn test_dce_inside_impl() {
        let items = vec![Item::Impl(ImplBlock {
            target: "Foo".to_string(),
            trait_impls: vec![],
            methods: vec![Function {
                name: "m".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(1))),
                        Stmt::Expr(Expr::Integer(99)),
                    ],
                    span: dummy_span(),
                },
                is_pub: false,
                must_use: false,
                span: dummy_span(),
            }],
            span: dummy_span(),
        })];
        let mut program = Program { items };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 1);
    }

    // --- DCE inside Store (actions, computed, effects) ---

    fn make_method(name: &str, stmts: Vec<Stmt>) -> Function {
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

    #[test]
    fn test_dce_inside_store() {
        let items = vec![Item::Store(StoreDef {
            name: "S".to_string(),
            signals: vec![],
            actions: vec![ActionDef {
                name: "inc".to_string(), params: vec![],
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(1))),
                        Stmt::Expr(Expr::Integer(99)), // dead
                    ],
                    span: dummy_span(),
                },
                is_async: false, span: dummy_span(),
            }],
            computed: vec![ComputedDef {
                name: "dbl".to_string(), return_type: None,
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(2))),
                        Stmt::Expr(Expr::Integer(99)), // dead
                    ],
                    span: dummy_span(),
                },
                span: dummy_span(),
            }],
            effects: vec![EffectDef {
                name: "log".to_string(),
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(3))),
                        Stmt::Expr(Expr::Integer(99)), // dead
                    ],
                    span: dummy_span(),
                },
                span: dummy_span(),
            }],
            selectors: vec![],
            is_pub: false, span: dummy_span(),
        })];
        let mut program = Program { items };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 3);
    }

    // --- DCE inside Agent (methods, tools) ---

    #[test]
    fn test_dce_inside_agent() {
        let items = vec![Item::Agent(AgentDef {
            name: "Bot".to_string(),
            system_prompt: None,
            tools: vec![ToolDef {
                name: "search".to_string(), description: None, params: vec![],
                return_type: None,
                body: Block {
                    stmts: vec![
                        Stmt::Return(Some(Expr::Integer(1))),
                        Stmt::Expr(Expr::Integer(99)), // dead
                    ],
                    span: dummy_span(),
                },
                span: dummy_span(),
            }],
            state: vec![],
            methods: vec![make_method("go", vec![
                Stmt::Return(Some(Expr::Integer(2))),
                Stmt::Expr(Expr::Integer(99)), // dead
            ])],
            render: None, span: dummy_span(),
        })];
        let mut program = Program { items };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 2);
    }

    // --- DCE inside Page ---

    #[test]
    fn test_dce_inside_page() {
        let items = vec![Item::Page(PageDef {
            name: "Home".to_string(), props: vec![], meta: None, state: vec![],
            methods: vec![make_method("init", vec![
                Stmt::Return(Some(Expr::Integer(1))),
                Stmt::Expr(Expr::Integer(99)), // dead
            ])],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Fragment(vec![]),
                span: dummy_span(),
            },
            permissions: None, gestures: vec![],
            is_pub: false, span: dummy_span(),
        })];
        let mut program = Program { items };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 1);
    }

    // --- DCE inside Form ---

    #[test]
    fn test_dce_inside_form() {
        let items = vec![Item::Form(FormDef {
            name: "F".to_string(), fields: vec![], on_submit: None, steps: vec![],
            methods: vec![make_method("submit", vec![
                Stmt::Return(Some(Expr::Integer(1))),
                Stmt::Expr(Expr::Integer(99)), // dead
            ])],
            styles: vec![], render: None, is_pub: false, span: dummy_span(),
        })];
        let mut program = Program { items };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 1);
    }

    // --- collect_references_in_expr coverage ---

    #[test]
    fn test_collect_refs_binary() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::Binary {
            op: BinOp::Add,
            left: Box::new(Expr::Ident("a".to_string())),
            right: Box::new(Expr::Ident("b".to_string())),
        }, &mut refs);
        assert!(refs.contains("a"));
        assert!(refs.contains("b"));
    }

    #[test]
    fn test_collect_refs_unary() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::Unary {
            op: UnaryOp::Neg,
            operand: Box::new(Expr::Ident("x".to_string())),
        }, &mut refs);
        assert!(refs.contains("x"));
    }

    #[test]
    fn test_collect_refs_method_call() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::MethodCall {
            object: Box::new(Expr::Ident("obj".to_string())),
            method: "m".to_string(),
            args: vec![Expr::Ident("arg".to_string())],
        }, &mut refs);
        assert!(refs.contains("obj"));
        assert!(refs.contains("arg"));
    }

    #[test]
    fn test_collect_refs_field_access() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::FieldAccess {
            object: Box::new(Expr::Ident("x".to_string())),
            field: "f".to_string(),
        }, &mut refs);
        assert!(refs.contains("x"));
    }

    #[test]
    fn test_collect_refs_index() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::Index {
            object: Box::new(Expr::Ident("arr".to_string())),
            index: Box::new(Expr::Ident("i".to_string())),
        }, &mut refs);
        assert!(refs.contains("arr"));
        assert!(refs.contains("i"));
    }

    #[test]
    fn test_collect_refs_assign() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::Assign {
            target: Box::new(Expr::Ident("t".to_string())),
            value: Box::new(Expr::Ident("v".to_string())),
        }, &mut refs);
        assert!(refs.contains("t"));
        assert!(refs.contains("v"));
    }

    #[test]
    fn test_collect_refs_closure() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::Closure {
            params: vec![],
            body: Box::new(Expr::Ident("body_dep".to_string())),
        }, &mut refs);
        assert!(refs.contains("body_dep"));
    }

    #[test]
    fn test_collect_refs_struct_init() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::StructInit {
            name: "S".to_string(),
            fields: vec![("x".to_string(), Expr::Ident("val".to_string()))],
        }, &mut refs);
        assert!(refs.contains("val"));
    }

    #[test]
    fn test_collect_refs_match() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::Match {
            subject: Box::new(Expr::Ident("subj".to_string())),
            arms: vec![MatchArm {
                pattern: Pattern::Wildcard,
                body: Expr::Ident("arm_body".to_string()),
            }],
        }, &mut refs);
        assert!(refs.contains("subj"));
        assert!(refs.contains("arm_body"));
    }

    #[test]
    fn test_collect_refs_borrow_await_etc() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::Borrow(Box::new(Expr::Ident("a".to_string()))), &mut refs);
        collect_references_in_expr(&Expr::BorrowMut(Box::new(Expr::Ident("b".to_string()))), &mut refs);
        collect_references_in_expr(&Expr::Await(Box::new(Expr::Ident("c".to_string()))), &mut refs);
        collect_references_in_expr(&Expr::Stream { source: Box::new(Expr::Ident("d".to_string())) }, &mut refs);
        collect_references_in_expr(&Expr::Navigate { path: Box::new(Expr::Ident("e".to_string())) }, &mut refs);
        collect_references_in_expr(&Expr::Receive { channel: Box::new(Expr::Ident("f".to_string())) }, &mut refs);
        assert!(refs.contains("a") && refs.contains("b") && refs.contains("c"));
        assert!(refs.contains("d") && refs.contains("e") && refs.contains("f"));
    }

    #[test]
    fn test_collect_refs_spawn_send_suspend() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::Spawn {
            body: Block { stmts: vec![Stmt::Expr(Expr::Ident("sp".to_string()))], span: dummy_span() },
            span: dummy_span(),
        }, &mut refs);
        collect_references_in_expr(&Expr::Send {
            channel: Box::new(Expr::Ident("ch".to_string())),
            value: Box::new(Expr::Ident("v".to_string())),
        }, &mut refs);
        collect_references_in_expr(&Expr::Suspend {
            fallback: Box::new(Expr::Ident("fb".to_string())),
            body: Box::new(Expr::Ident("bd".to_string())),
        }, &mut refs);
        assert!(refs.contains("sp") && refs.contains("ch") && refs.contains("v"));
        assert!(refs.contains("fb") && refs.contains("bd"));
    }

    #[test]
    fn test_collect_refs_try_catch_fetch_parallel() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::TryCatch {
            body: Box::new(Expr::Ident("tc".to_string())),
            error_binding: "e".to_string(),
            catch_body: Box::new(Expr::Ident("ca".to_string())),
        }, &mut refs);
        collect_references_in_expr(&Expr::Fetch {
            url: Box::new(Expr::Ident("u".to_string())),
            options: Some(Box::new(Expr::Ident("o".to_string()))),
            contract: None,
        }, &mut refs);
        collect_references_in_expr(&Expr::Parallel {
            tasks: vec![Expr::Ident("t1".to_string())],
            span: dummy_span(),
        }, &mut refs);
        assert!(refs.contains("tc") && refs.contains("ca"));
        assert!(refs.contains("u") && refs.contains("o"));
        assert!(refs.contains("t1"));
    }

    #[test]
    fn test_collect_refs_prompt_env_trace_flag() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_expr(&Expr::PromptTemplate {
            template: "hi".to_string(),
            interpolations: vec![("x".to_string(), Expr::Ident("pt".to_string()))],
        }, &mut refs);
        collect_references_in_expr(&Expr::Env {
            name: Box::new(Expr::Ident("en".to_string())),
            span: dummy_span(),
        }, &mut refs);
        collect_references_in_expr(&Expr::Trace {
            label: Box::new(Expr::Ident("lb".to_string())),
            body: Block { stmts: vec![Stmt::Expr(Expr::Ident("tb".to_string()))], span: dummy_span() },
            span: dummy_span(),
        }, &mut refs);
        collect_references_in_expr(&Expr::Flag {
            name: Box::new(Expr::Ident("fl".to_string())),
            span: dummy_span(),
        }, &mut refs);
        assert!(refs.contains("pt") && refs.contains("en"));
        assert!(refs.contains("lb") && refs.contains("tb"));
        assert!(refs.contains("fl"));
    }

    // --- is_pure coverage ---

    #[test]
    fn test_is_pure_binary() {
        assert!(is_pure(&Expr::Binary {
            op: BinOp::Add,
            left: Box::new(Expr::Integer(1)),
            right: Box::new(Expr::Integer(2)),
        }));
    }

    #[test]
    fn test_is_pure_unary() {
        assert!(is_pure(&Expr::Unary {
            op: UnaryOp::Neg,
            operand: Box::new(Expr::Integer(1)),
        }));
    }

    #[test]
    fn test_is_pure_struct_init() {
        assert!(is_pure(&Expr::StructInit {
            name: "S".to_string(),
            fields: vec![("x".to_string(), Expr::Integer(1))],
        }));
    }

    #[test]
    fn test_is_pure_field_access() {
        assert!(is_pure(&Expr::FieldAccess {
            object: Box::new(Expr::Ident("x".to_string())),
            field: "f".to_string(),
        }));
    }

    #[test]
    fn test_is_pure_borrow() {
        assert!(is_pure(&Expr::Borrow(Box::new(Expr::Ident("x".to_string())))));
        assert!(is_pure(&Expr::BorrowMut(Box::new(Expr::Ident("x".to_string())))));
    }

    #[test]
    fn test_is_pure_self_expr() {
        assert!(is_pure(&Expr::SelfExpr));
    }

    #[test]
    fn test_is_not_pure_fn_call() {
        assert!(!is_pure(&Expr::FnCall {
            callee: Box::new(Expr::Ident("f".to_string())),
            args: vec![],
        }));
    }

    // --- collect_called_functions_in_item coverage ---

    #[test]
    fn test_called_fns_in_store() {
        let mut called = std::collections::HashSet::new();
        let item = Item::Store(StoreDef {
            name: "S".to_string(), signals: vec![],
            actions: vec![ActionDef {
                name: "a".to_string(), params: vec![],
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("dep1".to_string()))], span: dummy_span() },
                is_async: false, span: dummy_span(),
            }],
            computed: vec![ComputedDef {
                name: "c".to_string(), return_type: None,
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("dep2".to_string()))], span: dummy_span() },
                span: dummy_span(),
            }],
            effects: vec![EffectDef {
                name: "e".to_string(),
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("dep3".to_string()))], span: dummy_span() },
                span: dummy_span(),
            }],
            selectors: vec![], is_pub: false, span: dummy_span(),
        });
        collect_called_functions_in_item(&item, &mut called);
        assert!(called.contains("dep1"));
        assert!(called.contains("dep2"));
        assert!(called.contains("dep3"));
    }

    #[test]
    fn test_called_fns_in_agent() {
        let mut called = std::collections::HashSet::new();
        let item = Item::Agent(AgentDef {
            name: "A".to_string(), system_prompt: None,
            tools: vec![ToolDef {
                name: "t".to_string(), description: None, params: vec![],
                return_type: None,
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("tool_dep".to_string()))], span: dummy_span() },
                span: dummy_span(),
            }],
            state: vec![],
            methods: vec![Function {
                name: "m".to_string(), lifetimes: vec![], type_params: vec![],
                params: vec![], return_type: None, trait_bounds: vec![],
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("meth_dep".to_string()))], span: dummy_span() },
                is_pub: false, must_use: false, span: dummy_span(),
            }],
            render: None, span: dummy_span(),
        });
        collect_called_functions_in_item(&item, &mut called);
        assert!(called.contains("tool_dep"));
        assert!(called.contains("meth_dep"));
    }

    #[test]
    fn test_called_fns_in_router() {
        let mut called = std::collections::HashSet::new();
        let item = Item::Router(RouterDef {
            name: "R".to_string(),
            routes: vec![RouteDef {
                path: "/".to_string(), params: vec![],
                component: "Home".to_string(),
                guard: Some(Expr::Ident("guard_fn".to_string())),
                transition: None,
                span: dummy_span(),
            }],
            fallback: None, layout: None, transition: None, span: dummy_span(),
        });
        collect_called_functions_in_item(&item, &mut called);
        assert!(called.contains("Home"));
        assert!(called.contains("guard_fn"));
    }

    // --- eliminate_in_stmt for Signal and Yield ---

    #[test]
    fn test_dce_signal_stmt() {
        let stmts = vec![
            Stmt::Signal {
                name: "s".to_string(), ty: None, secret: false, atomic: false,
                value: Expr::If {
                    condition: Box::new(Expr::Bool(true)),
                    then_block: Block {
                        stmts: vec![
                            Stmt::Return(Some(Expr::Integer(1))),
                            Stmt::Expr(Expr::Integer(99)), // dead
                        ],
                        span: dummy_span(),
                    },
                    else_block: None,
                },
            },
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 1);
    }

    #[test]
    fn test_dce_yield_stmt() {
        let stmts = vec![
            Stmt::Yield(Expr::Block(Block {
                stmts: vec![
                    Stmt::Return(Some(Expr::Integer(1))),
                    Stmt::Expr(Expr::Integer(99)), // dead
                ],
                span: dummy_span(),
            })),
        ];
        let mut program = Program {
            items: vec![make_fn("test", true, stmts)],
        };
        let mut stats = DceStats::default();
        eliminate_dead_code(&mut program, &mut stats);
        assert_eq!(stats.stmts_removed, 1);
    }

    // --- collect_references_in_stmt for Yield and Return(None) ---

    #[test]
    fn test_collect_refs_in_stmt_yield() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_stmt(&Stmt::Yield(Expr::Ident("y".to_string())), &mut refs);
        assert!(refs.contains("y"));
    }

    #[test]
    fn test_collect_refs_in_stmt_return_none() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_stmt(&Stmt::Return(None), &mut refs);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_collect_refs_in_stmt_signal() {
        let mut refs = std::collections::HashSet::new();
        collect_references_in_stmt(&Stmt::Signal {
            name: "s".to_string(), ty: None, secret: false, atomic: false,
            value: Expr::Ident("sig_dep".to_string()),
        }, &mut refs);
        assert!(refs.contains("sig_dep"));
    }
}
