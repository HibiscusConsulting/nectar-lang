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
    /// Contract — API boundary type with runtime validation and content hashing.
    ///
    /// ```nectar
    /// contract CustomerResponse {
    ///     id: u32,
    ///     name: String,
    ///     email: String,
    ///     balance_cents: i64,
    ///     tier: enum { free, pro, enterprise },
    ///     addresses: [Address],
    ///     deleted_at: DateTime?,
    /// }
    /// ```
    Contract(ContractDef),
    /// PWA app definition — root-level application with manifest, offline support, push
    App(AppDef),
    /// Page definition — a component with SEO metadata
    Page(PageDef),
    /// Declarative form definition
    Form(FormDef),
    /// Channel definition — real-time WebSocket connection with handlers
    Channel(ChannelDef),
    /// Third-party embed — sandboxed external script integration
    Embed(EmbedDef),
    /// PDF document definition — generates downloadable PDF documents
    Pdf(PdfDef),
    /// Payment gateway definition — PCI-compliant payment processing
    Payment(PaymentDef),
    /// Authentication definition — OAuth, JWT, session-based auth
    Auth(AuthDef),
    /// File upload definition — resumable chunked file uploads
    Upload(UploadDef),
    /// Local database definition (IndexedDB abstraction)
    Db(DbDef),
    /// Cache definition — intelligent data caching with queries and mutations
    Cache(CacheDef),
    /// Responsive breakpoints configuration
    Breakpoints(BreakpointsDef),
    /// Theme definition — opt-in light/dark theming
    Theme(ThemeDef),
    /// Animation block — spring physics, keyframes, or stagger animations
    Animation(AnimationBlockDef),
}

/// Contract definition — an API boundary type that generates:
/// 1. Compile-time field access checking (like structs)
/// 2. A content hash baked into the WASM binary for wire-level staleness detection
/// 3. A WASM-native runtime validator that checks every API response before it enters the app
///
/// Contracts are structurally similar to structs but semantically different: they define
/// the shape of *external* data (API responses, WebSocket messages, etc.) and enforce
/// that untrusted data is validated before it can be used in typed Nectar code.
#[derive(Debug)]
pub struct ContractDef {
    pub name: String,
    pub fields: Vec<ContractField>,
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

/// A field within a contract definition.
/// Unlike struct fields, contract fields can have inline enum types.
#[derive(Debug)]
pub struct ContractField {
    pub name: String,
    pub ty: Type,
    pub nullable: bool,
    #[allow(dead_code)]
    pub span: Span,
}

/// Accessibility mode for components and apps
#[derive(Debug, Clone, PartialEq)]
pub enum A11yMode {
    Auto,    // compiler generates full a11y layer
    Hybrid,  // developer overrides specific attrs, compiler fills the rest
    Manual,  // developer handles everything
}

/// App definition — root-level PWA application with manifest, offline support, push
#[derive(Debug)]
pub struct AppDef {
    pub name: String,
    pub manifest: Option<ManifestDef>,
    pub offline: Option<OfflineDef>,
    pub push: Option<PushDef>,
    pub router: Option<RouterDef>,
    pub a11y: Option<A11yMode>,
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

#[derive(Debug)]
pub struct ManifestDef {
    pub entries: Vec<(String, Expr)>,  // name-value pairs
    #[allow(dead_code)]
    pub span: Span,
}

#[derive(Debug)]
pub struct OfflineDef {
    pub precache: Vec<String>,
    pub strategy: String,
    #[allow(dead_code)]
    pub fallback: Option<String>,  // component name
    #[allow(dead_code)]
    pub span: Span,
}

#[derive(Debug)]
pub struct PushDef {
    pub vapid_key: Option<Expr>,
    #[allow(dead_code)]
    pub on_message: Option<String>,  // handler function name
    #[allow(dead_code)]
    pub span: Span,
}

/// Page definition — a component with SEO metadata (title, description, structured data, etc.)
#[derive(Debug)]
pub struct PageDef {
    pub name: String,
    pub props: Vec<Param>,
    pub meta: Option<MetaDef>,
    pub state: Vec<StateField>,
    pub methods: Vec<Function>,
    pub styles: Vec<StyleBlock>,
    pub render: RenderBlock,
    pub permissions: Option<PermissionsDef>,
    pub gestures: Vec<GestureDef>,
    pub is_pub: bool,
    pub span: Span,
}

/// SEO metadata block within a page definition
#[derive(Debug)]
pub struct MetaDef {
    pub title: Option<Expr>,
    pub description: Option<Expr>,
    pub canonical: Option<Expr>,
    pub og_image: Option<Expr>,
    pub structured_data: Vec<StructuredDataDef>,
    pub extra: Vec<(String, Expr)>,  // arbitrary meta key-value pairs
    #[allow(dead_code)]
    pub span: Span,
}

/// Structured data definition (JSON-LD) within a meta block
#[derive(Debug)]
pub struct StructuredDataDef {
    pub schema_type: String,   // e.g. "Article", "Organization", "Product"
    pub fields: Vec<(String, Expr)>,
    #[allow(dead_code)]
    pub span: Span,
}

/// Declarative form definition
#[derive(Debug, Clone)]
pub struct FormDef {
    pub name: String,
    pub fields: Vec<FormFieldDef>,
    pub on_submit: Option<String>,  // handler function name
    #[allow(dead_code)]
    pub steps: Vec<FormStep>,       // for multi-step forms, empty if single-step
    pub methods: Vec<Function>,
    #[allow(dead_code)]
    pub styles: Vec<StyleBlock>,
    #[allow(dead_code)]
    pub render: Option<RenderBlock>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FormFieldDef {
    pub name: String,
    pub ty: Type,
    pub validators: Vec<ValidatorDef>,
    pub label: Option<Expr>,
    pub placeholder: Option<Expr>,
    pub default_value: Option<Expr>,
    #[allow(dead_code)]
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ValidatorDef {
    pub kind: ValidatorKind,
    pub message: Option<Expr>,
    #[allow(dead_code)]
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ValidatorKind {
    Required,
    MinLength(usize),
    MaxLength(usize),
    Pattern(String),
    Email,
    Url,
    Min(i64),
    Max(i64),
    Custom(String),  // function name
}

#[derive(Debug, Clone)]
pub struct FormStep {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub fields: Vec<String>,  // field names in this step
    #[allow(dead_code)]
    pub span: Span,
}

/// Channel definition — real-time WebSocket connection with event handlers
///
/// ```nectar
/// channel Chat -> ChatMessage {
///     url: "/ws/chat",
///     reconnect: true,
///     heartbeat: 30000,
///
///     on_message fn(msg) { ... }
///     on_connect fn() { ... }
///     on_disconnect fn() { ... }
///
///     fn send_message(text: String) { ... }
/// }
/// ```
#[derive(Debug)]
pub struct ChannelDef {
    pub name: String,
    pub url: Expr,
    pub contract: Option<String>,
    pub on_message: Option<Function>,
    pub on_connect: Option<Function>,
    pub on_disconnect: Option<Function>,
    pub reconnect: bool,
    pub heartbeat_interval: Option<u64>,
    pub methods: Vec<Function>,
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

/// Third-party embed definition — sandboxed external script/widget integration
///
/// ```nectar
/// embed GoogleAnalytics {
///     src: "https://www.googletagmanager.com/gtag/js?id=G-XXXXX",
///     loading: "lazy",
///     sandbox: true,
///     integrity: "sha384-...",
/// }
/// ```
#[derive(Debug, Clone)]
pub struct EmbedDef {
    pub name: String,
    pub src: Expr,
    pub loading: Option<String>,     // "defer", "async", "lazy", "idle"
    pub sandbox: bool,
    pub integrity: Option<Expr>,     // SRI hash
    #[allow(dead_code)]
    pub permissions: Option<PermissionsDef>,
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

/// PDF document definition — generates downloadable PDF documents
///
/// ```nectar
/// pdf InvoicePdf {
///     page_size: "A4",
///     orientation: "portrait",
///     render {
///         <div>...</div>
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct PdfDef {
    pub name: String,
    #[allow(dead_code)]
    pub render: RenderBlock,
    pub page_size: Option<String>,   // "A4", "letter", etc.
    pub orientation: Option<String>, // "portrait", "landscape"
    #[allow(dead_code)]
    pub margins: Option<Expr>,
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

/// Payment gateway definition — PCI-compliant payment processing
#[derive(Debug, Clone)]
pub struct PaymentDef {
    pub name: String,
    pub provider: Option<Expr>,        // "stripe", "paypal", etc.
    pub public_key: Option<Expr>,
    pub sandbox_mode: bool,            // PCI-compliant isolation
    pub on_success: Option<Function>,
    pub on_error: Option<Function>,
    pub methods: Vec<Function>,
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

/// Authentication definition — OAuth, JWT, session-based auth
#[derive(Debug, Clone)]
pub struct AuthDef {
    pub name: String,
    pub provider: Option<Expr>,         // "oauth", "jwt", "session"
    pub providers: Vec<AuthProvider>,
    pub on_login: Option<Function>,
    pub on_logout: Option<Function>,
    pub on_error: Option<Function>,
    pub session_storage: Option<String>, // "cookie", "local", "session"
    pub methods: Vec<Function>,
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

/// An individual auth provider configuration (e.g. Google, GitHub, email)
#[derive(Debug, Clone)]
pub struct AuthProvider {
    pub name: String,      // "google", "github", "email"
    #[allow(dead_code)]
    pub client_id: Option<Expr>,
    pub scopes: Vec<String>,
    #[allow(dead_code)]
    pub span: Span,
}

/// File upload definition — resumable chunked file uploads
#[derive(Debug, Clone)]
pub struct UploadDef {
    pub name: String,
    pub endpoint: Expr,
    pub max_size: Option<Expr>,        // bytes
    pub accept: Vec<String>,           // MIME types: ["image/*", "application/pdf"]
    pub chunked: bool,                 // resumable chunked upload
    pub on_progress: Option<Function>,
    pub on_complete: Option<Function>,
    pub on_error: Option<Function>,
    pub methods: Vec<Function>,
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

/// Local database definition — IndexedDB abstraction
#[derive(Debug, Clone)]
pub struct DbDef {
    pub name: String,
    pub version: Option<u32>,
    #[allow(dead_code)]
    pub stores: Vec<DbStoreDef>,
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

/// A single object store within a database definition
#[derive(Debug, Clone)]
pub struct DbStoreDef {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub key: String,
    #[allow(dead_code)]
    pub indexes: Vec<(String, String)>,  // (name, key_path)
    #[allow(dead_code)]
    pub span: Span,
}

/// Cache definition — intelligent data caching with queries and mutations
#[derive(Debug, Clone)]
pub struct CacheDef {
    pub name: String,
    pub strategy: Option<String>,        // "stale-while-revalidate", "cache-first", "network-first"
    pub default_ttl: Option<u64>,        // seconds
    pub persist: bool,                    // use IndexedDB for persistence
    pub max_entries: Option<u64>,
    pub queries: Vec<CacheQueryDef>,
    pub mutations: Vec<CacheMutationDef>,
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CacheQueryDef {
    pub name: String,
    #[allow(dead_code)]
    pub params: Vec<Param>,
    pub fetch_expr: Expr,                // the fetch(...) expression
    pub contract: Option<String>,        // -> ContractName
    pub ttl: Option<u64>,
    pub stale: Option<u64>,              // stale-while-revalidate window
    pub invalidate_on: Vec<String>,      // event names
    #[allow(dead_code)]
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CacheMutationDef {
    pub name: String,
    #[allow(dead_code)]
    pub params: Vec<Param>,
    #[allow(dead_code)]
    pub fetch_expr: Expr,
    pub optimistic: bool,
    pub rollback_on_error: bool,
    pub invalidate: Vec<String>,         // query names to invalidate
    #[allow(dead_code)]
    pub span: Span,
}

/// Responsive breakpoints definition
#[derive(Debug, Clone)]
pub struct BreakpointsDef {
    pub breakpoints: Vec<(String, u32)>,  // name -> pixels
    #[allow(dead_code)]
    pub span: Span,
}

/// Theme definition — opt-in light/dark theming with auto-generation
#[derive(Debug, Clone)]
pub struct ThemeDef {
    pub name: String,
    pub light: Option<Vec<(String, Expr)>>,
    pub dark: Option<Vec<(String, Expr)>>,
    pub dark_auto: bool,              // dark: auto
    pub primary: Option<Expr>,        // for full auto mode
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

/// Animation block definition — spring physics, keyframes, or stagger
#[derive(Debug, Clone)]
pub struct AnimationBlockDef {
    pub name: String,
    pub kind: AnimationKind,
    pub is_pub: bool,
    #[allow(dead_code)]
    pub span: Span,
}

/// The kind of animation in an animation block
#[derive(Debug, Clone)]
pub enum AnimationKind {
    Spring {
        stiffness: Option<f64>,
        damping: Option<f64>,
        mass: Option<f64>,
        properties: Vec<String>,
    },
    Keyframes {
        frames: Vec<(f64, Vec<(String, Expr)>)>,
        duration: Option<String>,
        easing: Option<String>,
    },
    Stagger {
        animation: String,
        delay: Option<String>,
        selector: Option<String>,
    },
}

/// Keyboard shortcut definition inside a component
#[derive(Debug, Clone)]
pub struct ShortcutDef {
    pub keys: String,
    #[allow(dead_code)]
    pub body: Block,
    #[allow(dead_code)]
    pub span: Span,
}

/// Gesture definition inside a component
#[derive(Debug, Clone)]
pub struct GestureDef {
    pub gesture_type: String,   // swipe_left, swipe_right, long_press, pinch, etc.
    pub target: Option<String>, // on:element_name, or None for the component root
    #[allow(dead_code)]
    pub body: Block,
    #[allow(dead_code)]
    pub span: Span,
}

/// Test definition — a named block of test code
#[derive(Debug)]
pub struct TestDef {
    pub name: String,
    pub body: Block,
    pub span: Span,
}

/// Trait definition — Rust-style interface with optional default method bodies
#[derive(Debug, Clone)]
pub struct TraitDef {
    pub name: String,
    pub type_params: Vec<String>,
    pub methods: Vec<TraitMethod>,
    pub span: Span,
}

/// A method declaration inside a trait (may have a default body)
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub lifetimes: Vec<String>,
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub trait_bounds: Vec<TraitBound>,
    pub body: Block,
    pub is_pub: bool,
    pub must_use: bool,
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
    pub permissions: Option<PermissionsDef>,
    pub gestures: Vec<GestureDef>,
    pub skeleton: Option<SkeletonDef>,
    pub error_boundary: Option<ErrorBoundary>,
    /// Code-split chunk name — `chunk "dashboard"` tags this component
    /// so the bundler emits it into a separate chunk.
    pub chunk: Option<String>,
    /// Lifecycle cleanup callback — `fn on_destroy` method reference
    pub on_destroy: Option<Function>,
    /// Accessibility mode — `a11y auto` or `a11y manual`
    pub a11y: Option<A11yMode>,
    /// Keyboard shortcuts — `shortcut "ctrl+s" { ... }`
    pub shortcuts: Vec<ShortcutDef>,
    pub span: Span,
}

/// Permission declaration inside a component
#[derive(Debug, Clone)]
pub struct PermissionsDef {
    pub network: Vec<String>,      // allowed URL patterns
    pub storage: Vec<String>,      // allowed storage keys
    pub capabilities: Vec<String>, // camera, geolocation, etc.
    #[allow(dead_code)]
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
#[derive(Debug, Clone)]
pub struct SkeletonDef {
    pub body: RenderBlock,
    #[allow(dead_code)]
    pub span: Span,
}

/// Error boundary — catches render errors and shows a fallback UI
#[derive(Debug, Clone)]
pub struct ErrorBoundary {
    pub fallback: RenderBlock,
    pub body: RenderBlock,
    #[allow(dead_code)]
    pub span: Span,
}

/// Component property (immutable by default)
#[derive(Debug, Clone)]
pub struct Prop {
    pub name: String,
    pub ty: Type,
    pub default: Option<Expr>,
}

/// Component state field (reactive signal)
#[derive(Debug, Clone)]
pub struct StateField {
    pub name: String,
    pub ty: Option<Type>,
    pub mutable: bool,
    pub secret: bool,
    /// Whether this signal uses atomic operations for race-free concurrent access
    pub atomic: bool,
    pub initializer: Expr,
    #[allow(dead_code)]
    pub ownership: Ownership,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ownership {
    Owned,
    Borrowed,
    MutBorrowed,
}

/// Render block — produces a template tree
#[derive(Debug, Clone)]
pub struct RenderBlock {
    pub body: TemplateNode,
    #[allow(dead_code)]
    pub span: Span,
}

/// Template nodes — the JSX-like part
#[derive(Debug, Clone)]
pub enum TemplateNode {
    Element(Element),
    TextLiteral(String),
    Expression(Box<Expr>),
    Fragment(Vec<TemplateNode>),
    /// <Link to="/about" class="btn">About</Link>
    Link { to: Expr, attributes: Vec<Attribute>, children: Vec<TemplateNode> },
    /// <Outlet /> — marks where routed content renders inside a layout
    Outlet,
    /// Layout primitives — compile-time sugar for semantic HTML + CSS
    Layout(LayoutNode),
    /// {if condition { <template> } else { <template> }}
    TemplateIf {
        condition: Box<Expr>,
        then_children: Vec<TemplateNode>,
        else_children: Option<Vec<TemplateNode>>,
    },
    /// {for binding in iterator { <template> }}
    /// When `lazy` is true, items are rendered in batches as the user scrolls
    /// using IntersectionObserver (progressive/lazy rendering).
    TemplateFor {
        binding: String,
        iterator: Box<Expr>,
        children: Vec<TemplateNode>,
        lazy: bool,
    },
    /// {match subject { Pattern => <template>, ... }}
    TemplateMatch {
        subject: Box<Expr>,
        arms: Vec<TemplateMatchArm>,
    },
}

/// A single arm in a `{match}` template expression.
#[derive(Debug, Clone)]
pub struct TemplateMatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Vec<TemplateNode>,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub tag: String,
    pub attributes: Vec<Attribute>,
    pub children: Vec<TemplateNode>,
    pub span: Span,
}

/// Layout primitives — compile to semantic HTML + CSS at codegen time.
/// Zero runtime cost — pure syntactic sugar.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum LayoutNode {
    /// `<Stack gap="16">` → column flexbox
    Stack { gap: Option<String>, children: Vec<TemplateNode>, span: Span },
    /// `<Row gap="8" align="center">` → row flexbox
    Row { gap: Option<String>, align: Option<String>, children: Vec<TemplateNode>, span: Span },
    /// `<Grid cols="3" gap="16">` → CSS Grid
    Grid { cols: Option<String>, rows: Option<String>, gap: Option<String>, children: Vec<TemplateNode>, span: Span },
    /// `<Center max_width="800">` → flex centering
    Center { max_width: Option<String>, children: Vec<TemplateNode>, span: Span },
    /// `<Cluster gap="8">` → flex-wrap row
    Cluster { gap: Option<String>, children: Vec<TemplateNode>, span: Span },
    /// `<Sidebar side="left" width="300">` → CSS Grid sidebar
    Sidebar { side: Option<String>, width: Option<String>, children: Vec<TemplateNode>, span: Span },
    /// `<Switcher threshold="600">` → flexbox that wraps based on container
    Switcher { threshold: Option<String>, children: Vec<TemplateNode>, span: Span },
}

#[derive(Debug, Clone)]
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
    pub selectors: Vec<SelectorDef>,
    pub is_pub: bool,
    pub span: Span,
}

/// Selector — a derived value that depends on one or more signals/stores
#[derive(Debug)]
pub struct SelectorDef {
    pub name: String,
    #[allow(dead_code)]
    pub deps: Vec<String>,     // store/signal names this derives from
    pub body: Expr,            // computation expression
    #[allow(dead_code)]
    pub span: Span,
}

/// Store action — a method that can mutate store state
#[derive(Debug)]
pub struct ActionDef {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
    pub is_async: bool,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    /// Persistent layout shell — content renders into `<Outlet />`
    #[allow(dead_code)]
    pub layout: Option<RenderBlock>,
    /// Default transition for all routes (can be overridden per-route)
    #[allow(dead_code)]
    pub transition: Option<String>,
    #[allow(dead_code)]
    pub span: Span,
}

/// A single route definition — path pattern mapped to a component
#[derive(Debug)]
pub struct RouteDef {
    pub path: String,
    pub params: Vec<String>,      // extracted from :param segments in path
    pub component: String,        // component to render
    pub guard: Option<Expr>,      // optional auth/permission guard
    /// Per-route transition override: "fade", "slide-left", "slide-right", "none"
    #[allow(dead_code)]
    pub transition: Option<String>,
    #[allow(dead_code)]
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
#[derive(Debug, Clone)]
pub struct StyleBlock {
    pub selector: String,
    pub properties: Vec<(String, String)>,
    #[allow(dead_code)]
    pub span: Span,
}

/// Transition definition — CSS transition on a property
#[derive(Debug, Clone)]
pub struct TransitionDef {
    pub property: String,
    pub duration: String,
    pub easing: String,
    #[allow(dead_code)]
    pub span: Span,
}

/// Animation definition — keyframe animation
#[derive(Debug)]
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub ownership: Ownership,
    /// Whether the parameter is marked `secret` — compile-time only, prevents logging/rendering.
    pub secret: bool,
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
        secret: bool,
        value: Expr,
        ownership: Ownership,
    },
    Signal {
        name: String,
        ty: Option<Type>,
        secret: bool,
        atomic: bool,
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

    // Array literal: [1, 2, 3] or []
    ArrayLit(Vec<Expr>),

    // Object literal: { key: value, ... }
    ObjectLit {
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
    // fetch(url) -> ContractName  — validates response against contract
    Fetch {
        url: Box<Expr>,
        options: Option<Box<Expr>>,
        /// Optional contract type for response validation.
        /// When present, the response is validated against the contract's schema
        /// before entering the app, and the request includes an
        /// `X-Nectar-Contract: Name@hash` header for staleness detection.
        contract: Option<String>,
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
    /// spawn { ... } — runs block in Web Worker
    Spawn { body: Block, span: Span },
    Channel { ty: Option<Type> },
    Send { channel: Box<Expr>, value: Box<Expr> },
    Receive { channel: Box<Expr> },
    /// parallel { a, b, c } — runs multiple expressions concurrently
    Parallel { tasks: Vec<Expr>, span: Span },

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

    /// Dynamic import: `import("./module")` — triggers code split
    DynamicImport {
        path: Box<Expr>,
        span: Span,
    },

    /// Download trigger — `download(data, "filename.ext")`
    Download {
        data: Box<Expr>,
        filename: Box<Expr>,
        span: Span,
    },

    /// env("VAR_NAME") — compile-time validated environment variable access
    Env {
        name: Box<Expr>,
        span: Span,
    },

    /// trace("label") { ... } — performance/error tracing block
    Trace {
        label: Box<Expr>,
        body: Block,
        span: Span,
    },

    /// flag("feature_name") — compile-time feature flag check
    Flag {
        name: Box<Expr>,
        span: Span,
    },

    /// virtual list — efficient rendering for large datasets
    VirtualList {
        items: Box<Expr>,
        item_height: Box<Expr>,
        template: Box<Expr>,
        buffer: Option<u32>,
        span: Span,
    },

    /// Range expression — `start..end`
    /// Produces an integer range [start, end) for use in for loops
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
    },
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
    pub guard: Option<Expr>,
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
