/// Token types for the Nectar language
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Integer(i64),
    Float(f64),
    StringLit(String),
    #[allow(dead_code)]
    Bool(bool),

    // Identifiers & keywords
    Ident(String),

    // Keywords
    Let,
    Mut,
    Fn,
    Component,
    Render,
    Struct,
    Enum,
    Impl,
    Trait,
    If,
    Else,
    Match,
    For,
    In,
    While,
    Return,
    Own,
    Ref,
    SelfKw,      // self
    SelfType,    // Self
    Pub,
    Use,
    Mod,
    True,
    False,
    Signal,
    Store,
    Action,
    Effect,
    Computed,
    Async,
    Await,
    Fetch,
    Derive,
    Spawn,
    Channel,
    Select,
    Parallel,
    Stream,
    OnMessage,
    OnConnect,
    OnDisconnect,
    Lazy,
    Suspend,
    Yield,
    Agent,
    Prompt,
    Tool,
    Route,
    Link,
    Navigate,
    Router,
    Fallback,
    Guard,
    Style,
    Try,
    Catch,
    Test,
    Assert,
    Expect,
    AssertEq,
    Transition,
    Animate,
    Contract,
    App,
    Manifest,
    Offline,
    Push,
    Gesture,
    Haptic,
    Biometric,
    Camera,
    Geolocation,
    As,
    Where,
    Secret,
    Permissions,
    Page,
    Meta,
    Sitemap,
    Schema,
    Canonical,
    Form,
    Field,
    Validate,
    MustUse,
    Chunk,
    Atomic,
    Selector,
    Embed,
    Sandbox,
    Loading,
    Instant,
    Duration,
    Pdf,
    Download,
    Payment,
    Banking,
    MapKeyword,
    Auth,
    Upload,
    Env,
    Db,
    Trace,
    Flag,
    Cache,
    Query,
    Mutation,
    Invalidate,
    Optimistic,
    Breakpoint,
    Fluid,
    Clipboard,
    Draggable,
    Droppable,
    A11y,
    Manual,
    Hybrid,
    Layout,
    Outlet,
    Crypto,
    Theme,
    Spring,
    Stagger,
    Keyframes,
    Shortcut,
    Virtual,
    Break,
    Continue,
    Inplace,

    // Types
    I32,
    I64,
    F32,
    F64,
    U32,
    U64,
    Bool_,
    StringType,

    // Symbols
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftAngle,
    RightAngle,
    Comma,
    Colon,
    ColonColon,
    Semicolon,
    Dot,
    DotDot,      // ..
    Arrow,       // ->
    FatArrow,    // =>
    Ampersand,   // &
    Pipe,        // |
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Equals,
    DoubleEquals,
    NotEquals,   // !=
    Bang,
    LessEqual,
    GreaterEqual,
    AmpAmp,     // &&
    PipePipe,   // ||
    PlusEquals,
    MinusEquals,
    StarEquals,
    SlashEquals,
    QuestionMark, // ?
    QuestionDot,  // ?.
    Hash,         // #
    SingleQuote,  // '
    At,           // @

    // JSX-like
    #[allow(dead_code)]
    TagOpen,     // <ident
    #[allow(dead_code)]
    TagClose,    // </ident>
    #[allow(dead_code)]
    TagSelfClose,// />
    On,          // on:

    // Format string: f"hello {name}, age {age}"
    // Stored as alternating literal and expression segments
    FormatString(Vec<FormatStringPart>),

    // Lifetime — `'a`, `'b`, `'static`
    Lifetime(String),

    // Special
    Eof,
}

/// A segment within a format string literal.
/// `f"hello {name}, you are {age} years old"` produces:
///   [Lit("hello "), Expr("name"), Lit(", you are "), Expr("age"), Lit(" years old")]
#[derive(Debug, Clone, PartialEq)]
pub enum FormatStringPart {
    /// A literal text segment.
    Lit(String),
    /// An expression segment (the text between `{` and `}`).
    Expr(String),
}

impl TokenKind {
    /// Returns the keyword text for contextual keywords that can be used as identifiers.
    /// Excludes structural keywords (let, fn, if, else, for, while, return, struct, enum, etc.)
    /// that would create ambiguity if used as identifiers.
    pub fn as_contextual_ident(&self) -> Option<&'static str> {
        match self {
            TokenKind::True => Some("true"), TokenKind::False => Some("false"),
            TokenKind::Component => Some("component"), TokenKind::Render => Some("render"),
            TokenKind::Store => Some("store"), TokenKind::Signal => Some("signal"),
            TokenKind::Action => Some("action"), TokenKind::Computed => Some("computed"),
            TokenKind::Effect => Some("effect"), TokenKind::Selector => Some("selector"),
            TokenKind::Router => Some("router"), TokenKind::Route => Some("route"),
            TokenKind::Page => Some("page"), TokenKind::Form => Some("form"),
            TokenKind::Channel => Some("channel"), TokenKind::Agent => Some("agent"),
            TokenKind::App => Some("app"), TokenKind::Theme => Some("theme"),
            TokenKind::Auth => Some("auth"), TokenKind::Payment => Some("payment"),
            TokenKind::Upload => Some("upload"), TokenKind::Embed => Some("embed"),
            TokenKind::Pdf => Some("pdf"), TokenKind::Db => Some("db"),
            TokenKind::Cache => Some("cache"), TokenKind::Contract => Some("contract"),
            TokenKind::Async => Some("async"), TokenKind::Await => Some("await"),
            TokenKind::Lazy => Some("lazy"), TokenKind::Spring => Some("spring"),
            TokenKind::Keyframes => Some("keyframes"), TokenKind::Stagger => Some("stagger"),
            TokenKind::Shortcut => Some("shortcut"), TokenKind::Secret => Some("secret"),
            TokenKind::Atomic => Some("atomic"), TokenKind::Select => Some("select"),
            TokenKind::Test => Some("test"), TokenKind::Navigate => Some("navigate"),
            TokenKind::Fallback => Some("fallback"), TokenKind::Guard => Some("guard"),
            TokenKind::Layout => Some("layout"), TokenKind::Breakpoint => Some("breakpoint"),
            TokenKind::Spawn => Some("spawn"), TokenKind::Suspend => Some("suspend"),
            TokenKind::MustUse => Some("must_use"), TokenKind::Yield => Some("yield"),
            TokenKind::Try => Some("try"), TokenKind::Catch => Some("catch"),
            TokenKind::Banking => Some("banking"), TokenKind::MapKeyword => Some("map"),
            TokenKind::A11y => Some("a11y"), TokenKind::Manual => Some("manual"),
            TokenKind::Hybrid => Some("hybrid"), TokenKind::Outlet => Some("outlet"),
            TokenKind::Crypto => Some("crypto"), TokenKind::Virtual => Some("virtual"),
            TokenKind::Canonical => Some("canonical"), TokenKind::Sandbox => Some("sandbox"),
            TokenKind::Loading => Some("loading"), TokenKind::Duration => Some("duration"),
            TokenKind::Invalidate => Some("invalidate"), TokenKind::Optimistic => Some("optimistic"),
            TokenKind::Validate => Some("validate"), TokenKind::Schema => Some("schema"),
            TokenKind::Instant => Some("instant"), TokenKind::Fluid => Some("fluid"),
            TokenKind::Clipboard => Some("clipboard"), TokenKind::Draggable => Some("draggable"),
            TokenKind::Droppable => Some("droppable"), TokenKind::Download => Some("download"),
            TokenKind::Haptic => Some("haptic"), TokenKind::Biometric => Some("biometric"),
            TokenKind::Camera => Some("camera"), TokenKind::Geolocation => Some("geolocation"),
            TokenKind::Flag => Some("flag"), TokenKind::Trace => Some("trace"),
            TokenKind::Env => Some("env"), TokenKind::Style => Some("style"),
            TokenKind::Push => Some("push"), TokenKind::Query => Some("query"),
            TokenKind::OnMessage => Some("on_message"), TokenKind::Chunk => Some("chunk"),
            TokenKind::Link => Some("link"), TokenKind::Fetch => Some("fetch"),
            TokenKind::Stream => Some("stream"), TokenKind::Parallel => Some("parallel"),
            TokenKind::OnConnect => Some("on_connect"), TokenKind::OnDisconnect => Some("on_disconnect"),
            TokenKind::Prompt => Some("prompt"), TokenKind::Tool => Some("tool"),
            TokenKind::Transition => Some("transition"), TokenKind::Animate => Some("animate"),
            TokenKind::Manifest => Some("manifest"), TokenKind::Offline => Some("offline"),
            TokenKind::Gesture => Some("gesture"), TokenKind::Permissions => Some("permissions"),
            TokenKind::Meta => Some("meta"), TokenKind::Sitemap => Some("sitemap"),
            TokenKind::Field => Some("field"), TokenKind::Mutation => Some("mutation"),
            TokenKind::Assert => Some("assert"), TokenKind::Expect => Some("expect"),
            TokenKind::AssertEq => Some("assert_eq"), TokenKind::Derive => Some("derive"),
            _ => None,
        }
    }

    /// Text representation for CSS selector context.
    /// All keywords return their lowercase text so class names
    /// like .app-layout, .form-row, .page-header parse correctly.
    pub fn as_css_text(&self) -> String {
        match self {
            TokenKind::Ident(s) => s.clone(),
            TokenKind::Integer(n) => n.to_string(),
            TokenKind::Let => "let".into(), TokenKind::Mut => "mut".into(),
            TokenKind::Fn => "fn".into(), TokenKind::Return => "return".into(),
            TokenKind::If => "if".into(), TokenKind::Else => "else".into(),
            TokenKind::While => "while".into(), TokenKind::For => "for".into(),
            TokenKind::In => "in".into(), TokenKind::Struct => "struct".into(),
            TokenKind::Enum => "enum".into(), TokenKind::Impl => "impl".into(),
            TokenKind::Trait => "trait".into(), TokenKind::Pub => "pub".into(),
            TokenKind::Use => "use".into(), TokenKind::Mod => "mod".into(),
            TokenKind::True => "true".into(), TokenKind::False => "false".into(),
            TokenKind::Component => "component".into(), TokenKind::Render => "render".into(),
            TokenKind::Store => "store".into(), TokenKind::Signal => "signal".into(),
            TokenKind::Action => "action".into(), TokenKind::Computed => "computed".into(),
            TokenKind::Effect => "effect".into(), TokenKind::Selector => "selector".into(),
            TokenKind::Router => "router".into(), TokenKind::Route => "route".into(),
            TokenKind::Page => "page".into(), TokenKind::Form => "form".into(),
            TokenKind::Channel => "channel".into(), TokenKind::Agent => "agent".into(),
            TokenKind::App => "app".into(), TokenKind::Theme => "theme".into(),
            TokenKind::Auth => "auth".into(), TokenKind::Payment => "payment".into(),
            TokenKind::Upload => "upload".into(), TokenKind::Embed => "embed".into(),
            TokenKind::Pdf => "pdf".into(), TokenKind::Db => "db".into(),
            TokenKind::Cache => "cache".into(), TokenKind::Contract => "contract".into(),
            TokenKind::Async => "async".into(), TokenKind::Await => "await".into(),
            TokenKind::Lazy => "lazy".into(), TokenKind::Spring => "spring".into(),
            TokenKind::Keyframes => "keyframes".into(), TokenKind::Stagger => "stagger".into(),
            TokenKind::Shortcut => "shortcut".into(), TokenKind::Secret => "secret".into(),
            TokenKind::Atomic => "atomic".into(), TokenKind::Select => "select".into(),
            TokenKind::Test => "test".into(), TokenKind::Match => "match".into(),
            TokenKind::Fallback => "fallback".into(), TokenKind::Navigate => "navigate".into(),
            TokenKind::Guard => "guard".into(), TokenKind::Layout => "layout".into(),
            TokenKind::Breakpoint => "breakpoint".into(), TokenKind::Where => "where".into(),
            TokenKind::As => "as".into(), TokenKind::SelfKw => "self".into(),
            TokenKind::Spawn => "spawn".into(), TokenKind::Suspend => "suspend".into(),
            TokenKind::MustUse => "must_use".into(), TokenKind::Yield => "yield".into(),
            TokenKind::Try => "try".into(), TokenKind::Catch => "catch".into(),
            TokenKind::Banking => "banking".into(), TokenKind::MapKeyword => "map".into(),
            TokenKind::A11y => "a11y".into(), TokenKind::Manual => "manual".into(),
            TokenKind::Hybrid => "hybrid".into(), TokenKind::Outlet => "outlet".into(),
            TokenKind::Crypto => "crypto".into(), TokenKind::Virtual => "virtual".into(),
            _ => String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub col: u32,
}

impl Span {
    pub fn new(start: usize, end: usize, line: u32, col: u32) -> Self {
        Self { start, end, line, col }
    }
}
