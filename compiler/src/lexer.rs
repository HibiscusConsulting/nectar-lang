use crate::token::{Token, TokenKind, FormatStringPart, Span};

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();

        while !self.is_eof() {
            self.skip_whitespace();
            if self.is_eof() {
                break;
            }

            // Skip comments
            if self.peek() == '/' && self.peek_next() == '/' {
                self.skip_line_comment();
                continue;
            }

            let token = self.next_token()?;
            tokens.push(token);
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.pos, self.pos, self.line, self.col),
        });

        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Token, LexError> {
        let start = self.pos;
        let line = self.line;
        let col = self.col;
        let ch = self.advance();

        let kind = match ch {
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            '[' => TokenKind::LeftBracket,
            ']' => TokenKind::RightBracket,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            '.' => {
                if self.match_char('.') {
                    TokenKind::DotDot
                } else {
                    TokenKind::Dot
                }
            }
            '?' => {
                if self.match_char('.') {
                    TokenKind::QuestionDot
                } else {
                    TokenKind::QuestionMark
                }
            }
            '#' => TokenKind::Hash,
            '%' => TokenKind::Percent,
            '&' => {
                if self.match_char('&') {
                    TokenKind::AmpAmp
                } else {
                    TokenKind::Ampersand
                }
            }
            '|' => {
                if self.match_char('|') {
                    TokenKind::PipePipe
                } else {
                    TokenKind::Pipe
                }
            }
            ':' => {
                if self.match_char(':') {
                    TokenKind::ColonColon
                } else {
                    TokenKind::Colon
                }
            }
            '+' => {
                if self.match_char('=') {
                    TokenKind::PlusEquals
                } else {
                    TokenKind::Plus
                }
            }
            '-' => {
                if self.match_char('>') {
                    TokenKind::Arrow
                } else if self.match_char('=') {
                    TokenKind::MinusEquals
                } else {
                    TokenKind::Minus
                }
            }
            '*' => {
                if self.match_char('=') {
                    TokenKind::StarEquals
                } else {
                    TokenKind::Star
                }
            }
            '/' => {
                if self.match_char('=') {
                    TokenKind::SlashEquals
                } else {
                    TokenKind::Slash
                }
            }
            '=' => {
                if self.match_char('=') {
                    TokenKind::DoubleEquals
                } else if self.match_char('>') {
                    TokenKind::FatArrow
                } else {
                    TokenKind::Equals
                }
            }
            '!' => {
                if self.match_char('=') {
                    TokenKind::NotEquals
                } else {
                    TokenKind::Bang
                }
            }
            '<' => {
                if self.match_char('=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::LeftAngle
                }
            }
            '>' => {
                if self.match_char('=') {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::RightAngle
                }
            }
            '\'' => {
                // Lifetime: 'a, 'b, 'static, etc.
                // If the next character is alphabetic, lex as a lifetime token.
                // Also used as single-quote in CSS values (handled by parser).
                if !self.is_eof() && self.peek().is_ascii_alphabetic() {
                    self.read_lifetime()
                } else {
                    TokenKind::SingleQuote
                }
            }
            '"' => self.read_string()?,
            c if c.is_ascii_digit() => self.read_number(c)?,
            c if c.is_ascii_alphabetic() || c == '_' => {
                // Special case: `f"..."` is a format string literal.
                // `f` followed immediately by `"` (no intervening alphanumerics).
                if c == 'f' && !self.is_eof() && self.peek() == '"' {
                    self.advance(); // consume the opening "
                    self.read_format_string()?
                } else {
                    self.read_identifier(c)
                }
            }
            '@' => TokenKind::At,
            c => {
                return Err(LexError {
                    message: format!("Unexpected character: '{c}'"),
                    line,
                    col,
                });
            }
        };

        Ok(Token {
            kind,
            span: Span::new(start, self.pos, line, col),
        })
    }

    fn read_string(&mut self) -> Result<TokenKind, LexError> {
        let mut s = String::new();
        let start_line = self.line;
        let start_col = self.col;

        while !self.is_eof() && self.peek() != '"' {
            if self.peek() == '\\' {
                self.advance();
                match self.advance() {
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    '\\' => s.push('\\'),
                    '"' => s.push('"'),
                    c => s.push(c),
                }
            } else {
                s.push(self.advance());
            }
        }

        if self.is_eof() {
            return Err(LexError {
                message: "Unterminated string literal".into(),
                line: start_line,
                col: start_col,
            });
        }

        self.advance(); // closing "
        Ok(TokenKind::StringLit(s))
    }

    fn read_format_string(&mut self) -> Result<TokenKind, LexError> {
        let start_line = self.line;
        let start_col = self.col;
        let mut parts = Vec::new();
        let mut current_lit = String::new();

        while !self.is_eof() && self.peek() != '"' {
            if self.peek() == '{' {
                // Push any accumulated literal text
                if !current_lit.is_empty() {
                    parts.push(FormatStringPart::Lit(std::mem::take(&mut current_lit)));
                }
                self.advance(); // consume '{'
                let mut expr_text = String::new();
                let mut depth = 1u32;
                while !self.is_eof() && depth > 0 {
                    let c = self.advance();
                    if c == '{' {
                        depth += 1;
                        expr_text.push(c);
                    } else if c == '}' {
                        depth -= 1;
                        if depth > 0 {
                            expr_text.push(c);
                        }
                    } else {
                        expr_text.push(c);
                    }
                }
                parts.push(FormatStringPart::Expr(expr_text));
            } else if self.peek() == '\\' {
                self.advance();
                match self.advance() {
                    'n' => current_lit.push('\n'),
                    't' => current_lit.push('\t'),
                    '\\' => current_lit.push('\\'),
                    '"' => current_lit.push('"'),
                    '{' => current_lit.push('{'),
                    '}' => current_lit.push('}'),
                    c => current_lit.push(c),
                }
            } else {
                current_lit.push(self.advance());
            }
        }

        if self.is_eof() {
            return Err(LexError {
                message: "Unterminated format string literal".into(),
                line: start_line,
                col: start_col,
            });
        }

        if !current_lit.is_empty() {
            parts.push(FormatStringPart::Lit(current_lit));
        }

        self.advance(); // closing "
        Ok(TokenKind::FormatString(parts))
    }

    fn read_number(&mut self, first: char) -> Result<TokenKind, LexError> {
        let mut num = String::from(first);
        let mut is_float = false;

        while !self.is_eof() && (self.peek().is_ascii_digit() || self.peek() == '_') {
            let c = self.advance();
            if c != '_' {
                num.push(c);
            }
        }

        if !self.is_eof() && self.peek() == '.' && self.peek_next().is_ascii_digit() {
            is_float = true;
            num.push(self.advance()); // .
            while !self.is_eof() && self.peek().is_ascii_digit() {
                num.push(self.advance());
            }
        }

        if is_float {
            Ok(TokenKind::Float(num.parse().unwrap()))
        } else {
            Ok(TokenKind::Integer(num.parse().unwrap()))
        }
    }

    fn read_lifetime(&mut self) -> TokenKind {
        let mut name = String::new();
        while !self.is_eof() && (self.peek().is_ascii_alphanumeric() || self.peek() == '_') {
            name.push(self.advance());
        }
        TokenKind::Lifetime(name)
    }

    fn read_identifier(&mut self, first: char) -> TokenKind {
        let mut ident = String::from(first);

        while !self.is_eof() && (self.peek().is_ascii_alphanumeric() || self.peek() == '_') {
            ident.push(self.advance());
        }

        match ident.as_str() {
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "fn" => TokenKind::Fn,
            "component" => TokenKind::Component,
            "render" => TokenKind::Render,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "impl" => TokenKind::Impl,
            "trait" => TokenKind::Trait,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "match" => TokenKind::Match,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "while" => TokenKind::While,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "return" => TokenKind::Return,
            "own" => TokenKind::Own,
            "ref" => TokenKind::Ref,
            "self" => TokenKind::SelfKw,
            "Self" => TokenKind::SelfType,
            "pub" => TokenKind::Pub,
            "use" => TokenKind::Use,
            "mod" => TokenKind::Mod,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "signal" => TokenKind::Signal,
            "store" => TokenKind::Store,
            "action" => TokenKind::Action,
            "effect" => TokenKind::Effect,
            "computed" => TokenKind::Computed,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
            "fetch" => TokenKind::Fetch,
            "derive" => TokenKind::Derive,
            "spawn" => TokenKind::Spawn,
            "channel" => TokenKind::Channel,
            "select" => TokenKind::Select,
            "parallel" => TokenKind::Parallel,
            "stream" => TokenKind::Stream,
            "on_message" => TokenKind::OnMessage,
            "on_connect" => TokenKind::OnConnect,
            "on_disconnect" => TokenKind::OnDisconnect,
            "lazy" => TokenKind::Lazy,
            "inplace" => TokenKind::Inplace,
            "suspend" => TokenKind::Suspend,
            "yield" => TokenKind::Yield,
            "agent" => TokenKind::Agent,
            "prompt" => TokenKind::Prompt,
            "tool" => TokenKind::Tool,
            "route" => TokenKind::Route,
            "link" => TokenKind::Link,
            "navigate" => TokenKind::Navigate,
            "router" => TokenKind::Router,
            "fallback" => TokenKind::Fallback,
            "guard" => TokenKind::Guard,
            "style" => TokenKind::Style,
            "try" => TokenKind::Try,
            "catch" => TokenKind::Catch,
            "test" => TokenKind::Test,
            "assert" => TokenKind::Assert,
            "expect" => TokenKind::Expect,
            "assert_eq" => TokenKind::AssertEq,
            "transition" => TokenKind::Transition,
            "animate" => TokenKind::Animate,
            "contract" => TokenKind::Contract,
            "app" => TokenKind::App,
            "manifest" => TokenKind::Manifest,
            "offline" => TokenKind::Offline,
            "push" => TokenKind::Push,
            "gesture" => TokenKind::Gesture,
            "haptic" => TokenKind::Haptic,
            "biometric" => TokenKind::Biometric,
            "camera" => TokenKind::Camera,
            "geolocation" => TokenKind::Geolocation,
            "as" => TokenKind::As,
            "where" => TokenKind::Where,
            "secret" => TokenKind::Secret,
            "permissions" => TokenKind::Permissions,
            "page" => TokenKind::Page,
            "meta" => TokenKind::Meta,
            "sitemap" => TokenKind::Sitemap,
            "schema" => TokenKind::Schema,
            "canonical" => TokenKind::Canonical,
            "form" => TokenKind::Form,
            "field" => TokenKind::Field,
            "validate" => TokenKind::Validate,
            "must_use" => TokenKind::MustUse,
            "chunk" => TokenKind::Chunk,
            "atomic" => TokenKind::Atomic,
            "selector" => TokenKind::Selector,
            "embed" => TokenKind::Embed,
            "sandbox" => TokenKind::Sandbox,
            "loading" => TokenKind::Loading,
            "instant" => TokenKind::Instant,
            "duration" => TokenKind::Duration,
            "pdf" => TokenKind::Pdf,
            "download" => TokenKind::Download,
            "payment" => TokenKind::Payment,
            "miniprogram" => TokenKind::MiniProgram,
            "banking" => TokenKind::Banking,
            "map" => TokenKind::MapKeyword,
            "auth" => TokenKind::Auth,
            "upload" => TokenKind::Upload,
            "env" => TokenKind::Env,
            "db" => TokenKind::Db,
            "trace" => TokenKind::Trace,
            "flag" => TokenKind::Flag,
            "cache" => TokenKind::Cache,
            "query" => TokenKind::Query,
            "mutation" => TokenKind::Mutation,
            "invalidate" => TokenKind::Invalidate,
            "optimistic" => TokenKind::Optimistic,
            "breakpoint" => TokenKind::Breakpoint,
            "fluid" => TokenKind::Fluid,
            "clipboard" => TokenKind::Clipboard,
            "draggable" => TokenKind::Draggable,
            "droppable" => TokenKind::Droppable,
            "a11y" => TokenKind::A11y,
            "manual" => TokenKind::Manual,
            "hybrid" => TokenKind::Hybrid,
            "layout" => TokenKind::Layout,
            "outlet" => TokenKind::Outlet,
            "crypto" => TokenKind::Crypto,
            "theme" => TokenKind::Theme,
            "spring" => TokenKind::Spring,
            "stagger" => TokenKind::Stagger,
            "keyframes" => TokenKind::Keyframes,
            "shortcut" => TokenKind::Shortcut,
            "virtual" => TokenKind::Virtual,
            "i32" => TokenKind::I32,
            "i64" => TokenKind::I64,
            "f32" => TokenKind::F32,
            "f64" => TokenKind::F64,
            "u32" => TokenKind::U32,
            "u64" => TokenKind::U64,
            "bool" => TokenKind::Bool_,
            "String" => TokenKind::StringType,
            _ => TokenKind::Ident(ident),
        }
    }

    fn skip_whitespace(&mut self) {
        while !self.is_eof() && self.peek().is_whitespace() {
            if self.peek() == '\n' {
                self.line += 1;
                self.col = 0;
            }
            self.advance();
        }
    }

    fn skip_line_comment(&mut self) {
        while !self.is_eof() && self.peek() != '\n' {
            self.advance();
        }
    }

    fn peek(&self) -> char {
        self.source.get(self.pos).copied().unwrap_or('\0')
    }

    fn peek_next(&self) -> char {
        self.source.get(self.pos + 1).copied().unwrap_or('\0')
    }

    fn advance(&mut self) -> char {
        let ch = self.source[self.pos];
        self.pos += 1;
        self.col += 1;
        ch
    }

    fn match_char(&mut self, expected: char) -> bool {
        if !self.is_eof() && self.peek() == expected {
            self.advance();
            true
        } else {
            false
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.source.len()
    }
}

#[derive(Debug)]
pub struct LexError {
    pub message: String,
    pub line: u32,
    pub col: u32,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:{}] {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for LexError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let mut lexer = Lexer::new("let x = 42;");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Let);
        assert_eq!(tokens[1].kind, TokenKind::Ident("x".into()));
        assert_eq!(tokens[2].kind, TokenKind::Equals);
        assert_eq!(tokens[3].kind, TokenKind::Integer(42));
        assert_eq!(tokens[4].kind, TokenKind::Semicolon);
    }

    #[test]
    fn test_component_keyword() {
        let mut lexer = Lexer::new("component App {}");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Component);
        assert_eq!(tokens[1].kind, TokenKind::Ident("App".into()));
    }

    #[test]
    fn test_string_literal() {
        let mut lexer = Lexer::new("\"hello world\"");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::StringLit("hello world".into()));
    }

    #[test]
    fn test_operators() {
        let mut lexer = Lexer::new("-> => == != && ||");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Arrow);
        assert_eq!(tokens[1].kind, TokenKind::FatArrow);
        assert_eq!(tokens[2].kind, TokenKind::DoubleEquals);
        assert_eq!(tokens[3].kind, TokenKind::NotEquals);
        assert_eq!(tokens[4].kind, TokenKind::AmpAmp);
        assert_eq!(tokens[5].kind, TokenKind::PipePipe);
    }

    #[test]
    fn test_float_literal() {
        let mut lexer = Lexer::new("3.14");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Float(3.14));
    }

    #[test]
    fn test_format_string_simple() {
        let mut lexer = Lexer::new("f\"hello {name}\"");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::FormatString(vec![
                FormatStringPart::Lit("hello ".into()),
                FormatStringPart::Expr("name".into()),
            ])
        );
    }

    #[test]
    fn test_format_string_multiple_interpolations() {
        let mut lexer = Lexer::new("f\"hello {name}, you are {age} years old\"");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::FormatString(vec![
                FormatStringPart::Lit("hello ".into()),
                FormatStringPart::Expr("name".into()),
                FormatStringPart::Lit(", you are ".into()),
                FormatStringPart::Expr("age".into()),
                FormatStringPart::Lit(" years old".into()),
            ])
        );
    }

    #[test]
    fn test_format_string_expression() {
        let mut lexer = Lexer::new("f\"result: {a + b}\"");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::FormatString(vec![
                FormatStringPart::Lit("result: ".into()),
                FormatStringPart::Expr("a + b".into()),
            ])
        );
    }

    #[test]
    fn test_format_string_no_interpolation() {
        let mut lexer = Lexer::new("f\"just a string\"");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::FormatString(vec![
                FormatStringPart::Lit("just a string".into()),
            ])
        );
    }

    #[test]
    fn test_format_string_escaped_brace() {
        let mut lexer = Lexer::new("f\"value: \\{not interpolated\\}\"");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::FormatString(vec![
                FormatStringPart::Lit("value: {not interpolated}".into()),
            ])
        );
    }

    #[test]
    fn test_format_string_only_interpolation() {
        let mut lexer = Lexer::new("f\"{x}\"");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::FormatString(vec![
                FormatStringPart::Expr("x".into()),
            ])
        );
    }

    #[test]
    fn test_f_identifier_not_format_string() {
        // `foo"` should not be parsed as a format string
        let mut lexer = Lexer::new("foo");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ident("foo".into()));
    }

    #[test]
    fn test_lifetime_tokens() {
        let mut lexer = Lexer::new("'a 'b 'static");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Lifetime("a".into()));
        assert_eq!(tokens[1].kind, TokenKind::Lifetime("b".into()));
        assert_eq!(tokens[2].kind, TokenKind::Lifetime("static".into()));
    }

    #[test]
    fn test_lifetime_in_reference_type() {
        let mut lexer = Lexer::new("&'a mut T");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ampersand);
        assert_eq!(tokens[1].kind, TokenKind::Lifetime("a".into()));
        assert_eq!(tokens[2].kind, TokenKind::Mut);
        assert_eq!(tokens[3].kind, TokenKind::Ident("T".into()));
    }

    #[test]
    fn test_lifetime_in_angle_brackets() {
        let mut lexer = Lexer::new("<'a, T>");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::LeftAngle);
        assert_eq!(tokens[1].kind, TokenKind::Lifetime("a".into()));
        assert_eq!(tokens[2].kind, TokenKind::Comma);
        assert_eq!(tokens[3].kind, TokenKind::Ident("T".into()));
        assert_eq!(tokens[4].kind, TokenKind::RightAngle);
    }

    #[test]
    fn test_question_mark_token() {
        let mut lexer = Lexer::new("x?");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ident("x".into()));
        assert_eq!(tokens[1].kind, TokenKind::QuestionMark);
    }

    // ========================================================================
    // COMPREHENSIVE LEXER COVERAGE TESTS
    // ========================================================================

    #[test]
    fn test_all_single_char_tokens() {
        let mut lexer = Lexer::new("( ) { } [ ] , ; . ? %");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::LeftParen);
        assert_eq!(tokens[1].kind, TokenKind::RightParen);
        assert_eq!(tokens[2].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[3].kind, TokenKind::RightBrace);
        assert_eq!(tokens[4].kind, TokenKind::LeftBracket);
        assert_eq!(tokens[5].kind, TokenKind::RightBracket);
        assert_eq!(tokens[6].kind, TokenKind::Comma);
        assert_eq!(tokens[7].kind, TokenKind::Semicolon);
        assert_eq!(tokens[8].kind, TokenKind::Dot);
        assert_eq!(tokens[9].kind, TokenKind::QuestionMark);
        assert_eq!(tokens[10].kind, TokenKind::Percent);
    }

    #[test]
    fn test_ampersand_tokens() {
        let mut lexer = Lexer::new("& &&");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ampersand);
        assert_eq!(tokens[1].kind, TokenKind::AmpAmp);
    }

    #[test]
    fn test_pipe_tokens() {
        let mut lexer = Lexer::new("| ||");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Pipe);
        assert_eq!(tokens[1].kind, TokenKind::PipePipe);
    }

    #[test]
    fn test_colon_tokens() {
        let mut lexer = Lexer::new(": ::");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Colon);
        assert_eq!(tokens[1].kind, TokenKind::ColonColon);
    }

    #[test]
    fn test_plus_tokens() {
        let mut lexer = Lexer::new("+ +=");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Plus);
        assert_eq!(tokens[1].kind, TokenKind::PlusEquals);
    }

    #[test]
    fn test_minus_tokens() {
        let mut lexer = Lexer::new("- -> -=");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Minus);
        assert_eq!(tokens[1].kind, TokenKind::Arrow);
        assert_eq!(tokens[2].kind, TokenKind::MinusEquals);
    }

    #[test]
    fn test_star_tokens() {
        let mut lexer = Lexer::new("* *=");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Star);
        assert_eq!(tokens[1].kind, TokenKind::StarEquals);
    }

    #[test]
    fn test_slash_tokens() {
        let mut lexer = Lexer::new("/ /=");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Slash);
        assert_eq!(tokens[1].kind, TokenKind::SlashEquals);
    }

    #[test]
    fn test_equals_tokens() {
        let mut lexer = Lexer::new("= == =>");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Equals);
        assert_eq!(tokens[1].kind, TokenKind::DoubleEquals);
        assert_eq!(tokens[2].kind, TokenKind::FatArrow);
    }

    #[test]
    fn test_bang_tokens() {
        let mut lexer = Lexer::new("! !=");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Bang);
        assert_eq!(tokens[1].kind, TokenKind::NotEquals);
    }

    #[test]
    fn test_angle_tokens() {
        let mut lexer = Lexer::new("< <= > >=");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::LeftAngle);
        assert_eq!(tokens[1].kind, TokenKind::LessEqual);
        assert_eq!(tokens[2].kind, TokenKind::RightAngle);
        assert_eq!(tokens[3].kind, TokenKind::GreaterEqual);
    }

    #[test]
    fn test_string_escape_sequences() {
        let mut lexer = Lexer::new(r#""hello\nworld\t\\\"""#);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::StringLit("hello\nworld\t\\\"".into()));
    }

    #[test]
    fn test_string_unknown_escape() {
        let mut lexer = Lexer::new(r#""\q""#);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::StringLit("q".into()));
    }

    #[test]
    fn test_unterminated_string() {
        let mut lexer = Lexer::new("\"hello");
        let result = lexer.tokenize();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unterminated"));
    }

    #[test]
    fn test_number_with_underscores() {
        let mut lexer = Lexer::new("1_000_000");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Integer(1000000));
    }

    #[test]
    fn test_float_number() {
        let mut lexer = Lexer::new("42.5");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Float(42.5));
    }

    #[test]
    fn test_line_comments_skipped() {
        let mut lexer = Lexer::new("// this is a comment\nlet x = 1;");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Let);
    }

    #[test]
    fn test_unexpected_character() {
        let mut lexer = Lexer::new("~");
        let result = lexer.tokenize();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unexpected character"));
    }

    #[test]
    fn test_at_token() {
        let mut lexer = Lexer::new("@md");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 3); // At + Ident("md") + EOF
        assert!(matches!(tokens[0].kind, TokenKind::At));
        assert!(matches!(&tokens[1].kind, TokenKind::Ident(s) if s == "md"));
    }

    #[test]
    fn test_single_quote_token() {
        let mut lexer = Lexer::new("'");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 2); // SingleQuote + EOF
        assert!(matches!(tokens[0].kind, TokenKind::SingleQuote));
    }

    #[test]
    fn test_lex_error_display() {
        let err = LexError {
            message: "test error".into(),
            line: 5,
            col: 10,
        };
        assert_eq!(format!("{}", err), "[5:10] test error");
    }

    #[test]
    fn test_format_string_escapes() {
        let mut lexer = Lexer::new(r#"f"tab:\there\n""#);
        let tokens = lexer.tokenize().unwrap();
        if let TokenKind::FormatString(parts) = &tokens[0].kind {
            if let FormatStringPart::Lit(s) = &parts[0] {
                assert!(s.contains('\t'));
                assert!(s.contains('\n'));
            }
        } else {
            panic!("Expected FormatString");
        }
    }

    #[test]
    fn test_unterminated_format_string() {
        let mut lexer = Lexer::new("f\"hello {name");
        let result = lexer.tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn test_format_string_nested_braces() {
        let mut lexer = Lexer::new("f\"result: {foo(a, {b})}\"");
        let tokens = lexer.tokenize().unwrap();
        if let TokenKind::FormatString(parts) = &tokens[0].kind {
            assert_eq!(parts.len(), 2);
            if let FormatStringPart::Expr(e) = &parts[1] {
                assert!(e.contains("{b}"));
            }
        } else {
            panic!("Expected FormatString");
        }
    }

    #[test]
    fn test_all_keywords() {
        let keywords = vec![
            ("let", TokenKind::Let), ("mut", TokenKind::Mut), ("fn", TokenKind::Fn),
            ("component", TokenKind::Component), ("render", TokenKind::Render),
            ("struct", TokenKind::Struct), ("enum", TokenKind::Enum),
            ("impl", TokenKind::Impl), ("trait", TokenKind::Trait),
            ("if", TokenKind::If), ("else", TokenKind::Else),
            ("match", TokenKind::Match), ("for", TokenKind::For),
            ("in", TokenKind::In), ("while", TokenKind::While),
            ("return", TokenKind::Return), ("own", TokenKind::Own),
            ("ref", TokenKind::Ref), ("self", TokenKind::SelfKw),
            ("Self", TokenKind::SelfType), ("pub", TokenKind::Pub),
            ("use", TokenKind::Use), ("mod", TokenKind::Mod),
            ("true", TokenKind::True), ("false", TokenKind::False),
            ("signal", TokenKind::Signal), ("store", TokenKind::Store),
            ("action", TokenKind::Action), ("effect", TokenKind::Effect),
            ("computed", TokenKind::Computed), ("async", TokenKind::Async),
            ("await", TokenKind::Await), ("fetch", TokenKind::Fetch),
            ("derive", TokenKind::Derive), ("spawn", TokenKind::Spawn),
            ("channel", TokenKind::Channel), ("select", TokenKind::Select),
            ("parallel", TokenKind::Parallel), ("stream", TokenKind::Stream),
            ("on_message", TokenKind::OnMessage),
            ("on_connect", TokenKind::OnConnect),
            ("on_disconnect", TokenKind::OnDisconnect),
            ("lazy", TokenKind::Lazy), ("suspend", TokenKind::Suspend),
            ("yield", TokenKind::Yield), ("agent", TokenKind::Agent),
            ("prompt", TokenKind::Prompt), ("tool", TokenKind::Tool),
            ("route", TokenKind::Route), ("link", TokenKind::Link),
            ("navigate", TokenKind::Navigate), ("router", TokenKind::Router),
            ("fallback", TokenKind::Fallback), ("guard", TokenKind::Guard),
            ("style", TokenKind::Style), ("try", TokenKind::Try),
            ("catch", TokenKind::Catch), ("test", TokenKind::Test),
            ("assert", TokenKind::Assert), ("expect", TokenKind::Expect),
            ("assert_eq", TokenKind::AssertEq),
            ("transition", TokenKind::Transition),
            ("animate", TokenKind::Animate), ("contract", TokenKind::Contract),
            ("app", TokenKind::App), ("manifest", TokenKind::Manifest),
            ("offline", TokenKind::Offline), ("push", TokenKind::Push),
            ("gesture", TokenKind::Gesture), ("haptic", TokenKind::Haptic),
            ("biometric", TokenKind::Biometric), ("camera", TokenKind::Camera),
            ("geolocation", TokenKind::Geolocation), ("as", TokenKind::As),
            ("where", TokenKind::Where), ("secret", TokenKind::Secret),
            ("permissions", TokenKind::Permissions), ("page", TokenKind::Page),
            ("meta", TokenKind::Meta), ("sitemap", TokenKind::Sitemap),
            ("schema", TokenKind::Schema), ("canonical", TokenKind::Canonical),
            ("form", TokenKind::Form), ("field", TokenKind::Field),
            ("validate", TokenKind::Validate), ("must_use", TokenKind::MustUse),
            ("chunk", TokenKind::Chunk), ("atomic", TokenKind::Atomic),
            ("selector", TokenKind::Selector), ("embed", TokenKind::Embed),
            ("sandbox", TokenKind::Sandbox), ("loading", TokenKind::Loading),
            ("instant", TokenKind::Instant), ("duration", TokenKind::Duration),
            ("pdf", TokenKind::Pdf), ("download", TokenKind::Download),
            ("payment", TokenKind::Payment), ("miniprogram", TokenKind::MiniProgram),
            ("banking", TokenKind::Banking),
            ("map", TokenKind::MapKeyword), ("auth", TokenKind::Auth),
            ("upload", TokenKind::Upload), ("env", TokenKind::Env),
            ("db", TokenKind::Db), ("trace", TokenKind::Trace),
            ("flag", TokenKind::Flag), ("cache", TokenKind::Cache),
            ("query", TokenKind::Query), ("mutation", TokenKind::Mutation),
            ("invalidate", TokenKind::Invalidate),
            ("optimistic", TokenKind::Optimistic),
            ("breakpoint", TokenKind::Breakpoint), ("fluid", TokenKind::Fluid),
            ("clipboard", TokenKind::Clipboard),
            ("draggable", TokenKind::Draggable),
            ("droppable", TokenKind::Droppable), ("a11y", TokenKind::A11y),
            ("manual", TokenKind::Manual), ("hybrid", TokenKind::Hybrid),
            ("layout", TokenKind::Layout), ("outlet", TokenKind::Outlet),
            ("crypto", TokenKind::Crypto),
            ("theme", TokenKind::Theme), ("spring", TokenKind::Spring),
            ("stagger", TokenKind::Stagger), ("keyframes", TokenKind::Keyframes),
            ("shortcut", TokenKind::Shortcut), ("virtual", TokenKind::Virtual),
        ];
        for (src, expected) in keywords {
            let mut lexer = Lexer::new(src);
            let tokens = lexer.tokenize().unwrap();
            assert_eq!(tokens[0].kind, expected, "keyword '{}' mismatch", src);
        }
    }

    #[test]
    fn test_type_keywords() {
        let types = vec![
            ("i32", TokenKind::I32), ("i64", TokenKind::I64),
            ("f32", TokenKind::F32), ("f64", TokenKind::F64),
            ("u32", TokenKind::U32), ("u64", TokenKind::U64),
            ("bool", TokenKind::Bool_), ("String", TokenKind::StringType),
        ];
        for (src, expected) in types {
            let mut lexer = Lexer::new(src);
            let tokens = lexer.tokenize().unwrap();
            assert_eq!(tokens[0].kind, expected, "type '{}' mismatch", src);
        }
    }

    #[test]
    fn test_identifier_with_underscore() {
        let mut lexer = Lexer::new("_foo _bar_baz __x");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ident("_foo".into()));
        assert_eq!(tokens[1].kind, TokenKind::Ident("_bar_baz".into()));
        assert_eq!(tokens[2].kind, TokenKind::Ident("__x".into()));
    }

    #[test]
    fn test_empty_input() {
        let mut lexer = Lexer::new("");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn test_whitespace_only() {
        let mut lexer = Lexer::new("   \n\t  \n  ");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn test_span_tracking() {
        let mut lexer = Lexer::new("let x");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].span.line, 1);
        assert_eq!(tokens[0].span.col, 1);
    }

    #[test]
    fn test_multiline_span() {
        let mut lexer = Lexer::new("let\nx");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].span.line, 1);
        assert_eq!(tokens[1].span.line, 2);
    }

    #[test]
    fn test_f_ident_followed_by_space() {
        // `f` not followed by `"` should be a normal identifier
        let mut lexer = Lexer::new("f x");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ident("f".into()));
        assert_eq!(tokens[1].kind, TokenKind::Ident("x".into()));
    }

    #[test]
    fn test_format_string_backslash_escapes() {
        let mut lexer = Lexer::new(r#"f"a\\b\"c""#);
        let tokens = lexer.tokenize().unwrap();
        if let TokenKind::FormatString(parts) = &tokens[0].kind {
            if let FormatStringPart::Lit(s) = &parts[0] {
                assert!(s.contains('\\'));
                assert!(s.contains('"'));
            }
        }
    }

    #[test]
    fn test_comment_at_end_of_file() {
        let mut lexer = Lexer::new("let x = 1; // end comment");
        let tokens = lexer.tokenize().unwrap();
        // Should parse the tokens before the comment, then EOF
        assert_eq!(tokens.last().unwrap().kind, TokenKind::Eof);
        assert!(tokens.len() > 1);
    }

    #[test]
    fn test_dot_dot_token() {
        let mut lexer = Lexer::new("0..10");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Integer(0));
        assert_eq!(tokens[1].kind, TokenKind::DotDot);
        assert_eq!(tokens[2].kind, TokenKind::Integer(10));
    }

    #[test]
    fn test_question_dot_token() {
        let mut lexer = Lexer::new("a?.b");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ident("a".into()));
        assert_eq!(tokens[1].kind, TokenKind::QuestionDot);
        assert_eq!(tokens[2].kind, TokenKind::Ident("b".into()));
    }

    #[test]
    fn test_question_dot_vs_question_mark() {
        let mut lexer = Lexer::new("x? y?.z");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ident("x".into()));
        assert_eq!(tokens[1].kind, TokenKind::QuestionMark);
        assert_eq!(tokens[2].kind, TokenKind::Ident("y".into()));
        assert_eq!(tokens[3].kind, TokenKind::QuestionDot);
        assert_eq!(tokens[4].kind, TokenKind::Ident("z".into()));
    }

    #[test]
    fn test_dot_dot_vs_single_dot() {
        let mut lexer = Lexer::new("a.b 0..10");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ident("a".into()));
        assert_eq!(tokens[1].kind, TokenKind::Dot);
        assert_eq!(tokens[2].kind, TokenKind::Ident("b".into()));
        assert_eq!(tokens[3].kind, TokenKind::Integer(0));
        assert_eq!(tokens[4].kind, TokenKind::DotDot);
        assert_eq!(tokens[5].kind, TokenKind::Integer(10));
    }

    #[test]
    fn test_dot_dot_with_expressions() {
        let mut lexer = Lexer::new("start..end");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ident("start".into()));
        assert_eq!(tokens[1].kind, TokenKind::DotDot);
        assert_eq!(tokens[2].kind, TokenKind::Ident("end".into()));
    }
}
