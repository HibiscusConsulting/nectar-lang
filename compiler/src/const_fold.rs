//! Constant folding pass — evaluates constant expressions at compile time.
//!
//! This pass walks the AST and replaces expressions that can be computed
//! at compile time with their resulting literal values. It handles:
//! - Arithmetic on integer and float literals
//! - Boolean logic (&&, ||, !)
//! - Comparison operators
//! - String concatenation of literals
//! - If-expressions with statically known conditions
//! - Nested constant expressions

use crate::ast::*;

/// Statistics about what constant folding accomplished.
#[derive(Debug, Default)]
pub struct FoldStats {
    pub constants_folded: usize,
}

/// Fold all constant expressions in the entire program.
pub fn fold_program(program: &mut Program, stats: &mut FoldStats) {
    for item in &mut program.items {
        fold_item(item, stats);
    }
}

fn fold_item(item: &mut Item, stats: &mut FoldStats) {
    match item {
        Item::Function(f) => fold_function(f, stats),
        Item::Component(c) => {
            for method in &mut c.methods {
                fold_function(method, stats);
            }
            for prop in &mut c.props {
                if let Some(ref mut default) = prop.default {
                    fold_expr(default, stats);
                }
            }
            for state in &mut c.state {
                fold_expr(&mut state.initializer, stats);
            }
        }
        Item::Impl(imp) => {
            for method in &mut imp.methods {
                fold_function(method, stats);
            }
        }
        Item::Store(store) => {
            for signal in &mut store.signals {
                fold_expr(&mut signal.initializer, stats);
            }
            for action in &mut store.actions {
                fold_block(&mut action.body, stats);
            }
            for computed in &mut store.computed {
                fold_block(&mut computed.body, stats);
            }
            for effect in &mut store.effects {
                fold_block(&mut effect.body, stats);
            }
        }
        Item::Agent(agent) => {
            for method in &mut agent.methods {
                fold_function(method, stats);
            }
            for tool in &mut agent.tools {
                fold_block(&mut tool.body, stats);
            }
        }
        Item::Page(page) => {
            for method in &mut page.methods {
                fold_function(method, stats);
            }
            for state in &mut page.state {
                fold_expr(&mut state.initializer, stats);
            }
        }
        Item::Form(form) => {
            for field in &mut form.fields {
                if let Some(ref mut default) = field.default_value {
                    fold_expr(default, stats);
                }
            }
            for method in &mut form.methods {
                fold_function(method, stats);
            }
        }
        Item::Struct(_) | Item::Enum(_) | Item::Use(_)
        | Item::Router(_) | Item::LazyComponent(_) | Item::Test(_) | Item::Trait(_) | Item::Mod(_) => {}
            Item::Contract(_) => {}
            Item::App(_) => {}
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
            Item::Channel(ch) => {
                for method in &mut ch.methods {
                    fold_function(method, stats);
                }
            }
    }
}

fn fold_function(f: &mut Function, stats: &mut FoldStats) {
    fold_block(&mut f.body, stats);
}

fn fold_block(block: &mut Block, stats: &mut FoldStats) {
    for stmt in &mut block.stmts {
        fold_stmt(stmt, stats);
    }
}

fn fold_stmt(stmt: &mut Stmt, stats: &mut FoldStats) {
    match stmt {
        Stmt::Let { value, .. } => fold_expr(value, stats),
        Stmt::Signal { value, .. } => fold_expr(value, stats),
        Stmt::Expr(expr) => fold_expr(expr, stats),
        Stmt::Return(Some(expr)) => fold_expr(expr, stats),
        Stmt::Return(None) => {}
        Stmt::Yield(expr) => fold_expr(expr, stats),
        _ => {}
    }
}

/// Attempt to fold an expression. Replaces the expression in-place if foldable.
pub fn fold_expr(expr: &mut Expr, stats: &mut FoldStats) {
    // First, recursively fold sub-expressions.
    match expr {
        Expr::Binary { left, right, .. } => {
            fold_expr(left, stats);
            fold_expr(right, stats);
        }
        Expr::Unary { operand, .. } => {
            fold_expr(operand, stats);
        }
        Expr::If { condition, then_block, else_block, .. } => {
            fold_expr(condition, stats);
            fold_block(then_block, stats);
            if let Some(eb) = else_block {
                fold_block(eb, stats);
            }
        }
        Expr::FnCall { callee, args, .. } => {
            fold_expr(callee, stats);
            for arg in args {
                fold_expr(arg, stats);
            }
        }
        Expr::MethodCall { object, args, .. } => {
            fold_expr(object, stats);
            for arg in args {
                fold_expr(arg, stats);
            }
        }
        Expr::Block(block) => fold_block(block, stats),
        Expr::For { iterator, body, .. } => {
            fold_expr(iterator, stats);
            fold_block(body, stats);
        }
        Expr::While { condition, body, .. } => {
            fold_expr(condition, stats);
            fold_block(body, stats);
        }
        Expr::Assign { target, value, .. } => {
            fold_expr(target, stats);
            fold_expr(value, stats);
        }
        Expr::Index { object, index, .. } => {
            fold_expr(object, stats);
            fold_expr(index, stats);
        }
        Expr::FieldAccess { object, .. } => {
            fold_expr(object, stats);
        }
        Expr::StructInit { fields, .. } => {
            for (_, val) in fields {
                fold_expr(val, stats);
            }
        }
        Expr::Closure { body, .. } => {
            fold_expr(body, stats);
        }
        Expr::Match { subject, arms, .. } => {
            fold_expr(subject, stats);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    fold_expr(guard, stats);
                }
                fold_expr(&mut arm.body, stats);
            }
        }
        Expr::Borrow(inner) | Expr::BorrowMut(inner) | Expr::Await(inner)
        | Expr::Stream { source: inner } | Expr::Navigate { path: inner }
        | Expr::Receive { channel: inner } => {
            fold_expr(inner, stats);
        }
        Expr::Spawn { body: blk, .. } => {
            fold_block(blk, stats);
        }
        Expr::Send { channel, value } => {
            fold_expr(channel, stats);
            fold_expr(value, stats);
        }
        Expr::Suspend { fallback, body } => {
            fold_expr(fallback, stats);
            fold_expr(body, stats);
        }
        Expr::TryCatch { body, catch_body, .. } => {
            fold_expr(body, stats);
            fold_expr(catch_body, stats);
        }
        Expr::Fetch { url, options, .. } => {
            fold_expr(url, stats);
            if let Some(opts) = options {
                fold_expr(opts, stats);
            }
        }
        Expr::Parallel { tasks, .. } => {
            for e in tasks {
                fold_expr(e, stats);
            }
        }
        Expr::PromptTemplate { interpolations, .. } => {
            for (_, e) in interpolations {
                fold_expr(e, stats);
            }
        }
        Expr::Assert { condition, .. } => {
            fold_expr(condition, stats);
        }
        Expr::AssertEq { left, right, .. } => {
            fold_expr(left, stats);
            fold_expr(right, stats);
        }
        Expr::Animate { target, .. } => {
            fold_expr(target, stats);
        }
        Expr::FormatString { parts } => {
            for part in parts {
                if let FormatPart::Expression(expr) = part {
                    fold_expr(expr, stats);
                }
            }
        }
        Expr::Download { data, filename, .. } => {
            fold_expr(data, stats);
            fold_expr(filename, stats);
        }
        Expr::Env { name, .. } => {
            fold_expr(name, stats);
        }
        Expr::Trace { label, body, .. } => {
            fold_expr(label, stats);
            fold_block(body, stats);
        }
        Expr::Flag { name, .. } => {
            fold_expr(name, stats);
        }
        Expr::ArrayLit(elements) => {
            for e in elements { fold_expr(e, stats); }
        }
        Expr::ObjectLit { fields } => {
            for (_, v) in fields { fold_expr(v, stats); }
        }
        // Leaf nodes — nothing to fold further into
        Expr::Integer(_) | Expr::Float(_) | Expr::StringLit(_) | Expr::Bool(_)
        | Expr::Ident(_) | Expr::SelfExpr | Expr::Channel { .. } => {}
        _ => {}
    }

    // Now try to fold this node itself.
    if let Some(folded) = try_fold(expr) {
        *expr = folded;
        stats.constants_folded += 1;
    }
}

/// Try to fold an expression into a simpler form. Returns `Some` if foldable.
fn try_fold(expr: &Expr) -> Option<Expr> {
    match expr {
        // Binary operations on integer literals
        Expr::Binary { op, left, right } => {
            match (left.as_ref(), right.as_ref()) {
                (Expr::Integer(a), Expr::Integer(b)) => fold_int_binary(*op, *a, *b),
                (Expr::Float(a), Expr::Float(b)) => fold_float_binary(*op, *a, *b),
                (Expr::Bool(a), Expr::Bool(b)) => fold_bool_binary(*op, *a, *b),
                (Expr::StringLit(a), Expr::StringLit(b)) => {
                    if matches!(op, BinOp::Add) {
                        Some(Expr::StringLit(format!("{}{}", a, b)))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }

        // Unary operations
        Expr::Unary { op, operand } => {
            match (op, operand.as_ref()) {
                (UnaryOp::Neg, Expr::Integer(n)) => Some(Expr::Integer(-n)),
                (UnaryOp::Neg, Expr::Float(f)) => Some(Expr::Float(-f)),
                (UnaryOp::Not, Expr::Bool(b)) => Some(Expr::Bool(!b)),
                _ => None,
            }
        }

        // If with known condition
        Expr::If { condition, then_block, else_block } => {
            match condition.as_ref() {
                Expr::Bool(true) => {
                    // Replace with then-block contents
                    Some(Expr::Block(then_block.clone()))
                }
                Expr::Bool(false) => {
                    if let Some(eb) = else_block {
                        Some(Expr::Block(eb.clone()))
                    } else {
                        // `if false { ... }` with no else => unit/empty block
                        None
                    }
                }
                _ => None,
            }
        }

        _ => None,
    }
}

fn fold_int_binary(op: BinOp, a: i64, b: i64) -> Option<Expr> {
    match op {
        BinOp::Add => Some(Expr::Integer(a.wrapping_add(b))),
        BinOp::Sub => Some(Expr::Integer(a.wrapping_sub(b))),
        BinOp::Mul => Some(Expr::Integer(a.wrapping_mul(b))),
        BinOp::Div => {
            if b == 0 { None } else { Some(Expr::Integer(a / b)) }
        }
        BinOp::Mod => {
            if b == 0 { None } else { Some(Expr::Integer(a % b)) }
        }
        BinOp::Eq => Some(Expr::Bool(a == b)),
        BinOp::Neq => Some(Expr::Bool(a != b)),
        BinOp::Lt => Some(Expr::Bool(a < b)),
        BinOp::Gt => Some(Expr::Bool(a > b)),
        BinOp::Lte => Some(Expr::Bool(a <= b)),
        BinOp::Gte => Some(Expr::Bool(a >= b)),
        BinOp::And | BinOp::Or => None,
    }
}

fn fold_float_binary(op: BinOp, a: f64, b: f64) -> Option<Expr> {
    match op {
        BinOp::Add => Some(Expr::Float(a + b)),
        BinOp::Sub => Some(Expr::Float(a - b)),
        BinOp::Mul => Some(Expr::Float(a * b)),
        BinOp::Div => {
            if b == 0.0 { None } else { Some(Expr::Float(a / b)) }
        }
        BinOp::Lt => Some(Expr::Bool(a < b)),
        BinOp::Gt => Some(Expr::Bool(a > b)),
        BinOp::Lte => Some(Expr::Bool(a <= b)),
        BinOp::Gte => Some(Expr::Bool(a >= b)),
        _ => None,
    }
}

fn fold_bool_binary(op: BinOp, a: bool, b: bool) -> Option<Expr> {
    match op {
        BinOp::And => Some(Expr::Bool(a && b)),
        BinOp::Or => Some(Expr::Bool(a || b)),
        BinOp::Eq => Some(Expr::Bool(a == b)),
        BinOp::Neq => Some(Expr::Bool(a != b)),
        _ => None,
    }
}

// We need BinOp to be Copy for the match
impl Copy for BinOp {}
impl Copy for UnaryOp {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Span;

    fn dummy_span() -> Span {
        Span { start: 0, end: 0, line: 0, col: 0 }
    }

    fn make_program(stmts: Vec<Stmt>) -> Program {
        Program {
            items: vec![Item::Function(Function {
                name: "test".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block { stmts, span: dummy_span() },
                is_pub: false,
                is_async: false,
                must_use: false,
                span: dummy_span(),
            })],
        }
    }

    fn get_first_stmt(program: &Program) -> &Stmt {
        match &program.items[0] {
            Item::Function(f) => &f.body.stmts[0],
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_fold_integer_addition() {
        // 2 + 3 => 5
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Add,
            left: Box::new(Expr::Integer(2)),
            right: Box::new(Expr::Integer(3)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);

        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Integer(5)));
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_integer_multiplication() {
        // 10 * 2 => 20
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Mul,
            left: Box::new(Expr::Integer(10)),
            right: Box::new(Expr::Integer(2)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);

        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Integer(20)));
    }

    #[test]
    fn test_fold_nested_expression() {
        // (2 + 3) * 4 => 20
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Mul,
            left: Box::new(Expr::Binary {
                op: BinOp::Add,
                left: Box::new(Expr::Integer(2)),
                right: Box::new(Expr::Integer(3)),
            }),
            right: Box::new(Expr::Integer(4)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);

        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Integer(20)));
        assert_eq!(stats.constants_folded, 2); // inner add, then outer mul
    }

    #[test]
    fn test_fold_boolean_logic() {
        // true && false => false
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::And,
            left: Box::new(Expr::Bool(true)),
            right: Box::new(Expr::Bool(false)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);

        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(false)));
    }

    #[test]
    fn test_fold_not_operator() {
        // !true => false
        let stmts = vec![Stmt::Expr(Expr::Unary {
            op: UnaryOp::Not,
            operand: Box::new(Expr::Bool(true)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);

        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(false)));
    }

    #[test]
    fn test_fold_comparison() {
        // 5 > 3 => true
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Gt,
            left: Box::new(Expr::Integer(5)),
            right: Box::new(Expr::Integer(3)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);

        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    #[test]
    fn test_fold_string_concatenation() {
        // "hello" + " world" => "hello world"
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Add,
            left: Box::new(Expr::StringLit("hello".to_string())),
            right: Box::new(Expr::StringLit(" world".to_string())),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);

        assert_eq!(
            *get_first_stmt(&program),
            Stmt::Expr(Expr::StringLit("hello world".to_string()))
        );
    }

    #[test]
    fn test_fold_if_true() {
        // if true { 42 } else { 99 } => block containing 42
        let stmts = vec![Stmt::Expr(Expr::If {
            condition: Box::new(Expr::Bool(true)),
            then_block: Block {
                stmts: vec![Stmt::Expr(Expr::Integer(42))],
                span: dummy_span(),
            },
            else_block: Some(Block {
                stmts: vec![Stmt::Expr(Expr::Integer(99))],
                span: dummy_span(),
            }),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);

        // Should become Block containing 42
        match get_first_stmt(&program) {
            Stmt::Expr(Expr::Block(block)) => {
                assert_eq!(block.stmts.len(), 1);
                assert_eq!(block.stmts[0], Stmt::Expr(Expr::Integer(42)));
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn test_no_fold_division_by_zero() {
        // 10 / 0 should NOT be folded
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Div,
            left: Box::new(Expr::Integer(10)),
            right: Box::new(Expr::Integer(0)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);

        assert_eq!(stats.constants_folded, 0);
    }

    #[test]
    fn test_no_fold_variable_expression() {
        // x + 3 should NOT be folded
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Add,
            left: Box::new(Expr::Ident("x".to_string())),
            right: Box::new(Expr::Integer(3)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);

        assert_eq!(stats.constants_folded, 0);
    }

    // --- Float operations ---

    #[test]
    fn test_fold_float_addition() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Add,
            left: Box::new(Expr::Float(1.5)),
            right: Box::new(Expr::Float(2.5)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Float(4.0)));
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_float_subtraction() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Sub,
            left: Box::new(Expr::Float(10.0)),
            right: Box::new(Expr::Float(3.5)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Float(6.5)));
    }

    #[test]
    fn test_fold_float_multiplication() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Mul,
            left: Box::new(Expr::Float(3.0)),
            right: Box::new(Expr::Float(4.0)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Float(12.0)));
    }

    #[test]
    fn test_fold_float_division() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Div,
            left: Box::new(Expr::Float(10.0)),
            right: Box::new(Expr::Float(4.0)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Float(2.5)));
    }

    #[test]
    fn test_no_fold_float_division_by_zero() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Div,
            left: Box::new(Expr::Float(10.0)),
            right: Box::new(Expr::Float(0.0)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 0);
    }

    #[test]
    fn test_fold_float_comparison_lt() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Lt,
            left: Box::new(Expr::Float(1.0)),
            right: Box::new(Expr::Float(2.0)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    #[test]
    fn test_fold_float_comparison_gte() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Gte,
            left: Box::new(Expr::Float(3.0)),
            right: Box::new(Expr::Float(3.0)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    // --- Integer sub/mod ---

    #[test]
    fn test_fold_integer_subtraction() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Sub,
            left: Box::new(Expr::Integer(10)),
            right: Box::new(Expr::Integer(3)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Integer(7)));
    }

    #[test]
    fn test_fold_integer_modulo() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Mod,
            left: Box::new(Expr::Integer(10)),
            right: Box::new(Expr::Integer(3)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Integer(1)));
    }

    #[test]
    fn test_no_fold_integer_mod_by_zero() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Mod,
            left: Box::new(Expr::Integer(10)),
            right: Box::new(Expr::Integer(0)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 0);
    }

    // --- Bool or/eq/neq ---

    #[test]
    fn test_fold_bool_or() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Or,
            left: Box::new(Expr::Bool(false)),
            right: Box::new(Expr::Bool(true)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    #[test]
    fn test_fold_bool_eq() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Eq,
            left: Box::new(Expr::Bool(true)),
            right: Box::new(Expr::Bool(true)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    #[test]
    fn test_fold_bool_neq() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Neq,
            left: Box::new(Expr::Bool(true)),
            right: Box::new(Expr::Bool(false)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    // --- Negation ---

    #[test]
    fn test_fold_negate_integer() {
        let stmts = vec![Stmt::Expr(Expr::Unary {
            op: UnaryOp::Neg,
            operand: Box::new(Expr::Integer(42)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Integer(-42)));
    }

    #[test]
    fn test_fold_negate_float() {
        let stmts = vec![Stmt::Expr(Expr::Unary {
            op: UnaryOp::Neg,
            operand: Box::new(Expr::Float(3.14)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Float(-3.14)));
    }

    // --- If false with/without else ---

    #[test]
    fn test_fold_if_false_with_else() {
        let stmts = vec![Stmt::Expr(Expr::If {
            condition: Box::new(Expr::Bool(false)),
            then_block: Block {
                stmts: vec![Stmt::Expr(Expr::Integer(1))],
                span: dummy_span(),
            },
            else_block: Some(Block {
                stmts: vec![Stmt::Expr(Expr::Integer(2))],
                span: dummy_span(),
            }),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        match get_first_stmt(&program) {
            Stmt::Expr(Expr::Block(block)) => {
                assert_eq!(block.stmts[0], Stmt::Expr(Expr::Integer(2)));
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn test_fold_if_false_without_else() {
        // if false { ... } with no else cannot be folded to a block
        let stmts = vec![Stmt::Expr(Expr::If {
            condition: Box::new(Expr::Bool(false)),
            then_block: Block {
                stmts: vec![Stmt::Expr(Expr::Integer(1))],
                span: dummy_span(),
            },
            else_block: None,
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        // Should NOT be folded (no replacement)
        assert_eq!(stats.constants_folded, 0);
    }

    // --- Folding inside various item types ---

    fn make_function(name: &str, stmts: Vec<Stmt>) -> Function {
        Function {
            name: name.to_string(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: Block { stmts, span: dummy_span() },
            is_pub: false,
            is_async: false,
            must_use: false,
            span: dummy_span(),
        }
    }

    fn foldable_expr() -> Expr {
        Expr::Binary {
            op: BinOp::Add,
            left: Box::new(Expr::Integer(1)),
            right: Box::new(Expr::Integer(2)),
        }
    }

    #[test]
    fn test_fold_inside_store() {
        let mut program = Program {
            items: vec![Item::Store(StoreDef {
                name: "TestStore".to_string(),
                signals: vec![StateField {
                    name: "count".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    atomic: false,
                    initializer: foldable_expr(),
                    ownership: Ownership::Owned,
                }],
                actions: vec![ActionDef {
                    name: "inc".to_string(),
                    params: vec![],
                    body: Block { stmts: vec![Stmt::Expr(foldable_expr())], span: dummy_span() },
                    is_async: false,
                    span: dummy_span(),
                }],
                computed: vec![ComputedDef {
                    name: "double".to_string(),
                    return_type: None,
                    body: Block { stmts: vec![Stmt::Expr(foldable_expr())], span: dummy_span() },
                    span: dummy_span(),
                }],
                effects: vec![EffectDef {
                    name: "log".to_string(),
                    body: Block { stmts: vec![Stmt::Expr(foldable_expr())], span: dummy_span() },
                    span: dummy_span(),
                }],
                selectors: vec![],
                is_pub: false,
                span: dummy_span(),
            })],
        };
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 4);
    }

    #[test]
    fn test_fold_inside_component() {
        let mut program = Program {
            items: vec![Item::Component(Component {
                name: "Test".to_string(),
                type_params: vec![],
                props: vec![Prop {
                    name: "x".to_string(),
                    ty: Type::Named("i32".to_string()),
                    default: Some(foldable_expr()),
                }],
                state: vec![StateField {
                    name: "s".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    atomic: false,
                    initializer: foldable_expr(),
                    ownership: Ownership::Owned,
                }],
                methods: vec![make_function("m", vec![Stmt::Expr(foldable_expr())])],
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
            })],
        };
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 3);
    }

    #[test]
    fn test_fold_inside_impl() {
        let mut program = Program {
            items: vec![Item::Impl(ImplBlock {
                target: "Foo".to_string(),
                trait_impls: vec![],
                methods: vec![make_function("bar", vec![Stmt::Expr(foldable_expr())])],
                span: dummy_span(),
            })],
        };
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_agent() {
        let mut program = Program {
            items: vec![Item::Agent(AgentDef {
                name: "Bot".to_string(),
                system_prompt: None,
                tools: vec![ToolDef {
                    name: "search".to_string(),
                    description: None,
                    params: vec![],
                    return_type: None,
                    body: Block { stmts: vec![Stmt::Expr(foldable_expr())], span: dummy_span() },
                    span: dummy_span(),
                }],
                state: vec![],
                methods: vec![make_function("go", vec![Stmt::Expr(foldable_expr())])],
                render: None,
                span: dummy_span(),
            })],
        };
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_page() {
        let mut program = Program {
            items: vec![Item::Page(PageDef {
                name: "Home".to_string(),
                props: vec![],
                meta: None,
                state: vec![StateField {
                    name: "x".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    atomic: false,
                    initializer: foldable_expr(),
                    ownership: Ownership::Owned,
                }],
                methods: vec![make_function("go", vec![Stmt::Expr(foldable_expr())])],
                styles: vec![],
                render: RenderBlock {
                    body: TemplateNode::Fragment(vec![]),
                    span: dummy_span(),
                },
                permissions: None,
                gestures: vec![],
                is_pub: false,
                span: dummy_span(),
            })],
        };
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_form() {
        let mut program = Program {
            items: vec![Item::Form(FormDef {
                name: "F".to_string(),
                fields: vec![FormFieldDef {
                    name: "x".to_string(),
                    ty: Type::Named("i32".to_string()),
                    validators: vec![],
                    label: None,
                    placeholder: None,
                    default_value: Some(foldable_expr()),
                    span: dummy_span(),
                }],
                on_submit: None,
                steps: vec![],
                methods: vec![make_function("go", vec![Stmt::Expr(foldable_expr())])],
                styles: vec![],
                render: None,
                is_pub: false,
                span: dummy_span(),
            })],
        };
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_channel() {
        let mut program = Program {
            items: vec![Item::Channel(ChannelDef {
                name: "Ch".to_string(),
                url: Expr::StringLit("/ws".to_string()),
                provider: None,
                contract: None,
                on_message: None,
                on_connect: None,
                on_disconnect: None,
                reconnect: false,
                heartbeat_interval: None,
                methods: vec![make_function("send", vec![Stmt::Expr(foldable_expr())])],
                is_pub: false,
                span: dummy_span(),
            })],
        };
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    // --- Folding inside expressions ---

    #[test]
    fn test_fold_inside_for() {
        let mut expr = Expr::For {
            binding: "i".to_string(),
            iterator: Box::new(foldable_expr()),
            body: Block { stmts: vec![Stmt::Expr(foldable_expr())], span: dummy_span() },
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_while() {
        let mut expr = Expr::While {
            condition: Box::new(foldable_expr()),
            body: Block { stmts: vec![Stmt::Expr(foldable_expr())], span: dummy_span() },
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_match() {
        let mut expr = Expr::Match {
            subject: Box::new(foldable_expr()),
            arms: vec![MatchArm {
                pattern: Pattern::Wildcard,
                guard: None,
                body: foldable_expr(),
            }],
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_closure() {
        let mut expr = Expr::Closure {
            params: vec![],
            body: Box::new(foldable_expr()),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_assign() {
        let mut expr = Expr::Assign {
            target: Box::new(Expr::Ident("x".to_string())),
            value: Box::new(foldable_expr()),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_index() {
        let mut expr = Expr::Index {
            object: Box::new(Expr::Ident("arr".to_string())),
            index: Box::new(foldable_expr()),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_field_access() {
        let mut expr = Expr::FieldAccess {
            object: Box::new(foldable_expr()),
            field: "x".to_string(),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_struct_init() {
        let mut expr = Expr::StructInit {
            name: "Point".to_string(),
            fields: vec![("x".to_string(), foldable_expr())],
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_borrow() {
        let mut expr = Expr::Borrow(Box::new(foldable_expr()));
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_spawn() {
        let mut expr = Expr::Spawn {
            body: Block { stmts: vec![Stmt::Expr(foldable_expr())], span: dummy_span() },
            span: dummy_span(),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_send() {
        let mut expr = Expr::Send {
            channel: Box::new(Expr::Ident("ch".to_string())),
            value: Box::new(foldable_expr()),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_suspend() {
        let mut expr = Expr::Suspend {
            fallback: Box::new(foldable_expr()),
            body: Box::new(foldable_expr()),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_try_catch() {
        let mut expr = Expr::TryCatch {
            body: Box::new(foldable_expr()),
            error_binding: "e".to_string(),
            catch_body: Box::new(foldable_expr()),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_fetch() {
        let mut expr = Expr::Fetch {
            url: Box::new(Expr::StringLit("http://x".to_string())),
            options: Some(Box::new(foldable_expr())),
            contract: None,
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_parallel() {
        let mut expr = Expr::Parallel {
            tasks: vec![foldable_expr(), foldable_expr()],
            span: dummy_span(),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_prompt_template() {
        let mut expr = Expr::PromptTemplate {
            template: "hello {x}".to_string(),
            interpolations: vec![("x".to_string(), foldable_expr())],
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_assert() {
        let mut expr = Expr::Assert {
            condition: Box::new(foldable_expr()),
            message: None,
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_assert_eq() {
        let mut expr = Expr::AssertEq {
            left: Box::new(foldable_expr()),
            right: Box::new(foldable_expr()),
            message: None,
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_animate() {
        let mut expr = Expr::Animate {
            target: Box::new(foldable_expr()),
            animation: "fade".to_string(),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_format_string() {
        let mut expr = Expr::FormatString {
            parts: vec![
                FormatPart::Literal("hi ".to_string()),
                FormatPart::Expression(Box::new(foldable_expr())),
            ],
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_download() {
        let mut expr = Expr::Download {
            data: Box::new(foldable_expr()),
            filename: Box::new(foldable_expr()),
            span: dummy_span(),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_env() {
        let mut expr = Expr::Env {
            name: Box::new(Expr::StringLit("KEY".to_string())),
            span: dummy_span(),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 0); // nothing to fold
    }

    #[test]
    fn test_fold_inside_trace() {
        let mut expr = Expr::Trace {
            label: Box::new(Expr::StringLit("t".to_string())),
            body: Block { stmts: vec![Stmt::Expr(foldable_expr())], span: dummy_span() },
            span: dummy_span(),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_flag() {
        let mut expr = Expr::Flag {
            name: Box::new(Expr::StringLit("f".to_string())),
            span: dummy_span(),
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 0);
    }

    // --- Integer comparison edge cases ---

    #[test]
    fn test_fold_int_eq() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Eq,
            left: Box::new(Expr::Integer(5)),
            right: Box::new(Expr::Integer(5)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    #[test]
    fn test_fold_int_neq() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Neq,
            left: Box::new(Expr::Integer(1)),
            right: Box::new(Expr::Integer(2)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    #[test]
    fn test_fold_int_lte() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Lte,
            left: Box::new(Expr::Integer(5)),
            right: Box::new(Expr::Integer(5)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    // --- Stmt folding ---

    #[test]
    fn test_fold_in_let_stmt() {
        let stmts = vec![Stmt::Let {
            name: "x".to_string(),
            ty: None,
            mutable: false,
            secret: false,
            value: foldable_expr(),
            ownership: Ownership::Owned,
        }];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_in_signal_stmt() {
        let stmts = vec![Stmt::Signal {
            name: "s".to_string(),
            ty: None,
            secret: false,
            atomic: false,
            value: foldable_expr(),
        }];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_in_return_stmt() {
        let stmts = vec![Stmt::Return(Some(foldable_expr()))];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_in_yield_stmt() {
        let stmts = vec![Stmt::Yield(foldable_expr())];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_return_none() {
        let stmts = vec![Stmt::Return(None)];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 0);
    }

    // --- Float comparison edge cases (Gt, Lte) ---

    #[test]
    fn test_fold_float_gt() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Gt,
            left: Box::new(Expr::Float(5.0)),
            right: Box::new(Expr::Float(3.0)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    #[test]
    fn test_fold_float_lte() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Lte,
            left: Box::new(Expr::Float(3.0)),
            right: Box::new(Expr::Float(3.0)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    // --- Float ops that return None ---

    #[test]
    fn test_no_fold_float_eq() {
        // Float Eq is not supported
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Eq,
            left: Box::new(Expr::Float(1.0)),
            right: Box::new(Expr::Float(1.0)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 0);
    }

    #[test]
    fn test_no_fold_float_mod() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Mod,
            left: Box::new(Expr::Float(10.0)),
            right: Box::new(Expr::Float(3.0)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 0);
    }

    // --- Int And/Or returning None ---

    #[test]
    fn test_no_fold_int_and() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::And,
            left: Box::new(Expr::Integer(1)),
            right: Box::new(Expr::Integer(2)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 0);
    }

    // --- Bool ops that return None ---

    #[test]
    fn test_no_fold_bool_lt() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Lt,
            left: Box::new(Expr::Bool(true)),
            right: Box::new(Expr::Bool(false)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 0);
    }

    // --- String non-Add returning None ---

    #[test]
    fn test_no_fold_string_sub() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Sub,
            left: Box::new(Expr::StringLit("a".to_string())),
            right: Box::new(Expr::StringLit("b".to_string())),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(stats.constants_folded, 0);
    }

    // --- Fold inside fn call and method call ---

    #[test]
    fn test_fold_inside_fn_call() {
        let mut expr = Expr::FnCall {
            callee: Box::new(Expr::Ident("f".to_string())),
            args: vec![foldable_expr()],
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_method_call() {
        let mut expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("obj".to_string())),
            method: "m".to_string(),
            args: vec![foldable_expr()],
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    // --- Int division ---

    #[test]
    fn test_fold_int_division() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Div,
            left: Box::new(Expr::Integer(10)),
            right: Box::new(Expr::Integer(3)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Integer(3)));
    }

    // --- Int Lt ---

    #[test]
    fn test_fold_int_lt() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Lt,
            left: Box::new(Expr::Integer(1)),
            right: Box::new(Expr::Integer(5)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    // --- Int Gte ---

    #[test]
    fn test_fold_int_gte() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinOp::Gte,
            left: Box::new(Expr::Integer(5)),
            right: Box::new(Expr::Integer(5)),
        })];
        let mut program = make_program(stmts);
        let mut stats = FoldStats::default();
        fold_program(&mut program, &mut stats);
        assert_eq!(*get_first_stmt(&program), Stmt::Expr(Expr::Bool(true)));
    }

    // --- Fold inside BorrowMut, Await, Stream, Navigate, Receive ---

    #[test]
    fn test_fold_inside_borrow_mut() {
        let mut expr = Expr::BorrowMut(Box::new(foldable_expr()));
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_await() {
        let mut expr = Expr::Await(Box::new(foldable_expr()));
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_navigate() {
        let mut expr = Expr::Navigate { path: Box::new(foldable_expr()) };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_receive() {
        let mut expr = Expr::Receive { channel: Box::new(foldable_expr()) };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }

    #[test]
    fn test_fold_inside_match_guard() {
        let mut expr = Expr::Match {
            subject: Box::new(foldable_expr()),
            arms: vec![MatchArm {
                pattern: Pattern::Wildcard,
                guard: Some(foldable_expr()),
                body: foldable_expr(),
            }],
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        // subject + guard + body = 3 folds
        assert_eq!(stats.constants_folded, 3);
    }

    #[test]
    fn test_fold_inside_array_lit() {
        let mut expr = Expr::ArrayLit(vec![foldable_expr(), foldable_expr()]);
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_inside_object_lit() {
        let mut expr = Expr::ObjectLit {
            fields: vec![
                ("x".into(), foldable_expr()),
                ("y".into(), foldable_expr()),
            ],
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 2);
    }

    #[test]
    fn test_fold_empty_array_lit() {
        let mut expr = Expr::ArrayLit(vec![]);
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 0);
    }

    #[test]
    fn test_fold_empty_object_lit() {
        let mut expr = Expr::ObjectLit { fields: vec![] };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 0);
    }

    #[test]
    fn test_fold_match_guard() {
        let mut expr = Expr::Match {
            subject: Box::new(Expr::Ident("x".into())),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Ident("n".into()),
                    guard: Some(foldable_expr()),
                    body: foldable_expr(),
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: Expr::Integer(0),
                },
            ],
        };
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 2, "Should fold guard and body");
    }

    #[test]
    fn test_fold_nested_array_lit() {
        let mut expr = Expr::ArrayLit(vec![
            Expr::ArrayLit(vec![foldable_expr()]),
        ]);
        let mut stats = FoldStats::default();
        fold_expr(&mut expr, &mut stats);
        assert_eq!(stats.constants_folded, 1);
    }
}
