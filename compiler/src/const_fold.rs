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
            Item::Auth(_) => {}
            Item::Upload(_) => {}
            Item::Db(_) => {}
            Item::Cache(_) => {}
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
}
