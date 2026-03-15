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
        if matches!(item, Item::Use(_) | Item::Test(_) | Item::Contract(_) | Item::App(_) | Item::Page(_) | Item::Form(_) | Item::Channel(_) | Item::Embed(_) | Item::Pdf(_) | Item::Payment(_) | Item::Banking(_) | Item::Map(_) | Item::Auth(_) | Item::Upload(_) | Item::Db(_) | Item::Cache(_) | Item::Breakpoints(_) | Item::Theme(_) | Item::Animation(_)) {
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
        Item::Banking(b) => Some(b.name.clone()),
        Item::Map(m) => Some(m.name.clone()),
        Item::Auth(a) => Some(a.name.clone()),
        Item::Upload(u) => Some(u.name.clone()),
        Item::Db(d) => Some(d.name.clone()),
        Item::Cache(c) => Some(c.name.clone()),
        Item::Breakpoints(_) => None,
        Item::Theme(t) => Some(t.name.clone()),
        Item::Animation(a) => Some(a.name.clone()),
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
            Item::Pdf(_pdf) => {
                // Pdf render blocks can reference components
            }
            Item::Payment(_) => {}
            Item::Banking(_) => {}
            Item::Map(_) => {}
            Item::Auth(_) => {}
            Item::Upload(u) => {
                collect_expr_deps(&u.endpoint, deps);
            }
            Item::Db(_) => {}
            Item::Cache(_) => {}
            Item::Breakpoints(_) => {}
            Item::Theme(_) => {}
            Item::Animation(_) => {}
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
                if let Some(guard) = &arm.guard {
                    collect_expr_deps(guard, deps);
                }
                collect_expr_deps(&arm.body, deps);
                collect_pattern_deps(&arm.pattern, deps);
            }
        }
        Expr::ArrayLit(elements) => {
            for e in elements { collect_expr_deps(e, deps); }
        }
        Expr::ObjectLit { fields } => {
            for (_, v) in fields { collect_expr_deps(v, deps); }
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
        TemplateNode::Link { to, attributes, children } => {
            collect_expr_deps(to, deps);
            for attr in attributes {
                match attr {
                    Attribute::Dynamic { value, .. }
                    | Attribute::Aria { value, .. }
                    | Attribute::EventHandler { handler: value, .. } => {
                        collect_expr_deps(value, deps);
                    }
                    _ => {}
                }
            }
            for child in children { collect_template_deps(child, deps); }
        }
        TemplateNode::TextLiteral(_) => {}
        TemplateNode::Outlet => {}
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
                collect_template_deps(child, deps);
            }
        }
        TemplateNode::TemplateIf { condition, then_children, else_children } => {
            collect_expr_deps(condition, deps);
            for child in then_children {
                collect_template_deps(child, deps);
            }
            if let Some(else_nodes) = else_children {
                for child in else_nodes {
                    collect_template_deps(child, deps);
                }
            }
        }
        TemplateNode::TemplateFor { iterator, children, .. } => {
            collect_expr_deps(iterator, deps);
            for child in children {
                collect_template_deps(child, deps);
            }
        }
        TemplateNode::TemplateMatch { subject, arms } => {
            collect_expr_deps(subject, deps);
            for arm in arms {
                for child in &arm.body {
                    collect_template_deps(child, deps);
                }
            }
        }
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
            is_async: false,
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

    // --- Multiple entry points ---

    #[test]
    fn test_shake_multiple_entry_points() {
        let items = vec![
            make_fn("a", false, vec![Stmt::Expr(Expr::Integer(1))]),
            make_fn("b", false, vec![Stmt::Expr(Expr::Integer(2))]),
            make_fn("c", false, vec![Stmt::Expr(Expr::Integer(3))]),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &["a".to_string(), "c".to_string()], &mut stats);

        let names = item_names(&program);
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"c".to_string()));
        assert!(!names.contains(&"b".to_string()));
        assert_eq!(stats.items_removed, 1);
    }

    // --- Deeply nested transitive deps ---

    #[test]
    fn test_shake_deeply_nested_transitive_deps() {
        // main -> a -> b -> c -> d
        let items = vec![
            make_fn("main", true, vec![
                Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("a".to_string())),
                    args: vec![],
                }),
            ]),
            make_fn("a", false, vec![
                Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("b".to_string())),
                    args: vec![],
                }),
            ]),
            make_fn("b", false, vec![
                Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("c".to_string())),
                    args: vec![],
                }),
            ]),
            make_fn("c", false, vec![
                Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("d".to_string())),
                    args: vec![],
                }),
            ]),
            make_fn("d", false, vec![Stmt::Return(Some(Expr::Integer(42)))]),
            make_fn("orphan", false, vec![Stmt::Expr(Expr::Integer(0))]),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);

        let names = item_names(&program);
        assert!(names.contains(&"main".to_string()));
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"b".to_string()));
        assert!(names.contains(&"c".to_string()));
        assert!(names.contains(&"d".to_string()));
        assert!(!names.contains(&"orphan".to_string()));
        assert_eq!(stats.items_removed, 1);
    }

    // --- Struct + impl shaking ---

    #[test]
    fn test_shake_struct_with_impl() {
        let items = vec![
            make_fn("main", true, vec![
                Stmt::Expr(Expr::StructInit {
                    name: "Foo".to_string(),
                    fields: vec![("x".to_string(), Expr::Integer(1))],
                }),
            ]),
            Item::Struct(StructDef {
                name: "Foo".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                fields: vec![Field {
                    name: "x".to_string(),
                    ty: Type::Named("i32".to_string()),
                    is_pub: true,
                }],
                trait_bounds: vec![],
                is_pub: false,
                span: dummy_span(),
            }),
            Item::Impl(ImplBlock {
                target: "Foo".to_string(),
                trait_impls: vec![],
                methods: vec![],
                span: dummy_span(),
            }),
            Item::Struct(StructDef {
                name: "Bar".to_string(),
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
        assert!(names.contains(&"Foo".to_string()));
        assert!(names.contains(&"Foo".to_string())); // impl target name
        assert!(!names.contains(&"Bar".to_string()));
    }

    // --- Enum shaking ---

    #[test]
    fn test_shake_removes_unused_enum() {
        let items = vec![
            make_fn("main", true, vec![Stmt::Expr(Expr::Integer(1))]),
            Item::Enum(EnumDef {
                name: "Color".to_string(),
                type_params: vec![],
                variants: vec![
                    Variant { name: "Red".to_string(), fields: vec![] },
                ],
                is_pub: false,
                span: dummy_span(),
            }),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);

        let names = item_names(&program);
        assert!(!names.contains(&"Color".to_string()));
        assert_eq!(stats.items_removed, 1);
    }

    #[test]
    fn test_shake_keeps_used_enum() {
        // The tree shaker tracks deps by name. Pattern::Variant inserts the
        // variant name ("Red"), not the enum name. So to keep the enum, we
        // reference it directly via Ident("Color") or through a function
        // param type. Here we use a type annotation that references Color.
        let items = vec![
            make_fn("main", true, vec![
                Stmt::Let {
                    name: "c".to_string(),
                    ty: Some(Type::Named("Color".to_string())),
                    mutable: false,
                    secret: false,
                    value: Expr::Ident("Color".to_string()),
                    ownership: Ownership::Owned,
                },
            ]),
            Item::Enum(EnumDef {
                name: "Color".to_string(),
                type_params: vec![],
                variants: vec![
                    Variant { name: "Red".to_string(), fields: vec![] },
                ],
                is_pub: false,
                span: dummy_span(),
            }),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);

        let names = item_names(&program);
        assert!(names.contains(&"Color".to_string()));
    }

    // --- Unused imports (Use items are always kept) ---

    #[test]
    fn test_shake_keeps_use_items() {
        let items = vec![
            make_fn("main", true, vec![Stmt::Expr(Expr::Integer(1))]),
            Item::Use(UsePath {
                segments: vec!["std".to_string(), "io".to_string()],
                alias: None,
                glob: false,
                group: None,
                span: dummy_span(),
            }),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);
        // Use items are always kept
        assert_eq!(program.items.len(), 2);
    }

    // --- Circular function references ---

    #[test]
    fn test_shake_circular_functions() {
        // main -> a, a -> b, b -> a (circular)
        let items = vec![
            make_fn("main", true, vec![
                Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("a".to_string())),
                    args: vec![],
                }),
            ]),
            make_fn("a", false, vec![
                Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("b".to_string())),
                    args: vec![],
                }),
            ]),
            make_fn("b", false, vec![
                Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("a".to_string())),
                    args: vec![],
                }),
            ]),
            make_fn("orphan", false, vec![Stmt::Expr(Expr::Integer(0))]),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);

        let names = item_names(&program);
        assert!(names.contains(&"main".to_string()));
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"b".to_string()));
        assert!(!names.contains(&"orphan".to_string()));
        assert_eq!(stats.items_removed, 1);
    }

    // --- Components as entry points ---

    #[test]
    fn test_shake_component_is_entry_point() {
        let items = vec![
            Item::Component(Component {
                name: "App".to_string(),
                type_params: vec![],
                props: vec![],
                state: vec![],
                methods: vec![],
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
            }),
            make_fn("orphan", false, vec![Stmt::Expr(Expr::Integer(0))]),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);

        let names = item_names(&program);
        assert!(names.contains(&"App".to_string()));
        assert!(!names.contains(&"orphan".to_string()));
    }

    // --- Empty program ---

    #[test]
    fn test_shake_empty_program() {
        let mut program = Program { items: vec![] };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);
        assert_eq!(stats.items_removed, 0);
    }

    // --- item_name coverage for all Item types ---

    fn make_component(name: &str) -> Component {
        Component {
            name: name.to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
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
        }
    }

    #[test]
    fn test_item_name_all_variants() {
        // Store
        assert_eq!(item_name(&Item::Store(StoreDef {
            name: "S".to_string(), signals: vec![], actions: vec![],
            computed: vec![], effects: vec![], selectors: vec![],
            is_pub: false, span: dummy_span(),
        })), Some("S".to_string()));

        // Agent
        assert_eq!(item_name(&Item::Agent(AgentDef {
            name: "A".to_string(), system_prompt: None, tools: vec![],
            state: vec![], methods: vec![], render: None, span: dummy_span(),
        })), Some("A".to_string()));

        // Router
        assert_eq!(item_name(&Item::Router(RouterDef {
            name: "R".to_string(), routes: vec![], fallback: None, layout: None, transition: None, span: dummy_span(),
        })), Some("R".to_string()));

        // LazyComponent
        assert_eq!(item_name(&Item::LazyComponent(LazyComponentDef {
            component: make_component("Lz"), span: dummy_span(),
        })), Some("Lz".to_string()));

        // Contract
        assert_eq!(item_name(&Item::Contract(ContractDef {
            name: "C".to_string(), fields: vec![], is_pub: false, span: dummy_span(),
        })), Some("C".to_string()));

        // App
        assert_eq!(item_name(&Item::App(AppDef {
            name: "Ap".to_string(), manifest: None, offline: None,
            push: None, router: None, a11y: None, is_pub: false, span: dummy_span(),
        })), Some("Ap".to_string()));

        // Trait
        assert_eq!(item_name(&Item::Trait(TraitDef {
            name: "T".to_string(), methods: vec![], type_params: vec![], span: dummy_span(),
        })), Some("T".to_string()));

        // Page
        assert_eq!(item_name(&Item::Page(PageDef {
            name: "P".to_string(), props: vec![], meta: None, state: vec![],
            methods: vec![], styles: vec![], render: RenderBlock {
                body: TemplateNode::Fragment(vec![]), span: dummy_span(),
            }, permissions: None, gestures: vec![], is_pub: false, span: dummy_span(),
        })), Some("P".to_string()));

        // Form
        assert_eq!(item_name(&Item::Form(FormDef {
            name: "F".to_string(), fields: vec![], on_submit: None, steps: vec![],
            methods: vec![], styles: vec![], render: None, is_pub: false, span: dummy_span(),
        })), Some("F".to_string()));

        // Channel
        assert_eq!(item_name(&Item::Channel(ChannelDef {
            name: "Ch".to_string(), url: Expr::StringLit("/ws".to_string()),
            provider: None, contract: None, on_message: None, on_connect: None, on_disconnect: None,
            reconnect: false, heartbeat_interval: None, methods: vec![],
            is_pub: false, span: dummy_span(),
        })), Some("Ch".to_string()));

        // Mod
        assert_eq!(item_name(&Item::Mod(ModDef {
            name: "M".to_string(), items: None, is_external: true, span: dummy_span(),
        })), Some("M".to_string()));

        // Embed
        assert_eq!(item_name(&Item::Embed(EmbedDef {
            name: "E".to_string(), src: Expr::StringLit("x".to_string()),
            loading: None, sandbox: false, integrity: None, permissions: None,
            is_pub: false, span: dummy_span(),
        })), Some("E".to_string()));

        // Pdf
        assert_eq!(item_name(&Item::Pdf(PdfDef {
            name: "Pd".to_string(), render: RenderBlock {
                body: TemplateNode::Fragment(vec![]), span: dummy_span(),
            }, page_size: None, orientation: None, margins: None,
            is_pub: false, span: dummy_span(),
        })), Some("Pd".to_string()));

        // Payment
        assert_eq!(item_name(&Item::Payment(PaymentDef {
            name: "Py".to_string(), provider: None, public_key: None,
            sandbox_mode: false, on_success: None, on_error: None,
            methods: vec![], is_pub: false, span: dummy_span(),
        })), Some("Py".to_string()));

        // Banking
        assert_eq!(item_name(&Item::Banking(BankingDef {
            name: "Bk".to_string(), provider: None,
            on_success: None, on_exit: None, on_error: None,
            methods: vec![], is_pub: false, span: dummy_span(),
        })), Some("Bk".to_string()));

        // Map
        assert_eq!(item_name(&Item::Map(MapDef {
            name: "Mp".to_string(), provider: None,
            center: None, zoom: None, style: None,
            on_ready: None, on_click: None,
            methods: vec![], is_pub: false, span: dummy_span(),
        })), Some("Mp".to_string()));

        // Auth
        assert_eq!(item_name(&Item::Auth(AuthDef {
            name: "Au".to_string(), provider: None, providers: vec![],
            on_login: None, on_logout: None, on_error: None,
            session_storage: None, methods: vec![], is_pub: false, span: dummy_span(),
        })), Some("Au".to_string()));

        // Upload
        assert_eq!(item_name(&Item::Upload(UploadDef {
            name: "Up".to_string(), endpoint: Expr::StringLit("/up".to_string()),
            max_size: None, accept: vec![], chunked: false,
            on_progress: None, on_complete: None, on_error: None,
            methods: vec![], is_pub: false, span: dummy_span(),
        })), Some("Up".to_string()));

        // Db
        assert_eq!(item_name(&Item::Db(DbDef {
            name: "Db".to_string(), version: None, stores: vec![],
            is_pub: false, span: dummy_span(),
        })), Some("Db".to_string()));

        // Cache
        assert_eq!(item_name(&Item::Cache(CacheDef {
            name: "Ca".to_string(), strategy: None, default_ttl: None,
            persist: false, max_entries: None, queries: vec![],
            mutations: vec![], is_pub: false, span: dummy_span(),
        })), Some("Ca".to_string()));

        // Breakpoints - returns None
        assert_eq!(item_name(&Item::Breakpoints(BreakpointsDef {
            breakpoints: vec![], span: dummy_span(),
        })), None);

        // Theme
        assert_eq!(item_name(&Item::Theme(ThemeDef {
            name: "Th".to_string(), light: None, dark: None,
            dark_auto: false, primary: None, is_pub: false, span: dummy_span(),
        })), Some("Th".to_string()));

        // Animation
        assert_eq!(item_name(&Item::Animation(AnimationBlockDef {
            name: "An".to_string(), kind: AnimationKind::Spring {
                stiffness: None, damping: None, mass: None, properties: vec![],
            }, is_pub: false, span: dummy_span(),
        })), Some("An".to_string()));
    }

    // --- is_entry_point coverage ---

    #[test]
    fn test_is_entry_point_store_pub() {
        let item = Item::Store(StoreDef {
            name: "S".to_string(), signals: vec![], actions: vec![],
            computed: vec![], effects: vec![], selectors: vec![],
            is_pub: true, span: dummy_span(),
        });
        assert!(is_entry_point(&item));
    }

    #[test]
    fn test_is_entry_point_store_private() {
        let item = Item::Store(StoreDef {
            name: "S".to_string(), signals: vec![], actions: vec![],
            computed: vec![], effects: vec![], selectors: vec![],
            is_pub: false, span: dummy_span(),
        });
        assert!(!is_entry_point(&item));
    }

    #[test]
    fn test_is_entry_point_agent() {
        let item = Item::Agent(AgentDef {
            name: "A".to_string(), system_prompt: None, tools: vec![],
            state: vec![], methods: vec![], render: None, span: dummy_span(),
        });
        assert!(is_entry_point(&item));
    }

    #[test]
    fn test_is_entry_point_lazy_component() {
        let item = Item::LazyComponent(LazyComponentDef {
            component: make_component("L"), span: dummy_span(),
        });
        assert!(is_entry_point(&item));
    }

    #[test]
    fn test_is_entry_point_page() {
        let item = Item::Page(PageDef {
            name: "P".to_string(), props: vec![], meta: None, state: vec![],
            methods: vec![], styles: vec![], render: RenderBlock {
                body: TemplateNode::Fragment(vec![]), span: dummy_span(),
            }, permissions: None, gestures: vec![], is_pub: false, span: dummy_span(),
        });
        assert!(is_entry_point(&item));
    }

    #[test]
    fn test_is_entry_point_router() {
        let item = Item::Router(RouterDef {
            name: "R".to_string(), routes: vec![], fallback: None, layout: None, transition: None, span: dummy_span(),
        });
        assert!(is_entry_point(&item));
    }

    #[test]
    fn test_is_entry_point_struct_is_false() {
        let item = Item::Struct(StructDef {
            name: "S".to_string(), lifetimes: vec![], type_params: vec![],
            fields: vec![], trait_bounds: vec![], is_pub: false, span: dummy_span(),
        });
        assert!(!is_entry_point(&item));
    }

    // --- collect_item_deps for Store ---

    #[test]
    fn test_collect_deps_store() {
        let store = Item::Store(StoreDef {
            name: "S".to_string(),
            signals: vec![],
            actions: vec![ActionDef {
                name: "inc".to_string(), params: vec![],
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("helper".to_string()))], span: dummy_span() },
                is_async: false, span: dummy_span(),
            }],
            computed: vec![ComputedDef {
                name: "dbl".to_string(), return_type: None,
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("comp_dep".to_string()))], span: dummy_span() },
                span: dummy_span(),
            }],
            effects: vec![EffectDef {
                name: "log".to_string(),
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("eff_dep".to_string()))], span: dummy_span() },
                span: dummy_span(),
            }],
            selectors: vec![], is_pub: false, span: dummy_span(),
        });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&store, &mut deps);
        assert!(deps.contains("helper"));
        assert!(deps.contains("comp_dep"));
        assert!(deps.contains("eff_dep"));
    }

    // --- collect_item_deps for Agent ---

    #[test]
    fn test_collect_deps_agent() {
        let agent = Item::Agent(AgentDef {
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
                is_pub: false, is_async: false, must_use: false, span: dummy_span(),
            }],
            render: None, span: dummy_span(),
        });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&agent, &mut deps);
        assert!(deps.contains("tool_dep"));
        assert!(deps.contains("meth_dep"));
    }

    // --- collect_item_deps for Router ---

    #[test]
    fn test_collect_deps_router() {
        let router = Item::Router(RouterDef {
            name: "R".to_string(),
            routes: vec![RouteDef {
                path: "/".to_string(), params: vec![], component: "Home".to_string(),
                guard: Some(Expr::Ident("is_auth".to_string())), transition: None, span: dummy_span(),
            }],
            fallback: None, layout: None, transition: None, span: dummy_span(),
        });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&router, &mut deps);
        assert!(deps.contains("Home"));
        assert!(deps.contains("is_auth"));
    }

    // --- collect_item_deps for LazyComponent ---

    #[test]
    fn test_collect_deps_lazy_component() {
        let mut c = make_component("Lz");
        c.methods = vec![Function {
            name: "m".to_string(), lifetimes: vec![], type_params: vec![],
            params: vec![], return_type: None, trait_bounds: vec![],
            body: Block { stmts: vec![Stmt::Expr(Expr::Ident("lazy_dep".to_string()))], span: dummy_span() },
            is_pub: false, is_async: false, must_use: false, span: dummy_span(),
        }];
        let item = Item::LazyComponent(LazyComponentDef { component: c, span: dummy_span() });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&item, &mut deps);
        assert!(deps.contains("lazy_dep"));
    }

    // --- collect_item_deps for Page ---

    #[test]
    fn test_collect_deps_page() {
        let page = Item::Page(PageDef {
            name: "P".to_string(), props: vec![], meta: None,
            state: vec![StateField {
                name: "s".to_string(), ty: None, mutable: false, secret: false,
                atomic: false, initializer: Expr::Ident("init_dep".to_string()),
                ownership: Ownership::Owned,
            }],
            methods: vec![Function {
                name: "m".to_string(), lifetimes: vec![], type_params: vec![],
                params: vec![], return_type: None, trait_bounds: vec![],
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("page_dep".to_string()))], span: dummy_span() },
                is_pub: false, is_async: false, must_use: false, span: dummy_span(),
            }],
            styles: vec![], render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "MyComp".to_string(),
                    attributes: vec![], children: vec![],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }, permissions: None, gestures: vec![], is_pub: false, span: dummy_span(),
        });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&page, &mut deps);
        assert!(deps.contains("init_dep"));
        assert!(deps.contains("page_dep"));
        assert!(deps.contains("MyComp"));
    }

    // --- collect_item_deps for Form ---

    #[test]
    fn test_collect_deps_form() {
        let form = Item::Form(FormDef {
            name: "F".to_string(), fields: vec![], on_submit: None, steps: vec![],
            methods: vec![Function {
                name: "m".to_string(), lifetimes: vec![], type_params: vec![],
                params: vec![], return_type: None, trait_bounds: vec![],
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("form_dep".to_string()))], span: dummy_span() },
                is_pub: false, is_async: false, must_use: false, span: dummy_span(),
            }],
            styles: vec![], render: None, is_pub: false, span: dummy_span(),
        });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&form, &mut deps);
        assert!(deps.contains("form_dep"));
    }

    // --- collect_item_deps for Channel ---

    #[test]
    fn test_collect_deps_channel() {
        let ch = Item::Channel(ChannelDef {
            name: "Ch".to_string(),
            url: Expr::Ident("url_dep".to_string()),
            provider: None,
            contract: Some("MyContract".to_string()),
            on_message: Some(Function {
                name: "on_msg".to_string(), lifetimes: vec![], type_params: vec![],
                params: vec![], return_type: None, trait_bounds: vec![],
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("msg_dep".to_string()))], span: dummy_span() },
                is_pub: false, is_async: false, must_use: false, span: dummy_span(),
            }),
            on_connect: Some(Function {
                name: "on_conn".to_string(), lifetimes: vec![], type_params: vec![],
                params: vec![], return_type: None, trait_bounds: vec![],
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("conn_dep".to_string()))], span: dummy_span() },
                is_pub: false, is_async: false, must_use: false, span: dummy_span(),
            }),
            on_disconnect: Some(Function {
                name: "on_disc".to_string(), lifetimes: vec![], type_params: vec![],
                params: vec![], return_type: None, trait_bounds: vec![],
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("disc_dep".to_string()))], span: dummy_span() },
                is_pub: false, is_async: false, must_use: false, span: dummy_span(),
            }),
            reconnect: false, heartbeat_interval: None,
            methods: vec![Function {
                name: "send".to_string(), lifetimes: vec![], type_params: vec![],
                params: vec![], return_type: None, trait_bounds: vec![],
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("ch_meth_dep".to_string()))], span: dummy_span() },
                is_pub: false, is_async: false, must_use: false, span: dummy_span(),
            }],
            is_pub: false, span: dummy_span(),
        });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&ch, &mut deps);
        assert!(deps.contains("url_dep"));
        assert!(deps.contains("MyContract"));
        assert!(deps.contains("msg_dep"));
        assert!(deps.contains("conn_dep"));
        assert!(deps.contains("disc_dep"));
        assert!(deps.contains("ch_meth_dep"));
    }

    // --- collect_item_deps for App with router ---

    #[test]
    fn test_collect_deps_app_with_router() {
        let app = Item::App(AppDef {
            name: "MyApp".to_string(), manifest: None, offline: None,
            push: None,
            router: Some(RouterDef {
                name: "R".to_string(),
                routes: vec![RouteDef {
                    path: "/".to_string(), params: vec![],
                    component: "Home".to_string(), guard: None, transition: None, span: dummy_span(),
                }],
                fallback: None, layout: None, transition: None, span: dummy_span(),
            }),
            a11y: None, is_pub: false, span: dummy_span(),
        });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&app, &mut deps);
        assert!(deps.contains("Home"));
    }

    // --- collect_item_deps for Upload ---

    #[test]
    fn test_collect_deps_upload() {
        let upload = Item::Upload(UploadDef {
            name: "Up".to_string(),
            endpoint: Expr::Ident("upload_url".to_string()),
            max_size: None, accept: vec![], chunked: false,
            on_progress: None, on_complete: None, on_error: None,
            methods: vec![], is_pub: false, span: dummy_span(),
        });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&upload, &mut deps);
        assert!(deps.contains("upload_url"));
    }

    // --- collect_item_deps for Enum with fields ---

    #[test]
    fn test_collect_deps_enum() {
        let en = Item::Enum(EnumDef {
            name: "Shape".to_string(), type_params: vec![],
            variants: vec![Variant {
                name: "Circle".to_string(),
                fields: vec![Type::Named("Radius".to_string())],
            }],
            is_pub: false, span: dummy_span(),
        });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&en, &mut deps);
        assert!(deps.contains("Radius"));
    }

    // --- collect_item_deps for Component with props, state, methods, template ---

    #[test]
    fn test_collect_deps_component_full() {
        let comp = Item::Component(Component {
            name: "C".to_string(), type_params: vec![],
            props: vec![Prop {
                name: "x".to_string(),
                ty: Type::Named("PropType".to_string()),
                default: Some(Expr::Ident("default_val".to_string())),
            }],
            state: vec![StateField {
                name: "s".to_string(), ty: None, mutable: false, secret: false,
                atomic: false, initializer: Expr::Ident("state_dep".to_string()),
                ownership: Ownership::Owned,
            }],
            methods: vec![Function {
                name: "m".to_string(), lifetimes: vec![], type_params: vec![],
                params: vec![], return_type: None, trait_bounds: vec![],
                body: Block { stmts: vec![Stmt::Expr(Expr::Ident("meth_dep".to_string()))], span: dummy_span() },
                is_pub: false, is_async: false, must_use: false, span: dummy_span(),
            }],
            styles: vec![], transitions: vec![], trait_bounds: vec![],
            render: RenderBlock {
                body: TemplateNode::Fragment(vec![]),
                span: dummy_span(),
            },
            permissions: None, gestures: vec![], skeleton: None,
            error_boundary: None, chunk: None, on_destroy: None,
            a11y: None, shortcuts: vec![], span: dummy_span(),
        });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&comp, &mut deps);
        assert!(deps.contains("PropType"));
        assert!(deps.contains("default_val"));
        assert!(deps.contains("state_dep"));
        assert!(deps.contains("meth_dep"));
    }

    // --- collect_type_deps coverage ---

    #[test]
    fn test_collect_type_deps_generic() {
        let ty = Type::Generic {
            name: "Vec".to_string(),
            args: vec![Type::Named("MyStruct".to_string())],
        };
        let mut deps = std::collections::HashSet::new();
        collect_type_deps(&ty, &mut deps);
        assert!(deps.contains("Vec"));
        assert!(deps.contains("MyStruct"));
    }

    #[test]
    fn test_collect_type_deps_reference() {
        let ty = Type::Reference {
            inner: Box::new(Type::Named("Foo".to_string())),
            mutable: false,
            lifetime: None,
        };
        let mut deps = std::collections::HashSet::new();
        collect_type_deps(&ty, &mut deps);
        assert!(deps.contains("Foo"));
    }

    #[test]
    fn test_collect_type_deps_array() {
        let ty = Type::Array(Box::new(Type::Named("Item".to_string())));
        let mut deps = std::collections::HashSet::new();
        collect_type_deps(&ty, &mut deps);
        assert!(deps.contains("Item"));
    }

    #[test]
    fn test_collect_type_deps_option() {
        let ty = Type::Option(Box::new(Type::Named("Val".to_string())));
        let mut deps = std::collections::HashSet::new();
        collect_type_deps(&ty, &mut deps);
        assert!(deps.contains("Val"));
    }

    #[test]
    fn test_collect_type_deps_tuple() {
        let ty = Type::Tuple(vec![Type::Named("A".to_string()), Type::Named("B".to_string())]);
        let mut deps = std::collections::HashSet::new();
        collect_type_deps(&ty, &mut deps);
        assert!(deps.contains("A"));
        assert!(deps.contains("B"));
    }

    #[test]
    fn test_collect_type_deps_function() {
        let ty = Type::Function {
            params: vec![Type::Named("In".to_string())],
            ret: Box::new(Type::Named("Out".to_string())),
        };
        let mut deps = std::collections::HashSet::new();
        collect_type_deps(&ty, &mut deps);
        assert!(deps.contains("In"));
        assert!(deps.contains("Out"));
    }

    // --- collect_expr_deps coverage ---

    #[test]
    fn test_collect_expr_deps_method_call() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("obj".to_string())),
            method: "do_thing".to_string(),
            args: vec![Expr::Ident("arg1".to_string())],
        };
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&expr, &mut deps);
        assert!(deps.contains("obj"));
        assert!(deps.contains("arg1"));
    }

    #[test]
    fn test_collect_expr_deps_field_access() {
        let expr = Expr::FieldAccess {
            object: Box::new(Expr::Ident("obj".to_string())),
            field: "f".to_string(),
        };
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&expr, &mut deps);
        assert!(deps.contains("obj"));
    }

    #[test]
    fn test_collect_expr_deps_index() {
        let expr = Expr::Index {
            object: Box::new(Expr::Ident("arr".to_string())),
            index: Box::new(Expr::Ident("idx".to_string())),
        };
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&expr, &mut deps);
        assert!(deps.contains("arr"));
        assert!(deps.contains("idx"));
    }

    #[test]
    fn test_collect_expr_deps_if_with_else() {
        let expr = Expr::If {
            condition: Box::new(Expr::Ident("cond".to_string())),
            then_block: Block { stmts: vec![Stmt::Expr(Expr::Ident("then_dep".to_string()))], span: dummy_span() },
            else_block: Some(Block { stmts: vec![Stmt::Expr(Expr::Ident("else_dep".to_string()))], span: dummy_span() }),
        };
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&expr, &mut deps);
        assert!(deps.contains("cond"));
        assert!(deps.contains("then_dep"));
        assert!(deps.contains("else_dep"));
    }

    #[test]
    fn test_collect_expr_deps_match() {
        let expr = Expr::Match {
            subject: Box::new(Expr::Ident("subj".to_string())),
            arms: vec![MatchArm {
                pattern: Pattern::Variant { name: "Var".to_string(), fields: vec![] },
                guard: None,
                body: Expr::Ident("body_dep".to_string()),
            }],
        };
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&expr, &mut deps);
        assert!(deps.contains("subj"));
        assert!(deps.contains("Var"));
        assert!(deps.contains("body_dep"));
    }

    #[test]
    fn test_collect_expr_deps_borrow_await_etc() {
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&Expr::Borrow(Box::new(Expr::Ident("a".to_string()))), &mut deps);
        collect_expr_deps(&Expr::BorrowMut(Box::new(Expr::Ident("b".to_string()))), &mut deps);
        collect_expr_deps(&Expr::Await(Box::new(Expr::Ident("c".to_string()))), &mut deps);
        collect_expr_deps(&Expr::Stream { source: Box::new(Expr::Ident("d".to_string())) }, &mut deps);
        collect_expr_deps(&Expr::Navigate { path: Box::new(Expr::Ident("e".to_string())) }, &mut deps);
        collect_expr_deps(&Expr::Receive { channel: Box::new(Expr::Ident("f".to_string())) }, &mut deps);
        assert!(deps.contains("a"));
        assert!(deps.contains("b"));
        assert!(deps.contains("c"));
        assert!(deps.contains("d"));
        assert!(deps.contains("e"));
        assert!(deps.contains("f"));
    }

    #[test]
    fn test_collect_expr_deps_spawn_send_suspend() {
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&Expr::Spawn {
            body: Block { stmts: vec![Stmt::Expr(Expr::Ident("sp".to_string()))], span: dummy_span() },
            span: dummy_span(),
        }, &mut deps);
        collect_expr_deps(&Expr::Send {
            channel: Box::new(Expr::Ident("ch".to_string())),
            value: Box::new(Expr::Ident("val".to_string())),
        }, &mut deps);
        collect_expr_deps(&Expr::Suspend {
            fallback: Box::new(Expr::Ident("fb".to_string())),
            body: Box::new(Expr::Ident("bd".to_string())),
        }, &mut deps);
        assert!(deps.contains("sp"));
        assert!(deps.contains("ch"));
        assert!(deps.contains("val"));
        assert!(deps.contains("fb"));
        assert!(deps.contains("bd"));
    }

    #[test]
    fn test_collect_expr_deps_try_catch_fetch_parallel() {
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&Expr::TryCatch {
            body: Box::new(Expr::Ident("tc_body".to_string())),
            error_binding: "e".to_string(),
            catch_body: Box::new(Expr::Ident("tc_catch".to_string())),
        }, &mut deps);
        collect_expr_deps(&Expr::Fetch {
            url: Box::new(Expr::Ident("url".to_string())),
            options: Some(Box::new(Expr::Ident("opts".to_string()))),
            contract: None,
        }, &mut deps);
        collect_expr_deps(&Expr::Parallel {
            tasks: vec![Expr::Ident("t1".to_string()), Expr::Ident("t2".to_string())],
            span: dummy_span(),
        }, &mut deps);
        assert!(deps.contains("tc_body"));
        assert!(deps.contains("tc_catch"));
        assert!(deps.contains("url"));
        assert!(deps.contains("opts"));
        assert!(deps.contains("t1"));
        assert!(deps.contains("t2"));
    }

    #[test]
    fn test_collect_expr_deps_prompt_env_trace_flag() {
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&Expr::PromptTemplate {
            template: "hi {x}".to_string(),
            interpolations: vec![("x".to_string(), Expr::Ident("pt_dep".to_string()))],
        }, &mut deps);
        collect_expr_deps(&Expr::Env {
            name: Box::new(Expr::Ident("env_dep".to_string())),
            span: dummy_span(),
        }, &mut deps);
        collect_expr_deps(&Expr::Trace {
            label: Box::new(Expr::Ident("tr_label".to_string())),
            body: Block { stmts: vec![Stmt::Expr(Expr::Ident("tr_body".to_string()))], span: dummy_span() },
            span: dummy_span(),
        }, &mut deps);
        collect_expr_deps(&Expr::Flag {
            name: Box::new(Expr::Ident("fl_dep".to_string())),
            span: dummy_span(),
        }, &mut deps);
        assert!(deps.contains("pt_dep"));
        assert!(deps.contains("env_dep"));
        assert!(deps.contains("tr_label"));
        assert!(deps.contains("tr_body"));
        assert!(deps.contains("fl_dep"));
    }

    // --- collect_stmt_deps coverage ---

    #[test]
    fn test_collect_stmt_deps_signal() {
        let stmt = Stmt::Signal {
            name: "s".to_string(),
            ty: Some(Type::Named("SigType".to_string())),
            secret: false, atomic: false,
            value: Expr::Ident("sig_val".to_string()),
        };
        let mut deps = std::collections::HashSet::new();
        collect_stmt_deps(&stmt, &mut deps);
        assert!(deps.contains("sig_val"));
        assert!(deps.contains("SigType"));
    }

    #[test]
    fn test_collect_stmt_deps_yield() {
        let stmt = Stmt::Yield(Expr::Ident("y_dep".to_string()));
        let mut deps = std::collections::HashSet::new();
        collect_stmt_deps(&stmt, &mut deps);
        assert!(deps.contains("y_dep"));
    }

    // --- collect_template_deps coverage ---

    #[test]
    fn test_collect_template_deps_element_with_attrs() {
        let template = TemplateNode::Element(Element {
            tag: "div".to_string(),
            attributes: vec![
                Attribute::Dynamic { name: "class".to_string(), value: Expr::Ident("cls".to_string()) },
                Attribute::EventHandler { event: "click".to_string(), handler: Expr::Ident("handler".to_string()) },
            ],
            children: vec![TemplateNode::Expression(Box::new(Expr::Ident("child_dep".to_string())))],
            span: Span::new(0, 0, 0, 0),
        });
        let mut deps = std::collections::HashSet::new();
        collect_template_deps(&template, &mut deps);
        assert!(deps.contains("cls"));
        assert!(deps.contains("handler"));
        assert!(deps.contains("child_dep"));
    }

    #[test]
    fn test_collect_template_deps_link() {
        let template = TemplateNode::Link {
            to: Expr::Ident("dest".to_string()),
            attributes: vec![],
            children: vec![TemplateNode::TextLiteral("Go".to_string())],
        };
        let mut deps = std::collections::HashSet::new();
        collect_template_deps(&template, &mut deps);
        assert!(deps.contains("dest"));
    }

    // --- Shake keeps always-kept items ---

    #[test]
    fn test_shake_keeps_test_items() {
        let items = vec![
            make_fn("orphan", false, vec![]),
            Item::Test(TestDef {
                name: "my test".to_string(),
                body: Block { stmts: vec![], span: dummy_span() },
                span: dummy_span(),
            }),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);
        assert_eq!(program.items.len(), 1); // test kept, orphan removed
    }

    #[test]
    fn test_shake_keeps_contract_items() {
        let items = vec![
            make_fn("orphan", false, vec![]),
            Item::Contract(ContractDef {
                name: "C".to_string(), fields: vec![], is_pub: false, span: dummy_span(),
            }),
        ];
        let mut program = Program { items };
        let mut stats = ShakeStats::default();
        shake(&mut program, &[], &mut stats);
        assert!(program.items.len() >= 1);
    }

    // --- collect_pattern_deps coverage ---

    #[test]
    fn test_collect_pattern_deps_variant() {
        let pat = Pattern::Variant {
            name: "Some".to_string(),
            fields: vec![Pattern::Ident("val".to_string())],
        };
        let mut deps = std::collections::HashSet::new();
        collect_pattern_deps(&pat, &mut deps);
        assert!(deps.contains("Some"));
        assert!(deps.contains("val"));
    }

    // --- Function with params and return type deps ---

    #[test]
    fn test_collect_deps_function_with_params_and_ret() {
        let item = Item::Function(Function {
            name: "f".to_string(), lifetimes: vec![], type_params: vec![],
            params: vec![Param {
                name: "x".to_string(), ty: Type::Named("ParamType".to_string()),
                ownership: Ownership::Owned,
                secret: false,
}],
            return_type: Some(Type::Named("RetType".to_string())),
            trait_bounds: vec![],
            body: Block { stmts: vec![], span: dummy_span() },
            is_pub: false, is_async: false, must_use: false, span: dummy_span(),
        });
        let mut deps = std::collections::HashSet::new();
        collect_item_deps(&item, &mut deps);
        assert!(deps.contains("ParamType"));
        assert!(deps.contains("RetType"));
    }

    #[test]
    fn test_collect_expr_deps_array_lit() {
        let expr = Expr::ArrayLit(vec![
            Expr::Ident("a".to_string()),
            Expr::Ident("b".to_string()),
        ]);
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&expr, &mut deps);
        assert!(deps.contains("a"));
        assert!(deps.contains("b"));
    }

    #[test]
    fn test_collect_expr_deps_object_lit() {
        let expr = Expr::ObjectLit {
            fields: vec![
                ("x".into(), Expr::Ident("val".to_string())),
            ],
        };
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&expr, &mut deps);
        assert!(deps.contains("val"));
    }

    #[test]
    fn test_collect_expr_deps_match_with_guard() {
        let expr = Expr::Match {
            subject: Box::new(Expr::Ident("subj".to_string())),
            arms: vec![MatchArm {
                pattern: Pattern::Wildcard,
                guard: Some(Expr::Ident("guard_dep".to_string())),
                body: Expr::Ident("body_dep".to_string()),
            }],
        };
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&expr, &mut deps);
        assert!(deps.contains("subj"));
        assert!(deps.contains("guard_dep"));
        assert!(deps.contains("body_dep"));
    }

    #[test]
    fn test_collect_expr_deps_empty_array() {
        let expr = Expr::ArrayLit(vec![]);
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&expr, &mut deps);
        assert!(deps.is_empty());
    }

    #[test]
    fn test_collect_expr_deps_empty_object() {
        let expr = Expr::ObjectLit { fields: vec![] };
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&expr, &mut deps);
        assert!(deps.is_empty());
    }

    #[test]
    fn test_collect_expr_deps_match_no_guard() {
        let expr = Expr::Match {
            subject: Box::new(Expr::Ident("s".to_string())),
            arms: vec![MatchArm {
                pattern: Pattern::Wildcard,
                guard: None,
                body: Expr::Ident("b".to_string()),
            }],
        };
        let mut deps = std::collections::HashSet::new();
        collect_expr_deps(&expr, &mut deps);
        assert!(deps.contains("s"));
        assert!(deps.contains("b"));
        assert_eq!(deps.len(), 2);
    }
}
