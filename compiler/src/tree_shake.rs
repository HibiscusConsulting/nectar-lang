//! Tree shaking pass — removes unused top-level items.
//!
//! Starting from a set of entry points (public functions, components, routers),
//! this pass builds a dependency graph, marks all reachable items, and removes
//! everything else.

use std::collections::{HashMap, HashSet};

use crate::ast::*;

/// Statistics about what tree shaking accomplished.
#[derive(Debug, Default)]
pub struct ShakeStats {
    pub items_removed: usize,
    pub removed_names: Vec<String>,
}

/// Shake the program tree — keep only items reachable from entry points.
///
/// If `entry_points` is empty, all public items and components are treated
/// as entry points.
pub fn shake(program: &mut Program, entry_points: &[String], stats: &mut ShakeStats) {
    // Step 1: Build a name -> index map and a dependency graph.
    let mut name_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, item) in program.items.iter().enumerate() {
        if let Some(name) = item_name(item) {
            name_to_idx.insert(name, i);
        }
    }

    // Step 2: Build the dependency graph (item index -> set of referenced names).
    let mut deps: HashMap<usize, HashSet<String>> = HashMap::new();
    for (i, item) in program.items.iter().enumerate() {
        let mut referenced = HashSet::new();
        collect_item_deps(item, &mut referenced);
        deps.insert(i, referenced);
    }

    // Step 3: Determine entry points.
    let mut roots: HashSet<usize> = HashSet::new();

    if entry_points.is_empty() {
        // Use all public items, components, routers, stores, agents as roots.
        for (i, item) in program.items.iter().enumerate() {
            if is_entry_point(item) {
                roots.insert(i);
            }
        }
    } else {
        for ep in entry_points {
            if let Some(&idx) = name_to_idx.get(ep) {
                roots.insert(idx);
            }
        }
    }

    // Step 4: BFS/DFS to mark all reachable items.
    let mut reachable: HashSet<usize> = HashSet::new();
    let mut worklist: Vec<usize> = roots.into_iter().collect();

    while let Some(idx) = worklist.pop() {
        if !reachable.insert(idx) {
            continue;
        }
        if let Some(referenced) = deps.get(&idx) {
            for name in referenced {
                if let Some(&dep_idx) = name_to_idx.get(name) {
                    if !reachable.contains(&dep_idx) {
                        worklist.push(dep_idx);
                    }
                }
            }
        }
    }

    // Also keep Use items (imports) and Test items always.
    for (i, item) in program.items.iter().enumerate() {
        if matches!(item, Item::Use(_) | Item::Test(_) | Item::Contract(_) | Item::App(_) | Item::Page(_) | Item::Form(_) | Item::Channel(_) | Item::Embed(_) | Item::Pdf(_) | Item::Payment(_) | Item::Auth(_) | Item::Upload(_) | Item::Db(_) | Item::Cache(_)) {
            // Always keep these
            reachable.insert(i);
        }
    }

    // Step 5: Remove unreachable items.
    let total = program.items.len();
    let mut removed_names = Vec::new();
    let mut keep_indices: Vec<bool> = vec![false; total];
    for i in 0..total {
        if reachable.contains(&i) {
            keep_indices[i] = true;
        } else {
            if let Some(name) = item_name(&program.items[i]) {
                removed_names.push(name);
            }
        }
    }

    let mut idx = 0;
    program.items.retain(|_| {
        let keep = keep_indices[idx];
        idx += 1;
        keep
    });

    stats.items_removed = total - program.items.len();
    stats.removed_names = removed_names;
}

/// Get the name of a top-level item, if it has one.
fn item_name(item: &Item) -> Option<String> {
    match item {
        Item::Function(f) => Some(f.name.clone()),
        Item::Component(c) => Some(c.name.clone()),
        Item::Struct(s) => Some(s.name.clone()),
        Item::Enum(e) => Some(e.name.clone()),
        Item::Impl(i) => Some(i.target.clone()),
        Item::Store(s) => Some(s.name.clone()),
        Item::Agent(a) => Some(a.name.clone()),
        Item::Router(r) => Some(r.name.clone()),
        Item::LazyComponent(l) => Some(l.component.name.clone()),
        Item::Use(_) => None,
        Item::Test(_) => None,
        Item::Contract(c) => Some(c.name.clone()),
        Item::App(a) => Some(a.name.clone()),
        Item::Trait(t) => Some(t.name.clone()),
        Item::Page(p) => Some(p.name.clone()),
        Item::Form(f) => Some(f.name.clone()),
        Item::Channel(ch) => Some(ch.name.clone()),
        Item::Mod(m) => Some(m.name.clone()),
        Item::Embed(e) => Some(e.name.clone()),
        Item::Pdf(p) => Some(p.name.clone()),
        Item::Payment(p) => Some(p.name.clone()),
        Item::Auth(a) => Some(a.name.clone()),
        Item::Upload(u) => Some(u.name.clone()),
        Item::Db(d) => Some(d.name.clone()),
        Item::Cache(c) => Some(c.name.clone()),
    }
}

/// Determine if an item is an entry point (always kept).
fn is_entry_point(item: &Item) -> bool {
    match item {
        Item::Function(f) => f.is_pub,
        Item::Component(_) => true,     // components are entry points
        Item::Router(_) => true,        // routers are entry points
        Item::Store(s) => s.is_pub,
        Item::Agent(_) => true,
        Item::LazyComponent(_) => true,
        Item::Page(_) => true,         // pages are entry points
        _ => false,
    }
}

/// Collect all names referenced by an item (dependency edges).
fn collect_item_deps(item: &Item, deps: &mut HashSet<String>) {
    match item {
        Item::Function(f) => {
            collect_block_deps(&f.body, deps);
            collect_type_deps_from_params(&f.params, deps);
            if let Some(ref rt) = f.return_type {
                collect_type_deps(rt, deps);
            }
        }
        Item::Component(c) => {
            for prop in &c.props {
                collect_type_deps(&prop.ty, deps);
                if let Some(ref d) = prop.default {
                    collect_expr_deps(d, deps);
                }
            }
            for state in &c.state {
                collect_expr_deps(&state.initializer, deps);
            }
            for method in &c.methods {
                collect_block_deps(&method.body, deps);
            }
            collect_template_deps(&c.render.body, deps);
        }
        Item::Impl(imp) => {
            deps.insert(imp.target.clone());
            for method in &imp.methods {
                collect_block_deps(&method.body, deps);
            }
        }
        Item::Struct(s) => {
            for field in &s.fields {
                collect_type_deps(&field.ty, deps);
            }
        }
        Item::Enum(e) => {
            for variant in &e.variants {
                for ty in &variant.fields {
                    collect_type_deps(ty, deps);
                }
            }
        }
        Item::Store(store) => {
            for action in &store.actions {
                collect_block_deps(&action.body, deps);
            }
            for computed in &store.computed {
                collect_block_deps(&computed.body, deps);
            }
            for effect in &store.effects {
                collect_block_deps(&effect.body, deps);
            }
        }
        Item::Agent(agent) => {
            for method in &agent.methods {
                collect_block_deps(&method.body, deps);
            }
            for tool in &agent.tools {
                collect_block_deps(&tool.body, deps);
            }
        }
        Item::Router(router) => {
            for route in &router.routes {
                deps.insert(route.component.clone());
                if let Some(ref guard) = route.guard {
                    collect_expr_deps(guard, deps);
                }
            }
        }
        Item::LazyComponent(l) => {
            // Treat like a component
            for method in &l.component.methods {
                collect_block_deps(&method.body, deps);
            }
            collect_template_deps(&l.component.render.body, deps);
        }
        Item::Page(page) => {
            for state in &page.state {
                collect_expr_deps(&state.initializer, deps);
            }
            for method in &page.methods {
                collect_block_deps(&method.body, deps);
            }
            collect_template_deps(&page.render.body, deps);
        }
        Item::Form(form) => {
            for method in &form.methods {
                collect_block_deps(&method.body, deps);
            }
        }
        Item::Use(_) | Item::Test(_) | Item::Trait(_) | Item::Mod(_) => {}
            Item::Contract(_) => {}
            Item::Embed(_) => {}
            Item::Pdf(pdf) => {
                // Pdf render blocks can reference components
            }
            Item::Payment(_) => {}
            Item::Auth(_) => {}
            Item::Upload(u) => {
                collect_expr_deps(&u.endpoint, deps);
            }
            Item::Db(_) => {}
            Item::Cache(_) => {}
            Item::App(app) => {
                if let Some(ref router) = app.router {
                    for route in &router.routes {
                        deps.insert(route.component.clone());
                    }
                }
            }
            Item::Channel(ch) => {
                collect_expr_deps(&ch.url, deps);
                if let Some(ref contract) = ch.contract {
                    deps.insert(contract.clone());
                }
                for method in &ch.methods {
                    collect_block_deps(&method.body, deps);
                }
                if let Some(ref handler) = ch.on_message {
                    collect_block_deps(&handler.body, deps);
                }
                if let Some(ref handler) = ch.on_connect {
                    collect_block_deps(&handler.body, deps);
                }
                if let Some(ref handler) = ch.on_disconnect {
                    collect_block_deps(&handler.body, deps);
                }
            }
    }
}

fn collect_type_deps_from_params(params: &[Param], deps: &mut HashSet<String>) {
    for p in params {
        collect_type_deps(&p.ty, deps);
    }
}

fn collect_type_deps(ty: &Type, deps: &mut HashSet<String>) {
    match ty {
        Type::Named(name) => { deps.insert(name.clone()); }
        Type::Generic { name, args } => {
            deps.insert(name.clone());
            for arg in args { collect_type_deps(arg, deps); }
        }
        Type::Reference { inner, .. } => collect_type_deps(inner, deps),
        Type::Array(inner) => collect_type_deps(inner, deps),
        Type::Option(inner) => collect_type_deps(inner, deps),
        Type::Tuple(items) => {
            for t in items { collect_type_deps(t, deps); }
        }
        Type::Function { params, ret } => {
            for p in params { collect_type_deps(p, deps); }
            collect_type_deps(ret, deps);
        }
        _ => {}
    }
}

fn collect_block_deps(block: &Block, deps: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_stmt_deps(stmt, deps);
    }
}

fn collect_stmt_deps(stmt: &Stmt, deps: &mut HashSet<String>) {
    match stmt {
        Stmt::Let { value, ty, .. } => {
            collect_expr_deps(value, deps);
            if let Some(t) = ty { collect_type_deps(t, deps); }
        }
        Stmt::Signal { value, ty, .. } => {
            collect_expr_deps(value, deps);
            if let Some(t) = ty { collect_type_deps(t, deps); }
        }
        Stmt::Expr(expr) => collect_expr_deps(expr, deps),
        Stmt::Return(Some(expr)) => collect_expr_deps(expr, deps),
        Stmt::Return(None) => {}
        Stmt::Yield(expr) => collect_expr_deps(expr, deps),
        _ => {}
    }
}

fn collect_expr_deps(expr: &Expr, deps: &mut HashSet<String>) {
    match expr {
        Expr::Ident(name) => { deps.insert(name.clone()); }
        Expr::Binary { left, right, .. } => {
            collect_expr_deps(left, deps);
            collect_expr_deps(right, deps);
        }
        Expr::Unary { operand, .. } => collect_expr_deps(operand, deps),
        Expr::FnCall { callee, args, .. } => {
            collect_expr_deps(callee, deps);
            for a in args { collect_expr_deps(a, deps); }
        }
        Expr::MethodCall { object, args, .. } => {
            collect_expr_deps(object, deps);
            for a in args { collect_expr_deps(a, deps); }
        }
        Expr::FieldAccess { object, .. } => collect_expr_deps(object, deps),
        Expr::Index { object, index, .. } => {
            collect_expr_deps(object, deps);
            collect_expr_deps(index, deps);
        }
        Expr::If { condition, then_block, else_block, .. } => {
            collect_expr_deps(condition, deps);
            collect_block_deps(then_block, deps);
            if let Some(eb) = else_block { collect_block_deps(eb, deps); }
        }
        Expr::Block(block) => collect_block_deps(block, deps),
        Expr::For { iterator, body, .. } => {
            collect_expr_deps(iterator, deps);
            collect_block_deps(body, deps);
        }
        Expr::While { condition, body, .. } => {
            collect_expr_deps(condition, deps);
            collect_block_deps(body, deps);
        }
        Expr::Assign { target, value, .. } => {
            collect_expr_deps(target, deps);
            collect_expr_deps(value, deps);
        }
        Expr::Closure { body, .. } => collect_expr_deps(body, deps),
        Expr::StructInit { name, fields, .. } => {
            deps.insert(name.clone());
            for (_, v) in fields { collect_expr_deps(v, deps); }
        }
        Expr::Match { subject, arms, .. } => {
            collect_expr_deps(subject, deps);
            for arm in arms {
                collect_expr_deps(&arm.body, deps);
                collect_pattern_deps(&arm.pattern, deps);
            }
        }
        Expr::Borrow(e) | Expr::BorrowMut(e) | Expr::Await(e)
        | Expr::Stream { source: e } | Expr::Navigate { path: e }
        | Expr::Receive { channel: e } => {
            collect_expr_deps(e, deps);
        }
        Expr::Spawn { body, .. } => {
            collect_block_deps(body, deps);
        }
        Expr::Send { channel, value } => {
            collect_expr_deps(channel, deps);
            collect_expr_deps(value, deps);
        }
        Expr::Suspend { fallback, body } => {
            collect_expr_deps(fallback, deps);
            collect_expr_deps(body, deps);
        }
        Expr::TryCatch { body, catch_body, .. } => {
            collect_expr_deps(body, deps);
            collect_expr_deps(catch_body, deps);
        }
        Expr::Fetch { url, options, .. } => {
            collect_expr_deps(url, deps);
            if let Some(opts) = options { collect_expr_deps(opts, deps); }
        }
        Expr::Parallel { tasks, .. } => {
            for e in tasks { collect_expr_deps(e, deps); }
        }
        Expr::PromptTemplate { interpolations, .. } => {
            for (_, e) in interpolations { collect_expr_deps(e, deps); }
        }
        Expr::Env { name, .. } => {
            collect_expr_deps(name, deps);
        }
        Expr::Trace { label, body, .. } => {
            collect_expr_deps(label, deps);
            collect_block_deps(body, deps);
        }
        Expr::Flag { name, .. } => {
            collect_expr_deps(name, deps);
        }
        _ => {}
    }
}

fn collect_pattern_deps(pattern: &Pattern, deps: &mut HashSet<String>) {
    match pattern {
        Pattern::Variant { name, fields } => {
            deps.insert(name.clone());
            for f in fields { collect_pattern_deps(f, deps); }
        }
        Pattern::Ident(name) => { deps.insert(name.clone()); }
        _ => {}
    }
}

fn collect_template_deps(node: &TemplateNode, deps: &mut HashSet<String>) {
    match node {
        TemplateNode::Element(el) => {
            // If the tag starts uppercase, it's a component reference
            if el.tag.chars().next().is_some_and(|c| c.is_uppercase()) {
                deps.insert(el.tag.clone());
            }
            for attr in &el.attributes {
                match attr {
                    Attribute::Dynamic { value, .. }
                    | Attribute::EventHandler { handler: value, .. }
                    | Attribute::Aria { value, .. } => {
                        collect_expr_deps(value, deps);
                    }
                    _ => {}
                }
            }
            for child in &el.children {
                collect_template_deps(child, deps);
            }
        }
        TemplateNode::Expression(expr) => collect_expr_deps(expr, deps),
        TemplateNode::Fragment(children) => {
            for child in children { collect_template_deps(child, deps); }
        }
        TemplateNode::Link { to, children } => {
            collect_expr_deps(to, deps);
            for child in children { collect_template_deps(child, deps); }
        }
        TemplateNode::TextLiteral(_) => {}
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

    fn item_names(program: &Program) -> Vec<String> {
        program.items.iter().filter_map(|item| item_name(item)).collect()
    }

    #[test]
    fn test_shake_removes_unreachable_function() {
        let items = vec![
            make_fn("main", true, vec![Stmt::Expr(Expr::Integer(1))]),
            make_fn("unused_helper", false, vec![Stmt::Expr(Expr::Integer(2))]),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);

        let names = item_names(&program);
        assert!(names.contains(&"main".to_string()));
        assert!(!names.contains(&"unused_helper".to_string()));
        assert_eq!(stats.items_removed, 1);
    }

    #[test]
    fn test_shake_keeps_transitive_dependency() {
        // main -> helper -> deep_helper
        let items = vec![
            make_fn("main", true, vec![
                Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("helper".to_string())),
                    args: vec![],
                }),
            ]),
            make_fn("helper", false, vec![
                Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("deep_helper".to_string())),
                    args: vec![],
                }),
            ]),
            make_fn("deep_helper", false, vec![
                Stmt::Return(Some(Expr::Integer(42))),
            ]),
            make_fn("totally_unused", false, vec![
                Stmt::Expr(Expr::Integer(0)),
            ]),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);

        let names = item_names(&program);
        assert!(names.contains(&"main".to_string()));
        assert!(names.contains(&"helper".to_string()));
        assert!(names.contains(&"deep_helper".to_string()));
        assert!(!names.contains(&"totally_unused".to_string()));
        assert_eq!(stats.items_removed, 1);
    }

    #[test]
    fn test_shake_with_explicit_entry_points() {
        let items = vec![
            make_fn("foo", false, vec![Stmt::Expr(Expr::Integer(1))]),
            make_fn("bar", false, vec![Stmt::Expr(Expr::Integer(2))]),
            make_fn("baz", false, vec![Stmt::Expr(Expr::Integer(3))]),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &["foo".to_string()], &mut stats);

        let names = item_names(&program);
        assert!(names.contains(&"foo".to_string()));
        assert!(!names.contains(&"bar".to_string()));
        assert!(!names.contains(&"baz".to_string()));
        assert_eq!(stats.items_removed, 2);
    }

    #[test]
    fn test_shake_keeps_struct_used_by_function() {
        let items = vec![
            make_fn("main", true, vec![
                Stmt::Expr(Expr::StructInit {
                    name: "Point".to_string(),
                    fields: vec![
                        ("x".to_string(), Expr::Integer(1)),
                        ("y".to_string(), Expr::Integer(2)),
                    ],
                }),
            ]),
            Item::Struct(StructDef {
                name: "Point".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                fields: vec![
                    Field { name: "x".to_string(), ty: Type::Named("i32".to_string()), is_pub: true },
                    Field { name: "y".to_string(), ty: Type::Named("i32".to_string()), is_pub: true },
                ],
                trait_bounds: vec![],
                is_pub: false,
                span: dummy_span(),
            }),
            Item::Struct(StructDef {
                name: "UnusedStruct".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                fields: vec![],
                trait_bounds: vec![],
                is_pub: false,
                span: dummy_span(),
            }),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);

        let names = item_names(&program);
        assert!(names.contains(&"Point".to_string()));
        assert!(!names.contains(&"UnusedStruct".to_string()));
    }

    #[test]
    fn test_shake_reports_removed_names() {
        let items = vec![
            make_fn("main", true, vec![]),
            make_fn("dead_a", false, vec![]),
            make_fn("dead_b", false, vec![]),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);

        assert_eq!(stats.items_removed, 2);
        assert!(stats.removed_names.contains(&"dead_a".to_string()));
        assert!(stats.removed_names.contains(&"dead_b".to_string()));
    }
}
