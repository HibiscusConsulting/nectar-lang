use crate::ast::*;
use std::collections::HashSet;

/// Analyzes a program to determine which runtime modules are needed.
/// The compiler uses this to invoke the runtime builder with only
/// the modules that the compiled program actually uses, keeping
/// the output bundle size minimal.
pub fn detect_required_modules(program: &Program) -> HashSet<String> {
    let mut modules = HashSet::new();
    modules.insert("core".to_string()); // always needed

    for item in &program.items {
        match item {
            Item::Page(_) => {
                modules.insert("seo".to_string());
            }
            Item::Form(_) => {
                modules.insert("form".to_string());
            }
            Item::Channel(_) => {
                modules.insert("channel".to_string());
            }
            Item::Contract(_) => {
                modules.insert("contract".to_string());
            }
            Item::App(_) => {
                modules.insert("pwa".to_string());
            }
            Item::Embed(_) => {
                modules.insert("embed".to_string());
            }
            Item::Pdf(_) => {
                modules.insert("pdf".to_string());
            }
            Item::Payment(_) => {
                modules.insert("payment".to_string());
            }
            Item::Auth(_) => {
                modules.insert("auth".to_string());
            }
            Item::Upload(_) => {
                modules.insert("upload".to_string());
            }
            Item::Db(_) => {
                modules.insert("db".to_string());
            }
            Item::Component(c) => {
                if c.permissions.is_some() {
                    modules.insert("permissions".to_string());
                }
                if !c.gestures.is_empty() {
                    modules.insert("pwa".to_string());
                }
                if c.on_destroy.is_some() {
                    modules.insert("lifecycle".to_string());
                }
                check_exprs_in_component(c, &mut modules);
            }
            Item::Store(s) => {
                if !s.selectors.is_empty() {
                    modules.insert("atomic".to_string());
                }
                for field in &s.signals {
                    if field.atomic {
                        modules.insert("atomic".to_string());
                    }
                }
            }
            Item::LazyComponent(lazy) => {
                modules.insert("loader".to_string());
                if lazy.component.permissions.is_some() {
                    modules.insert("permissions".to_string());
                }
                if !lazy.component.gestures.is_empty() {
                    modules.insert("pwa".to_string());
                }
                if lazy.component.on_destroy.is_some() {
                    modules.insert("lifecycle".to_string());
                }
                check_exprs_in_component(&lazy.component, &mut modules);
            }
            _ => {}
        }
    }

    modules
}

fn check_exprs_in_component(component: &Component, modules: &mut HashSet<String>) {
    for method in &component.methods {
        check_exprs_in_block(&method.body, modules);
    }
}

fn check_exprs_in_block(block: &Block, modules: &mut HashSet<String>) {
    for stmt in &block.stmts {
        check_exprs_in_stmt(stmt, modules);
    }
}

fn check_exprs_in_stmt(stmt: &Stmt, modules: &mut HashSet<String>) {
    match stmt {
        Stmt::Expr(expr) | Stmt::Return(Some(expr)) => {
            check_expr(expr, modules);
        }
        Stmt::Let { value, .. } => {
            check_expr(value, modules);
        }
        Stmt::Signal { value, .. } => {
            check_expr(value, modules);
        }
        Stmt::LetDestructure { value, .. } => {
            check_expr(value, modules);
        }
        Stmt::Yield(expr) => {
            check_expr(expr, modules);
        }
        Stmt::Return(None) => {}
    }
}

fn check_expr(expr: &Expr, modules: &mut HashSet<String>) {
    match expr {
        Expr::Spawn { body, .. } => {
            modules.insert("worker".to_string());
            check_exprs_in_block(body, modules);
        }
        Expr::Parallel { tasks, .. } => {
            modules.insert("worker".to_string());
            for task in tasks {
                check_expr(task, modules);
            }
        }
        Expr::Env { .. } => {
            modules.insert("env".to_string());
        }
        Expr::Trace { body, .. } => {
            modules.insert("trace".to_string());
            check_exprs_in_block(body, modules);
        }
        Expr::Flag { .. } => {
            modules.insert("flags".to_string());
        }
        Expr::Download { .. } => {
            modules.insert("pdf".to_string());
        }
        Expr::DynamicImport { .. } => {
            modules.insert("loader".to_string());
        }
        Expr::Fetch { .. } => { /* core handles fetch */ }
        // Recurse into sub-expressions
        Expr::Binary { left, right, .. } => {
            check_expr(left, modules);
            check_expr(right, modules);
        }
        Expr::Unary { operand, .. } => {
            check_expr(operand, modules);
        }
        Expr::FieldAccess { object, .. } => {
            check_expr(object, modules);
        }
        Expr::MethodCall { object, args, .. } => {
            check_expr(object, modules);
            for arg in args {
                check_expr(arg, modules);
            }
        }
        Expr::FnCall { callee, args, .. } => {
            check_expr(callee, modules);
            for arg in args {
                check_expr(arg, modules);
            }
        }
        Expr::Index { object, index, .. } => {
            check_expr(object, modules);
            check_expr(index, modules);
        }
        Expr::If { condition, then_block, else_block, .. } => {
            check_expr(condition, modules);
            check_exprs_in_block(then_block, modules);
            if let Some(eb) = else_block {
                check_exprs_in_block(eb, modules);
            }
        }
        Expr::Match { subject, arms, .. } => {
            check_expr(subject, modules);
            for arm in arms {
                check_expr(&arm.body, modules);
            }
        }
        Expr::For { iterator, body, .. } => {
            check_expr(iterator, modules);
            check_exprs_in_block(body, modules);
        }
        Expr::While { condition, body, .. } => {
            check_expr(condition, modules);
            check_exprs_in_block(body, modules);
        }
        Expr::Block(block) => {
            check_exprs_in_block(block, modules);
        }
        Expr::Assign { target, value, .. } => {
            check_expr(target, modules);
            check_expr(value, modules);
        }
        Expr::Await(inner) => {
            check_expr(inner, modules);
        }
        Expr::TryCatch { body, catch_body, .. } => {
            check_expr(body, modules);
            check_expr(catch_body, modules);
        }
        Expr::Closure { body, .. } => {
            check_expr(body, modules);
        }
        Expr::Borrow(inner) | Expr::BorrowMut(inner) | Expr::Try(inner) | Expr::Stream { source: inner } => {
            check_expr(inner, modules);
        }
        Expr::Suspend { fallback, body, .. } => {
            check_expr(fallback, modules);
            check_expr(body, modules);
        }
        Expr::Send { channel, value, .. } => {
            modules.insert("worker".to_string());
            check_expr(channel, modules);
            check_expr(value, modules);
        }
        Expr::Receive { channel, .. } => {
            modules.insert("worker".to_string());
            check_expr(channel, modules);
        }
        Expr::Channel { .. } => {
            modules.insert("worker".to_string());
        }
        _ => {}
    }
}

/// Format the detected modules as a comma-separated string suitable
/// for passing to `build-runtime.js --modules`.
pub fn modules_to_string(modules: &HashSet<String>) -> String {
    let mut sorted: Vec<&String> = modules.iter().collect();
    sorted.sort();
    sorted.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Span;

    fn empty_span() -> Span {
        Span { start: 0, end: 0, line: 0, col: 0 }
    }

    #[test]
    fn test_core_always_included() {
        let program = Program { items: vec![] };
        let modules = detect_required_modules(&program);
        assert!(modules.contains("core"));
        assert_eq!(modules.len(), 1);
    }

    #[test]
    fn test_page_includes_seo() {
        let program = Program {
            items: vec![Item::Page(PageDef {
                name: "Home".to_string(),
                props: vec![],
                meta: None,
                state: vec![],
                methods: vec![],
                styles: vec![],
                render: RenderBlock {
                    body: TemplateNode::TextLiteral("hello".to_string()),
                    span: empty_span(),
                },
                permissions: None,
                gestures: vec![],
                is_pub: false,
                span: empty_span(),
            })],
        };
        let modules = detect_required_modules(&program);
        assert!(modules.contains("core"));
        assert!(modules.contains("seo"));
    }

    #[test]
    fn test_contract_includes_contract() {
        let program = Program {
            items: vec![Item::Contract(ContractDef {
                name: "TestContract".to_string(),
                fields: vec![],
                is_pub: false,
                span: empty_span(),
            })],
        };
        let modules = detect_required_modules(&program);
        assert!(modules.contains("contract"));
    }

    #[test]
    fn test_form_includes_form() {
        let program = Program {
            items: vec![Item::Form(FormDef {
                name: "TestForm".to_string(),
                fields: vec![],
                on_submit: None,
                steps: vec![],
                methods: vec![],
                styles: vec![],
                render: None,
                is_pub: false,
                span: empty_span(),
            })],
        };
        let modules = detect_required_modules(&program);
        assert!(modules.contains("form"));
    }

    #[test]
    fn test_modules_to_string() {
        let mut modules = HashSet::new();
        modules.insert("core".to_string());
        modules.insert("seo".to_string());
        modules.insert("form".to_string());
        let result = modules_to_string(&modules);
        assert_eq!(result, "core,form,seo");
    }
}
