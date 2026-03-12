use crate::ast::*;
use std::collections::HashSet;

/// Analyzes a program to determine which WASM import namespaces are used.
///
/// With the unified single-file runtime (core.js), there is no JS module
/// tree-shaking. However, this analysis is still useful for:
///   1. Dead code elimination in the WASM binary (don't emit unused imports)
///   2. Build diagnostics (report which features a program uses)
///   3. Future: conditional compilation of WASM-side feature modules
pub fn detect_required_namespaces(program: &Program) -> HashSet<String> {
    let mut ns = HashSet::new();
    ns.insert("dom".to_string());     // always needed
    ns.insert("mem".to_string());     // always needed
    ns.insert("string".to_string());  // always needed

    for item in &program.items {
        match item {
            Item::Page(_) => { ns.insert("seo".to_string()); }
            Item::Form(_) => { ns.insert("form".to_string()); }
            Item::Channel(_) => { ns.insert("channel".to_string()); }
            Item::Contract(_) => { ns.insert("contract".to_string()); }
            Item::App(app) => {
                ns.insert("pwa".to_string());
                if app.a11y.is_some() { ns.insert("a11y".to_string()); }
            }
            Item::Embed(_) => { ns.insert("embed".to_string()); }
            Item::Pdf(_) => { ns.insert("pdf".to_string()); }
            Item::Payment(_) => { ns.insert("payment".to_string()); }
            Item::Auth(_) => { ns.insert("auth".to_string()); }
            Item::Upload(_) => { ns.insert("upload".to_string()); }
            Item::Db(_) => { ns.insert("db".to_string()); }
            Item::Breakpoints(_) => { ns.insert("responsive".to_string()); }
            Item::Animation(_) => { ns.insert("animation".to_string()); }
            Item::Theme(_) => { ns.insert("theme".to_string()); }
            Item::Component(c) => {
                if c.a11y.is_some() { ns.insert("a11y".to_string()); }
                if !c.shortcuts.is_empty() { ns.insert("shortcuts".to_string()); }
                if c.permissions.is_some() { ns.insert("permissions".to_string()); }
                if !c.gestures.is_empty() { ns.insert("gesture".to_string()); }
                if c.on_destroy.is_some() { ns.insert("lifecycle".to_string()); }
                check_exprs_in_component(c, &mut ns);
            }
            Item::Store(s) => {
                if !s.selectors.is_empty() { ns.insert("state".to_string()); }
                for field in &s.signals {
                    if field.atomic { ns.insert("state".to_string()); }
                }
            }
            Item::LazyComponent(lazy) => {
                ns.insert("loader".to_string());
                if lazy.component.permissions.is_some() { ns.insert("permissions".to_string()); }
                if !lazy.component.gestures.is_empty() { ns.insert("gesture".to_string()); }
                if lazy.component.on_destroy.is_some() { ns.insert("lifecycle".to_string()); }
                check_exprs_in_component(&lazy.component, &mut ns);
            }
            _ => {}
        }
    }

    ns
}

/// Legacy API — returns the same result as detect_required_namespaces.
/// Kept for backward compatibility with existing build tooling.
pub fn detect_required_modules(program: &Program) -> HashSet<String> {
    detect_required_namespaces(program)
}

fn check_exprs_in_component(component: &Component, ns: &mut HashSet<String>) {
    for method in &component.methods {
        check_exprs_in_block(&method.body, ns);
    }
}

fn check_exprs_in_block(block: &Block, ns: &mut HashSet<String>) {
    for stmt in &block.stmts {
        check_exprs_in_stmt(stmt, ns);
    }
}

fn check_exprs_in_stmt(stmt: &Stmt, ns: &mut HashSet<String>) {
    match stmt {
        Stmt::Expr(expr) | Stmt::Return(Some(expr)) => { check_expr(expr, ns); }
        Stmt::Let { value, .. } => { check_expr(value, ns); }
        Stmt::Signal { value, .. } => { check_expr(value, ns); }
        Stmt::LetDestructure { value, .. } => { check_expr(value, ns); }
        Stmt::Yield(expr) => { check_expr(expr, ns); }
        Stmt::Return(None) => {}
    }
}

fn check_expr(expr: &Expr, ns: &mut HashSet<String>) {
    match expr {
        Expr::Spawn { body, .. } => {
            ns.insert("worker".to_string());
            check_exprs_in_block(body, ns);
        }
        Expr::Parallel { tasks, .. } => {
            ns.insert("worker".to_string());
            for task in tasks { check_expr(task, ns); }
        }
        Expr::Env { .. } => { ns.insert("env".to_string()); }
        Expr::Trace { body, .. } => {
            ns.insert("trace".to_string());
            check_exprs_in_block(body, ns);
        }
        Expr::Flag { .. } => { ns.insert("flags".to_string()); }
        Expr::Download { .. } => { ns.insert("io".to_string()); }
        Expr::DynamicImport { .. } => { ns.insert("loader".to_string()); }
        Expr::VirtualList { items, item_height, template, .. } => {
            ns.insert("virtual".to_string());
            check_expr(items, ns);
            check_expr(item_height, ns);
            check_expr(template, ns);
        }
        Expr::Fetch { .. } => { ns.insert("http".to_string()); }
        Expr::FnCall { callee, args, .. } => {
            if let Expr::FieldAccess { object, .. } = &**callee {
                if let Expr::Ident(ref name) = **object {
                    match name.as_str() {
                        "theme" => { ns.insert("theme".to_string()); }
                        "auth" => { ns.insert("auth".to_string()); }
                        "upload" => { ns.insert("upload".to_string()); }
                        "db" => { ns.insert("db".to_string()); }
                        "animate" => { ns.insert("animate".to_string()); }
                        "responsive" => { ns.insert("responsive".to_string()); }
                        "clipboard" => { ns.insert("clipboard".to_string()); }
                        "share" => { ns.insert("share".to_string()); }
                        "storage" => { ns.insert("webapi".to_string()); }
                        "rtc" => { ns.insert("rtc".to_string()); }
                        _ => {} // std lib namespaces are pure WASM — no JS imports
                    }
                }
            }
            // Detect rtc_ prefixed bare function calls
            if let Expr::Ident(ref name) = **callee {
                if name.starts_with("rtc_") { ns.insert("rtc".to_string()); }
            }
            check_expr(callee, ns);
            for arg in args { check_expr(arg, ns); }
        }
        Expr::MethodCall { object, args, .. } => {
            if let Expr::Ident(ref name) = **object {
                match name.as_str() {
                    "clipboard" => { ns.insert("clipboard".to_string()); }
                    _ => {}
                }
            }
            check_expr(object, ns);
            for arg in args { check_expr(arg, ns); }
        }
        // Recurse into sub-expressions
        Expr::Binary { left, right, .. } => { check_expr(left, ns); check_expr(right, ns); }
        Expr::Unary { operand, .. } => { check_expr(operand, ns); }
        Expr::FieldAccess { object, .. } => { check_expr(object, ns); }
        Expr::Index { object, index, .. } => { check_expr(object, ns); check_expr(index, ns); }
        Expr::If { condition, then_block, else_block, .. } => {
            check_expr(condition, ns);
            check_exprs_in_block(then_block, ns);
            if let Some(eb) = else_block { check_exprs_in_block(eb, ns); }
        }
        Expr::Match { subject, arms, .. } => {
            check_expr(subject, ns);
            for arm in arms { check_expr(&arm.body, ns); }
        }
        Expr::For { iterator, body, .. } => { check_expr(iterator, ns); check_exprs_in_block(body, ns); }
        Expr::While { condition, body, .. } => { check_expr(condition, ns); check_exprs_in_block(body, ns); }
        Expr::Block(block) => { check_exprs_in_block(block, ns); }
        Expr::Assign { target, value, .. } => { check_expr(target, ns); check_expr(value, ns); }
        Expr::Await(inner) => { check_expr(inner, ns); }
        Expr::TryCatch { body, catch_body, .. } => { check_expr(body, ns); check_expr(catch_body, ns); }
        Expr::Closure { body, .. } => { check_expr(body, ns); }
        Expr::Borrow(inner) | Expr::BorrowMut(inner) | Expr::Try(inner) | Expr::Stream { source: inner } => {
            check_expr(inner, ns);
        }
        Expr::Suspend { fallback, body, .. } => { check_expr(fallback, ns); check_expr(body, ns); }
        Expr::Send { channel, value, .. } => {
            ns.insert("worker".to_string());
            check_expr(channel, ns); check_expr(value, ns);
        }
        Expr::Receive { channel, .. } => {
            ns.insert("worker".to_string());
            check_expr(channel, ns);
        }
        Expr::Channel { .. } => { ns.insert("worker".to_string()); }
        _ => {}
    }
}

/// Format detected namespaces as a comma-separated string for diagnostics.
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
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("dom"));
        assert!(ns.contains("mem"));
        assert!(ns.contains("string"));
        assert_eq!(ns.len(), 3);
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
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("dom"));
        assert!(ns.contains("seo"));
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
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("contract"));
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
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("form"));
    }

    #[test]
    fn test_modules_to_string() {
        let mut ns = HashSet::new();
        ns.insert("dom".to_string());
        ns.insert("seo".to_string());
        ns.insert("form".to_string());
        let result = modules_to_string(&ns);
        assert_eq!(result, "dom,form,seo");
    }

    // --- Item-level detection ---

    #[test]
    fn test_channel_includes_channel() {
        let program = Program {
            items: vec![Item::Channel(ChannelDef {
                name: "Chat".to_string(),
                url: Expr::StringLit("/ws".to_string()),
                contract: None,
                on_message: None,
                on_connect: None,
                on_disconnect: None,
                reconnect: false,
                heartbeat_interval: None,
                methods: vec![],
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("channel"));
    }

    #[test]
    fn test_embed_includes_embed() {
        let program = Program {
            items: vec![Item::Embed(EmbedDef {
                name: "GA".to_string(),
                src: Expr::StringLit("https://example.com".to_string()),
                loading: None,
                sandbox: false,
                integrity: None,
                permissions: None,
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("embed"));
    }

    #[test]
    fn test_pdf_includes_pdf() {
        let program = Program {
            items: vec![Item::Pdf(PdfDef {
                name: "Invoice".to_string(),
                render: RenderBlock {
                    body: TemplateNode::TextLiteral("pdf".to_string()),
                    span: empty_span(),
                },
                page_size: None,
                orientation: None,
                margins: None,
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("pdf"));
    }

    #[test]
    fn test_payment_includes_payment() {
        let program = Program {
            items: vec![Item::Payment(PaymentDef {
                name: "Pay".to_string(),
                provider: None,
                public_key: None,
                sandbox_mode: false,
                on_success: None,
                on_error: None,
                methods: vec![],
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("payment"));
    }

    #[test]
    fn test_auth_includes_auth() {
        let program = Program {
            items: vec![Item::Auth(AuthDef {
                name: "Auth".to_string(),
                provider: None,
                providers: vec![],
                on_login: None,
                on_logout: None,
                on_error: None,
                session_storage: None,
                methods: vec![],
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("auth"));
    }

    #[test]
    fn test_upload_includes_upload() {
        let program = Program {
            items: vec![Item::Upload(UploadDef {
                name: "Upload".to_string(),
                endpoint: Expr::StringLit("/upload".to_string()),
                max_size: None,
                accept: vec![],
                chunked: false,
                on_progress: None,
                on_complete: None,
                on_error: None,
                methods: vec![],
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("upload"));
    }

    #[test]
    fn test_db_includes_db() {
        let program = Program {
            items: vec![Item::Db(DbDef {
                name: "Db".to_string(),
                version: None,
                stores: vec![],
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("db"));
    }

    #[test]
    fn test_breakpoints_includes_responsive() {
        let program = Program {
            items: vec![Item::Breakpoints(BreakpointsDef {
                breakpoints: vec![("sm".to_string(), 640)],
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("responsive"));
    }

    #[test]
    fn test_animation_includes_animation() {
        let program = Program {
            items: vec![Item::Animation(AnimationBlockDef {
                name: "Fade".to_string(),
                kind: AnimationKind::Spring {
                    stiffness: None,
                    damping: None,
                    mass: None,
                    properties: vec![],
                },
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("animation"));
    }

    #[test]
    fn test_theme_includes_theme() {
        let program = Program {
            items: vec![Item::Theme(ThemeDef {
                name: "T".to_string(),
                light: None,
                dark: None,
                dark_auto: false,
                primary: None,
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("theme"));
    }

    #[test]
    fn test_app_includes_pwa() {
        let program = Program {
            items: vec![Item::App(AppDef {
                name: "MyApp".to_string(),
                manifest: None,
                offline: None,
                push: None,
                router: None,
                a11y: None,
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("pwa"));
    }

    #[test]
    fn test_app_with_a11y_includes_a11y() {
        let program = Program {
            items: vec![Item::App(AppDef {
                name: "MyApp".to_string(),
                manifest: None,
                offline: None,
                push: None,
                router: None,
                a11y: Some(A11yMode::Auto),
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("a11y"));
    }

    // --- Component features ---

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
                span: empty_span(),
            },
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: empty_span(),
        }
    }

    #[test]
    fn test_component_with_a11y() {
        let mut c = make_component("C");
        c.a11y = Some(A11yMode::Auto);
        let program = Program { items: vec![Item::Component(c)] };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("a11y"));
    }

    #[test]
    fn test_component_with_shortcuts() {
        let mut c = make_component("C");
        c.shortcuts = vec![ShortcutDef {
            keys: "ctrl+s".to_string(),
            body: Block { stmts: vec![], span: empty_span() },
            span: empty_span(),
        }];
        let program = Program { items: vec![Item::Component(c)] };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("shortcuts"));
    }

    #[test]
    fn test_component_with_permissions() {
        let mut c = make_component("C");
        c.permissions = Some(PermissionsDef {
            network: vec![],
            storage: vec![],
            capabilities: vec!["camera".to_string()],
            span: empty_span(),
        });
        let program = Program { items: vec![Item::Component(c)] };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("permissions"));
    }

    #[test]
    fn test_component_with_gestures() {
        let mut c = make_component("C");
        c.gestures = vec![GestureDef {
            gesture_type: "swipe_left".to_string(),
            target: None,
            body: Block { stmts: vec![], span: empty_span() },
            span: empty_span(),
        }];
        let program = Program { items: vec![Item::Component(c)] };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("gesture"));
    }

    #[test]
    fn test_component_with_on_destroy() {
        let mut c = make_component("C");
        c.on_destroy = Some(Function {
            name: "cleanup".to_string(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: Block { stmts: vec![], span: empty_span() },
            is_pub: false,
            must_use: false,
            span: empty_span(),
        });
        let program = Program { items: vec![Item::Component(c)] };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("lifecycle"));
    }

    // --- Store features ---

    #[test]
    fn test_store_with_selectors() {
        let program = Program {
            items: vec![Item::Store(StoreDef {
                name: "S".to_string(),
                signals: vec![],
                actions: vec![],
                computed: vec![],
                effects: vec![],
                selectors: vec![SelectorDef {
                    name: "sel".to_string(),
                    deps: vec![],
                    body: Expr::Integer(0),
                    span: empty_span(),
                }],
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("state"));
    }

    #[test]
    fn test_store_with_atomic_signal() {
        let program = Program {
            items: vec![Item::Store(StoreDef {
                name: "S".to_string(),
                signals: vec![StateField {
                    name: "count".to_string(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    atomic: true,
                    initializer: Expr::Integer(0),
                    ownership: Ownership::Owned,
                }],
                actions: vec![],
                computed: vec![],
                effects: vec![],
                selectors: vec![],
                is_pub: false,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("state"));
    }

    // --- LazyComponent ---

    #[test]
    fn test_lazy_component_includes_loader() {
        let c = make_component("Heavy");
        let program = Program {
            items: vec![Item::LazyComponent(LazyComponentDef {
                component: c,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("loader"));
    }

    #[test]
    fn test_lazy_component_with_permissions() {
        let mut c = make_component("Heavy");
        c.permissions = Some(PermissionsDef {
            network: vec![],
            storage: vec![],
            capabilities: vec![],
            span: empty_span(),
        });
        let program = Program {
            items: vec![Item::LazyComponent(LazyComponentDef {
                component: c,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("loader"));
        assert!(ns.contains("permissions"));
    }

    #[test]
    fn test_lazy_component_with_gestures() {
        let mut c = make_component("Heavy");
        c.gestures = vec![GestureDef {
            gesture_type: "pinch".to_string(),
            target: None,
            body: Block { stmts: vec![], span: empty_span() },
            span: empty_span(),
        }];
        let program = Program {
            items: vec![Item::LazyComponent(LazyComponentDef {
                component: c,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("gesture"));
    }

    #[test]
    fn test_lazy_component_with_on_destroy() {
        let mut c = make_component("Heavy");
        c.on_destroy = Some(Function {
            name: "cleanup".to_string(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: Block { stmts: vec![], span: empty_span() },
            is_pub: false,
            must_use: false,
            span: empty_span(),
        });
        let program = Program {
            items: vec![Item::LazyComponent(LazyComponentDef {
                component: c,
                span: empty_span(),
            })],
        };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("lifecycle"));
    }

    // --- Expression-level detection ---

    fn make_fn_with_expr(expr: Expr) -> Function {
        Function {
            name: "test".to_string(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: Block { stmts: vec![Stmt::Expr(expr)], span: empty_span() },
            is_pub: false,
            must_use: false,
            span: empty_span(),
        }
    }

    fn program_with_component_method(expr: Expr) -> Program {
        let mut c = make_component("C");
        c.methods = vec![make_fn_with_expr(expr)];
        Program { items: vec![Item::Component(c)] }
    }

    #[test]
    fn test_spawn_includes_worker() {
        let program = program_with_component_method(Expr::Spawn {
            body: Block { stmts: vec![], span: empty_span() },
            span: empty_span(),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("worker"));
    }

    #[test]
    fn test_parallel_includes_worker() {
        let program = program_with_component_method(Expr::Parallel {
            tasks: vec![Expr::Integer(1)],
            span: empty_span(),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("worker"));
    }

    #[test]
    fn test_env_includes_env() {
        let program = program_with_component_method(Expr::Env {
            name: Box::new(Expr::StringLit("KEY".to_string())),
            span: empty_span(),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("env"));
    }

    #[test]
    fn test_trace_includes_trace() {
        let program = program_with_component_method(Expr::Trace {
            label: Box::new(Expr::StringLit("t".to_string())),
            body: Block { stmts: vec![], span: empty_span() },
            span: empty_span(),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("trace"));
    }

    #[test]
    fn test_flag_includes_flags() {
        let program = program_with_component_method(Expr::Flag {
            name: Box::new(Expr::StringLit("f".to_string())),
            span: empty_span(),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("flags"));
    }

    #[test]
    fn test_download_includes_io() {
        let program = program_with_component_method(Expr::Download {
            data: Box::new(Expr::Integer(0)),
            filename: Box::new(Expr::StringLit("f.txt".to_string())),
            span: empty_span(),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("io"));
    }

    #[test]
    fn test_dynamic_import_includes_loader() {
        let program = program_with_component_method(Expr::DynamicImport {
            path: Box::new(Expr::StringLit("./mod".to_string())),
            span: empty_span(),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("loader"));
    }

    #[test]
    fn test_fetch_includes_http() {
        let program = program_with_component_method(Expr::Fetch {
            url: Box::new(Expr::StringLit("http://x".to_string())),
            options: None,
            contract: None,
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("http"));
    }

    #[test]
    fn test_virtual_list_includes_virtual() {
        let program = program_with_component_method(Expr::VirtualList {
            items: Box::new(Expr::Ident("data".to_string())),
            item_height: Box::new(Expr::Integer(50)),
            template: Box::new(Expr::Ident("row".to_string())),
            buffer: None,
            span: empty_span(),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("virtual"));
    }

    #[test]
    fn test_send_includes_worker() {
        let program = program_with_component_method(Expr::Send {
            channel: Box::new(Expr::Ident("ch".to_string())),
            value: Box::new(Expr::Integer(1)),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("worker"));
    }

    #[test]
    fn test_receive_includes_worker() {
        let program = program_with_component_method(Expr::Receive {
            channel: Box::new(Expr::Ident("ch".to_string())),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("worker"));
    }

    #[test]
    fn test_channel_expr_includes_worker() {
        let program = program_with_component_method(Expr::Channel {
            ty: Some(Type::Named("i32".to_string())),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("worker"));
    }

    // --- FnCall with field access patterns ---

    #[test]
    fn test_fncall_theme_includes_theme() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("theme".to_string())),
                field: "get".to_string(),
            }),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("theme"));
    }

    #[test]
    fn test_fncall_auth_includes_auth() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("auth".to_string())),
                field: "login".to_string(),
            }),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("auth"));
    }

    #[test]
    fn test_fncall_upload_includes_upload() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("upload".to_string())),
                field: "start".to_string(),
            }),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("upload"));
    }

    #[test]
    fn test_fncall_db_includes_db() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("db".to_string())),
                field: "query".to_string(),
            }),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("db"));
    }

    #[test]
    fn test_fncall_animate_includes_animate() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("animate".to_string())),
                field: "play".to_string(),
            }),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("animate"));
    }

    #[test]
    fn test_fncall_responsive_includes_responsive() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("responsive".to_string())),
                field: "check".to_string(),
            }),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("responsive"));
    }

    #[test]
    fn test_fncall_clipboard_includes_clipboard() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("clipboard".to_string())),
                field: "write".to_string(),
            }),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("clipboard"));
    }

    #[test]
    fn test_fncall_share_includes_share() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("share".to_string())),
                field: "open".to_string(),
            }),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("share"));
    }

    #[test]
    fn test_fncall_storage_includes_webapi() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("storage".to_string())),
                field: "get".to_string(),
            }),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("webapi"));
    }

    // --- MethodCall with clipboard ---

    #[test]
    fn test_methodcall_clipboard_includes_clipboard() {
        let program = program_with_component_method(Expr::MethodCall {
            object: Box::new(Expr::Ident("clipboard".to_string())),
            method: "read".to_string(),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("clipboard"));
    }

    // --- detect_required_modules is alias ---

    #[test]
    fn test_detect_required_modules_alias() {
        let program = Program { items: vec![] };
        let ns1 = detect_required_namespaces(&program);
        let ns2 = detect_required_modules(&program);
        assert_eq!(ns1, ns2);
    }

    // --- LetDestructure stmt in component method ---

    #[test]
    fn test_let_destructure_stmt() {
        let mut c = make_component("C");
        c.methods = vec![make_fn_with_expr(Expr::Block(Block {
            stmts: vec![Stmt::LetDestructure {
                pattern: Pattern::Wildcard,
                ty: None,
                value: Expr::Spawn {
                    body: Block { stmts: vec![], span: empty_span() },
                    span: empty_span(),
                },
            }],
            span: empty_span(),
        }))];
        let program = Program { items: vec![Item::Component(c)] };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("worker"));
    }

    // --- Expression recursion: Binary, Unary, FieldAccess, Index, If, Match, For, While ---

    #[test]
    fn test_check_expr_binary() {
        let program = program_with_component_method(Expr::Binary {
            op: BinOp::Add,
            left: Box::new(Expr::Spawn {
                body: Block { stmts: vec![], span: empty_span() },
                span: empty_span(),
            }),
            right: Box::new(Expr::Integer(1)),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("worker"));
    }

    #[test]
    fn test_check_expr_unary() {
        let program = program_with_component_method(Expr::Unary {
            op: UnaryOp::Neg,
            operand: Box::new(Expr::Env {
                name: Box::new(Expr::StringLit("K".to_string())),
                span: empty_span(),
            }),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("env"));
    }

    #[test]
    fn test_check_expr_field_access() {
        let program = program_with_component_method(Expr::FieldAccess {
            object: Box::new(Expr::Env {
                name: Box::new(Expr::StringLit("K".to_string())),
                span: empty_span(),
            }),
            field: "f".to_string(),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("env"));
    }

    #[test]
    fn test_check_expr_index() {
        let program = program_with_component_method(Expr::Index {
            object: Box::new(Expr::Ident("arr".to_string())),
            index: Box::new(Expr::Env {
                name: Box::new(Expr::StringLit("K".to_string())),
                span: empty_span(),
            }),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("env"));
    }

    #[test]
    fn test_check_expr_if() {
        let program = program_with_component_method(Expr::If {
            condition: Box::new(Expr::Bool(true)),
            then_block: Block {
                stmts: vec![Stmt::Expr(Expr::Spawn {
                    body: Block { stmts: vec![], span: empty_span() },
                    span: empty_span(),
                })],
                span: empty_span(),
            },
            else_block: Some(Block {
                stmts: vec![Stmt::Expr(Expr::Env {
                    name: Box::new(Expr::StringLit("K".to_string())),
                    span: empty_span(),
                })],
                span: empty_span(),
            }),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("worker"));
        assert!(ns.contains("env"));
    }

    #[test]
    fn test_check_expr_match() {
        let program = program_with_component_method(Expr::Match {
            subject: Box::new(Expr::Ident("x".to_string())),
            arms: vec![MatchArm {
                pattern: Pattern::Wildcard,
                body: Expr::Spawn {
                    body: Block { stmts: vec![], span: empty_span() },
                    span: empty_span(),
                },
            }],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("worker"));
    }

    #[test]
    fn test_check_expr_for_while_block_assign() {
        let mut c = make_component("C");
        c.methods = vec![Function {
            name: "m".to_string(),
            lifetimes: vec![], type_params: vec![], params: vec![],
            return_type: None, trait_bounds: vec![],
            body: Block {
                stmts: vec![
                    Stmt::Expr(Expr::For {
                        binding: "i".to_string(),
                        iterator: Box::new(Expr::Ident("items".to_string())),
                        body: Block {
                            stmts: vec![Stmt::Expr(Expr::Flag {
                                name: Box::new(Expr::StringLit("f".to_string())),
                                span: empty_span(),
                            })],
                            span: empty_span(),
                        },
                    }),
                    Stmt::Expr(Expr::While {
                        condition: Box::new(Expr::Bool(true)),
                        body: Block {
                            stmts: vec![Stmt::Expr(Expr::Trace {
                                label: Box::new(Expr::StringLit("t".to_string())),
                                body: Block { stmts: vec![], span: empty_span() },
                                span: empty_span(),
                            })],
                            span: empty_span(),
                        },
                    }),
                    Stmt::Expr(Expr::Block(Block {
                        stmts: vec![Stmt::Expr(Expr::Download {
                            data: Box::new(Expr::Integer(0)),
                            filename: Box::new(Expr::StringLit("f.txt".to_string())),
                            span: empty_span(),
                        })],
                        span: empty_span(),
                    })),
                    Stmt::Expr(Expr::Assign {
                        target: Box::new(Expr::Ident("x".to_string())),
                        value: Box::new(Expr::DynamicImport {
                            path: Box::new(Expr::StringLit("m".to_string())),
                            span: empty_span(),
                        }),
                    }),
                ],
                span: empty_span(),
            },
            is_pub: false, must_use: false, span: empty_span(),
        }];
        let program = Program { items: vec![Item::Component(c)] };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("flags"));
        assert!(ns.contains("trace"));
        assert!(ns.contains("io"));
        assert!(ns.contains("loader"));
    }

    // --- Await, TryCatch, Closure, Borrow, Try, Stream ---

    #[test]
    fn test_check_expr_await() {
        let program = program_with_component_method(Expr::Await(
            Box::new(Expr::Fetch {
                url: Box::new(Expr::StringLit("http://x".to_string())),
                options: None, contract: None,
            }),
        ));
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("http"));
    }

    #[test]
    fn test_check_expr_try_catch() {
        let program = program_with_component_method(Expr::TryCatch {
            body: Box::new(Expr::Spawn {
                body: Block { stmts: vec![], span: empty_span() },
                span: empty_span(),
            }),
            error_binding: "e".to_string(),
            catch_body: Box::new(Expr::Integer(0)),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("worker"));
    }

    #[test]
    fn test_check_expr_closure() {
        let program = program_with_component_method(Expr::Closure {
            params: vec![],
            body: Box::new(Expr::Env {
                name: Box::new(Expr::StringLit("K".to_string())),
                span: empty_span(),
            }),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("env"));
    }

    #[test]
    fn test_check_expr_suspend() {
        let program = program_with_component_method(Expr::Suspend {
            fallback: Box::new(Expr::Integer(0)),
            body: Box::new(Expr::Spawn {
                body: Block { stmts: vec![], span: empty_span() },
                span: empty_span(),
            }),
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("worker"));
    }

    // --- Yield stmt coverage ---

    #[test]
    fn test_yield_stmt_detected() {
        let mut c = make_component("C");
        c.methods = vec![Function {
            name: "m".to_string(),
            lifetimes: vec![], type_params: vec![], params: vec![],
            return_type: None, trait_bounds: vec![],
            body: Block {
                stmts: vec![Stmt::Yield(Expr::Env {
                    name: Box::new(Expr::StringLit("K".to_string())),
                    span: empty_span(),
                })],
                span: empty_span(),
            },
            is_pub: false, must_use: false, span: empty_span(),
        }];
        let program = Program { items: vec![Item::Component(c)] };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("env"));
    }

    // --- Return(None) does not crash ---

    #[test]
    fn test_return_none_stmt() {
        let mut c = make_component("C");
        c.methods = vec![Function {
            name: "m".to_string(),
            lifetimes: vec![], type_params: vec![], params: vec![],
            return_type: None, trait_bounds: vec![],
            body: Block {
                stmts: vec![Stmt::Return(None)],
                span: empty_span(),
            },
            is_pub: false, must_use: false, span: empty_span(),
        }];
        let program = Program { items: vec![Item::Component(c)] };
        let ns = detect_required_namespaces(&program);
        // Just dom, mem, string — no additional
        assert_eq!(ns.len(), 3);
    }

    // --- Signal stmt in check_exprs_in_stmt ---

    #[test]
    fn test_signal_stmt() {
        let mut c = make_component("C");
        c.methods = vec![Function {
            name: "m".to_string(),
            lifetimes: vec![], type_params: vec![], params: vec![],
            return_type: None, trait_bounds: vec![],
            body: Block {
                stmts: vec![Stmt::Signal {
                    name: "s".to_string(), ty: None, secret: false, atomic: false,
                    value: Expr::Env {
                        name: Box::new(Expr::StringLit("K".to_string())),
                        span: empty_span(),
                    },
                }],
                span: empty_span(),
            },
            is_pub: false, must_use: false, span: empty_span(),
        }];
        let program = Program { items: vec![Item::Component(c)] };
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("env"));
    }

    // --- MethodCall with non-matching name ---

    #[test]
    fn test_methodcall_unknown_no_extra_ns() {
        let program = program_with_component_method(Expr::MethodCall {
            object: Box::new(Expr::Ident("unknown".to_string())),
            method: "do_thing".to_string(),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        // Only the base dom, mem, string
        assert_eq!(ns.len(), 3);
    }

    // --- WebRTC (rtc) namespace detection ---

    #[test]
    fn test_rtc_function_call_detects_rtc_namespace() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::Ident("rtc_create_peer".to_string())),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("rtc"), "rtc_create_peer should trigger rtc namespace");
    }

    #[test]
    fn test_rtc_data_channel_detects_rtc_namespace() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::Ident("rtc_create_data_channel".to_string())),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("rtc"), "rtc_create_data_channel should trigger rtc namespace");
    }

    #[test]
    fn test_rtc_media_detects_rtc_namespace() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::Ident("rtc_get_user_media".to_string())),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("rtc"), "rtc_get_user_media should trigger rtc namespace");
    }

    #[test]
    fn test_rtc_field_access_detects_rtc_namespace() {
        let program = program_with_component_method(Expr::FnCall {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("rtc".to_string())),
                field: "create_peer".to_string(),
            }),
            args: vec![],
        });
        let ns = detect_required_namespaces(&program);
        assert!(ns.contains("rtc"), "rtc.create_peer should trigger rtc namespace");
    }
}
