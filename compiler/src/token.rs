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
    Hash,         // #
    SingleQuote,  // '

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
