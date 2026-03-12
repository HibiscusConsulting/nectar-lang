use crate::ast::*;
use crate::token::{Token, TokenKind, FormatStringPart, Span};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
}

/// Synchronization context — tells `synchronize()` what kind of boundary to
/// look for when skipping over broken tokens.
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
                // async fn ...
                self.advance();
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
            TokenKind::Lazy => {
                // lazy component Name { ... }
                self.advance();
                Ok(Item::LazyComponent(self.parse_lazy_component()?))
            }
            TokenKind::Test => Ok(Item::Test(self.parse_test_def()?)),
            _ => Err(self.error("Expected item (fn, component, struct, enum, impl, trait, use, mod, store, agent, router, lazy, test)")),
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

        Ok(Function { name, lifetimes, type_params, params, return_type, trait_bounds, body, is_pub, span })
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
        let mut render = None;
        let mut skeleton = None;
        let mut error_boundary = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.peek_kind() {
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
                TokenKind::Transition => {
                    transitions.extend(self.parse_transition_block()?);
                }
                TokenKind::Render => {
                    render = Some(self.parse_render_block()?);
                }
                TokenKind::Ident(ref id) if id == "skeleton" => {
                    skeleton = Some(self.parse_skeleton_block()?);
                }
                TokenKind::Ident(ref id) if id == "error_boundary" => {
                    error_boundary = Some(self.parse_error_boundary()?);
                }
                _ => return Err(self.error("Expected let, signal, fn, style, transition, render, skeleton, or error_boundary in component")),
            }
        }

        self.expect(&TokenKind::RightBrace)?;

        let render = render.ok_or_else(|| ParseError {
            message: format!("Component '{name}' missing render block"),
            span,
        })?;

        Ok(Component { name, type_params, props, state, methods, styles, transitions, trait_bounds, render, skeleton, error_boundary, span })
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

        Ok(StateField { name, ty, mutable, initializer, ownership })
    }

    fn parse_signal_field(&mut self) -> Result<StateField, ParseError> {
        self.expect(&TokenKind::Signal)?;
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
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RightBrace)?;
                Ok(TemplateNode::Expression(Box::new(expr)))
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
            } else {
                self.expect(&TokenKind::Equals)?;
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
        // "Link" tag name already consumed; parse the `to` attribute
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

        // Self-closing: <Link to="/" />
        if self.match_token(&TokenKind::Slash) {
            self.expect(&TokenKind::RightAngle)?;
            return Ok(TemplateNode::Link { to, children: vec![] });
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

        Ok(TemplateNode::Link { to, children })
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

    fn parse_store(&mut self, is_pub: bool) -> Result<StoreDef, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Store)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut signals = Vec::new();
        let mut actions = Vec::new();
        let mut computed = Vec::new();
        let mut effects = Vec::new();

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
                _ => return Err(self.error("Expected signal, action, computed, or effect in store")),
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(StoreDef { name, signals, actions, computed, effects, is_pub, span })
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
            // Handle &self, &mut self, self
            if self.check(&TokenKind::Ampersand) || self.check(&TokenKind::SelfKw) {
                let ownership = if self.match_token(&TokenKind::Ampersand) {
                    if self.match_token(&TokenKind::Mut) {
                        Ownership::MutBorrowed
                    } else {
                        Ownership::Borrowed
                    }
                } else {
                    Ownership::Owned
                };
                self.expect(&TokenKind::SelfKw)?;
                params.push(Param {
                    name: "self".into(),
                    ty: Type::Named("Self".into()),
                    ownership,
                });
            } else {
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

                params.push(Param { name, ty, ownership });
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
                        Ok(Stmt::Let { name, ty, mutable, value, ownership })
                    }
                }
            }
            TokenKind::Signal => {
                self.advance();
                let name = self.expect_ident()?;
                let ty = if self.match_token(&TokenKind::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect(&TokenKind::Equals)?;
                let value = self.parse_expr()?;
                self.expect(&TokenKind::Semicolon)?;
                Ok(Stmt::Signal { name, ty, value })
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
            _ => {
                let expr = self.parse_expr()?;
                // Optional semicolon for expression statements
                self.match_token(&TokenKind::Semicolon);
                Ok(Stmt::Expr(expr))
            }
        }
    }

    // === Expression parsing (Pratt parser) ===

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_or()?;

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
                Ok(Expr::Fetch { url: Box::new(url), options })
            }
            TokenKind::Navigate => {
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let path = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                Ok(Expr::Navigate { path: Box::new(path) })
            }
            TokenKind::Spawn => {
                self.advance();
                self.expect(&TokenKind::LeftBrace)?;
                let body = self.parse_expr()?;
                self.expect(&TokenKind::RightBrace)?;
                Ok(Expr::Spawn { body: Box::new(body) })
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
                self.expect(&TokenKind::LeftBrace)?;
                let mut exprs = Vec::new();
                while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                    exprs.push(self.parse_expr()?);
                    if !self.check(&TokenKind::RightBrace) {
                        self.expect(&TokenKind::Comma)?;
                    }
                }
                self.expect(&TokenKind::RightBrace)?;
                Ok(Expr::Parallel { exprs })
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
                let block = self.parse_block()?;
                Ok(Expr::Block(block))
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

            TokenKind::Ident(_) => {
                let name = self.expect_ident()?;
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
            self.expect(&TokenKind::FatArrow)?;
            let body = self.parse_expr()?;
            arms.push(MatchArm { pattern, body });
            if !self.check(&TokenKind::RightBrace) {
                self.expect(&TokenKind::Comma)?;
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(Expr::Match { subject: Box::new(subject), arms })
    }

    fn parse_for_expr(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::For)?;
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
                if self.match_token(&TokenKind::LeftParen) {
                    let mut fields = Vec::new();
                    while !self.check(&TokenKind::RightParen) {
                        fields.push(self.parse_pattern()?);
                        if !self.check(&TokenKind::RightParen) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&TokenKind::RightParen)?;
                    Ok(Pattern::Variant { name, fields })
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
                if self.check(&TokenKind::Dot) {
                    // Rest pattern ".."
                    self.advance(); // first dot
                    self.expect(&TokenKind::Dot)?; // second dot
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
            if self.check(&TokenKind::Dot) {
                self.advance(); // first dot
                self.expect(&TokenKind::Dot)?; // second dot
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
            } else {
                return Err(self.error("Expected 'route' or 'fallback' in router block"));
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(RouterDef { name, routes, fallback, span })
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

        Ok(RouteDef { path, params, component, guard, span })
    }

    // === Style parsing ===

    fn parse_style_blocks(&mut self) -> Result<Vec<StyleBlock>, ParseError> {
        self.expect(&TokenKind::Style)?;
        self.expect(&TokenKind::LeftBrace)?;

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
                        if !selector.is_empty() && !selector.ends_with('.') && !selector.ends_with(' ') {
                            selector.push(' ');
                        }
                        selector.push_str(s);
                    }
                    TokenKind::Colon => selector.push(':'),
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
                    _ => {
                        if !selector.is_empty() && !selector.ends_with(' ') {
                            selector.push(' ');
                        }
                    }
                }
            }

            self.expect(&TokenKind::LeftBrace)?;

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
                    if let TokenKind::StringLit(s) = self.advance().kind {
                        s
                    } else {
                        unreachable!()
                    }
                } else {
                    return Err(self.error("Expected string literal for CSS property value"));
                };

                self.expect(&TokenKind::Semicolon)?;
                properties.push((prop_name, value));
            }

            self.expect(&TokenKind::RightBrace)?;
            blocks.push(StyleBlock { selector: selector.trim().to_string(), properties, span });
        }

        self.expect(&TokenKind::RightBrace)?;
        Ok(blocks)
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
            other => Err(ParseError {
                message: format!("Expected identifier, found {:?}", other),
                span: self.current_span(),
            }),
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
}
