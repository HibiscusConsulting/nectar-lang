# Nectar — Complete AI Reference

This is the definitive guide for AI assistants building applications with the Nectar programming language. If you are an AI generating Nectar code, read this entire document.

## Core Thesis

Nectar exists to prove that the web does not need JavaScript. Every feature — from form validation to payment processing to real-time WebSocket communication — is a **first-class language construct**, not a library you import. The compiler handles what frameworks used to handle. There is no `node_modules`, no `package.json`, no bundler, no build pipeline.

**Everything compiles to WebAssembly.** The only JavaScript is a thin syscall layer (~3 KB) for browser APIs that WASM physically cannot call.

---

## Types

```
i32  i64  u32  u64  f32  f64  bool  String
[T]  (T, U)  Option<T>  Result<T, E>  &T  &mut T  &'a T  fn(T) -> U
```

## Variables

```nectar
let x: i32 = 42;               // immutable binding
let mut count = 0;              // mutable binding
signal name: String = "";       // reactive (components/stores only)
let (a, b) = get_pair();        // tuple destructure
let User { name, .. } = user;   // struct destructure
let [first, ..] = items;        // array destructure
```

## Functions

```nectar
fn add(a: i32, b: i32) -> i32 { a + b }
pub fn greet(name: String) -> String { f"Hello {name}" }
async fn load(url: String) -> Data { await fetch(url).json() }
fn first<'a, T>(items: &'a [T]) -> &'a T { items[0] }
fn print_all<T>(items: [T]) where T: Display { /* ... */ }
```

## Ownership

One owner per value. `&val` immutable borrow (many OK). `&mut val` mutable borrow (exclusive). Assignment moves. This is enforced at compile time.

```nectar
let a = "hello";
let b = a;           // a is moved to b; a can no longer be used
let c = "world";
let d = &c;          // d borrows c immutably; c is still valid
```

---

## Component

The fundamental UI building block. Components combine state, behavior, styles, and rendering.

```nectar
component Counter(initial: i32 = 0) {
    let mut count: i32 = initial;          // local state
    signal label: String = "Count";        // reactive state — DOM auto-updates

    fn increment(&mut self) { self.count = self.count + 1; }

    style { .wrap { padding: "8px"; } }
    transition { opacity: "0.3s ease"; }
    skeleton { <div class="nectar-skeleton nectar-skeleton-rect" /> }
    error_boundary { fallback { <p>"Error"</p> } { <RiskyChild /> } }

    render {
        <div class="wrap">
            <span>{self.label}: {self.count}</span>
            <button on:click={self.increment}>"+"</button>
        </div>
    }
}
```

**Blocks** (all optional except `render`): props in parens, state (`let`/`signal`), methods (`fn`), `style`, `transition`, `skeleton`, `error_boundary`, `render`.

### Generic / Lazy Components

```nectar
component List<T>(items: [T]) where T: Display {
    render { <ul>{for item in items { <li>{item.to_string()}</li> }}</ul> }
}
lazy component HeavyChart(data: [f64]) { render { <canvas /> } }
```

---

## Store

Global reactive state. Any component can read from and dispatch actions to a store.

```nectar
store AppStore {
    signal count: i32 = 0;
    signal user: Option<User> = None;

    action increment(&mut self) { self.count = self.count + 1; }
    async action fetch_user(&mut self, id: u32) {
        let r = await fetch(f"/api/users/{id}");
        self.user = Some(r.json());
    }
    computed double(&self) -> i32 { self.count * 2 }
    effect log_count(&self) { println(self.count); }
}
// Usage: AppStore::increment(), AppStore::double()
```

---

## Router

Client-side routing with guards for protected routes.

```nectar
router AppRouter {
    route "/" => Home,
    route "/user/:id" => UserProfile,
    route "/admin/*" => Admin guard { AuthStore::is_logged_in() },
    fallback => NotFound,
}
```

Path patterns: static `"/about"`, param `"/user/:id"`, wildcard `"/admin/*"`.
Programmatic: `navigate("/path");`

---

## Agent

First-class AI integration. Wraps LLM communication with typed tools and reactive UI.

```nectar
agent Assistant {
    prompt system = "You are a helpful assistant.";
    signal messages: [Message] = [];

    tool search(query: String) -> String {
        return await fetch(f"/api/search?q={query}").json().summary;
    }

    fn send(&mut self) { /* push message, call AI */ }

    render {
        <div>{for msg in self.messages { <p>{msg.content}</p> }}</div>
    }
}
```

---

## Struct / Enum / Impl / Trait

```nectar
struct User { id: u32, name: String, email: String }
pub struct Point<T> { pub x: T, pub y: T }
enum Status { Active, Inactive, Error(String) }
impl User { pub fn new(n: String) -> Self { User { id: 0, name: n, email: "" } } }
impl Display for User { fn to_string(&self) -> String { self.name } }
trait Drawable { fn draw(&self); fn bounds(&self) -> (f64, f64) { (0.0, 0.0) } }
```

---

## Contract

Type-safe API boundaries. The compiler validates that API responses match the contract at compile time, and the runtime validates at the wire level.

```nectar
contract UserResponse {
    id: i32,
    name: String,
    email: String,
    role: enum { Admin, User, Guest },
    avatar: String?,    // ? = nullable (Option<String>)
}
```

**When to use:** Any time your app talks to an external API. Contracts replace hand-written TypeScript interfaces and runtime validation libraries.

**Capabilities:**
- Compile-time field checking against usage
- Runtime response validation
- Wire-level content hashing to detect backend/frontend contract drift
- Exportable to JSON Schema, OpenAPI, or Protobuf

---

## Page

SEO-optimized pages with meta tags, structured data, and pre-rendering.

```nectar
page BlogPost(slug: String) {
    signal post: Option<Post> = None;

    meta {
        title: f"Blog - {self.post.title}",
        description: self.post.excerpt,
        og_image: self.post.cover_image,
        og_type: "article",
    }

    schema {
        type: "Article",
        headline: self.post.title,
        author: self.post.author,
    }

    async fn load(&mut self) {
        self.post = Some(await fetch(f"/api/posts/{slug}").json());
    }

    style { .post { max-width: "800px"; margin: "0 auto"; } }

    render {
        <article class="post">
            <h1>{self.post.title}</h1>
            <p>{self.post.body}</p>
        </article>
    }
}
```

**When to use:** Any route that needs SEO — blog posts, landing pages, product pages. Use `component` for interactive widgets that don't need SEO. Use `page` for anything a search engine or AI should index.

**Blocks:** `meta` (title, description, OG tags), `schema` (JSON-LD structured data), `permissions`, `gesture`, state, methods, style, render (required).

Build modes: `nectar build --ssr` for server rendering, `nectar build --ssg` for static generation.

---

## Form

Declarative forms with built-in validation. No form libraries needed.

```nectar
form ContactForm {
    field name: String {
        label: "Full Name",
        placeholder: "Jane Doe",
        required,
        min_length: 2,
        max_length: 100,
    }

    field email: String {
        label: "Email",
        placeholder: "jane@example.com",
        required,
        email,
    }

    field message: String {
        label: "Message",
        required,
        min_length: 10,
    }

    async fn on_submit(&mut self) {
        await fetch("/api/contact", {
            method: "POST",
            body: self.values(),
        });
    }

    render {
        <form on:submit={self.on_submit}>
            {self.render_fields()}
            <button type="submit" disabled={!self.is_valid()}>"Send"</button>
        </form>
    }
}
```

**When to use:** Any user input — contact forms, signup flows, settings pages, search filters.

**Built-in validators:** `required`, `min_length: N`, `max_length: N`, `pattern: "regex"`, `email`, `url`, `validate: custom_fn`.

**Automatic features:** Dirty tracking, error state per field, `is_valid()`, `values()`, `reset()`.

---

## Channel

WebSocket connections with automatic reconnection and type-safe messages.

```nectar
contract ChatMessage {
    user: String,
    text: String,
    timestamp: i64,
}

channel ChatRoom -> ChatMessage {
    url: f"wss://api.example.com/ws/chat",
    reconnect: true,
    heartbeat: 30000,

    on_connect {
        println("Connected to chat");
    }

    on_message {
        ChatStore::add_message(message);
    }

    on_disconnect {
        println("Disconnected");
    }

    fn send_message(&mut self, text: String) {
        self.send(ChatMessage { user: "me", text: text, timestamp: now() });
    }
}
```

**When to use:** Real-time features — chat, live updates, collaborative editing, notifications. Replaces Socket.io and Pusher.

**Features:** `-> ContractName` binds message types, `reconnect: true` handles drops, `heartbeat: N` sends keepalives (ms).

---

## Auth

Declarative OAuth/authentication with session management.

```nectar
auth AppAuth {
    provider "google" {
        client_id: env("GOOGLE_CLIENT_ID"),
        scopes: ["openid", "email", "profile"],
    }

    provider "github" {
        client_id: env("GITHUB_CLIENT_ID"),
        scopes: ["user:email"],
    }

    session: "cookie",

    fn on_login(&mut self) {
        navigate("/dashboard");
    }

    fn on_logout(&mut self) {
        navigate("/");
    }
}
```

**When to use:** Any app that needs user authentication. Replaces NextAuth, Auth0 SDKs, Passport.js.

**Features:** Multiple providers, session storage strategy (`"cookie"`, `"local"`), lifecycle hooks (`on_login`, `on_logout`, `on_error`).

---

## Payment

PCI-compliant payment processing via sandboxed iframes.

```nectar
payment Checkout {
    provider: "stripe",
    public_key: env("STRIPE_PUBLIC_KEY"),
    sandbox: true,

    async fn on_success(&mut self) {
        await fetch("/api/orders/confirm", { method: "POST" });
        navigate("/thank-you");
    }

    fn on_error(&mut self) {
        self.show_error("Payment failed");
    }
}
```

**When to use:** Any e-commerce or SaaS billing flow. Card data never touches your component state — the compiler guarantees payment data isolation through sandboxed iframes.

---

## Upload

File uploads with progress tracking, validation, and chunked transfer.

```nectar
upload AvatarUpload {
    endpoint: "/api/upload/avatar",
    max_size: 5242880,              // 5MB
    accept: ["image/png", "image/jpeg", "image/webp"],
    chunked: true,

    fn on_progress(&mut self) {
        self.progress_bar.set_width(f"{self.percent}%");
    }

    async fn on_complete(&mut self) {
        self.avatar_url = self.response_url;
    }

    fn on_error(&mut self) {
        self.show_error("Upload failed");
    }
}
```

**When to use:** Profile pictures, document uploads, media galleries. Replaces Dropzone, Uppy, and custom XHR upload code.

---

## Db

Client-side database abstraction over IndexedDB with declarative schema.

```nectar
db AppDatabase {
    version: 1,

    store "users" {
        key: "id",
        index "by_email" => "email",
        index "by_name" => "name",
    }

    store "posts" {
        key: "id",
        index "by_author" => "authorId",
        index "by_date" => "createdAt",
    }
}
```

**When to use:** Offline-first apps, local caching, client-side search. Replaces raw IndexedDB transactions with type-safe, declarative schemas.

**Usage from components:**
```nectar
let user = await AppDatabase::users.get(id);
await AppDatabase::users.put(user);
let all = await AppDatabase::users.get_all();
await AppDatabase::users.delete(id);
```

---

## Cache

Data caching with stale-while-revalidate, TTL, and optimistic updates.

```nectar
cache ApiCache {
    strategy: "stale-while-revalidate",
    ttl: 300,           // 5 minutes default
    persist: true,       // survive page reload via IndexedDB
    max_entries: 100,

    query get_users() : fetch("/api/users") -> UserResponse {
        ttl: 60,
        stale: 30,
        invalidate_on: ["user_created", "user_updated"],
    }

    query get_user(id: u32) : fetch(f"/api/users/{id}") -> UserResponse {
        ttl: 120,
    }

    mutation create_user(data: UserInput) : fetch("/api/users", { method: "POST", body: data }) {
        optimistic: true,
        rollback_on_error: true,
        invalidate: ["get_users"],
    }
}
```

**When to use:** Any API data fetching. Replaces React Query, SWR, Apollo Cache with zero bundle impact.

**Query features:** `ttl` (seconds), `stale` (stale-while-revalidate window), `invalidate_on` (event-based cache busting), contract binding (`-> ContractName`).

**Mutation features:** `optimistic` updates, `rollback_on_error`, `invalidate` (queries to refetch after mutation).

---

## Embed

Third-party script embedding with security controls.

```nectar
embed Analytics {
    src: "https://analytics.example.com/script.js",
    loading: "defer",
    sandbox: true,
    integrity: "sha384-abc123...",

    permissions {
        allow: ["analytics"],
        deny: ["dom_access", "network"],
    }
}
```

**When to use:** Analytics, third-party widgets, ad scripts. The compiler prevents embedded scripts from accessing your DOM or making unauthorized network requests.

**Loading strategies:** `"defer"`, `"async"`, `"lazy"`, `"idle"` (loads during idle time).

---

## Pdf

Generate PDFs from render blocks.

```nectar
pdf Invoice {
    page_size: "A4",
    orientation: "portrait",
    margins: "2cm",

    render {
        <div class="invoice">
            <h1>"Invoice #1234"</h1>
            <table>
                <tr><td>"Item"</td><td>"$100"</td></tr>
            </table>
        </div>
    }
}
```

**When to use:** Invoices, reports, receipts, certificates. No jsPDF, Puppeteer, or headless browser needed.

**Trigger download:** `Invoice::download("invoice.pdf");`

---

## App (PWA)

Progressive Web App configuration — manifest, offline support, push notifications.

```nectar
app MyApp {
    manifest {
        name: "My Application",
        short_name: "MyApp",
        start_url: "/",
        theme_color: "#4a90d9",
        background_color: "#ffffff",
        display: "standalone",
    }

    offline {
        precache: ["/", "/about", "/offline"],
        strategy: "cache-first",
        fallback: OfflinePage,
    }

    push {
        vapid_key: env("VAPID_PUBLIC_KEY"),
        on_message: handle_push,
    }
}
```

**When to use:** Any app that should work offline, be installable, or receive push notifications.

---

## Theme

Design tokens for light/dark modes with zero flash of wrong theme.

```nectar
theme AppTheme {
    light {
        bg: "#ffffff",
        text: "#1a1a1a",
        primary: "#4a90d9",
        surface: "#f5f5f5",
    }

    dark {
        bg: "#1a1a1a",
        text: "#e0e0e0",
        primary: "#6ab0ff",
        surface: "#2d2d2d",
    }
}
```

The compiler generates CSS custom properties (`--bg`, `--text`, `--primary`, `--surface`) and a toggle mechanism. Respects `prefers-color-scheme` by default.

**Usage in components:** `style { .card { background: var(--surface); color: var(--text); } }`

**Toggle:** `AppTheme::toggle();` or `AppTheme::set("dark");`

---

## Breakpoints

Responsive design breakpoints.

```nectar
breakpoints {
    mobile: 320,
    tablet: 768,
    desktop: 1024,
    wide: 1440,
}
```

**Usage in styles:**
```nectar
style {
    .grid { columns: 1; }
    @tablet { .grid { columns: 2; } }
    @desktop { .grid { columns: 3; } }
}
```

---

## Animations

Three animation primitives — spring physics, keyframes, and stagger.

### Spring

```nectar
spring MenuSlide {
    stiffness: 200,
    damping: 20,
    mass: 1,
    properties: ["transform", "opacity"],
}
```

### Keyframes

```nectar
keyframes FadeIn {
    0% { opacity: "0", transform: "translateY(10px)" }
    100% { opacity: "1", transform: "translateY(0)" }
    duration: "0.3s",
    easing: "ease-out",
}
```

### Stagger

```nectar
stagger ListReveal {
    animation: FadeIn,
    delay: "50ms",
    selector: ".list-item",
}
```

**When to use:** Any motion. Replaces Framer Motion, GSAP, CSS-in-JS animation libraries. Automatically respects `prefers-reduced-motion`.

---

## Template Syntax (inside render blocks)

```nectar
<div class="static" id="x">"text content"</div>    // element + text
<img src={dynamic_url} />                           // dynamic attr
<button on:click={self.handle}>"Click"</button>     // event handler
<input bind:value={query} />                        // two-way bind
<button aria-label="Close" role="button" />         // accessibility
{if loading { <Spinner /> } else { <Content /> }}   // conditional
{for item in items { <li>{item.name}</li> }}        // loop
{match s { Some(e) => <Err m={e} />, _ => <Ok /> }} // match
<UserCard user={u} />                               // child component
<Link to="/about">"About"</Link>                    // client-side nav
<Fragment><h1>"A"</h1><p>"B"</p></Fragment>         // fragment
```

---

## Expressions Cheat Sheet

```nectar
a + b  a - b  a * b  a / b  a % b       // arithmetic
a == b  a != b  a < b  a > b             // comparison
a && b  a || b  !a                       // logical
x = 42                                   // assignment
user.name                                // field access
items.push(1)                            // method call
add(1, 2)                                // function call
items[0]                                 // index
&val  &mut val                           // borrow
f"hello {name}"                          // format string
await fetch(url)                         // async
|x: i32| x * 2                          // closure
prompt "Summarize: {doc}"                // AI prompt template
navigate("/path")                        // routing
spawn { work() }                         // concurrency
channel<i32>()                           // channel create
ch.send(1)  ch.recv()                    // channel ops
parallel { a(), b() }                    // parallel exec
try { op()? } catch e { handle(e) }      // error handling
expr?                                    // error propagation
assert(x > 0)  assert_eq(a, b)          // testing
animate(el, "fadeIn")                    // animation
suspend(<Spinner />) { <Heavy /> }       // lazy loading
for chunk in stream fetch(url) { ... }   // streaming
```

---

## Modules and Imports

```nectar
mod utils;                              // external file
mod helpers { pub fn cap(s: String) -> String { /* ... */ } }
use std::collections;                   // single import
use http::Client as HttpClient;         // aliased import
use utils::*;                           // glob import
use models::{User, Post as BlogPost};   // group import
```

Standard library is auto-included — no imports needed for `crypto`, `format`, `collections`, `BigDecimal`, `url`, `search`, `debounce`, `throttle`, `toast`, `skeleton`, `pagination`, `mask`.

---

## Testing

```nectar
test "math works" {
    assert_eq(add(2, 3), 5);
    assert(10 > 0, "should be positive");
}

test "counter increments on click" {
    let el = render(<Counter />);
    el.findByText("+1").click();
    assert_eq(el.findByRole("counter").getText(), "1");
}

test "mock AI response" {
    ai::mock_response("The answer is 42.");
    let response = await ai::chat_complete(messages);
    assert_eq(response.content, "The answer is 42.");
}
```

Run: `nectar test file.nectar` or `nectar test file.nectar --filter "math"`

---

## Security (Built-in)

```nectar
// secret types — compiler prevents logging, rendering, serializing
let api_key: secret String = env("API_KEY");

// capability-based permissions
permissions {
    allow: ["network:api.example.com", "storage:local"],
    deny: ["network:*"],
}
```

XSS is structurally impossible — all text is WASM string data, never interpreted as HTML. No `innerHTML` from user input can exist because WASM controls all rendering.

---

## Crypto (WASM-Internal)

All cryptography runs in pure WASM — no JavaScript, no Web Crypto API. Accessed via `crypto::` namespace.

```nectar
// Hashing
let hash = crypto::sha256(data);        // SHA-256 hex string
let h512 = crypto::sha512(data);        // SHA-512
let h384 = crypto::sha384(data);        // SHA-384
let h1   = crypto::sha1(data);          // SHA-1

// HMAC
let mac    = crypto::hmac(key, data);        // HMAC-SHA256
let mac512 = crypto::hmac_sha512(key, data); // HMAC-SHA512

// Encryption (AES)
let ct = crypto::encrypt(key, plaintext);        // AES-GCM
let pt = crypto::decrypt(key, ciphertext);       // AES-GCM
let ct_cbc = crypto::encrypt_aes_cbc(key, data); // AES-CBC
let ct_ctr = crypto::encrypt_aes_ctr(key, data); // AES-CTR

// Signatures (Ed25519)
let sig = crypto::sign(private_key, data);
let ok  = crypto::verify(public_key, data, sig);

// Key derivation
let dk = crypto::derive_key(password, salt);     // PBKDF2
let bits = crypto::derive_bits(pwd, salt, 256);  // PBKDF2 bits
let hk = crypto::hkdf(ikm, salt, info, 32);     // HKDF

// Key management
let (pub_key, priv_key) = crypto::generate_key_pair("ed25519");
let exported = crypto::export_key(key, "raw");
let shared = crypto::ecdh_derive(my_priv, their_pub);

// Random
let uuid = crypto::random_uuid();        // UUID v4
let bytes = crypto::random_bytes(32);    // Random hex string
```

---

## WebRTC (Real-Time Communication)

Peer connections, data channels, and media — WASM orchestrates, JS bridges only the browser APIs.

```nectar
// Create peer connection
channel video_call {
    // Peer connection with ICE servers
    let pc = rtc::create_peer_with_ice("stun:stun.l.google.com:19302");

    // Create offer/answer for signaling
    let offer_sdp = rtc::create_offer(pc);
    rtc::set_local_description(pc, "offer", offer_sdp);

    // Data channels (text and binary)
    let chat = rtc::create_data_channel(pc, "chat");
    rtc::data_channel_send(chat, "hello");
    rtc::data_channel_send_binary(chat, binary_data);

    // Media tracks
    let stream = rtc::get_user_media({ audio: true, video: true });
    rtc::add_track(pc, track, stream);
    rtc::attach_stream(video_element, stream);

    // Event callbacks
    on rtc::ice_candidate(pc) |candidate| {
        // Send candidate to remote peer via signaling
    }

    on rtc::track(pc) |track| {
        // Handle incoming media track
    }

    on rtc::data_channel(pc) |channel| {
        // Handle incoming data channel
    }

    // State queries
    let state = rtc::get_connection_state(pc);
    let ice_state = rtc::get_ice_connection_state(pc);

    // Screen sharing
    let display = rtc::get_display_media({ video: true });

    // Cleanup
    rtc::stop_track(track);
    rtc::close(pc);
}
```

---

## Patterns — When to Use What

| Need | Use | Not |
|---|---|---|
| Interactive widget | `component` | |
| SEO-indexed route | `page` | `component` |
| Global state | `store` | Prop drilling |
| API type safety | `contract` | Untyped fetch |
| User input | `form` | Manual input handling |
| Real-time data | `channel` | Raw WebSocket |
| User login | `auth` | Manual OAuth |
| E-commerce | `payment` | Stripe.js directly |
| File uploads | `upload` | Manual XHR |
| Local storage | `db` | Raw IndexedDB |
| API caching | `cache` | Manual caching |
| PDF generation | `pdf` | jsPDF/Puppeteer |
| Third-party scripts | `embed` | `<script>` tags |
| Offline/PWA | `app` | Manual SW |
| Theming | `theme` | CSS variables manually |
| AI features | `agent` | Manual API calls |
| Animation | `spring`/`keyframes`/`stagger` | CSS animations manually |
| Responsive | `breakpoints` | Media queries manually |
| Navigation | `router` | Manual history API |
| Hashing/encryption | `crypto::` namespace | Web Crypto API |
| Video/audio calls | `channel` + `rtc::` | Raw WebRTC APIs |
| P2P data transfer | `rtc::` data channels | Manual RTCPeerConnection |

---

## Complete App Example

```nectar
// Types
contract TodoItem {
    id: i32,
    title: String,
    done: bool,
}

// Theme
theme AppTheme {
    light { bg: "#fff", text: "#111", primary: "#4a90d9" }
    dark { bg: "#111", text: "#eee", primary: "#6ab0ff" }
}

// State
store TodoStore {
    signal todos: [TodoItem] = [];
    signal filter: String = "all";

    async action load(&mut self) {
        self.todos = await fetch("/api/todos").json();
    }

    action add(&mut self, title: String) {
        self.todos.push(TodoItem { id: self.todos.len(), title: title, done: false });
    }

    action toggle(&mut self, id: i32) {
        for todo in &mut self.todos {
            if todo.id == id { todo.done = !todo.done; }
        }
    }

    computed visible(&self) -> [TodoItem] {
        match self.filter {
            "active" => self.todos.filter(|t| !t.done),
            "done" => self.todos.filter(|t| t.done),
            _ => self.todos,
        }
    }
}

// UI
component TodoApp() {
    style {
        .app { max-width: "600px"; margin: "0 auto"; padding: "20px"; }
        .done { text-decoration: "line-through"; opacity: "0.6"; }
    }

    render {
        <div class="app">
            <h1>"Todos"</h1>
            {for todo in TodoStore::visible() {
                <div class={if todo.done { "done" } else { "" }}
                     on:click={TodoStore::toggle(todo.id)}>
                    {todo.title}
                </div>
            }}
        </div>
    }
}

// Routing
router AppRouter {
    route "/" => TodoApp,
    fallback => TodoApp,
}
```

Build: `nectar build app.nectar --emit-wasm`
Dev: `nectar dev --src . --port 3000`
