//! Optimization pass manager — coordinates all optimization passes.
//!
//! The optimizer runs passes in sequence:
//! 1. Constant folding (evaluate compile-time expressions)
//! 2. Dead code elimination (remove unreachable/unused code)
//! 3. Tree shaking (remove unused top-level items)

use crate::ast::Program;
use crate::const_fold;
use crate::dce;
use crate::tree_shake;

/// Optimization level controlling which passes run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationLevel {
    /// No optimizations.
    None,
    /// Basic optimizations: constant folding + dead code elimination.
    Basic,
    /// Full optimizations: all passes including tree shaking.
    Full,
}

impl OptimizationLevel {
    /// Parse from a numeric string: "0", "1", "2".
    pub fn from_level(level: u8) -> Self {
        match level {
            0 => OptimizationLevel::None,
            1 => OptimizationLevel::Basic,
            _ => OptimizationLevel::Full,
        }
    }
}

/// Aggregated statistics from all optimization passes.
#[derive(Debug, Default)]
pub struct OptimizeStats {
    pub constants_folded: usize,
    pub stmts_removed: usize,
    pub functions_removed: usize,
    pub unused_vars_removed: usize,
    pub items_shaken: usize,
    pub shaken_names: Vec<String>,
}

impl std::fmt::Display for OptimizeStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if self.constants_folded > 0 {
            parts.push(format!("{} constants folded", self.constants_folded));
        }
        if self.stmts_removed > 0 {
            parts.push(format!("{} dead statements removed", self.stmts_removed));
        }
        if self.functions_removed > 0 {
            parts.push(format!("{} unused functions removed", self.functions_removed));
        }
        if self.unused_vars_removed > 0 {
            parts.push(format!("{} unused variables removed", self.unused_vars_removed));
        }
        if self.items_shaken > 0 {
            parts.push(format!("{} items tree-shaken", self.items_shaken));
        }
        if parts.is_empty() {
            write!(f, "no optimizations applied")
        } else {
            write!(f, "{}", parts.join(", "))
        }
    }
}

/// Run optimization passes on the program at the given level.
///
/// Returns statistics about what was optimized.
pub fn optimize(program: &mut Program, level: OptimizationLevel) -> OptimizeStats {
    let mut stats = OptimizeStats::default();

    if level == OptimizationLevel::None {
        return stats;
    }

    // Pass 1: Constant folding
    let mut fold_stats = const_fold::FoldStats::default();
    const_fold::fold_program(program, &mut fold_stats);
    stats.constants_folded = fold_stats.constants_folded;

    // Pass 2: Dead code elimination
    let mut dce_stats = dce::DceStats::default();
    dce::eliminate_dead_code(program, &mut dce_stats);
    stats.stmts_removed = dce_stats.stmts_removed;
    stats.functions_removed = dce_stats.functions_removed;
    stats.unused_vars_removed = dce_stats.unused_vars_removed;

    if level == OptimizationLevel::Full {
        // Pass 3: Tree shaking
        let mut shake_stats = tree_shake::ShakeStats::default();
        tree_shake::shake(program, &[], &mut shake_stats);
        stats.items_shaken = shake_stats.items_removed;
        stats.shaken_names = shake_stats.removed_names;
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
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
            is_async: false,
            must_use: false,
            span: dummy_span(),
        })
    }

    #[test]
    fn test_optimization_level_none() {
        let mut program = Program {
            items: vec![make_fn("test", true, vec![
                Stmt::Expr(Expr::Binary {
                    op: BinOp::Add,
                    left: Box::new(Expr::Integer(2)),
                    right: Box::new(Expr::Integer(3)),
                }),
            ])],
        };
        let stats = optimize(&mut program, OptimizationLevel::None);
        assert_eq!(stats.constants_folded, 0);

        // Expression should NOT be folded
        match &program.items[0] {
            Item::Function(f) => {
                assert!(matches!(&f.body.stmts[0], Stmt::Expr(Expr::Binary { .. })));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_optimization_level_basic() {
        let mut program = Program {
            items: vec![make_fn("test", true, vec![
                Stmt::Expr(Expr::Binary {
                    op: BinOp::Add,
                    left: Box::new(Expr::Integer(2)),
                    right: Box::new(Expr::Integer(3)),
                }),
                Stmt::Return(Some(Expr::Integer(1))),
                Stmt::Expr(Expr::Integer(99)), // dead code
            ])],
        };
        let stats = optimize(&mut program, OptimizationLevel::Basic);
        assert!(stats.constants_folded >= 1);
        assert!(stats.stmts_removed >= 1);
    }

    #[test]
    fn test_optimization_level_full() {
        let mut program = Program {
            items: vec![
                make_fn("main", true, vec![
                    Stmt::Expr(Expr::Binary {
                        op: BinOp::Add,
                        left: Box::new(Expr::Integer(2)),
                        right: Box::new(Expr::Integer(3)),
                    }),
                ]),
                make_fn("unused", false, vec![
                    Stmt::Expr(Expr::Integer(0)),
                ]),
            ],
        };
        let stats = optimize(&mut program, OptimizationLevel::Full);
        assert!(stats.constants_folded >= 1);
        // unused function should be shaken
        assert!(stats.items_shaken >= 1 || stats.functions_removed >= 1);
    }

    #[test]
    fn test_from_level() {
        assert_eq!(OptimizationLevel::from_level(0), OptimizationLevel::None);
        assert_eq!(OptimizationLevel::from_level(1), OptimizationLevel::Basic);
        assert_eq!(OptimizationLevel::from_level(2), OptimizationLevel::Full);
        assert_eq!(OptimizationLevel::from_level(3), OptimizationLevel::Full);
    }

    #[test]
    fn test_stats_display() {
        let stats = OptimizeStats {
            constants_folded: 5,
            stmts_removed: 3,
            functions_removed: 1,
            unused_vars_removed: 2,
            items_shaken: 0,
            shaken_names: vec![],
        };
        let display = format!("{}", stats);
        assert!(display.contains("5 constants folded"));
        assert!(display.contains("3 dead statements removed"));
        assert!(display.contains("1 unused functions removed"));
    }
}
