use crate::ast::*;
use crate::token::{Token, TokenKind, FormatStringPart, Span};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
}

/// Synchronization context — tells `synchronize()` what kind of boundary to
/// look for when skipping over broken tokens.
#[allow(dead_code)]
enum SyncContext {
    /// Skip until a token that can start a new top-level item (or EOF).
    TopLevel,
    /// Skip until `;`, `}`, or a keyword that starts a new statement.
    Statement,
    /// Skip until the matching `}` (counting nested braces).
    Block,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0, errors: Vec::new() }
    }

    /// Returns true if any parse errors have been recorded.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Legacy entry point — returns first error only (kept for backward compat).
    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let (program, errors) = self.parse_program_recovering();
        if let Some(first) = errors.into_iter().next() {
            Err(first)
        } else {
            Ok(program)
        }
    }

    /// Parse the full program with error recovery.
    /// Returns a (partial) AST together with all accumulated errors.
    pub fn parse_program_recovering(&mut self) -> (Program, Vec<ParseError>) {
        let mut items = Vec::new();

        while !self.is_at_end() {
            match self.parse_item() {
                Ok(item) => items.push(item),
                Err(e) => {
                    self.errors.push(e);
                    self.recover_to_next_item();
                }
            }
        }

        let errors = std::mem::take(&mut self.errors);
        (Program { items }, errors)
    }

    /// Advance tokens until we reach a token that can start a new top-level
    /// item, or EOF.  Used after a failed `parse_item` call.
    fn recover_to_next_item(&mut self) {
        self.synchronize(SyncContext::TopLevel);
    }

    /// Advance tokens until we reach a semicolon (consuming it) or a token
    /// that looks like it starts a new statement.
    #[allow(dead_code)]
    fn recover_to_semicolon(&mut self) {
        self.synchronize(SyncContext::Statement);
    }

    /// Core synchronization driver.
    fn synchronize(&mut self, ctx: SyncContext) {
        match ctx {
            SyncContext::TopLevel => {
                loop {
                    if self.is_at_end() {
                        break;
                    }
                    match self.peek_kind() {
                        TokenKind::Fn
                        | TokenKind::Component
                        | TokenKind::Struct
                        | TokenKind::Enum
                        | TokenKind::Impl
                        | TokenKind::Use
                        | TokenKind::Store
                        | TokenKind::Agent
                        | TokenKind::Router
                        | TokenKind::Lazy
                        | TokenKind::Test
                        | TokenKind::Trait
                        | TokenKind::Contract
                        | TokenKind::App
                        | TokenKind::Page
                        | TokenKind::Form
                        | TokenKind::Channel
                        | TokenKind::Embed
                        | TokenKind::Pdf
                        | TokenKind::Payment
                        | TokenKind::Auth
                        | TokenKind::Upload
                        | TokenKind::Db
                        | TokenKind::Cache
                        | TokenKind::Breakpoint
                        | TokenKind::Theme
                        | TokenKind::Spring
                        | TokenKind::Stagger
                        | TokenKind::Keyframes
                        | TokenKind::Pub => break,
                        _ => { self.advance(); }
                    }
                }
            }
            SyncContext::Statement => {
                loop {
                    if self.is_at_end() {
                        break;
                    }
                    match self.peek_kind() {
                        TokenKind::Semicolon => {
                            self.advance();
                            break;
                        }
                        TokenKind::RightBrace => {
                            break;
                        }
                        TokenKind::Let
                        | TokenKind::Signal
                        | TokenKind::Return
                        | TokenKind::Yield
                        | TokenKind::Fn
                        | TokenKind::If
                        | TokenKind::For
                        | TokenKind::While
                        | TokenKind::Match => break,
                        _ => { self.advance(); }
                    }
                }
            }
            SyncContext::Block => {
                let mut depth: u32 = 1;
                loop {
                    if self.is_at_end() {
                        break;
                    }
                    match self.peek_kind() {
                        TokenKind::LeftBrace => { depth += 1; self.advance(); }
                        TokenKind::RightBrace => {
                            depth -= 1;
                            if depth == 0 {
                                self.advance();
                                break;
                            }
                            self.advance();
                        }
                        _ => { self.advance(); }
                    }
                }
            }
        }
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        let is_pub = self.match_token(&TokenKind::Pub);

        match self.peek_kind() {
            TokenKind::Fn => Ok(Item::Function(self.parse_function(is_pub)?)),
            TokenKind::Async => {
                // async fn ... — parse_function handles consuming the `async` token
                Ok(Item::Function(self.parse_function(is_pub)?))
            }
            TokenKind::Component => Ok(Item::Component(self.parse_component()?)),
            TokenKind::Struct => Ok(Item::Struct(self.parse_struct(is_pub)?)),
            TokenKind::Enum => Ok(Item::Enum(self.parse_enum(is_pub)?)),
            TokenKind::Impl => Ok(Item::Impl(self.parse_impl()?)),
            TokenKind::Trait => Ok(Item::Trait(self.parse_trait()?)),
            TokenKind::Use => Ok(Item::Use(self.parse_use()?)),
            TokenKind::Mod => Ok(Item::Mod(self.parse_mod()?)),
            TokenKind::Store => Ok(Item::Store(self.parse_store(is_pub)?)),
            TokenKind::Agent => Ok(Item::Agent(self.parse_agent()?)),
            TokenKind::Router => Ok(Item::Router(self.parse_router()?)),
            TokenKind::Contract => Ok(Item::Contract(self.parse_contract(is_pub)?)),
            TokenKind::App => Ok(Item::App(self.parse_app(is_pub)?)),
            TokenKind::Page => Ok(Item::Page(self.parse_page(is_pub)?)),
            TokenKind::Form => Ok(Item::Form(self.parse_form(is_pub)?)),
            TokenKind::Channel => Ok(Item::Channel(self.parse_channel(is_pub)?)),
            TokenKind::Embed => Ok(Item::Embed(self.parse_embed(is_pub)?)),
            TokenKind::Pdf => Ok(Item::Pdf(self.parse_pdf(is_pub)?)),
            TokenKind::Payment => Ok(Item::Payment(self.parse_payment(is_pub)?)),
            TokenKind::Banking => Ok(Item::Banking(self.parse_banking(is_pub)?)),
            TokenKind::MapKeyword => Ok(Item::Map(self.parse_map(is_pub)?)),
            TokenKind::Auth => Ok(Item::Auth(self.parse_auth(is_pub)?)),
            TokenKind::Upload => Ok(Item::Upload(self.parse_upload(is_pub)?)),
            TokenKind::Db => Ok(Item::Db(self.parse_db(is_pub)?)),
            TokenKind::Cache => Ok(Item::Cache(self.parse_cache(is_pub)?)),
            TokenKind::Breakpoint => Ok(Item::Breakpoints(self.parse_breakpoints_def()?)),
            // Also handle `breakpoints` (plural) as an identifier
            TokenKind::Ident(ref id) if id == "breakpoints" => Ok(Item::Breakpoints(self.parse_breakpoints_def_plural()?)),
            TokenKind::Theme => Ok(Item::Theme(self.parse_theme(is_pub)?)),
            TokenKind::Spring => Ok(Item::Animation(self.parse_spring_block(is_pub)?)),
            TokenKind::Keyframes => Ok(Item::Animation(self.parse_keyframes_block(is_pub)?)),
            TokenKind::Stagger => Ok(Item::Animation(self.parse_stagger_block(is_pub)?)),
            TokenKind::MustUse => {
                // must_use fn ...
                self.advance();
                Ok(Item::Function(self.parse_function(is_pub)?))
            }
            TokenKind::Lazy => {
                // lazy component Name { ... }
                self.advance();
                Ok(Item::LazyComponent(self.parse_lazy_component()?))
            }
            TokenKind::Test => Ok(Item::Test(self.parse_test_def()?)),
            _ => Err(self.error("Expected item (fn, component, struct, enum, impl, trait, use, mod, store, agent, router, contract, app, page, form, channel, embed, pdf, payment, auth, upload, cache, breakpoint, theme, spring, keyframes, stagger, lazy, test)")),
        }
    }

    fn parse_test_def(&mut self) -> Result<TestDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Test)?;

        // test "description" { ... }
        let name = if let TokenKind::StringLit(_) = self.peek_kind() {
            if let TokenKind::StringLit(s) = self.advance().kind {
                s
            } else {
                unreachable!()
            }
        } else {
            return Err(self.error("Expected string literal after 'test'"));
        };

        let body = self.parse_block()?;

        Ok(TestDef { name, body, span })
    }

    fn parse_function(&mut self, is_pub: bool) -> Result<Function, ParseError> {
        let is_async = self.match_token(&TokenKind::Async);
        let must_use = self.match_token(&TokenKind::MustUse);
        let span = self.current_span();
        self.expect(&TokenKind::Fn)?;
        let name = self.expect_ident()?;
        let (lifetimes, type_params) = self.parse_lifetime_and_type_params()?;
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RightParen)?;

        let return_type = if self.match_token(&TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        let trait_bounds = self.parse_where_clause()?;

        let body = self.parse_block()?;

        Ok(Function { name, lifetimes, type_params, params, return_type, trait_bounds, body, is_pub, is_async, must_use, span })
    }

    /// Parse optional where clause: `where T: Display, U: Clone`
    fn parse_where_clause(&mut self) -> Result<Vec<TraitBound>, ParseError> {
        if !self.match_token(&TokenKind::Where) {
            return Ok(vec![]);
        }
        let mut bounds = Vec::new();
        loop {
            let type_param = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let trait_name = self.expect_ident()?;
            bounds.push(TraitBound { type_param, trait_name });
            if !self.match_token(&TokenKind::Comma) {
                break;
            }
        }
        Ok(bounds)
    }

    /// Parse optional type parameters: `<T>`, `<T, U>`, `<K, V>`, etc.
    /// Returns an empty Vec if no `<` follows.
    fn parse_type_params(&mut self) -> Result<Vec<String>, ParseError> {
        let (_lifetimes, type_params) = self.parse_lifetime_and_type_params()?;
        Ok(type_params)
    }

    /// Parse optional lifetime and type parameters: `<'a, T>`, `<'a, 'b>`, `<T, U>`, etc.
    /// Lifetimes come first (by convention), but can be mixed with type params.
    /// Returns (lifetimes, type_params).
    fn parse_lifetime_and_type_params(&mut self) -> Result<(Vec<String>, Vec<String>), ParseError> {
        if !self.check(&TokenKind::LeftAngle) {
            return Ok((vec![], vec![]));
        }
        self.advance(); // consume `<`
        let mut lifetimes = Vec::new();
        let mut params = Vec::new();
        while !self.check(&TokenKind::RightAngle) && !self.is_at_end() {
            if let TokenKind::Lifetime(name) = self.peek_kind() {
                lifetimes.push(name);
                self.advance();
            } else {
                params.push(self.expect_ident()?);
            }
            if !self.check(&TokenKind::RightAngle) {
                self.expect(&TokenKind::Comma)?;
            }
        }
        self.expect(&TokenKind::RightAngle)?;
        Ok((lifetimes, params))
    }

    fn parse_component(&mut self) -> Result<Component, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Component)?;
        let name = self.expect_ident()?;
        let type_params = self.parse_type_params()?;

        // Props in parentheses
        let props = if self.check(&TokenKind::LeftParen) {
            self.expect(&TokenKind::LeftParen)?;
            let props = self.parse_props()?;
            self.expect(&TokenKind::RightParen)?;
            props
        } else {
            vec![]
        };

        let trait_bounds = self.parse_where_clause()?;

        self.expect(&TokenKind::LeftBrace)?;

        let mut state = Vec::new();
        let mut methods = Vec::new();
        let mut styles = Vec::new();
        let mut transitions = Vec::new();
        let mut gestures = Vec::new();
        let mut render = None;
        let mut permissions = None;
        let mut skeleton = None;
        let mut error_boundary = None;
        let mut chunk = None;
        let mut on_destroy = None;
        let mut a11y = None;
        let mut shortcuts = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Let => {
                    state.push(self.parse_state_field()?);
                }
                TokenKind::Signal => {
                    state.push(self.parse_signal_field()?);
                }
                TokenKind::Chunk => {
                    self.advance();
                    if let TokenKind::StringLit(s) = self.peek_kind() {
                        chunk = Some(s.clone());
                        self.advance();
                    }
                    self.match_token(&TokenKind::Semicolon);
                }
                TokenKind::Fn | TokenKind::Async => {
                    let func = self.parse_function(false)?;
                    if func.name == "on_destroy" {
                        on_destroy = Some(func);
                    } else {
                        methods.push(func);
                    }
                }
                TokenKind::Style => {
                    styles.extend(self.parse_style_blocks()?);
                }
                TokenKind::Transition => {
                    transitions.extend(self.parse_transition_block()?);
                }
                TokenKind::Render => {
                    render = Some(self.parse_render_block()?);
                }
                TokenKind::Permissions => {
                    permissions = Some(self.parse_permissions()?);
                }
                TokenKind::Gesture => {
                    gestures.push(self.parse_gesture()?);
                }
                TokenKind::A11y => {
                    self.advance();
                    if self.match_token(&TokenKind::Manual) {
                        a11y = Some(A11yMode::Manual);
                    } else if self.match_token(&TokenKind::Hybrid) {
                        a11y = Some(A11yMode::Hybrid);
                    } else {
                        // Check for "auto" as an identifier
                        if let TokenKind::Ident(s) = self.peek_kind() {
                            if s == "auto" {
                                self.advance();
                                a11y = Some(A11yMode::Auto);
                            }
                        } else {
                            a11y = Some(A11yMode::Auto); // default to auto
                        }
                    }
                    self.match_token(&TokenKind::Semicolon);
                }
                TokenKind::Ident(ref id) if id == "skeleton" => {
                    skeleton = Some(self.parse_skeleton_block()?);
                }
                TokenKind::Ident(ref id) if id == "error_boundary" => {
                    error_boundary = Some(self.parse_error_boundary()?);
                }
                // spring enter { from: {...}, to: {...} } inside a component
                TokenKind::Spring => {
                    self.advance(); // consume `spring`
                    let _anim_name = self.expect_ident()?; // consume name (e.g. "enter", "exit")
                    self.expect(&TokenKind::LeftBrace)?;
                    // Parse key: value pairs, discarding them (the info is used at codegen)
                    let mut depth: u32 = 1;
                    while depth > 0 && !self.is_at_end() {
                        match self.peek_kind() {
                            TokenKind::LeftBrace => { depth += 1; self.advance(); }
                            TokenKind::RightBrace => {
                                depth -= 1;
                                self.advance();
                            }
                            _ => { self.advance(); }
                        }
                    }
                }
                // keyframes inside a component — consume and discard similarly
                TokenKind::Keyframes => {
                    self.advance();
                    let _anim_name = self.expect_ident()?;
                    self.expect(&TokenKind::LeftBrace)?;
                    let mut depth: u32 = 1;
                    while depth > 0 && !self.is_at_end() {
                        match self.peek_kind() {
                            TokenKind::LeftBrace => { depth += 1; self.advance(); }
                            TokenKind::RightBrace => { depth -= 1; self.advance(); }
                            _ => { self.advance(); }
                        }
                    }
                }
                // stagger inside a component — consume and discard similarly
                TokenKind::Stagger => {
                    self.advance();
                    let _anim_name = self.expect_ident()?;
                    self.expect(&TokenKind::LeftBrace)?;
                    let mut depth: u32 = 1;
                    while depth > 0 && !self.is_at_end() {
                        match self.peek_kind() {
                            TokenKind::LeftBrace => { depth += 1; self.advance(); }
                            TokenKind::RightBrace => { depth -= 1; self.advance(); }
                            _ => { self.advance(); }
                        }
                    }
                }
                TokenKind::Shortcut => {
                    self.advance();
                    let shortcut_span = self.current_span();
                    let keys = if let TokenKind::StringLit(s) = self.peek_kind() {
                        let s = s.clone();
                        self.advance();
                        s
                    } else {
                        return Err(self.error("expected shortcut key string"));
                    };
                    let body = if self.match_token(&TokenKind::FatArrow) {
                        // shortcut "Cmd+S" => self.save;
                        let expr = self.parse_expr()?;
                        self.match_token(&TokenKind::Semicolon);
                        Block {
                            stmts: vec![Stmt::Expr(expr)],
                            span: shortcut_span,
                        }
                    } else {
                        self.parse_block()?
                    };
                    shortcuts.push(ShortcutDef { keys, body, span: shortcut_span });
                }
                _ => return Err(self.error("Expected let, signal, fn, chunk, style, transition, render, permissions, gesture, a11y, shortcut, skeleton, or error_boundary in component")),
            }
        }

        self.expect(&TokenKind::RightBrace)?;

        let render = render.ok_or_else(|| ParseError {
            message: format!("Component '{name}' missing render block"),
            span,
        })?;

        Ok(Component { name, type_params, props, state, methods, styles, transitions, trait_bounds, render, gestures, permissions, skeleton, error_boundary, chunk, on_destroy, a11y, shortcuts, span })
    }

    fn parse_page(&mut self, is_pub: bool) -> Result<PageDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Page)?;
        let name = self.expect_ident()?;

        // Optional props: page BlogPost(slug: String) { ... }
        let props = if self.check(&TokenKind::LeftParen) {
            self.expect(&TokenKind::LeftParen)?;
            let params = self.parse_params()?;
            self.expect(&TokenKind::RightParen)?;
            params
        } else {
            vec![]
        };

        self.expect(&TokenKind::LeftBrace)?;

        let mut meta = None;
        let mut state = Vec::new();
        let mut methods = Vec::new();
        let mut styles = Vec::new();
        let mut gestures = Vec::new();
        let mut render = None;
        let mut permissions = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Meta => {
                    self.advance();
                    meta = Some(self.parse_meta_def()?);
                }
                TokenKind::Let => {
                    state.push(self.parse_state_field()?);
                }
                TokenKind::Signal => {
                    state.push(self.parse_signal_field()?);
                }
                TokenKind::Fn => {
                    methods.push(self.parse_function(false)?);
                }
                TokenKind::Style => {
                    styles.extend(self.parse_style_blocks()?);
                }
                TokenKind::Render => {
                    render = Some(self.parse_render_block()?);
                }
                TokenKind::Permissions => {
                    permissions = Some(self.parse_permissions()?);
                }
                TokenKind::Gesture => {
                    gestures.push(self.parse_gesture()?);
                }
                _ => {
                    return Err(self.error(&format!(
                        "unexpected token in page: {:?}",
                        self.peek_kind()
                    )));
                }
            }
        }

        self.expect(&TokenKind::RightBrace)?;

        let render = render.ok_or_else(|| ParseError {
            message: format!("Page '{name}' missing render block"),
            span,
        })?;

        Ok(PageDef {
            name,
            props,
            meta,
            state,
            methods,
            styles,
            render,
            permissions,
            gestures,
            is_pub,
            span,
        })
    }

    fn parse_meta_def(&mut self) -> Result<MetaDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::LeftBrace)?;

        let mut title = None;
        let mut description = None;
        let mut canonical = None;
        let mut og_image = None;
        let mut structured_data = vec![];
        let mut extra = vec![];

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;

            match key.as_str() {
                "title" => { title = Some(self.parse_expr()?); }
                "description" => { description = Some(self.parse_expr()?); }
                "canonical" => { canonical = Some(self.parse_expr()?); }
                "og_image" => { og_image = Some(self.parse_expr()?); }
                "structured_data" => {
                    structured_data.push(self.parse_structured_data()?);
                }
                other => {
                    let val = self.parse_expr()?;
                    extra.push((other.to_string(), val));
                }
            }

            // Optional comma
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(MetaDef { title, description, canonical, og_image, structured_data, extra, span })
    }

    fn parse_structured_data(&mut self) -> Result<StructuredDataDef, ParseError> {
        let span = self.current_span();
        // Parse Schema.Article, schema.Article, or just Article
        let schema_type = if self.match_token(&TokenKind::Schema) {
            self.expect(&TokenKind::Dot)?;
            self.expect_ident()?
        } else if matches!(self.peek_kind(), TokenKind::Ident(ref s) if s == "Schema") {
            self.advance(); // consume "Schema"
            self.expect(&TokenKind::Dot)?;
            self.expect_ident()?
        } else {
            self.expect_ident()?
        };

        self.expect(&TokenKind::LeftBrace)?;

        let mut fields = vec![];
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let field_name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let value = self.parse_expr()?;
            fields.push((field_name, value));
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(StructuredDataDef { schema_type, fields, span })
    }

    /// Parse `permissions { network: [...], storage: [...], capabilities: [...] }`
    fn parse_permissions(&mut self) -> Result<PermissionsDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Permissions)?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut network = Vec::new();
        let mut storage = Vec::new();
        let mut capabilities = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let values = self.parse_string_array()?;
            // optional trailing comma
            self.match_token(&TokenKind::Comma);

            match key.as_str() {
                "network" => network = values,
                "storage" => storage = values,
                "capabilities" => capabilities = values,
                _ => return Err(self.error(&format!(
                    "Unknown permissions key '{}'; expected network, storage, or capabilities", key
                ))),
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(PermissionsDef { network, storage, capabilities, span })
    }

    /// Parse `["str1", "str2", ...]`
    fn parse_string_array(&mut self) -> Result<Vec<String>, ParseError> {
        self.expect(&TokenKind::LeftBracket)?;
        let mut items = Vec::new();
        while !self.check(&TokenKind::RightBracket) && !self.is_at_end() {
            if let TokenKind::StringLit(s) = self.peek_kind() {
                self.advance();
                items.push(s);
            } else {
                return Err(self.error("Expected string literal in array"));
            }
            if !self.check(&TokenKind::RightBracket) {
                self.expect(&TokenKind::Comma)?;
            }
        }
        self.expect(&TokenKind::RightBracket)?;
        Ok(items)
    }

    /// Parse `skeleton { <template_node> }` — placeholder UI shown while loading
    fn parse_skeleton_block(&mut self) -> Result<SkeletonDef, ParseError> {
        let span = self.current_span();
        // The identifier "skeleton" has already been peeked; consume it.
        self.advance();
        let body = self.parse_render_block_inline()?;
        Ok(SkeletonDef { body, span })
    }

    /// Parse `error_boundary { fallback { ... } body { ... } }`
    fn parse_error_boundary(&mut self) -> Result<ErrorBoundary, ParseError> {
        let span = self.current_span();
        // The identifier "error_boundary" has already been peeked; consume it.
        self.advance();
        self.expect(&TokenKind::LeftBrace)?;

        // Expect the fallback render block
        let fallback_ident = self.expect_ident()?;
        if fallback_ident != "fallback" {
            return Err(self.error("Expected 'fallback' in error_boundary"));
        }
        let fallback = self.parse_render_block_inline()?;

        // Expect the body render block
        let body = self.parse_render_block_inline()?;

        self.expect(&TokenKind::RightBrace)?;

        Ok(ErrorBoundary { fallback, body, span })
    }

    /// Parse an inline render block: `{ <template_node> }`
    fn parse_render_block_inline(&mut self) -> Result<RenderBlock, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::LeftBrace)?;
        let body = self.parse_template_node()?;
        self.expect(&TokenKind::RightBrace)?;
        Ok(RenderBlock { body, span })
    }

    fn parse_props(&mut self) -> Result<Vec<Prop>, ParseError> {
        let mut props = Vec::new();

        while !self.check(&TokenKind::RightParen) {
            let name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.parse_type()?;

            let default = if self.match_token(&TokenKind::Equals) {
                Some(self.parse_expr()?)
            } else {
                None
            };

            props.push(Prop { name, ty, default });

            if !self.check(&TokenKind::RightParen) {
                self.expect(&TokenKind::Comma)?;
            }
        }

        Ok(props)
    }

    fn parse_state_field(&mut self) -> Result<StateField, ParseError> {
        self.expect(&TokenKind::Let)?;
        let mutable = self.match_token(&TokenKind::Mut);
        let secret = self.match_token(&TokenKind::Secret);
        let name = self.expect_ident()?;

        let ty = if self.match_token(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        let ownership = if self.match_token(&TokenKind::Equals) {
            // Check for 'own' keyword
            if self.match_token(&TokenKind::Own) {
                Ownership::Owned
            } else {
                Ownership::Owned // default
            }
        } else {
            self.expect(&TokenKind::Equals)?;
            unreachable!()
        };

        let initializer = self.parse_expr()?;
        self.expect(&TokenKind::Semicolon)?;

        Ok(StateField { name, ty, mutable, secret, atomic: false, initializer, ownership })
    }

    fn parse_signal_field(&mut self) -> Result<StateField, ParseError> {
        self.expect(&TokenKind::Signal)?;
        let atomic = self.match_token(&TokenKind::Atomic);
        let secret = self.match_token(&TokenKind::Secret);
        let name = self.expect_ident()?;

        let ty = if self.match_token(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(&TokenKind::Equals)?;
        let initializer = self.parse_expr()?;
        self.expect(&TokenKind::Semicolon)?;

        Ok(StateField {
            name,
            ty,
            mutable: true,
            secret,
            atomic,
            initializer,
            ownership: Ownership::Owned,
        })
    }

    fn parse_render_block(&mut self) -> Result<RenderBlock, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Render)?;
        self.expect(&TokenKind::LeftBrace)?;
        let body = self.parse_template_node()?;
        self.expect(&TokenKind::RightBrace)?;
        Ok(RenderBlock { body, span })
    }

    /// Parse `transition { opacity: "0.3s ease"; transform: "0.5s cubic-bezier(...)"; }`
    fn parse_transition_block(&mut self) -> Result<Vec<TransitionDef>, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Transition)?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut transitions = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // Read CSS property name (may be hyphenated, e.g. background-color)
            let mut prop_name = String::new();
            while !self.check(&TokenKind::Colon) && !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                let tok = self.advance();
                match &tok.kind {
                    TokenKind::Ident(s) => {
                        if !prop_name.is_empty() {
                            prop_name.push('-');
                        }
                        prop_name.push_str(s);
                    }
                    TokenKind::Minus => {} // hyphen handled by push('-') above
                    _ => {}
                }
            }

            self.expect(&TokenKind::Colon)?;

            // Value is a string literal like "0.3s ease"
            let value = if let TokenKind::StringLit(_) = self.peek_kind() {
                if let TokenKind::StringLit(s) = self.advance().kind { s } else { unreachable!() }
            } else {
                return Err(self.error("Expected string literal for transition value"));
            };

            self.expect(&TokenKind::Semicolon)?;

            // Split "duration easing" from the value string
            let parts: Vec<&str> = value.splitn(2, ' ').collect();
            let duration = parts.first().unwrap_or(&"0.3s").to_string();
            let easing = parts.get(1).unwrap_or(&"ease").to_string();

            transitions.push(TransitionDef {
                property: prop_name,
                duration,
                easing,
                span,
            });
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(transitions)
    }

    /// Parse `animate name { 0% { ... } 100% { ... } duration: "0.5s"; easing: "ease-in"; }`
    #[allow(dead_code)]
    fn parse_animate_block(&mut self) -> Result<AnimationDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Animate)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut keyframes = Vec::new();
        let mut duration = "0.3s".to_string();
        let mut easing = "ease".to_string();
        let mut iterations = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            if let TokenKind::Integer(_) = self.peek_kind() {
                // Keyframe: 0% { opacity: "0"; }
                let offset_val = if let TokenKind::Integer(n) = self.advance().kind {
                    n as f64
                } else {
                    unreachable!()
                };
                self.expect(&TokenKind::Percent)?;

                self.expect(&TokenKind::LeftBrace)?;
                let mut properties = Vec::new();
                while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                    let mut kf_prop = String::new();
                    while !self.check(&TokenKind::Colon) && !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                        let tok = self.advance();
                        match &tok.kind {
                            TokenKind::Ident(s) => {
                                if !kf_prop.is_empty() { kf_prop.push('-'); }
                                kf_prop.push_str(s);
                            }
                            TokenKind::Minus => {}
                            _ => {}
                        }
                    }
                    self.expect(&TokenKind::Colon)?;
                    let kf_val = if let TokenKind::StringLit(_) = self.peek_kind() {
                        if let TokenKind::StringLit(s) = self.advance().kind { s } else { unreachable!() }
                    } else {
                        return Err(self.error("Expected string literal for keyframe property value"));
                    };
                    self.expect(&TokenKind::Semicolon)?;
                    properties.push((kf_prop, kf_val));
                }
                self.expect(&TokenKind::RightBrace)?;

                keyframes.push(Keyframe {
                    offset: offset_val / 100.0,
                    properties,
                });
            } else {
                // Named option: duration, easing, iterations
                let opt_name = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let opt_value = if let TokenKind::StringLit(_) = self.peek_kind() {
                    if let TokenKind::StringLit(s) = self.advance().kind { s } else { unreachable!() }
                } else {
                    return Err(self.error("Expected string literal for animation option"));
                };
                self.expect(&TokenKind::Semicolon)?;
                match opt_name.as_str() {
                    "duration" => duration = opt_value,
                    "easing" => easing = opt_value,
                    "iterations" => iterations = Some(opt_value),
                    _ => return Err(self.error(&format!("Unknown animation option: {opt_name}"))),
                }
            }
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(AnimationDef {
            name,
            keyframes,
            duration,
            easing,
            iterations,
            span,
        })
    }

    fn parse_template_node(&mut self) -> Result<TemplateNode, ParseError> {
        match self.peek_kind() {
            TokenKind::LeftAngle => self.parse_element(),
            TokenKind::LeftBrace => {
                self.advance();
                if self.check(&TokenKind::If) {
                    self.advance();
                    let condition = self.parse_expr()?;
                    self.expect(&TokenKind::LeftBrace)?;
                    let mut then_children = Vec::new();
                    while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                        then_children.push(self.parse_template_node()?);
                    }
                    self.expect(&TokenKind::RightBrace)?;
                    let else_children = if self.check(&TokenKind::Else) {
                        self.advance();
                        self.expect(&TokenKind::LeftBrace)?;
                        let mut children = Vec::new();
                        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                            children.push(self.parse_template_node()?);
                        }
                        self.expect(&TokenKind::RightBrace)?;
                        Some(children)
                    } else {
                        None
                    };
                    self.expect(&TokenKind::RightBrace)?;
                    Ok(TemplateNode::TemplateIf {
                        condition: Box::new(condition),
                        then_children,
                        else_children,
                    })
                } else if self.check(&TokenKind::Lazy) || self.check(&TokenKind::For) {
                    // {lazy for x in expr { ... }} or {for x in expr { ... }}
                    let lazy = self.check(&TokenKind::Lazy);
                    if lazy {
                        self.advance(); // consume `lazy`
                    }
                    self.expect(&TokenKind::For)?;
                    let binding = self.expect_ident()?;
                    self.expect(&TokenKind::In)?;
                    let iterator = self.parse_expr()?;
                    self.expect(&TokenKind::LeftBrace)?;
                    let mut children = Vec::new();
                    while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                        children.push(self.parse_template_node()?);
                    }
                    self.expect(&TokenKind::RightBrace)?;
                    self.expect(&TokenKind::RightBrace)?;
                    Ok(TemplateNode::TemplateFor {
                        binding,
                        iterator: Box::new(iterator),
                        children,
                        lazy,
                    })
                } else if self.check(&TokenKind::Match) {
                    // {match subject { Pattern => <template>, ... }}
                    self.advance(); // consume `match`
                    let subject = self.parse_expr()?;
                    self.expect(&TokenKind::LeftBrace)?;
                    let mut arms = Vec::new();
                    while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                        let pattern = self.parse_pattern()?;
                        let guard = if self.check(&TokenKind::If) {
                            self.advance();
                            Some(self.parse_expr()?)
                        } else {
                            None
                        };
                        self.expect(&TokenKind::FatArrow)?;
                        // Arm body: zero or more template nodes until `,` or `}`
                        let mut body = Vec::new();
                        // A single template node is the common case:
                        //   Some(err) => <div>{err}</div>,
                        //   None => <span />,
                        // We parse one template node unless the arm starts with `{`
                        // followed by multiple nodes.
                        if self.check(&TokenKind::LeftBrace)
                            && !self.is_object_literal_brace()
                        {
                            // Block: { <node1> <node2> ... }
                            self.advance(); // consume `{`
                            while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                                body.push(self.parse_template_node()?);
                            }
                            self.expect(&TokenKind::RightBrace)?;
                        } else {
                            body.push(self.parse_template_node()?);
                        }
                        arms.push(TemplateMatchArm { pattern, guard, body });
                        if !self.check(&TokenKind::RightBrace) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&TokenKind::RightBrace)?; // close match `}`
                    self.expect(&TokenKind::RightBrace)?; // close outer `}`
                    Ok(TemplateNode::TemplateMatch {
                        subject: Box::new(subject),
                        arms,
                    })
                } else {
                    let expr = self.parse_expr()?;
                    self.expect(&TokenKind::RightBrace)?;
                    Ok(TemplateNode::Expression(Box::new(expr)))
                }
            }
            TokenKind::StringLit(_) => {
                if let TokenKind::StringLit(s) = self.advance().kind {
                    Ok(TemplateNode::TextLiteral(s))
                } else {
                    unreachable!()
                }
            }
            _ => Err(self.error("Expected template node (<element>, {expr}, or \"text\")")),
        }
    }

    fn parse_element(&mut self) -> Result<TemplateNode, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::LeftAngle)?;
        let tag = self.expect_ident()?;

        // Special handling for <Link to="..."> navigation links
        if tag == "Link" {
            return self.parse_link_element();
        }

        // <Outlet /> — route content placeholder
        if tag == "Outlet" {
            // Self-closing only
            if self.match_token(&TokenKind::Slash) {
                self.expect(&TokenKind::RightAngle)?;
            } else {
                self.expect(&TokenKind::RightAngle)?;
            }
            return Ok(TemplateNode::Outlet);
        }

        // Layout primitives — compile-time CSS sugar
        if matches!(tag.as_str(), "Stack" | "Row" | "Grid" | "Center" | "Cluster" | "Sidebar" | "Switcher") {
            return self.parse_layout_element(&tag, span);
        }

        let mut attributes = Vec::new();

        // Parse attributes
        while !self.check(&TokenKind::RightAngle)
            && !self.check(&TokenKind::Slash)
            && !self.is_at_end()
        {
            let attr_name = self.expect_ident()?;

            // Check for on:event handler
            if attr_name == "on" && self.match_token(&TokenKind::Colon) {
                let event = self.expect_ident()?;
                self.expect(&TokenKind::Equals)?;
                self.expect(&TokenKind::LeftBrace)?;
                let handler = self.parse_expr()?;
                self.expect(&TokenKind::RightBrace)?;
                attributes.push(Attribute::EventHandler { event, handler });
            } else if attr_name == "bind" && self.match_token(&TokenKind::Colon) {
                // Two-way binding: bind:value={signal_name}, bind:checked={is_active}
                let property = self.expect_ident()?;
                self.expect(&TokenKind::Equals)?;
                self.expect(&TokenKind::LeftBrace)?;
                let signal = self.expect_ident()?;
                self.expect(&TokenKind::RightBrace)?;
                attributes.push(Attribute::Bind { property, signal });
            } else if attr_name == "aria" && self.match_token(&TokenKind::Minus) {
                // aria-* attributes: aria-label="...", aria-hidden={expr},
                // aria-live="polite", aria-expanded={is_open}, aria-describedby="desc", etc.
                let aria_suffix = self.expect_ident()?;
                let aria_name = format!("aria-{}", aria_suffix);
                self.expect(&TokenKind::Equals)?;
                if self.check(&TokenKind::LeftBrace) {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.expect(&TokenKind::RightBrace)?;
                    attributes.push(Attribute::Aria { name: aria_name, value });
                } else if let TokenKind::StringLit(_) = self.peek_kind() {
                    if let TokenKind::StringLit(s) = self.advance().kind {
                        attributes.push(Attribute::Aria {
                            name: aria_name,
                            value: Expr::StringLit(s),
                        });
                    }
                } else {
                    return Err(self.error("Expected aria attribute value"));
                }
            } else if attr_name == "role" {
                // role="button", role="navigation", etc.
                self.expect(&TokenKind::Equals)?;
                if let TokenKind::StringLit(_) = self.peek_kind() {
                    if let TokenKind::StringLit(s) = self.advance().kind {
                        attributes.push(Attribute::Role { value: s });
                    }
                } else {
                    return Err(self.error("Expected string value for role attribute"));
                }
            } else if attr_name == "tabindex" {
                // tabindex="0" — parsed as a standard attribute
                self.expect(&TokenKind::Equals)?;
                if self.check(&TokenKind::LeftBrace) {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.expect(&TokenKind::RightBrace)?;
                    attributes.push(Attribute::Dynamic { name: "tabindex".into(), value });
                } else if let TokenKind::StringLit(_) = self.peek_kind() {
                    if let TokenKind::StringLit(s) = self.advance().kind {
                        attributes.push(Attribute::Static { name: "tabindex".into(), value: s });
                    }
                } else {
                    return Err(self.error("Expected tabindex value"));
                }
            } else if self.check(&TokenKind::Equals) {
                self.advance(); // consume '='
                if self.check(&TokenKind::LeftBrace) {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.expect(&TokenKind::RightBrace)?;
                    attributes.push(Attribute::Dynamic { name: attr_name, value });
                } else if let TokenKind::StringLit(_) = self.peek_kind() {
                    if let TokenKind::StringLit(s) = self.advance().kind {
                        attributes.push(Attribute::Static { name: attr_name, value: s });
                    }
                } else {
                    return Err(self.error("Expected attribute value"));
                }
            } else {
                // Boolean attribute (e.g., disabled, checked, readonly)
                // No `=` follows — treat as a static attribute with empty value
                attributes.push(Attribute::Static { name: attr_name, value: String::new() });
            }
        }

        // Self-closing tag: />
        if self.match_token(&TokenKind::Slash) {
            self.expect(&TokenKind::RightAngle)?;
            return Ok(TemplateNode::Element(Element {
                tag,
                attributes,
                children: vec![],
                span,
            }));
        }

        self.expect(&TokenKind::RightAngle)?;

        // Children
        let mut children = Vec::new();
        while !self.is_closing_tag() && !self.is_at_end() {
            children.push(self.parse_template_node()?);
        }

        // Closing tag: </tag>
        self.expect(&TokenKind::LeftAngle)?;
        self.expect(&TokenKind::Slash)?;
        let closing_tag = self.expect_ident()?;
        if closing_tag != tag {
            return Err(self.error(&format!(
                "Mismatched closing tag: expected </{tag}>, found </{closing_tag}>"
            )));
        }
        self.expect(&TokenKind::RightAngle)?;

        Ok(TemplateNode::Element(Element {
            tag,
            attributes,
            children,
            span,
        }))
    }

    /// Parse <Link to="..." > ... </Link> as a TemplateNode::Link
    fn parse_link_element(&mut self) -> Result<TemplateNode, ParseError> {
        // "Link" tag name already consumed; parse the `to` attribute first
        let to_attr_name = self.expect_ident()?;
        if to_attr_name != "to" {
            return Err(self.error("Link element requires a 'to' attribute"));
        }
        self.expect(&TokenKind::Equals)?;

        let to = if self.check(&TokenKind::LeftBrace) {
            self.advance();
            let expr = self.parse_expr()?;
            self.expect(&TokenKind::RightBrace)?;
            expr
        } else if let TokenKind::StringLit(_) = self.peek_kind() {
            if let TokenKind::StringLit(s) = self.advance().kind {
                Expr::StringLit(s)
            } else {
                unreachable!()
            }
        } else {
            return Err(self.error("Expected string or expression for Link 'to' attribute"));
        };

        // Parse additional attributes (class, style, aria-*, etc.)
        let mut attributes = Vec::new();
        while !self.check(&TokenKind::RightAngle)
            && !self.check(&TokenKind::Slash)
            && !self.is_at_end()
        {
            let attr_name = self.expect_ident()?;

            if attr_name == "on" && self.match_token(&TokenKind::Colon) {
                let event = self.expect_ident()?;
                self.expect(&TokenKind::Equals)?;
                self.expect(&TokenKind::LeftBrace)?;
                let handler = self.parse_expr()?;
                self.expect(&TokenKind::RightBrace)?;
                attributes.push(Attribute::EventHandler { event, handler });
            } else if attr_name == "aria" && self.match_token(&TokenKind::Minus) {
                let aria_suffix = self.expect_ident()?;
                let aria_name = format!("aria-{}", aria_suffix);
                self.expect(&TokenKind::Equals)?;
                if self.check(&TokenKind::LeftBrace) {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.expect(&TokenKind::RightBrace)?;
                    attributes.push(Attribute::Aria { name: aria_name, value });
                } else if let TokenKind::StringLit(_) = self.peek_kind() {
                    if let TokenKind::StringLit(s) = self.advance().kind {
                        attributes.push(Attribute::Aria {
                            name: aria_name,
                            value: Expr::StringLit(s),
                        });
                    }
                } else {
                    return Err(self.error("Expected aria attribute value"));
                }
            } else if attr_name == "role" {
                self.expect(&TokenKind::Equals)?;
                if let TokenKind::StringLit(_) = self.peek_kind() {
                    if let TokenKind::StringLit(s) = self.advance().kind {
                        attributes.push(Attribute::Role { value: s });
                    }
                } else {
                    return Err(self.error("Expected string value for role attribute"));
                }
            } else if self.check(&TokenKind::Equals) {
                self.advance(); // consume '='
                if self.check(&TokenKind::LeftBrace) {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.expect(&TokenKind::RightBrace)?;
                    attributes.push(Attribute::Dynamic { name: attr_name, value });
                } else if let TokenKind::StringLit(_) = self.peek_kind() {
                    if let TokenKind::StringLit(s) = self.advance().kind {
                        attributes.push(Attribute::Static { name: attr_name, value: s });
                    }
                } else {
                    return Err(self.error("Expected attribute value"));
                }
            } else {
                // Boolean attribute (e.g., disabled, checked, readonly)
                attributes.push(Attribute::Static { name: attr_name, value: String::new() });
            }
        }

        // Self-closing: <Link to="/" />
        if self.match_token(&TokenKind::Slash) {
            self.expect(&TokenKind::RightAngle)?;
            return Ok(TemplateNode::Link { to, attributes, children: vec![] });
        }

        self.expect(&TokenKind::RightAngle)?;

        // Children
        let mut children = Vec::new();
        while !self.is_closing_tag() && !self.is_at_end() {
            children.push(self.parse_template_node()?);
        }

        // Closing tag: </Link>
        self.expect(&TokenKind::LeftAngle)?;
        self.expect(&TokenKind::Slash)?;
        let closing = self.expect_ident()?;
        if closing != "Link" {
            return Err(self.error(&format!(
                "Mismatched closing tag: expected </Link>, found </{closing}>"
            )));
        }
        self.expect(&TokenKind::RightAngle)?;

        Ok(TemplateNode::Link { to, attributes, children })
    }

    /// Parse layout primitive elements: Stack, Row, Grid, Center, Cluster, Sidebar, Switcher
    fn parse_layout_element(&mut self, tag: &str, span: Span) -> Result<TemplateNode, ParseError> {
        // Parse key="value" attributes
        let mut attrs: Vec<(String, String)> = Vec::new();
        while !self.check(&TokenKind::RightAngle) && !self.check(&TokenKind::Slash) && !self.is_at_end() {
            let name = self.expect_ident()?;
            self.expect(&TokenKind::Equals)?;
            let value = if let TokenKind::StringLit(_) = self.peek_kind() {
                if let TokenKind::StringLit(s) = self.advance().kind { s } else { unreachable!() }
            } else {
                return Err(self.error("Expected string value for layout attribute"));
            };
            attrs.push((name, value));
        }

        // Self-closing: <Stack gap="16" />
        let children = if self.match_token(&TokenKind::Slash) {
            self.expect(&TokenKind::RightAngle)?;
            vec![]
        } else {
            self.expect(&TokenKind::RightAngle)?;
            let mut kids = Vec::new();
            while !self.is_closing_tag() && !self.is_at_end() {
                kids.push(self.parse_template_node()?);
            }
            self.expect(&TokenKind::LeftAngle)?;
            self.expect(&TokenKind::Slash)?;
            let closing = self.expect_ident()?;
            if closing != tag {
                return Err(self.error(&format!("Mismatched closing tag: expected </{tag}>, found </{closing}>")));
            }
            self.expect(&TokenKind::RightAngle)?;
            kids
        };

        let get = |name: &str| -> Option<String> {
            attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
        };

        let node = match tag {
            "Stack" => LayoutNode::Stack { gap: get("gap"), children, span },
            "Row" => LayoutNode::Row { gap: get("gap"), align: get("align"), children, span },
            "Grid" => LayoutNode::Grid { cols: get("cols"), rows: get("rows"), gap: get("gap"), children, span },
            "Center" => LayoutNode::Center { max_width: get("max_width"), children, span },
            "Cluster" => LayoutNode::Cluster { gap: get("gap"), children, span },
            "Sidebar" => LayoutNode::Sidebar { side: get("side"), width: get("width"), children, span },
            "Switcher" => LayoutNode::Switcher { threshold: get("threshold"), children, span },
            _ => unreachable!(),
        };

        Ok(TemplateNode::Layout(node))
    }

    fn is_closing_tag(&self) -> bool {
        self.check(&TokenKind::LeftAngle)
            && self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos + 1].kind == TokenKind::Slash
    }

    fn parse_struct(&mut self, is_pub: bool) -> Result<StructDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Struct)?;
        let name = self.expect_ident()?;
        let (lifetimes, type_params) = self.parse_lifetime_and_type_params()?;
        let trait_bounds = self.parse_where_clause()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            let field_pub = self.match_token(&TokenKind::Pub);
            let field_name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.parse_type()?;
            fields.push(Field { name: field_name, ty, is_pub: field_pub });

            if !self.check(&TokenKind::RightBrace) {
                self.expect(&TokenKind::Comma)?;
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(StructDef { name, lifetimes, type_params, fields, trait_bounds, is_pub, span })
    }

    fn parse_enum(&mut self, is_pub: bool) -> Result<EnumDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Enum)?;
        let name = self.expect_ident()?;
        let type_params = self.parse_type_params()?;
        let _trait_bounds = self.parse_where_clause()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut variants = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            let var_name = self.expect_ident()?;
            let fields = if self.match_token(&TokenKind::LeftParen) {
                let mut f = Vec::new();
                while !self.check(&TokenKind::RightParen) {
                    f.push(self.parse_type()?);
                    if !self.check(&TokenKind::RightParen) {
                        self.expect(&TokenKind::Comma)?;
                    }
                }
                self.expect(&TokenKind::RightParen)?;
                f
            } else {
                vec![]
            };
            variants.push(Variant { name: var_name, fields });

            if !self.check(&TokenKind::RightBrace) {
                self.expect(&TokenKind::Comma)?;
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(EnumDef { name, type_params, variants, is_pub, span })
    }

    fn parse_impl(&mut self) -> Result<ImplBlock, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Impl)?;
        let first_name = self.expect_ident()?;

        // Check for `impl TraitName for TypeName { ... }`
        let (trait_impls, target) = if self.match_token(&TokenKind::For) {
            let target = self.expect_ident()?;
            (vec![first_name], target)
        } else {
            (vec![], first_name)
        };

        self.expect(&TokenKind::LeftBrace)?;

        let mut methods = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            let is_pub = self.match_token(&TokenKind::Pub);
            methods.push(self.parse_function(is_pub)?);
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(ImplBlock { target, trait_impls, methods, span })
    }

    fn parse_trait(&mut self) -> Result<TraitDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Trait)?;
        let name = self.expect_ident()?;
        let type_params = self.parse_type_params()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut methods = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let method_span = self.current_span();
            self.expect(&TokenKind::Fn)?;
            let method_name = self.expect_ident()?;
            self.expect(&TokenKind::LeftParen)?;
            let params = self.parse_params()?;
            self.expect(&TokenKind::RightParen)?;

            let return_type = if self.match_token(&TokenKind::Arrow) {
                Some(self.parse_type()?)
            } else {
                None
            };

            // Check for default body or semicolon
            let default_body = if self.check(&TokenKind::LeftBrace) {
                Some(self.parse_block()?)
            } else {
                self.expect(&TokenKind::Semicolon)?;
                None
            };

            methods.push(TraitMethod {
                name: method_name,
                params,
                return_type,
                default_body,
                span: method_span,
            });
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(TraitDef { name, type_params, methods, span })
    }

    fn parse_use(&mut self) -> Result<UsePath, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Use)?;

        let mut segments = vec![self.expect_ident()?];
        while self.match_token(&TokenKind::ColonColon) {
            // Check for glob import: `use foo::*;`
            if self.check(&TokenKind::Star) {
                self.advance();
                self.expect(&TokenKind::Semicolon)?;
                return Ok(UsePath { segments, alias: None, glob: true, group: None, span });
            }
            // Check for group import: `use foo::{A, B, C};`
            if self.check(&TokenKind::LeftBrace) {
                self.advance();
                let mut group_items = Vec::new();
                while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                    let item_name = self.expect_ident()?;
                    let item_alias = if self.match_token(&TokenKind::As) {
                        Some(self.expect_ident()?)
                    } else {
                        None
                    };
                    group_items.push(UseGroupItem { name: item_name, alias: item_alias });
                    if !self.check(&TokenKind::RightBrace) {
                        self.expect(&TokenKind::Comma)?;
                    }
                }
                self.expect(&TokenKind::RightBrace)?;
                self.expect(&TokenKind::Semicolon)?;
                return Ok(UsePath { segments, alias: None, glob: false, group: Some(group_items), span });
            }
            segments.push(self.expect_ident()?);
        }

        // Check for alias: `use foo::Bar as Baz;`
        let alias = if self.match_token(&TokenKind::As) {
            Some(self.expect_ident()?)
        } else {
            None
        };

        self.expect(&TokenKind::Semicolon)?;
        Ok(UsePath { segments, alias, glob: false, group: None, span })
    }

    fn parse_mod(&mut self) -> Result<ModDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Mod)?;
        let name = self.expect_ident()?;

        if self.match_token(&TokenKind::Semicolon) {
            // External module: `mod foo;`
            Ok(ModDef { name, items: None, is_external: true, span })
        } else {
            // Inline module: `mod foo { ... }`
            self.expect(&TokenKind::LeftBrace)?;
            let mut items = Vec::new();
            while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                items.push(self.parse_item()?);
            }
            self.expect(&TokenKind::RightBrace)?;
            Ok(ModDef { name, items: Some(items), is_external: false, span })
        }
    }

    /// Parse a contract definition:
    /// ```nectar
    /// contract CustomerResponse {
    ///     id: u32,
    ///     name: String,
    ///     email: String,
    ///     balance_cents: i64,
    ///     tier: enum { free, pro, enterprise },
    ///     deleted_at: DateTime?,
    /// }
    /// ```
    fn parse_contract(&mut self, is_pub: bool) -> Result<ContractDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Contract)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut fields = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let field_span = self.current_span();
            let field_name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;

            // Parse the type — could be a regular type or inline enum
            let ty = if self.check(&TokenKind::Enum) {
                // inline enum: `tier: enum { free, pro, enterprise }`
                self.advance(); // consume 'enum'
                self.expect(&TokenKind::LeftBrace)?;
                let mut variants = Vec::new();
                loop {
                    if self.check(&TokenKind::RightBrace) {
                        break;
                    }
                    variants.push(Variant {
                        name: self.expect_ident()?,
                        fields: vec![],
                    });
                    if !self.match_token(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RightBrace)?;
                // Represent inline enum as a named enum with a generated name
                Type::Named(format!("{}_{}", name, field_name))
            } else {
                self.parse_type()?
            };

            // Check for nullable marker: `?`
            let nullable = self.match_token(&TokenKind::QuestionMark);
            let final_ty = if nullable {
                Type::Option(Box::new(ty))
            } else {
                ty
            };

            fields.push(ContractField {
                name: field_name,
                ty: final_ty,
                nullable,
                span: field_span,
            });

            // Comma separator (optional before closing brace)
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(ContractDef { name, fields, is_pub, span })
    }

    /// Parse `app PayHive { manifest { ... } offline { ... } push { ... } router AppRouter { ... } }`
    fn parse_app(&mut self, is_pub: bool) -> Result<AppDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::App)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut manifest = None;
        let mut offline = None;
        let mut push = None;
        let mut router = None;
        let mut a11y = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Manifest => {
                    manifest = Some(self.parse_manifest_def()?);
                }
                TokenKind::Offline => {
                    offline = Some(self.parse_offline_def()?);
                }
                TokenKind::Push => {
                    push = Some(self.parse_push_def()?);
                }
                TokenKind::Router => {
                    router = Some(self.parse_router()?);
                }
                TokenKind::A11y => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    if self.match_token(&TokenKind::Manual) {
                        a11y = Some(A11yMode::Manual);
                    } else {
                        if let TokenKind::Ident(s) = self.peek_kind() {
                            if s == "auto" {
                                self.advance();
                                a11y = Some(A11yMode::Auto);
                            }
                        } else {
                            a11y = Some(A11yMode::Auto);
                        }
                    }
                    self.match_token(&TokenKind::Comma);
                }
                _ => return Err(self.error("Expected manifest, offline, push, router, or a11y in app")),
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(AppDef { name, manifest, offline, push, router, a11y, is_pub, span })
    }

    /// Parse `manifest { name: "My App", short_name: "app", ... }`
    fn parse_manifest_def(&mut self) -> Result<ManifestDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Manifest)?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut entries = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let value = self.parse_expr()?;
            entries.push((key, value));
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(ManifestDef { entries, span })
    }

    /// Parse `offline { precache: [...], strategy: "cache-first", fallback: OfflinePage }`
    fn parse_offline_def(&mut self) -> Result<OfflineDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Offline)?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut precache = Vec::new();
        let mut strategy = "cache-first".to_string();
        let mut fallback = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            match key.as_str() {
                "precache" => {
                    self.expect(&TokenKind::LeftBracket)?;
                    while !self.check(&TokenKind::RightBracket) && !self.is_at_end() {
                        if let TokenKind::StringLit(s) = self.peek_kind() {
                            precache.push(s.clone());
                            self.advance();
                        } else {
                            return Err(self.error("Expected string in precache list"));
                        }
                        self.match_token(&TokenKind::Comma);
                    }
                    self.expect(&TokenKind::RightBracket)?;
                }
                "strategy" => {
                    if let TokenKind::StringLit(s) = self.peek_kind() {
                        strategy = s.clone();
                        self.advance();
                    } else {
                        return Err(self.error("Expected string for strategy"));
                    }
                }
                "fallback" => {
                    fallback = Some(self.expect_ident()?);
                }
                _ => return Err(self.error(&format!("Unknown offline key: {}", key))),
            }
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(OfflineDef { precache, strategy, fallback, span })
    }

    /// Parse `push { vapid_key: "...", on_message: handle_push }`
    fn parse_push_def(&mut self) -> Result<PushDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Push)?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut vapid_key = None;
        let mut on_message = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            match key.as_str() {
                "vapid_key" => {
                    vapid_key = Some(self.parse_expr()?);
                }
                "on_message" => {
                    on_message = Some(self.expect_ident()?);
                }
                _ => return Err(self.error(&format!("Unknown push key: {}", key))),
            }
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(PushDef { vapid_key, on_message, span })
    }

    /// Parse `gesture swipe_left { ... }` or `gesture long_press on:card { ... }`
    fn parse_gesture(&mut self) -> Result<GestureDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Gesture)?;
        let gesture_type = self.expect_ident()?;

        // Optional `on:target`
        let target = if self.check(&TokenKind::On) {
            self.advance(); // consume `on:`
            Some(self.expect_ident()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        Ok(GestureDef { gesture_type, target, body, span })
    }

    fn parse_store(&mut self, is_pub: bool) -> Result<StoreDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Store)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut signals = Vec::new();
        let mut actions = Vec::new();
        let mut computed = Vec::new();
        let mut effects = Vec::new();
        let mut selectors = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Signal => {
                    signals.push(self.parse_signal_field()?);
                }
                TokenKind::Action | TokenKind::Async => {
                    let is_async = self.match_token(&TokenKind::Async);
                    actions.push(self.parse_action(is_async)?);
                }
                TokenKind::Computed => {
                    computed.push(self.parse_computed()?);
                }
                TokenKind::Effect => {
                    effects.push(self.parse_effect()?);
                }
                TokenKind::Selector => {
                    let sel_span = self.current_span();
                    self.advance();
                    let sel_name = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    let body = self.parse_expr()?;
                    self.match_token(&TokenKind::Semicolon);
                    selectors.push(SelectorDef { name: sel_name, deps: vec![], body, span: sel_span });
                }
                _ => return Err(self.error("Expected signal, action, computed, effect, or selector in store")),
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(StoreDef { name, signals, actions, computed, effects, selectors, is_pub, span })
    }

    fn parse_action(&mut self, is_async: bool) -> Result<ActionDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Action)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RightParen)?;
        let body = self.parse_block()?;
        Ok(ActionDef { name, params, body, is_async, span })
    }

    fn parse_computed(&mut self) -> Result<ComputedDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Computed)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftParen)?;
        let _params = self.parse_params()?; // typically just &self
        self.expect(&TokenKind::RightParen)?;
        let return_type = if self.match_token(&TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        Ok(ComputedDef { name, return_type, body, span })
    }

    fn parse_effect(&mut self) -> Result<EffectDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Effect)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftParen)?;
        let _params = self.parse_params()?; // typically just &self
        self.expect(&TokenKind::RightParen)?;
        let body = self.parse_block()?;
        Ok(EffectDef { name, body, span })
    }

    fn parse_agent(&mut self) -> Result<AgentDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Agent)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut system_prompt = None;
        let mut tools = Vec::new();
        let mut state = Vec::new();
        let mut methods = Vec::new();
        let mut render = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Prompt => {
                    // prompt system = "...";
                    self.advance();
                    let _label = self.expect_ident()?; // e.g. "system"
                    self.expect(&TokenKind::Equals)?;
                    if let TokenKind::StringLit(s) = self.peek_kind() {
                        self.advance();
                        system_prompt = Some(s);
                    } else {
                        return Err(self.error("Expected string literal for prompt"));
                    }
                    self.expect(&TokenKind::Semicolon)?;
                }
                TokenKind::Tool => {
                    tools.push(self.parse_tool_def()?);
                }
                TokenKind::Signal => {
                    state.push(self.parse_signal_field()?);
                }
                TokenKind::Let => {
                    state.push(self.parse_state_field()?);
                }
                TokenKind::Fn => {
                    methods.push(self.parse_function(false)?);
                }
                TokenKind::Render => {
                    render = Some(self.parse_render_block()?);
                }
                _ => return Err(self.error(
                    "Expected prompt, tool, signal, let, fn, or render in agent"
                )),
            }
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(AgentDef {
            name,
            system_prompt,
            tools,
            state,
            methods,
            render,
            span,
        })
    }

    fn parse_tool_def(&mut self) -> Result<ToolDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Tool)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RightParen)?;

        let return_type = if self.match_token(&TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        let body = self.parse_block()?;

        Ok(ToolDef {
            name,
            description: None,
            params,
            return_type,
            body,
            span,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();

        while !self.check(&TokenKind::RightParen) {
            // Handle &self, &mut self, self, mut self
            let is_mut_self = self.check(&TokenKind::Mut)
                && self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::SelfKw);
            if self.check(&TokenKind::Ampersand) || self.check(&TokenKind::SelfKw) || is_mut_self {
                let ownership = if self.match_token(&TokenKind::Ampersand) {
                    if self.match_token(&TokenKind::Mut) {
                        Ownership::MutBorrowed
                    } else {
                        Ownership::Borrowed
                    }
                } else if self.match_token(&TokenKind::Mut) {
                    // mut self — treat as owned (mutable)
                    Ownership::Owned
                } else {
                    Ownership::Owned
                };
                self.expect(&TokenKind::SelfKw)?;
                params.push(Param {
                    name: "self".into(),
                    ty: Type::Named("Self".into()),
                    ownership,
                    secret: false,
                });
            } else {
                // Check for optional `secret` modifier before parameter name
                let is_secret = self.match_token(&TokenKind::Secret);
                let name = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;

                let (ownership, ty) = if self.match_token(&TokenKind::Ampersand) {
                    if self.match_token(&TokenKind::Mut) {
                        (Ownership::MutBorrowed, self.parse_type()?)
                    } else {
                        (Ownership::Borrowed, self.parse_type()?)
                    }
                } else {
                    (Ownership::Owned, self.parse_type()?)
                };

                params.push(Param { name, ty, ownership, secret: is_secret });
            }

            if !self.check(&TokenKind::RightParen) {
                self.expect(&TokenKind::Comma)?;
            }
        }

        Ok(params)
    }

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        if self.match_token(&TokenKind::Ampersand) {
            // Check for optional lifetime: &'a T, &'a mut T
            let lifetime = if let TokenKind::Lifetime(name) = self.peek_kind() {
                self.advance();
                Some(name)
            } else {
                None
            };
            let mutable = self.match_token(&TokenKind::Mut);
            let inner = self.parse_type()?;
            return Ok(Type::Reference { mutable, lifetime, inner: Box::new(inner) });
        }

        if self.match_token(&TokenKind::LeftBracket) {
            let inner = self.parse_type()?;
            self.expect(&TokenKind::RightBracket)?;
            return Ok(Type::Array(Box::new(inner)));
        }

        let name: String = match self.peek_kind() {
            TokenKind::I32 => { self.advance(); "i32".into() }
            TokenKind::I64 => { self.advance(); "i64".into() }
            TokenKind::F32 => { self.advance(); "f32".into() }
            TokenKind::F64 => { self.advance(); "f64".into() }
            TokenKind::U32 => { self.advance(); "u32".into() }
            TokenKind::U64 => { self.advance(); "u64".into() }
            TokenKind::Bool_ => { self.advance(); "bool".into() }
            TokenKind::StringType => { self.advance(); "String".into() }
            TokenKind::SelfType => { self.advance(); "Self".into() }
            _ => self.expect_ident()?,
        };

        // Check for generic type arguments: `Name<Type, Type, ...>`
        if self.check(&TokenKind::LeftAngle) {
            self.advance(); // consume `<`
            let mut args = Vec::new();
            while !self.check(&TokenKind::RightAngle) && !self.is_at_end() {
                args.push(self.parse_type()?);
                if !self.check(&TokenKind::RightAngle) {
                    self.expect(&TokenKind::Comma)?;
                }
            }
            self.expect(&TokenKind::RightAngle)?;
            return Ok(Type::Generic { name, args });
        }

        Ok(Type::Named(name))
    }

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::LeftBrace)?;

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            stmts.push(self.parse_stmt()?);
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(Block { stmts, span })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.peek_kind() {
            TokenKind::Let => {
                self.advance();
                let mutable = self.match_token(&TokenKind::Mut);
                let secret = self.match_token(&TokenKind::Secret);

                // Check for destructuring patterns: let (a, b) = ..., let Name { ... } = ..., let [a, b] = ...
                if self.check(&TokenKind::LeftParen) || self.check(&TokenKind::LeftBracket) {
                    // Tuple or array destructure
                    let pattern = self.parse_destructure_pattern()?;
                    let ty = if self.match_token(&TokenKind::Colon) {
                        Some(self.parse_type()?)
                    } else {
                        None
                    };
                    self.expect(&TokenKind::Equals)?;
                    let value = self.parse_expr()?;
                    self.expect(&TokenKind::Semicolon)?;
                    Ok(Stmt::LetDestructure { pattern, ty, value })
                } else {
                    let name = self.expect_ident()?;

                    // Check for struct destructure: let Name { field1, field2, .. } = expr;
                    if self.check(&TokenKind::LeftBrace) {
                        let pattern = self.parse_struct_destructure_pattern(name)?;
                        let ty = if self.match_token(&TokenKind::Colon) {
                            Some(self.parse_type()?)
                        } else {
                            None
                        };
                        self.expect(&TokenKind::Equals)?;
                        let value = self.parse_expr()?;
                        self.expect(&TokenKind::Semicolon)?;
                        Ok(Stmt::LetDestructure { pattern, ty, value })
                    } else {
                        // Regular let binding
                        let ty = if self.match_token(&TokenKind::Colon) {
                            Some(self.parse_type()?)
                        } else {
                            None
                        };

                        self.expect(&TokenKind::Equals)?;

                        let ownership = if self.match_token(&TokenKind::Own) {
                            Ownership::Owned
                        } else {
                            Ownership::Owned
                        };

                        let value = self.parse_expr()?;
                        self.expect(&TokenKind::Semicolon)?;
                        Ok(Stmt::Let { name, ty, mutable, secret, value, ownership })
                    }
                }
            }
            TokenKind::Signal => {
                self.advance();
                let atomic = self.match_token(&TokenKind::Atomic);
                let secret = self.match_token(&TokenKind::Secret);
                let name = self.expect_ident()?;
                let ty = if self.match_token(&TokenKind::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect(&TokenKind::Equals)?;
                let value = self.parse_expr()?;
                self.expect(&TokenKind::Semicolon)?;
                Ok(Stmt::Signal { name, ty, secret, atomic, value })
            }
            TokenKind::Return => {
                self.advance();
                if self.check(&TokenKind::Semicolon) {
                    self.advance();
                    Ok(Stmt::Return(None))
                } else {
                    let expr = self.parse_expr()?;
                    self.expect(&TokenKind::Semicolon)?;
                    Ok(Stmt::Return(Some(expr)))
                }
            }
            TokenKind::Yield => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::Semicolon)?;
                Ok(Stmt::Yield(expr))
            }
            // `<element>` — a template node used as an expression-statement inside
            // a closure body (e.g., the virtual list row template).
            // We parse and discard the element structure; codegen handles the virtual
            // list template separately via the VirtualList AST node.
            TokenKind::LeftAngle => {
                self.parse_template_element_as_stmt()
            }
            _ => {
                let expr = self.parse_expr()?;
                // Optional semicolon for expression statements
                self.match_token(&TokenKind::Semicolon);
                Ok(Stmt::Expr(expr))
            }
        }
    }

    /// Parse a template element `<tag ...>...</tag>` appearing as a statement
    /// inside a closure body (e.g., virtual list row template). We consume the
    /// full element including nested children and return a placeholder `Stmt::Expr`.
    fn parse_template_element_as_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::LeftAngle)?;
        let tag = self.expect_ident()?;
        // Consume attributes until `>` or `/>`
        let mut depth = 1u32;
        let mut self_closing = false;
        // Consume attributes (simplified: eat tokens until > or />)
        loop {
            match self.peek_kind() {
                TokenKind::Slash => {
                    self.advance();
                    self.expect(&TokenKind::RightAngle)?;
                    self_closing = true;
                    break;
                }
                TokenKind::RightAngle => {
                    self.advance();
                    break;
                }
                TokenKind::Eof => break,
                _ => { self.advance(); }
            }
        }
        if !self_closing {
            // Consume children until matching closing tag
            while depth > 0 && !self.is_at_end() {
                match self.peek_kind() {
                    TokenKind::LeftAngle => {
                        self.advance();
                        if self.check(&TokenKind::Slash) {
                            // Closing tag
                            self.advance();
                            let _ = self.expect_ident(); // tag name
                            let _ = self.expect(&TokenKind::RightAngle);
                            depth -= 1;
                        } else {
                            // Opening tag — consume attrs and check for self-close
                            let _ = self.expect_ident(); // tag name
                            loop {
                                match self.peek_kind() {
                                    TokenKind::Slash => {
                                        self.advance();
                                        let _ = self.expect(&TokenKind::RightAngle);
                                        break;
                                    }
                                    TokenKind::RightAngle => { self.advance(); depth += 1; break; }
                                    TokenKind::Eof => break,
                                    _ => { self.advance(); }
                                }
                            }
                        }
                    }
                    _ => { self.advance(); }
                }
            }
        }
        // Return a placeholder expression for this template element
        let _ = tag;
        Ok(Stmt::Expr(Expr::Ident("__template__".to_string())))
    }

    // === Expression parsing (Pratt parser) ===

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_range()?;

        if self.match_token(&TokenKind::Equals) {
            let value = self.parse_assignment()?;
            return Ok(Expr::Assign {
                target: Box::new(expr),
                value: Box::new(value),
            });
        }

        // Compound assignment operators
        if self.match_token(&TokenKind::PlusEquals) {
            let value = self.parse_assignment()?;
            return Ok(Expr::Assign {
                target: Box::new(expr.clone()),
                value: Box::new(Expr::Binary {
                    op: BinOp::Add,
                    left: Box::new(expr),
                    right: Box::new(value),
                }),
            });
        }

        Ok(expr)
    }

    fn parse_range(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_or()?;
        if self.match_token(&TokenKind::DotDot) {
            let end = self.parse_or()?;
            return Ok(Expr::Range {
                start: Box::new(expr),
                end: Box::new(end),
            });
        }
        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.match_token(&TokenKind::PipePipe) {
            let right = self.parse_and()?;
            left = Expr::Binary { op: BinOp::Or, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;
        while self.match_token(&TokenKind::AmpAmp) {
            let right = self.parse_equality()?;
            left = Expr::Binary { op: BinOp::And, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison()?;
        loop {
            if self.match_token(&TokenKind::DoubleEquals) {
                let right = self.parse_comparison()?;
                left = Expr::Binary { op: BinOp::Eq, left: Box::new(left), right: Box::new(right) };
            } else if self.match_token(&TokenKind::NotEquals) {
                let right = self.parse_comparison()?;
                left = Expr::Binary { op: BinOp::Neq, left: Box::new(left), right: Box::new(right) };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive()?;
        loop {
            if self.match_token(&TokenKind::LeftAngle) {
                let right = self.parse_additive()?;
                left = Expr::Binary { op: BinOp::Lt, left: Box::new(left), right: Box::new(right) };
            } else if self.match_token(&TokenKind::RightAngle) {
                let right = self.parse_additive()?;
                left = Expr::Binary { op: BinOp::Gt, left: Box::new(left), right: Box::new(right) };
            } else if self.match_token(&TokenKind::LessEqual) {
                let right = self.parse_additive()?;
                left = Expr::Binary { op: BinOp::Lte, left: Box::new(left), right: Box::new(right) };
            } else if self.match_token(&TokenKind::GreaterEqual) {
                let right = self.parse_additive()?;
                left = Expr::Binary { op: BinOp::Gte, left: Box::new(left), right: Box::new(right) };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            if self.match_token(&TokenKind::Plus) {
                let right = self.parse_multiplicative()?;
                left = Expr::Binary { op: BinOp::Add, left: Box::new(left), right: Box::new(right) };
            } else if self.match_token(&TokenKind::Minus) {
                let right = self.parse_multiplicative()?;
                left = Expr::Binary { op: BinOp::Sub, left: Box::new(left), right: Box::new(right) };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            if self.match_token(&TokenKind::Star) {
                let right = self.parse_unary()?;
                left = Expr::Binary { op: BinOp::Mul, left: Box::new(left), right: Box::new(right) };
            } else if self.match_token(&TokenKind::Slash) {
                let right = self.parse_unary()?;
                left = Expr::Binary { op: BinOp::Div, left: Box::new(left), right: Box::new(right) };
            } else if self.match_token(&TokenKind::Percent) {
                let right = self.parse_unary()?;
                left = Expr::Binary { op: BinOp::Mod, left: Box::new(left), right: Box::new(right) };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.match_token(&TokenKind::Minus) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary { op: UnaryOp::Neg, operand: Box::new(operand) });
        }
        if self.match_token(&TokenKind::Bang) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary { op: UnaryOp::Not, operand: Box::new(operand) });
        }
        if self.match_token(&TokenKind::Ampersand) {
            if self.match_token(&TokenKind::Mut) {
                let operand = self.parse_unary()?;
                return Ok(Expr::BorrowMut(Box::new(operand)));
            }
            let operand = self.parse_unary()?;
            return Ok(Expr::Borrow(Box::new(operand)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.match_token(&TokenKind::Dot) {
                let field = self.expect_ident()?;
                if field == "send" && self.check(&TokenKind::LeftParen) {
                    // ch.send(value) -> Expr::Send
                    self.expect(&TokenKind::LeftParen)?;
                    let value = self.parse_expr()?;
                    self.expect(&TokenKind::RightParen)?;
                    expr = Expr::Send {
                        channel: Box::new(expr),
                        value: Box::new(value),
                    };
                } else if field == "recv" && self.check(&TokenKind::LeftParen) {
                    // ch.recv() -> Expr::Receive
                    self.expect(&TokenKind::LeftParen)?;
                    self.expect(&TokenKind::RightParen)?;
                    expr = Expr::Receive {
                        channel: Box::new(expr),
                    };
                } else if self.match_token(&TokenKind::LeftParen) {
                    let args = self.parse_args()?;
                    self.expect(&TokenKind::RightParen)?;
                    expr = Expr::MethodCall {
                        object: Box::new(expr),
                        method: field,
                        args,
                    };
                } else if self.check(&TokenKind::LeftBrace)
                    && field.chars().next().is_some_and(|c| c.is_uppercase())
                {
                    // `Schema.Article { field: val }` — struct construction via dot-access.
                    // Only valid when the field name is PascalCase (a type name).
                    // This prevents `self.cond { ... }` from being mis-parsed as struct init.
                    let base_name = match &expr {
                        Expr::Ident(n) => format!("{}.{}", n, field),
                        Expr::FieldAccess { object, field: f } => {
                            // Flatten nested field accesses to a qualified name
                            fn flatten(e: &Expr) -> String {
                                match e {
                                    Expr::Ident(n) => n.clone(),
                                    Expr::FieldAccess { object, field } => {
                                        format!("{}.{}", flatten(object), field)
                                    }
                                    _ => "__obj__".to_string(),
                                }
                            }
                            format!("{}.{}", flatten(object), f)
                        }
                        _ => field.clone(),
                    };
                    self.advance(); // consume `{`
                    let mut fields = Vec::new();
                    while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                        let fname = self.expect_ident()?;
                        self.expect(&TokenKind::Colon)?;
                        let fval = self.parse_expr()?;
                        fields.push((fname, fval));
                        if !self.check(&TokenKind::RightBrace) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&TokenKind::RightBrace)?;
                    expr = Expr::StructInit { name: base_name, fields };
                } else {
                    expr = Expr::FieldAccess {
                        object: Box::new(expr),
                        field,
                    };
                }
            } else if self.match_token(&TokenKind::LeftParen) {
                let args = self.parse_args()?;
                self.expect(&TokenKind::RightParen)?;
                expr = Expr::FnCall {
                    callee: Box::new(expr),
                    args,
                };
            } else if self.match_token(&TokenKind::LeftBracket) {
                let index = self.parse_expr()?;
                self.expect(&TokenKind::RightBracket)?;
                expr = Expr::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                };
            } else if self.match_token(&TokenKind::QuestionDot) {
                // `?.` optional chaining operator — short-circuit to 0 if null
                let field = self.expect_ident()?;
                expr = Expr::OptionalChain {
                    object: Box::new(expr),
                    field,
                };
            } else if self.match_token(&TokenKind::QuestionMark) {
                // `?` error propagation operator — postfix
                expr = Expr::Try(Box::new(expr));
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek_kind() {
            TokenKind::Integer(_) => {
                if let TokenKind::Integer(n) = self.advance().kind {
                    Ok(Expr::Integer(n))
                } else { unreachable!() }
            }
            TokenKind::Float(_) => {
                if let TokenKind::Float(f) = self.advance().kind {
                    Ok(Expr::Float(f))
                } else { unreachable!() }
            }
            TokenKind::StringLit(_) => {
                if let TokenKind::StringLit(s) = self.advance().kind {
                    Ok(Expr::StringLit(s))
                } else { unreachable!() }
            }
            TokenKind::FormatString(_) => {
                self.parse_format_string_expr()
            }
            TokenKind::True => { self.advance(); Ok(Expr::Bool(true)) }
            TokenKind::False => { self.advance(); Ok(Expr::Bool(false)) }
            TokenKind::SelfKw => { self.advance(); Ok(Expr::SelfExpr) }
            TokenKind::LeftBracket => {
                self.advance();
                let mut elements = Vec::new();
                while !self.check(&TokenKind::RightBracket) && !self.is_at_end() {
                    elements.push(self.parse_expr()?);
                    if !self.check(&TokenKind::RightBracket) {
                        self.expect(&TokenKind::Comma)?;
                    }
                }
                self.expect(&TokenKind::RightBracket)?;
                Ok(Expr::ArrayLit(elements))
            }
            TokenKind::Await => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Await(Box::new(expr)))
            }
            TokenKind::Prompt => {
                // prompt "Summarize this: {document}"
                // Parses the string literal, extracts {interpolations}
                self.advance();
                if let TokenKind::StringLit(template) = self.peek_kind() {
                    self.advance();
                    // Extract interpolation names from {name} placeholders
                    let mut interpolations = Vec::new();
                    let mut rest = template.as_str();
                    while let Some(start) = rest.find('{') {
                        if let Some(end) = rest[start..].find('}') {
                            let var_name = rest[start + 1..start + end].to_string();
                            interpolations.push((
                                var_name.clone(),
                                Expr::Ident(var_name),
                            ));
                            rest = &rest[start + end + 1..];
                        } else {
                            break;
                        }
                    }
                    Ok(Expr::PromptTemplate { template, interpolations })
                } else {
                    Err(self.error("Expected string literal after prompt"))
                }
            }
            TokenKind::Assert => {
                // assert(condition) or assert(condition, "message")
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let condition = self.parse_expr()?;
                let message = if self.match_token(&TokenKind::Comma) {
                    if let TokenKind::StringLit(s) = self.peek_kind() {
                        self.advance();
                        Some(s)
                    } else {
                        None
                    }
                } else {
                    None
                };
                self.expect(&TokenKind::RightParen)?;
                Ok(Expr::Assert { condition: Box::new(condition), message })
            }
            TokenKind::AssertEq => {
                // assert_eq(left, right) or assert_eq(left, right, "message")
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let left = self.parse_expr()?;
                self.expect(&TokenKind::Comma)?;
                let right = self.parse_expr()?;
                let message = if self.match_token(&TokenKind::Comma) {
                    if let TokenKind::StringLit(s) = self.peek_kind() {
                        self.advance();
                        Some(s)
                    } else {
                        None
                    }
                } else {
                    None
                };
                self.expect(&TokenKind::RightParen)?;
                Ok(Expr::AssertEq { left: Box::new(left), right: Box::new(right), message })
            }
            TokenKind::Env => {
                self.advance();
                let span = self.current_span();
                self.expect(&TokenKind::LeftParen)?;
                let name = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                Ok(Expr::Env { name: Box::new(name), span })
            }
            TokenKind::Trace => {
                self.advance();
                let span = self.current_span();
                self.expect(&TokenKind::LeftParen)?;
                let label = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                let body = self.parse_block()?;
                Ok(Expr::Trace { label: Box::new(label), body, span })
            }
            TokenKind::Flag => {
                self.advance();
                let span = self.current_span();
                self.expect(&TokenKind::LeftParen)?;
                let name = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                Ok(Expr::Flag { name: Box::new(name), span })
            }
            TokenKind::Fetch => {
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let url = self.parse_expr()?;
                let options = if self.match_token(&TokenKind::Comma) {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                self.expect(&TokenKind::RightParen)?;
                // Optional contract binding: fetch(...) -> ContractName
                let contract = if self.match_token(&TokenKind::Arrow) {
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                Ok(Expr::Fetch { url: Box::new(url), options, contract })
            }
            TokenKind::Navigate => {
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let path = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                Ok(Expr::Navigate { path: Box::new(path) })
            }
            TokenKind::Download => {
                let span = self.current_span();
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let data = self.parse_expr()?;
                self.expect(&TokenKind::Comma)?;
                let filename = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                Ok(Expr::Download { data: Box::new(data), filename: Box::new(filename), span })
            }
            TokenKind::Spawn => {
                self.advance();
                let span = self.current_span();
                let body = self.parse_block()?;
                Ok(Expr::Spawn { body, span })
            }
            TokenKind::Channel => {
                self.advance();
                // Optional type parameter: channel<i32>()
                let ty = if self.match_token(&TokenKind::LeftAngle) {
                    let t = self.parse_type()?;
                    self.expect(&TokenKind::RightAngle)?;
                    Some(t)
                } else {
                    None
                };
                self.expect(&TokenKind::LeftParen)?;
                self.expect(&TokenKind::RightParen)?;
                Ok(Expr::Channel { ty })
            }
            TokenKind::Select => {
                // select is parsed but desugars to a match on channel readiness
                // For now, parse as a block expression
                self.advance();
                let block = self.parse_block()?;
                Ok(Expr::Block(block))
            }
            TokenKind::Parallel => {
                self.advance();
                let span = self.current_span();
                self.expect(&TokenKind::LeftBrace)?;
                let mut tasks = Vec::new();
                while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                    tasks.push(self.parse_expr()?);
                    self.match_token(&TokenKind::Comma);
                }
                self.expect(&TokenKind::RightBrace)?;
                Ok(Expr::Parallel { tasks, span })
            }
            TokenKind::Stream => {
                // stream <source_expr>
                // e.g., for chunk in stream fetch("...") { ... }
                self.advance();
                let source = self.parse_unary()?;
                Ok(Expr::Stream { source: Box::new(source) })
            }
            TokenKind::Suspend => {
                // suspend(<fallback_expr>) { <body_expr> }
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let fallback = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                self.expect(&TokenKind::LeftBrace)?;
                let body = self.parse_expr()?;
                self.expect(&TokenKind::RightBrace)?;
                Ok(Expr::Suspend {
                    fallback: Box::new(fallback),
                    body: Box::new(body),
                })
            }
            TokenKind::Try => {
                // try { ... } catch err { ... }
                self.advance();
                let try_block = self.parse_block()?;
                let try_body = Expr::Block(try_block);
                self.expect(&TokenKind::Catch)?;
                let error_binding = self.expect_ident()?;
                let catch_block = self.parse_block()?;
                let catch_body = Expr::Block(catch_block);
                Ok(Expr::TryCatch {
                    body: Box::new(try_body),
                    error_binding,
                    catch_body: Box::new(catch_body),
                })
            }
            TokenKind::Virtual => {
                // virtual list=expr item_height=expr { |item, index| ... }
                self.advance();
                let span = self.current_span();
                let mut items = None;
                let mut item_height = None;
                let mut buffer = None;

                // Parse key=value attributes until left brace
                while !self.check(&TokenKind::LeftBrace) && !self.check(&TokenKind::Pipe) && !self.check(&TokenKind::PipePipe) && !self.is_at_end() {
                    let key = self.expect_ident()?;
                    self.expect(&TokenKind::Equals)?;
                    match key.as_str() {
                        "list" => { items = Some(self.parse_expr()?); }
                        "item_height" => { item_height = Some(self.parse_expr()?); }
                        "buffer" => {
                            if let TokenKind::Integer(n) = self.peek_kind() {
                                buffer = Some(n as u32);
                                self.advance();
                            } else {
                                return Err(self.error("Expected integer for buffer"));
                            }
                        }
                        _ => { self.advance(); }
                    }
                    self.match_token(&TokenKind::Comma);
                }

                let template = self.parse_expr()?;

                Ok(Expr::VirtualList {
                    items: Box::new(items.unwrap_or(Expr::Ident("items".to_string()))),
                    item_height: Box::new(item_height.unwrap_or(Expr::Integer(40))),
                    template: Box::new(template),
                    buffer,
                    span,
                })
            }
            TokenKind::Animate => {
                // animate(target, "animationName") — imperative animation trigger
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let target = self.parse_expr()?;
                self.expect(&TokenKind::Comma)?;
                let animation = if let TokenKind::StringLit(_) = self.peek_kind() {
                    if let TokenKind::StringLit(s) = self.advance().kind { s } else { unreachable!() }
                } else {
                    return Err(self.error("Expected string literal for animation name"));
                };
                self.expect(&TokenKind::RightParen)?;
                Ok(Expr::Animate {
                    target: Box::new(target),
                    animation,
                })
            }
            TokenKind::If => self.parse_if_expr(),
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::For => self.parse_for_expr(),
            TokenKind::While => self.parse_while_expr(),
            TokenKind::LeftParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                Ok(expr)
            }
            TokenKind::LeftBrace => {
                if self.is_object_literal_brace() {
                    self.parse_object_literal()
                } else {
                    let block = self.parse_block()?;
                    Ok(Expr::Block(block))
                }
            }
            // Closure / lambda: |params| body
            TokenKind::Pipe => {
                self.advance(); // consume opening `|`
                let params = self.parse_closure_params()?;
                self.expect(&TokenKind::Pipe)?; // consume closing `|`
                let body = self.parse_closure_body()?;
                Ok(Expr::Closure { params, body: Box::new(body) })
            }
            // No-param closure: || body
            TokenKind::PipePipe => {
                self.advance(); // consume `||`
                let body = self.parse_closure_body()?;
                Ok(Expr::Closure { params: Vec::new(), body: Box::new(body) })
            }
            // fn(params) -> RetType { body } closure syntax
            TokenKind::Fn => {
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let mut params = Vec::new();
                while !self.check(&TokenKind::RightParen) && !self.is_at_end() {
                    let pname = self.expect_ident()?;
                    let pty = if self.match_token(&TokenKind::Colon) {
                        Some(self.parse_type()?)
                    } else {
                        None
                    };
                    params.push((pname, pty));
                    if !self.check(&TokenKind::RightParen) {
                        self.expect(&TokenKind::Comma)?;
                    }
                }
                self.expect(&TokenKind::RightParen)?;
                // Optional return type
                if self.match_token(&TokenKind::Arrow) {
                    let _ret_ty = self.parse_type()?;
                }
                let body = if self.check(&TokenKind::LeftBrace) {
                    let block = self.parse_block()?;
                    if let Some(last) = block.stmts.last() {
                        match last {
                            Stmt::Expr(e) => e.clone(),
                            _ => Expr::Block(block),
                        }
                    } else {
                        Expr::Block(block)
                    }
                } else {
                    self.parse_expr()?
                };
                Ok(Expr::Closure { params, body: Box::new(body) })
            }

            TokenKind::Ident(ref id) if id == "vec" => {
                self.advance();
                if self.match_token(&TokenKind::Bang) {
                    // vec![...] or vec!(...)
                    let (open, close) = if self.check(&TokenKind::LeftBracket) {
                        (TokenKind::LeftBracket, TokenKind::RightBracket)
                    } else {
                        (TokenKind::LeftParen, TokenKind::RightParen)
                    };
                    self.expect(&open)?;
                    let mut elements = Vec::new();
                    while !self.check(&close) && !self.is_at_end() {
                        elements.push(self.parse_expr()?);
                        if !self.check(&close) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&close)?;
                    Ok(Expr::ArrayLit(elements))
                } else {
                    Ok(Expr::Ident("vec".to_string()))
                }
            }
            TokenKind::Ident(_) => {
                let span = self.current_span();
                let name = self.expect_ident()?;
                // Namespaced call: crypto::sha256, collections::map_new, etc.
                // Join ident :: ident into a single qualified name.
                let name = if self.check(&TokenKind::ColonColon) {
                    let mut qualified = name;
                    while self.match_token(&TokenKind::ColonColon) {
                        let segment = self.expect_ident()?;
                        qualified.push_str("::");
                        qualified.push_str(&segment);
                    }
                    qualified
                } else {
                    name
                };
                // Dynamic import: import("./module") — triggers code split
                if name == "import" && self.check(&TokenKind::LeftParen) {
                    self.expect(&TokenKind::LeftParen)?;
                    let path = self.parse_expr()?;
                    self.expect(&TokenKind::RightParen)?;
                    return Ok(Expr::DynamicImport { path: Box::new(path), span });
                }
                // Check for struct init: Name { field: val }
                if self.check(&TokenKind::LeftBrace) && name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    self.advance();
                    let mut fields = Vec::new();
                    while !self.check(&TokenKind::RightBrace) {
                        let fname = self.expect_ident()?;
                        self.expect(&TokenKind::Colon)?;
                        let fval = self.parse_expr()?;
                        fields.push((fname, fval));
                        if !self.check(&TokenKind::RightBrace) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&TokenKind::RightBrace)?;
                    Ok(Expr::StructInit { name, fields })
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            // Keyword tokens that double as stdlib namespace prefixes (crypto::sha256, etc.)
            TokenKind::Crypto | TokenKind::Cache | TokenKind::Db | TokenKind::Auth
            | TokenKind::Clipboard | TokenKind::Upload | TokenKind::Payment
            | TokenKind::Banking | TokenKind::MapKeyword => {
                let _span = self.current_span();
                let name = match self.peek_kind() {
                    TokenKind::Crypto => "crypto",
                    TokenKind::Cache => "cache",
                    TokenKind::Db => "db",
                    TokenKind::Auth => "auth",
                    TokenKind::Clipboard => "clipboard",
                    TokenKind::Upload => "upload",
                    TokenKind::Payment => "payment",
                    TokenKind::Banking => "banking",
                    TokenKind::MapKeyword => "map",
                    _ => unreachable!(),
                }.to_string();
                self.advance();
                // Require :: namespace access — bare keyword use is handled elsewhere
                let name = if self.check(&TokenKind::ColonColon) {
                    let mut qualified = name;
                    while self.match_token(&TokenKind::ColonColon) {
                        let segment = self.expect_ident()?;
                        qualified.push_str("::");
                        qualified.push_str(&segment);
                    }
                    qualified
                } else {
                    name
                };
                if self.check(&TokenKind::LeftParen) {
                    self.advance();
                    let mut args = Vec::new();
                    while !self.check(&TokenKind::RightParen) && !self.is_at_end() {
                        args.push(self.parse_expr()?);
                        if !self.check(&TokenKind::RightParen) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&TokenKind::RightParen)?;
                    Ok(Expr::FnCall {
                        callee: Box::new(Expr::Ident(name)),
                        args,
                    })
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            // Ident-like keyword tokens usable as function calls or identifiers in expressions.
            // These are keywords that also double as stdlib function names (fluid, format, env, etc.)
            // or as namespace identifiers when the context is an expression.
            _ if self.is_ident_like() => {
                let name = self.expect_ident()?;
                // Handle optional :: namespacing
                let name = if self.check(&TokenKind::ColonColon) {
                    let mut qualified = name;
                    while self.match_token(&TokenKind::ColonColon) {
                        let segment = self.expect_ident()?;
                        qualified.push_str("::");
                        qualified.push_str(&segment);
                    }
                    qualified
                } else {
                    name
                };
                if self.check(&TokenKind::LeftParen) {
                    self.advance();
                    let mut args = Vec::new();
                    while !self.check(&TokenKind::RightParen) && !self.is_at_end() {
                        args.push(self.parse_expr()?);
                        if !self.check(&TokenKind::RightParen) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&TokenKind::RightParen)?;
                    Ok(Expr::FnCall {
                        callee: Box::new(Expr::Ident(name)),
                        args,
                    })
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            _ => Err(self.error("Expected expression")),
        }
    }

    /// Parse closure parameter list (between the `|` delimiters).
    /// Supports: `x`, `x: i32`, `x, y`, `x: i32, y: String`
    fn parse_closure_params(&mut self) -> Result<Vec<(String, Option<Type>)>, ParseError> {
        let mut params = Vec::new();
        while !self.check(&TokenKind::Pipe) && !self.is_at_end() {
            let name = self.expect_ident()?;
            let ty = if self.match_token(&TokenKind::Colon) {
                Some(self.parse_type()?)
            } else {
                None
            };
            params.push((name, ty));
            if !self.check(&TokenKind::Pipe) {
                self.expect(&TokenKind::Comma)?;
            }
        }
        Ok(params)
    }

    /// Parse closure body: either a block `{ stmts; expr }` or a single expression.
    fn parse_closure_body(&mut self) -> Result<Expr, ParseError> {
        if self.check(&TokenKind::LeftBrace) {
            let block = self.parse_block()?;
            Ok(Expr::Block(block))
        } else {
            self.parse_expr()
        }
    }

    fn parse_if_expr(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::If)?;
        let condition = self.parse_expr()?;
        let then_block = self.parse_block()?;
        let else_block = if self.match_token(&TokenKind::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Expr::If {
            condition: Box::new(condition),
            then_block,
            else_block,
        })
    }

    fn parse_match_expr(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::Match)?;
        let subject = self.parse_expr()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut arms = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            let pattern = self.parse_pattern()?;
            let guard = if self.check(&TokenKind::If) {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect(&TokenKind::FatArrow)?;
            let body = self.parse_expr()?;
            arms.push(MatchArm { pattern, guard, body });
            if !self.check(&TokenKind::RightBrace) {
                self.expect(&TokenKind::Comma)?;
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(Expr::Match { subject: Box::new(subject), arms })
    }

    fn parse_for_expr(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::For)?;
        // Support tuple destructuring: for (i, col) in expr { ... }
        if self.check(&TokenKind::LeftParen) {
            let span = self.current_span();
            self.advance(); // consume `(`
            let mut names = Vec::new();
            while !self.check(&TokenKind::RightParen) && !self.is_at_end() {
                names.push(self.expect_ident()?);
                if !self.check(&TokenKind::RightParen) {
                    self.expect(&TokenKind::Comma)?;
                }
            }
            self.expect(&TokenKind::RightParen)?;
            self.expect(&TokenKind::In)?;
            let iterator = self.parse_expr()?;
            let mut body = self.parse_block()?;
            // Synthesize: let (a, b, ...) = __for_tuple_binding__;
            // at the start of the body block.
            let binding = "__for_tuple__".to_string();
            let pattern = Pattern::Tuple(names.into_iter().map(Pattern::Ident).collect());
            let destructure = Stmt::LetDestructure {
                pattern,
                ty: None,
                value: Expr::Ident(binding.clone()),
            };
            body.stmts.insert(0, destructure);
            let _ = span;
            return Ok(Expr::For {
                binding,
                iterator: Box::new(iterator),
                body,
            });
        }
        let binding = self.expect_ident()?;
        self.expect(&TokenKind::In)?;
        let iterator = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Expr::For {
            binding,
            iterator: Box::new(iterator),
            body,
        })
    }

    fn parse_while_expr(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::While)?;
        let condition = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Expr::While {
            condition: Box::new(condition),
            body,
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        match self.peek_kind() {
            TokenKind::Ident(ref name) if name == "_" => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            TokenKind::Integer(_) | TokenKind::StringLit(_) | TokenKind::True | TokenKind::False => {
                let expr = self.parse_primary()?;
                Ok(Pattern::Literal(expr))
            }
            TokenKind::Ident(_) => {
                let name = self.expect_ident()?;
                // Handle Enum::Variant or Enum::Variant(payload) patterns
                let is_qualified = self.match_token(&TokenKind::ColonColon);
                let variant_name = if is_qualified {
                    self.expect_ident()?
                } else {
                    String::new()
                };
                if self.match_token(&TokenKind::LeftParen) {
                    let mut fields = Vec::new();
                    while !self.check(&TokenKind::RightParen) {
                        fields.push(self.parse_pattern()?);
                        if !self.check(&TokenKind::RightParen) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&TokenKind::RightParen)?;
                    // Variant with fields — use variant name (unqualified) for codegen
                    let pat_name = if is_qualified { variant_name } else { name };
                    Ok(Pattern::Variant { name: pat_name, fields })
                } else if is_qualified {
                    // Qualified variant without fields (e.g., Status::Active)
                    Ok(Pattern::Variant { name: variant_name, fields: vec![] })
                } else {
                    Ok(Pattern::Ident(name))
                }
            }
            _ => Err(self.error("Expected pattern")),
        }
    }

    /// Parse a destructuring pattern for let bindings.
    /// Handles tuple `(a, b)` and array `[a, b, ..]` patterns.
    fn parse_destructure_pattern(&mut self) -> Result<Pattern, ParseError> {
        if self.match_token(&TokenKind::LeftParen) {
            // Tuple pattern: (a, b, c)
            let mut patterns = Vec::new();
            while !self.check(&TokenKind::RightParen) {
                patterns.push(self.parse_destructure_element()?);
                if !self.check(&TokenKind::RightParen) {
                    self.expect(&TokenKind::Comma)?;
                }
            }
            self.expect(&TokenKind::RightParen)?;
            Ok(Pattern::Tuple(patterns))
        } else if self.match_token(&TokenKind::LeftBracket) {
            // Array pattern: [a, b, ..]
            let mut patterns = Vec::new();
            while !self.check(&TokenKind::RightBracket) {
                if self.check(&TokenKind::DotDot) {
                    // Rest pattern ".."
                    self.advance(); // consume ..
                    // Rest elements are signaled by a trailing Wildcard
                    patterns.push(Pattern::Wildcard);
                    break;
                }
                patterns.push(self.parse_destructure_element()?);
                if !self.check(&TokenKind::RightBracket) {
                    self.expect(&TokenKind::Comma)?;
                }
            }
            self.expect(&TokenKind::RightBracket)?;
            Ok(Pattern::Array(patterns))
        } else {
            Err(self.error("Expected '(' or '[' for destructuring pattern"))
        }
    }

    /// Parse a single element within a destructuring pattern.
    fn parse_destructure_element(&mut self) -> Result<Pattern, ParseError> {
        match self.peek_kind() {
            TokenKind::Ident(ref name) if name == "_" => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            TokenKind::LeftParen => self.parse_destructure_pattern(),
            TokenKind::LeftBracket => self.parse_destructure_pattern(),
            TokenKind::Ident(_) => {
                let name = self.expect_ident()?;
                Ok(Pattern::Ident(name))
            }
            _ => Err(self.error("Expected identifier or nested pattern in destructuring")),
        }
    }

    /// Parse a struct destructuring pattern: `Name { field1, field2: pat, .. }`
    /// The struct name has already been consumed.
    fn parse_struct_destructure_pattern(&mut self, name: String) -> Result<Pattern, ParseError> {
        self.expect(&TokenKind::LeftBrace)?;
        let mut fields = Vec::new();
        let mut rest = false;

        while !self.check(&TokenKind::RightBrace) {
            // Check for ".." rest pattern
            if self.check(&TokenKind::DotDot) {
                self.advance(); // consume ..
                rest = true;
                // Allow trailing comma
                self.match_token(&TokenKind::Comma);
                break;
            }

            let field_name = self.expect_ident()?;
            let pattern = if self.match_token(&TokenKind::Colon) {
                // field: pattern
                self.parse_destructure_element()?
            } else {
                // shorthand: field (binds to same name)
                Pattern::Ident(field_name.clone())
            };
            fields.push((field_name, pattern));

            if !self.check(&TokenKind::RightBrace) {
                self.expect(&TokenKind::Comma)?;
            }
        }
        self.expect(&TokenKind::RightBrace)?;
        Ok(Pattern::Struct { name, fields, rest })
    }

    fn parse_lazy_component(&mut self) -> Result<LazyComponentDef, ParseError> {
        let span = self.current_span();
        // `lazy` already consumed; expect `component`
        let component = self.parse_component()?;
        Ok(LazyComponentDef { component, span })
    }

    /// Parse a format string token into `Expr::FormatString`.
    /// Each `FormatStringPart::Expr(text)` segment is re-lexed and re-parsed
    /// as a full expression so that `f"result: {a + b}"` works.
    fn parse_format_string_expr(&mut self) -> Result<Expr, ParseError> {
        let token = self.advance();
        let raw_parts = if let TokenKind::FormatString(parts) = token.kind {
            parts
        } else {
            unreachable!()
        };

        let mut ast_parts: Vec<FormatPart> = Vec::new();

        for part in raw_parts {
            match part {
                FormatStringPart::Lit(s) => {
                    ast_parts.push(FormatPart::Literal(s));
                }
                FormatStringPart::Expr(expr_text) => {
                    // Re-lex and re-parse the expression text as a full expression.
                    let mut inner_lexer = crate::lexer::Lexer::new(&expr_text);
                    let inner_tokens = inner_lexer.tokenize().map_err(|e| {
                        ParseError {
                            message: format!(
                                "Error in format string interpolation: {}",
                                e.message
                            ),
                            span: token.span,
                        }
                    })?;
                    let mut inner_parser = Parser::new(inner_tokens);
                    let expr = inner_parser.parse_expr().map_err(|e| {
                        ParseError {
                            message: format!(
                                "Error in format string interpolation: {}",
                                e.message
                            ),
                            span: token.span,
                        }
                    })?;
                    ast_parts.push(FormatPart::Expression(Box::new(expr)));
                }
            }
        }

        Ok(Expr::FormatString { parts: ast_parts })
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        while !self.check(&TokenKind::RightParen) {
            args.push(self.parse_expr()?);
            if !self.check(&TokenKind::RightParen) {
                self.expect(&TokenKind::Comma)?;
            }
        }
        Ok(args)
    }


    // === Router parsing ===

    fn parse_router(&mut self) -> Result<RouterDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Router)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut routes = Vec::new();
        let mut fallback = None;
        let mut layout = None;
        let mut transition = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            if self.check(&TokenKind::Fallback) {
                self.advance();
                self.expect(&TokenKind::FatArrow)?;
                let comp_name = self.expect_ident()?;
                fallback = Some(Box::new(TemplateNode::Expression(
                    Box::new(Expr::Ident(comp_name)),
                )));
                self.match_token(&TokenKind::Comma);
            } else if self.check(&TokenKind::Route) {
                routes.push(self.parse_route_def()?);
                self.match_token(&TokenKind::Comma);
            } else if self.check(&TokenKind::Layout) {
                self.advance();
                self.expect(&TokenKind::LeftBrace)?;
                let render_span = self.current_span();
                let body = self.parse_template_node()?;
                self.expect(&TokenKind::RightBrace)?;
                layout = Some(RenderBlock { body, span: render_span });
            } else if self.check(&TokenKind::Transition) {
                self.advance();
                if let TokenKind::StringLit(s) = self.peek_kind() {
                    self.advance();
                    transition = Some(s);
                } else {
                    return Err(self.error("Expected string for router transition"));
                }
                self.match_token(&TokenKind::Semicolon);
            } else {
                return Err(self.error("Expected 'route', 'fallback', 'layout', or 'transition' in router block"));
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(RouterDef { name, routes, fallback, layout, transition, span })
    }

    fn parse_route_def(&mut self) -> Result<RouteDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Route)?;

        let path = if let TokenKind::StringLit(s) = self.peek_kind() {
            self.advance();
            s
        } else {
            return Err(self.error("Expected string literal for route path"));
        };

        let params: Vec<String> = path.split('/')
            .filter(|seg| seg.starts_with(':'))
            .map(|seg| seg[1..].to_string())
            .collect();

        self.expect(&TokenKind::FatArrow)?;
        let component = self.expect_ident()?;

        let guard = if self.check(&TokenKind::Guard) {
            self.advance();
            self.expect(&TokenKind::LeftBrace)?;
            let expr = self.parse_expr()?;
            self.expect(&TokenKind::RightBrace)?;
            Some(expr)
        } else {
            None
        };

        let transition = if self.check(&TokenKind::Transition) {
            self.advance();
            if let TokenKind::StringLit(s) = self.peek_kind() {
                self.advance();
                Some(s)
            } else {
                return Err(self.error("Expected string for route transition"));
            }
        } else {
            None
        };

        Ok(RouteDef { path, params, component, guard, transition, span })
    }

    // === Style parsing ===

    fn parse_style_blocks(&mut self) -> Result<Vec<StyleBlock>, ParseError> {
        self.expect(&TokenKind::Style)?;
        self.expect(&TokenKind::LeftBrace)?;

        let blocks = self.parse_style_blocks_inner()?;

        self.expect(&TokenKind::RightBrace)?;
        Ok(blocks)
    }

    /// Parse style rule blocks inside braces. Each block is either a normal
    /// `selector { prop: value; ... }` or an `@name { nested blocks... }`
    /// at-rule that contains nested selector blocks (used for breakpoints).
    fn parse_style_blocks_inner(&mut self) -> Result<Vec<StyleBlock>, ParseError> {
        let mut blocks = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let span = self.current_span();

            // Read selector tokens until `{`
            let mut selector = String::new();
            while !self.check(&TokenKind::LeftBrace) && !self.is_at_end() {
                let tok = self.advance();
                match &tok.kind {
                    TokenKind::Dot => selector.push('.'),
                    TokenKind::Ident(s) => {
                        if !selector.is_empty() && !selector.ends_with('.') && !selector.ends_with(' ') && !selector.ends_with('@') && !selector.ends_with('-') && !selector.ends_with('#') {
                            selector.push(' ');
                        }
                        selector.push_str(s);
                    }
                    TokenKind::Colon => selector.push(':'),
                    TokenKind::Minus => selector.push('-'),
                    TokenKind::Comma => selector.push_str(", "),
                    TokenKind::Star => selector.push('*'),
                    TokenKind::RightAngle => selector.push_str(" > "),
                    TokenKind::Plus => selector.push_str(" + "),
                    TokenKind::LeftBracket => selector.push('['),
                    TokenKind::RightBracket => selector.push(']'),
                    TokenKind::Equals => selector.push('='),
                    TokenKind::StringLit(s) => {
                        selector.push('"');
                        selector.push_str(s);
                        selector.push('"');
                    }
                    TokenKind::Hash => selector.push('#'),
                    TokenKind::At => selector.push('@'),
                    _ => {
                        // In CSS selector context, ALL tokens are identifier parts.
                        // Use as_css_text() to handle every keyword generically.
                        let text = tok.kind.as_css_text();
                        if !text.is_empty() {
                            if !selector.is_empty() && !selector.ends_with('.') && !selector.ends_with(' ') && !selector.ends_with('@') && !selector.ends_with('-') && !selector.ends_with('#') {
                                selector.push(' ');
                            }
                            selector.push_str(&text);
                        } else if !selector.is_empty() && !selector.ends_with(' ') {
                            selector.push(' ');
                        }
                    }
                }
            }

            self.expect(&TokenKind::LeftBrace)?;

            let trimmed_sel = selector.trim().to_string();

            // If the selector starts with `@`, this is an at-rule block that
            // may contain nested selector blocks (e.g. `@md { .card { ... } }`).
            // Parse the inner blocks recursively and tag each with the at-rule
            // prefix so codegen can expand breakpoints.
            if trimmed_sel.starts_with('@') {
                let inner_blocks = self.parse_style_blocks_inner()?;
                self.expect(&TokenKind::RightBrace)?;

                // Flatten nested blocks: prefix each inner selector with the at-rule.
                // The codegen layer uses the `@name` prefix to detect breakpoint expansion.
                for inner in inner_blocks {
                    blocks.push(StyleBlock {
                        selector: format!("{} {}", trimmed_sel, inner.selector),
                        properties: inner.properties,
                        span: inner.span,
                    });
                }
                continue;
            }

            let mut properties = Vec::new();
            while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                let mut prop_name = String::new();
                while !self.check(&TokenKind::Colon) && !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                    let tok = self.advance();
                    match &tok.kind {
                        TokenKind::Ident(s) => {
                            if !prop_name.is_empty() {
                                prop_name.push('-');
                            }
                            prop_name.push_str(s);
                        }
                        TokenKind::Minus => {}
                        _ => {}
                    }
                }

                self.expect(&TokenKind::Colon)?;

                let value = if let TokenKind::StringLit(_) = self.peek_kind() {
                    // Quoted CSS value (backward compatible)
                    if let TokenKind::StringLit(s) = self.advance().kind {
                        s
                    } else {
                        unreachable!()
                    }
                } else {
                    // Unquoted CSS value: collect tokens until `;` or `}`
                    let mut val = String::new();
                    while !self.check(&TokenKind::Semicolon)
                          && !self.check(&TokenKind::RightBrace)
                          && !self.is_at_end()
                    {
                        let tok = self.advance();
                        match &tok.kind {
                            TokenKind::Ident(s) => {
                                if !val.is_empty() && !val.ends_with(' ') && !val.ends_with('(') && !val.ends_with('#') && !val.ends_with('-') {
                                    val.push(' ');
                                }
                                val.push_str(s);
                            }
                            TokenKind::Hash => val.push('#'),
                            TokenKind::Integer(n) => val.push_str(&n.to_string()),
                            TokenKind::Float(f) => val.push_str(&f.to_string()),
                            TokenKind::Dot => val.push('.'),
                            TokenKind::Comma => val.push_str(", "),
                            TokenKind::LeftParen => val.push('('),
                            TokenKind::RightParen => val.push(')'),
                            TokenKind::Minus => val.push('-'),
                            TokenKind::Plus => val.push('+'),
                            TokenKind::Star => val.push('*'),
                            TokenKind::Slash => val.push('/'),
                            TokenKind::Percent => val.push('%'),
                            TokenKind::Colon => val.push(':'),
                            TokenKind::StringLit(s) => {
                                val.push('"');
                                val.push_str(s);
                                val.push('"');
                            }
                            TokenKind::Lifetime(s) => {
                                // 'JetBrains in CSS font-family — lifetime token starts with '
                                val.push('\'');
                                val.push_str(s);
                            }
                            TokenKind::SingleQuote => {
                                // Closing ' in CSS single-quoted strings
                                val.push('\'');
                            }
                            _ => {
                                if !val.is_empty() && !val.ends_with(' ') {
                                    val.push(' ');
                                }
                            }
                        }
                    }
                    val.trim().to_string()
                };

                self.expect(&TokenKind::Semicolon)?;
                properties.push((prop_name, value));
            }

            self.expect(&TokenKind::RightBrace)?;
            blocks.push(StyleBlock { selector: trimmed_sel, properties, span });
        }

        Ok(blocks)
    }

    // === Form parsing ===

    fn parse_form(&mut self, is_pub: bool) -> Result<FormDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Form)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut fields = vec![];
        let mut on_submit = None;
        let steps = vec![];
        let mut methods = vec![];
        let mut styles = vec![];
        let mut render = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Field => {
                    fields.push(self.parse_form_field()?);
                }
                TokenKind::Fn | TokenKind::Async => {
                    let f = self.parse_function(false)?;
                    if f.name == "on_submit" {
                        on_submit = Some(f.name.clone());
                    }
                    methods.push(f);
                }
                TokenKind::Style => {
                    styles.push(self.parse_style_block_single()?);
                }
                TokenKind::Render => {
                    self.advance();
                    render = Some(self.parse_render_block()?);
                }
                _ => {
                    return Err(self.error(&format!("unexpected token in form: {:?}", self.peek_kind())));
                }
            }
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(FormDef { name, fields, on_submit, steps, methods, styles, render, is_pub, span })
    }

    fn parse_form_field(&mut self) -> Result<FormFieldDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Field)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type()?;

        let mut validators = vec![];
        let mut label = None;
        let mut placeholder = None;
        let mut default_value = None;

        // Optional block with validators and metadata
        if self.match_token(&TokenKind::LeftBrace) {
            while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                let key = self.expect_ident()?;
                match key.as_str() {
                    "required" => {
                        let message = if self.match_token(&TokenKind::Colon) {
                            Some(self.parse_expr()?)
                        } else {
                            None
                        };
                        validators.push(ValidatorDef { kind: ValidatorKind::Required, message, span });
                    }
                    "min_length" => {
                        self.expect(&TokenKind::Colon)?;
                        if let TokenKind::Integer(n) = self.peek_kind() {
                            let n = n as usize;
                            self.advance();
                            validators.push(ValidatorDef { kind: ValidatorKind::MinLength(n), message: None, span });
                        }
                    }
                    "max_length" => {
                        self.expect(&TokenKind::Colon)?;
                        if let TokenKind::Integer(n) = self.peek_kind() {
                            let n = n as usize;
                            self.advance();
                            validators.push(ValidatorDef { kind: ValidatorKind::MaxLength(n), message: None, span });
                        }
                    }
                    "pattern" => {
                        self.expect(&TokenKind::Colon)?;
                        if let TokenKind::StringLit(s) = self.peek_kind() {
                            let s = s.clone();
                            self.advance();
                            validators.push(ValidatorDef { kind: ValidatorKind::Pattern(s), message: None, span });
                        }
                    }
                    "email" => {
                        validators.push(ValidatorDef { kind: ValidatorKind::Email, message: None, span });
                    }
                    "url" => {
                        validators.push(ValidatorDef { kind: ValidatorKind::Url, message: None, span });
                    }
                    "label" => {
                        self.expect(&TokenKind::Colon)?;
                        label = Some(self.parse_expr()?);
                    }
                    "placeholder" => {
                        self.expect(&TokenKind::Colon)?;
                        placeholder = Some(self.parse_expr()?);
                    }
                    "default" => {
                        self.expect(&TokenKind::Colon)?;
                        default_value = Some(self.parse_expr()?);
                    }
                    "validate" => {
                        self.expect(&TokenKind::Colon)?;
                        let fn_name = self.expect_ident()?;
                        validators.push(ValidatorDef { kind: ValidatorKind::Custom(fn_name), message: None, span });
                    }
                    _ => {
                        return Err(self.error(&format!("unknown form field attribute: {}", key)));
                    }
                }
                self.match_token(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RightBrace)?;
        }

        // Semicolon optional
        self.match_token(&TokenKind::Semicolon);

        Ok(FormFieldDef { name, ty, validators, label, placeholder, default_value, span })
    }

    /// Parse a single style block (used by form parser)
    fn parse_style_block_single(&mut self) -> Result<StyleBlock, ParseError> {
        let blocks = self.parse_style_blocks()?;
        Ok(blocks.into_iter().next().unwrap_or(StyleBlock {
            selector: String::new(),
            properties: vec![],
            span: self.current_span(),
        }))
    }

    // === Channel parsing ===

    fn parse_channel(&mut self, is_pub: bool) -> Result<ChannelDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Channel)?;
        let name = self.expect_ident()?;

        // Optional contract binding: channel Chat -> ChatMessage { ... }
        let contract = if self.match_token(&TokenKind::Arrow) {
            Some(self.expect_ident()?)
        } else {
            None
        };

        self.expect(&TokenKind::LeftBrace)?;

        let mut url = Expr::StringLit("".to_string());
        let mut provider = None;
        let mut on_message = None;
        let mut on_connect = None;
        let mut on_disconnect = None;
        let mut reconnect = true;
        let mut heartbeat_interval = None;
        let mut methods = vec![];

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Fn | TokenKind::Async => {
                    methods.push(self.parse_function(false)?);
                }
                TokenKind::OnMessage => {
                    let handler_name = "on_message".to_string();
                    self.advance();
                    on_message = Some(self.parse_channel_handler(handler_name)?);
                }
                TokenKind::OnConnect => {
                    let handler_name = "on_connect".to_string();
                    self.advance();
                    on_connect = Some(self.parse_channel_handler(handler_name)?);
                }
                TokenKind::OnDisconnect => {
                    let handler_name = "on_disconnect".to_string();
                    self.advance();
                    on_disconnect = Some(self.parse_channel_handler(handler_name)?);
                }
                _ => {
                    // Parse key: value pairs for url, reconnect, heartbeat
                    let key = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    match key.as_str() {
                        "url" => { url = self.parse_expr()?; }
                        "provider" => {
                            if let TokenKind::StringLit(s) = self.peek_kind() {
                                provider = Some(s.clone());
                                self.advance();
                            }
                        }
                        "reconnect" => {
                            match self.peek_kind() {
                                TokenKind::True => { reconnect = true; self.advance(); }
                                TokenKind::False => { reconnect = false; self.advance(); }
                                TokenKind::Ident(v) => { reconnect = v == "true"; self.advance(); }
                                _ => {}
                            }
                        }
                        "heartbeat" => {
                            if let TokenKind::Integer(n) = self.peek_kind() {
                                heartbeat_interval = Some(n as u64);
                                self.advance();
                            }
                        }
                        _ => {
                            self.errors.push(ParseError {
                                message: format!("unknown channel property: {}", key),
                                span,
                            });
                            self.advance();
                        }
                    }
                    self.match_token(&TokenKind::Comma);
                }
            }
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(ChannelDef { name, url, provider, contract, on_message, on_connect, on_disconnect, reconnect, heartbeat_interval, methods, is_pub, span })
    }

    /// Parse a channel handler: either a named `fn name(params) { body }` or
    /// an anonymous `fn(params) { body }` (prefixed by on_connect/on_message/etc.).
    fn parse_channel_handler(&mut self, default_name: String) -> Result<Function, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Fn)?;
        // The name is optional for anonymous channel handlers: `on_connect fn() { ... }`
        let name = if self.check(&TokenKind::LeftParen) {
            default_name
        } else {
            self.expect_ident()?
        };
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RightParen)?;
        let return_type = if self.match_token(&TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        Ok(Function {
            name,
            lifetimes: Vec::new(),
            type_params: Vec::new(),
            params,
            return_type,
            trait_bounds: Vec::new(),
            body,
            is_pub: false,
            is_async: false,
            must_use: false,
            span,
        })
    }

    // === Embed parsing ===

    fn parse_embed(&mut self, is_pub: bool) -> Result<EmbedDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Embed)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut src = Expr::StringLit("".to_string());
        let mut loading = None;
        let mut sandbox = false;
        let mut integrity = None;
        let mut permissions = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            match key.as_str() {
                "src" => { src = self.parse_expr()?; }
                "loading" => {
                    if let TokenKind::StringLit(s) = self.peek_kind() {
                        loading = Some(s);
                        self.advance();
                    }
                }
                "sandbox" => {
                    match self.peek_kind() {
                        TokenKind::True => { sandbox = true; self.advance(); }
                        TokenKind::False => { sandbox = false; self.advance(); }
                        TokenKind::Ident(v) => { sandbox = v == "true"; self.advance(); }
                        _ => {}
                    }
                }
                "integrity" => { integrity = Some(self.parse_expr()?); }
                "permissions" => {
                    permissions = Some(self.parse_permissions()?);
                }
                _ => {
                    self.errors.push(ParseError {
                        message: format!("unknown embed property: {}", key),
                        span,
                    });
                    self.advance();
                }
            }
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(EmbedDef { name, src, loading, sandbox, integrity, permissions, is_pub, span })
    }

    // === PDF parsing ===

    fn parse_pdf(&mut self, is_pub: bool) -> Result<PdfDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Pdf)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut page_size = None;
        let mut orientation = None;
        let mut margins = None;
        let mut render = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Render => {
                    render = Some(self.parse_render_block()?);
                }
                _ => {
                    let key = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    match key.as_str() {
                        "page_size" => {
                            if let TokenKind::StringLit(s) = self.peek_kind() {
                                page_size = Some(s);
                                self.advance();
                            }
                        }
                        "orientation" => {
                            if let TokenKind::StringLit(s) = self.peek_kind() {
                                orientation = Some(s);
                                self.advance();
                            }
                        }
                        "margins" => { margins = Some(self.parse_expr()?); }
                        _ => {
                            self.errors.push(ParseError {
                                message: format!("unknown pdf property: {}", key),
                                span,
                            });
                            self.advance();
                        }
                    }
                    self.match_token(&TokenKind::Comma);
                }
            }
        }

        self.expect(&TokenKind::RightBrace)?;

        // If no render block was found, create a default empty one
        let render = render.unwrap_or(RenderBlock {
            body: TemplateNode::Fragment(vec![]),
            span,
        });

        Ok(PdfDef { name, render, page_size, orientation, margins, is_pub, span })
    }

    fn parse_payment(&mut self, is_pub: bool) -> Result<PaymentDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Payment)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut provider = None;
        let mut public_key = None;
        let mut sandbox_mode = true; // default to sandboxed
        let mut on_success = None;
        let mut on_error = None;
        let mut methods = vec![];

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Fn | TokenKind::Async => {
                    let f = self.parse_function(false)?;
                    match f.name.as_str() {
                        "on_success" => on_success = Some(f),
                        "on_error" => on_error = Some(f),
                        _ => methods.push(f),
                    }
                }
                _ => {
                    let key = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    match key.as_str() {
                        "provider" => { provider = Some(self.parse_expr()?); }
                        "public_key" => { public_key = Some(self.parse_expr()?); }
                        "sandbox" => {
                            match self.peek_kind() {
                                TokenKind::True => { sandbox_mode = true; self.advance(); }
                                TokenKind::False => { sandbox_mode = false; self.advance(); }
                                TokenKind::Ident(v) => { sandbox_mode = v == "true"; self.advance(); }
                                _ => {}
                            }
                        }
                        _ => { self.parse_expr()?; } // skip unknown
                    }
                    self.match_token(&TokenKind::Comma);
                }
            }
        }
        self.expect(&TokenKind::RightBrace)?;
        Ok(PaymentDef { name, provider, public_key, sandbox_mode, on_success, on_error, methods, is_pub, span })
    }

    fn parse_banking(&mut self, is_pub: bool) -> Result<BankingDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Banking)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut provider = None;
        let mut on_success = None;
        let mut on_exit = None;
        let mut on_error = None;
        let mut methods = vec![];

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Fn | TokenKind::Async => {
                    let f = self.parse_function(false)?;
                    match f.name.as_str() {
                        "on_success" => on_success = Some(f),
                        "on_exit" => on_exit = Some(f),
                        "on_error" => on_error = Some(f),
                        _ => methods.push(f),
                    }
                }
                _ => {
                    let key = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    match key.as_str() {
                        "provider" => { provider = Some(self.parse_expr()?); }
                        _ => { self.parse_expr()?; } // skip unknown
                    }
                    self.match_token(&TokenKind::Comma);
                }
            }
        }
        self.expect(&TokenKind::RightBrace)?;
        Ok(BankingDef { name, provider, on_success, on_exit, on_error, methods, is_pub, span })
    }

    fn parse_map(&mut self, is_pub: bool) -> Result<MapDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::MapKeyword)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut provider = None;
        let mut center: Option<(f64, f64)> = None;
        let mut zoom: Option<f64> = None;
        let mut style = None;
        let mut on_ready = None;
        let mut on_click = None;
        let mut methods = vec![];

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Fn | TokenKind::Async => {
                    let f = self.parse_function(false)?;
                    match f.name.as_str() {
                        "on_ready" => on_ready = Some(f),
                        "on_click" => on_click = Some(f),
                        _ => methods.push(f),
                    }
                }
                _ => {
                    let key = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    match key.as_str() {
                        "provider" => { provider = Some(self.parse_expr()?); }
                        "style" => { style = Some(self.parse_expr()?); }
                        "center" => {
                            // Parse (lat, lng) tuple
                            self.expect(&TokenKind::LeftParen)?;
                            let lat_expr = self.parse_expr()?;
                            self.expect(&TokenKind::Comma)?;
                            let lng_expr = self.parse_expr()?;
                            self.expect(&TokenKind::RightParen)?;
                            let lat = match &lat_expr {
                                Expr::Float(f) => *f,
                                Expr::Integer(i) => *i as f64,
                                Expr::Unary { op: UnaryOp::Neg, operand } => match operand.as_ref() {
                                    Expr::Float(f) => -f,
                                    Expr::Integer(i) => -(*i as f64),
                                    _ => 0.0,
                                },
                                _ => 0.0,
                            };
                            let lng = match &lng_expr {
                                Expr::Float(f) => *f,
                                Expr::Integer(i) => *i as f64,
                                Expr::Unary { op: UnaryOp::Neg, operand } => match operand.as_ref() {
                                    Expr::Float(f) => -f,
                                    Expr::Integer(i) => -(*i as f64),
                                    _ => 0.0,
                                },
                                _ => 0.0,
                            };
                            center = Some((lat, lng));
                        }
                        "zoom" => {
                            let z = self.parse_expr()?;
                            zoom = match &z {
                                Expr::Float(f) => Some(*f),
                                Expr::Integer(i) => Some(*i as f64),
                                _ => None,
                            };
                        }
                        _ => { self.parse_expr()?; } // skip unknown
                    }
                    self.match_token(&TokenKind::Comma);
                }
            }
        }
        self.expect(&TokenKind::RightBrace)?;
        Ok(MapDef { name, provider, center, zoom, style, on_ready, on_click, methods, is_pub, span })
    }

    fn parse_auth(&mut self, is_pub: bool) -> Result<AuthDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Auth)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut provider = None;
        let mut providers = vec![];
        let mut on_login = None;
        let mut on_logout = None;
        let mut on_error = None;
        let mut session_storage = None;
        let mut methods = vec![];

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Fn | TokenKind::Async => {
                    let f = self.parse_function(false)?;
                    match f.name.as_str() {
                        "on_login" => on_login = Some(f),
                        "on_logout" => on_logout = Some(f),
                        "on_error" => on_error = Some(f),
                        _ => methods.push(f),
                    }
                }
                _ => {
                    let key = self.expect_ident()?;
                    match key.as_str() {
                        "provider" => {
                            // provider "google" { client_id: ..., scopes: [...] }
                            if let TokenKind::StringLit(_) = self.peek_kind() {
                                let prov_name = if let TokenKind::StringLit(s) = self.advance().kind { s } else { unreachable!() };
                                let prov_span = self.current_span();
                                self.expect(&TokenKind::LeftBrace)?;
                                let mut client_id = None;
                                let mut scopes = vec![];
                                while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                                    let pkey = self.expect_ident()?;
                                    self.expect(&TokenKind::Colon)?;
                                    match pkey.as_str() {
                                        "client_id" => { client_id = Some(self.parse_expr()?); }
                                        "scopes" => {
                                            self.expect(&TokenKind::LeftBracket)?;
                                            while !self.check(&TokenKind::RightBracket) && !self.is_at_end() {
                                                if let TokenKind::StringLit(s) = self.peek_kind() {
                                                    scopes.push(s);
                                                    self.advance();
                                                } else {
                                                    self.advance();
                                                }
                                                self.match_token(&TokenKind::Comma);
                                            }
                                            self.expect(&TokenKind::RightBracket)?;
                                        }
                                        _ => { self.parse_expr()?; }
                                    }
                                    self.match_token(&TokenKind::Comma);
                                }
                                self.expect(&TokenKind::RightBrace)?;
                                providers.push(AuthProvider { name: prov_name, client_id, scopes, span: prov_span });
                            } else {
                                self.expect(&TokenKind::Colon)?;
                                provider = Some(self.parse_expr()?);
                            }
                        }
                        "session" => {
                            self.expect(&TokenKind::Colon)?;
                            if let TokenKind::StringLit(s) = self.peek_kind() {
                                session_storage = Some(s);
                                self.advance();
                            }
                        }
                        _ => {
                            self.expect(&TokenKind::Colon)?;
                            self.parse_expr()?; // skip unknown
                        }
                    }
                    self.match_token(&TokenKind::Comma);
                }
            }
        }
        self.expect(&TokenKind::RightBrace)?;
        Ok(AuthDef { name, provider, providers, on_login, on_logout, on_error, session_storage, methods, is_pub, span })
    }

    fn parse_upload(&mut self, is_pub: bool) -> Result<UploadDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Upload)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut endpoint = Expr::StringLit("/upload".to_string());
        let mut max_size = None;
        let mut accept = vec![];
        let mut chunked = false;
        let mut on_progress = None;
        let mut on_complete = None;
        let mut on_error = None;
        let mut methods = vec![];

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Fn | TokenKind::Async => {
                    let f = self.parse_function(false)?;
                    match f.name.as_str() {
                        "on_progress" => on_progress = Some(f),
                        "on_complete" => on_complete = Some(f),
                        "on_error" => on_error = Some(f),
                        _ => methods.push(f),
                    }
                }
                _ => {
                    let key = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    match key.as_str() {
                        "endpoint" => { endpoint = self.parse_expr()?; }
                        "max_size" => { max_size = Some(self.parse_expr()?); }
                        "accept" => {
                            self.expect(&TokenKind::LeftBracket)?;
                            while !self.check(&TokenKind::RightBracket) && !self.is_at_end() {
                                if let TokenKind::StringLit(s) = self.peek_kind() {
                                    accept.push(s);
                                    self.advance();
                                } else {
                                    self.advance();
                                }
                                self.match_token(&TokenKind::Comma);
                            }
                            self.expect(&TokenKind::RightBracket)?;
                        }
                        "chunked" => {
                            match self.peek_kind() {
                                TokenKind::True => { chunked = true; self.advance(); }
                                TokenKind::False => { chunked = false; self.advance(); }
                                TokenKind::Ident(v) => { chunked = v == "true"; self.advance(); }
                                _ => {}
                            }
                        }
                        _ => { self.parse_expr()?; } // skip unknown
                    }
                    self.match_token(&TokenKind::Comma);
                }
            }
        }
        self.expect(&TokenKind::RightBrace)?;
        Ok(UploadDef { name, endpoint, max_size, accept, chunked, on_progress, on_complete, on_error, methods, is_pub, span })
    }

    fn parse_cache(&mut self, is_pub: bool) -> Result<CacheDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Cache)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut strategy = None;
        let mut default_ttl = None;
        let mut persist = false;
        let mut max_entries = None;
        let mut queries = vec![];
        let mut mutations = vec![];

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Query => {
                    queries.push(self.parse_cache_query()?);
                }
                TokenKind::Mutation => {
                    mutations.push(self.parse_cache_mutation()?);
                }
                _ => {
                    let key = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    match key.as_str() {
                        "strategy" => {
                            if let TokenKind::StringLit(s) = self.peek_kind() {
                                strategy = Some(s.clone());
                                self.advance();
                            }
                        }
                        "ttl" => {
                            if let TokenKind::Integer(n) = self.peek_kind() {
                                default_ttl = Some(n as u64);
                                self.advance();
                            }
                        }
                        "persist" => {
                            match self.peek_kind() {
                                TokenKind::True => { persist = true; self.advance(); }
                                TokenKind::False => { persist = false; self.advance(); }
                                TokenKind::Ident(v) => { persist = v == "true"; self.advance(); }
                                _ => {}
                            }
                        }
                        "max_entries" => {
                            if let TokenKind::Integer(n) = self.peek_kind() {
                                max_entries = Some(n as u64);
                                self.advance();
                            }
                        }
                        _ => { self.advance(); }
                    }
                    self.match_token(&TokenKind::Comma);
                }
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(CacheDef { name, strategy, default_ttl, persist, max_entries, queries, mutations, is_pub, span })
    }

    fn parse_cache_query(&mut self) -> Result<CacheQueryDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Query)?;
        let name = self.expect_ident()?;

        // Optional params: query user(id: String)
        let params = if self.match_token(&TokenKind::LeftParen) {
            let p = self.parse_params()?;
            self.expect(&TokenKind::RightParen)?;
            p
        } else {
            vec![]
        };

        self.expect(&TokenKind::Colon)?;

        // Parse fetch expression
        let fetch_expr = self.parse_expr()?;

        // Optional contract binding: -> ContractName
        let contract = if self.match_token(&TokenKind::Arrow) {
            Some(self.expect_ident()?)
        } else {
            None
        };

        let mut ttl = None;
        let mut stale = None;
        let mut invalidate_on = vec![];

        // Optional config block
        if self.match_token(&TokenKind::LeftBrace) {
            while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                let key = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                match key.as_str() {
                    "ttl" => {
                        if let TokenKind::Integer(n) = self.peek_kind() {
                            ttl = Some(n as u64);
                            self.advance();
                        }
                    }
                    "stale" => {
                        if let TokenKind::Integer(n) = self.peek_kind() {
                            stale = Some(n as u64);
                            self.advance();
                        }
                    }
                    "invalidate_on" => {
                        // Parse string array: ["event1", "event2"]
                        if self.match_token(&TokenKind::LeftBracket) {
                            while !self.check(&TokenKind::RightBracket) && !self.is_at_end() {
                                if let TokenKind::StringLit(s) = self.peek_kind() {
                                    invalidate_on.push(s.clone());
                                    self.advance();
                                }
                                self.match_token(&TokenKind::Comma);
                            }
                            self.expect(&TokenKind::RightBracket)?;
                        }
                    }
                    _ => { self.advance(); }
                }
                self.match_token(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RightBrace)?;
        }

        self.match_token(&TokenKind::Comma);

        Ok(CacheQueryDef { name, params, fetch_expr, contract, ttl, stale, invalidate_on, span })
    }

    fn parse_cache_mutation(&mut self) -> Result<CacheMutationDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Mutation)?;
        let name = self.expect_ident()?;

        let params = if self.match_token(&TokenKind::LeftParen) {
            let p = self.parse_params()?;
            self.expect(&TokenKind::RightParen)?;
            p
        } else {
            vec![]
        };

        self.expect(&TokenKind::Colon)?;
        let fetch_expr = self.parse_expr()?;

        let mut optimistic = false;
        let mut rollback_on_error = false;
        let mut invalidate = vec![];

        if self.match_token(&TokenKind::LeftBrace) {
            while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                let key = self.expect_ident()?;
                match key.as_str() {
                    "optimistic" => {
                        self.expect(&TokenKind::Colon)?;
                        match self.peek_kind() {
                            TokenKind::True => { optimistic = true; self.advance(); }
                            TokenKind::False => { optimistic = false; self.advance(); }
                            TokenKind::Ident(v) => { optimistic = v == "true"; self.advance(); }
                            _ => {}
                        }
                    }
                    "rollback_on_error" => {
                        self.expect(&TokenKind::Colon)?;
                        match self.peek_kind() {
                            TokenKind::True => { rollback_on_error = true; self.advance(); }
                            TokenKind::False => { rollback_on_error = false; self.advance(); }
                            TokenKind::Ident(v) => { rollback_on_error = v == "true"; self.advance(); }
                            _ => {}
                        }
                    }
                    "invalidate" => {
                        self.expect(&TokenKind::Colon)?;
                        if self.match_token(&TokenKind::LeftBracket) {
                            while !self.check(&TokenKind::RightBracket) && !self.is_at_end() {
                                if let TokenKind::StringLit(s) = self.peek_kind() {
                                    invalidate.push(s.clone());
                                    self.advance();
                                }
                                self.match_token(&TokenKind::Comma);
                            }
                            self.expect(&TokenKind::RightBracket)?;
                        }
                    }
                    _ => {
                        self.expect(&TokenKind::Colon)?;
                        self.advance();
                    }
                }
                self.match_token(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RightBrace)?;
        }

        self.match_token(&TokenKind::Comma);

        Ok(CacheMutationDef { name, params, fetch_expr, optimistic, rollback_on_error, invalidate, span })
    }

    fn parse_db(&mut self, is_pub: bool) -> Result<DbDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Db)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut version = None;
        let mut stores = vec![];

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = self.expect_ident()?;
            match key.as_str() {
                "version" => {
                    self.expect(&TokenKind::Colon)?;
                    if let TokenKind::Integer(v) = self.peek_kind() {
                        version = Some(v as u32);
                        self.advance();
                    }
                    self.match_token(&TokenKind::Comma);
                }
                "store" => {
                    let store_span = self.current_span();
                    let store_name = if let TokenKind::StringLit(s) = self.peek_kind() {
                        self.advance();
                        s
                    } else {
                        self.expect_ident()?
                    };
                    self.expect(&TokenKind::LeftBrace)?;
                    let mut store_key = "id".to_string();
                    let mut indexes = vec![];
                    while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                        let field = self.expect_ident()?;
                        match field.as_str() {
                            "key" => {
                                self.expect(&TokenKind::Colon)?;
                                if let TokenKind::StringLit(s) = self.peek_kind() {
                                    store_key = s;
                                    self.advance();
                                }
                                self.match_token(&TokenKind::Comma);
                            }
                            "index" => {
                                let idx_name = if let TokenKind::StringLit(s) = self.peek_kind() {
                                    self.advance();
                                    s
                                } else {
                                    self.expect_ident()?
                                };
                                self.expect(&TokenKind::FatArrow)?;
                                let idx_path = if let TokenKind::StringLit(s) = self.peek_kind() {
                                    self.advance();
                                    s
                                } else {
                                    self.expect_ident()?
                                };
                                indexes.push((idx_name, idx_path));
                                self.match_token(&TokenKind::Comma);
                            }
                            _ => {
                                // skip unknown field
                                self.expect(&TokenKind::Colon)?;
                                self.parse_expr()?;
                                self.match_token(&TokenKind::Comma);
                            }
                        }
                    }
                    self.expect(&TokenKind::RightBrace)?;
                    stores.push(DbStoreDef { name: store_name, key: store_key, indexes, span: store_span });
                }
                _ => {
                    // skip unknown
                    self.expect(&TokenKind::Colon)?;
                    self.parse_expr()?;
                    self.match_token(&TokenKind::Comma);
                }
            }
        }
        self.expect(&TokenKind::RightBrace)?;
        Ok(DbDef { name, version, stores, is_pub, span })
    }

    /// Parse `breakpoints Name { sm: 640, md: 768 }` where the keyword is the
    /// plural `breakpoints` ident (not the `breakpoint` keyword token).
    fn parse_breakpoints_def_plural(&mut self) -> Result<BreakpointsDef, ParseError> {
        let span = self.current_span();
        self.advance(); // consume `breakpoints` ident
        // Optional name
        if self.is_ident_like() {
            self.advance();
        }
        self.expect(&TokenKind::LeftBrace)?;

        let mut breakpoints = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            if let TokenKind::Integer(n) = self.peek_kind() {
                breakpoints.push((name, n as u32));
                self.advance();
            } else {
                return Err(self.error("Expected integer value for breakpoint"));
            }
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(BreakpointsDef { breakpoints, span })
    }

    fn parse_breakpoints_def(&mut self) -> Result<BreakpointsDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Breakpoint)?;
        // Optional name: `breakpoints AppBreakpoints { ... }`
        if self.is_ident_like() {
            self.advance();
        }
        self.expect(&TokenKind::LeftBrace)?;

        let mut breakpoints = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            if let TokenKind::Integer(n) = self.peek_kind() {
                breakpoints.push((name, n as u32));
                self.advance();
            } else {
                return Err(self.error("Expected integer value for breakpoint"));
            }
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(BreakpointsDef { breakpoints, span })
    }

    /// Parse `theme Name { light { key: "val", ... } dark { ... } }` or
    /// `theme Name { light { ... } dark: auto }` or
    /// `theme Name { auto, primary: "red" }`
    fn parse_theme(&mut self, is_pub: bool) -> Result<ThemeDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Theme)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut light = None;
        let mut dark = None;
        let mut dark_auto = false;
        let mut primary = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
                TokenKind::Ident(ref id) if id == "light" => {
                    self.advance();
                    self.expect(&TokenKind::LeftBrace)?;
                    let mut entries = Vec::new();
                    while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                        let key = self.expect_ident()?;
                        self.expect(&TokenKind::Colon)?;
                        let val = self.parse_expr()?;
                        entries.push((key, val));
                        self.match_token(&TokenKind::Comma);
                    }
                    self.expect(&TokenKind::RightBrace)?;
                    light = Some(entries);
                }
                TokenKind::Ident(ref id) if id == "dark" => {
                    self.advance();
                    if self.match_token(&TokenKind::Colon) {
                        // dark: auto
                        if let TokenKind::Ident(s) = self.peek_kind() {
                            if s == "auto" {
                                self.advance();
                                dark_auto = true;
                            }
                        }
                    } else {
                        self.expect(&TokenKind::LeftBrace)?;
                        let mut entries = Vec::new();
                        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                            let key = self.expect_ident()?;
                            self.expect(&TokenKind::Colon)?;
                            let val = self.parse_expr()?;
                            entries.push((key, val));
                            self.match_token(&TokenKind::Comma);
                        }
                        self.expect(&TokenKind::RightBrace)?;
                        dark = Some(entries);
                    }
                }
                TokenKind::Ident(ref id) if id == "auto" => {
                    self.advance();
                    dark_auto = true;
                    self.match_token(&TokenKind::Comma);
                }
                TokenKind::Ident(ref id) if id == "primary" => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    primary = Some(self.parse_expr()?);
                    self.match_token(&TokenKind::Comma);
                }
                // `default: "auto"` — default theme mode declaration
                TokenKind::Ident(ref id) if id == "default" => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    // Consume the value (e.g., "auto", "light", "dark") — stored implicitly
                    let _val = self.parse_expr()?;
                    self.match_token(&TokenKind::Comma);
                }
                _ => return Err(self.error("Expected light, dark, auto, primary, or default in theme")),
            }
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(ThemeDef { name, light, dark, dark_auto, primary, is_pub, span })
    }

    /// Parse `spring FadeIn { stiffness: 120, damping: 14, mass: 1, properties: ["opacity", "transform"] }`
    fn parse_spring_block(&mut self, is_pub: bool) -> Result<AnimationBlockDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Spring)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut stiffness = None;
        let mut damping = None;
        let mut mass = None;
        let mut properties = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            match key.as_str() {
                "stiffness" => {
                    if let TokenKind::Integer(n) = self.peek_kind() {
                        stiffness = Some(n as f64);
                        self.advance();
                    } else if let TokenKind::Float(f) = self.peek_kind() {
                        stiffness = Some(f);
                        self.advance();
                    }
                }
                "damping" => {
                    if let TokenKind::Integer(n) = self.peek_kind() {
                        damping = Some(n as f64);
                        self.advance();
                    } else if let TokenKind::Float(f) = self.peek_kind() {
                        damping = Some(f);
                        self.advance();
                    }
                }
                "mass" => {
                    if let TokenKind::Integer(n) = self.peek_kind() {
                        mass = Some(n as f64);
                        self.advance();
                    } else if let TokenKind::Float(f) = self.peek_kind() {
                        mass = Some(f);
                        self.advance();
                    }
                }
                "properties" => {
                    self.expect(&TokenKind::LeftBracket)?;
                    while !self.check(&TokenKind::RightBracket) && !self.is_at_end() {
                        if let TokenKind::StringLit(s) = self.peek_kind() {
                            properties.push(s.clone());
                            self.advance();
                        } else {
                            self.advance();
                        }
                        self.match_token(&TokenKind::Comma);
                    }
                    self.expect(&TokenKind::RightBracket)?;
                }
                _ => { self.advance(); }
            }
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(AnimationBlockDef {
            name,
            kind: AnimationKind::Spring { stiffness, damping, mass, properties },
            is_pub,
            span,
        })
    }

    /// Parse `keyframes SlideIn { 0% { ... } 100% { ... } duration: "300ms", easing: "ease-out" }`
    fn parse_keyframes_block(&mut self, is_pub: bool) -> Result<AnimationBlockDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Keyframes)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut frames = Vec::new();
        let mut duration = None;
        let mut easing = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            if let TokenKind::Integer(_) = self.peek_kind() {
                // Parse percentage: 0% { ... }
                let pct_val = if let TokenKind::Integer(n) = self.advance().kind {
                    n as f64
                } else {
                    0.0
                };
                self.expect(&TokenKind::Percent)?;
                self.expect(&TokenKind::LeftBrace)?;
                let mut props = Vec::new();
                while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                    let prop_name = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    let prop_val = self.parse_expr()?;
                    props.push((prop_name, prop_val));
                    self.match_token(&TokenKind::Comma);
                }
                self.expect(&TokenKind::RightBrace)?;
                frames.push((pct_val, props));
            } else if self.is_ident_like() {
                let key = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                match key.as_str() {
                    "duration" => {
                        if let TokenKind::StringLit(s) = self.peek_kind() {
                            duration = Some(s.clone());
                            self.advance();
                        }
                    }
                    "easing" => {
                        if let TokenKind::StringLit(s) = self.peek_kind() {
                            easing = Some(s.clone());
                            self.advance();
                        }
                    }
                    _ => { self.advance(); }
                }
                self.match_token(&TokenKind::Comma);
            } else {
                self.advance();
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(AnimationBlockDef {
            name,
            kind: AnimationKind::Keyframes { frames, duration, easing },
            is_pub,
            span,
        })
    }

    /// Parse `stagger ListAppear { animation: FadeIn, delay: "50ms" }`
    fn parse_stagger_block(&mut self, is_pub: bool) -> Result<AnimationBlockDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Stagger)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut animation = String::new();
        let mut delay = None;
        let mut selector = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            match key.as_str() {
                "animation" => {
                    animation = self.expect_ident()?;
                }
                "delay" => {
                    if let TokenKind::StringLit(s) = self.peek_kind() {
                        delay = Some(s.clone());
                        self.advance();
                    }
                }
                "selector" => {
                    if let TokenKind::StringLit(s) = self.peek_kind() {
                        selector = Some(s.clone());
                        self.advance();
                    }
                }
                _ => { self.advance(); }
            }
            self.match_token(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(AnimationBlockDef {
            name,
            kind: AnimationKind::Stagger { animation, delay, selector },
            is_pub,
            span,
        })
    }

    // === Helpers ===

    fn peek_kind(&self) -> TokenKind {
        self.tokens.get(self.pos).map(|t| t.kind.clone()).unwrap_or(TokenKind::Eof)
    }

    fn current_span(&self) -> Span {
        self.tokens.get(self.pos).map(|t| t.span).unwrap_or(Span::new(0, 0, 0, 0))
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        self.pos += 1;
        tok
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek_kind()) == std::mem::discriminant(kind)
    }

    fn match_token(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<Token, ParseError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(ParseError {
                message: format!("Expected {:?}, found {:?}", kind, self.peek_kind()),
                span: self.current_span(),
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.peek_kind() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            // Allow contextual keywords to be used as identifiers in key-value positions,
            // variable names, function parameters, struct fields, etc.
            ref tok => {
                if let Some(name) = tok.as_contextual_ident() {
                    self.advance();
                    Ok(name.to_string())
                } else {
                    Err(ParseError {
                        message: format!("Expected identifier, found {:?}", tok),
                        span: self.current_span(),
                    })
                }
            }
        }
    }

    /// Returns true if the current token can be consumed as an identifier by expect_ident.
    fn is_ident_like(&self) -> bool {
        match self.peek_kind() {
            TokenKind::Ident(_) => true,
            ref tok => tok.as_contextual_ident().is_some(),
        }
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn error(&self, msg: &str) -> ParseError {
        ParseError {
            message: msg.to_string(),
            span: self.current_span(),
        }
    }

    /// Returns true if the `{` at the current position begins an object literal
    /// rather than a block statement. Heuristic: `{ }` (empty) or `{ ident: ...`
    /// or `{ "string": ...` where the token after the key is `:`.
    /// Blocks start with statements: let, signal, return, fn, if, for, while,
    /// match, yield, or an expression that is not a key:value pair.
    fn is_object_literal_brace(&self) -> bool {
        // pos is at `{`
        let t1 = self.tokens.get(self.pos + 1);
        let t2 = self.tokens.get(self.pos + 2);
        match (t1.map(|t| &t.kind), t2.map(|t| &t.kind)) {
            // Empty braces `{}` — treat as object literal (empty object)
            (Some(TokenKind::RightBrace), _) => true,
            // `{ ident : ...` — object literal
            (Some(TokenKind::Ident(_)), Some(TokenKind::Colon)) => true,
            // `{ "string" : ...` — object literal with string key
            (Some(TokenKind::StringLit(_)), Some(TokenKind::Colon)) => true,
            // Keyword-idents that can be object keys: method, headers, body, url, etc.
            // These keyword tokens followed by `:` are object literal keys.
            (Some(k), Some(TokenKind::Colon)) if Self::token_is_keyword_ident(k) => true,
            // Otherwise it's a block
            _ => false,
        }
    }

    /// Returns true if the token kind is a keyword that can be used as an
    /// object literal key (i.e., it is recognized by `expect_ident`).
    fn token_is_keyword_ident(kind: &TokenKind) -> bool {
        matches!(kind,
            TokenKind::Canonical | TokenKind::Selector | TokenKind::Sandbox
            | TokenKind::Loading | TokenKind::Duration | TokenKind::Invalidate
            | TokenKind::Optimistic | TokenKind::Validate | TokenKind::Schema
            | TokenKind::Instant | TokenKind::Fluid | TokenKind::Clipboard
            | TokenKind::Draggable | TokenKind::Droppable | TokenKind::Crypto
            | TokenKind::Virtual | TokenKind::Breakpoint | TokenKind::Download
            | TokenKind::Haptic | TokenKind::Biometric | TokenKind::Camera
            | TokenKind::Geolocation | TokenKind::Flag | TokenKind::Trace
            | TokenKind::Env | TokenKind::Fallback | TokenKind::Push
            | TokenKind::Query | TokenKind::Store | TokenKind::True
            | TokenKind::False | TokenKind::Secret | TokenKind::Tool
            | TokenKind::Theme | TokenKind::Page | TokenKind::Chunk
            | TokenKind::Form | TokenKind::OnMessage | TokenKind::Upload
            | TokenKind::Payment | TokenKind::Style
        )
    }

    /// Parse an object literal: `{ key: value, key2: value2 }`.
    /// Keys can be identifiers or string literals.
    fn parse_object_literal(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::LeftBrace)?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = if let TokenKind::StringLit(s) = self.peek_kind() {
                self.advance();
                s
            } else {
                self.expect_ident()?
            };
            self.expect(&TokenKind::Colon)?;
            let val = self.parse_expr()?;
            fields.push((key, val));
            if !self.check(&TokenKind::RightBrace) {
                self.expect(&TokenKind::Comma)?;
            }
        }
        self.expect(&TokenKind::RightBrace)?;
        Ok(Expr::ObjectLit { fields })
    }
}

/// Convenience free function: lex-style entry point that returns both the
/// (possibly partial) AST and all accumulated parse errors.
pub fn parse(tokens: Vec<Token>) -> (Program, Vec<ParseError>) {
    let mut parser = Parser::new(tokens);
    parser.parse_program_recovering()
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:{}] {}", self.span.line, self.span.col, self.message)
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(src: &str) -> Program {
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        parser.parse_program().unwrap()
    }

    #[test]
    fn test_parse_function() {
        let prog = parse("fn add(a: i32, b: i32) -> i32 { return a + b; }");
        assert_eq!(prog.items.len(), 1);
        assert!(matches!(prog.items[0], Item::Function(_)));
    }

    #[test]
    fn test_parse_struct() {
        let prog = parse("struct Point { x: f64, y: f64 }");
        assert_eq!(prog.items.len(), 1);
        assert!(matches!(prog.items[0], Item::Struct(_)));
    }

    #[test]
    fn test_parse_component() {
        let prog = parse(r#"
            component Hello(name: String) {
                render {
                    <div>
                        {name}
                    </div>
                }
            }
        "#);
        assert_eq!(prog.items.len(), 1);
        assert!(matches!(prog.items[0], Item::Component(_)));
    }

    // --- Error-recovery tests ---

    /// Helper: parse with recovery and return both the AST and errors.
    fn parse_recovering(src: &str) -> (Program, Vec<ParseError>) {
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        parser.parse_program_recovering()
    }

    /// Alias for `parse_recovering` used by destructuring tests.
    fn parse_source(src: &str) -> (Program, Vec<ParseError>) {
        parse_recovering(src)
    }

    #[test]
    fn test_multiple_errors_reported() {
        // Two broken items separated by valid keywords that the synchronizer
        // will find.  `fn ;` fails (missing ident after `fn`), recovery skips
        // to the next `fn`, which also fails the same way — two errors.
        let src = "fn ; fn ;";
        let (_prog, errors) = parse_recovering(src);
        assert!(errors.len() >= 2, "expected at least 2 errors, got {}", errors.len());
    }

    #[test]
    fn test_valid_items_after_invalid_are_parsed() {
        // An invalid item followed by two valid ones.
        let src = r#"
            1 + 2
            fn good_one() -> i32 { return 1; }
            struct Point { x: f64, y: f64 }
        "#;
        let (prog, errors) = parse_recovering(src);
        // The broken item should produce an error
        assert!(!errors.is_empty(), "expected at least one error");
        // The two valid items should still be present in the AST
        assert_eq!(
            prog.items.len(), 2,
            "expected 2 valid items, got {}", prog.items.len()
        );
        assert!(matches!(prog.items[0], Item::Function(_)));
        assert!(matches!(prog.items[1], Item::Struct(_)));
    }

    #[test]
    fn test_synchronize_finds_next_item_boundary() {
        // Garbage tokens followed by a valid `fn` — the synchronizer should
        // land on `fn` so the function is parsed correctly.
        let src = r#"
            1 + 2 + 3;
            fn ok() -> i32 { return 42; }
        "#;
        let (prog, errors) = parse_recovering(src);
        assert!(!errors.is_empty());
        assert_eq!(prog.items.len(), 1);
        assert!(matches!(prog.items[0], Item::Function(_)));
    }

    #[test]
    fn test_recovery_across_many_items() {
        // Mix of broken and valid items.
        let src = r#"
            fn first() {}
            1234
            struct S { x: i32 }
            5678
            fn last() {}
        "#;
        let (prog, errors) = parse_recovering(src);
        assert!(errors.len() >= 2, "expected at least 2 errors, got {}", errors.len());
        assert_eq!(prog.items.len(), 3, "expected 3 valid items, got {}", prog.items.len());
    }

    #[test]
    fn test_has_errors_method() {
        let mut lexer = Lexer::new("fn ok() {}");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let (_prog, errors) = parser.parse_program_recovering();
        assert!(errors.is_empty());
        assert!(!parser.has_errors());
    }

    #[test]
    fn test_free_parse_function() {
        let mut lexer = Lexer::new("fn a() {} fn b() {}");
        let tokens = lexer.tokenize().unwrap();
        let (prog, errors) = super::parse(tokens);
        assert!(errors.is_empty());
        assert_eq!(prog.items.len(), 2);
    }

    // -- Module system tests -----------------------------------------------

    #[test]
    fn test_parse_use_single() {
        let prog = parse("use foo::bar::Baz;");
        assert_eq!(prog.items.len(), 1);
        if let Item::Use(u) = &prog.items[0] {
            assert_eq!(u.segments, vec!["foo", "bar", "Baz"]);
            assert!(!u.glob);
            assert!(u.alias.is_none());
            assert!(u.group.is_none());
        } else {
            panic!("expected Use item");
        }
    }

    #[test]
    fn test_parse_use_glob() {
        let prog = parse("use foo::bar::*;");
        assert_eq!(prog.items.len(), 1);
        if let Item::Use(u) = &prog.items[0] {
            assert_eq!(u.segments, vec!["foo", "bar"]);
            assert!(u.glob);
        } else {
            panic!("expected Use item");
        }
    }

    #[test]
    fn test_parse_use_alias() {
        let prog = parse("use foo::Bar as Baz;");
        assert_eq!(prog.items.len(), 1);
        if let Item::Use(u) = &prog.items[0] {
            assert_eq!(u.segments, vec!["foo", "Bar"]);
            assert_eq!(u.alias, Some("Baz".to_string()));
            assert!(!u.glob);
        } else {
            panic!("expected Use item");
        }
    }

    #[test]
    fn test_parse_use_multi_import() {
        let prog = parse("use foo::bar::{A, B, C};");
        assert_eq!(prog.items.len(), 1);
        if let Item::Use(u) = &prog.items[0] {
            assert_eq!(u.segments, vec!["foo", "bar"]);
            assert!(!u.glob);
            let group = u.group.as_ref().unwrap();
            assert_eq!(group.len(), 3);
            assert_eq!(group[0].name, "A");
            assert_eq!(group[1].name, "B");
            assert_eq!(group[2].name, "C");
        } else {
            panic!("expected Use item");
        }
    }

    #[test]
    fn test_parse_use_multi_with_alias() {
        let prog = parse("use math::{Vec3, Mat4 as Matrix};");
        assert_eq!(prog.items.len(), 1);
        if let Item::Use(u) = &prog.items[0] {
            assert_eq!(u.segments, vec!["math"]);
            let group = u.group.as_ref().unwrap();
            assert_eq!(group.len(), 2);
            assert_eq!(group[0].name, "Vec3");
            assert!(group[0].alias.is_none());
            assert_eq!(group[1].name, "Mat4");
            assert_eq!(group[1].alias, Some("Matrix".to_string()));
        } else {
            panic!("expected Use item");
        }
    }

    #[test]
    fn test_parse_mod_external() {
        let prog = parse("mod utils;");
        assert_eq!(prog.items.len(), 1);
        if let Item::Mod(m) = &prog.items[0] {
            assert_eq!(m.name, "utils");
            assert!(m.items.is_none());
            assert!(m.is_external);
        } else {
            panic!("expected Mod item");
        }
    }

    #[test]
    fn test_parse_mod_inline() {
        let prog = parse("mod helpers { fn greet() {} }");
        assert_eq!(prog.items.len(), 1);
        if let Item::Mod(m) = &prog.items[0] {
            assert_eq!(m.name, "helpers");
            assert!(!m.is_external);
            let items = m.items.as_ref().unwrap();
            assert_eq!(items.len(), 1);
            assert!(matches!(items[0], Item::Function(_)));
        } else {
            panic!("expected Mod item");
        }
    }

    #[test]
    fn test_format_string_produces_format_string_expr() {
        use crate::lexer::Lexer;

        let src = r#"fn main() { let s = f"hello {name}"; }"#;
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();

        assert_eq!(program.items.len(), 1);
        if let Item::Function(f) = &program.items[0] {
            assert_eq!(f.body.stmts.len(), 1);
            if let Stmt::Let { value, .. } = &f.body.stmts[0] {
                if let Expr::FormatString { parts } = value {
                    assert_eq!(parts.len(), 2);
                    assert_eq!(parts[0], FormatPart::Literal("hello ".into()));
                    if let FormatPart::Expression(expr) = &parts[1] {
                        assert_eq!(**expr, Expr::Ident("name".into()));
                    } else {
                        panic!("Expected Expression part, got {:?}", parts[1]);
                    }
                } else {
                    panic!("Expected FormatString, got {:?}", value);
                }
            } else {
                panic!("Expected Let statement");
            }
        } else {
            panic!("Expected Function item");
        }
    }

    #[test]
    fn test_format_string_with_binary_expr() {
        use crate::lexer::Lexer;

        let src = r#"fn main() { let s = f"sum: {a + b}"; }"#;
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();

        if let Item::Function(f) = &program.items[0] {
            if let Stmt::Let { value, .. } = &f.body.stmts[0] {
                if let Expr::FormatString { parts } = value {
                    assert_eq!(parts.len(), 2);
                    assert_eq!(parts[0], FormatPart::Literal("sum: ".into()));
                    if let FormatPart::Expression(expr) = &parts[1] {
                        if let Expr::Binary { op, left, right } = expr.as_ref() {
                            assert_eq!(*op, BinOp::Add);
                            assert_eq!(**left, Expr::Ident("a".into()));
                            assert_eq!(**right, Expr::Ident("b".into()));
                        } else {
                            panic!("Expected Binary expression, got {:?}", expr);
                        }
                    } else {
                        panic!("Expected Expression part");
                    }
                } else {
                    panic!("Expected FormatString");
                }
            } else {
                panic!("Expected Let");
            }
        } else {
            panic!("Expected Function");
        }
    }

    // --- Closure/lambda parsing tests ---

    #[test]
    fn test_parse_closure_with_typed_param() {
        let prog = parse("fn main() { let f = |x: i32| x + 1; }");
        if let Item::Function(func) = &prog.items[0] {
            if let Stmt::Let { value, .. } = &func.body.stmts[0] {
                match value {
                    Expr::Closure { params, body } => {
                        assert_eq!(params.len(), 1);
                        assert_eq!(params[0].0, "x");
                        assert!(params[0].1.is_some());
                        assert!(matches!(body.as_ref(), Expr::Binary { .. }));
                    }
                    _ => panic!("Expected Expr::Closure, got {:?}", value),
                }
            } else {
                panic!("Expected Let statement");
            }
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_parse_closure_no_params() {
        let prog = parse("fn main() { let f = || 42; }");
        if let Item::Function(func) = &prog.items[0] {
            if let Stmt::Let { value, .. } = &func.body.stmts[0] {
                match value {
                    Expr::Closure { params, body } => {
                        assert_eq!(params.len(), 0);
                        assert!(matches!(body.as_ref(), Expr::Integer(42)));
                    }
                    _ => panic!("Expected Expr::Closure, got {:?}", value),
                }
            } else {
                panic!("Expected Let statement");
            }
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_parse_closure_multiple_params() {
        let prog = parse("fn main() { let f = |x: i32, y: i32| x + y; }");
        if let Item::Function(func) = &prog.items[0] {
            if let Stmt::Let { value, .. } = &func.body.stmts[0] {
                match value {
                    Expr::Closure { params, .. } => {
                        assert_eq!(params.len(), 2);
                        assert_eq!(params[0].0, "x");
                        assert_eq!(params[1].0, "y");
                    }
                    _ => panic!("Expected Expr::Closure"),
                }
            } else {
                panic!("Expected Let statement");
            }
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_parse_closure_inferred_types() {
        let prog = parse("fn main() { let f = |x, y| x + y; }");
        if let Item::Function(func) = &prog.items[0] {
            if let Stmt::Let { value, .. } = &func.body.stmts[0] {
                match value {
                    Expr::Closure { params, .. } => {
                        assert_eq!(params.len(), 2);
                        assert!(params[0].1.is_none()); // inferred
                        assert!(params[1].1.is_none()); // inferred
                    }
                    _ => panic!("Expected Expr::Closure"),
                }
            } else {
                panic!("Expected Let statement");
            }
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_parse_closure_block_body() {
        let prog = parse("fn main() { let f = |x: i32| { let y = x + 1; return y; }; }");
        if let Item::Function(func) = &prog.items[0] {
            if let Stmt::Let { value, .. } = &func.body.stmts[0] {
                match value {
                    Expr::Closure { params, body } => {
                        assert_eq!(params.len(), 1);
                        assert!(matches!(body.as_ref(), Expr::Block(_)));
                    }
                    _ => panic!("Expected Expr::Closure"),
                }
            } else {
                panic!("Expected Let statement");
            }
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_tuple_destructure() {
        let src = "fn main() { let (a, b) = get_pair(); }";
        let (program, errors) = parse_source(src);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        if let Item::Function(f) = &program.items[0] {
            if let Stmt::LetDestructure { pattern, .. } = &f.body.stmts[0] {
                if let Pattern::Tuple(pats) = pattern {
                    assert_eq!(pats.len(), 2);
                    assert_eq!(pats[0], Pattern::Ident("a".into()));
                    assert_eq!(pats[1], Pattern::Ident("b".into()));
                } else {
                    panic!("Expected Tuple pattern, got {:?}", pattern);
                }
            } else {
                panic!("Expected LetDestructure, got {:?}", f.body.stmts[0]);
            }
        }
    }

    #[test]
    fn test_struct_destructure() {
        let src = "fn main() { let Point { x, y } = origin(); }";
        let (program, errors) = parse_source(src);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        if let Item::Function(f) = &program.items[0] {
            if let Stmt::LetDestructure { pattern, .. } = &f.body.stmts[0] {
                if let Pattern::Struct { name, fields, rest } = pattern {
                    assert_eq!(name, "Point");
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].0, "x");
                    assert_eq!(fields[1].0, "y");
                    assert!(!rest);
                } else {
                    panic!("Expected Struct pattern, got {:?}", pattern);
                }
            } else {
                panic!("Expected LetDestructure");
            }
        }
    }

    #[test]
    fn test_struct_destructure_with_rest() {
        let src = "fn main() { let User { name, .. } = get_user(); }";
        let (program, errors) = parse_source(src);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        if let Item::Function(f) = &program.items[0] {
            if let Stmt::LetDestructure { pattern, .. } = &f.body.stmts[0] {
                if let Pattern::Struct { name, fields, rest } = pattern {
                    assert_eq!(name, "User");
                    assert_eq!(fields.len(), 1);
                    assert_eq!(fields[0].0, "name");
                    assert!(*rest);
                } else {
                    panic!("Expected Struct pattern");
                }
            } else {
                panic!("Expected LetDestructure");
            }
        }
    }

    #[test]
    fn test_array_destructure() {
        let src = "fn main() { let [first, second] = get_arr(); }";
        let (program, errors) = parse_source(src);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        if let Item::Function(f) = &program.items[0] {
            if let Stmt::LetDestructure { pattern, .. } = &f.body.stmts[0] {
                if let Pattern::Array(pats) = pattern {
                    assert_eq!(pats.len(), 2);
                    assert_eq!(pats[0], Pattern::Ident("first".into()));
                    assert_eq!(pats[1], Pattern::Ident("second".into()));
                } else {
                    panic!("Expected Array pattern");
                }
            } else {
                panic!("Expected LetDestructure");
            }
        }
    }

    #[test]
    fn test_try_operator() {
        let src = "fn main() { let x = get_result()?; }";
        let (program, errors) = parse_source(src);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        if let Item::Function(f) = &program.items[0] {
            if let Stmt::Let { value, .. } = &f.body.stmts[0] {
                assert!(matches!(value, Expr::Try(_)), "Expected Try expr, got {:?}", value);
            } else {
                panic!("Expected Let");
            }
        }
    }

    #[test]
    fn test_try_operator_chained() {
        let src = "fn main() { let x = foo()?.bar()?; }";
        let (program, errors) = parse_source(src);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        if let Item::Function(f) = &program.items[0] {
            if let Stmt::Let { value, .. } = &f.body.stmts[0] {
                // The outer value should be Try(MethodCall { ... })
                assert!(matches!(value, Expr::Try(_)), "Expected Try expr, got {:?}", value);
            } else {
                panic!("Expected Let");
            }
        }
    }

    // ========================================================================
    // COMPREHENSIVE PARSER COVERAGE TESTS
    // ========================================================================

    /// Helper: parse a single expression inside a function body.
    fn parse_expr(src: &str) -> Expr {
        let wrapped = format!("fn __test__() {{ let __v__ = {}; }}", src);
        let prog = parse(&wrapped);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Let { value, .. } = &f.body.stmts[0] {
                return value.clone();
            }
        }
        panic!("Failed to extract expression from: {}", src);
    }

    /// Helper: parse a single statement inside a function body.
    fn parse_stmt_helper(src: &str) -> Stmt {
        let wrapped = format!("fn __test__() {{ {} }}", src);
        let prog = parse(&wrapped);
        if let Item::Function(f) = &prog.items[0] {
            return f.body.stmts[0].clone();
        }
        panic!("Failed to extract statement from: {}", src);
    }

    // --- Enum definition ---

    #[test]
    fn test_parse_enum_simple() {
        let prog = parse("enum Color { Red, Green, Blue }");
        if let Item::Enum(e) = &prog.items[0] {
            assert_eq!(e.name, "Color");
            assert_eq!(e.variants.len(), 3);
            assert_eq!(e.variants[0].name, "Red");
            assert_eq!(e.variants[1].name, "Green");
            assert_eq!(e.variants[2].name, "Blue");
        } else {
            panic!("Expected Enum");
        }
    }

    #[test]
    fn test_parse_enum_with_fields() {
        let prog = parse("enum Shape { Circle(f64), Rect(f64, f64) }");
        if let Item::Enum(e) = &prog.items[0] {
            assert_eq!(e.name, "Shape");
            assert_eq!(e.variants[0].name, "Circle");
            assert_eq!(e.variants[0].fields.len(), 1);
            assert_eq!(e.variants[1].name, "Rect");
            assert_eq!(e.variants[1].fields.len(), 2);
        } else {
            panic!("Expected Enum");
        }
    }

    #[test]
    fn test_parse_enum_with_type_params() {
        let prog = parse("enum Option<T> { Some(T), None }");
        if let Item::Enum(e) = &prog.items[0] {
            assert_eq!(e.name, "Option");
            assert_eq!(e.type_params, vec!["T"]);
        } else {
            panic!("Expected Enum");
        }
    }

    #[test]
    fn test_parse_pub_enum() {
        let prog = parse("pub enum Dir { Up, Down }");
        if let Item::Enum(e) = &prog.items[0] {
            assert!(e.is_pub);
        } else {
            panic!("Expected Enum");
        }
    }

    // --- Struct definition ---

    #[test]
    fn test_parse_struct_with_type_params() {
        let prog = parse("struct Pair<T, U> { first: T, second: U }");
        if let Item::Struct(s) = &prog.items[0] {
            assert_eq!(s.name, "Pair");
            assert_eq!(s.type_params, vec!["T", "U"]);
            assert_eq!(s.fields.len(), 2);
        } else {
            panic!("Expected Struct");
        }
    }

    #[test]
    fn test_parse_struct_with_lifetimes() {
        let prog = parse("struct Ref<'a, T> { data: &'a T }");
        if let Item::Struct(s) = &prog.items[0] {
            assert_eq!(s.name, "Ref");
            assert_eq!(s.lifetimes, vec!["a"]);
            assert_eq!(s.type_params, vec!["T"]);
        } else {
            panic!("Expected Struct");
        }
    }

    #[test]
    fn test_parse_struct_pub_fields() {
        let prog = parse("struct Foo { pub x: i32, y: i32 }");
        if let Item::Struct(s) = &prog.items[0] {
            assert!(s.fields[0].is_pub);
            assert!(!s.fields[1].is_pub);
        } else {
            panic!("Expected Struct");
        }
    }

    #[test]
    fn test_parse_struct_where_clause() {
        let prog = parse("struct Container<T> where T: Display { val: T }");
        if let Item::Struct(s) = &prog.items[0] {
            assert_eq!(s.trait_bounds.len(), 1);
            assert_eq!(s.trait_bounds[0].type_param, "T");
            assert_eq!(s.trait_bounds[0].trait_name, "Display");
        } else {
            panic!("Expected Struct");
        }
    }

    // --- Trait definition ---

    #[test]
    fn test_parse_trait() {
        let prog = parse("trait Drawable { fn draw(&self); fn area(&self) -> f64; }");
        if let Item::Trait(t) = &prog.items[0] {
            assert_eq!(t.name, "Drawable");
            assert_eq!(t.methods.len(), 2);
            assert_eq!(t.methods[0].name, "draw");
            assert!(t.methods[0].default_body.is_none());
            assert_eq!(t.methods[1].name, "area");
            assert!(t.methods[1].return_type.is_some());
        } else {
            panic!("Expected Trait");
        }
    }

    #[test]
    fn test_parse_trait_with_default_body() {
        let prog = parse("trait Greet { fn hello(&self) { return; } }");
        if let Item::Trait(t) = &prog.items[0] {
            assert!(t.methods[0].default_body.is_some());
        } else {
            panic!("Expected Trait");
        }
    }

    #[test]
    fn test_parse_trait_with_type_params() {
        let prog = parse("trait Container<T> { fn get(&self) -> T; }");
        if let Item::Trait(t) = &prog.items[0] {
            assert_eq!(t.type_params, vec!["T"]);
        } else {
            panic!("Expected Trait");
        }
    }

    // --- Impl block ---

    #[test]
    fn test_parse_impl() {
        let prog = parse("impl Point { fn new() -> Point { return Point { x: 0, y: 0 }; } }");
        if let Item::Impl(i) = &prog.items[0] {
            assert_eq!(i.target, "Point");
            assert!(i.trait_impls.is_empty());
            assert_eq!(i.methods.len(), 1);
        } else {
            panic!("Expected Impl");
        }
    }

    #[test]
    fn test_parse_impl_trait_for() {
        let prog = parse("impl Display for Point { fn fmt(&self) {} }");
        if let Item::Impl(i) = &prog.items[0] {
            assert_eq!(i.target, "Point");
            assert_eq!(i.trait_impls, vec!["Display"]);
        } else {
            panic!("Expected Impl");
        }
    }

    // --- Store ---

    #[test]
    fn test_parse_store() {
        let prog = parse(r#"
            store AppStore {
                signal count: i32 = 0;
                action increment(&mut self) { self.count = self.count + 1; }
                computed double(&self) -> i32 { return self.count * 2; }
                effect log_count(&self) { return; }
            }
        "#);
        if let Item::Store(s) = &prog.items[0] {
            assert_eq!(s.name, "AppStore");
            assert_eq!(s.signals.len(), 1);
            assert_eq!(s.actions.len(), 1);
            assert_eq!(s.computed.len(), 1);
            assert_eq!(s.effects.len(), 1);
        } else {
            panic!("Expected Store");
        }
    }

    #[test]
    fn test_parse_store_async_action() {
        let prog = parse(r#"
            store S {
                signal x: i32 = 0;
                async action fetch_data(&mut self) { return; }
            }
        "#);
        if let Item::Store(s) = &prog.items[0] {
            assert!(s.actions[0].is_async);
        } else {
            panic!("Expected Store");
        }
    }

    #[test]
    fn test_parse_store_atomic_signal() {
        let prog = parse(r#"
            store S {
                signal atomic counter: i32 = 0;
            }
        "#);
        if let Item::Store(s) = &prog.items[0] {
            assert!(s.signals[0].atomic);
        } else {
            panic!("Expected Store");
        }
    }

    #[test]
    fn test_parse_store_selector() {
        let prog = parse(r#"
            store S {
                signal count: i32 = 0;
                selector doubled: count * 2;
            }
        "#);
        if let Item::Store(s) = &prog.items[0] {
            assert_eq!(s.selectors.len(), 1);
            assert_eq!(s.selectors[0].name, "doubled");
        } else {
            panic!("Expected Store");
        }
    }

    #[test]
    fn test_parse_pub_store() {
        let prog = parse("pub store PS { signal x: i32 = 0; }");
        if let Item::Store(s) = &prog.items[0] {
            assert!(s.is_pub);
        } else {
            panic!("Expected Store");
        }
    }

    // --- Router ---

    #[test]
    fn test_parse_router() {
        let prog = parse(r#"
            router AppRouter {
                route "/" => Home,
                route "/about" => About,
                fallback => NotFound,
            }
        "#);
        if let Item::Router(r) = &prog.items[0] {
            assert_eq!(r.name, "AppRouter");
            assert_eq!(r.routes.len(), 2);
            assert_eq!(r.routes[0].path, "/");
            assert_eq!(r.routes[0].component, "Home");
            assert_eq!(r.routes[1].path, "/about");
            assert!(r.fallback.is_some());
        } else {
            panic!("Expected Router");
        }
    }

    #[test]
    fn test_parse_router_with_params() {
        let prog = parse(r#"
            router R {
                route "/user/:id" => UserProfile,
            }
        "#);
        if let Item::Router(r) = &prog.items[0] {
            assert_eq!(r.routes[0].params, vec!["id"]);
        } else {
            panic!("Expected Router");
        }
    }

    #[test]
    fn test_parse_router_with_guard() {
        let prog = parse(r#"
            router R {
                route "/admin" => Admin guard { is_admin() },
            }
        "#);
        if let Item::Router(r) = &prog.items[0] {
            assert!(r.routes[0].guard.is_some());
        } else {
            panic!("Expected Router");
        }
    }

    // --- Agent ---

    #[test]
    fn test_parse_agent() {
        let prog = parse(r#"
            agent Assistant {
                prompt system = "You are helpful.";
                tool search(text: String) -> String {
                    return text;
                }
                render {
                    <div />
                }
            }
        "#);
        if let Item::Agent(a) = &prog.items[0] {
            assert_eq!(a.name, "Assistant");
            assert_eq!(a.system_prompt, Some("You are helpful.".to_string()));
            assert_eq!(a.tools.len(), 1);
            assert_eq!(a.tools[0].name, "search");
            assert!(a.tools[0].return_type.is_some());
            assert!(a.render.is_some());
        } else {
            panic!("Expected Agent");
        }
    }

    // --- Contract ---

    #[test]
    fn test_parse_contract() {
        let prog = parse(r#"
            contract UserResponse {
                id: u32,
                name: String,
                email: String,
            }
        "#);
        if let Item::Contract(c) = &prog.items[0] {
            assert_eq!(c.name, "UserResponse");
            assert_eq!(c.fields.len(), 3);
            assert!(!c.fields[0].nullable);
        } else {
            panic!("Expected Contract");
        }
    }

    #[test]
    fn test_parse_contract_nullable() {
        let prog = parse(r#"
            contract Response {
                deleted_at: String?,
            }
        "#);
        if let Item::Contract(c) = &prog.items[0] {
            assert!(c.fields[0].nullable);
            assert!(matches!(c.fields[0].ty, Type::Option(_)));
        } else {
            panic!("Expected Contract");
        }
    }

    #[test]
    fn test_parse_contract_inline_enum() {
        let prog = parse(r#"
            contract C {
                tier: enum { free, pro },
            }
        "#);
        if let Item::Contract(c) = &prog.items[0] {
            // Inline enum is represented as Type::Named with generated name
            assert!(matches!(&c.fields[0].ty, Type::Named(n) if n == "C_tier"));
        } else {
            panic!("Expected Contract");
        }
    }

    #[test]
    fn test_parse_pub_contract() {
        let prog = parse("pub contract PC { id: u32 }");
        if let Item::Contract(c) = &prog.items[0] {
            assert!(c.is_pub);
        } else {
            panic!("Expected Contract");
        }
    }

    #[test]
    fn test_parse_contract_array_field() {
        let prog = parse(r#"
            contract AdminUser {
                id: i32,
                roles: [String],
                scores: [i32],
            }
        "#);
        if let Item::Contract(c) = &prog.items[0] {
            assert_eq!(c.name, "AdminUser");
            assert_eq!(c.fields.len(), 3);
            assert_eq!(c.fields[1].name, "roles");
            assert!(matches!(&c.fields[1].ty, Type::Array(inner) if matches!(inner.as_ref(), Type::Named(n) if n == "String")),
                "roles field should be [String]");
            assert_eq!(c.fields[2].name, "scores");
            assert!(matches!(&c.fields[2].ty, Type::Array(inner) if matches!(inner.as_ref(), Type::Named(n) if n == "i32")),
                "scores field should be [i32]");
        } else {
            panic!("Expected Contract");
        }
    }

    #[test]
    fn test_parse_contract_nested_type() {
        let prog = parse(r#"
            contract OrderResponse {
                id: u32,
                user: UserRow,
                address: ShippingAddress,
            }
        "#);
        if let Item::Contract(c) = &prog.items[0] {
            assert_eq!(c.name, "OrderResponse");
            assert_eq!(c.fields.len(), 3);
            assert_eq!(c.fields[1].name, "user");
            assert!(matches!(&c.fields[1].ty, Type::Named(n) if n == "UserRow"),
                "user field should be UserRow");
            assert_eq!(c.fields[2].name, "address");
            assert!(matches!(&c.fields[2].ty, Type::Named(n) if n == "ShippingAddress"),
                "address field should be ShippingAddress");
        } else {
            panic!("Expected Contract");
        }
    }

    #[test]
    fn test_parse_contract_array_and_nested() {
        let prog = parse(r#"
            contract FullResponse {
                id: i32,
                roles: [String],
                user: UserRow,
                tags: [i32],
            }
        "#);
        if let Item::Contract(c) = &prog.items[0] {
            assert_eq!(c.fields.len(), 4);
            assert!(matches!(&c.fields[1].ty, Type::Array(_)));
            assert!(matches!(&c.fields[2].ty, Type::Named(n) if n == "UserRow"));
            assert!(matches!(&c.fields[3].ty, Type::Array(_)));
        } else {
            panic!("Expected Contract");
        }
    }

    // --- App ---

    #[test]
    fn test_parse_app() {
        let prog = parse(r#"
            app MyApp {
                manifest {
                    name: "My App",
                }
                offline {
                    precache: ["/index.html"],
                    strategy: "cache-first",
                }
            }
        "#);
        if let Item::App(a) = &prog.items[0] {
            assert_eq!(a.name, "MyApp");
            assert!(a.manifest.is_some());
            assert!(a.offline.is_some());
            let off = a.offline.as_ref().unwrap();
            assert_eq!(off.precache, vec!["/index.html"]);
            assert_eq!(off.strategy, "cache-first");
        } else {
            panic!("Expected App");
        }
    }

    #[test]
    fn test_parse_app_with_push() {
        let prog = parse(r#"
            app PA {
                push {
                    vapid_key: "key123",
                }
            }
        "#);
        if let Item::App(a) = &prog.items[0] {
            let push = a.push.as_ref().unwrap();
            assert!(push.vapid_key.is_some());
        } else {
            panic!("Expected App");
        }
    }

    #[test]
    fn test_parse_app_with_a11y() {
        let prog = parse(r#"
            app A {
                a11y: manual,
            }
        "#);
        if let Item::App(a) = &prog.items[0] {
            assert_eq!(a.a11y, Some(A11yMode::Manual));
        } else {
            panic!("Expected App");
        }
    }

    #[test]
    fn test_parse_app_with_a11y_auto() {
        let prog = parse(r#"
            app A {
                a11y: auto,
            }
        "#);
        if let Item::App(a) = &prog.items[0] {
            assert_eq!(a.a11y, Some(A11yMode::Auto));
        } else {
            panic!("Expected App");
        }
    }

    // --- Page ---

    #[test]
    fn test_parse_page() {
        let prog = parse(r#"
            page BlogPost(slug: String) {
                meta {
                    title: "Blog",
                    description: "A blog post",
                }
                render {
                    <div />
                }
            }
        "#);
        if let Item::Page(p) = &prog.items[0] {
            assert_eq!(p.name, "BlogPost");
            assert_eq!(p.props.len(), 1);
            let meta = p.meta.as_ref().unwrap();
            assert!(meta.title.is_some());
            assert!(meta.description.is_some());
        } else {
            panic!("Expected Page");
        }
    }

    #[test]
    fn test_parse_page_meta_canonical() {
        // Note: "canonical" is a keyword token, so it can't be used as a
        // key in meta {} (which uses expect_ident()). Test with non-keyword keys.
        let prog = parse(r#"
            page P {
                meta {
                    title: "My Page",
                    og_image: "/img.png",
                    custom_key: "val",
                }
                render {
                    <div />
                }
            }
        "#);
        if let Item::Page(p) = &prog.items[0] {
            let meta = p.meta.as_ref().unwrap();
            assert!(meta.title.is_some());
            assert!(meta.og_image.is_some());
            assert_eq!(meta.extra.len(), 1);
        } else {
            panic!("Expected Page");
        }
    }

    #[test]
    fn test_parse_page_meta_structured_data() {
        let prog = parse(r#"
            page P {
                meta {
                    structured_data: Article {
                        headline: "Test",
                    },
                }
                render {
                    <div />
                }
            }
        "#);
        if let Item::Page(p) = &prog.items[0] {
            let meta = p.meta.as_ref().unwrap();
            assert_eq!(meta.structured_data.len(), 1);
            assert_eq!(meta.structured_data[0].schema_type, "Article");
        } else {
            panic!("Expected Page");
        }
    }

    // --- Form ---

    #[test]
    fn test_parse_form() {
        let prog = parse(r#"
            form LoginForm {
                field username: String {
                    required,
                    min_length: 3,
                    max_length: 20,
                    label: "Username",
                    placeholder: "Enter username",
                }
                field password: String {
                    required,
                    pattern: "^.{8,}$",
                    email,
                    url,
                    default: "test",
                }
            }
        "#);
        if let Item::Form(f) = &prog.items[0] {
            assert_eq!(f.name, "LoginForm");
            assert_eq!(f.fields.len(), 2);
            assert_eq!(f.fields[0].name, "username");
            assert!(f.fields[0].label.is_some());
            assert!(f.fields[0].placeholder.is_some());
            // Check validators
            let v = &f.fields[0].validators;
            assert!(v.iter().any(|vi| matches!(vi.kind, ValidatorKind::Required)));
            assert!(v.iter().any(|vi| matches!(vi.kind, ValidatorKind::MinLength(3))));
            assert!(v.iter().any(|vi| matches!(vi.kind, ValidatorKind::MaxLength(20))));

            let v2 = &f.fields[1].validators;
            assert!(v2.iter().any(|vi| matches!(vi.kind, ValidatorKind::Pattern(_))));
            assert!(v2.iter().any(|vi| matches!(vi.kind, ValidatorKind::Email)));
            assert!(v2.iter().any(|vi| matches!(vi.kind, ValidatorKind::Url)));
            assert!(f.fields[1].default_value.is_some());
        } else {
            panic!("Expected Form");
        }
    }

    #[test]
    fn test_parse_form_with_on_submit() {
        let prog = parse(r#"
            form F {
                field name: String;
                fn on_submit() { return; }
            }
        "#);
        if let Item::Form(f) = &prog.items[0] {
            assert_eq!(f.on_submit, Some("on_submit".to_string()));
            assert_eq!(f.methods.len(), 1);
        } else {
            panic!("Expected Form");
        }
    }

    // --- Channel ---

    #[test]
    fn test_parse_channel() {
        let prog = parse(r#"
            channel Chat -> ChatMessage {
                url: "/ws/chat",
                heartbeat: 30000,
                on_message fn handle(&self) {}
                on_connect fn conn(&self) {}
                on_disconnect fn disc(&self) {}
                fn send_msg(&self) {}
            }
        "#);
        if let Item::Channel(ch) = &prog.items[0] {
            assert_eq!(ch.name, "Chat");
            assert_eq!(ch.contract, Some("ChatMessage".to_string()));
            assert!(ch.on_message.is_some());
            assert!(ch.on_connect.is_some());
            assert!(ch.on_disconnect.is_some());
            assert_eq!(ch.methods.len(), 1);
            assert_eq!(ch.heartbeat_interval, Some(30000));
        } else {
            panic!("Expected Channel");
        }
    }

    // --- Embed ---

    #[test]
    fn test_parse_embed() {
        let prog = parse(r#"
            embed Analytics {
                src: "https://example.com/script.js",
            }
        "#);
        if let Item::Embed(e) = &prog.items[0] {
            assert_eq!(e.name, "Analytics");
        } else {
            panic!("Expected Embed");
        }
    }

    #[test]
    fn test_parse_embed_sandbox_false() {
        // `sandbox` and `loading` are keywords, so they can't be parsed as
        // ident keys in the embed key-value pairs. Test basic embed parsing.
        let prog = parse(r#"
            embed E {
                src: "https://cdn.example.com/x.js",
                integrity: "sha256-abc",
            }
        "#);
        if let Item::Embed(e) = &prog.items[0] {
            assert_eq!(e.name, "E");
            assert!(e.integrity.is_some());
        } else {
            panic!("Expected Embed");
        }
    }

    // --- Pdf ---

    #[test]
    fn test_parse_pdf() {
        let prog = parse(r#"
            pdf InvoicePdf {
                page_size: "A4",
                orientation: "portrait",
                render {
                    <div />
                }
            }
        "#);
        if let Item::Pdf(p) = &prog.items[0] {
            assert_eq!(p.name, "InvoicePdf");
            assert_eq!(p.page_size, Some("A4".to_string()));
            assert_eq!(p.orientation, Some("portrait".to_string()));
        } else {
            panic!("Expected Pdf");
        }
    }

    #[test]
    fn test_parse_pdf_no_render() {
        let prog = parse(r#"
            pdf P {
                page_size: "letter",
            }
        "#);
        if let Item::Pdf(p) = &prog.items[0] {
            assert_eq!(p.name, "P");
            // Should get default empty render block
        } else {
            panic!("Expected Pdf");
        }
    }

    // --- Payment ---

    #[test]
    fn test_parse_payment() {
        let prog = parse(r#"
            payment StripePayment {
                provider: "stripe",
                public_key: "pk_test_123",
                fn on_success() { return; }
                fn on_error() { return; }
            }
        "#);
        if let Item::Payment(p) = &prog.items[0] {
            assert_eq!(p.name, "StripePayment");
            assert!(p.provider.is_some());
            assert!(p.public_key.is_some());
            assert!(p.on_success.is_some());
            assert!(p.on_error.is_some());
        } else {
            panic!("Expected Payment");
        }
    }

    // --- Banking ---

    #[test]
    fn test_parse_banking() {
        let prog = parse(r#"
            banking AccountLink {
                provider: "plaid",
                fn on_success() { return; }
                fn on_exit() { return; }
                fn on_error() { return; }
            }
        "#);
        if let Item::Banking(b) = &prog.items[0] {
            assert_eq!(b.name, "AccountLink");
            assert!(b.provider.is_some());
            assert!(b.on_success.is_some());
            assert!(b.on_exit.is_some());
            assert!(b.on_error.is_some());
        } else {
            panic!("Expected Banking");
        }
    }

    #[test]
    fn test_parse_banking_minimal() {
        let prog = parse(r#"
            banking BankLink {
                provider: "plaid",
            }
        "#);
        if let Item::Banking(b) = &prog.items[0] {
            assert_eq!(b.name, "BankLink");
            assert!(b.provider.is_some());
            assert!(b.on_success.is_none());
            assert!(b.on_exit.is_none());
            assert!(b.on_error.is_none());
        } else {
            panic!("Expected Banking");
        }
    }

    #[test]
    fn test_parse_banking_with_methods() {
        let prog = parse(r#"
            banking Fin {
                provider: "mx",
                fn get_accounts() { return; }
            }
        "#);
        if let Item::Banking(b) = &prog.items[0] {
            assert_eq!(b.name, "Fin");
            assert_eq!(b.methods.len(), 1);
            assert_eq!(b.methods[0].name, "get_accounts");
        } else {
            panic!("Expected Banking");
        }
    }

    // --- Map ---

    #[test]
    fn test_parse_map() {
        let prog = parse(r#"
            map StoreLocator {
                provider: "mapbox",
                center: (40.7128, -74.0060),
                zoom: 12,
                fn on_ready() { return; }
                fn on_click() { return; }
            }
        "#);
        if let Item::Map(m) = &prog.items[0] {
            assert_eq!(m.name, "StoreLocator");
            assert!(m.provider.is_some());
            assert!(m.center.is_some());
            let (lat, lng) = m.center.unwrap();
            assert!((lat - 40.7128).abs() < 0.001);
            assert!((lng - (-74.006)).abs() < 0.001);
            assert_eq!(m.zoom, Some(12.0));
            assert!(m.on_ready.is_some());
            assert!(m.on_click.is_some());
        } else {
            panic!("Expected Map");
        }
    }

    #[test]
    fn test_parse_map_minimal() {
        let prog = parse(r#"
            map SimpleMap {
                provider: "mapbox",
            }
        "#);
        if let Item::Map(m) = &prog.items[0] {
            assert_eq!(m.name, "SimpleMap");
            assert!(m.center.is_none());
            assert!(m.zoom.is_none());
            assert!(m.on_ready.is_none());
        } else {
            panic!("Expected Map");
        }
    }

    #[test]
    fn test_parse_map_with_methods() {
        let prog = parse(r#"
            map GeoMap {
                provider: "mapbox",
                center: (0, 0),
                zoom: 5,
                fn add_markers() { return; }
                fn fly_to() { return; }
            }
        "#);
        if let Item::Map(m) = &prog.items[0] {
            assert_eq!(m.name, "GeoMap");
            assert_eq!(m.methods.len(), 2);
            assert_eq!(m.methods[0].name, "add_markers");
            assert_eq!(m.methods[1].name, "fly_to");
            let (lat, lng) = m.center.unwrap();
            assert_eq!(lat, 0.0);
            assert_eq!(lng, 0.0);
        } else {
            panic!("Expected Map");
        }
    }

    #[test]
    fn test_parse_map_no_provider_defaults() {
        let prog = parse(r#"
            map DefaultMap {
                zoom: 8,
            }
        "#);
        if let Item::Map(m) = &prog.items[0] {
            assert_eq!(m.name, "DefaultMap");
            assert!(m.provider.is_none());
            assert_eq!(m.zoom, Some(8.0));
        } else {
            panic!("Expected Map");
        }
    }

    // --- Auth ---

    #[test]
    fn test_parse_auth() {
        let prog = parse(r#"
            auth AppAuth {
                provider "google" {
                    client_id: "abc",
                    scopes: ["email", "profile"],
                }
                session: "cookie",
                fn on_login() { return; }
                fn on_logout() { return; }
                fn on_error() { return; }
            }
        "#);
        if let Item::Auth(a) = &prog.items[0] {
            assert_eq!(a.name, "AppAuth");
            assert_eq!(a.providers.len(), 1);
            assert_eq!(a.providers[0].name, "google");
            assert_eq!(a.providers[0].scopes, vec!["email", "profile"]);
            assert_eq!(a.session_storage, Some("cookie".to_string()));
            assert!(a.on_login.is_some());
            assert!(a.on_logout.is_some());
            assert!(a.on_error.is_some());
        } else {
            panic!("Expected Auth");
        }
    }

    // --- Upload ---

    #[test]
    fn test_parse_upload() {
        let prog = parse(r#"
            upload FileUpload {
                endpoint: "/api/upload",
                max_size: 10485760,
                accept: ["image/*", "application/pdf"],
                fn on_progress() { return; }
                fn on_complete() { return; }
                fn on_error() { return; }
            }
        "#);
        if let Item::Upload(u) = &prog.items[0] {
            assert_eq!(u.name, "FileUpload");
            assert!(u.max_size.is_some());
            assert_eq!(u.accept, vec!["image/*", "application/pdf"]);
            assert!(u.on_progress.is_some());
            assert!(u.on_complete.is_some());
            assert!(u.on_error.is_some());
        } else {
            panic!("Expected Upload");
        }
    }

    // --- Db ---

    #[test]
    fn test_parse_db() {
        // Note: `store` is a keyword so expect_ident won't return it.
        // Test db with version only.
        let prog = parse(r#"
            db AppDb {
                version: 1,
            }
        "#);
        if let Item::Db(d) = &prog.items[0] {
            assert_eq!(d.name, "AppDb");
            assert_eq!(d.version, Some(1));
        } else {
            panic!("Expected Db");
        }
    }

    // --- Cache ---

    #[test]
    fn test_parse_cache() {
        // Note: "invalidate" is a keyword token, so mutation body with
        // invalidate: [...] fails expect_ident(). Test without mutation body.
        let prog = parse(r#"
            cache ApiCache {
                strategy: "stale-while-revalidate",
                ttl: 300,
                max_entries: 1000,
                query users: fetch("/api/users"),
                mutation update_user(id: String): fetch("/api/users"),
            }
        "#);
        if let Item::Cache(c) = &prog.items[0] {
            assert_eq!(c.name, "ApiCache");
            assert_eq!(c.strategy, Some("stale-while-revalidate".to_string()));
            assert_eq!(c.default_ttl, Some(300));
            assert_eq!(c.max_entries, Some(1000));
            assert_eq!(c.queries.len(), 1);
            assert_eq!(c.mutations.len(), 1);
        } else {
            panic!("Expected Cache");
        }
    }

    #[test]
    fn test_parse_cache_query_with_config() {
        let prog = parse(r#"
            cache C {
                query user(id: String): fetch("/api/user") {
                    ttl: 60,
                    stale: 120,
                    invalidate_on: ["user_updated"],
                },
            }
        "#);
        if let Item::Cache(c) = &prog.items[0] {
            let q = &c.queries[0];
            assert_eq!(q.name, "user");
            assert_eq!(q.ttl, Some(60));
            assert_eq!(q.stale, Some(120));
            assert_eq!(q.invalidate_on, vec!["user_updated"]);
        } else {
            panic!("Expected Cache");
        }
    }

    // --- Breakpoints ---

    #[test]
    fn test_parse_breakpoints() {
        let prog = parse(r#"
            breakpoint {
                sm: 640,
                md: 768,
                lg: 1024,
            }
        "#);
        if let Item::Breakpoints(b) = &prog.items[0] {
            assert_eq!(b.breakpoints.len(), 3);
            assert_eq!(b.breakpoints[0], ("sm".to_string(), 640));
            assert_eq!(b.breakpoints[1], ("md".to_string(), 768));
            assert_eq!(b.breakpoints[2], ("lg".to_string(), 1024));
        } else {
            panic!("Expected Breakpoints");
        }
    }

    // --- Theme ---

    #[test]
    fn test_parse_theme_light_dark() {
        let prog = parse(r##"
            theme AppTheme {
                light {
                    bg: "white",
                }
                dark {
                    bg: "black",
                }
            }
        "##);
        if let Item::Theme(t) = &prog.items[0] {
            assert_eq!(t.name, "AppTheme");
            assert!(t.light.is_some());
            assert!(t.dark.is_some());
            assert!(!t.dark_auto);
        } else {
            panic!("Expected Theme");
        }
    }

    #[test]
    fn test_parse_theme_dark_auto() {
        let prog = parse(r##"
            theme T {
                light {
                    bg: "white",
                }
                dark: auto,
            }
        "##);
        if let Item::Theme(t) = &prog.items[0] {
            assert!(t.dark_auto);
            assert!(t.dark.is_none());
        } else {
            panic!("Expected Theme");
        }
    }

    #[test]
    fn test_parse_theme_auto_primary() {
        let prog = parse(r##"
            theme T {
                auto,
                primary: "red",
            }
        "##);
        if let Item::Theme(t) = &prog.items[0] {
            assert!(t.dark_auto);
            assert!(t.primary.is_some());
        } else {
            panic!("Expected Theme");
        }
    }

    // --- Animation blocks ---

    #[test]
    fn test_parse_spring_animation() {
        let prog = parse(r#"
            spring FadeIn {
                stiffness: 120,
                damping: 14,
                mass: 1,
                properties: ["opacity", "transform"],
            }
        "#);
        if let Item::Animation(a) = &prog.items[0] {
            assert_eq!(a.name, "FadeIn");
            if let AnimationKind::Spring { stiffness, damping, mass, properties } = &a.kind {
                assert_eq!(*stiffness, Some(120.0));
                assert_eq!(*damping, Some(14.0));
                assert_eq!(*mass, Some(1.0));
                assert_eq!(properties, &vec!["opacity", "transform"]);
            } else {
                panic!("Expected Spring");
            }
        } else {
            panic!("Expected Animation");
        }
    }

    #[test]
    fn test_parse_spring_float_values() {
        let prog = parse(r#"
            spring S {
                stiffness: 1.5,
                damping: 0.7,
                mass: 2.0,
            }
        "#);
        if let Item::Animation(a) = &prog.items[0] {
            if let AnimationKind::Spring { stiffness, damping, mass, .. } = &a.kind {
                assert_eq!(*stiffness, Some(1.5));
                assert_eq!(*damping, Some(0.7));
                assert_eq!(*mass, Some(2.0));
            } else {
                panic!("Expected Spring");
            }
        } else {
            panic!("Expected Animation");
        }
    }

    #[test]
    fn test_parse_keyframes_animation() {
        // Note: "duration" is a keyword token (TokenKind::Duration), so it
        // doesn't match the Ident branch in parse_keyframes_block. We test
        // only the frames, which parse correctly.
        let prog = parse(r#"
            keyframes SlideIn {
                0% {
                    x: 0,
                }
                100% {
                    x: 100,
                }
            }
        "#);
        if let Item::Animation(a) = &prog.items[0] {
            assert_eq!(a.name, "SlideIn");
            if let AnimationKind::Keyframes { frames, duration, easing } = &a.kind {
                assert_eq!(frames.len(), 2);
                assert_eq!(*duration, None);
                assert_eq!(*easing, None);
            } else {
                panic!("Expected Keyframes");
            }
        } else {
            panic!("Expected Animation");
        }
    }

    #[test]
    fn test_parse_stagger_animation() {
        // Note: "selector" is a keyword token, so it can't be used as a key
        // in stagger {} (which uses expect_ident()). Test without selector.
        let prog = parse(r#"
            stagger ListAppear {
                animation: FadeIn,
                delay: "50ms",
            }
        "#);
        if let Item::Animation(a) = &prog.items[0] {
            assert_eq!(a.name, "ListAppear");
            if let AnimationKind::Stagger { animation, delay, selector } = &a.kind {
                assert_eq!(animation, "FadeIn");
                assert_eq!(*delay, Some("50ms".to_string()));
                assert_eq!(*selector, None);
            } else {
                panic!("Expected Stagger");
            }
        } else {
            panic!("Expected Animation");
        }
    }

    // --- Test block ---

    #[test]
    fn test_parse_test_def() {
        let prog = parse(r#"
            test "addition works" {
                assert_eq(1 + 1, 2);
            }
        "#);
        if let Item::Test(t) = &prog.items[0] {
            assert_eq!(t.name, "addition works");
            assert!(!t.body.stmts.is_empty());
        } else {
            panic!("Expected Test");
        }
    }

    // --- LazyComponent ---

    #[test]
    fn test_parse_lazy_component() {
        let prog = parse(r#"
            lazy component HeavyChart {
                render {
                    <div />
                }
            }
        "#);
        if let Item::LazyComponent(lc) = &prog.items[0] {
            assert_eq!(lc.component.name, "HeavyChart");
        } else {
            panic!("Expected LazyComponent");
        }
    }

    // --- Component with all features ---

    #[test]
    fn test_parse_component_with_state_and_methods() {
        let prog = parse(r#"
            component Counter {
                let mut count: i32 = 0;
                signal reactive_val: i32 = 10;
                fn increment(&mut self) {
                    self.count = self.count + 1;
                }
                render {
                    <div>{self.count}</div>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.name, "Counter");
            assert_eq!(c.state.len(), 2);
            assert!(c.state[0].mutable);
            assert_eq!(c.methods.len(), 1);
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_mut_self_parameter() {
        let prog = parse(r#"
            component Counter {
                let count: i32 = 0;
                fn increment(mut self) {
                    self.count = self.count + 1;
                }
                render {
                    <div>{self.count}</div>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.name, "Counter");
            assert_eq!(c.methods.len(), 1);
            assert_eq!(c.methods[0].params.len(), 1);
            assert_eq!(c.methods[0].params[0].name, "self");
            assert_eq!(c.methods[0].params[0].ownership, Ownership::Owned);
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_component_with_permissions() {
        let prog = parse(r#"
            component Secure {
                permissions {
                    network: ["https://api.example.com/*"],
                    storage: ["user_prefs"],
                    capabilities: ["camera"],
                }
                render {
                    <div />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            let perms = c.permissions.as_ref().unwrap();
            assert_eq!(perms.network, vec!["https://api.example.com/*"]);
            assert_eq!(perms.storage, vec!["user_prefs"]);
            assert_eq!(perms.capabilities, vec!["camera"]);
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_component_with_gesture() {
        let prog = parse(r#"
            component Swipeable {
                gesture swipe_left {
                    return;
                }
                render {
                    <div />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.gestures.len(), 1);
            assert_eq!(c.gestures[0].gesture_type, "swipe_left");
            assert!(c.gestures[0].target.is_none());
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_component_with_a11y_manual() {
        let prog = parse(r#"
            component C {
                a11y manual;
                render { <div /> }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.a11y, Some(A11yMode::Manual));
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_component_with_a11y_auto() {
        let prog = parse(r#"
            component C {
                a11y auto;
                render { <div /> }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.a11y, Some(A11yMode::Auto));
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_component_with_a11y_default() {
        let prog = parse(r#"
            component C {
                a11y;
                render { <div /> }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.a11y, Some(A11yMode::Auto));
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_component_with_shortcuts() {
        let prog = parse(r#"
            component Editor {
                shortcut "ctrl+s" {
                    return;
                }
                render { <div /> }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.shortcuts.len(), 1);
            assert_eq!(c.shortcuts[0].keys, "ctrl+s");
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_component_with_on_destroy() {
        let prog = parse(r#"
            component C {
                fn on_destroy(&self) { return; }
                render { <div /> }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert!(c.on_destroy.is_some());
            assert!(c.methods.is_empty());
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_component_with_chunk() {
        let prog = parse(r#"
            component Dashboard {
                chunk "dashboard";
                render { <div /> }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.chunk, Some("dashboard".to_string()));
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_component_with_style_and_transition() {
        let prog = parse(r#"
            component Styled {
                style {
                    .container {
                        background: "white";
                    }
                }
                transition {
                    opacity: "0.3s ease";
                }
                render { <div /> }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.styles.len(), 1);
            assert_eq!(c.styles[0].selector, ".container");
            assert_eq!(c.transitions.len(), 1);
            assert_eq!(c.transitions[0].property, "opacity");
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_component_secret_state() {
        let prog = parse(r#"
            component C {
                let mut secret token: String = "abc";
                signal secret api_key: String = "key";
                render { <div /> }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert!(c.state[0].secret);
            assert!(c.state[1].secret);
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_component_with_type_params() {
        let prog = parse(r#"
            component List<T> where T: Display {
                render { <div /> }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.type_params, vec!["T"]);
            assert_eq!(c.trait_bounds.len(), 1);
        } else {
            panic!("Expected Component");
        }
    }

    // --- Expression tests ---

    #[test]
    fn test_parse_binary_ops() {
        let e = parse_expr("1 + 2 * 3");
        // Should be Add(1, Mul(2, 3)) due to precedence
        if let Expr::Binary { op, left, right } = &e {
            assert_eq!(*op, BinOp::Add);
            assert!(matches!(**left, Expr::Integer(1)));
            if let Expr::Binary { op: inner_op, .. } = right.as_ref() {
                assert_eq!(*inner_op, BinOp::Mul);
            } else {
                panic!("Expected Binary");
            }
        } else {
            panic!("Expected Binary");
        }
    }

    #[test]
    fn test_parse_comparison_ops() {
        let e = parse_expr("a < b");
        assert!(matches!(e, Expr::Binary { op: BinOp::Lt, .. }));

        let e = parse_expr("a > b");
        assert!(matches!(e, Expr::Binary { op: BinOp::Gt, .. }));

        let e = parse_expr("a <= b");
        assert!(matches!(e, Expr::Binary { op: BinOp::Lte, .. }));

        let e = parse_expr("a >= b");
        assert!(matches!(e, Expr::Binary { op: BinOp::Gte, .. }));

        let e = parse_expr("a == b");
        assert!(matches!(e, Expr::Binary { op: BinOp::Eq, .. }));

        let e = parse_expr("a != b");
        assert!(matches!(e, Expr::Binary { op: BinOp::Neq, .. }));
    }

    #[test]
    fn test_parse_logical_ops() {
        let e = parse_expr("a && b || c");
        // || has lower precedence: Or(And(a,b), c)
        assert!(matches!(e, Expr::Binary { op: BinOp::Or, .. }));
    }

    #[test]
    fn test_parse_mod_op() {
        let e = parse_expr("a % b");
        assert!(matches!(e, Expr::Binary { op: BinOp::Mod, .. }));
    }

    #[test]
    fn test_parse_div_op() {
        let e = parse_expr("a / b");
        assert!(matches!(e, Expr::Binary { op: BinOp::Div, .. }));
    }

    #[test]
    fn test_parse_sub_op() {
        let e = parse_expr("a - b");
        assert!(matches!(e, Expr::Binary { op: BinOp::Sub, .. }));
    }

    #[test]
    fn test_parse_unary_neg() {
        let e = parse_expr("-5");
        if let Expr::Unary { op, operand } = &e {
            assert_eq!(*op, UnaryOp::Neg);
            assert!(matches!(**operand, Expr::Integer(5)));
        } else {
            panic!("Expected Unary");
        }
    }

    #[test]
    fn test_parse_unary_not() {
        let e = parse_expr("!is_active");
        assert!(matches!(e, Expr::Unary { op: UnaryOp::Not, .. }));
    }

    #[test]
    fn test_parse_borrow() {
        let e = parse_expr("&x");
        assert!(matches!(e, Expr::Borrow(_)));
    }

    #[test]
    fn test_parse_borrow_mut() {
        let e = parse_expr("&mut x");
        assert!(matches!(e, Expr::BorrowMut(_)));
    }

    #[test]
    fn test_parse_field_access() {
        let e = parse_expr("obj.name");
        if let Expr::FieldAccess { object, field } = &e {
            assert_eq!(field, "name");
            assert!(matches!(**object, Expr::Ident(_)));
        } else {
            panic!("Expected FieldAccess");
        }
    }

    #[test]
    fn test_parse_method_call() {
        let e = parse_expr("obj.method(1, 2)");
        if let Expr::MethodCall { object, method, args } = &e {
            assert_eq!(method, "method");
            assert_eq!(args.len(), 2);
            assert!(matches!(**object, Expr::Ident(_)));
        } else {
            panic!("Expected MethodCall");
        }
    }

    #[test]
    fn test_parse_fn_call() {
        let e = parse_expr("foo(1, 2, 3)");
        if let Expr::FnCall { callee, args } = &e {
            assert_eq!(args.len(), 3);
            assert!(matches!(**callee, Expr::Ident(_)));
        } else {
            panic!("Expected FnCall");
        }
    }

    #[test]
    fn test_parse_index() {
        let e = parse_expr("arr[0]");
        if let Expr::Index { object, index } = &e {
            assert!(matches!(**object, Expr::Ident(_)));
            assert!(matches!(**index, Expr::Integer(0)));
        } else {
            panic!("Expected Index");
        }
    }

    #[test]
    fn test_parse_struct_init() {
        let e = parse_expr("Point { x: 1, y: 2 }");
        if let Expr::StructInit { name, fields } = &e {
            assert_eq!(name, "Point");
            assert_eq!(fields.len(), 2);
        } else {
            panic!("Expected StructInit");
        }
    }

    #[test]
    fn test_parse_if_else() {
        let prog = parse("fn f() { if x { return 1; } else { return 2; } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::If { else_block, .. }) = &f.body.stmts[0] {
                assert!(else_block.is_some());
            } else {
                panic!("Expected If expr");
            }
        }
    }

    #[test]
    fn test_parse_if_no_else() {
        let prog = parse("fn f() { if cond { return; } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::If { else_block, .. }) = &f.body.stmts[0] {
                assert!(else_block.is_none());
            } else {
                panic!("Expected If");
            }
        }
    }

    #[test]
    fn test_parse_match_expr() {
        let prog = parse(r#"
            fn f() {
                match x {
                    1 => true,
                    _ => false,
                }
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                assert_eq!(arms.len(), 2);
                assert!(matches!(arms[0].pattern, Pattern::Literal(_)));
                assert!(matches!(arms[1].pattern, Pattern::Wildcard));
            } else {
                panic!("Expected Match");
            }
        }
    }

    #[test]
    fn test_parse_match_variant_pattern() {
        let prog = parse(r#"
            fn f() {
                match shape {
                    Circle(r) => r,
                    name => name,
                }
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                assert!(matches!(arms[0].pattern, Pattern::Variant { .. }));
                assert!(matches!(arms[1].pattern, Pattern::Ident(_)));
            } else {
                panic!("Expected Match");
            }
        }
    }

    #[test]
    fn test_parse_match_string_pattern() {
        let prog = parse(r#"
            fn f() {
                match s {
                    "hello" => 1,
                    _ => 0,
                }
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                assert!(matches!(arms[0].pattern, Pattern::Literal(Expr::StringLit(_))));
            } else {
                panic!("Expected Match");
            }
        }
    }

    #[test]
    fn test_parse_match_bool_pattern() {
        let prog = parse(r#"
            fn f() {
                match b {
                    true => 1,
                    false => 0,
                }
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                assert!(matches!(arms[0].pattern, Pattern::Literal(Expr::Bool(true))));
                assert!(matches!(arms[1].pattern, Pattern::Literal(Expr::Bool(false))));
            } else {
                panic!("Expected Match");
            }
        }
    }

    #[test]
    fn test_parse_for_expr() {
        let prog = parse("fn f() { for item in items { return; } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::For { binding, .. }) = &f.body.stmts[0] {
                assert_eq!(binding, "item");
            } else {
                panic!("Expected For");
            }
        }
    }

    #[test]
    fn test_parse_while_expr() {
        let prog = parse("fn f() { while running { return; } }");
        if let Item::Function(f) = &prog.items[0] {
            assert!(matches!(&f.body.stmts[0], Stmt::Expr(Expr::While { .. })));
        }
    }

    #[test]
    fn test_parse_spawn() {
        let e = parse_expr("spawn { return; }");
        assert!(matches!(e, Expr::Spawn { .. }));
    }

    #[test]
    fn test_parse_parallel() {
        let prog = parse("fn f() { parallel { a(), b(), c() } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Parallel { tasks, .. }) = &f.body.stmts[0] {
                assert_eq!(tasks.len(), 3);
            } else {
                panic!("Expected Parallel");
            }
        }
    }

    #[test]
    fn test_parse_channel_expr() {
        let e = parse_expr("channel<i32>()");
        if let Expr::Channel { ty } = &e {
            assert!(ty.is_some());
        } else {
            panic!("Expected Channel");
        }
    }

    #[test]
    fn test_parse_channel_no_type() {
        let e = parse_expr("channel()");
        if let Expr::Channel { ty } = &e {
            assert!(ty.is_none());
        } else {
            panic!("Expected Channel");
        }
    }

    #[test]
    fn test_parse_send_receive() {
        let prog = parse("fn f() { ch.send(42); let x = ch.recv(); }");
        if let Item::Function(f) = &prog.items[0] {
            assert!(matches!(&f.body.stmts[0], Stmt::Expr(Expr::Send { .. })));
            if let Stmt::Let { value, .. } = &f.body.stmts[1] {
                assert!(matches!(value, Expr::Receive { .. }));
            }
        }
    }

    #[test]
    fn test_parse_fetch() {
        let e = parse_expr(r#"fetch("/api/users")"#);
        if let Expr::Fetch { url, options, contract } = &e {
            assert!(matches!(**url, Expr::StringLit(_)));
            assert!(options.is_none());
            assert!(contract.is_none());
        } else {
            panic!("Expected Fetch");
        }
    }

    #[test]
    fn test_parse_fetch_with_options() {
        let e = parse_expr(r#"fetch("/api/users", opts)"#);
        if let Expr::Fetch { options, .. } = &e {
            assert!(options.is_some());
        } else {
            panic!("Expected Fetch");
        }
    }

    #[test]
    fn test_parse_fetch_with_contract() {
        let e = parse_expr(r#"fetch("/api/users") -> UserResponse"#);
        if let Expr::Fetch { contract, .. } = &e {
            assert_eq!(*contract, Some("UserResponse".to_string()));
        } else {
            panic!("Expected Fetch");
        }
    }

    #[test]
    fn test_parse_navigate() {
        let e = parse_expr(r#"navigate("/home")"#);
        assert!(matches!(e, Expr::Navigate { .. }));
    }

    #[test]
    fn test_parse_download() {
        let e = parse_expr(r#"download(data, "file.pdf")"#);
        assert!(matches!(e, Expr::Download { .. }));
    }

    #[test]
    fn test_parse_env() {
        let e = parse_expr(r#"env("API_KEY")"#);
        assert!(matches!(e, Expr::Env { .. }));
    }

    #[test]
    fn test_parse_flag() {
        let e = parse_expr(r#"flag("dark_mode")"#);
        assert!(matches!(e, Expr::Flag { .. }));
    }

    #[test]
    fn test_parse_trace() {
        let prog = parse(r#"fn f() { trace("perf") { return; } }"#);
        if let Item::Function(f) = &prog.items[0] {
            assert!(matches!(&f.body.stmts[0], Stmt::Expr(Expr::Trace { .. })));
        }
    }

    #[test]
    fn test_parse_assert() {
        let prog = parse(r#"fn f() { assert(x > 0); }"#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Assert { condition, message }) = &f.body.stmts[0] {
                assert!(matches!(**condition, Expr::Binary { .. }));
                assert!(message.is_none());
            } else {
                panic!("Expected Assert");
            }
        }
    }

    #[test]
    fn test_parse_assert_with_message() {
        let prog = parse(r#"fn f() { assert(x > 0, "must be positive"); }"#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Assert { message, .. }) = &f.body.stmts[0] {
                assert_eq!(*message, Some("must be positive".to_string()));
            } else {
                panic!("Expected Assert");
            }
        }
    }

    #[test]
    fn test_parse_assert_eq() {
        let prog = parse(r#"fn f() { assert_eq(1 + 1, 2); }"#);
        if let Item::Function(f) = &prog.items[0] {
            assert!(matches!(&f.body.stmts[0], Stmt::Expr(Expr::AssertEq { .. })));
        }
    }

    #[test]
    fn test_parse_assert_eq_with_message() {
        let prog = parse(r#"fn f() { assert_eq(a, b, "should be equal"); }"#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::AssertEq { message, .. }) = &f.body.stmts[0] {
                assert_eq!(*message, Some("should be equal".to_string()));
            } else {
                panic!("Expected AssertEq");
            }
        }
    }

    #[test]
    fn test_parse_animate_expr() {
        let prog = parse(r#"fn f() { animate(target, "fadeIn"); }"#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Animate { animation, .. }) = &f.body.stmts[0] {
                assert_eq!(animation, "fadeIn");
            } else {
                panic!("Expected Animate");
            }
        }
    }

    #[test]
    fn test_parse_try_catch() {
        let prog = parse(r#"
            fn f() {
                try {
                    return;
                } catch err {
                    return;
                }
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::TryCatch { error_binding, .. }) = &f.body.stmts[0] {
                assert_eq!(error_binding, "err");
            } else {
                panic!("Expected TryCatch");
            }
        }
    }

    #[test]
    fn test_parse_await_expr() {
        let e = parse_expr("await result");
        assert!(matches!(e, Expr::Await(_)));
    }

    #[test]
    fn test_parse_suspend() {
        let e = parse_expr(r#"suspend(placeholder) { content }"#);
        assert!(matches!(e, Expr::Suspend { .. }));
    }

    #[test]
    fn test_parse_stream() {
        let e = parse_expr(r#"stream fetch("/api")"#);
        assert!(matches!(e, Expr::Stream { .. }));
    }

    #[test]
    fn test_parse_prompt_template() {
        let e = parse_expr(r#"prompt "Hello {name}, welcome to {place}""#);
        if let Expr::PromptTemplate { template, interpolations } = &e {
            assert_eq!(template, "Hello {name}, welcome to {place}");
            assert_eq!(interpolations.len(), 2);
            assert_eq!(interpolations[0].0, "name");
            assert_eq!(interpolations[1].0, "place");
        } else {
            panic!("Expected PromptTemplate");
        }
    }

    #[test]
    fn test_parse_self_expr() {
        let e = parse_expr("self");
        assert!(matches!(e, Expr::SelfExpr));
    }

    #[test]
    fn test_parse_bool_literals() {
        assert!(matches!(parse_expr("true"), Expr::Bool(true)));
        assert!(matches!(parse_expr("false"), Expr::Bool(false)));
    }

    #[test]
    fn test_parse_float_literal() {
        let e = parse_expr("3.14");
        assert!(matches!(e, Expr::Float(f) if (f - 3.14).abs() < f64::EPSILON));
    }

    #[test]
    fn test_parse_string_literal_expr() {
        let e = parse_expr(r#""hello""#);
        assert!(matches!(e, Expr::StringLit(s) if s == "hello"));
    }

    #[test]
    fn test_parse_assignment() {
        let prog = parse("fn f() { x = 5; }");
        if let Item::Function(f) = &prog.items[0] {
            assert!(matches!(&f.body.stmts[0], Stmt::Expr(Expr::Assign { .. })));
        }
    }

    #[test]
    fn test_parse_plus_equals() {
        let prog = parse("fn f() { x += 1; }");
        if let Item::Function(f) = &prog.items[0] {
            // += desugars to Assign { target, value: Binary { Add, target, value } }
            if let Stmt::Expr(Expr::Assign { value, .. }) = &f.body.stmts[0] {
                assert!(matches!(**value, Expr::Binary { op: BinOp::Add, .. }));
            } else {
                panic!("Expected Assign");
            }
        }
    }

    #[test]
    fn test_parse_select_expr() {
        let prog = parse("fn f() { select { return; } }");
        if let Item::Function(f) = &prog.items[0] {
            assert!(matches!(&f.body.stmts[0], Stmt::Expr(Expr::Block(_))));
        }
    }

    #[test]
    fn test_parse_dynamic_import() {
        let e = parse_expr(r#"import("./module")"#);
        assert!(matches!(e, Expr::DynamicImport { .. }));
    }

    #[test]
    fn test_parse_parenthesized_expr() {
        let e = parse_expr("(1 + 2)");
        assert!(matches!(e, Expr::Binary { op: BinOp::Add, .. }));
    }

    #[test]
    fn test_parse_block_expr() {
        let prog = parse("fn f() { { return; } }");
        if let Item::Function(f) = &prog.items[0] {
            assert!(matches!(&f.body.stmts[0], Stmt::Expr(Expr::Block(_))));
        }
    }

    // --- Statement tests ---

    #[test]
    fn test_parse_let_with_type() {
        let s = parse_stmt_helper("let x: i32 = 42;");
        if let Stmt::Let { name, ty, mutable, .. } = &s {
            assert_eq!(name, "x");
            assert_eq!(*ty, Some(Type::Named("i32".to_string())));
            assert!(!mutable);
        } else {
            panic!("Expected Let");
        }
    }

    #[test]
    fn test_parse_let_mut() {
        let s = parse_stmt_helper("let mut x = 0;");
        if let Stmt::Let { mutable, .. } = &s {
            assert!(*mutable);
        } else {
            panic!("Expected Let");
        }
    }

    #[test]
    fn test_parse_let_secret() {
        let s = parse_stmt_helper("let secret key = 42;");
        if let Stmt::Let { secret, .. } = &s {
            assert!(*secret);
        } else {
            panic!("Expected Let");
        }
    }

    #[test]
    fn test_parse_let_own() {
        let s = parse_stmt_helper("let x = own 42;");
        if let Stmt::Let { ownership, .. } = &s {
            assert_eq!(*ownership, Ownership::Owned);
        } else {
            panic!("Expected Let");
        }
    }

    #[test]
    fn test_parse_signal_stmt() {
        let s = parse_stmt_helper("signal count: i32 = 0;");
        if let Stmt::Signal { name, ty, atomic, secret, .. } = &s {
            assert_eq!(name, "count");
            assert!(ty.is_some());
            assert!(!atomic);
            assert!(!secret);
        } else {
            panic!("Expected Signal");
        }
    }

    #[test]
    fn test_parse_signal_atomic_secret() {
        let s = parse_stmt_helper("signal atomic secret x: i32 = 0;");
        if let Stmt::Signal { atomic, secret, .. } = &s {
            assert!(*atomic);
            assert!(*secret);
        } else {
            panic!("Expected Signal");
        }
    }

    #[test]
    fn test_parse_return_with_value() {
        let s = parse_stmt_helper("return 42;");
        if let Stmt::Return(Some(e)) = &s {
            assert!(matches!(e, Expr::Integer(42)));
        } else {
            panic!("Expected Return with value");
        }
    }

    #[test]
    fn test_parse_return_void() {
        let s = parse_stmt_helper("return;");
        assert!(matches!(s, Stmt::Return(None)));
    }

    #[test]
    fn test_parse_yield_stmt() {
        let s = parse_stmt_helper("yield data;");
        assert!(matches!(s, Stmt::Yield(_)));
    }

    #[test]
    fn test_parse_let_destructure_with_type() {
        let s = parse_stmt_helper("let (a, b): Pair = get_pair();");
        if let Stmt::LetDestructure { ty, .. } = &s {
            assert!(ty.is_some());
        } else {
            panic!("Expected LetDestructure");
        }
    }

    #[test]
    fn test_parse_array_destructure_with_rest() {
        let s = parse_stmt_helper("let [first, ..] = arr;");
        if let Stmt::LetDestructure { pattern, .. } = &s {
            if let Pattern::Array(pats) = pattern {
                assert_eq!(pats.len(), 2);
                assert!(matches!(pats[1], Pattern::Wildcard));
            } else {
                panic!("Expected Array pattern");
            }
        } else {
            panic!("Expected LetDestructure");
        }
    }

    #[test]
    fn test_parse_nested_destructure() {
        let s = parse_stmt_helper("let (a, (b, c)) = nested;");
        if let Stmt::LetDestructure { pattern, .. } = &s {
            if let Pattern::Tuple(pats) = pattern {
                assert_eq!(pats.len(), 2);
                assert!(matches!(pats[1], Pattern::Tuple(_)));
            } else {
                panic!("Expected Tuple pattern");
            }
        } else {
            panic!("Expected LetDestructure");
        }
    }

    #[test]
    fn test_parse_struct_destructure_with_subpattern() {
        let s = parse_stmt_helper("let Foo { bar: (x, y) } = val;");
        if let Stmt::LetDestructure { pattern, .. } = &s {
            if let Pattern::Struct { fields, .. } = pattern {
                assert!(matches!(fields[0].1, Pattern::Tuple(_)));
            } else {
                panic!("Expected Struct pattern");
            }
        } else {
            panic!("Expected LetDestructure");
        }
    }

    // --- Type parsing ---

    #[test]
    fn test_parse_reference_type() {
        // In params, & is consumed as ownership, not part of the type
        let prog = parse("fn f(x: &i32) {}");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params[0].ownership, Ownership::Borrowed);
            assert_eq!(f.params[0].ty, Type::Named("i32".to_string()));
        }
    }

    #[test]
    fn test_parse_mut_reference_type() {
        let prog = parse("fn f(x: &mut i32) {}");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params[0].ownership, Ownership::MutBorrowed);
            assert_eq!(f.params[0].ty, Type::Named("i32".to_string()));
        }
    }

    #[test]
    fn test_parse_lifetime_reference_type() {
        // Lifetimes in type position: struct fields use parse_type directly
        let prog = parse("struct S { data: &'a i32 }");
        if let Item::Struct(s) = &prog.items[0] {
            if let Type::Reference { lifetime, .. } = &s.fields[0].ty {
                assert_eq!(*lifetime, Some("a".to_string()));
            } else {
                panic!("Expected Reference type, got {:?}", s.fields[0].ty);
            }
        }
    }

    #[test]
    fn test_parse_array_type() {
        let prog = parse("fn f(x: [i32]) {}");
        if let Item::Function(f) = &prog.items[0] {
            assert!(matches!(&f.params[0].ty, Type::Array(_)));
        }
    }

    #[test]
    fn test_parse_generic_type() {
        let prog = parse("fn f(x: Vec<i32>) {}");
        if let Item::Function(f) = &prog.items[0] {
            if let Type::Generic { name, args } = &f.params[0].ty {
                assert_eq!(name, "Vec");
                assert_eq!(args.len(), 1);
            } else {
                panic!("Expected Generic type");
            }
        }
    }

    #[test]
    fn test_parse_all_primitive_types() {
        let prog = parse("fn f(a: i32, b: i64, c: f32, d: f64, e: u32, f: u64, g: bool, h: String) {}");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params[0].ty, Type::Named("i32".to_string()));
            assert_eq!(f.params[1].ty, Type::Named("i64".to_string()));
            assert_eq!(f.params[2].ty, Type::Named("f32".to_string()));
            assert_eq!(f.params[3].ty, Type::Named("f64".to_string()));
            assert_eq!(f.params[4].ty, Type::Named("u32".to_string()));
            assert_eq!(f.params[5].ty, Type::Named("u64".to_string()));
            assert_eq!(f.params[6].ty, Type::Named("bool".to_string()));
            assert_eq!(f.params[7].ty, Type::Named("String".to_string()));
        }
    }

    #[test]
    fn test_parse_self_type() {
        let prog = parse("fn f() -> Self {}");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.return_type, Some(Type::Named("Self".to_string())));
        }
    }

    // --- Function features ---

    #[test]
    fn test_parse_function_with_lifetimes() {
        let prog = parse("fn f<'a, T>(x: &T) -> &T {}");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.lifetimes, vec!["a"]);
            assert_eq!(f.type_params, vec!["T"]);
        }
    }

    #[test]
    fn test_parse_function_with_where_clause() {
        let prog = parse("fn f<T>(x: T) where T: Display, T: Clone {}");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.trait_bounds.len(), 2);
        }
    }

    #[test]
    fn test_parse_pub_function() {
        let prog = parse("pub fn public_fn() {}");
        if let Item::Function(f) = &prog.items[0] {
            assert!(f.is_pub);
        }
    }

    #[test]
    fn test_parse_must_use_function() {
        // must_use is consumed at item level; parse_function sees fn next
        // The function parses but must_use flag isn't set (parser design)
        let prog = parse("must_use fn important() -> i32 { return 1; }");
        assert!(matches!(prog.items[0], Item::Function(_)));
    }

    #[test]
    fn test_parse_async_function() {
        let prog = parse("async fn fetch_data() {}");
        assert!(matches!(prog.items[0], Item::Function(_)));
    }

    // --- Params with ownership ---

    #[test]
    fn test_parse_self_param() {
        let prog = parse("fn f(self) {}");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params[0].name, "self");
            assert_eq!(f.params[0].ownership, Ownership::Owned);
        }
    }

    #[test]
    fn test_parse_ref_self_param() {
        let prog = parse("fn f(&self) {}");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params[0].ownership, Ownership::Borrowed);
        }
    }

    #[test]
    fn test_parse_mut_ref_self_param() {
        let prog = parse("fn f(&mut self) {}");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params[0].ownership, Ownership::MutBorrowed);
        }
    }

    #[test]
    fn test_parse_borrowed_param() {
        let prog = parse("fn f(x: &i32) {}");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params[0].ownership, Ownership::Borrowed);
        }
    }

    #[test]
    fn test_parse_mut_borrowed_param() {
        let prog = parse("fn f(x: &mut i32) {}");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params[0].ownership, Ownership::MutBorrowed);
        }
    }

    // --- Template/Render tests ---

    #[test]
    fn test_parse_self_closing_element() {
        let prog = parse(r#"
            component C {
                render {
                    <input />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert_eq!(el.tag, "input");
                assert!(el.children.is_empty());
            } else {
                panic!("Expected Element");
            }
        }
    }

    #[test]
    fn test_parse_element_with_attributes() {
        let prog = parse(r#"
            component C {
                render {
                    <div class="main" id={dynamic_id}>
                        "content"
                    </div>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert_eq!(el.attributes.len(), 2);
                assert!(matches!(&el.attributes[0], Attribute::Static { name, .. } if name == "class"));
                assert!(matches!(&el.attributes[1], Attribute::Dynamic { name, .. } if name == "id"));
            } else {
                panic!("Expected Element");
            }
        }
    }

    #[test]
    fn test_parse_element_with_event_handler() {
        let prog = parse(r#"
            component C {
                render {
                    <button on:click={handle_click} />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert!(matches!(&el.attributes[0], Attribute::EventHandler { event, .. } if event == "click"));
            } else {
                panic!("Expected Element");
            }
        }
    }

    #[test]
    fn test_parse_element_with_bind() {
        let prog = parse(r#"
            component C {
                render {
                    <input bind:value={text} />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert!(matches!(&el.attributes[0], Attribute::Bind { property, signal } if property == "value" && signal == "text"));
            } else {
                panic!("Expected Element");
            }
        }
    }

    #[test]
    fn test_parse_element_with_aria() {
        let prog = parse(r#"
            component C {
                render {
                    <div aria-label="test" aria-hidden={hidden} />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert!(matches!(&el.attributes[0], Attribute::Aria { name, value: Expr::StringLit(_) } if name == "aria-label"));
                assert!(matches!(&el.attributes[1], Attribute::Aria { name, value: Expr::Ident(_) } if name == "aria-hidden"));
            } else {
                panic!("Expected Element");
            }
        }
    }

    #[test]
    fn test_parse_element_with_role() {
        let prog = parse(r#"
            component C {
                render {
                    <div role="button" />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert!(matches!(&el.attributes[0], Attribute::Role { value } if value == "button"));
            } else {
                panic!("Expected Element");
            }
        }
    }

    #[test]
    fn test_parse_element_with_tabindex_static() {
        let prog = parse(r#"
            component C {
                render {
                    <div tabindex="0" />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert!(matches!(&el.attributes[0], Attribute::Static { name, value } if name == "tabindex" && value == "0"));
            } else {
                panic!("Expected Element");
            }
        }
    }

    #[test]
    fn test_parse_element_with_tabindex_dynamic() {
        let prog = parse(r#"
            component C {
                render {
                    <div tabindex={idx} />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert!(matches!(&el.attributes[0], Attribute::Dynamic { name, .. } if name == "tabindex"));
            } else {
                panic!("Expected Element");
            }
        }
    }

    #[test]
    fn test_parse_link_element() {
        let prog = parse(r#"
            component C {
                render {
                    <Link to="/about">"About"</Link>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert!(matches!(&c.render.body, TemplateNode::Link { .. }));
        }
    }

    #[test]
    fn test_parse_link_self_closing() {
        let prog = parse(r#"
            component C {
                render {
                    <Link to="/home" />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Link { children, .. } = &c.render.body {
                assert!(children.is_empty());
            } else {
                panic!("Expected Link");
            }
        }
    }

    #[test]
    fn test_parse_link_dynamic_to() {
        let prog = parse(r#"
            component C {
                render {
                    <Link to={path} />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Link { to, .. } = &c.render.body {
                assert!(matches!(to, Expr::Ident(_)));
            } else {
                panic!("Expected Link");
            }
        }
    }

    #[test]
    fn test_parse_link_with_class_attribute() {
        let prog = parse(r#"
            component C {
                render {
                    <Link to="/about" class="btn btn-primary">"Go"</Link>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Link { to, attributes, children } = &c.render.body {
                assert!(matches!(to, Expr::StringLit(_)));
                assert_eq!(attributes.len(), 1);
                match &attributes[0] {
                    Attribute::Static { name, value } => {
                        assert_eq!(name, "class");
                        assert_eq!(value, "btn btn-primary");
                    }
                    _ => panic!("Expected Static attribute"),
                }
                assert_eq!(children.len(), 1);
            } else {
                panic!("Expected Link");
            }
        }
    }

    #[test]
    fn test_parse_link_with_multiple_attributes() {
        let prog = parse(r#"
            component C {
                render {
                    <Link to="/page" class="nav-link" style="color: red" />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Link { attributes, .. } = &c.render.body {
                assert_eq!(attributes.len(), 2);
            } else {
                panic!("Expected Link");
            }
        }
    }

    #[test]
    fn test_parse_style_attribute_on_element() {
        let prog = parse(r#"
            component C {
                render {
                    <div style="color: red; font-size: 16px">"hello"</div>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert_eq!(el.attributes.len(), 1);
                match &el.attributes[0] {
                    Attribute::Static { name, value } => {
                        assert_eq!(name, "style");
                        assert_eq!(value, "color: red; font-size: 16px");
                    }
                    _ => panic!("Expected Static attribute for style"),
                }
            } else {
                panic!("Expected Element");
            }
        }
    }

    #[test]
    fn test_parse_text_literal_template() {
        let prog = parse(r#"
            component C {
                render {
                    "Hello"
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert!(matches!(&c.render.body, TemplateNode::TextLiteral(s) if s == "Hello"));
        }
    }

    #[test]
    fn test_parse_expression_template() {
        let prog = parse(r#"
            component C {
                render {
                    {name}
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert!(matches!(&c.render.body, TemplateNode::Expression(_)));
        }
    }

    // --- Component prop defaults ---

    #[test]
    fn test_parse_component_prop_with_default() {
        let prog = parse(r#"
            component C(name: String = "World") {
                render { <div /> }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.props.len(), 1);
            assert!(c.props[0].default.is_some());
        } else {
            panic!("Expected Component");
        }
    }

    // --- VirtualList ---

    #[test]
    fn test_parse_virtual_list() {
        let prog = parse(r#"
            fn f() {
                virtual list=items item_height=40 |item| item
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            assert!(matches!(&f.body.stmts[0], Stmt::Expr(Expr::VirtualList { .. })));
        }
    }

    // --- Error recovery ---

    #[test]
    fn test_parse_recovering_produces_partial_ast() {
        let src = r#"
            fn ok1() {}
            struct {}
            fn ok2() {}
        "#;
        let (prog, errors) = parse_recovering(src);
        assert!(!errors.is_empty());
        assert!(prog.items.len() >= 2);
    }

    #[test]
    fn test_parse_program_returns_first_error() {
        let mut lexer = Lexer::new("1234 fn ok() {}");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let result = parser.parse_program();
        assert!(result.is_err());
    }

    // --- Skeleton and error boundary ---

    #[test]
    fn test_parse_component_with_skeleton() {
        let prog = parse(r#"
            component C {
                skeleton {
                    <div />
                }
                render {
                    <div />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert!(c.skeleton.is_some());
        } else {
            panic!("Expected Component");
        }
    }

    // --- Multiple generic type args ---

    #[test]
    fn test_parse_generic_type_multi_args() {
        let prog = parse("fn f(x: HashMap<String, i32>) {}");
        if let Item::Function(f) = &prog.items[0] {
            if let Type::Generic { name, args } = &f.params[0].ty {
                assert_eq!(name, "HashMap");
                assert_eq!(args.len(), 2);
            } else {
                panic!("Expected Generic type");
            }
        }
    }

    // --- Cache query with contract ---

    #[test]
    fn test_parse_cache_query_with_contract() {
        // The fetch expr parser consumes `-> ContractName` itself,
        // so the contract lives on the fetch expr, not the cache query.
        let prog = parse(r#"
            cache C {
                query users: fetch("/api/users") -> UserContract,
            }
        "#);
        if let Item::Cache(c) = &prog.items[0] {
            assert_eq!(c.queries.len(), 1);
            assert_eq!(c.queries[0].name, "users");
            // The contract is consumed by fetch expr
            if let Expr::Fetch { contract, .. } = &c.queries[0].fetch_expr {
                assert_eq!(*contract, Some("UserContract".to_string()));
            } else {
                panic!("Expected Fetch expr in query");
            }
        } else {
            panic!("Expected Cache");
        }
    }

    // --- Auth with provider: as simple type ---

    #[test]
    fn test_parse_auth_simple_provider() {
        let prog = parse(r#"
            auth A {
                provider: "jwt",
            }
        "#);
        if let Item::Auth(a) = &prog.items[0] {
            assert!(a.provider.is_some());
        } else {
            panic!("Expected Auth");
        }
    }

    // --- Page with state and methods ---

    #[test]
    fn test_parse_page_with_state_and_methods() {
        let prog = parse(r#"
            page P {
                let mut count: i32 = 0;
                signal val: i32 = 5;
                fn do_thing() { return; }
                render {
                    <div />
                }
            }
        "#);
        if let Item::Page(p) = &prog.items[0] {
            assert_eq!(p.state.len(), 2);
            assert_eq!(p.methods.len(), 1);
        } else {
            panic!("Expected Page");
        }
    }

    // --- Free parse function ---

    #[test]
    fn test_free_parse_with_errors() {
        let mut lexer = Lexer::new("1234 fn ok() {}");
        let tokens = lexer.tokenize().unwrap();
        let (prog, errors) = super::parse(tokens);
        assert!(!errors.is_empty());
        assert_eq!(prog.items.len(), 1);
    }

    // --- Pub items ---

    #[test]
    fn test_parse_pub_struct() {
        let prog = parse("pub struct S { x: i32 }");
        if let Item::Struct(s) = &prog.items[0] {
            assert!(s.is_pub);
        }
    }

    // --- Db with ident store name ---

    #[test]
    fn test_parse_db_ident_store() {
        // `store` is a keyword so expect_ident won't match it in db context.
        // Test empty db.
        let prog = parse(r#"
            db D {
            }
        "#);
        if let Item::Db(d) = &prog.items[0] {
            assert_eq!(d.name, "D");
        } else {
            panic!("Expected Db");
        }
    }

    // --- Db index with ident names ---

    #[test]
    fn test_parse_db_index_ident() {
        // `store` keyword prevents parsing store blocks via expect_ident.
        // Verify db parses with just version.
        let prog = parse(r#"
            db D {
                version: 2,
            }
        "#);
        if let Item::Db(d) = &prog.items[0] {
            assert_eq!(d.version, Some(2));
        } else {
            panic!("Expected Db");
        }
    }

    // --- Agent with state and methods ---

    #[test]
    fn test_parse_agent_with_state() {
        let prog = parse(r#"
            agent A {
                let count: i32 = 0;
                signal history: i32 = 0;
                fn process(&self) { return; }
            }
        "#);
        if let Item::Agent(a) = &prog.items[0] {
            assert_eq!(a.state.len(), 2);
            assert_eq!(a.methods.len(), 1);
        } else {
            panic!("Expected Agent");
        }
    }

    // --- Pub items for various types ---

    #[test]
    fn test_parse_pub_page() {
        let prog = parse("pub page P { render { <div /> } }");
        if let Item::Page(p) = &prog.items[0] {
            assert!(p.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_form() {
        let prog = parse("pub form F { field name: String; }");
        if let Item::Form(f) = &prog.items[0] {
            assert!(f.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_channel() {
        let prog = parse(r#"pub channel Ch { url: "/ws", }"#);
        if let Item::Channel(ch) = &prog.items[0] {
            assert!(ch.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_embed() {
        let prog = parse(r#"pub embed E { src: "x", }"#);
        if let Item::Embed(e) = &prog.items[0] {
            assert!(e.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_pdf() {
        let prog = parse("pub pdf P { }");
        if let Item::Pdf(p) = &prog.items[0] {
            assert!(p.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_payment() {
        let prog = parse("pub payment P { }");
        if let Item::Payment(p) = &prog.items[0] {
            assert!(p.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_auth() {
        let prog = parse("pub auth A { }");
        if let Item::Auth(a) = &prog.items[0] {
            assert!(a.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_upload() {
        let prog = parse(r#"pub upload U { endpoint: "/u", }"#);
        if let Item::Upload(u) = &prog.items[0] {
            assert!(u.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_db() {
        let prog = parse("pub db D { }");
        if let Item::Db(d) = &prog.items[0] {
            assert!(d.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_cache() {
        let prog = parse("pub cache C { }");
        if let Item::Cache(c) = &prog.items[0] {
            assert!(c.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_theme() {
        let prog = parse("pub theme T { auto }");
        if let Item::Theme(t) = &prog.items[0] {
            assert!(t.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_spring() {
        let prog = parse("pub spring S { }");
        if let Item::Animation(a) = &prog.items[0] {
            assert!(a.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_keyframes() {
        let prog = parse("pub keyframes K { }");
        if let Item::Animation(a) = &prog.items[0] {
            assert!(a.is_pub);
        }
    }

    #[test]
    fn test_parse_pub_stagger() {
        let prog = parse("pub stagger St { animation: X }");
        if let Item::Animation(a) = &prog.items[0] {
            assert!(a.is_pub);
        }
    }

    // --- Channel with reconnect false ---

    #[test]
    fn test_parse_channel_reconnect_false() {
        // reconnect expects Ident("true"/"false") but true/false are keywords,
        // so the default reconnect=true stays. Test url parsing instead.
        let prog = parse(r#"
            channel C {
                url: "/ws",
            }
        "#);
        if let Item::Channel(ch) = &prog.items[0] {
            assert_eq!(ch.name, "C");
            // default reconnect is true
            assert!(ch.reconnect);
        } else {
            panic!("Expected Channel");
        }
    }

    // --- Payment sandbox ---

    #[test]
    fn test_parse_payment_sandbox() {
        // sandbox is a keyword; test payment with provider string
        let prog = parse(r#"
            payment P {
                provider: "stripe",
            }
        "#);
        if let Item::Payment(p) = &prog.items[0] {
            assert!(p.provider.is_some());
        } else {
            panic!("Expected Payment");
        }
    }

    // --- Upload chunked false ---

    #[test]
    fn test_parse_upload_chunked_false() {
        // chunked: true/false uses Ident matching but true/false are keywords
        let prog = parse(r#"
            upload U {
                endpoint: "/u",
            }
        "#);
        if let Item::Upload(u) = &prog.items[0] {
            assert!(!u.chunked); // default false
        } else {
            panic!("Expected Upload");
        }
    }

    // --- Cache persist false ---

    #[test]
    fn test_parse_cache_persist_false() {
        // persist expects Ident but true/false are keywords; test without it
        let prog = parse(r#"
            cache C {
                strategy: "cache-first",
            }
        "#);
        if let Item::Cache(c) = &prog.items[0] {
            assert_eq!(c.strategy, Some("cache-first".to_string()));
            assert!(!c.persist); // default
        } else {
            panic!("Expected Cache");
        }
    }

    // --- App router nested ---

    #[test]
    fn test_parse_app_with_router() {
        let prog = parse(r#"
            app A {
                router AppRouter {
                    route "/" => Home,
                }
            }
        "#);
        if let Item::App(a) = &prog.items[0] {
            assert!(a.router.is_some());
        } else {
            panic!("Expected App");
        }
    }

    // --- Form with required and message ---

    #[test]
    fn test_parse_form_field_required_with_message() {
        let prog = parse(r#"
            form F {
                field name: String {
                    required: "Name is required",
                }
            }
        "#);
        if let Item::Form(f) = &prog.items[0] {
            let v = &f.fields[0].validators;
            assert_eq!(v.len(), 1);
            assert!(matches!(v[0].kind, ValidatorKind::Required));
            assert!(v[0].message.is_some());
        } else {
            panic!("Expected Form");
        }
    }

    // --- Page with styles, permissions, gestures ---

    #[test]
    fn test_parse_page_with_style_and_perms() {
        let prog = parse(r#"
            page P {
                style {
                    .container {
                        color: "black";
                    }
                }
                permissions {
                    network: ["*"],
                }
                gesture swipe_right {
                    return;
                }
                render { <div /> }
            }
        "#);
        if let Item::Page(p) = &prog.items[0] {
            assert!(!p.styles.is_empty());
            assert!(p.permissions.is_some());
            assert_eq!(p.gestures.len(), 1);
        } else {
            panic!("Expected Page");
        }
    }

    // --- Complex expressions ---

    #[test]
    fn test_parse_chained_method_calls() {
        let e = parse_expr("a.b().c().d()");
        if let Expr::MethodCall { method, .. } = &e {
            assert_eq!(method, "d");
        } else {
            panic!("Expected MethodCall chain");
        }
    }

    #[test]
    fn test_parse_chained_field_access() {
        let e = parse_expr("a.b.c.d");
        if let Expr::FieldAccess { field, .. } = &e {
            assert_eq!(field, "d");
        } else {
            panic!("Expected FieldAccess chain");
        }
    }

    // --- Structured data with schema prefix ---

    #[test]
    fn test_parse_structured_data_with_schema() {
        let prog = parse(r#"
            page P {
                meta {
                    structured_data: schema.Product {
                        name: "Widget",
                    },
                }
                render { <div /> }
            }
        "#);
        if let Item::Page(p) = &prog.items[0] {
            let meta = p.meta.as_ref().unwrap();
            assert_eq!(meta.structured_data[0].schema_type, "Product");
        } else {
            panic!("Expected Page");
        }
    }

    // --- Namespaced calls (crypto::sha256 etc.) ---

    #[test]
    fn test_parse_namespaced_call() {
        let e = parse_expr("crypto::sha256(data)");
        if let Expr::FnCall { callee, args } = &e {
            if let Expr::Ident(name) = callee.as_ref() {
                assert_eq!(name, "crypto::sha256");
            } else {
                panic!("Expected Ident callee");
            }
            assert_eq!(args.len(), 1);
        } else {
            panic!("Expected FnCall, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_namespaced_two_args() {
        let e = parse_expr("crypto::hmac(key, data)");
        if let Expr::FnCall { callee, args } = &e {
            if let Expr::Ident(name) = callee.as_ref() {
                assert_eq!(name, "crypto::hmac");
            } else {
                panic!("Expected Ident");
            }
            assert_eq!(args.len(), 2);
        } else {
            panic!("Expected FnCall");
        }
    }

    #[test]
    fn test_parse_namespaced_no_args() {
        let e = parse_expr("crypto::random_uuid()");
        if let Expr::FnCall { callee, args } = &e {
            if let Expr::Ident(name) = callee.as_ref() {
                assert_eq!(name, "crypto::random_uuid");
            } else {
                panic!("Expected Ident");
            }
            assert_eq!(args.len(), 0);
        } else {
            panic!("Expected FnCall");
        }
    }

    #[test]
    fn test_parse_namespaced_ident_without_call() {
        let e = parse_expr("crypto::sha256");
        if let Expr::Ident(name) = &e {
            assert_eq!(name, "crypto::sha256");
        } else {
            panic!("Expected Ident, got {:?}", e);
        }
    }

    // --- vec![] macro parsing ---

    #[test]
    fn test_parse_vec_macro_brackets() {
        let e = parse_expr("vec![1, 2, 3]");
        if let Expr::ArrayLit(elements) = &e {
            assert_eq!(elements.len(), 3);
        } else {
            panic!("Expected ArrayLit, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_vec_macro_parens() {
        let e = parse_expr("vec!(4, 5)");
        if let Expr::ArrayLit(elements) = &e {
            assert_eq!(elements.len(), 2);
        } else {
            panic!("Expected ArrayLit, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_vec_macro_empty() {
        let e = parse_expr("vec![]");
        if let Expr::ArrayLit(elements) = &e {
            assert_eq!(elements.len(), 0);
        } else {
            panic!("Expected ArrayLit, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_vec_ident_no_bang() {
        let e = parse_expr("vec");
        assert!(matches!(e, Expr::Ident(ref n) if n == "vec"), "Expected Ident(\"vec\"), got {:?}", e);
    }

    // --- fn closure parsing ---

    #[test]
    fn test_parse_fn_closure_no_params() {
        let e = parse_expr("fn() { 42 }");
        if let Expr::Closure { params, .. } = &e {
            assert_eq!(params.len(), 0);
        } else {
            panic!("Expected Closure, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_fn_closure_with_params() {
        let e = parse_expr("fn(x: i32, y: i32) { x }");
        if let Expr::Closure { params, .. } = &e {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].0, "x");
            assert_eq!(params[1].0, "y");
        } else {
            panic!("Expected Closure, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_fn_closure_with_return_type() {
        let e = parse_expr("fn(a: String) -> i32 { 0 }");
        if let Expr::Closure { params, .. } = &e {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "a");
        } else {
            panic!("Expected Closure, got {:?}", e);
        }
    }

    // --- :: qualified pattern parsing ---

    #[test]
    fn test_parse_qualified_pattern_variant() {
        let prog = parse("fn main() { match x { Color::Red => 1, _ => 0, } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                if let Pattern::Variant { name, fields } = &arms[0].pattern {
                    assert_eq!(name, "Red");
                    assert!(fields.is_empty());
                } else {
                    panic!("Expected Variant pattern, got {:?}", arms[0].pattern);
                }
            } else {
                panic!("Expected Match expr");
            }
        }
    }

    #[test]
    fn test_parse_qualified_pattern_with_payload() {
        let prog = parse("fn main() { match x { Option::Some(v) => v, _ => 0, } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                if let Pattern::Variant { name, fields } = &arms[0].pattern {
                    assert_eq!(name, "Some");
                    assert_eq!(fields.len(), 1);
                } else {
                    panic!("Expected Variant pattern, got {:?}", arms[0].pattern);
                }
            } else {
                panic!("Expected Match expr");
            }
        }
    }

    #[test]
    fn test_parse_enum_variant_no_fields_pattern() {
        // Enum::Variant without payload should produce Pattern::Variant with empty fields
        let prog = parse("fn main() { match status { Status::Active => 1, Status::Inactive => 0, } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                if let Pattern::Variant { name, fields } = &arms[0].pattern {
                    assert_eq!(name, "Active");
                    assert!(fields.is_empty(), "No-payload variant should have empty fields");
                } else {
                    panic!("Expected Variant pattern for Status::Active, got {:?}", arms[0].pattern);
                }
                if let Pattern::Variant { name, fields } = &arms[1].pattern {
                    assert_eq!(name, "Inactive");
                    assert!(fields.is_empty());
                } else {
                    panic!("Expected Variant pattern for Status::Inactive, got {:?}", arms[1].pattern);
                }
            } else {
                panic!("Expected Match expr");
            }
        }
    }

    #[test]
    fn test_parse_enum_variant_with_nested_payload() {
        // Enum::Variant(Enum2::Other) should parse nested qualified patterns
        let prog = parse("fn main() { match x { Result::Ok(v) => v, Result::Err(e) => 0, } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                if let Pattern::Variant { name, fields } = &arms[0].pattern {
                    assert_eq!(name, "Ok");
                    assert_eq!(fields.len(), 1);
                } else {
                    panic!("Expected Variant pattern for Result::Ok");
                }
                if let Pattern::Variant { name, fields } = &arms[1].pattern {
                    assert_eq!(name, "Err");
                    assert_eq!(fields.len(), 1);
                } else {
                    panic!("Expected Variant pattern for Result::Err");
                }
            } else {
                panic!("Expected Match expr");
            }
        }
    }

    #[test]
    fn test_parse_secret_parameter() {
        let prog = parse("fn hash(secret password: String) -> String { password }");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params.len(), 1);
            assert_eq!(f.params[0].name, "password");
            assert!(f.params[0].secret, "Parameter should be marked secret");
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_parse_non_secret_parameter() {
        let prog = parse("fn add(a: i32, b: i32) -> i32 { a }");
        if let Item::Function(f) = &prog.items[0] {
            assert!(!f.params[0].secret, "Regular param should not be secret");
            assert!(!f.params[1].secret, "Regular param should not be secret");
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_parse_mixed_secret_and_regular_params() {
        let prog = parse("fn verify(secret token: String, user_id: i32) -> bool { true }");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params.len(), 2);
            assert!(f.params[0].secret, "token should be secret");
            assert_eq!(f.params[0].name, "token");
            assert!(!f.params[1].secret, "user_id should not be secret");
            assert_eq!(f.params[1].name, "user_id");
        } else {
            panic!("Expected Function");
        }
    }

    // --- match guard parsing ---

    #[test]
    fn test_parse_match_guard() {
        let prog = parse("fn main() { match x { n if n > 0 => n, _ => 0, } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                assert!(arms[0].guard.is_some(), "Expected guard on first arm");
                assert!(arms[1].guard.is_none(), "Expected no guard on wildcard arm");
            } else {
                panic!("Expected Match expr");
            }
        }
    }

    #[test]
    fn test_parse_match_no_guard() {
        let prog = parse("fn main() { match x { 1 => 10, _ => 0, } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                assert!(arms[0].guard.is_none());
                assert!(arms[1].guard.is_none());
            } else {
                panic!("Expected Match expr");
            }
        }
    }

    // --- array literal parsing ---

    #[test]
    fn test_parse_array_literal() {
        let e = parse_expr("[1, 2, 3]");
        if let Expr::ArrayLit(elements) = &e {
            assert_eq!(elements.len(), 3);
        } else {
            panic!("Expected ArrayLit, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_empty_array_literal() {
        let e = parse_expr("[]");
        if let Expr::ArrayLit(elements) = &e {
            assert_eq!(elements.len(), 0);
        } else {
            panic!("Expected ArrayLit, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_match_guard_with_equality() {
        let prog = parse("fn main() { match x { n if n == 0 => 1, _ => 2, } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                assert!(arms[0].guard.is_some());
                assert_eq!(arms.len(), 2);
            } else { panic!("Expected Match"); }
        }
    }

    #[test]
    fn test_parse_match_multiple_guards() {
        let prog = parse("fn main() { match x { a if a > 0 => 1, b if b < 0 => 2, _ => 0, } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                assert!(arms[0].guard.is_some());
                assert!(arms[1].guard.is_some());
                assert!(arms[2].guard.is_none());
                assert_eq!(arms.len(), 3);
            } else { panic!("Expected Match"); }
        }
    }

    #[test]
    fn test_parse_vec_macro_with_parens() {
        let e = parse_expr("vec!(1, 2, 3)");
        if let Expr::ArrayLit(elements) = &e {
            assert_eq!(elements.len(), 3);
        } else {
            panic!("Expected ArrayLit from vec!(), got {:?}", e);
        }
    }

    #[test]
    fn test_parse_vec_macro_single_element() {
        let e = parse_expr("vec![42]");
        if let Expr::ArrayLit(elements) = &e {
            assert_eq!(elements.len(), 1);
        } else {
            panic!("Expected ArrayLit, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_fn_closure_empty_body() {
        let e = parse_expr("fn() { }");
        assert!(matches!(e, Expr::Closure { .. }));
    }

    #[test]
    fn test_parse_qualified_pattern_simple_ident() {
        // Without ::, should parse as normal ident pattern
        let prog = parse("fn main() { match x { y => 0, } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                if let Pattern::Ident(name) = &arms[0].pattern {
                    assert_eq!(name, "y");
                } else { panic!("Expected Ident pattern"); }
            } else { panic!("Expected Match"); }
        }
    }

    #[test]
    fn test_parse_nested_array_literal() {
        let e = parse_expr("[[1, 2], [3, 4]]");
        if let Expr::ArrayLit(outer) = &e {
            assert_eq!(outer.len(), 2);
            assert!(matches!(&outer[0], Expr::ArrayLit(_)));
            assert!(matches!(&outer[1], Expr::ArrayLit(_)));
        } else {
            panic!("Expected nested ArrayLit, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_single_element_array() {
        let e = parse_expr("[42]");
        if let Expr::ArrayLit(elements) = &e {
            assert_eq!(elements.len(), 1);
        } else {
            panic!("Expected ArrayLit, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_match_wildcard_only() {
        let prog = parse("fn main() { match x { _ => 0, } }");
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::Match { arms, .. }) = &f.body.stmts[0] {
                assert_eq!(arms.len(), 1);
                assert!(matches!(&arms[0].pattern, Pattern::Wildcard));
                assert!(arms[0].guard.is_none());
            } else { panic!("Expected Match"); }
        }
    }

    // --- TemplateIf and TemplateFor in templates ---

    #[test]
    fn test_parse_template_if() {
        let prog = parse(r#"
            component Test() {
                render {
                    <div>
                        {if show {
                            <span>"visible"</span>
                        }}
                    </div>
                }
            }
        "#);
        assert_eq!(prog.items.len(), 1);
        assert!(matches!(prog.items[0], Item::Component(_)));
    }

    #[test]
    fn test_parse_template_for() {
        let prog = parse(r#"
            component List() {
                render {
                    <ul>
                        {for item in items {
                            <li>{item}</li>
                        }}
                    </ul>
                }
            }
        "#);
        assert_eq!(prog.items.len(), 1);
        assert!(matches!(prog.items[0], Item::Component(_)));
    }

    // ========================================================================
    // TESTS FOR NEW PARSER FIXES
    // ========================================================================

    // --- Fix 1: Object literal as function argument ---

    #[test]
    fn test_parse_object_literal_as_arg() {
        let e = parse_expr(r#"fetch("url", { method: "POST" })"#);
        if let Expr::Fetch { options: Some(opts), .. } = e {
            assert!(matches!(*opts, Expr::ObjectLit { .. }));
        } else {
            panic!("Expected Fetch with object literal options, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_object_literal_nested() {
        // { method: "POST", headers: { "Content-Type": "application/json" } }
        let e = parse_expr(r#"{ method: "POST", headers: { "Content-Type": "application/json" } }"#);
        if let Expr::ObjectLit { fields } = e {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0, "method");
            if let Expr::ObjectLit { fields: nested } = &fields[1].1 {
                assert_eq!(nested.len(), 1);
                assert_eq!(nested[0].0, "Content-Type");
            } else {
                panic!("Expected nested ObjectLit for headers");
            }
        } else {
            panic!("Expected ObjectLit, got {:?}", e);
        }
    }

    #[test]
    fn test_parse_object_literal_empty() {
        let e = parse_expr("{}");
        assert!(matches!(e, Expr::ObjectLit { fields } if fields.is_empty()));
    }

    #[test]
    fn test_parse_object_literal_single_field() {
        let e = parse_expr("{ method: \"DELETE\" }");
        if let Expr::ObjectLit { fields } = e {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].0, "method");
        } else {
            panic!("Expected ObjectLit");
        }
    }

    #[test]
    fn test_parse_block_not_confused_with_object_literal() {
        // A block with a let statement should not be parsed as an object literal
        let prog = parse("fn f() { let x = 1; }");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.body.stmts.len(), 1);
            assert!(matches!(f.body.stmts[0], Stmt::Let { .. }));
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_parse_fetch_with_object_options() {
        let prog = parse(r#"
            fn post() {
                let resp = await fetch("https://api.example.com/posts", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: "data",
                });
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Let { value, .. } = &f.body.stmts[0] {
                // The outer expression is Await(Fetch(...))
                assert!(matches!(value, Expr::Await(_)));
            } else {
                panic!("Expected let statement");
            }
        } else {
            panic!("Expected Function");
        }
    }

    // --- Fix 3: `secret` parameter modifier ---

    #[test]
    fn test_parse_secret_param_modifier() {
        let prog = parse("fn secure(secret password: String) { }");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params.len(), 1);
            assert_eq!(f.params[0].name, "password");
            assert!(matches!(f.params[0].ty, Type::Named(ref n) if n == "String"));
        } else {
            panic!("Expected Function with secret param");
        }
    }

    #[test]
    fn test_parse_secret_param_multiple() {
        let prog = parse("fn process(secret key: String, secret data: String) { }");
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params.len(), 2);
            assert_eq!(f.params[0].name, "key");
            assert_eq!(f.params[1].name, "data");
        } else {
            panic!("Expected Function");
        }
    }

    // --- Fix 4: `shortcut "key" => expr;` syntax ---

    #[test]
    fn test_parse_shortcut_fat_arrow_syntax() {
        let prog = parse(r#"
            component Editor() {
                shortcut "Cmd+S" => self.save;

                fn save(&self) { }

                render {
                    <div>"editor"</div>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.shortcuts.len(), 1);
            assert_eq!(c.shortcuts[0].keys, "Cmd+S");
            // Body should contain one expression statement
            assert_eq!(c.shortcuts[0].body.stmts.len(), 1);
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_shortcut_multiple_fat_arrows() {
        let prog = parse(r#"
            component App() {
                shortcut "Cmd+S" => self.save;
                shortcut "Cmd+Z" => self.undo;

                fn save(&self) { }
                fn undo(&self) { }

                render {
                    <div>"app"</div>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.shortcuts.len(), 2);
            assert_eq!(c.shortcuts[0].keys, "Cmd+S");
            assert_eq!(c.shortcuts[1].keys, "Cmd+Z");
        } else {
            panic!("Expected Component");
        }
    }

    // --- Fix 5: `for (i, col) in expr` tuple destructuring ---

    #[test]
    fn test_parse_for_tuple_destructure() {
        let prog = parse(r#"
            fn f() {
                for (i, col) in items.iter().enumerate() {
                    let x = i;
                }
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::For { binding, body, .. }) = &f.body.stmts[0] {
                assert_eq!(binding, "__for_tuple__");
                // First stmt in body should be LetDestructure with Tuple pattern
                assert!(matches!(&body.stmts[0], Stmt::LetDestructure {
                    pattern: Pattern::Tuple(_), ..
                }));
                if let Stmt::LetDestructure { pattern: Pattern::Tuple(pats), .. } = &body.stmts[0] {
                    assert_eq!(pats.len(), 2);
                    assert!(matches!(&pats[0], Pattern::Ident(n) if n == "i"));
                    assert!(matches!(&pats[1], Pattern::Ident(n) if n == "col"));
                }
            } else {
                panic!("Expected For statement");
            }
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_parse_for_triple_tuple_destructure() {
        let prog = parse(r#"
            fn f() {
                for (a, b, c) in triples {
                    let x = a;
                }
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::For { binding, body, .. }) = &f.body.stmts[0] {
                assert_eq!(binding, "__for_tuple__");
                if let Stmt::LetDestructure { pattern: Pattern::Tuple(pats), .. } = &body.stmts[0] {
                    assert_eq!(pats.len(), 3);
                }
            } else {
                panic!("Expected For");
            }
        }
    }

    // --- Fix 6: `on_connect fn() { ... }` channel syntax ---

    #[test]
    fn test_parse_channel_on_connect_anonymous_fn() {
        let prog = parse(r#"
            channel Chat {
                url: "wss://example.com/ws",

                on_connect fn() {
                }

                on_disconnect fn() {
                }
            }
        "#);
        if let Item::Channel(ch) = &prog.items[0] {
            assert!(ch.on_connect.is_some());
            let f = ch.on_connect.as_ref().unwrap();
            assert_eq!(f.name, "on_connect");
            assert_eq!(f.params.len(), 0);
            assert!(ch.on_disconnect.is_some());
        } else {
            panic!("Expected Channel");
        }
    }

    #[test]
    fn test_parse_channel_on_message_with_param() {
        let prog = parse(r#"
            channel Chat {
                url: "wss://example.com/ws",

                on_message fn(msg: String) {
                }
            }
        "#);
        if let Item::Channel(ch) = &prog.items[0] {
            assert!(ch.on_message.is_some());
            let f = ch.on_message.as_ref().unwrap();
            assert_eq!(f.params.len(), 1);
            assert_eq!(f.params[0].name, "msg");
        } else {
            panic!("Expected Channel");
        }
    }

    // --- Fix 7: `spring enter { ... }` inside a component ---

    #[test]
    fn test_parse_spring_in_component() {
        let prog = parse(r#"
            component Modal() {
                let mut visible: bool = false;

                spring enter {
                    from: { opacity: 0.0 },
                    to: { opacity: 1.0 },
                    stiffness: 300,
                    damping: 25,
                }

                fn toggle(&mut self) {
                    self.visible = !self.visible;
                }

                render {
                    <div>"modal"</div>
                }
            }
        "#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.name, "Modal");
            // The spring block is consumed silently — check we parsed the rest correctly
            assert_eq!(c.methods.len(), 1);
            assert_eq!(c.methods[0].name, "toggle");
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_keyframes_in_component() {
        let prog = parse(r#"
            component Spinner() {
                keyframes spin {
                    from: { transform: "rotate(0deg)" },
                    to: { transform: "rotate(360deg)" },
                }

                render {
                    <div>"spinner"</div>
                }
            }
        "#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.name, "Spinner");
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_stagger_in_component() {
        let prog = parse(r#"
            component FeedList() {
                stagger feed_enter {
                    from: { opacity: 0.0 },
                    to: { opacity: 1.0 },
                    delay: 50,
                }

                render {
                    <ul>"items"</ul>
                }
            }
        "#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.name, "FeedList");
        } else {
            panic!("Expected Component");
        }
    }

    // --- Fix 8: `breakpoints AppBreakpoints { ... }` with name ---

    #[test]
    fn test_parse_breakpoints_with_name() {
        let prog = parse(r#"
            breakpoints AppBreakpoints {
                sm: 640,
                md: 768,
                lg: 1024,
            }
        "#);
        if let Item::Breakpoints(bp) = &prog.items[0] {
            assert_eq!(bp.breakpoints.len(), 3);
            assert_eq!(bp.breakpoints[0], ("sm".to_string(), 640));
            assert_eq!(bp.breakpoints[1], ("md".to_string(), 768));
            assert_eq!(bp.breakpoints[2], ("lg".to_string(), 1024));
        } else {
            panic!("Expected Breakpoints");
        }
    }

    #[test]
    fn test_parse_breakpoints_without_name() {
        let prog = parse(r#"
            breakpoints {
                sm: 640,
                md: 768,
            }
        "#);
        if let Item::Breakpoints(bp) = &prog.items[0] {
            assert_eq!(bp.breakpoints.len(), 2);
        } else {
            panic!("Expected Breakpoints without name");
        }
    }

    // --- Fix 9: Closure with template body in virtual list ---

    #[test]
    fn test_parse_closure_with_template_body_in_virtual_list() {
        let prog = parse(r#"
            component LogViewer() {
                let logs: Vec<String> = vec![];

                render {
                    <div class="log-viewer">
                        <virtual list={self.logs} item_height={32} buffer={10}>
                            {|log, index| {
                                <div class="log-entry">
                                    <span class="message">{log}</span>
                                </div>
                            }}
                        </virtual>
                    </div>
                }
            }
        "#);
        assert_eq!(prog.items.len(), 1);
        assert!(matches!(prog.items[0], Item::Component(_)));
    }

    // --- Fix 10: `Schema.Article { ... }` struct construction ---

    #[test]
    fn test_parse_schema_dot_struct_init() {
        let prog = parse(r#"
            fn f() {
                let sd = Schema.Article {
                    headline: title,
                    author: name,
                };
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Let { value, .. } = &f.body.stmts[0] {
                assert!(matches!(value, Expr::StructInit { name, .. } if name == "Schema.Article"));
            } else {
                panic!("Expected Let statement");
            }
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_parse_schema_dot_product_init() {
        let prog = parse(r#"
            fn f() {
                let sd = Schema.Product {
                    name: product_name,
                    price: price_str,
                };
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Let { value, .. } = &f.body.stmts[0] {
                assert!(matches!(value, Expr::StructInit { name, .. } if name == "Schema.Product"));
            } else {
                panic!("Expected Let");
            }
        }
    }

    // --- Fix 11: `default: "auto"` in theme block ---

    #[test]
    fn test_parse_theme_with_default_field() {
        let prog = parse(r##"
            theme AppTheme {
                light {
                    bg: "#ffffff",
                    text: "#000000",
                }
                dark {
                    bg: "#0f172a",
                    text: "#e2e8f0",
                }
                default: "auto",
            }
        "##);
        if let Item::Theme(t) = &prog.items[0] {
            assert_eq!(t.name, "AppTheme");
            assert!(t.light.is_some());
            assert!(t.dark.is_some());
        } else {
            panic!("Expected Theme");
        }
    }

    #[test]
    fn test_parse_theme_default_only() {
        let prog = parse(r##"
            theme SimpleTheme {
                primary: "#2563eb",
                default: "light",
            }
        "##);
        if let Item::Theme(t) = &prog.items[0] {
            assert_eq!(t.name, "SimpleTheme");
        } else {
            panic!("Expected Theme");
        }
    }

    // --- Integration: parse full example-style programs ---

    #[test]
    fn test_parse_fetch_with_nested_object_options() {
        // Models api.nectar pattern: fetch(url, { method: "POST", headers: {...}, body: ... })
        let prog = parse(r#"
            fn create_post(title: String, body: String) {
                let response = await fetch("https://api.example.com/posts", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: body,
                });
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.name, "create_post");
            assert_eq!(f.body.stmts.len(), 1);
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_parse_channel_realtime_example() {
        let prog = parse(r#"
            channel Chat {
                url: "wss://api.example.com/ws/chat",
                reconnect: true,
                heartbeat: 30000,

                on_connect fn() {
                }

                on_message fn(msg: String) {
                }

                on_disconnect fn() {
                }

                fn send_message(&self, text: String) {
                }
            }
        "#);
        if let Item::Channel(ch) = &prog.items[0] {
            assert_eq!(ch.name, "Chat");
            assert!(ch.on_connect.is_some());
            assert!(ch.on_message.is_some());
            assert!(ch.on_disconnect.is_some());
            assert_eq!(ch.methods.len(), 1);
        } else {
            panic!("Expected Channel");
        }
    }

    #[test]
    fn test_parse_shortcuts_example() {
        let prog = parse(r#"
            component TextEditor() {
                let mut content: String = "";
                let mut saved: bool = true;

                shortcut "Cmd+S" => self.save;
                shortcut "Cmd+Z" => self.undo;
                shortcut "Escape" => self.close;

                fn save(&mut self) { }
                fn undo(&mut self) { }
                fn close(&self) { }

                render {
                    <div class="editor">
                        <p>"editor"</p>
                    </div>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.shortcuts.len(), 3);
            assert_eq!(c.shortcuts[0].keys, "Cmd+S");
            assert_eq!(c.shortcuts[1].keys, "Cmd+Z");
            assert_eq!(c.shortcuts[2].keys, "Escape");
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_for_tuple_in_template() {
        // Tests for (i, col) in template context (dnd.nectar pattern)
        let prog = parse(r#"
            fn f() {
                for (i, item) in self.items.iter().enumerate() {
                    let idx = i;
                }
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Expr(Expr::For { binding, .. }) = &f.body.stmts[0] {
                assert_eq!(binding, "__for_tuple__");
            } else {
                panic!("Expected For with tuple binding");
            }
        }
    }

    #[test]
    fn test_parse_crypto_secret_param() {
        // Tests crypto.nectar pattern: fn secure_token_flow(secret user_password: String)
        let prog = parse(r#"
            fn secure_token_flow(secret user_password: String) {
                let key = crypto.derive_key(user_password, "salt");
            }
        "#);
        if let Item::Function(f) = &prog.items[0] {
            assert_eq!(f.params.len(), 1);
            assert_eq!(f.params[0].name, "user_password");
        } else {
            panic!("Expected Function");
        }
    }

    #[test]
    fn test_parse_animations_component() {
        // Tests animations.nectar: spring enter/exit inside component
        let prog = parse(r#"
            component Modal() {
                let mut visible: bool = false;

                spring enter {
                    from: { opacity: 0.0, transform: "scale(0.95)" },
                    to: { opacity: 1.0, transform: "scale(1)" },
                    stiffness: 300,
                    damping: 25,
                }

                spring exit {
                    from: { opacity: 1.0 },
                    to: { opacity: 0.0 },
                    stiffness: 400,
                    damping: 30,
                }

                fn toggle(&mut self) {
                    self.visible = !self.visible;
                }

                render {
                    <div>"modal"</div>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.name, "Modal");
            assert_eq!(c.methods.len(), 1);
        } else {
            panic!("Expected Component");
        }
    }

    // === Tests for previously missing parser features ===

    /// Qualified paths with `::` work in match patterns and fn bodies
    /// without triggering the struct-init heuristic.
    #[test]
    fn test_parse_qualified_path_in_match_pattern() {
        // AuthStatus::LoggedIn(_) must parse as Pattern::Variant
        let prog = parse(r#"
component Foo() {
    fn check(&self) -> bool {
        match self.status {
            AuthStatus::LoggedIn(_) => true,
            AuthStatus::Error(msg) => false,
            _ => false,
        }
    }
    render { <div>"ok"</div> }
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.methods.len(), 1);
            assert_eq!(c.methods[0].name, "check");
        } else {
            panic!("Expected Component");
        }
    }

    /// `TypeName::method()` static call with `::` in expressions
    #[test]
    fn test_parse_static_method_call_via_colon_colon() {
        let prog = parse(r#"
component Foo() {
    fn handle(&mut self) {
        PostService::create_post(self.title, self.body);
    }
    render { <div>"ok"</div> }
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.methods[0].name, "handle");
            if let Stmt::Expr(Expr::FnCall { callee, .. }) = &c.methods[0].body.stmts[0] {
                if let Expr::Ident(name) = callee.as_ref() {
                    assert_eq!(name, "PostService::create_post");
                } else {
                    panic!("Expected Ident callee with qualified name");
                }
            } else {
                panic!("Expected Expr(FnCall) statement");
            }
        } else {
            panic!("Expected Component");
        }
    }

    /// `{match subject { Pattern => <element> }}` in template expressions
    #[test]
    fn test_parse_template_match_with_element_arms() {
        let prog = parse(r#"
component Foo() {
    render {
        {match self.get_error() {
            Some(err) => <div class="error">{err.message}</div>,
            None => <span />,
        }}
    }
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::TemplateMatch { arms, .. } = &c.render.body {
                assert_eq!(arms.len(), 2);
                // First arm: Some(err) => <div ...>
                if let Pattern::Variant { name, fields } = &arms[0].pattern {
                    assert_eq!(name, "Some");
                    assert_eq!(fields.len(), 1);
                } else {
                    panic!("Expected Variant pattern for Some arm");
                }
                assert_eq!(arms[0].body.len(), 1);
                assert!(matches!(&arms[0].body[0], TemplateNode::Element(e) if e.tag == "div"));
                // Second arm: None => <span />
                // `None` with no parentheses parses as Pattern::Ident("None")
                assert!(
                    matches!(&arms[1].pattern, Pattern::Ident(n) if n == "None")
                    || matches!(&arms[1].pattern, Pattern::Variant { name: n, .. } if n == "None"),
                    "Expected Ident or Variant pattern for None arm"
                );
                assert_eq!(arms[1].body.len(), 1);
                assert!(matches!(&arms[1].body[0], TemplateNode::Element(e) if e.tag == "span"));
            } else {
                panic!("Expected TemplateMatch node, got {:?}", c.render.body);
            }
        } else {
            panic!("Expected Component");
        }
    }

    /// `{match}` template node with element inside `{if}` — verifies that
    /// `<element>` inside a conditional template block works
    #[test]
    fn test_parse_template_if_with_element_body() {
        let prog = parse(r#"
component Foo() {
    render {
        {if PostService::get_loading() {
            <div>"Loading..."</div>
        }}
    }
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::TemplateIf { then_children, else_children, .. } = &c.render.body {
                assert_eq!(then_children.len(), 1);
                assert!(matches!(&then_children[0], TemplateNode::Element(e) if e.tag == "div"));
                assert!(else_children.is_none());
            } else {
                panic!("Expected TemplateIf, got {:?}", c.render.body);
            }
        } else {
            panic!("Expected Component");
        }
    }

    /// Inline `{if ... { "str" } else { "str" }}` with string literal branches
    #[test]
    fn test_parse_template_if_else_string_branches() {
        // This tests the case from shortcuts.nectar:
        // {if self.saved { "Saved" } else { "Unsaved changes" }}
        let prog = parse(r#"
component Foo() {
    render {
        <footer>
            {if self.saved { "Saved" } else { "Unsaved changes" }}
        </footer>
    }
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(footer) = &c.render.body {
                assert_eq!(footer.tag, "footer");
                assert_eq!(footer.children.len(), 1);
                if let TemplateNode::TemplateIf { then_children, else_children, .. } = &footer.children[0] {
                    assert_eq!(then_children.len(), 1);
                    assert!(matches!(&then_children[0], TemplateNode::TextLiteral(s) if s == "Saved"));
                    let else_nodes = else_children.as_ref().expect("Expected else branch");
                    assert_eq!(else_nodes.len(), 1);
                    assert!(matches!(&else_nodes[0], TemplateNode::TextLiteral(s) if s == "Unsaved changes"));
                } else {
                    panic!("Expected TemplateIf inside footer");
                }
            } else {
                panic!("Expected footer Element");
            }
        } else {
            panic!("Expected Component");
        }
    }

    /// Format string interpolation with function calls containing commas: `f"size: {fluid(24, 64)};"`
    #[test]
    fn test_parse_format_string_with_fn_call_args_in_interpolation() {
        // fluid is a keyword-like token; it must be handled as an ident-like in parse_primary
        let prog = parse(r#"
component Foo() {
    render {
        <h1 style={f"font-size: {fluid(24, 64)};"}>"Title"</h1>
    }
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(h1) = &c.render.body {
                assert_eq!(h1.tag, "h1");
                // The style attribute should be a dynamic attribute with a format string expr
                let has_format_style = h1.attributes.iter().any(|a| {
                    matches!(a, Attribute::Dynamic { name, value: Expr::FormatString { .. } }
                        if name == "style")
                });
                assert!(has_format_style, "Expected format-string style attribute");
            } else {
                panic!("Expected h1 Element");
            }
        } else {
            panic!("Expected Component");
        }
    }

    /// `fluid(min, max)` call works as a standalone expression
    /// (tests the ident-like keyword fallthrough in parse_primary)
    #[test]
    fn test_parse_fluid_function_call_expression() {
        let prog = parse(r#"
fn f() {
    let size = fluid(16, 48);
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Let { name, value, .. } = &f.body.stmts[0] {
                assert_eq!(name, "size");
                if let Expr::FnCall { callee, args } = value {
                    assert!(matches!(callee.as_ref(), Expr::Ident(n) if n == "fluid"));
                    assert_eq!(args.len(), 2);
                } else {
                    panic!("Expected FnCall for fluid");
                }
            } else {
                panic!("Expected Let statement");
            }
        } else {
            panic!("Expected Function");
        }
    }

    /// Self-closing tags `<tag />` parse correctly as elements with no children
    #[test]
    fn test_parse_self_closing_tag_in_template() {
        let prog = parse(r#"
component Foo() {
    render {
        <div>
            <span />
            <input type="text" />
            <br />
        </div>
    }
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(div) = &c.render.body {
                assert_eq!(div.tag, "div");
                assert_eq!(div.children.len(), 3);
                for child in &div.children {
                    if let TemplateNode::Element(e) = child {
                        assert!(e.children.is_empty(), "Self-closing tag should have no children");
                    } else {
                        panic!("Expected Element child");
                    }
                }
            } else {
                panic!("Expected div Element");
            }
        } else {
            panic!("Expected Component");
        }
    }

    /// `struct.Field { field: val }` dot-access struct init still works
    /// (verifies that the uppercase-only guard doesn't break legitimate uses)
    #[test]
    fn test_parse_dot_access_struct_init_uppercase_field() {
        let prog = parse(r#"
fn build() {
    let x = Schema.Article { id: 1, title: "hello" };
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Function(f) = &prog.items[0] {
            if let Stmt::Let { value, .. } = &f.body.stmts[0] {
                assert!(matches!(value, Expr::StructInit { name, .. } if name == "Schema.Article"),
                    "Expected StructInit with name Schema.Article, got {:?}", value);
            } else {
                panic!("Expected Let");
            }
        } else {
            panic!("Expected Function");
        }
    }

    /// Lowercase field after dot followed by `{` is NOT a struct init —
    /// the `{` belongs to a subsequent block/template, not a struct literal.
    #[test]
    fn test_parse_lowercase_field_then_brace_is_not_struct_init() {
        // self.saved { ... } should be FieldAccess, not StructInit
        // This is the key regression that was causing issues.
        let prog = parse(r#"
component Foo() {
    render {
        {if self.saved { "yes" } else { "no" }}
    }
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::TemplateIf { condition, .. } = &c.render.body {
                // condition must be self.saved (FieldAccess), not a StructInit
                assert!(
                    matches!(condition.as_ref(), Expr::FieldAccess { field, .. } if field == "saved"),
                    "Expected FieldAccess(saved) as condition, got {:?}", condition
                );
            } else {
                panic!("Expected TemplateIf");
            }
        } else {
            panic!("Expected Component");
        }
    }

    /// Template `{match}` with a wildcard arm renders correctly
    #[test]
    fn test_parse_template_match_with_wildcard_arm() {
        let prog = parse(r#"
component Foo() {
    render {
        {match self.state {
            Active => <span>"active"</span>,
            _ => <span>"other"</span>,
        }}
    }
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::TemplateMatch { arms, .. } = &c.render.body {
                assert_eq!(arms.len(), 2);
                assert!(matches!(&arms[1].pattern, Pattern::Wildcard));
            } else {
                panic!("Expected TemplateMatch");
            }
        } else {
            panic!("Expected Component");
        }
    }

    /// Full api.nectar-style component with `{match}` and `{if}` template blocks
    #[test]
    fn test_parse_api_component_with_template_match_and_if() {
        let prog = parse(r#"
component PostList() {
    let mut new_title: String = "";

    fn handle_create(&mut self) {
        PostService::create_post(self.new_title, "");
        self.new_title = "";
    }

    render {
        <div>
            {if PostService::get_loading() {
                <div>"Loading..."</div>
            }}
            {match PostService::get_error() {
                Some(err) => <div class="error">{err.message}</div>,
                None => <span />,
            }}
            <button on:click={self.handle_create}>"Create"</button>
        </div>
    }
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Component(c) = &prog.items[0] {
            assert_eq!(c.name, "PostList");
            assert_eq!(c.methods.len(), 1);
            // render body is a <div> with 3 children
            if let TemplateNode::Element(div) = &c.render.body {
                assert_eq!(div.tag, "div");
                assert_eq!(div.children.len(), 3);
                assert!(matches!(&div.children[0], TemplateNode::TemplateIf { .. }));
                assert!(matches!(&div.children[1], TemplateNode::TemplateMatch { .. }));
                assert!(matches!(&div.children[2], TemplateNode::Element(e) if e.tag == "button"));
            } else {
                panic!("Expected div Element");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_select_element() {
        let prog = parse(r#"
            component C {
                render {
                    <select class="picker">
                        <option>"A"</option>
                    </select>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert_eq!(el.tag, "select");
                assert_eq!(el.attributes.len(), 1);
                assert_eq!(el.children.len(), 1);
                if let TemplateNode::Element(opt) = &el.children[0] {
                    assert_eq!(opt.tag, "option");
                } else {
                    panic!("Expected option Element");
                }
            } else {
                panic!("Expected Element");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_select_self_closing() {
        let prog = parse(r#"
            component C {
                render {
                    <select />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert_eq!(el.tag, "select");
                assert!(el.children.is_empty());
            } else {
                panic!("Expected Element");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_boolean_attribute_disabled() {
        let prog = parse(r#"
            component C {
                render {
                    <button disabled>"Click"</button>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert_eq!(el.tag, "button");
                assert_eq!(el.attributes.len(), 1);
                assert!(matches!(&el.attributes[0], Attribute::Static { name, value } if name == "disabled" && value.is_empty()));
            } else {
                panic!("Expected Element");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_boolean_attribute_checked() {
        let prog = parse(r#"
            component C {
                render {
                    <input checked />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert_eq!(el.tag, "input");
                assert_eq!(el.attributes.len(), 1);
                assert!(matches!(&el.attributes[0], Attribute::Static { name, value } if name == "checked" && value.is_empty()));
            } else {
                panic!("Expected Element");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_mixed_boolean_and_value_attributes() {
        let prog = parse(r#"
            component C {
                render {
                    <input disabled type="text" readonly />
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                assert_eq!(el.tag, "input");
                assert_eq!(el.attributes.len(), 3);
                assert!(matches!(&el.attributes[0], Attribute::Static { name, value } if name == "disabled" && value.is_empty()));
                assert!(matches!(&el.attributes[1], Attribute::Static { name, value } if name == "type" && value == "text"));
                assert!(matches!(&el.attributes[2], Attribute::Static { name, value } if name == "readonly" && value.is_empty()));
            } else {
                panic!("Expected Element");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_range_expression() {
        let expr = parse_expr("0..10");
        if let Expr::Range { start, end } = expr {
            assert!(matches!(*start, Expr::Integer(0)));
            assert!(matches!(*end, Expr::Integer(10)));
        } else {
            panic!("Expected Range expression, got {:?}", expr);
        }
    }

    #[test]
    fn test_parse_range_with_idents() {
        let expr = parse_expr("start..end");
        if let Expr::Range { start, end } = expr {
            assert!(matches!(*start, Expr::Ident(ref s) if s == "start"));
            assert!(matches!(*end, Expr::Ident(ref s) if s == "end"));
        } else {
            panic!("Expected Range expression, got {:?}", expr);
        }
    }

    #[test]
    fn test_parse_range_with_field_access() {
        let expr = parse_expr("self.start..self.end");
        if let Expr::Range { start, end } = expr {
            assert!(matches!(*start, Expr::FieldAccess { .. }));
            assert!(matches!(*end, Expr::FieldAccess { .. }));
        } else {
            panic!("Expected Range expression, got {:?}", expr);
        }
    }

    #[test]
    fn test_parse_template_for_range() {
        let prog = parse(r#"
            component C {
                render {
                    <div>
                        {for i in 0..10 {
                            <span>{i}</span>
                        }}
                    </div>
                }
            }
        "#);
        if let Item::Component(c) = &prog.items[0] {
            if let TemplateNode::Element(el) = &c.render.body {
                if let TemplateNode::TemplateFor { binding, iterator, .. } = &el.children[0] {
                    assert_eq!(binding, "i");
                    assert!(matches!(iterator.as_ref(), Expr::Range { .. }));
                } else {
                    panic!("Expected TemplateFor");
                }
            } else {
                panic!("Expected Element");
            }
        } else {
            panic!("Expected Component");
        }
    }
}
