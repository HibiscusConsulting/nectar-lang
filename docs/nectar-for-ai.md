# Nectar -- Complete AI Reference

This is the definitive reference for AI assistants generating Nectar code. Every feature documented here has been verified to compile and produce working WASM output. Features that are parsed but not yet fully functional in codegen are clearly marked as NOT YET WORKING.

## Core Thesis

Nectar compiles to WebAssembly. All logic, state, rendering, and computation run in WASM. The only JavaScript is `core.js` (~3 KB gzip) -- a thin syscall layer for browser APIs that WASM physically cannot call (DOM, fetch, WebSocket, IndexedDB, etc.). There is no `node_modules`, no `package.json`, no bundler.

The compiler is a single Rust binary called `nectar`.

---

## Build and Run

```bash
nectar build app.nectar --emit-wasm   # Compile .nectar to .wasm + core.js
nectar dev --src . --port 3000        # Dev server with hot reload
nectar fmt app.nectar                 # Format source
nectar lint app.nectar                # Lint source
nectar test app.nectar                # Run tests
nectar check app.nectar               # Type-check + borrow-check without emitting WASM
nectar build --ssr                    # Server-side rendering
```

---

## Types

```
i32  i64  u32  u64  f32  f64  bool  String
[T]  Option<T>  Result<T, E>
```

### Additional type notes:

- `(T, U)` -- tuple types work. Construction via `(a, b)` and access via `.0`, `.1` compile to WASM.
- `&T` / `&mut T` / `&'a T` -- borrow syntax is parsed and checked by the borrow checker but does not affect WASM output (WASM has no references). The borrow checker enforces field-level borrows, NLL, return ref verification, and reborrowing.
- `fn(T) -> U` -- function types parse but higher-order functions are limited
- Generics like `fn first<T>(items: [T]) -> T` -- generic type parameters are monomorphized. The compiler specializes each generic function for every concrete type it is called with.

---

## Variables

```nectar
let x: i32 = 42;              // immutable binding
let mut count: i32 = 0;       // mutable binding (compiles to WASM local)
```

Inside components, `let mut` fields become reactive signals:

```nectar
component Counter() {
    let mut count: i32 = 0;           // reactive signal -- DOM auto-updates
    let mut name: String = "hello";   // string signal
    let mut items: [Product] = [];    // array signal
    // ...
}
```

### Variable notes:

- `signal name: String = "";` -- the `signal` keyword is parsed but in practice `let mut` inside components achieves the same thing and is the proven pattern
- `let (a, b) = expr;` -- let-binding tuple destructuring parses but has no codegen. Use `match` for destructuring.
- `let User { name, .. } = user;` -- let-binding struct destructuring parses but has no codegen. Use `match` for destructuring.
- Tuple, struct, and array destructuring WORK in `match` arms

---

## Functions

```nectar
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

Functions compile to WASM functions. They can take typed parameters and return values.

### Inside components, methods use `&mut self`:

```nectar
fn increment(&mut self) {
    self.count = self.count + 1;
}
```

### What does NOT work for functions:

- `async fn` -- parses but async/await has no runtime support
- `f"Hello {name}"` -- format string interpolation (`f"..."`) parses into the AST but is NOT reliably generated in codegen. Use `format("{}", value)` instead.

### What DOES work for functions:

- Generic functions (`fn first<T>(items: [T]) -> T`) -- monomorphized at compile time for each concrete type
- Lifetime annotations (`fn first<'a>(x: &'a str) -> &'a str`) -- validated by the borrow checker
- `where T: Display` bounds -- static trait dispatch at compile time
- Closures with environment capture -- compiled to function table entries

---

## Ownership and Borrowing

The borrow checker runs at compile time and enforces:

- One owner per value
- Assignment moves by default
- `&val` for immutable borrows
- `&mut val` for exclusive mutable borrows

However, at the WASM level these are erased. Borrow checking prevents invalid programs at compile time, but the generated WASM uses i32 values and pointers regardless.

---

## Struct

Struct definitions compile to linear memory layouts with 4-byte field offsets.

```nectar
struct Product {
    name: String,
    category: String,
    price: String,
    rating: String,
    stock: String,
    img: String,
}
```

### Struct construction (works):

```nectar
let p = Product {
    name: "Widget",
    category: "Electronics",
    price: "$49.99",
    rating: "4.5",
    stock: "In Stock",
    img: "https://example.com/img.jpg",
};
```

### Field access (works):

```nectar
let n = p.name;      // field access from local variable
item.category         // field access from for-loop binding
product.price         // field access from handler parameter
```

### What works for structs:

- Generic structs -- monomorphized at compile time
- `impl` blocks with methods -- method calls compile to `call $Type_method`
- Trait implementations (`impl Display for User`) -- static trait dispatch, no vtable needed
- Struct destructuring in match arms

---

## Enum

```nectar
enum Status {
    Active,
    Inactive,
    Error(String),
}
```

Enums parse and are used in pattern matching. However, full enum variant construction and matching beyond `Ok`/`Err`/`Some`/`None` is limited.

---

## Component (FULLY WORKING)

The fundamental UI building block. This is the most complete and battle-tested feature.

```nectar
component CounterPanel() {
    let mut count: i32 = 0;
    let mut updates: i32 = 0;

    fn increment(&mut self) {
        self.count = self.count + 1;
        self.updates = self.updates + 1;
    }

    fn decrement(&mut self) {
        self.count = self.count - 1;
        self.updates = self.updates + 1;
    }

    fn reset(&mut self) {
        self.count = 0;
        self.updates = self.updates + 1;
    }

    render {
        <div class="counter">
            <span class="value">{format("{}", self.count)}</span>
            <button class="btn" on:click={self.increment}>"+"</button>
            <button class="btn" on:click={self.decrement}>"-"</button>
            <button class="btn" on:click={self.reset}>"Reset"</button>
            <div class="info">
                <span>"Updates: "</span>
                <span>{format("{}", self.updates)}</span>
            </div>
        </div>
    }
}
```

### What compiles inside a component:

| Feature | Syntax | Status |
|---|---|---|
| Mutable state (i32) | `let mut count: i32 = 0;` | WORKS |
| Mutable state (String) | `let mut name: String = "hello";` | WORKS |
| Mutable state (array) | `let mut items: [Product] = [];` | WORKS |
| Methods | `fn name(&mut self) { ... }` | WORKS |
| Init lifecycle | `fn init(&mut self) { ... }` | WORKS -- called after signals created, before render |
| Style blocks | `style { .class { prop: "val"; } }` | WORKS |
| Render block | `render { <div>...</div> }` | WORKS |
| Props | `component Foo(label: String)` | WORKS -- passed as ptr+len pairs |
| Child components | `<ChildComp />` | WORKS |

### How components compile:

Each component produces:
1. **Signal globals** -- one `(global $__sig_CompName_field (mut i32))` per `let mut` field
2. **A mount function** -- `CompName_mount(root)` that creates DOM, initializes signals, and sets up reactive bindings
3. **Handler trampolines** -- one function per method, dispatched via `call_indirect` from `__callback`
4. **Signal updater functions** -- auto-generated functions that re-read signal values and call `dom_setText` to update DOM nodes

### Reactive signals and DOM updates:

When you write `{format("{}", self.count)}` in a template, the compiler:
1. Creates a text node during mount
2. Generates a signal updater function that reads `$__sig_Counter_count`, converts it to a string, and calls `dom_setText`
3. Subscribes the updater to the signal via `signal_subscribe`
4. When `self.count` changes in a method, `signal_set` fires, which triggers the updater via `call_indirect`

This gives O(1) DOM updates per signal change. No virtual DOM. No diffing.

---

## Displaying Values in Templates

### Integers: use `format("{}", value)`

The `format` function converts an i32 to a string. This is the proven pattern from the demo:

```nectar
<span>{format("{}", self.count)}</span>
<span>{format("{}", self.product_count)}</span>
```

`format` compiles to `call $format` which calls the WASM-internal `$string_fromI32` function to produce a (ptr, len) string pair.

### Strings: use directly

```nectar
<span>{self.name}</span>
<span>{self.active_cat}</span>
```

String signals are stored as pointers. When accessed in templates, the signal value (a string ptr) is used directly.

### Static text: use string literals

```nectar
<span>"Hello world"</span>
<p>"Click the button to increment"</p>
```

String literals are stored in WASM linear memory at compile time.

### Concatenating text and expressions:

```nectar
<p>"Last added: "{self.last_added}</p>
<p>"Count: "{format("{}", self.count)}</p>
```

Adjacent text literals and expressions in a template each become their own text node, appearing side by side in the DOM.

---

## Template Syntax (inside render blocks)

### Elements with static attributes (WORKS):

```nectar
<div class="wrapper" id="main">"content"</div>
<input type="text" class="search-box" placeholder="Search..." />
<img src="static.png" alt="photo" loading="lazy" width="280" height="180" />
<button type="submit" disabled>"Submit"</button>
```

Static attributes compile to `dom_setAttr` calls during mount.

### Dynamic attributes (WORKS):

```nectar
<img src={product.img} alt={product.name} />
```

Dynamic attributes evaluate the expression at mount time and call `dom_setAttr` with the result. The expression must produce a string (ptr).

### Style attribute (WORKS):

```nectar
<div style="color:var(--text2);font-size:0.9rem;margin-bottom:8px">"text"</div>
<div style="margin-top:24px;grid-template-columns:1fr 1fr">"grid"</div>
```

The `style` attribute is a static string attribute -- it compiles to a `dom_setAttr` call.

### Boolean attributes (WORKS):

```nectar
<button disabled>"Can't click"</button>
<input type="checkbox" checked />
```

Attributes without `="value"` are emitted as boolean attributes.

### Event handlers (WORKS):

```nectar
<button on:click={self.increment}>"Click me"</button>
<button on:click={self.set_cat_all}>"All"</button>
```

The handler reference resolves to a function table index. When the event fires, JavaScript calls `__callback(index)` which dispatches to the correct WASM handler trampoline.

### Event handlers with captured data (WORKS):

Inside `{for ...}` loops, event handlers capture the current loop item:

```nectar
{for product in self.products {
    <button on:click={self.add_to_cart}>"Add"</button>
}}
```

The compiler calls `$__cb_register_with_data` to create a parameterized callback that captures the item pointer. When the handler fires, the item pointer is passed via `$__callback_data`.

### Conditional rendering with `{if ...}` (WORKS):

```nectar
{if self.show_cart == 1 {
    <div class="cart-panel">
        <h3>"Shopping Cart"</h3>
        <p>"Items in cart: "{format("{}", self.cart_count)}</p>
    </div>
}}
```

The compiler generates:
1. A container element
2. A `cond_updater` function that evaluates the condition
3. When the condition becomes true, it mounts the children into the container
4. When the condition becomes false, it clears the container (`dom_clearChildren`)
5. The updater is subscribed to the signals used in the condition

### Conditional class names (WORKS):

```nectar
<button class={if self.active_cat == "Electronics" { "pill active" } else { "pill" }}
        on:click={self.set_cat_electronics}>
    "Electronics"
</button>
```

The `{if cond { "a" } else { "b" }}` expression inside an attribute evaluates at mount time and produces one of the two string values.

### For loops (WORKS):

```nectar
{for item in self.cart_items {
    <div class="cart-item">
        <span>{item.name}</span>
        <span>{item.category}</span>
        <span>{item.price}</span>
    </div>
}}
```

For loops iterate over array signals. Each iteration creates DOM elements and binds field access to the current struct pointer. The loop variable is a pointer into the array's linear memory.

### Lazy for loops (WORKS):

```nectar
{lazy for product in self.products {
    <div class="product-card">
        <img src={product.img} alt={product.name} />
        <div>{product.name}</div>
        <button on:click={self.add_to_cart}>"Add to Cart"</button>
    </div>
}}
```

Lazy for loops render the first batch (20 items) immediately, then render subsequent batches when a sentinel element becomes visible via IntersectionObserver. This is the recommended pattern for rendering large lists.

### Match in templates:

```nectar
{match status {
    Some(val) => <span>{val}</span>,
    None => <span>"Loading..."</span>,
}}
```

Template match is parsed and basic patterns work. See "Pattern Matching" below for caveats.

### Child components (WORKS):

```nectar
<EcommercePanel />
<CounterPanel />
```

The compiler detects known component names and emits:
1. A container div via `dom_createElement`
2. `dom_appendChild` to attach it to the parent
3. `call $ComponentName_mount` with the container as `$root`

### Fragment (WORKS):

```nectar
<Fragment>
    <h1>"Title"</h1>
    <p>"Body"</p>
</Fragment>
```

Fragments render their children directly into the parent without a wrapper element.

### Select elements (WORKS):

```nectar
<select class="filter">
    <option value="all">"All"</option>
    <option value="active">"Active"</option>
</select>
```

`<select>` is a regular HTML element and compiles like any other element.

---

## Store (FULLY WORKING)

Global reactive state container. Components can read signals and dispatch actions.

```nectar
store CartStore {
    signal count: i32 = 0;

    action add() {
        self.count = self.count + 1;
    }

    action clear() {
        self.count = 0;
    }
}
```

### What compiles for stores:

| Feature | Status |
|---|---|
| `signal name: Type = init;` | WORKS -- creates WASM global |
| `action name() { ... }` | WORKS -- compiles to exported function |
| `computed name() -> Type { ... }` | WORKS -- compiles to exported function |
| `effect name() { ... }` | WORKS -- compiles to exported function |
| `selector` | WORKS -- compiles to exported function |

### Store codegen:

A store named `CartStore` with `signal count: i32 = 0` produces:
- `(global $__sig_CartStore_count (mut i32) (i32.const -1))` -- signal ID
- `(func $CartStore_init ...)` -- creates the signal
- `(func $CartStore_get_count ...)` -- reads signal value
- `(func $CartStore_set_count ...)` -- writes signal value with reactive notification
- `(func $CartStore_add ...)` -- the action function

### Calling store methods from components:

```nectar
// In a component event handler:
fn handle_add(&mut self) {
    CartStore::add();
}
```

`CartStore::add()` compiles to `call $CartStore_add`. Signal getters compile to `call $CartStore_get_count`.

### Store limitations:

- `async action` -- parses but async runtime is not implemented

---

## Contract (FULLY WORKING)

Type-safe API boundaries with compile-time schema hashing and WASM-native JSON parsing.

```nectar
contract UserResponse {
    id: i32,
    name: String,
    email: String,
    role: String,
}
```

### What the compiler generates:

1. **Schema hash** -- SHA-256 of the canonical field representation, baked into the WASM binary
2. **`UserResponse_parse(json_ptr, json_len) -> struct_ptr`** -- WASM-native JSON parser that extracts fields by name and allocates a struct
3. **`UserResponse_serialize(struct_ptr) -> (json_ptr, json_len)`** -- WASM-native JSON serializer that builds JSON from struct fields
4. **`UserResponse_call(url_ptr, url_len, method_ptr, method_len, body_ptr, body_len, callback_idx)`** -- HTTP request using typed setters (setMethod, setBody, addHeader) with async callback
5. **`__contract_register_UserResponse()`** -- registers schema with runtime for drift detection

### Using contracts for API communication:

```nectar
// Define the contract
contract ProductList {
    products: [Product],
    total: i32,
}

// Call an API endpoint
fn load_products(&mut self) {
    ProductList::call(
        "/api/products",    // url
        "GET",              // method
        "",                 // body (empty for GET)
        42                  // callback index
    );
}

// Parse a JSON response
fn handle_response(&mut self, json_ptr: i32, json_len: i32) {
    let data = ProductList::parse(json_ptr, json_len);
    // data is now a struct pointer with fields at 4-byte offsets
}

// Serialize a struct to JSON
fn send_data(&mut self) {
    let json = ProductList::serialize(data_ptr);
    // json is (ptr, len) pair
}
```

### Contract fields support:

- `i32`, `i64`, `u32`, `u64` -- integer types (JSON type: integer)
- `f32`, `f64` -- float types (JSON type: number)
- `bool` -- boolean (JSON type: boolean)
- `String` -- string (JSON type: string)
- `[T]` -- array types (JSON type: array)
- `Type?` -- nullable fields (adds `"nullable":true` to schema)

### The JSON parser is WASM-native:

There is no `JSON.parse()` in JavaScript. The compiler generates WASM functions (`$json_parse`, `$json_get_field`, `$json_serialize_object`) that parse and build JSON directly in linear memory.

---

## Router (FULLY WORKING)

Client-side routing with path matching and guard conditions.

```nectar
router AppRouter {
    route "/" => Home,
    route "/about" => About,
    route "/user/:id" => UserProfile,
    route "/admin/*" => Admin guard { AuthStore::is_logged_in() },
    fallback => NotFound,
}
```

### What the compiler generates:

1. **`AppRouter_init()`** -- registers all routes with path patterns and component mount function indices
2. **`__route_mount_N(root)`** -- one mount function per route that delegates to the component's mount function
3. **Route guards** -- if a guard expression is present, it is evaluated before mounting; if it returns 0, the mount is skipped
4. **`AppRouter_fallback_mount(root)`** -- mount function for the fallback route

### Path patterns:

- Static: `"/about"`
- Parameters: `"/user/:id"` -- `:id` is extracted as a parameter
- Wildcard: `"/admin/*"` -- matches any path under `/admin/`

### Programmatic navigation:

```nectar
fn go_home(&mut self) {
    navigate("/");
}
```

`navigate("/path")` compiles to WASM calls that update the URL via `history.pushState` and trigger route matching.

### Component composition with layout blocks (WORKS):

```nectar
router AppRouter {
    layout {
        <div>
            <NavBar />
            <Outlet />
            <Footer />
        </div>
    }

    route "/" => Home,
    route "/about" => About,
    fallback => NotFound,
}
```

`<Outlet />` marks where the routed page content renders. The surrounding layout persists across navigations. The codegen generates a container div with `id="__nectar_outlet"` for route content swapping.

---

## Database (IndexedDB) (WORKS)

Client-side persistence via IndexedDB.

```nectar
db CartDB {
    version: 1,
}
```

### What the compiler generates:

- `CartDB_init()` -- opens the IndexedDB database
- `CartDB_put(key_ptr, key_len, val_ptr, val_len)` -- store a value
- `CartDB_get(key_ptr, key_len, cb_idx)` -- retrieve a value (async via callback)
- `CartDB_delete(key_ptr, key_len)` -- delete a value
- `CartDB_getAll(cb_idx)` -- retrieve all values (async via callback)

### With named stores:

```nectar
db AppDatabase {
    version: 1,

    store "users" {
        key: "id",
        index "by_email" => "email",
    }

    store "posts" {
        key: "id",
        index "by_date" => "createdAt",
    }
}
```

---

## Arrays and Vec (WORKS)

Arrays are the primary collection type. They compile to pointers into linear memory.

### Construction:

```nectar
let mut items: [Product] = [];           // empty array
let mut numbers: [i32] = [1, 2, 3];     // array literal
```

### Push:

```nectar
self.products.push(Product {
    name: "Widget",
    category: "Electronics",
    price: "$49.99",
    rating: "4.5",
    stock: "In Stock",
    img: "https://example.com/img.jpg",
});
```

`push` appends to the array's linear memory region. For component state arrays, this updates the signal.

### Indexing:

```nectar
let item = self.products[i];    // index access
```

### Length:

```nectar
let n = items.len();
```

### Contains:

```nectar
let found = items.contains(value);
```

---

## String Operations (WORKS)

All string operations run in WASM -- no JavaScript string processing.

```nectar
let s = "  hello world  ";
let trimmed = s.trim();                    // "hello world"
let upper = s.to_upper();                  // "  HELLO WORLD  "
let lower = s.to_lower();                  // "  hello world  "
let parts = s.split(" ");                  // splits into array
let replaced = s.replace("hello", "hi");   // "  hi world  "
let idx = s.index_of("world");            // returns offset or -1
let yes = s.starts_with("  he");          // bool
let no = s.ends_with("xyz");              // bool
let sub = s.slice(2, 7);                  // "hello"
let n = "42".parse_int();                 // 42
```

---

## Iterator Methods (WORKS)

These compile to WASM loops with closure callbacks via `call_indirect`.

```nectar
let doubled = items.map(|x: i32| x * 2);
let evens = items.filter(|x: i32| x % 2 == 0);
let sum = items.fold(0, |acc: i32, x: i32| acc + x);
let total = items.reduce(|a: i32, b: i32| a + b);
let found = items.find(|x: i32| x > 10);
let has_big = items.any(|x: i32| x > 100);
let all_pos = items.all(|x: i32| x > 0);
```

---

## Control Flow

### If/else (WORKS):

```nectar
if m == 1 {
    cat = "Clothing";
}

if self.show_cart == 0 {
    self.show_cart = 1;
} else {
    self.show_cart = 0;
}
```

### While loops (WORKS):

```nectar
let mut i: i32 = 0;
while i < 400 {
    // ... body ...
    i = i + 1;
}
```

### For loops in methods (WORKS):

```nectar
for item in self.items {
    // process item
}
```

### Range expressions (WORKS):

```nectar
for i in 0..10 {
    // i goes from 0 to 9
}
```

### Additional control flow that works:

- `break` -- compiles to WASM `br` targeting the correct block
- `continue` -- compiles to WASM `br` targeting the loop header
- Compound assignment: `-=`, `*=`, `/=` work alongside `+=`
- `for chunk in stream fetch(url) { ... }` -- streaming fetch codegen exists

---

## Pattern Matching

### Result and Option matching (WORKS):

```nectar
match result {
    Ok(value) => { /* use value */ },
    Err(e) => { /* handle error */ },
}

match option {
    Some(val) => { /* use val */ },
    None => { /* handle none */ },
}
```

### Wildcard patterns (WORKS):

```nectar
match x {
    1 => { /* ... */ },
    2 => { /* ... */ },
    _ => { /* default */ },
}
```

### What works for pattern matching:

- Struct destructuring in match arms -- `User { name, age, .. } => ...`
- Tuple destructuring in match arms -- `(x, y) => ...`
- Array destructuring in match arms -- `[first, ..] => ...`
- Guards (`match x { n if n > 0 => ... }`) -- supported in codegen

### What has limited support:

- Enum variant matching beyond Ok/Err/Some/None -- basic codegen exists
- Nested pattern matching -- parses but codegen coverage is incomplete

---

## Result and Option Types (WORKS)

```nectar
// Construction
let ok_val = Ok(42);
let err_val = Err("something went wrong");
let some_val = Some(42);
let none_val = None;

// The ? operator propagates errors
let value = some_function()?;   // returns early on Err/None
```

`Ok`, `Err`, and `Some` compile to tagged allocations in linear memory: `[tag, value]` at 4-byte offsets. The `?` operator inspects the tag at offset 0 and branches accordingly.

---

## Closures (WORKS)

```nectar
let double = |x: i32| x * 2;
items.map(|x: i32| x * 2);
items.filter(|x: i32| x > 0);
```

Closures compile to WASM functions with unique names. They are added to the function table for indirect calls.

---

## Let Declarations in Methods (WORKS)

Local variables in methods are hoisted to WASM function locals:

```nectar
fn init(&mut self) {
    let mut i: i32 = 0;
    let cat: String = "Electronics";
    let img: String = "https://example.com/img.jpg";
    let m: i32 = i - (i / 5) * 5;
    // ...
}
```

The compiler scans the method body, collects all `let` bindings, and emits `(local $name type)` declarations at the function preamble before any instructions. This is required by WASM's validation rules.

---

## Arithmetic and Comparison (WORKS)

```nectar
a + b    a - b    a * b    a / b    a % b      // arithmetic
a == b   a != b   a < b   a > b   a <= b  a >= b  // comparison
a && b   a || b   !a                            // logical
x = 42                                          // assignment
```

Integer arithmetic compiles to WASM `i32.add`, `i32.sub`, etc. String equality compiles to a byte-by-byte comparison function.

---

## HTTP and Fetch

### Via contracts (RECOMMENDED -- see Contract section above):

```nectar
ContractName::call(url, method, body, callback_idx);
```

### Via typed setters (WORKS):

The HTTP namespace uses typed setters instead of serialized option objects:

```nectar
// These compile to individual WASM import calls:
http.setMethod("POST");
http.setBody(json_str);
http.addHeader("Authorization", "Bearer token");
http.fetch(url);
```

### Via the fetch expression (PARSED but limited):

```nectar
let response = fetch("/api/data");
```

The `Expr::Fetch` AST node is parsed but the codegen for standalone fetch expressions is less complete than the contract-based approach.

---

## Auth (WORKS)

Authentication with OAuth providers.

```nectar
auth AppAuth {
    provider "google" {
        client_id: env("GOOGLE_CLIENT_ID"),
        scopes: ["openid", "email", "profile"],
    }

    session: "cookie",

    fn on_login(&mut self) {
        navigate("/dashboard");
    }
}
```

The compiler generates initialization and handler functions. Auth uses HttpOnly cookies by default for session storage (server-side verification).

---

## Payment (WORKS)

PCI-compliant payment processing via sandboxed iframes.

```nectar
payment Checkout {
    provider: "stripe",
    public_key: env("STRIPE_PUBLIC_KEY"),
    sandbox: true,
}
```

Generates registration and handler functions. Card data never touches component state.

---

## Upload (WORKS)

File uploads with endpoint, size limits, and MIME type filtering.

```nectar
upload AvatarUpload {
    endpoint: "/api/upload/avatar",
    max_size: 5242880,
    accept: ["image/png", "image/jpeg"],
    chunked: true,
}
```

---

## Cache (WORKS)

Data caching with queries and mutations.

```nectar
cache ApiCache {
    strategy: "stale-while-revalidate",
    ttl: 300,
    persist: true,
    max_entries: 100,

    query get_users() : fetch("/api/users") -> UserResponse {
        ttl: 60,
        invalidate_on: ["user_created"],
    }
}
```

---

## Channel / WebSocket (WORKS)

Real-time WebSocket connections.

```nectar
channel ChatRoom {
    url: "wss://api.example.com/ws/chat",
    reconnect: true,
    heartbeat: 30000,

    on_message fn(msg) {
        // handle incoming message
    }

    on_connect fn() {
        // connected
    }
}
```

---

## Theme (WORKS)

Light/dark theme with CSS custom properties.

```nectar
theme AppTheme {
    light {
        bg: "#ffffff",
        text: "#1a1a1a",
        primary: "#4a90d9",
    }

    dark {
        bg: "#1a1a1a",
        text: "#e0e0e0",
        primary: "#6ab0ff",
    }
}
```

Generates CSS custom properties (`--bg`, `--text`, `--primary`). Usage in styles: `background: var(--bg);`

---

## App / PWA (WORKS)

Progressive Web App configuration.

```nectar
app MyApp {
    manifest {
        name: "My Application",
        short_name: "MyApp",
        start_url: "/",
        theme_color: "#4a90d9",
        display: "standalone",
    }

    offline {
        precache: ["/", "/about"],
        strategy: "cache-first",
    }
}
```

Generates manifest JSON and service worker registration functions.

---

## Page (WORKS)

SEO-optimized pages with meta tags.

```nectar
page BlogPost(slug: String) {
    signal post: Option<Post> = None;

    meta {
        title: "Blog Post",
        description: "A blog post",
    }

    render {
        <article>
            <h1>"Blog Post"</h1>
        </article>
    }
}
```

Pages compile like components but with additional SEO metadata generation.

---

## Form (WORKS)

Declarative forms with built-in validation.

```nectar
form ContactForm {
    field name: String {
        label: "Name",
        required,
        min_length: 2,
    }

    field email: String {
        label: "Email",
        required,
        email,
    }
}
```

Generates form schema JSON and registration function. Built-in validators: `required`, `min_length`, `max_length`, `pattern`, `email`, `url`, `min`, `max`, `custom`.

---

## Embed (WORKS)

Third-party script embedding with sandboxing.

```nectar
embed Analytics {
    src: "https://analytics.example.com/script.js",
    loading: "defer",
    sandbox: true,
}
```

---

## PDF (WORKS)

PDF document definition.

```nectar
pdf Invoice {
    page_size: "A4",
    orientation: "portrait",

    render {
        <div>"Invoice content"</div>
    }
}
```

---

## Animation (WORKS)

Spring physics, keyframes, and stagger animations.

```nectar
spring MenuSlide {
    stiffness: 200,
    damping: 20,
    mass: 1,
    properties: ["transform", "opacity"],
}

keyframes FadeIn {
    0% { opacity: "0" }
    100% { opacity: "1" }
    duration: "0.3s",
    easing: "ease-out",
}
```

---

## Breakpoints (WORKS)

Responsive design breakpoints.

```nectar
breakpoints {
    mobile: 320,
    tablet: 768,
    desktop: 1024,
}
```

---

## Agent (WORKS)

AI agent definition with tools and system prompt.

```nectar
agent Assistant {
    prompt system = "You are a helpful assistant.";

    tool search(query: String) -> String {
        // tool implementation
    }
}
```

---

## Modules (WORKS)

```nectar
mod utils;                           // loads utils.nectar
mod helpers {                        // inline module
    pub fn cap(s: String) -> String {
        // ...
    }
}
use models::User;                    // import
```

---

## Timer and Console (WORKS)

```nectar
// Performance timing
let t0 = timer.now();          // calls $timer_now import
// ... work ...
let elapsed = timer.now();     // measure elapsed time

// Console output
webapi.consoleLog("message");  // calls $webapi_consoleLog import
```

---

## Crypto (WASM-Internal, WORKS)

All cryptography runs in pure WASM -- no JavaScript, no Web Crypto API.

```nectar
let hash = crypto::sha256(data);
let mac = crypto::hmac(key, data);
let ct = crypto::encrypt(key, plaintext);         // AES-GCM
let pt = crypto::decrypt(key, ciphertext);
let sig = crypto::sign(private_key, data);         // Ed25519
let ok = crypto::verify(public_key, data, sig);
let dk = crypto::derive_key(password, salt);       // PBKDF2
let uuid = crypto::random_uuid();
let bytes = crypto::random_bytes(32);
```

---

## WASM Architecture

### How Nectar compiles:

```
.nectar source
     |
Lexer -> Token stream
     |
Parser -> AST
     |
Type checker + Borrow checker
     |
Optimizations (const folding, DCE, tree shaking)
     |
Codegen -> WAT (WebAssembly Text Format)
     |
wasm_binary -> .wasm binary
     |
Browser loads .wasm + core.js (~3 KB gzip)
```

### Signal System:

Every `let mut` field in a component becomes a signal:

1. **Signal creation**: `signal_create(initial_value) -> signal_id`
2. **Signal read**: `signal_get(signal_id) -> value`
3. **Signal write**: `signal_set(signal_id, new_value)` -- triggers all subscribed effects
4. **Signal subscribe**: `signal_subscribe(signal_id, updater_func_table_idx)` -- registers an effect function

### DOM Update Flow:

1. User clicks button -> JS calls `__callback(handler_idx)`
2. WASM handler runs (e.g., `self.count = self.count + 1`)
3. Handler calls `signal_set` for the updated field
4. `signal_set` triggers subscribed updater functions via `call_indirect`
5. Updater reads the new signal value, converts to string, calls `dom_setText`
6. JS `dom_setText` updates the actual DOM text node

Total JS calls per signal change: 1 (`dom_setText`). No virtual DOM. No diffing.

### Command Buffer:

For bulk DOM operations, WASM writes opcodes into a command buffer in linear memory. A single `flush()` call per animation frame executes them all. Opcodes include: SET_TEXT, SET_ATTR, SET_STYLE, CLASS_ADD, CLASS_REMOVE, APPEND_CHILD, REMOVE_CHILD.

### Call Indirect and Function Table:

Event handlers and signal updaters use WASM's `call_indirect` instruction. Each handler function is registered in a function table at a unique index. The `__callback` dispatcher uses this index to route events to the correct handler.

### Linear Memory Layout:

```
[0..255]        Reserved
[256..]         String data (interned at compile time)
[heap..]        Dynamic allocations (structs, arrays, signal values)
```

Strings are interned: `self.store_string("text")` returns an offset. At runtime, string comparisons use byte-by-byte comparison in WASM.

---

## Complete Working Example

This is the pattern proven by the demo application. An AI generating Nectar code should follow this structure:

```nectar
// 1. Define data structures
struct Product {
    name: String,
    category: String,
    price: String,
}

// 2. Define contracts for API communication
contract ProductResponse {
    id: i32,
    name: String,
    price: String,
}

// 3. Define stores for shared state
store CartStore {
    signal count: i32 = 0;

    action add() {
        self.count = self.count + 1;
    }

    action clear() {
        self.count = 0;
    }
}

// 4. Define database for persistence
db AppDB {
    version: 1,
}

// 5. Build components
component ProductList() {
    let mut products: [Product] = [];
    let mut product_count: i32 = 0;
    let mut active_filter: String = "all";

    fn init(&mut self) {
        let mut i: i32 = 0;
        while i < 100 {
            let cat: String = "Electronics";
            let m: i32 = i - (i / 3) * 3;
            if m == 1 {
                cat = "Clothing";
            }
            if m == 2 {
                cat = "Home";
            }
            self.products.push(Product {
                name: "Product",
                category: cat,
                price: "$29.99",
            });
            i = i + 1;
        }
        self.product_count = 100;
    }

    fn set_filter_all(&mut self) {
        self.active_filter = "all";
        self.product_count = 100;
    }

    fn set_filter_electronics(&mut self) {
        self.active_filter = "Electronics";
        self.product_count = 34;
    }

    fn add_to_cart(&mut self, product: Product) {
        CartStore::add();
    }

    render {
        <div class="app">
            <h1>"Product Catalog"</h1>
            <div class="controls">
                <button class={if self.active_filter == "all" { "btn active" } else { "btn" }}
                        on:click={self.set_filter_all}>
                    "All"
                </button>
                <button class={if self.active_filter == "Electronics" { "btn active" } else { "btn" }}
                        on:click={self.set_filter_electronics}>
                    "Electronics"
                </button>
            </div>
            <p>"Showing: "{format("{}", self.product_count)}" products"</p>
            <div class="product-grid">
                {lazy for product in self.products {
                    <div class="product-card">
                        <div class="name">{product.name}</div>
                        <div class="category">{product.category}</div>
                        <div class="price">{product.price}</div>
                        <button class="add-btn" on:click={self.add_to_cart}>
                            "Add to Cart"
                        </button>
                    </div>
                }}
            </div>
        </div>
    }
}

// 6. Set up routing
router AppRouter {
    route "/" => ProductList,
    fallback => ProductList,
}
```

---

## Feature Status Summary

### Features with working codegen (use these):

| Feature | Status |
|---|---|
| Generic types / monomorphization | Working -- functions specialized per concrete type |
| Trait / impl dispatch | Working -- static dispatch at compile time |
| Tuple types `(T, U)` | Working -- literal construction and `.0`/`.1` access |
| Tuple/struct/array destructuring in `match` | Working |
| `break` / `continue` in loops | Working -- compiles to WASM `br` |
| `spawn { }` / `parallel { }` | Working -- Web Worker codegen |
| `channel<T>()` / `ch.send()` / `ch.recv()` | Working -- MessageChannel codegen |
| `suspend(<Fallback />) { <Heavy /> }` | Working -- codegen for fallback rendering |
| Dynamic imports `import("./module")` | Working -- codegen emits `dom_loadChunk` |
| `prompt "..."` (AI prompt templates) | Working -- builds string and triggers fetch |
| Streaming fetch `for chunk in stream fetch(url)` | Working -- codegen emits `streaming_streamFetch` |
| Closures with environment capture | Working -- compiled to function table entries |
| Component composition (`<Outlet />`) | Working -- router layout blocks |
| Lifetime validation | Working -- NLL, field borrows, return ref verification, reborrowing |

### Features with limited or no codegen (use with caution):

| Feature | Status |
|---|---|
| `async fn` / `await` | Parsed, no async runtime |
| `f"string {interpolation}"` | Parsed into AST as FormatString, codegen exists but less proven than `format()` |
| `let (a, b) = expr;` destructuring | Parsed but no codegen -- use `match` instead |
| `try { } catch e { }` | Parsed, limited codegen |
| `yield` | Parsed, no generator runtime |
| `bind:value={signal}` | Parsed and codegen exists, but less battle-tested than on:click handlers |
| Full enum variant matching | Limited -- Ok/Err/Some/None work, custom enum variants are incomplete |
| WebRTC (`rtc::` namespace) | Parsed, runtime imports exist but end-to-end is untested |

---

## Quick Reference: Patterns That Compile

```nectar
// Integer to string for display
{format("{}", self.count)}

// String comparison
if self.active == "all" { ... }

// String signal in template
{self.name}

// Static text
"Hello world"

// Adjacent text + expression
"Count: "{format("{}", self.count)}

// Conditional class
class={if self.active == "x" { "active" } else { "inactive" }}

// Event handler
on:click={self.method_name}

// Conditional block
{if self.flag == 1 { <div>"visible"</div> }}

// For loop with struct access
{for item in self.items { <div>{item.field}</div> }}

// Lazy for loop (large lists)
{lazy for item in self.items { <div>{item.field}</div> }}

// Array push
self.items.push(StructName { field: "value" });

// While loop
while i < n { i = i + 1; }

// Modulo (no % operator in demo, use subtraction)
let m: i32 = i - (i / 5) * 5;

// Integer to string
format("{}", int_value)

// Store action call
StoreName::action_name();

// Contract parse
ContractName::parse(json_ptr, json_len);

// Navigate
navigate("/path");
```

---

## Key Differences from Rust

If you know Rust, these are the differences:

1. **No `println!` macro** -- use `webapi.consoleLog()` or render in templates
2. **No `String::from()`** -- string literals are directly usable
3. **No `Vec::new()`** -- use `[]` for empty arrays
4. **No iterators with `.collect()`** -- map/filter/fold return arrays directly
5. **No `match` on strings** -- use `if/else` chains for string comparisons
6. **No `impl` methods in practice** -- use component methods with `&mut self`
7. **No generics** -- use concrete types everywhere
8. **No `use std::...`** -- stdlib is auto-included, accessed via `crypto::`, `format()`, etc.
9. **`format("{}", val)` not `format!("{}", val)`** -- it is a function, not a macro
10. **Signal updates are automatic** -- assigning to `self.field` in a method triggers DOM updates

---

## File Structure for a Nectar App

```
my-app/
  app.nectar          # Main application file
  components.nectar   # Additional components (use mod)
  styles.css          # Optional external CSS (loaded by core.js)
```

Build: `nectar build app.nectar --emit-wasm`

Output:
```
  app.wasm            # Compiled WebAssembly binary
  core.js             # Runtime syscall layer (~3 KB gzip)
  index.html          # Generated HTML shell
```
