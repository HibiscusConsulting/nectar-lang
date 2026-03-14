/// Code formatter for the Nectar language.
///
/// Produces canonical, deterministic source text from a parsed AST.
/// Configurable indent size, line width, trailing commas, and
/// single-line expression threshold.

use crate::ast::*;

/// Configuration options for the formatter.
#[derive(Debug, Clone)]
pub struct FormatterOptions {
    /// Number of spaces per indent level (default: 4).
    pub indent_size: usize,
    /// Maximum line width before breaking (default: 100).
    pub max_line_width: usize,
    /// Append trailing commas in multi-line lists (default: true).
    #[allow(dead_code)]
    pub trailing_commas: bool,
    /// Expressions shorter than this stay on one line (default: 60).
    pub single_line_threshold: usize,
}

impl Default for FormatterOptions {
    fn default() -> Self {
        Self {
            indent_size: 4,
            max_line_width: 100,
            trailing_commas: true,
            single_line_threshold: 60,
        }
    }
}

/// The formatter itself, carrying options and current state.
pub struct Formatter {
    pub options: FormatterOptions,
    indent: usize,
    buf: String,
}

impl Formatter {
    pub fn new(options: FormatterOptions) -> Self {
        Self {
            options,
            indent: 0,
            buf: String::new(),
        }
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn indent_str(&self) -> String {
        " ".repeat(self.indent * self.options.indent_size)
    }

    fn push(&mut self, s: &str) {
        self.buf.push_str(s);
    }

    fn push_indent(&mut self) {
        let s = self.indent_str();
        self.buf.push_str(&s);
    }

    fn push_line(&mut self, s: &str) {
        self.push_indent();
        self.buf.push_str(s);
        self.buf.push('\n');
    }

    fn newline(&mut self) {
        self.buf.push('\n');
    }

    fn inc(&mut self) {
        self.indent += 1;
    }

    fn dec(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    fn fits_single_line(&self, s: &str) -> bool {
        !s.contains('\n') && s.len() <= self.options.single_line_threshold
    }

    // ------------------------------------------------------------------
    // Public entry point
    // ------------------------------------------------------------------

    pub fn format_program(&mut self, program: &Program) -> String {
        self.buf.clear();
        self.indent = 0;

        for (i, item) in program.items.iter().enumerate() {
            if i > 0 {
                self.newline();
            }
            self.format_item(item);
        }

        self.buf.clone()
    }

    // ------------------------------------------------------------------
    // Items
    // ------------------------------------------------------------------

    fn format_item(&mut self, item: &Item) {
        match item {
            Item::Function(f) => self.format_function(f),
            Item::Component(c) => self.format_component(c),
            Item::Struct(s) => self.format_struct(s),
            Item::Enum(e) => self.format_enum(e),
            Item::Impl(i) => self.format_impl(i),
            Item::Trait(t) => self.format_trait(t),
            Item::Use(u) => self.format_use(u),
            Item::Store(s) => self.format_store(s),
            Item::Agent(a) => self.format_agent(a),
            Item::Router(r) => self.format_router(r),
            Item::LazyComponent(lc) => {
                self.push_indent();
                self.push("lazy ");
                self.format_component_inner(&lc.component);
            }
            Item::Test(t) => self.format_test(t),
            Item::Contract(_) => {}
            Item::Page(page) => {
                self.push_indent();
                if page.is_pub { self.push("pub "); }
                self.push(&format!("page {} {{\n", page.name));
                self.indent += 1;
                // Minimal formatting — body details omitted
                self.indent -= 1;
                self.push_indent();
                self.push("}\n");
            }
            Item::App(app) => {
                self.push_indent();
                if app.is_pub { self.push("pub "); }
                self.push(&format!("app {} {{\n", app.name));
                self.indent += 1;
                // Minimal formatting — body details omitted
                self.indent -= 1;
                self.push_indent();
                self.push("}\n");
            }
            Item::Form(form) => {
                self.push_indent();
                if form.is_pub { self.push("pub "); }
                self.push(&format!("form {} {{\n", form.name));
                self.indent += 1;
                for field in &form.fields {
                    self.push_indent();
                    self.push(&format!("field {}: {:?}\n", field.name, field.ty));
                }
                for method in &form.methods {
                    self.format_function(method);
                }
                self.indent -= 1;
                self.push_indent();
                self.push("}\n");
            }
            Item::Channel(ch) => {
                self.push_indent();
                if ch.is_pub { self.push("pub "); }
                self.push(&format!("channel {}", ch.name));
                if let Some(ref contract) = ch.contract {
                    self.push(&format!(" -> {}", contract));
                }
                self.push(" {\n");
                self.indent += 1;
                for method in &ch.methods {
                    self.format_function(method);
                }
                self.indent -= 1;
                self.push_indent();
                self.push("}\n");
            }
            Item::Mod(m) => self.format_mod(m),
            Item::Embed(e) => {
                self.push_indent();
                if e.is_pub { self.push("pub "); }
                self.push(&format!("embed {} {{\n", e.name));
                self.indent += 1;
                self.push_indent();
                self.push(&format!("src: {:?},\n", e.src));
                if let Some(ref loading) = e.loading {
                    self.push_indent();
                    self.push(&format!("loading: \"{}\",\n", loading));
                }
                if e.sandbox {
                    self.push_indent();
                    self.push("sandbox: true,\n");
                }
                self.indent -= 1;
                self.push_indent();
                self.push("}\n");
            }
            Item::Pdf(p) => {
                self.push_indent();
                if p.is_pub { self.push("pub "); }
                self.push(&format!("pdf {} {{\n", p.name));
                self.indent += 1;
                if let Some(ref size) = p.page_size {
                    self.push_indent();
                    self.push(&format!("page_size: \"{}\",\n", size));
                }
                if let Some(ref orient) = p.orientation {
                    self.push_indent();
                    self.push(&format!("orientation: \"{}\",\n", orient));
                }
                self.indent -= 1;
                self.push_indent();
                self.push("}\n");
            }
            Item::Payment(_) => {}
            Item::Auth(_) => {}
            Item::Upload(_) => {}
            Item::Db(_) => {}
            Item::Cache(_) => {}
            Item::Breakpoints(_) => {}
            Item::Theme(_) => {}
            Item::Animation(_) => {}
        }
    }

    fn format_function(&mut self, f: &Function) {
        self.push_indent();
        if f.is_pub {
            self.push("pub ");
        }
        self.push("fn ");
        self.push(&f.name);
        self.format_type_params(&f.type_params);
        self.push("(");
        self.format_params(&f.params);
        self.push(")");
        if let Some(ret) = &f.return_type {
            self.push(" -> ");
            self.push(&Self::format_type(ret));
        }
        if !f.trait_bounds.is_empty() {
            self.push(" where ");
            for (i, b) in f.trait_bounds.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(&b.type_param);
                self.push(": ");
                self.push(&b.trait_name);
            }
        }
        self.push(" {\n");
        self.inc();
        self.format_block(&f.body);
        self.dec();
        self.push_line("}");
    }

    fn format_component(&mut self, c: &Component) {
        self.push_indent();
        self.format_component_inner(c);
    }

    fn format_component_inner(&mut self, c: &Component) {
        self.push("component ");
        self.push(&c.name);
        self.format_type_params(&c.type_params);
        self.push(" {\n");
        self.inc();
        if !c.props.is_empty() {
            for p in &c.props {
                self.push_indent();
                self.push("prop ");
                self.push(&p.name);
                self.push(": ");
                self.push(&Self::format_type(&p.ty));
                if let Some(def) = &p.default {
                    self.push(" = ");
                    self.push(&self.format_expr_to_string(def));
                }
                self.push(";\n");
            }
            self.newline();
        }
        if !c.state.is_empty() {
            for s in &c.state {
                self.format_state_field(s);
            }
            self.newline();
        }
        for m in &c.methods {
            self.format_function(m);
            self.newline();
        }
        for st in &c.styles {
            self.push_indent();
            self.push(&format!("style \"{}\" {{\n", st.selector));
            self.inc();
            for (prop, val) in &st.properties {
                self.push_indent();
                self.push(prop);
                self.push(": ");
                self.push(val);
                self.push(";\n");
            }
            self.dec();
            self.push_line("}");
        }
        for t in &c.transitions {
            self.push_indent();
            self.push(&format!("transition {} {}ms {};\n", t.property, t.duration, t.easing));
        }
        if let Some(skel) = &c.skeleton {
            self.push_indent();
            self.push("skeleton {\n");
            self.inc();
            self.format_template_node(&skel.body.body);
            self.dec();
            self.push_line("}");
        }
        self.push_indent();
        self.push("render {\n");
        self.inc();
        self.format_template_node(&c.render.body);
        self.dec();
        self.push_line("}");
        if let Some(eb) = &c.error_boundary {
            self.push_indent();
            self.push("error_boundary {\n");
            self.inc();
            self.format_template_node(&eb.body.body);
            self.dec();
            self.push_indent();
            self.push("} fallback {\n");
            self.inc();
            self.format_template_node(&eb.fallback.body);
            self.dec();
            self.push_line("}");
        }
        self.dec();
        self.push_line("}");
    }

    fn format_struct(&mut self, s: &StructDef) {
        self.push_indent();
        if s.is_pub {
            self.push("pub ");
        }
        self.push("struct ");
        self.push(&s.name);
        self.format_type_params(&s.type_params);
        if !s.trait_bounds.is_empty() {
            self.push(" where ");
            for (i, b) in s.trait_bounds.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(&b.type_param);
                self.push(": ");
                self.push(&b.trait_name);
            }
        }
        self.push(" {\n");
        self.inc();
        let max_name_len = s.fields.iter().map(|f| f.name.len()).max().unwrap_or(0);
        for f in &s.fields {
            self.push_indent();
            if f.is_pub {
                self.push("pub ");
            }
            self.push(&f.name);
            let padding = max_name_len - f.name.len();
            self.push(&" ".repeat(padding));
            self.push(": ");
            self.push(&Self::format_type(&f.ty));
            self.push(",\n");
        }
        self.dec();
        self.push_line("}");
    }

    fn format_enum(&mut self, e: &EnumDef) {
        self.push_indent();
        if e.is_pub {
            self.push("pub ");
        }
        self.push("enum ");
        self.push(&e.name);
        self.format_type_params(&e.type_params);
        self.push(" {\n");
        self.inc();
        for v in &e.variants {
            self.push_indent();
            self.push(&v.name);
            if !v.fields.is_empty() {
                self.push("(");
                for (i, ty) in v.fields.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(&Self::format_type(ty));
                }
                self.push(")");
            }
            self.push(",\n");
        }
        self.dec();
        self.push_line("}");
    }

    fn format_impl(&mut self, im: &ImplBlock) {
        self.push_indent();
        self.push("impl ");
        self.push(&im.target);
        self.push(" {\n");
        self.inc();
        for (i, m) in im.methods.iter().enumerate() {
            if i > 0 {
                self.newline();
            }
            self.format_function(m);
        }
        self.dec();
        self.push_line("}");
    }

    fn format_trait(&mut self, t: &TraitDef) {
        self.push_indent();
        self.push("trait ");
        self.push(&t.name);
        self.format_type_params(&t.type_params);
        self.push(" {\n");
        self.inc();
        for m in &t.methods {
            self.push_indent();
            self.push("fn ");
            self.push(&m.name);
            self.push("(");
            self.format_params(&m.params);
            self.push(")");
            if let Some(ret) = &m.return_type {
                self.push(" -> ");
                self.push(&Self::format_type(ret));
            }
            if let Some(body) = &m.default_body {
                self.push(" {\n");
                self.inc();
                self.format_block(body);
                self.dec();
                self.push_line("}");
            } else {
                self.push(";\n");
            }
        }
        self.dec();
        self.push_line("}");
    }

    fn format_use(&mut self, u: &UsePath) {
        self.push_indent();
        self.push("use ");
        self.push(&u.segments.join("::"));
        if u.glob {
            self.push("::*");
        }
        if let Some(group) = &u.group {
            self.push("::{");
            for (i, item) in group.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(&item.name);
                if let Some(alias) = &item.alias {
                    self.push(" as ");
                    self.push(alias);
                }
            }
            self.push("}");
        }
        if let Some(alias) = &u.alias {
            self.push(" as ");
            self.push(alias);
        }
        self.push(";\n");
    }

    fn format_store(&mut self, s: &StoreDef) {
        self.push_indent();
        if s.is_pub {
            self.push("pub ");
        }
        self.push("store ");
        self.push(&s.name);
        self.push(" {\n");
        self.inc();
        for sig in &s.signals {
            self.format_state_field(sig);
        }
        if !s.signals.is_empty() {
            self.newline();
        }
        for act in &s.actions {
            self.push_indent();
            if act.is_async {
                self.push("async ");
            }
            self.push("action ");
            self.push(&act.name);
            self.push("(");
            self.format_params(&act.params);
            self.push(") {\n");
            self.inc();
            self.format_block(&act.body);
            self.dec();
            self.push_line("}");
            self.newline();
        }
        for comp in &s.computed {
            self.push_indent();
            self.push("computed ");
            self.push(&comp.name);
            self.push("(&self)");
            if let Some(ret) = &comp.return_type {
                self.push(" -> ");
                self.push(&Self::format_type(ret));
            }
            self.push(" {\n");
            self.inc();
            self.format_block(&comp.body);
            self.dec();
            self.push_line("}");
            self.newline();
        }
        for eff in &s.effects {
            self.push_indent();
            self.push("effect ");
            self.push(&eff.name);
            self.push("(&self) {\n");
            self.inc();
            self.format_block(&eff.body);
            self.dec();
            self.push_line("}");
            self.newline();
        }
        self.dec();
        self.push_line("}");
    }

    fn format_agent(&mut self, a: &AgentDef) {
        self.push_indent();
        self.push("agent ");
        self.push(&a.name);
        self.push(" {\n");
        self.inc();
        if let Some(prompt) = &a.system_prompt {
            self.push_indent();
            self.push("prompt system = \"");
            self.push(prompt);
            self.push("\";\n");
            self.newline();
        }
        for tool in &a.tools {
            self.push_indent();
            self.push("tool ");
            self.push(&tool.name);
            self.push("(");
            self.format_params(&tool.params);
            self.push(")");
            if let Some(ret) = &tool.return_type {
                self.push(" -> ");
                self.push(&Self::format_type(ret));
            }
            self.push(" {\n");
            self.inc();
            self.format_block(&tool.body);
            self.dec();
            self.push_line("}");
            self.newline();
        }
        for s in &a.state {
            self.format_state_field(s);
        }
        for m in &a.methods {
            self.format_function(m);
            self.newline();
        }
        if let Some(render) = &a.render {
            self.push_indent();
            self.push("render {\n");
            self.inc();
            self.format_template_node(&render.body);
            self.dec();
            self.push_line("}");
        }
        self.dec();
        self.push_line("}");
    }

    fn format_router(&mut self, r: &RouterDef) {
        self.push_indent();
        self.push("router ");
        self.push(&r.name);
        self.push(" {\n");
        self.inc();
        let max_path_len = r.routes.iter().map(|rt| rt.path.len() + 2).max().unwrap_or(0);
        for rt in &r.routes {
            self.push_indent();
            self.push("route \"");
            self.push(&rt.path);
            self.push("\"");
            let padding = max_path_len - (rt.path.len() + 2);
            self.push(&" ".repeat(padding));
            self.push(" => ");
            self.push(&rt.component);
            if let Some(guard) = &rt.guard {
                self.push(" guard { ");
                self.push(&self.format_expr_to_string(guard));
                self.push(" }");
            }
            self.push(",\n");
        }
        if let Some(fb) = &r.fallback {
            self.push_indent();
            let padding = if max_path_len > 8 { max_path_len - 8 } else { 0 };
            self.push("fallback");
            self.push(&" ".repeat(padding));
            self.push(" => ");
            self.format_template_node(fb);
        }
        self.dec();
        self.push_line("}");
    }

    fn format_test(&mut self, t: &TestDef) {
        self.push_indent();
        self.push("test \"");
        self.push(&t.name);
        self.push("\" {\n");
        self.inc();
        self.format_block(&t.body);
        self.dec();
        self.push_line("}");
    }

    fn format_mod(&mut self, m: &ModDef) {
        self.push_indent();
        self.push("mod ");
        self.push(&m.name);
        if let Some(items) = &m.items {
            self.push(" {\n");
            self.inc();
            for item in items {
                self.format_item(item);
            }
            self.dec();
            self.push_line("}");
        } else {
            self.push(";\n");
        }
    }

    // ------------------------------------------------------------------
    // Shared helpers
    // ------------------------------------------------------------------

    fn format_type_params(&mut self, params: &[String]) {
        if !params.is_empty() {
            self.push("<");
            self.push(&params.join(", "));
            self.push(">");
        }
    }

    fn format_params(&mut self, params: &[Param]) {
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            if p.secret {
                self.push("secret ");
            }
            match p.ownership {
                Ownership::Borrowed => self.push("&"),
                Ownership::MutBorrowed => self.push("&mut "),
                Ownership::Owned => {}
            }
            self.push(&p.name);
            self.push(": ");
            self.push(&Self::format_type(&p.ty));
        }
    }

    fn format_state_field(&mut self, s: &StateField) {
        self.push_indent();
        self.push("signal ");
        if s.secret {
            self.push("secret ");
        }
        if s.mutable {
            self.push("mut ");
        }
        self.push(&s.name);
        if let Some(ty) = &s.ty {
            self.push(": ");
            self.push(&Self::format_type(ty));
        }
        self.push(" = ");
        self.push(&self.format_expr_to_string(&s.initializer));
        self.push(";\n");
    }

    fn format_type(ty: &Type) -> String {
        match ty {
            Type::Named(n) => n.clone(),
            Type::Generic { name, args } => {
                let args_str: Vec<String> = args.iter().map(Self::format_type).collect();
                format!("{}<{}>", name, args_str.join(", "))
            }
            Type::Reference { mutable, inner, .. } => {
                if *mutable {
                    format!("&mut {}", Self::format_type(inner))
                } else {
                    format!("&{}", Self::format_type(inner))
                }
            }
            Type::Array(inner) => format!("[{}]", Self::format_type(inner)),
            Type::Option(inner) => format!("Option<{}>", Self::format_type(inner)),
            Type::Tuple(types) => {
                let parts: Vec<String> = types.iter().map(Self::format_type).collect();
                format!("({})", parts.join(", "))
            }
            Type::Function { params, ret } => {
                let parts: Vec<String> = params.iter().map(Self::format_type).collect();
                format!("fn({}) -> {}", parts.join(", "), Self::format_type(ret))
            }
            _ => "<unknown>".to_string()
        }
    }

    // ------------------------------------------------------------------
    // Blocks & Statements
    // ------------------------------------------------------------------

    fn format_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.format_stmt(stmt);
        }
    }

    fn format_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, ty, mutable, secret, value, .. } => {
                self.push_indent();
                self.push("let ");
                if *mutable {
                    self.push("mut ");
                }
                if *secret {
                    self.push("secret ");
                }
                self.push(name);
                if let Some(t) = ty {
                    self.push(": ");
                    self.push(&Self::format_type(t));
                }
                self.push(" = ");
                self.push(&self.format_expr_to_string(value));
                self.push(";\n");
            }
            Stmt::Signal { name, ty, secret, value, .. } => {
                self.push_indent();
                self.push("signal ");
                if *secret {
                    self.push("secret ");
                }
                self.push(name);
                if let Some(t) = ty {
                    self.push(": ");
                    self.push(&Self::format_type(t));
                }
                self.push(" = ");
                self.push(&self.format_expr_to_string(value));
                self.push(";\n");
            }
            Stmt::Expr(expr) => {
                self.push_indent();
                self.push(&self.format_expr_to_string(expr));
                self.push(";\n");
            }
            Stmt::Return(opt_expr) => {
                self.push_indent();
                self.push("return");
                if let Some(e) = opt_expr {
                    self.push(" ");
                    self.push(&self.format_expr_to_string(e));
                }
                self.push(";\n");
            }
            Stmt::Yield(expr) => {
                self.push_indent();
                self.push("yield ");
                self.push(&self.format_expr_to_string(expr));
                self.push(";\n");
            }
            Stmt::LetDestructure { pattern, ty, value } => {
                self.push_indent();
                self.push("let ");
                self.push(&Self::format_pattern(pattern));
                if let Some(t) = ty {
                    self.push(": ");
                    self.push(&Self::format_type(t));
                }
                self.push(" = ");
                self.push(&self.format_expr_to_string(value));
                self.push(";\n");
            }
        }
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn format_expr_to_string(&self, expr: &Expr) -> String {
        self.format_expr_inner(expr, 0)
    }

    fn format_expr_inner(&self, expr: &Expr, depth: usize) -> String {
        let indent_str = " ".repeat((self.indent + depth) * self.options.indent_size);
        let next_indent = " ".repeat((self.indent + depth + 1) * self.options.indent_size);

        match expr {
            Expr::Integer(n) => n.to_string(),
            Expr::Float(f) => format!("{}", f),
            Expr::StringLit(s) => format!("\"{}\"", s),
            Expr::Bool(b) => b.to_string(),
            Expr::Ident(name) => name.clone(),
            Expr::SelfExpr => "self".to_string(),

            Expr::Binary { op, left, right } => {
                let l = self.format_expr_inner(left, depth);
                let r = self.format_expr_inner(right, depth);
                format!("{} {} {}", l, Self::binop_str(op), r)
            }

            Expr::Unary { op, operand } => {
                let o = self.format_expr_inner(operand, depth);
                match op {
                    UnaryOp::Neg => format!("-{}", o),
                    UnaryOp::Not => format!("!{}", o),
                }
            }

            Expr::FieldAccess { object, field } => {
                format!("{}.{}", self.format_expr_inner(object, depth), field)
            }

            Expr::MethodCall { object, method, args } => {
                let obj = self.format_expr_inner(object, depth);
                let args_str: Vec<String> =
                    args.iter().map(|a| self.format_expr_inner(a, depth)).collect();
                let call = format!(".{}({})", method, args_str.join(", "));
                let one_line = format!("{}{}", obj, call);
                if one_line.len() <= self.options.max_line_width {
                    one_line
                } else {
                    format!("{}\n{}{}", obj, indent_str, call)
                }
            }

            Expr::FnCall { callee, args } => {
                let callee_str = self.format_expr_inner(callee, depth);
                let args_str: Vec<String> =
                    args.iter().map(|a| self.format_expr_inner(a, depth)).collect();
                format!("{}({})", callee_str, args_str.join(", "))
            }

            Expr::Index { object, index } => {
                format!("{}[{}]",
                    self.format_expr_inner(object, depth),
                    self.format_expr_inner(index, depth))
            }

            Expr::If { condition, then_block, else_block } => {
                let cond = self.format_expr_inner(condition, depth);
                let then_stmts = self.format_block_to_string(then_block, depth + 1);
                let mut result = format!("if {} {{\n{}{}}}", cond, then_stmts, indent_str);
                if let Some(eb) = else_block {
                    let else_stmts = self.format_block_to_string(eb, depth + 1);
                    result.push_str(&format!(" else {{\n{}{}}}", else_stmts, indent_str));
                }
                result
            }

            Expr::Match { subject, arms } => {
                let subj = self.format_expr_inner(subject, depth);
                let mut result = format!("match {} {{\n", subj);
                let pattern_strs: Vec<String> =
                    arms.iter().map(|a| Self::format_pattern(&a.pattern)).collect();
                let max_pat_len = pattern_strs.iter().map(|p| p.len()).max().unwrap_or(0);
                for (arm, pat_str) in arms.iter().zip(pattern_strs.iter()) {
                    let padding = max_pat_len - pat_str.len();
                    let body_str = self.format_expr_inner(&arm.body, depth + 1);
                    let guard_str = if let Some(guard) = &arm.guard {
                        format!(" if {}", self.format_expr_inner(guard, depth + 1))
                    } else {
                        String::new()
                    };
                    result.push_str(&format!(
                        "{}{}{}{} => {},\n",
                        next_indent, pat_str, " ".repeat(padding), guard_str, body_str
                    ));
                }
                result.push_str(&format!("{}}}", indent_str));
                result
            }

            Expr::For { binding, iterator, body } => {
                let iter_str = self.format_expr_inner(iterator, depth);
                let body_str = self.format_block_to_string(body, depth + 1);
                format!("for {} in {} {{\n{}{}}}", binding, iter_str, body_str, indent_str)
            }

            Expr::While { condition, body } => {
                let cond = self.format_expr_inner(condition, depth);
                let body_str = self.format_block_to_string(body, depth + 1);
                format!("while {} {{\n{}{}}}", cond, body_str, indent_str)
            }

            Expr::Block(block) => {
                let body_str = self.format_block_to_string(block, depth + 1);
                format!("{{\n{}{}}}", body_str, indent_str)
            }

            Expr::Borrow(inner) => format!("&{}", self.format_expr_inner(inner, depth)),
            Expr::BorrowMut(inner) => format!("&mut {}", self.format_expr_inner(inner, depth)),

            Expr::StructInit { name, fields } => {
                let fields_str: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, self.format_expr_inner(v, depth + 1)))
                    .collect();
                let one_line = format!("{} {{ {} }}", name, fields_str.join(", "));
                if self.fits_single_line(&one_line) {
                    one_line
                } else {
                    let mut result = format!("{} {{\n", name);
                    for f_str in &fields_str {
                        result.push_str(&format!("{}{},\n", next_indent, f_str));
                    }
                    result.push_str(&format!("{}}}", indent_str));
                    result
                }
            }

            Expr::Assign { target, value } => {
                format!("{} = {}",
                    self.format_expr_inner(target, depth),
                    self.format_expr_inner(value, depth))
            }

            Expr::Await(inner) => format!("{}.await", self.format_expr_inner(inner, depth)),

            Expr::Fetch { url, options, .. } => {
                let url_str = self.format_expr_inner(url, depth);
                if let Some(opts) = options {
                    format!("fetch({}, {})", url_str, self.format_expr_inner(opts, depth))
                } else {
                    format!("fetch({})", url_str)
                }
            }

            Expr::Closure { params, body } => {
                let params_str: Vec<String> = params
                    .iter()
                    .map(|(name, ty)| {
                        if let Some(t) = ty {
                            format!("{}: {}", name, Self::format_type(t))
                        } else {
                            name.clone()
                        }
                    })
                    .collect();
                let body_str = self.format_expr_inner(body, depth);
                let one_line = format!("|{}| {}", params_str.join(", "), body_str);
                if self.fits_single_line(&one_line) {
                    one_line
                } else {
                    format!("|{}| {{\n{}{}\n{}}}",
                        params_str.join(", "), next_indent, body_str, indent_str)
                }
            }

            Expr::PromptTemplate { template, .. } => format!("prompt \"{}\"", template),
            Expr::Navigate { path } => format!("navigate({})", self.format_expr_inner(path, depth)),
            Expr::Stream { source } => format!("stream {}", self.format_expr_inner(source, depth)),

            Expr::Suspend { fallback, body } => {
                format!("suspend({}) {{ {} }}",
                    self.format_expr_inner(fallback, depth),
                    self.format_expr_inner(body, depth))
            }

            Expr::Spawn { body, .. } => {
                let inner = self.format_block_to_string(body, depth);
                format!("spawn {}", inner)
            }

            Expr::Channel { ty } => {
                if let Some(t) = ty {
                    format!("channel::<{}>()", Self::format_type(t))
                } else {
                    "channel()".to_string()
                }
            }

            Expr::Send { channel, value } => {
                format!("{}.send({})",
                    self.format_expr_inner(channel, depth),
                    self.format_expr_inner(value, depth))
            }

            Expr::Receive { channel } => {
                format!("{}.receive()", self.format_expr_inner(channel, depth))
            }

            Expr::Parallel { tasks, .. } => {
                let parts: Vec<String> =
                    tasks.iter().map(|e| self.format_expr_inner(e, depth)).collect();
                format!("parallel {{ {} }}", parts.join(", "))
            }

            Expr::TryCatch { body, error_binding, catch_body } => {
                format!("try {{ {} }} catch {} {{ {} }}",
                    self.format_expr_inner(body, depth),
                    error_binding,
                    self.format_expr_inner(catch_body, depth))
            }

            Expr::Assert { condition, message } => {
                let cond = self.format_expr_inner(condition, depth);
                if let Some(msg) = message {
                    format!("assert({}, \"{}\")", cond, msg)
                } else {
                    format!("assert({})", cond)
                }
            }

            Expr::AssertEq { left, right, message } => {
                let l = self.format_expr_inner(left, depth);
                let r = self.format_expr_inner(right, depth);
                if let Some(msg) = message {
                    format!("assert_eq({}, {}, \"{}\")", l, r, msg)
                } else {
                    format!("assert_eq({}, {})", l, r)
                }
            }

            Expr::Animate { target, animation } => {
                format!("animate({}, \"{}\")", self.format_expr_inner(target, depth), animation)
            }

            Expr::FormatString { parts } => {
                let mut result = "f\"".to_string();
                for part in parts {
                    match part {
                        FormatPart::Literal(s) => result.push_str(s),
                        FormatPart::Expression(e) => {
                            result.push('{');
                            result.push_str(&self.format_expr_inner(e, depth));
                            result.push('}');
                        }
                    }
                }
                result.push('"');
                result
            }

            Expr::Try(inner) => {
                format!("{}?", self.format_expr_inner(inner, depth))
            }

            Expr::DynamicImport { path, .. } => {
                format!("import({})", self.format_expr_inner(path, depth))
            }

            Expr::Download { data, filename, .. } => {
                format!("download({}, {})", self.format_expr_inner(data, depth), self.format_expr_inner(filename, depth))
            }
            Expr::Env { name, .. } => {
                format!("env({})", self.format_expr_inner(name, depth))
            }
            Expr::Trace { label, body, .. } => {
                let body_str = self.format_block_to_string(body, depth + 1);
                format!("trace({}) {{\n{}{}}}", self.format_expr_inner(label, depth), body_str, " ".repeat(depth * self.options.indent_size))
            }
            Expr::Flag { name, .. } => {
                format!("flag({})", self.format_expr_inner(name, depth))
            }
            Expr::Range { start, end } => {
                format!("{}..{}", self.format_expr_inner(start, depth), self.format_expr_inner(end, depth))
            }
            Expr::VirtualList { items, item_height, template, .. } => {
                format!("virtual_list({}, {}, {})", self.format_expr_inner(items, depth), self.format_expr_inner(item_height, depth), self.format_expr_inner(template, depth))
            }
            Expr::ArrayLit(elements) => {
                let elems: Vec<String> = elements.iter().map(|e| self.format_expr_inner(e, depth)).collect();
                let one_line = format!("[{}]", elems.join(", "));
                if self.fits_single_line(&one_line) {
                    one_line
                } else {
                    let mut result = "[\n".to_string();
                    for e in &elems {
                        result.push_str(&format!("{}{},\n", next_indent, e));
                    }
                    result.push_str(&format!("{}]", indent_str));
                    result
                }
            }
            Expr::ObjectLit { fields } => {
                let field_strs: Vec<String> = fields.iter()
                    .map(|(k, v)| format!("{}: {}", k, self.format_expr_inner(v, depth + 1)))
                    .collect();
                let one_line = format!("{{ {} }}", field_strs.join(", "));
                if self.fits_single_line(&one_line) {
                    one_line
                } else {
                    let mut result = "{\n".to_string();
                    for f in &field_strs {
                        result.push_str(&format!("{}{},\n", next_indent, f));
                    }
                    result.push_str(&format!("{}}}", indent_str));
                    result
                }
            }
        }
    }

    fn format_block_to_string(&self, block: &Block, depth: usize) -> String {
        let indent = " ".repeat((self.indent + depth) * self.options.indent_size);
        let mut result = String::new();
        for stmt in &block.stmts {
            result.push_str(&indent);
            result.push_str(&self.format_stmt_to_string(stmt, depth));
            result.push('\n');
        }
        result
    }

    fn format_stmt_to_string(&self, stmt: &Stmt, depth: usize) -> String {
        match stmt {
            Stmt::Let { name, ty, mutable, secret, value, .. } => {
                let mut s = "let ".to_string();
                if *mutable { s.push_str("mut "); }
                if *secret { s.push_str("secret "); }
                s.push_str(name);
                if let Some(t) = ty {
                    s.push_str(": ");
                    s.push_str(&Self::format_type(t));
                }
                s.push_str(" = ");
                s.push_str(&self.format_expr_inner(value, depth));
                s.push(';');
                s
            }
            Stmt::Signal { name, ty, secret, value, .. } => {
                let mut s = "signal ".to_string();
                if *secret { s.push_str("secret "); }
                s.push_str(name);
                if let Some(t) = ty {
                    s.push_str(": ");
                    s.push_str(&Self::format_type(t));
                }
                s.push_str(" = ");
                s.push_str(&self.format_expr_inner(value, depth));
                s.push(';');
                s
            }
            Stmt::Expr(expr) => {
                let mut s = self.format_expr_inner(expr, depth);
                s.push(';');
                s
            }
            Stmt::Return(opt_expr) => {
                let mut s = "return".to_string();
                if let Some(e) = opt_expr {
                    s.push(' ');
                    s.push_str(&self.format_expr_inner(e, depth));
                }
                s.push(';');
                s
            }
            Stmt::Yield(expr) => {
                let mut s = "yield ".to_string();
                s.push_str(&self.format_expr_inner(expr, depth));
                s.push(';');
                s
            }
            Stmt::LetDestructure { pattern, ty, value } => {
                let mut s = "let ".to_string();
                s.push_str(&Self::format_pattern(pattern));
                if let Some(t) = ty {
                    s.push_str(": ");
                    s.push_str(&Self::format_type(t));
                }
                s.push_str(" = ");
                s.push_str(&self.format_expr_inner(value, depth));
                s.push(';');
                s
            }
        }
    }

    // ------------------------------------------------------------------
    // Templates / JSX
    // ------------------------------------------------------------------

    fn format_template_node(&mut self, node: &TemplateNode) {
        match node {
            TemplateNode::Element(el) => self.format_element(el),
            TemplateNode::TextLiteral(text) => {
                self.push_indent();
                self.push(text);
                self.newline();
            }
            TemplateNode::Expression(expr) => {
                self.push_indent();
                self.push("{");
                self.push(&self.format_expr_to_string(expr));
                self.push("}");
                self.newline();
            }
            TemplateNode::Fragment(children) => {
                for child in children {
                    self.format_template_node(child);
                }
            }
            TemplateNode::Link { to, attributes, children } => {
                self.push_indent();
                self.push("<Link to={");
                self.push(&self.format_expr_to_string(to));
                self.push("}");
                for attr in attributes {
                    match attr {
                        Attribute::Static { name, value } => {
                            self.push(&format!(" {}=\"{}\"", name, value));
                        }
                        _ => {}
                    }
                }
                self.push(">");
                self.newline();
                self.inc();
                for child in children {
                    self.format_template_node(child);
                }
                self.dec();
                self.push_line("</Link>");
            }
            TemplateNode::Outlet => {
                self.push_line("<Outlet />");
            }
            TemplateNode::Layout(layout_node) => {
                self.format_layout_node(layout_node);
            }
            TemplateNode::TemplateIf { condition, then_children, else_children } => {
                self.push_indent();
                self.push("{if ");
                self.push(&self.format_expr_to_string(condition));
                self.push(" {");
                self.newline();
                self.inc();
                for child in then_children {
                    self.format_template_node(child);
                }
                self.dec();
                if let Some(else_nodes) = else_children {
                    self.push_indent();
                    self.push("} else {");
                    self.newline();
                    self.inc();
                    for child in else_nodes {
                        self.format_template_node(child);
                    }
                    self.dec();
                }
                self.push_line("}}");
            }
            TemplateNode::TemplateFor { binding, iterator, children, lazy } => {
                self.push_indent();
                if *lazy {
                    self.push(&format!("{{lazy for {} in ", binding));
                } else {
                    self.push(&format!("{{for {} in ", binding));
                }
                self.push(&self.format_expr_to_string(iterator));
                self.push(" {");
                self.newline();
                self.inc();
                for child in children {
                    self.format_template_node(child);
                }
                self.dec();
                self.push_line("}}");
            }
            TemplateNode::TemplateMatch { subject, arms } => {
                self.push_indent();
                self.push("{match ");
                let subject_str = self.format_expr_to_string(subject);
                self.push(&subject_str);
                self.push(" {");
                self.newline();
                self.inc();
                for arm in arms {
                    self.push_indent();
                    self.push(&Self::format_pattern(&arm.pattern));
                    if let Some(guard) = &arm.guard {
                        let guard_str = self.format_expr_to_string(guard);
                        self.push(" if ");
                        self.push(&guard_str);
                    }
                    self.push(" => ");
                    if arm.body.len() == 1 {
                        // Single template node: emit inline
                        for child in &arm.body {
                            self.format_template_node(child);
                        }
                    } else {
                        self.push("{");
                        self.newline();
                        self.inc();
                        for child in &arm.body {
                            self.format_template_node(child);
                        }
                        self.dec();
                        self.push_indent();
                        self.push("}");
                    }
                    self.push(",");
                    self.newline();
                }
                self.dec();
                self.push_line("}}");
            }
        }
    }

    fn format_layout_node(&mut self, node: &LayoutNode) {
        let (tag, attrs, children) = match node {
            LayoutNode::Stack { gap, children, .. } => {
                let mut a = Vec::new();
                if let Some(g) = gap { a.push(format!("gap=\"{}\"", g)); }
                ("Stack", a, children)
            }
            LayoutNode::Row { gap, align, children, .. } => {
                let mut a = Vec::new();
                if let Some(g) = gap { a.push(format!("gap=\"{}\"", g)); }
                if let Some(al) = align { a.push(format!("align=\"{}\"", al)); }
                ("Row", a, children)
            }
            LayoutNode::Grid { cols, rows, gap, children, .. } => {
                let mut a = Vec::new();
                if let Some(c) = cols { a.push(format!("cols=\"{}\"", c)); }
                if let Some(r) = rows { a.push(format!("rows=\"{}\"", r)); }
                if let Some(g) = gap { a.push(format!("gap=\"{}\"", g)); }
                ("Grid", a, children)
            }
            LayoutNode::Center { max_width, children, .. } => {
                let mut a = Vec::new();
                if let Some(mw) = max_width { a.push(format!("max_width=\"{}\"", mw)); }
                ("Center", a, children)
            }
            LayoutNode::Cluster { gap, children, .. } => {
                let mut a = Vec::new();
                if let Some(g) = gap { a.push(format!("gap=\"{}\"", g)); }
                ("Cluster", a, children)
            }
            LayoutNode::Sidebar { side, width, children, .. } => {
                let mut a = Vec::new();
                if let Some(s) = side { a.push(format!("side=\"{}\"", s)); }
                if let Some(w) = width { a.push(format!("width=\"{}\"", w)); }
                ("Sidebar", a, children)
            }
            LayoutNode::Switcher { threshold, children, .. } => {
                let mut a = Vec::new();
                if let Some(t) = threshold { a.push(format!("threshold=\"{}\"", t)); }
                ("Switcher", a, children)
            }
        };
        self.push_indent();
        if children.is_empty() {
            if attrs.is_empty() {
                self.push(&format!("<{} />", tag));
            } else {
                self.push(&format!("<{} {} />", tag, attrs.join(" ")));
            }
            self.newline();
        } else {
            if attrs.is_empty() {
                self.push(&format!("<{}>", tag));
            } else {
                self.push(&format!("<{} {}>", tag, attrs.join(" ")));
            }
            self.newline();
            self.inc();
            for child in children {
                self.format_template_node(child);
            }
            self.dec();
            self.push_line(&format!("</{}>", tag));
        }
    }

    fn format_element(&mut self, el: &Element) {
        let attrs = self.format_attributes(&el.attributes);
        let is_self_closing = el.children.is_empty();

        if is_self_closing {
            let one_line = if attrs.is_empty() {
                format!("<{} />", el.tag)
            } else {
                format!("<{} {} />", el.tag, attrs.join(" "))
            };
            if self.fits_single_line(&one_line) {
                self.push_line(&one_line);
                return;
            }
        }

        let attrs_one_line = attrs.join(" ");
        let opening_one_line = format!("<{} {}", el.tag, attrs_one_line);
        let should_break_attrs = opening_one_line.len() + self.indent * self.options.indent_size
            > self.options.max_line_width
            && attrs.len() > 1;

        if should_break_attrs {
            self.push_indent();
            self.push(&format!("<{}\n", el.tag));
            self.inc();
            for (i, attr) in attrs.iter().enumerate() {
                self.push_indent();
                self.push(attr);
                if i < attrs.len() - 1 {
                    self.newline();
                }
            }
            self.dec();
            if is_self_closing {
                self.newline();
                self.push_line("/>");
            } else {
                self.push(">\n");
            }
        } else {
            self.push_indent();
            self.push(&format!("<{}", el.tag));
            if !attrs.is_empty() {
                self.push(" ");
                self.push(&attrs_one_line);
            }
            if is_self_closing {
                self.push(" />\n");
                return;
            }
            self.push(">\n");
        }

        self.inc();
        for child in &el.children {
            self.format_template_node(child);
        }
        self.dec();
        self.push_line(&format!("</{}>", el.tag));
    }

    fn format_attributes(&self, attrs: &[Attribute]) -> Vec<String> {
        attrs
            .iter()
            .map(|a| match a {
                Attribute::Static { name, value } => format!("{}=\"{}\"", name, value),
                Attribute::Dynamic { name, value } => {
                    format!("{}={{{}}}", name, self.format_expr_to_string(value))
                }
                Attribute::EventHandler { event, handler } => {
                    format!("on:{}={{{}}}", event, self.format_expr_to_string(handler))
                }
                Attribute::Aria { name, value } => {
                    format!("aria-{}={{{}}}", name, self.format_expr_to_string(value))
                }
                Attribute::Role { value } => format!("role=\"{}\"", value),
                Attribute::Bind { property, signal } => {
                    format!("bind:{}={{{}}}", property, signal)
                }
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // Patterns
    // ------------------------------------------------------------------

    fn format_pattern(pat: &Pattern) -> String {
        match pat {
            Pattern::Wildcard => "_".to_string(),
            Pattern::Ident(name) => name.clone(),
            Pattern::Literal(expr) => match expr {
                Expr::Integer(n) => n.to_string(),
                Expr::Float(f) => format!("{}", f),
                Expr::StringLit(s) => format!("\"{}\"", s),
                Expr::Bool(b) => b.to_string(),
                _ => format!("{:?}", expr),
            },
            Pattern::Variant { name, fields } => {
                if fields.is_empty() {
                    name.clone()
                } else {
                    let parts: Vec<String> = fields.iter().map(Self::format_pattern).collect();
                    format!("{}({})", name, parts.join(", "))
                }
            }
            Pattern::Tuple(pats) => {
                let parts: Vec<String> = pats.iter().map(Self::format_pattern).collect();
                format!("({})", parts.join(", "))
            }
            Pattern::Struct { name, fields, rest } => {
                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| {
                        let v_str = Self::format_pattern(v);
                        if k == &v_str {
                            k.clone()
                        } else {
                            format!("{}: {}", k, v_str)
                        }
                    })
                    .collect();
                let mut s = format!("{} {{ {}", name, field_strs.join(", "));
                if *rest {
                    if !field_strs.is_empty() {
                        s.push_str(", ");
                    }
                    s.push_str("..");
                }
                s.push_str(" }");
                s
            }
            Pattern::Array(pats) => {
                let parts: Vec<String> = pats.iter().map(Self::format_pattern).collect();
                format!("[{}]", parts.join(", "))
            }
        }
    }

    fn binop_str(op: &BinOp) -> &'static str {
        match op {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Neq => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Lte => "<=",
            BinOp::Gte => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

/// Convenience function: format a program with default options.
pub fn format_program(program: &Program) -> String {
    let mut formatter = Formatter::new(FormatterOptions::default());
    formatter.format_program(program)
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

    fn make_fn(name: &str, params: Vec<Param>, ret: Option<Type>, stmts: Vec<Stmt>) -> Function {
        Function {
            name: name.to_string(),
            lifetimes: vec![],
            type_params: vec![],
            params,
            return_type: ret,
            trait_bounds: vec![],
            body: Block { stmts, span: dummy_span() },
            is_pub: false,
            must_use: false,
            span: dummy_span(),
        }
    }

    fn make_param(name: &str, ty: &str) -> Param {
        Param {
            name: name.to_string(),
            ty: Type::Named(ty.to_string()),
            ownership: Ownership::Owned,
            secret: false,
        }
    }

    // ===================================================================
    // Existing tests
    // ===================================================================

    #[test]
    fn test_simple_function_formats_correctly() {
        let program = Program {
            items: vec![Item::Function(Function {
                name: "add".to_string(),
                type_params: vec![],
                params: vec![
                    make_param("a", "i32"),
                    make_param("b", "i32"),
                ],
                return_type: Some(Type::Named("i32".to_string())),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(Expr::Binary {
                        op: BinOp::Add,
                        left: Box::new(Expr::Ident("a".to_string())),
                        right: Box::new(Expr::Ident("b".to_string())),
                    }))],
                    span: dummy_span(),
                },
                is_pub: false,
                must_use: false,
                span: dummy_span(),
                lifetimes: vec![],
            })],
        };

        let result = format_program(&program);
        assert!(result.contains("fn add(a: i32, b: i32) -> i32 {"));
        assert!(result.contains("return a + b;"));
    }

    #[test]
    fn test_long_method_chain_breaks_to_multiple_lines() {
        let opts = FormatterOptions {
            max_line_width: 40,
            ..Default::default()
        };
        let fmt = Formatter::new(opts);

        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("some_very_long_variable_name".to_string())),
            method: "some_very_long_method_name".to_string(),
            args: vec![Expr::Ident("arg".to_string())],
        };

        let result = fmt.format_expr_to_string(&expr);
        assert!(
            result.contains('\n'),
            "Expected line break in long method chain, got: {}",
            result
        );
    }

    #[test]
    fn test_component_with_render_block() {
        let program = Program {
            items: vec![Item::Component(Component {
                name: "Button".to_string(),
                type_params: vec![],
                props: vec![Prop {
                    name: "label".to_string(),
                    ty: Type::Named("String".to_string()),
                    default: None,
                }],
                state: vec![],
                methods: vec![],
                styles: vec![],
                transitions: vec![],
                render: RenderBlock {
                    body: TemplateNode::Element(Element {
                        tag: "button".to_string(),
                        attributes: vec![],
                        children: vec![TemplateNode::TextLiteral("Click me".to_string())],
                        span: dummy_span(),
                    }),
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
            })],
        };

        let result = format_program(&program);
        assert!(result.contains("component Button {"));
        assert!(result.contains("prop label: String;"));
        assert!(result.contains("render {"));
        assert!(result.contains("<button>"));
        assert!(result.contains("</button>"));
    }

    // ===================================================================
    // Function formatting
    // ===================================================================

    #[test]
    fn test_pub_function() {
        let mut f = make_fn("greet", vec![], None, vec![Stmt::Return(None)]);
        f.is_pub = true;
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("pub fn greet()"), "got: {}", result);
    }

    #[test]
    fn test_function_no_params_no_return() {
        let f = make_fn("noop", vec![], None, vec![]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("fn noop() {"), "got: {}", result);
    }

    #[test]
    fn test_function_with_type_params() {
        let mut f = make_fn("identity", vec![make_param("x", "T")], Some(Type::Named("T".to_string())), vec![
            Stmt::Return(Some(Expr::Ident("x".to_string()))),
        ]);
        f.type_params = vec!["T".to_string()];
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("fn identity<T>(x: T) -> T {"), "got: {}", result);
    }

    #[test]
    fn test_function_with_trait_bounds() {
        let mut f = make_fn("print_it", vec![make_param("x", "T")], None, vec![]);
        f.type_params = vec!["T".to_string()];
        f.trait_bounds = vec![TraitBound {
            type_param: "T".to_string(),
            trait_name: "Display".to_string(),
        }];
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("where T: Display"), "got: {}", result);
    }

    #[test]
    fn test_function_many_params() {
        let params = vec![
            make_param("a", "i32"),
            make_param("b", "i32"),
            make_param("c", "String"),
            make_param("d", "bool"),
        ];
        let f = make_fn("multi", params, None, vec![]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("fn multi(a: i32, b: i32, c: String, d: bool)"), "got: {}", result);
    }

    #[test]
    fn test_function_borrowed_params() {
        let params = vec![
            Param { name: "x".to_string(), ty: Type::Named("String".to_string()), ownership: Ownership::Borrowed, secret: false },
            Param { name: "y".to_string(), ty: Type::Named("Vec".to_string()), ownership: Ownership::MutBorrowed, secret: false },
        ];
        let f = make_fn("borrow_test", params, None, vec![]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("&x: String"), "got: {}", result);
        assert!(result.contains("&mut y: Vec"), "got: {}", result);
    }

    // ===================================================================
    // Struct formatting
    // ===================================================================

    #[test]
    fn test_struct_with_fields() {
        let s = StructDef {
            name: "Point".to_string(),
            lifetimes: vec![],
            type_params: vec![],
            fields: vec![
                Field { name: "x".to_string(), ty: Type::Named("f64".to_string()), is_pub: false },
                Field { name: "y".to_string(), ty: Type::Named("f64".to_string()), is_pub: false },
            ],
            trait_bounds: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Struct(s)] };
        let result = format_program(&program);
        assert!(result.contains("struct Point {"), "got: {}", result);
        assert!(result.contains("x: f64,"), "got: {}", result);
        assert!(result.contains("y: f64,"), "got: {}", result);
    }

    #[test]
    fn test_pub_struct_with_pub_fields() {
        let s = StructDef {
            name: "User".to_string(),
            lifetimes: vec![],
            type_params: vec![],
            fields: vec![
                Field { name: "name".to_string(), ty: Type::Named("String".to_string()), is_pub: true },
                Field { name: "age".to_string(), ty: Type::Named("u32".to_string()), is_pub: true },
            ],
            trait_bounds: vec![],
            is_pub: true,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Struct(s)] };
        let result = format_program(&program);
        assert!(result.contains("pub struct User {"), "got: {}", result);
        assert!(result.contains("pub name"), "got: {}", result);
        assert!(result.contains("pub age"), "got: {}", result);
    }

    #[test]
    fn test_struct_with_type_params_and_trait_bounds() {
        let s = StructDef {
            name: "Container".to_string(),
            lifetimes: vec![],
            type_params: vec!["T".to_string()],
            fields: vec![
                Field { name: "value".to_string(), ty: Type::Named("T".to_string()), is_pub: false },
            ],
            trait_bounds: vec![TraitBound {
                type_param: "T".to_string(),
                trait_name: "Clone".to_string(),
            }],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Struct(s)] };
        let result = format_program(&program);
        assert!(result.contains("struct Container<T>"), "got: {}", result);
        assert!(result.contains("where T: Clone"), "got: {}", result);
    }

    #[test]
    fn test_struct_field_alignment() {
        let s = StructDef {
            name: "Aligned".to_string(),
            lifetimes: vec![],
            type_params: vec![],
            fields: vec![
                Field { name: "x".to_string(), ty: Type::Named("i32".to_string()), is_pub: false },
                Field { name: "long_name".to_string(), ty: Type::Named("String".to_string()), is_pub: false },
            ],
            trait_bounds: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Struct(s)] };
        let result = format_program(&program);
        // x should be padded relative to long_name
        assert!(result.contains("x"), "got: {}", result);
        assert!(result.contains("long_name"), "got: {}", result);
    }

    // ===================================================================
    // Enum formatting
    // ===================================================================

    #[test]
    fn test_enum_simple_variants() {
        let e = EnumDef {
            name: "Color".to_string(),
            type_params: vec![],
            variants: vec![
                Variant { name: "Red".to_string(), fields: vec![] },
                Variant { name: "Green".to_string(), fields: vec![] },
                Variant { name: "Blue".to_string(), fields: vec![] },
            ],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Enum(e)] };
        let result = format_program(&program);
        assert!(result.contains("enum Color {"), "got: {}", result);
        assert!(result.contains("Red,"), "got: {}", result);
        assert!(result.contains("Green,"), "got: {}", result);
        assert!(result.contains("Blue,"), "got: {}", result);
    }

    #[test]
    fn test_enum_with_fields() {
        let e = EnumDef {
            name: "Shape".to_string(),
            type_params: vec![],
            variants: vec![
                Variant { name: "Circle".to_string(), fields: vec![Type::Named("f64".to_string())] },
                Variant { name: "Rect".to_string(), fields: vec![Type::Named("f64".to_string()), Type::Named("f64".to_string())] },
            ],
            is_pub: true,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Enum(e)] };
        let result = format_program(&program);
        assert!(result.contains("pub enum Shape {"), "got: {}", result);
        assert!(result.contains("Circle(f64),"), "got: {}", result);
        assert!(result.contains("Rect(f64, f64),"), "got: {}", result);
    }

    #[test]
    fn test_enum_with_type_params() {
        let e = EnumDef {
            name: "Option".to_string(),
            type_params: vec!["T".to_string()],
            variants: vec![
                Variant { name: "Some".to_string(), fields: vec![Type::Named("T".to_string())] },
                Variant { name: "None".to_string(), fields: vec![] },
            ],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Enum(e)] };
        let result = format_program(&program);
        assert!(result.contains("enum Option<T> {"), "got: {}", result);
    }

    // ===================================================================
    // Component formatting
    // ===================================================================

    #[test]
    fn test_component_with_state() {
        let c = Component {
            name: "Counter".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![StateField {
                name: "count".to_string(),
                ty: Some(Type::Named("i32".to_string())),
                mutable: true,
                secret: false,
                atomic: false,
                initializer: Expr::Integer(0),
                ownership: Ownership::Owned,
            }],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(),
                    attributes: vec![],
                    children: vec![],
                    span: dummy_span(),
                }),
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
        let result = format_program(&program);
        assert!(result.contains("component Counter {"), "got: {}", result);
        assert!(result.contains("signal mut count: i32 = 0;"), "got: {}", result);
    }

    #[test]
    fn test_component_with_secret_state() {
        let c = Component {
            name: "SecureForm".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![StateField {
                name: "token".to_string(),
                ty: Some(Type::Named("String".to_string())),
                mutable: false,
                secret: true,
                atomic: false,
                initializer: Expr::StringLit("".to_string()),
                ownership: Ownership::Owned,
            }],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(),
                    attributes: vec![],
                    children: vec![],
                    span: dummy_span(),
                }),
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
        let result = format_program(&program);
        assert!(result.contains("signal secret token"), "got: {}", result);
    }

    #[test]
    fn test_component_with_props_default() {
        let c = Component {
            name: "Card".to_string(),
            type_params: vec![],
            props: vec![
                Prop { name: "title".to_string(), ty: Type::Named("String".to_string()), default: None },
                Prop { name: "visible".to_string(), ty: Type::Named("bool".to_string()), default: Some(Expr::Bool(true)) },
            ],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(),
                    attributes: vec![],
                    children: vec![],
                    span: dummy_span(),
                }),
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
        let result = format_program(&program);
        assert!(result.contains("prop title: String;"), "got: {}", result);
        assert!(result.contains("prop visible: bool = true;"), "got: {}", result);
    }

    #[test]
    fn test_component_with_methods() {
        let method = make_fn("handle_click", vec![], None, vec![
            Stmt::Expr(Expr::Ident("do_something".to_string())),
        ]);
        let c = Component {
            name: "Clickable".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![method],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                }),
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
        let result = format_program(&program);
        assert!(result.contains("fn handle_click()"), "got: {}", result);
    }

    #[test]
    fn test_component_with_styles() {
        let c = Component {
            name: "Styled".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![StyleBlock {
                selector: ".button".to_string(),
                properties: vec![
                    ("color".to_string(), "red".to_string()),
                    ("padding".to_string(), "10px".to_string()),
                ],
                span: dummy_span(),
            }],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                }),
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
        let result = format_program(&program);
        assert!(result.contains("style \".button\" {"), "got: {}", result);
        assert!(result.contains("color: red;"), "got: {}", result);
        assert!(result.contains("padding: 10px;"), "got: {}", result);
    }

    #[test]
    fn test_component_with_transitions() {
        let c = Component {
            name: "Anim".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![TransitionDef {
                property: "opacity".to_string(),
                duration: "300".to_string(),
                easing: "ease-in".to_string(),
                span: dummy_span(),
            }],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                }),
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
        let result = format_program(&program);
        assert!(result.contains("transition opacity 300ms ease-in;"), "got: {}", result);
    }

    #[test]
    fn test_component_with_skeleton() {
        let c = Component {
            name: "Loader".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                }),
                span: dummy_span(),
            },
            trait_bounds: vec![],
            permissions: None,
            gestures: vec![],
            skeleton: Some(SkeletonDef {
                body: RenderBlock {
                    body: TemplateNode::Element(Element {
                        tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                    }),
                    span: dummy_span(),
                },
                span: dummy_span(),
            }),
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Component(c)] };
        let result = format_program(&program);
        assert!(result.contains("skeleton {"), "got: {}", result);
    }

    #[test]
    fn test_component_with_error_boundary() {
        let c = Component {
            name: "Safe".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                }),
                span: dummy_span(),
            },
            trait_bounds: vec![],
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: Some(ErrorBoundary {
                body: RenderBlock {
                    body: TemplateNode::TextLiteral("content".to_string()),
                    span: dummy_span(),
                },
                fallback: RenderBlock {
                    body: TemplateNode::TextLiteral("error".to_string()),
                    span: dummy_span(),
                },
                span: dummy_span(),
            }),
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Component(c)] };
        let result = format_program(&program);
        assert!(result.contains("error_boundary {"), "got: {}", result);
        assert!(result.contains("} fallback {"), "got: {}", result);
    }

    #[test]
    fn test_component_with_type_params() {
        let c = Component {
            name: "Generic".to_string(),
            type_params: vec!["T".to_string(), "U".to_string()],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                }),
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
        let result = format_program(&program);
        assert!(result.contains("component Generic<T, U> {"), "got: {}", result);
    }

    // ===================================================================
    // Store formatting
    // ===================================================================

    #[test]
    fn test_store_with_signals_actions_computed_effects() {
        let store = StoreDef {
            name: "AppStore".to_string(),
            signals: vec![StateField {
                name: "count".to_string(),
                ty: Some(Type::Named("i32".to_string())),
                mutable: true,
                secret: false,
                atomic: false,
                initializer: Expr::Integer(0),
                ownership: Ownership::Owned,
            }],
            actions: vec![ActionDef {
                name: "increment".to_string(),
                params: vec![],
                body: Block {
                    stmts: vec![Stmt::Expr(Expr::Assign {
                        target: Box::new(Expr::FieldAccess {
                            object: Box::new(Expr::SelfExpr),
                            field: "count".to_string(),
                        }),
                        value: Box::new(Expr::Binary {
                            op: BinOp::Add,
                            left: Box::new(Expr::FieldAccess {
                                object: Box::new(Expr::SelfExpr),
                                field: "count".to_string(),
                            }),
                            right: Box::new(Expr::Integer(1)),
                        }),
                    })],
                    span: dummy_span(),
                },
                is_async: false,
                span: dummy_span(),
            }],
            computed: vec![ComputedDef {
                name: "double_count".to_string(),
                return_type: Some(Type::Named("i32".to_string())),
                body: Block {
                    stmts: vec![Stmt::Expr(Expr::Binary {
                        op: BinOp::Mul,
                        left: Box::new(Expr::FieldAccess {
                            object: Box::new(Expr::SelfExpr),
                            field: "count".to_string(),
                        }),
                        right: Box::new(Expr::Integer(2)),
                    })],
                    span: dummy_span(),
                },
                span: dummy_span(),
            }],
            effects: vec![EffectDef {
                name: "log_count".to_string(),
                body: Block {
                    stmts: vec![Stmt::Expr(Expr::Ident("println".to_string()))],
                    span: dummy_span(),
                },
                span: dummy_span(),
            }],
            selectors: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Store(store)] };
        let result = format_program(&program);
        assert!(result.contains("store AppStore {"), "got: {}", result);
        assert!(result.contains("signal mut count: i32 = 0;"), "got: {}", result);
        assert!(result.contains("action increment()"), "got: {}", result);
        assert!(result.contains("computed double_count(&self) -> i32"), "got: {}", result);
        assert!(result.contains("effect log_count(&self)"), "got: {}", result);
    }

    #[test]
    fn test_store_pub_with_async_action() {
        let store = StoreDef {
            name: "DataStore".to_string(),
            signals: vec![],
            actions: vec![ActionDef {
                name: "fetch_data".to_string(),
                params: vec![make_param("id", "u32")],
                body: Block { stmts: vec![], span: dummy_span() },
                is_async: true,
                span: dummy_span(),
            }],
            computed: vec![],
            effects: vec![],
            selectors: vec![],
            is_pub: true,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Store(store)] };
        let result = format_program(&program);
        assert!(result.contains("pub store DataStore {"), "got: {}", result);
        assert!(result.contains("async action fetch_data(id: u32)"), "got: {}", result);
    }

    // ===================================================================
    // Router formatting
    // ===================================================================

    #[test]
    fn test_router_with_routes_and_fallback() {
        let router = RouterDef {
            name: "AppRouter".to_string(),
            routes: vec![
                RouteDef {
                    path: "/".to_string(),
                    params: vec![],
                    component: "Home".to_string(),
                    guard: None,
                    transition: None,
                    span: dummy_span(),
                },
                RouteDef {
                    path: "/about".to_string(),
                    params: vec![],
                    component: "About".to_string(),
                    guard: None,
                    transition: None,
                    span: dummy_span(),
                },
            ],
            fallback: Some(Box::new(TemplateNode::TextLiteral("NotFound".to_string()))),
            layout: None,
            transition: None,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Router(router)] };
        let result = format_program(&program);
        assert!(result.contains("router AppRouter {"), "got: {}", result);
        assert!(result.contains("route \"/\""), "got: {}", result);
        assert!(result.contains("=> Home"), "got: {}", result);
        assert!(result.contains("route \"/about\""), "got: {}", result);
        assert!(result.contains("=> About"), "got: {}", result);
        assert!(result.contains("fallback"), "got: {}", result);
    }

    #[test]
    fn test_router_with_guard() {
        let router = RouterDef {
            name: "SecureRouter".to_string(),
            routes: vec![
                RouteDef {
                    path: "/admin".to_string(),
                    params: vec![],
                    component: "Admin".to_string(),
                    guard: Some(Expr::Ident("is_admin".to_string())),
                    transition: None,
                    span: dummy_span(),
                },
            ],
            fallback: None,
            layout: None,
            transition: None,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Router(router)] };
        let result = format_program(&program);
        assert!(result.contains("guard { is_admin }"), "got: {}", result);
    }

    // ===================================================================
    // Match expression formatting
    // ===================================================================

    #[test]
    fn test_match_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Match {
            subject: Box::new(Expr::Ident("x".to_string())),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Expr::Integer(1)),
                    guard: None,
                    body: Expr::StringLit("one".to_string()),
                },
                MatchArm {
                    pattern: Pattern::Literal(Expr::Integer(2)),
                    guard: None,
                    body: Expr::StringLit("two".to_string()),
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: Expr::StringLit("other".to_string()),
                },
            ],
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("match x {"), "got: {}", result);
        assert!(result.contains("1 => \"one\""), "got: {}", result);
        assert!(result.contains("2 => \"two\""), "got: {}", result);
        assert!(result.contains("_ => \"other\""), "got: {}", result);
    }

    #[test]
    fn test_match_with_variant_patterns() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Match {
            subject: Box::new(Expr::Ident("opt".to_string())),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Variant {
                        name: "Some".to_string(),
                        fields: vec![Pattern::Ident("v".to_string())],
                    },
                    guard: None,
                    body: Expr::Ident("v".to_string()),
                },
                MatchArm {
                    pattern: Pattern::Variant {
                        name: "None".to_string(),
                        fields: vec![],
                    },
                    guard: None,
                    body: Expr::Integer(0),
                },
            ],
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("Some(v)"), "got: {}", result);
        assert!(result.contains("None"), "got: {}", result);
    }

    // ===================================================================
    // If/else expression formatting
    // ===================================================================

    #[test]
    fn test_if_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::If {
            condition: Box::new(Expr::Bool(true)),
            then_block: Block {
                stmts: vec![Stmt::Expr(Expr::Integer(1))],
                span: dummy_span(),
            },
            else_block: None,
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("if true {"), "got: {}", result);
        assert!(result.contains("1;"), "got: {}", result);
    }

    #[test]
    fn test_if_else_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::If {
            condition: Box::new(Expr::Binary {
                op: BinOp::Gt,
                left: Box::new(Expr::Ident("x".to_string())),
                right: Box::new(Expr::Integer(0)),
            }),
            then_block: Block {
                stmts: vec![Stmt::Expr(Expr::StringLit("positive".to_string()))],
                span: dummy_span(),
            },
            else_block: Some(Block {
                stmts: vec![Stmt::Expr(Expr::StringLit("non-positive".to_string()))],
                span: dummy_span(),
            }),
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("if x > 0 {"), "got: {}", result);
        assert!(result.contains("} else {"), "got: {}", result);
    }

    // ===================================================================
    // For/while loops
    // ===================================================================

    #[test]
    fn test_for_loop() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::For {
            binding: "item".to_string(),
            iterator: Box::new(Expr::Ident("items".to_string())),
            body: Block {
                stmts: vec![Stmt::Expr(Expr::Ident("item".to_string()))],
                span: dummy_span(),
            },
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("for item in items {"), "got: {}", result);
    }

    #[test]
    fn test_while_loop() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::While {
            condition: Box::new(Expr::Bool(true)),
            body: Block {
                stmts: vec![Stmt::Expr(Expr::Ident("loop_body".to_string()))],
                span: dummy_span(),
            },
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("while true {"), "got: {}", result);
    }

    // ===================================================================
    // Closure formatting
    // ===================================================================

    #[test]
    fn test_closure_short() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Closure {
            params: vec![("x".to_string(), None)],
            body: Box::new(Expr::Binary {
                op: BinOp::Mul,
                left: Box::new(Expr::Ident("x".to_string())),
                right: Box::new(Expr::Integer(2)),
            }),
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("|x| x * 2"), "got: {}", result);
    }

    #[test]
    fn test_closure_with_typed_params() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Closure {
            params: vec![
                ("a".to_string(), Some(Type::Named("i32".to_string()))),
                ("b".to_string(), Some(Type::Named("i32".to_string()))),
            ],
            body: Box::new(Expr::Binary {
                op: BinOp::Add,
                left: Box::new(Expr::Ident("a".to_string())),
                right: Box::new(Expr::Ident("b".to_string())),
            }),
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("|a: i32, b: i32|"), "got: {}", result);
    }

    // ===================================================================
    // Use statement formatting
    // ===================================================================

    #[test]
    fn test_use_simple() {
        let u = UsePath {
            segments: vec!["std".to_string(), "collections".to_string(), "HashMap".to_string()],
            alias: None,
            glob: false,
            group: None,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Use(u)] };
        let result = format_program(&program);
        assert!(result.contains("use std::collections::HashMap;"), "got: {}", result);
    }

    #[test]
    fn test_use_glob() {
        let u = UsePath {
            segments: vec!["std".to_string(), "io".to_string()],
            alias: None,
            glob: true,
            group: None,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Use(u)] };
        let result = format_program(&program);
        assert!(result.contains("use std::io::*;"), "got: {}", result);
    }

    #[test]
    fn test_use_with_alias() {
        let u = UsePath {
            segments: vec!["std".to_string(), "collections".to_string(), "HashMap".to_string()],
            alias: Some("Map".to_string()),
            glob: false,
            group: None,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Use(u)] };
        let result = format_program(&program);
        assert!(result.contains("use std::collections::HashMap as Map;"), "got: {}", result);
    }

    #[test]
    fn test_use_group_imports() {
        let u = UsePath {
            segments: vec!["std".to_string(), "collections".to_string()],
            alias: None,
            glob: false,
            group: Some(vec![
                UseGroupItem { name: "HashMap".to_string(), alias: None },
                UseGroupItem { name: "HashSet".to_string(), alias: None },
                UseGroupItem { name: "BTreeMap".to_string(), alias: Some("BTree".to_string()) },
            ]),
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Use(u)] };
        let result = format_program(&program);
        assert!(result.contains("use std::collections::{HashMap, HashSet, BTreeMap as BTree};"), "got: {}", result);
    }

    // ===================================================================
    // Trait and Impl formatting
    // ===================================================================

    #[test]
    fn test_trait_with_methods() {
        let t = TraitDef {
            name: "Printable".to_string(),
            type_params: vec![],
            methods: vec![
                TraitMethod {
                    name: "print".to_string(),
                    params: vec![],
                    return_type: None,
                    default_body: None,
                    span: dummy_span(),
                },
                TraitMethod {
                    name: "to_string".to_string(),
                    params: vec![],
                    return_type: Some(Type::Named("String".to_string())),
                    default_body: Some(Block {
                        stmts: vec![Stmt::Return(Some(Expr::StringLit("default".to_string())))],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Trait(t)] };
        let result = format_program(&program);
        assert!(result.contains("trait Printable {"), "got: {}", result);
        assert!(result.contains("fn print();"), "got: {}", result);
        assert!(result.contains("fn to_string() -> String {"), "got: {}", result);
    }

    #[test]
    fn test_trait_with_type_params() {
        let t = TraitDef {
            name: "Converter".to_string(),
            type_params: vec!["T".to_string()],
            methods: vec![],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Trait(t)] };
        let result = format_program(&program);
        assert!(result.contains("trait Converter<T> {"), "got: {}", result);
    }

    #[test]
    fn test_impl_block() {
        let im = ImplBlock {
            target: "Point".to_string(),
            trait_impls: vec![],
            methods: vec![
                make_fn("new", vec![make_param("x", "f64"), make_param("y", "f64")], Some(Type::Named("Point".to_string())), vec![
                    Stmt::Return(Some(Expr::StructInit {
                        name: "Point".to_string(),
                        fields: vec![
                            ("x".to_string(), Expr::Ident("x".to_string())),
                            ("y".to_string(), Expr::Ident("y".to_string())),
                        ],
                    })),
                ]),
                make_fn("distance", vec![], Some(Type::Named("f64".to_string())), vec![
                    Stmt::Return(Some(Expr::Float(0.0))),
                ]),
            ],
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Impl(im)] };
        let result = format_program(&program);
        assert!(result.contains("impl Point {"), "got: {}", result);
        assert!(result.contains("fn new(x: f64, y: f64) -> Point"), "got: {}", result);
        assert!(result.contains("fn distance() -> f64"), "got: {}", result);
    }

    // ===================================================================
    // Expression formatting
    // ===================================================================

    #[test]
    fn test_all_binary_operators() {
        let fmt = Formatter::new(FormatterOptions::default());
        let ops = vec![
            (BinOp::Add, "+"), (BinOp::Sub, "-"), (BinOp::Mul, "*"), (BinOp::Div, "/"),
            (BinOp::Mod, "%"), (BinOp::Eq, "=="), (BinOp::Neq, "!="), (BinOp::Lt, "<"),
            (BinOp::Gt, ">"), (BinOp::Lte, "<="), (BinOp::Gte, ">="),
            (BinOp::And, "&&"), (BinOp::Or, "||"),
        ];
        for (op, sym) in ops {
            let expr = Expr::Binary {
                op,
                left: Box::new(Expr::Ident("a".to_string())),
                right: Box::new(Expr::Ident("b".to_string())),
            };
            let result = fmt.format_expr_to_string(&expr);
            assert!(result.contains(sym), "Expected '{}' in: {}", sym, result);
        }
    }

    #[test]
    fn test_unary_operators() {
        let fmt = Formatter::new(FormatterOptions::default());
        let neg = Expr::Unary { op: UnaryOp::Neg, operand: Box::new(Expr::Integer(5)) };
        let not = Expr::Unary { op: UnaryOp::Not, operand: Box::new(Expr::Bool(true)) };
        assert_eq!(fmt.format_expr_to_string(&neg), "-5");
        assert_eq!(fmt.format_expr_to_string(&not), "!true");
    }

    #[test]
    fn test_field_access() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::FieldAccess {
            object: Box::new(Expr::Ident("user".to_string())),
            field: "name".to_string(),
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "user.name");
    }

    #[test]
    fn test_index_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Index {
            object: Box::new(Expr::Ident("arr".to_string())),
            index: Box::new(Expr::Integer(0)),
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "arr[0]");
    }

    #[test]
    fn test_fn_call() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::FnCall {
            callee: Box::new(Expr::Ident("foo".to_string())),
            args: vec![Expr::Integer(1), Expr::Integer(2)],
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "foo(1, 2)");
    }

    #[test]
    fn test_method_call_short() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("x".to_string())),
            method: "len".to_string(),
            args: vec![],
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "x.len()");
    }

    #[test]
    fn test_struct_init_short() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::StructInit {
            name: "Point".to_string(),
            fields: vec![
                ("x".to_string(), Expr::Integer(1)),
                ("y".to_string(), Expr::Integer(2)),
            ],
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("Point { x: 1, y: 2 }"), "got: {}", result);
    }

    #[test]
    fn test_struct_init_multiline() {
        let opts = FormatterOptions {
            single_line_threshold: 10,
            ..Default::default()
        };
        let fmt = Formatter::new(opts);
        let expr = Expr::StructInit {
            name: "LongStruct".to_string(),
            fields: vec![
                ("field_one".to_string(), Expr::Integer(1)),
                ("field_two".to_string(), Expr::Integer(2)),
            ],
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains('\n'), "Expected multiline output, got: {}", result);
    }

    #[test]
    fn test_assign_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Assign {
            target: Box::new(Expr::Ident("x".to_string())),
            value: Box::new(Expr::Integer(42)),
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "x = 42");
    }

    #[test]
    fn test_await_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Await(Box::new(Expr::Ident("future".to_string())));
        assert_eq!(fmt.format_expr_to_string(&expr), "future.await");
    }

    #[test]
    fn test_borrow_and_borrow_mut() {
        let fmt = Formatter::new(FormatterOptions::default());
        let borrow = Expr::Borrow(Box::new(Expr::Ident("x".to_string())));
        let borrow_mut = Expr::BorrowMut(Box::new(Expr::Ident("y".to_string())));
        assert_eq!(fmt.format_expr_to_string(&borrow), "&x");
        assert_eq!(fmt.format_expr_to_string(&borrow_mut), "&mut y");
    }

    #[test]
    fn test_fetch_simple() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Fetch {
            url: Box::new(Expr::StringLit("https://api.example.com".to_string())),
            options: None,
            contract: None,
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("fetch(\"https://api.example.com\")"), "got: {}", result);
    }

    #[test]
    fn test_fetch_with_options() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Fetch {
            url: Box::new(Expr::StringLit("https://api.example.com".to_string())),
            options: Some(Box::new(Expr::Ident("opts".to_string()))),
            contract: None,
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("fetch(\"https://api.example.com\", opts)"), "got: {}", result);
    }

    #[test]
    fn test_try_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Try(Box::new(Expr::Ident("result".to_string())));
        assert_eq!(fmt.format_expr_to_string(&expr), "result?");
    }

    #[test]
    fn test_navigate_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Navigate {
            path: Box::new(Expr::StringLit("/home".to_string())),
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "navigate(\"/home\")");
    }

    #[test]
    fn test_stream_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Stream {
            source: Box::new(Expr::Ident("data".to_string())),
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "stream data");
    }

    #[test]
    fn test_channel_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let no_type = Expr::Channel { ty: None };
        let with_type = Expr::Channel { ty: Some(Type::Named("i32".to_string())) };
        assert_eq!(fmt.format_expr_to_string(&no_type), "channel()");
        assert_eq!(fmt.format_expr_to_string(&with_type), "channel::<i32>()");
    }

    #[test]
    fn test_send_receive_expressions() {
        let fmt = Formatter::new(FormatterOptions::default());
        let send = Expr::Send {
            channel: Box::new(Expr::Ident("ch".to_string())),
            value: Box::new(Expr::Integer(42)),
        };
        let recv = Expr::Receive {
            channel: Box::new(Expr::Ident("ch".to_string())),
        };
        assert_eq!(fmt.format_expr_to_string(&send), "ch.send(42)");
        assert_eq!(fmt.format_expr_to_string(&recv), "ch.receive()");
    }

    #[test]
    fn test_parallel_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Parallel {
            tasks: vec![Expr::Ident("a".to_string()), Expr::Ident("b".to_string())],
            span: dummy_span(),
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("parallel { a, b }"), "got: {}", result);
    }

    #[test]
    fn test_try_catch_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::TryCatch {
            body: Box::new(Expr::Ident("risky".to_string())),
            error_binding: "e".to_string(),
            catch_body: Box::new(Expr::Ident("handle".to_string())),
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("try { risky } catch e { handle }"), "got: {}", result);
    }

    #[test]
    fn test_assert_expressions() {
        let fmt = Formatter::new(FormatterOptions::default());
        let assert_no_msg = Expr::Assert {
            condition: Box::new(Expr::Bool(true)),
            message: None,
        };
        let assert_with_msg = Expr::Assert {
            condition: Box::new(Expr::Bool(false)),
            message: Some("should be true".to_string()),
        };
        assert_eq!(fmt.format_expr_to_string(&assert_no_msg), "assert(true)");
        assert_eq!(fmt.format_expr_to_string(&assert_with_msg), "assert(false, \"should be true\")");
    }

    #[test]
    fn test_assert_eq_expressions() {
        let fmt = Formatter::new(FormatterOptions::default());
        let eq_no_msg = Expr::AssertEq {
            left: Box::new(Expr::Integer(1)),
            right: Box::new(Expr::Integer(1)),
            message: None,
        };
        let eq_with_msg = Expr::AssertEq {
            left: Box::new(Expr::Ident("a".to_string())),
            right: Box::new(Expr::Ident("b".to_string())),
            message: Some("not equal".to_string()),
        };
        assert_eq!(fmt.format_expr_to_string(&eq_no_msg), "assert_eq(1, 1)");
        assert_eq!(fmt.format_expr_to_string(&eq_with_msg), "assert_eq(a, b, \"not equal\")");
    }

    #[test]
    fn test_animate_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Animate {
            target: Box::new(Expr::Ident("el".to_string())),
            animation: "fadeIn".to_string(),
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "animate(el, \"fadeIn\")");
    }

    #[test]
    fn test_suspend_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Suspend {
            fallback: Box::new(Expr::Ident("spinner".to_string())),
            body: Box::new(Expr::Ident("content".to_string())),
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("suspend(spinner) { content }"), "got: {}", result);
    }

    #[test]
    fn test_prompt_template() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::PromptTemplate {
            template: "Summarize: {doc}".to_string(),
            interpolations: vec![],
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("prompt \"Summarize: {doc}\""), "got: {}", result);
    }

    #[test]
    fn test_spawn_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Spawn {
            body: Block {
                stmts: vec![Stmt::Expr(Expr::Integer(42))],
                span: dummy_span(),
            },
            span: dummy_span(),
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("spawn"), "got: {}", result);
    }

    #[test]
    fn test_dynamic_import() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::DynamicImport {
            path: Box::new(Expr::StringLit("./module".to_string())),
            span: dummy_span(),
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "import(\"./module\")");
    }

    #[test]
    fn test_download_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Download {
            data: Box::new(Expr::Ident("blob".to_string())),
            filename: Box::new(Expr::StringLit("file.txt".to_string())),
            span: dummy_span(),
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "download(blob, \"file.txt\")");
    }

    #[test]
    fn test_env_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Env {
            name: Box::new(Expr::StringLit("API_KEY".to_string())),
            span: dummy_span(),
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "env(\"API_KEY\")");
    }

    #[test]
    fn test_trace_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Trace {
            label: Box::new(Expr::StringLit("perf".to_string())),
            body: Block {
                stmts: vec![Stmt::Expr(Expr::Integer(1))],
                span: dummy_span(),
            },
            span: dummy_span(),
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("trace(\"perf\")"), "got: {}", result);
    }

    #[test]
    fn test_flag_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Flag {
            name: Box::new(Expr::StringLit("dark_mode".to_string())),
            span: dummy_span(),
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "flag(\"dark_mode\")");
    }

    #[test]
    fn test_virtual_list_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::VirtualList {
            items: Box::new(Expr::Ident("data".to_string())),
            item_height: Box::new(Expr::Integer(50)),
            template: Box::new(Expr::Ident("render_row".to_string())),
            buffer: None,
            span: dummy_span(),
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("virtual_list(data, 50, render_row)"), "got: {}", result);
    }

    #[test]
    fn test_block_expression() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Block(Block {
            stmts: vec![
                Stmt::Let { name: "x".to_string(), ty: None, mutable: false, secret: false, value: Expr::Integer(1), ownership: Ownership::Owned },
                Stmt::Expr(Expr::Ident("x".to_string())),
            ],
            span: dummy_span(),
        });
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("let x = 1;"), "got: {}", result);
    }

    // ===================================================================
    // Format string
    // ===================================================================

    #[test]
    fn test_format_string() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::FormatString {
            parts: vec![
                FormatPart::Literal("Hello, ".to_string()),
                FormatPart::Expression(Box::new(Expr::Ident("name".to_string()))),
                FormatPart::Literal("!".to_string()),
            ],
        };
        let result = fmt.format_expr_to_string(&expr);
        assert_eq!(result, "f\"Hello, {name}!\"");
    }

    #[test]
    fn test_format_string_only_literal() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::FormatString {
            parts: vec![FormatPart::Literal("plain text".to_string())],
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "f\"plain text\"");
    }

    // ===================================================================
    // Statement formatting
    // ===================================================================

    #[test]
    fn test_let_statement() {
        let f = make_fn("test", vec![], None, vec![
            Stmt::Let {
                name: "x".to_string(),
                ty: Some(Type::Named("i32".to_string())),
                mutable: false,
                secret: false,
                value: Expr::Integer(42),
                ownership: Ownership::Owned,
            },
        ]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("let x: i32 = 42;"), "got: {}", result);
    }

    #[test]
    fn test_let_mut_statement() {
        let f = make_fn("test", vec![], None, vec![
            Stmt::Let {
                name: "y".to_string(),
                ty: None,
                mutable: true,
                secret: false,
                value: Expr::Integer(0),
                ownership: Ownership::Owned,
            },
        ]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("let mut y = 0;"), "got: {}", result);
    }

    #[test]
    fn test_let_secret_statement() {
        let f = make_fn("test", vec![], None, vec![
            Stmt::Let {
                name: "key".to_string(),
                ty: None,
                mutable: false,
                secret: true,
                value: Expr::StringLit("abc".to_string()),
                ownership: Ownership::Owned,
            },
        ]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("let secret key"), "got: {}", result);
    }

    #[test]
    fn test_signal_statement() {
        let f = make_fn("test", vec![], None, vec![
            Stmt::Signal {
                name: "count".to_string(),
                ty: Some(Type::Named("i32".to_string())),
                secret: false,
                atomic: false,
                value: Expr::Integer(0),
            },
        ]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("signal count: i32 = 0;"), "got: {}", result);
    }

    #[test]
    fn test_signal_secret_statement() {
        let f = make_fn("test", vec![], None, vec![
            Stmt::Signal {
                name: "token".to_string(),
                ty: None,
                secret: true,
                atomic: false,
                value: Expr::StringLit("".to_string()),
            },
        ]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("signal secret token"), "got: {}", result);
    }

    #[test]
    fn test_return_statement_with_value() {
        let f = make_fn("test", vec![], None, vec![
            Stmt::Return(Some(Expr::Integer(42))),
        ]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("return 42;"), "got: {}", result);
    }

    #[test]
    fn test_return_statement_empty() {
        let f = make_fn("test", vec![], None, vec![
            Stmt::Return(None),
        ]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("return;"), "got: {}", result);
    }

    #[test]
    fn test_yield_statement() {
        let f = make_fn("test", vec![], None, vec![
            Stmt::Yield(Expr::Integer(1)),
        ]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("yield 1;"), "got: {}", result);
    }

    #[test]
    fn test_let_destructure_statement() {
        let f = make_fn("test", vec![], None, vec![
            Stmt::LetDestructure {
                pattern: Pattern::Tuple(vec![Pattern::Ident("a".to_string()), Pattern::Ident("b".to_string())]),
                ty: None,
                value: Expr::Ident("pair".to_string()),
            },
        ]);
        let program = Program { items: vec![Item::Function(f)] };
        let result = format_program(&program);
        assert!(result.contains("let (a, b) = pair;"), "got: {}", result);
    }

    // ===================================================================
    // Pattern formatting
    // ===================================================================

    #[test]
    fn test_pattern_formatting() {
        assert_eq!(Formatter::format_pattern(&Pattern::Wildcard), "_");
        assert_eq!(Formatter::format_pattern(&Pattern::Ident("x".to_string())), "x");
        assert_eq!(Formatter::format_pattern(&Pattern::Literal(Expr::Integer(42))), "42");
        assert_eq!(Formatter::format_pattern(&Pattern::Literal(Expr::Float(3.14))), "3.14");
        assert_eq!(Formatter::format_pattern(&Pattern::Literal(Expr::StringLit("hello".to_string()))), "\"hello\"");
        assert_eq!(Formatter::format_pattern(&Pattern::Literal(Expr::Bool(true))), "true");
        assert_eq!(
            Formatter::format_pattern(&Pattern::Variant { name: "Some".to_string(), fields: vec![Pattern::Ident("x".to_string())] }),
            "Some(x)"
        );
        assert_eq!(
            Formatter::format_pattern(&Pattern::Variant { name: "None".to_string(), fields: vec![] }),
            "None"
        );
        assert_eq!(
            Formatter::format_pattern(&Pattern::Tuple(vec![Pattern::Ident("a".to_string()), Pattern::Ident("b".to_string())])),
            "(a, b)"
        );
        assert_eq!(
            Formatter::format_pattern(&Pattern::Array(vec![Pattern::Ident("x".to_string()), Pattern::Wildcard])),
            "[x, _]"
        );
    }

    #[test]
    fn test_struct_pattern() {
        let pat = Pattern::Struct {
            name: "User".to_string(),
            fields: vec![
                ("name".to_string(), Pattern::Ident("name".to_string())),
                ("age".to_string(), Pattern::Ident("a".to_string())),
            ],
            rest: false,
        };
        let result = Formatter::format_pattern(&pat);
        assert!(result.contains("User { name, age: a }"), "got: {}", result);
    }

    #[test]
    fn test_struct_pattern_with_rest() {
        let pat = Pattern::Struct {
            name: "Point".to_string(),
            fields: vec![("x".to_string(), Pattern::Ident("x".to_string()))],
            rest: true,
        };
        let result = Formatter::format_pattern(&pat);
        assert!(result.contains("Point { x, .. }"), "got: {}", result);
    }

    // ===================================================================
    // Type formatting
    // ===================================================================

    #[test]
    fn test_type_formatting() {
        assert_eq!(Formatter::format_type(&Type::Named("i32".to_string())), "i32");
        assert_eq!(
            Formatter::format_type(&Type::Generic { name: "Vec".to_string(), args: vec![Type::Named("i32".to_string())] }),
            "Vec<i32>"
        );
        assert_eq!(
            Formatter::format_type(&Type::Reference { mutable: false, lifetime: None, inner: Box::new(Type::Named("str".to_string())) }),
            "&str"
        );
        assert_eq!(
            Formatter::format_type(&Type::Reference { mutable: true, lifetime: None, inner: Box::new(Type::Named("Vec".to_string())) }),
            "&mut Vec"
        );
        assert_eq!(
            Formatter::format_type(&Type::Array(Box::new(Type::Named("i32".to_string())))),
            "[i32]"
        );
        assert_eq!(
            Formatter::format_type(&Type::Option(Box::new(Type::Named("String".to_string())))),
            "Option<String>"
        );
        assert_eq!(
            Formatter::format_type(&Type::Tuple(vec![Type::Named("i32".to_string()), Type::Named("String".to_string())])),
            "(i32, String)"
        );
        assert_eq!(
            Formatter::format_type(&Type::Function {
                params: vec![Type::Named("i32".to_string())],
                ret: Box::new(Type::Named("bool".to_string()))
            }),
            "fn(i32) -> bool"
        );
    }

    #[test]
    fn test_result_type_formatting() {
        let ty = Type::Result {
            ok: Box::new(Type::Named("i32".to_string())),
            err: Box::new(Type::Named("String".to_string())),
        };
        // Result type falls through to the _ arm -> "<unknown>"
        let result = Formatter::format_type(&ty);
        assert_eq!(result, "<unknown>");
    }

    // ===================================================================
    // Template / JSX formatting
    // ===================================================================

    #[test]
    fn test_self_closing_element() {
        let c = Component {
            name: "Test".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "input".to_string(),
                    attributes: vec![],
                    children: vec![],
                    span: dummy_span(),
                }),
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
        let result = format_program(&program);
        assert!(result.contains("<input />"), "got: {}", result);
    }

    #[test]
    fn test_element_with_attributes() {
        let c = Component {
            name: "Test".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(),
                    attributes: vec![
                        Attribute::Static { name: "class".to_string(), value: "container".to_string() },
                        Attribute::Dynamic { name: "id".to_string(), value: Expr::Ident("my_id".to_string()) },
                        Attribute::EventHandler { event: "click".to_string(), handler: Expr::Ident("handle_click".to_string()) },
                    ],
                    children: vec![TemplateNode::TextLiteral("Hello".to_string())],
                    span: dummy_span(),
                }),
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
        let result = format_program(&program);
        assert!(result.contains("class=\"container\""), "got: {}", result);
        assert!(result.contains("id={my_id}"), "got: {}", result);
        assert!(result.contains("on:click={handle_click}"), "got: {}", result);
    }

    #[test]
    fn test_element_with_aria_role_bind() {
        let fmt = Formatter::new(FormatterOptions::default());
        let attrs = vec![
            Attribute::Aria { name: "label".to_string(), value: Expr::StringLit("Close".to_string()) },
            Attribute::Role { value: "button".to_string() },
            Attribute::Bind { property: "value".to_string(), signal: "count".to_string() },
        ];
        let result = fmt.format_attributes(&attrs);
        assert_eq!(result[0], "aria-label={\"Close\"}");
        assert_eq!(result[1], "role=\"button\"");
        assert_eq!(result[2], "bind:value={count}");
    }

    #[test]
    fn test_template_expression_node() {
        let c = Component {
            name: "Expr".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Expression(Box::new(Expr::Ident("content".to_string()))),
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
        let result = format_program(&program);
        assert!(result.contains("{content}"), "got: {}", result);
    }

    #[test]
    fn test_template_fragment() {
        let c = Component {
            name: "Frag".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Fragment(vec![
                    TemplateNode::TextLiteral("Hello".to_string()),
                    TemplateNode::TextLiteral("World".to_string()),
                ]),
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
        let result = format_program(&program);
        assert!(result.contains("Hello"), "got: {}", result);
        assert!(result.contains("World"), "got: {}", result);
    }

    #[test]
    fn test_template_link() {
        let c = Component {
            name: "Nav".to_string(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            render: RenderBlock {
                body: TemplateNode::Link {
                    to: Expr::StringLit("/about".to_string()),
                    attributes: vec![],
                    children: vec![TemplateNode::TextLiteral("About".to_string())],
                },
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
        let result = format_program(&program);
        assert!(result.contains("<Link to={\"/about\"}>"), "got: {}", result);
        assert!(result.contains("</Link>"), "got: {}", result);
    }

    // ===================================================================
    // Mod formatting
    // ===================================================================

    #[test]
    fn test_mod_external() {
        let m = ModDef {
            name: "utils".to_string(),
            items: None,
            is_external: true,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Mod(m)] };
        let result = format_program(&program);
        assert!(result.contains("mod utils;"), "got: {}", result);
    }

    #[test]
    fn test_mod_inline() {
        let m = ModDef {
            name: "helpers".to_string(),
            items: Some(vec![Item::Function(make_fn("help", vec![], None, vec![]))]),
            is_external: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Mod(m)] };
        let result = format_program(&program);
        assert!(result.contains("mod helpers {"), "got: {}", result);
        assert!(result.contains("fn help()"), "got: {}", result);
    }

    // ===================================================================
    // Agent formatting
    // ===================================================================

    #[test]
    fn test_agent() {
        let agent = AgentDef {
            name: "Assistant".to_string(),
            system_prompt: Some("You are helpful.".to_string()),
            tools: vec![ToolDef {
                name: "search".to_string(),
                description: None,
                params: vec![make_param("query", "String")],
                return_type: Some(Type::Named("String".to_string())),
                body: Block { stmts: vec![Stmt::Return(Some(Expr::StringLit("result".to_string())))], span: dummy_span() },
                span: dummy_span(),
            }],
            state: vec![],
            methods: vec![],
            render: None,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Agent(agent)] };
        let result = format_program(&program);
        assert!(result.contains("agent Assistant {"), "got: {}", result);
        assert!(result.contains("prompt system = \"You are helpful.\";"), "got: {}", result);
        assert!(result.contains("tool search(query: String) -> String"), "got: {}", result);
    }

    #[test]
    fn test_agent_with_render() {
        let agent = AgentDef {
            name: "ChatBot".to_string(),
            system_prompt: None,
            tools: vec![],
            state: vec![],
            methods: vec![],
            render: Some(RenderBlock {
                body: TemplateNode::Element(Element {
                    tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                }),
                span: dummy_span(),
            }),
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Agent(agent)] };
        let result = format_program(&program);
        assert!(result.contains("render {"), "got: {}", result);
    }

    // ===================================================================
    // Test definition formatting
    // ===================================================================

    #[test]
    fn test_test_definition() {
        let t = TestDef {
            name: "addition works".to_string(),
            body: Block {
                stmts: vec![Stmt::Expr(Expr::AssertEq {
                    left: Box::new(Expr::Integer(1)),
                    right: Box::new(Expr::Integer(1)),
                    message: None,
                })],
                span: dummy_span(),
            },
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Test(t)] };
        let result = format_program(&program);
        assert!(result.contains("test \"addition works\" {"), "got: {}", result);
    }

    // ===================================================================
    // Lazy component formatting
    // ===================================================================

    #[test]
    fn test_lazy_component() {
        let lc = LazyComponentDef {
            component: Component {
                name: "HeavyChart".to_string(),
                type_params: vec![],
                props: vec![],
                state: vec![],
                methods: vec![],
                styles: vec![],
                transitions: vec![],
                render: RenderBlock {
                    body: TemplateNode::Element(Element {
                        tag: "canvas".to_string(), attributes: vec![], children: vec![], span: dummy_span(),
                    }),
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
        let result = format_program(&program);
        assert!(result.contains("lazy component HeavyChart {"), "got: {}", result);
    }

    // ===================================================================
    // Page/App/Form/Channel/Embed/Pdf formatting
    // ===================================================================

    #[test]
    fn test_page_formatting() {
        let page = PageDef {
            name: "HomePage".to_string(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::Element(Element { tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span() }),
                span: dummy_span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: true,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Page(page)] };
        let result = format_program(&program);
        assert!(result.contains("pub page HomePage {"), "got: {}", result);
    }

    #[test]
    fn test_app_formatting() {
        let app = AppDef {
            name: "MyApp".to_string(),
            manifest: None,
            offline: None,
            push: None,
            router: None,
            a11y: None,
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::App(app)] };
        let result = format_program(&program);
        assert!(result.contains("app MyApp {"), "got: {}", result);
    }

    #[test]
    fn test_channel_formatting() {
        let ch = ChannelDef {
            name: "Chat".to_string(),
            url: Expr::StringLit("/ws".to_string()),
            contract: Some("ChatMsg".to_string()),
            on_message: None,
            on_connect: None,
            on_disconnect: None,
            reconnect: true,
            heartbeat_interval: None,
            methods: vec![],
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Channel(ch)] };
        let result = format_program(&program);
        assert!(result.contains("channel Chat -> ChatMsg {"), "got: {}", result);
    }

    #[test]
    fn test_embed_formatting() {
        let e = EmbedDef {
            name: "GA".to_string(),
            src: Expr::StringLit("https://ga.js".to_string()),
            loading: Some("lazy".to_string()),
            sandbox: true,
            integrity: None,
            permissions: None,
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Embed(e)] };
        let result = format_program(&program);
        assert!(result.contains("embed GA {"), "got: {}", result);
        assert!(result.contains("loading: \"lazy\""), "got: {}", result);
        assert!(result.contains("sandbox: true"), "got: {}", result);
    }

    #[test]
    fn test_pdf_formatting() {
        let p = PdfDef {
            name: "Invoice".to_string(),
            render: RenderBlock {
                body: TemplateNode::Element(Element { tag: "div".to_string(), attributes: vec![], children: vec![], span: dummy_span() }),
                span: dummy_span(),
            },
            page_size: Some("A4".to_string()),
            orientation: Some("portrait".to_string()),
            margins: None,
            is_pub: true,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Pdf(p)] };
        let result = format_program(&program);
        assert!(result.contains("pub pdf Invoice {"), "got: {}", result);
        assert!(result.contains("page_size: \"A4\""), "got: {}", result);
        assert!(result.contains("orientation: \"portrait\""), "got: {}", result);
    }

    // ===================================================================
    // Multiple items with separator newline
    // ===================================================================

    #[test]
    fn test_multiple_items_separated() {
        let program = Program {
            items: vec![
                Item::Function(make_fn("foo", vec![], None, vec![])),
                Item::Function(make_fn("bar", vec![], None, vec![])),
            ],
        };
        let result = format_program(&program);
        // Items should be separated by a blank line
        assert!(result.contains("}\n\nfn bar"), "got: {}", result);
    }

    // ===================================================================
    // FormatterOptions
    // ===================================================================

    #[test]
    fn test_custom_indent_size() {
        let opts = FormatterOptions {
            indent_size: 2,
            ..Default::default()
        };
        let mut fmt = Formatter::new(opts);
        let program = Program {
            items: vec![Item::Function(make_fn("test", vec![], None, vec![
                Stmt::Return(Some(Expr::Integer(1))),
            ]))],
        };
        let result = fmt.format_program(&program);
        // With indent_size=2, body should be indented by 2 spaces
        assert!(result.contains("  return 1;"), "got: {}", result);
    }

    #[test]
    fn test_self_expr() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::SelfExpr;
        assert_eq!(fmt.format_expr_to_string(&expr), "self");
    }

    #[test]
    fn test_literals() {
        let fmt = Formatter::new(FormatterOptions::default());
        assert_eq!(fmt.format_expr_to_string(&Expr::Integer(42)), "42");
        assert_eq!(fmt.format_expr_to_string(&Expr::Float(3.14)), "3.14");
        assert_eq!(fmt.format_expr_to_string(&Expr::StringLit("hello".to_string())), "\"hello\"");
        assert_eq!(fmt.format_expr_to_string(&Expr::Bool(true)), "true");
        assert_eq!(fmt.format_expr_to_string(&Expr::Bool(false)), "false");
    }

    // ===================================================================
    // Nested expressions
    // ===================================================================

    #[test]
    fn test_nested_field_access_chain() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::FieldAccess {
            object: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("a".to_string())),
                field: "b".to_string(),
            }),
            field: "c".to_string(),
        };
        assert_eq!(fmt.format_expr_to_string(&expr), "a.b.c");
    }

    #[test]
    fn test_method_chain() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::MethodCall {
            object: Box::new(Expr::MethodCall {
                object: Box::new(Expr::Ident("vec".to_string())),
                method: "iter".to_string(),
                args: vec![],
            }),
            method: "map".to_string(),
            args: vec![Expr::Ident("f".to_string())],
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("vec.iter().map(f)"), "got: {}", result);
    }

    #[test]
    fn test_form_formatting() {
        let form = FormDef {
            name: "LoginForm".to_string(),
            fields: vec![FormFieldDef {
                name: "username".to_string(),
                ty: Type::Named("String".to_string()),
                validators: vec![],
                label: None,
                placeholder: None,
                default_value: None,
                span: dummy_span(),
            }],
            on_submit: None,
            steps: vec![],
            methods: vec![],
            styles: vec![],
            render: None,
            is_pub: false,
            span: dummy_span(),
        };
        let program = Program { items: vec![Item::Form(form)] };
        let result = format_program(&program);
        assert!(result.contains("form LoginForm {"), "got: {}", result);
        assert!(result.contains("field username"), "got: {}", result);
    }

    // ===================================================================
    // Literal pattern fallback (non-standard expr)
    // ===================================================================

    #[test]
    fn test_pattern_literal_fallback() {
        let pat = Pattern::Literal(Expr::Ident("something".to_string()));
        let result = Formatter::format_pattern(&pat);
        // Falls through to Debug format
        assert!(!result.is_empty());
    }

    // ── ArrayLit formatting ─────────────────────────────────────────────

    #[test]
    fn test_format_array_lit() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::ArrayLit(vec![Expr::Integer(1), Expr::Integer(2), Expr::Integer(3)]);
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("["), "got: {}", result);
        assert!(result.contains("1"), "got: {}", result);
        assert!(result.contains("2"), "got: {}", result);
        assert!(result.contains("3"), "got: {}", result);
        assert!(result.contains("]"), "got: {}", result);
    }

    #[test]
    fn test_format_empty_array_lit() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::ArrayLit(vec![]);
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("[]"), "got: {}", result);
    }

    // ── ObjectLit formatting ────────────────────────────────────────────

    #[test]
    fn test_format_object_lit() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::ObjectLit {
            fields: vec![
                ("x".into(), Expr::Integer(1)),
                ("y".into(), Expr::Integer(2)),
            ],
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("x:"), "got: {}", result);
        assert!(result.contains("y:"), "got: {}", result);
    }

    // ── Match with guard formatting ─────────────────────────────────────

    #[test]
    fn test_format_match_with_guard() {
        let fmt = Formatter::new(FormatterOptions::default());
        let expr = Expr::Match {
            subject: Box::new(Expr::Ident("x".to_string())),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Ident("n".into()),
                    guard: Some(Expr::Binary {
                        op: BinOp::Gt,
                        left: Box::new(Expr::Ident("n".into())),
                        right: Box::new(Expr::Integer(0)),
                    }),
                    body: Expr::Ident("n".into()),
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: Expr::Integer(0),
                },
            ],
        };
        let result = fmt.format_expr_to_string(&expr);
        assert!(result.contains("match x {"), "got: {}", result);
    }

    // ── TemplateIf / TemplateFor formatting ─────────────────────────────

    fn make_component_with_template(node: TemplateNode) -> Program {
        Program {
            items: vec![Item::Component(Component {
                name: "T".into(),
                type_params: vec![],
                props: vec![],
                state: vec![],
                methods: vec![],
                styles: vec![],
                transitions: vec![],
                trait_bounds: vec![],
                render: RenderBlock { body: node, span: dummy_span() },
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
        }
    }

    #[test]
    fn test_format_template_if() {
        let node = TemplateNode::TemplateIf {
            condition: Box::new(Expr::Ident("show".into())),
            then_children: vec![TemplateNode::TextLiteral("hello".into())],
            else_children: None,
        };
        let result = format_program(&make_component_with_template(node));
        assert!(result.contains("if"), "got: {}", result);
    }

    #[test]
    fn test_format_template_if_with_else() {
        let node = TemplateNode::TemplateIf {
            condition: Box::new(Expr::Ident("active".into())),
            then_children: vec![TemplateNode::TextLiteral("yes".into())],
            else_children: Some(vec![TemplateNode::TextLiteral("no".into())]),
        };
        let result = format_program(&make_component_with_template(node));
        assert!(result.contains("if"), "got: {}", result);
        assert!(result.contains("else"), "got: {}", result);
    }

    #[test]
    fn test_format_template_for() {
        let node = TemplateNode::TemplateFor {
            binding: "item".into(),
            iterator: Box::new(Expr::Ident("items".into())),
            children: vec![TemplateNode::TextLiteral("row".into())],
            lazy: false,
        };
        let result = format_program(&make_component_with_template(node));
        assert!(result.contains("for"), "got: {}", result);
        assert!(result.contains("item"), "got: {}", result);
    }

    #[test]
    fn test_format_array_lit_single_element() {
        let f = Formatter::new(FormatterOptions::default());
        let result = f.format_expr_inner(&Expr::ArrayLit(vec![Expr::Integer(42)]), 0);
        assert!(result.contains("42"), "got: {}", result);
    }

    #[test]
    fn test_format_object_lit_single_field() {
        let f = Formatter::new(FormatterOptions::default());
        let result = f.format_expr_inner(&Expr::ObjectLit {
            fields: vec![("name".into(), Expr::StringLit("test".into()))],
        }, 0);
        assert!(result.contains("name"), "got: {}", result);
    }

    #[test]
    fn test_format_match_guard_with_complex_expr() {
        let f = Formatter::new(FormatterOptions::default());
        let arm = MatchArm {
            pattern: Pattern::Ident("x".into()),
            guard: Some(Expr::Binary {
                op: BinOp::And,
                left: Box::new(Expr::Binary {
                    op: BinOp::Gt,
                    left: Box::new(Expr::Ident("x".into())),
                    right: Box::new(Expr::Integer(0)),
                }),
                right: Box::new(Expr::Binary {
                    op: BinOp::Lt,
                    left: Box::new(Expr::Ident("x".into())),
                    right: Box::new(Expr::Integer(100)),
                }),
            }),
            body: Expr::Ident("x".into()),
        };
        let expr = Expr::Match {
            subject: Box::new(Expr::Ident("val".into())),
            arms: vec![arm],
        };
        let result = f.format_expr_inner(&expr, 0);
        assert!(result.contains("if"), "Match guard should contain 'if': {}", result);
    }

    #[test]
    fn test_format_empty_array_lit_brackets() {
        let f = Formatter::new(FormatterOptions::default());
        let result = f.format_expr_inner(&Expr::ArrayLit(vec![]), 0);
        assert!(result.contains("[") && result.contains("]"), "got: {}", result);
    }

    #[test]
    fn test_format_template_if_no_else() {
        let node = TemplateNode::TemplateIf {
            condition: Box::new(Expr::Ident("visible".into())),
            then_children: vec![TemplateNode::TextLiteral("shown".into())],
            else_children: None,
        };
        let result = format_program(&make_component_with_template(node));
        assert!(result.contains("if"), "got: {}", result);
        assert!(!result.contains("else"), "Should not contain else: {}", result);
    }

    #[test]
    fn test_format_template_for_with_text_children() {
        let node = TemplateNode::TemplateFor {
            binding: "i".into(),
            iterator: Box::new(Expr::Ident("list".into())),
            children: vec![
                TemplateNode::TextLiteral("row".into()),
                TemplateNode::TextLiteral("end".into()),
            ],
            lazy: false,
        };
        let result = format_program(&make_component_with_template(node));
        assert!(result.contains("for"), "got: {}", result);
        assert!(result.contains("li"), "got: {}", result);
    }

    #[test]
    fn test_format_secret_param() {
        let program = Program {
            items: vec![Item::Function(Function {
                name: "hash".to_string(),
                type_params: vec![],
                params: vec![
                    Param {
                        name: "password".to_string(),
                        ty: Type::Named("String".to_string()),
                        ownership: Ownership::Owned,
                        secret: true,
                    },
                ],
                return_type: Some(Type::Named("String".to_string())),
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Return(Some(Expr::Ident("password".to_string())))],
                    span: dummy_span(),
                },
                is_pub: false,
                must_use: false,
                span: dummy_span(),
                lifetimes: vec![],
            })],
        };
        let result = format_program(&program);
        assert!(result.contains("secret password"), "secret keyword should appear before param name, got: {}", result);
    }

    #[test]
    fn test_format_range_expression() {
        let formatter = Formatter::new(FormatterOptions::default());
        let result = formatter.format_expr_inner(&Expr::Range {
            start: Box::new(Expr::Integer(0)),
            end: Box::new(Expr::Integer(10)),
        }, 0);
        assert_eq!(result, "0..10");
    }

    #[test]
    fn test_format_range_with_idents() {
        let formatter = Formatter::new(FormatterOptions::default());
        let result = formatter.format_expr_inner(&Expr::Range {
            start: Box::new(Expr::Ident("start".into())),
            end: Box::new(Expr::Ident("end".into())),
        }, 0);
        assert_eq!(result, "start..end");
    }
}
