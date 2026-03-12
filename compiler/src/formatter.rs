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
                    result.push_str(&format!(
                        "{}{}{} => {},\n",
                        next_indent, pat_str, " ".repeat(padding), body_str
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
            TemplateNode::Link { to, children } => {
                self.push_indent();
                self.push("<Link to={");
                self.push(&self.format_expr_to_string(to));
                self.push("}>");
                self.newline();
                self.inc();
                for child in children {
                    self.format_template_node(child);
                }
                self.dec();
                self.push_line("</Link>");
            }
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

    #[test]
    fn test_simple_function_formats_correctly() {
        let program = Program {
            items: vec![Item::Function(Function {
                name: "add".to_string(),
                type_params: vec![],
                params: vec![
                    Param {
                        name: "a".to_string(),
                        ty: Type::Named("i32".to_string()),
                        ownership: Ownership::Owned,
                    },
                    Param {
                        name: "b".to_string(),
                        ty: Type::Named("i32".to_string()),
                        ownership: Ownership::Owned,
                    },
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
        let mut fmt = Formatter::new(opts);

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
}
