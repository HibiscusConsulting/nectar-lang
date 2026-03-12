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
| Bundle size | N/A | 40-150 KB runtime | ~0 KB runtime overhead |

Nectar was designed from the ground up with these principles:

- **No GC, ever.** Ownership and borrowing at the language level means predictable, zero-pause memory management.
- **O(1) reactive updates.** Signals track dependencies at compile time. When state changes, only the exact DOM nodes that depend on it are updated -- no diffing, no reconciliation.
- **AI-native.** The `agent` keyword, `tool` definitions, and `prompt` templates are part of the grammar, not a library. Build AI-powered interfaces with the same safety guarantees as the rest of your code.
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
