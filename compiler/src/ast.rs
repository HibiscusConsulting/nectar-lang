use crate::token::Span;

/// Top-level program
#[derive(Debug)]
pub struct Program {
    pub items: Vec<Item>,
}

/// Top-level items
#[derive(Debug)]
pub enum Item {
    Function(Function),
    Component(Component),
    Struct(StructDef),
    Enum(EnumDef),
    Impl(ImplBlock),
    Trait(TraitDef),
    Use(UsePath),
    Mod(ModDef),
    Store(StoreDef),
    Agent(AgentDef),
    Router(RouterDef),
    /// Lazy-loaded component — only loaded when visible/needed
    /// `lazy component HeavyChart { ... }`
    LazyComponent(LazyComponentDef),
    /// Test block — `test "description" { ... }`
    Test(TestDef),
}

/// Test definition — a named block of test code
#[derive(Debug)]
pub struct TestDef {
    pub name: String,
    pub body: Block,
    pub span: Span,
}

/// Trait definition — Rust-style interface with optional default method bodies
#[derive(Debug)]
pub struct TraitDef {
    pub name: String,
    pub type_params: Vec<String>,
    pub methods: Vec<TraitMethod>,
    pub span: Span,
}

/// A method declaration inside a trait (may have a default body)
#[derive(Debug)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub default_body: Option<Block>,
    pub span: Span,
}

/// A trait bound — e.g. `T: Display`
#[derive(Debug, Clone)]
pub struct TraitBound {
    pub type_param: String,
    pub trait_name: String,
}

/// Function definition
#[derive(Debug)]
pub struct Function {
    pub name: String,
    pub lifetimes: Vec<String>,
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub trait_bounds: Vec<TraitBound>,
    pub body: Block,
    pub is_pub: bool,
    pub span: Span,
}

/// Component definition — first-class UI primitive
#[derive(Debug)]
pub struct Component {
    pub name: String,
    pub type_params: Vec<String>,
    pub props: Vec<Prop>,
    pub state: Vec<StateField>,
    pub methods: Vec<Function>,
    pub styles: Vec<StyleBlock>,
    pub transitions: Vec<TransitionDef>,
    pub trait_bounds: Vec<TraitBound>,
    pub render: RenderBlock,
    pub skeleton: Option<SkeletonDef>,
    pub error_boundary: Option<ErrorBoundary>,
    pub span: Span,
}

/// Skeleton definition — placeholder UI shown while data is loading
///
/// ```nectar
/// skeleton {
///     <div class="skeleton">
///         <div class="skeleton-avatar" />
///         <div class="skeleton-line" style="width: 60%" />
///     </div>
/// }
/// ```
#[derive(Debug)]
pub struct SkeletonDef {
    pub body: RenderBlock,
    pub span: Span,
}

/// Error boundary — catches render errors and shows a fallback UI
#[derive(Debug)]
pub struct ErrorBoundary {
    pub fallback: RenderBlock,
    pub body: RenderBlock,
    pub span: Span,
}

/// Component property (immutable by default)
#[derive(Debug)]
pub struct Prop {
    pub name: String,
    pub ty: Type,
    pub default: Option<Expr>,
}

/// Component state field (reactive signal)
#[derive(Debug)]
pub struct StateField {
    pub name: String,
    pub ty: Option<Type>,
    pub mutable: bool,
    pub initializer: Expr,
    pub ownership: Ownership,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ownership {
    Owned,
    Borrowed,
    MutBorrowed,
}

/// Render block — produces a template tree
#[derive(Debug)]
pub struct RenderBlock {
    pub body: TemplateNode,
    pub span: Span,
}

/// Template nodes — the JSX-like part
#[derive(Debug)]
pub enum TemplateNode {
    Element(Element),
    TextLiteral(String),
    Expression(Box<Expr>),
    Fragment(Vec<TemplateNode>),
    /// <Link to="/about">About</Link>
    Link { to: Expr, children: Vec<TemplateNode> },
}

#[derive(Debug)]
pub struct Element {
    pub tag: String,
    pub attributes: Vec<Attribute>,
    pub children: Vec<TemplateNode>,
    pub span: Span,
}

#[derive(Debug)]
pub enum Attribute {
    Static { name: String, value: String },
    Dynamic { name: String, value: Expr },
    EventHandler { event: String, handler: Expr },
    /// ARIA attribute — `aria-label={expr}`, `aria-hidden="true"`, etc.
    Aria { name: String, value: Expr },
    /// Role attribute — `role="button"`, `role="navigation"`, etc.
    Role { value: String },
    /// Two-way form binding — `bind:value={count}`, `bind:checked={is_active}`, etc.
    /// Sets the initial property from the signal, creates an effect to keep DOM
    /// in sync, and adds an input/change listener to push user edits back.
    Bind { property: String, signal: String },
}

/// Struct definition
#[derive(Debug)]
pub struct StructDef {
    pub name: String,
    pub lifetimes: Vec<String>,
    pub type_params: Vec<String>,
    pub fields: Vec<Field>,
    pub trait_bounds: Vec<TraitBound>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub is_pub: bool,
}

/// Enum definition
#[derive(Debug)]
pub struct EnumDef {
    pub name: String,
    pub type_params: Vec<String>,
    pub variants: Vec<Variant>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<Type>,
}

/// Impl block
#[derive(Debug)]
pub struct ImplBlock {
    pub target: String,
    pub trait_impls: Vec<String>,
    pub methods: Vec<Function>,
    pub span: Span,
}

/// Store — global reactive state container (Flux/Redux-like)
///
/// ```nectar
/// store AppStore {
///     signal count: i32 = 0;
///     signal user: Option<User> = None;
///
///     action increment(&mut self) {
///         self.count = self.count + 1;
///     }
///
///     computed double_count(&self) -> i32 {
///         self.count * 2
///     }
///
///     async action fetch_user(&mut self, id: u32) {
///         let response = fetch("https://api.example.com/users", {
///             method: "GET",
///         });
///         self.user = response.json();
///     }
///
///     effect on_count_change(&self) {
///         println(self.count);
///     }
/// }
/// ```
#[derive(Debug)]
pub struct StoreDef {
    pub name: String,
    pub signals: Vec<StateField>,
    pub actions: Vec<ActionDef>,
    pub computed: Vec<ComputedDef>,
    pub effects: Vec<EffectDef>,
    pub is_pub: bool,
    pub span: Span,
}

/// Store action — a method that can mutate store state
#[derive(Debug)]
pub struct ActionDef {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
    pub is_async: bool,
    pub span: Span,
}

/// Computed value — derived from signals, cached and auto-updated
#[derive(Debug)]
pub struct ComputedDef {
    pub name: String,
    pub return_type: Option<Type>,
    pub body: Block,
    pub span: Span,
}

/// Effect — side effect that runs when dependencies change
#[derive(Debug)]
pub struct EffectDef {
    pub name: String,
    pub body: Block,
    pub span: Span,
}

/// AI Agent — a component that wraps an LLM interaction
///
/// ```nectar
/// agent Assistant {
///     prompt system = "You are a helpful assistant.";
///
///     tool search(query: String) -> [Result] {
///         // This function can be called by the AI
///         return SearchService::search(query);
///     }
///
///     tool calculate(expr: String) -> f64 {
///         return eval(expr);
///     }
///
///     render {
///         <div>
///             <ChatMessages messages={self.messages} />
///             <input on:submit={self.send} />
///         </div>
///     }
/// }
/// ```
#[derive(Debug)]
pub struct AgentDef {
    pub name: String,
    pub system_prompt: Option<String>,
    pub tools: Vec<ToolDef>,
    pub state: Vec<StateField>,
    pub methods: Vec<Function>,
    pub render: Option<RenderBlock>,
    pub span: Span,
}

/// Tool definition — a function exposed for AI to call
#[derive(Debug)]
pub struct ToolDef {
    pub name: String,
    pub description: Option<String>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Block,
    pub span: Span,
}

/// Router definition — maps URL paths to components
///
/// ```nectar
/// router AppRouter {
///     route "/" => Home,
///     route "/about" => About,
///     route "/user/:id" => UserProfile,
///     route "/admin/*" => AdminPanel guard { AuthStore::is_logged_in() },
///     fallback => NotFound,
/// }
/// ```
#[derive(Debug)]
pub struct RouterDef {
    pub name: String,
    pub routes: Vec<RouteDef>,
    pub fallback: Option<Box<TemplateNode>>,
    pub span: Span,
}

/// A single route definition — path pattern mapped to a component
#[derive(Debug)]
pub struct RouteDef {
    pub path: String,
    pub params: Vec<String>,      // extracted from :param segments in path
    pub component: String,        // component to render
    pub guard: Option<Expr>,      // optional auth/permission guard
    pub span: Span,
}

/// Lazy-loaded component — only fetched/compiled when first rendered
///
/// ```nectar
/// lazy component HeavyChart(data: [f64]) {
///     render {
///         <canvas />
///     }
/// }
/// ```
#[derive(Debug)]
pub struct LazyComponentDef {
    pub component: Component,
    pub span: Span,
}

/// Scoped CSS style block within a component
#[derive(Debug)]
pub struct StyleBlock {
    pub selector: String,
    pub properties: Vec<(String, String)>,
    pub span: Span,
}

/// Transition definition — CSS transition on a property
#[derive(Debug)]
pub struct TransitionDef {
    pub property: String,
    pub duration: String,
    pub easing: String,
    pub span: Span,
}

/// Animation definition — keyframe animation
#[derive(Debug)]
pub struct AnimationDef {
    pub name: String,
    pub keyframes: Vec<Keyframe>,
    pub duration: String,
    pub easing: String,
    pub iterations: Option<String>,
    pub span: Span,
}

/// A single keyframe in a keyframe animation
#[derive(Debug)]
pub struct Keyframe {
    pub offset: f64,
    pub properties: Vec<(String, String)>,
}

/// Module definition — `mod foo;` (external) or `mod foo { ... }` (inline)
#[derive(Debug)]
pub struct ModDef {
    pub name: String,
    /// `Some(items)` for inline `mod foo { ... }`, `None` for `mod foo;` (external file)
    pub items: Option<Vec<Item>>,
    pub is_external: bool,
    pub span: Span,
}

/// Use/import path
#[derive(Debug)]
pub struct UsePath {
    pub segments: Vec<String>,
    /// Optional alias: `use foo::Bar as Baz;`
    pub alias: Option<String>,
    /// Whether this is a glob import: `use foo::*;`
    pub glob: bool,
    /// Group imports: `use foo::{A, B, C};`
    pub group: Option<Vec<UseGroupItem>>,
    pub span: Span,
}

/// A single item in a `use foo::{A, B as C}` group import
#[derive(Debug)]
pub struct UseGroupItem {
    pub name: String,
    pub alias: Option<String>,
}

/// Type representation
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Named(String),
    /// A generic type application, e.g. `Vec<i32>`, `HashMap<String, User>`
    Generic { name: String, args: Vec<Type> },
    Reference { mutable: bool, lifetime: Option<String>, inner: Box<Type> },
    Array(Box<Type>),
    Option(Box<Type>),
    Result { ok: Box<Type>, err: Box<Type> },
    Tuple(Vec<Type>),
    Function { params: Vec<Type>, ret: Box<Type> },
}

/// Function/method parameter
#[derive(Debug)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub ownership: Ownership,
}

/// Block of statements
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

/// Statements
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        name: String,
        ty: Option<Type>,
        mutable: bool,
        value: Expr,
        ownership: Ownership,
    },
    Signal {
        name: String,
        ty: Option<Type>,
        value: Expr,
    },
    /// Destructuring let — `let (a, b) = expr;`, `let User { name, .. } = expr;`, etc.
    LetDestructure {
        pattern: Pattern,
        ty: Option<Type>,
        value: Expr,
    },
    Expr(Expr),
    Return(Option<Expr>),
    /// Yield from a stream — `yield chunk;`
    Yield(Expr),
}

/// Expressions
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    // Literals
    Integer(i64),
    Float(f64),
    StringLit(String),
    Bool(bool),

    // Variables
    Ident(String),
    SelfExpr,

    // Operations
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },

    // Access
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    FnCall {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },

    // Control flow
    If {
        condition: Box<Expr>,
        then_block: Block,
        else_block: Option<Block>,
    },
    Match {
        subject: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    For {
        binding: String,
        iterator: Box<Expr>,
        body: Block,
    },
    While {
        condition: Box<Expr>,
        body: Block,
    },
    Block(Block),

    // Ownership
    Borrow(Box<Expr>),
    BorrowMut(Box<Expr>),

    // Struct construction
    StructInit {
        name: String,
        fields: Vec<(String, Expr)>,
    },

    // Assignment
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },

    // Async/Await
    Await(Box<Expr>),

    // HTTP fetch — first-class API communication
    // fetch(url, { method, headers, body })
    Fetch {
        url: Box<Expr>,
        options: Option<Box<Expr>>,
    },

    // Closure / lambda
    Closure {
        params: Vec<(String, Option<Type>)>,
        body: Box<Expr>,
    },

    // AI prompt template literal with interpolation
    // prompt "Summarize this: {document}"
    PromptTemplate {
        template: String,
        interpolations: Vec<(String, Expr)>,
    },

    // Programmatic navigation
    // navigate("/user/42")
    Navigate { path: Box<Expr> },

    // Streaming — iterate over async data as it arrives
    // for chunk in stream fetch("https://api.openai.com/chat") { ... }
    Stream { source: Box<Expr> },

    // Suspend with fallback — show fallback while loading
    // suspend(<LoadingSpinner />) { <HeavyComponent /> }
    Suspend { fallback: Box<Expr>, body: Box<Expr> },

    // Concurrency primitives
    Spawn { body: Box<Expr> },
    Channel { ty: Option<Type> },
    Send { channel: Box<Expr>, value: Box<Expr> },
    Receive { channel: Box<Expr> },
    Parallel { exprs: Vec<Expr> },

    // Error handling
    TryCatch {
        body: Box<Expr>,
        error_binding: String,
        catch_body: Box<Expr>,
    },

    // Testing assertions
    Assert { condition: Box<Expr>, message: Option<String> },
    AssertEq { left: Box<Expr>, right: Box<Expr>, message: Option<String> },

    // Animation — trigger a named animation on a target element
    // animate(target, "animationName")
    Animate { target: Box<Expr>, animation: String },

    // Format string interpolation — general-purpose string building
    // f"hello {name}, you are {age} years old"
    FormatString { parts: Vec<FormatPart> },

    /// `?` error propagation operator — `expr?`
    /// Unwraps Result<T,E> or Option<T>, propagating the error/None on failure.
    Try(Box<Expr>),
}

/// A segment in a format string expression.
#[derive(Debug, Clone, PartialEq)]
pub enum FormatPart {
    /// A literal text segment.
    Literal(String),
    /// An interpolated expression.
    Expression(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Neq, Lt, Gt, Lte, Gte,
    And, Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard,
    Ident(String),
    Literal(Expr),
    Variant { name: String, fields: Vec<Pattern> },
    /// Tuple destructuring — `(a, b, c)`
    Tuple(Vec<Pattern>),
    /// Struct destructuring — `User { name, age, .. }`
    Struct { name: String, fields: Vec<(String, Pattern)>, rest: bool },
    /// Array destructuring — `[first, second, ..]`
    Array(Vec<Pattern>),
}
