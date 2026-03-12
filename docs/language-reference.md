# Nectar Language Reference

This document is the complete reference for the Nectar programming language. It covers every language construct, from lexical structure to templates, with syntax, semantics, and examples.

---

## Table of Contents

1. [Lexical Structure](#lexical-structure)
2. [Types](#types)
3. [Variables](#variables)
4. [Functions](#functions)
5. [Components](#components)
6. [Stores](#stores)
7. [Structs and Enums](#structs-and-enums)
8. [Traits](#traits)
9. [Expressions](#expressions)
10. [Statements](#statements)
11. [Patterns](#patterns)
12. [Modules](#modules)
13. [Templates](#templates)
14. [Agents](#agents)
15. [Routers](#routers)
16. [Testing](#testing)

---

## Lexical Structure

### Comments

Nectar supports single-line comments with `//`:

```nectar
// This is a comment
let x: i32 = 42; // inline comment
```

### Keywords

The following identifiers are reserved keywords in Nectar:

| Category | Keywords |
|---|---|
| **Declarations** | `fn`, `component`, `struct`, `enum`, `impl`, `trait`, `store`, `agent`, `router`, `mod`, `use`, `pub`, `test`, `lazy` |
| **Variables** | `let`, `mut`, `signal`, `own`, `ref` |
| **Control Flow** | `if`, `else`, `match`, `for`, `in`, `while`, `return`, `yield` |
| **Async/Concurrency** | `async`, `await`, `fetch`, `spawn`, `channel`, `select`, `parallel`, `stream`, `suspend` |
| **AI** | `prompt`, `tool` |
| **Routing** | `route`, `fallback`, `guard`, `navigate` |
| **Components** | `render`, `style`, `transition`, `animate`, `skeleton` |
| **Stores** | `action`, `effect`, `computed` |
| **Error Handling** | `try`, `catch` |
| **Testing** | `assert`, `assert_eq`, `expect` |
| **Values** | `true`, `false`, `self`, `Self` |
| **Types** | `i32`, `i64`, `f32`, `f64`, `u32`, `u64`, `bool`, `String` |
| **Other** | `as`, `where`, `derive`, `Link`, `Fragment` |

### Operators and Symbols

| Symbol | Meaning |
|---|---|
| `+`, `-`, `*`, `/`, `%` | Arithmetic |
| `==`, `!=`, `<`, `>`, `<=`, `>=` | Comparison |
| `&&`, `\|\|`, `!` | Logical |
| `=` | Assignment |
| `+=`, `-=`, `*=`, `/=` | Compound assignment |
| `&`, `&mut` | Borrow / mutable borrow |
| `->` | Return type arrow |
| `=>` | Fat arrow (match arms, routes) |
| `::` | Path separator |
| `.` | Field access / method call |
| `?` | Error propagation (try operator) |
| `\|` | Closure parameter delimiter |
| `,` | Separator |
| `:` | Type annotation / key-value separator |
| `;` | Statement terminator |
| `( )`, `{ }`, `[ ]`, `< >` | Grouping / blocks / arrays / generics |

### Literals

**Integers** are written as decimal numbers and are typed as `i64` by default:

```nectar
let x = 42;
let y = -7;
let big = 1000000;
```

**Floating-point numbers** use a decimal point and are typed as `f64` by default:

```nectar
let pi = 3.14159;
let neg = -2.5;
```

**Strings** are double-quoted:

```nectar
let greeting = "Hello, world!";
```

**Format strings** are prefixed with `f` and support `{expression}` interpolation:

```nectar
let name = "Nectar";
let msg = f"Hello {name}, you have {count} messages";
```

**Booleans**:

```nectar
let yes = true;
let no = false;
```

### Lifetimes

Lifetimes are annotated with a leading apostrophe and are used in reference types and generic parameters:

```nectar
fn first<'a>(items: &'a [i32]) -> &'a i32 {
    return items[0];
}
```

The special lifetime `'static` denotes a reference that lives for the entire program duration.

---

## Types

Nectar has a rich type system combining primitive types, compound types, and ownership-aware reference types.

### Primitive Types

| Type | Description | Size |
|---|---|---|
| `i32` | 32-bit signed integer | 4 bytes |
| `i64` | 64-bit signed integer | 8 bytes |
| `u32` | 32-bit unsigned integer | 4 bytes |
| `u64` | 64-bit unsigned integer | 8 bytes |
| `f32` | 32-bit floating point | 4 bytes |
| `f64` | 64-bit floating point | 8 bytes |
| `bool` | Boolean (`true`/`false`) | 1 byte |
| `String` | UTF-8 string | variable |

### Arrays

Arrays use bracket syntax and hold elements of a single type:

```nectar
let numbers: [i32] = [1, 2, 3, 4, 5];
let names: [String] = ["Alice", "Bob"];
let empty: [f64] = [];
```

### Tuples

Tuples combine a fixed number of values of potentially different types:

```nectar
let pair: (i32, String) = (42, "hello");
let triple: (bool, f64, String) = (true, 3.14, "pi");
```

### Option

`Option<T>` represents a value that may or may not be present:

```nectar
let found: Option<User> = None;
let found: Option<User> = Some(user);
```

### Result

`Result<T, E>` represents an operation that may succeed with `T` or fail with `E`:

```nectar
fn parse(input: String) -> Result<i32, String> {
    // ...
}
```

### Reference Types

References provide borrowed access to values without taking ownership:

```nectar
let r: &i32 = &x;           // immutable borrow
let mr: &mut i32 = &mut x;  // mutable borrow
let lr: &'a i32 = &x;       // lifetime-annotated borrow
let lmr: &'a mut i32 = &mut x; // lifetime-annotated mutable borrow
```

### Generic Types

Generic types are parameterized with angle brackets:

```nectar
let items: Vec<i32> = vec_new();
let map: HashMap<String, User> = hash_map_new();
```

### Function Types

Function types describe callable signatures:

```nectar
let callback: fn(i32) -> bool = |x| x > 0;
```

### Self and Self Type

Within `impl` blocks and component methods, `self` refers to the current instance and `Self` refers to the enclosing type.

---

## Variables

### Let Bindings

Variables are introduced with `let`. They are immutable by default:

```nectar
let name = "Nectar";
let count: i32 = 0;
```

### Mutable Variables

Add `mut` to make a variable mutable:

```nectar
let mut counter: i32 = 0;
counter = counter + 1;
```

### Signal Variables

Signals are reactive variables that automatically trigger re-renders when their value changes. They are used inside components and stores:

```nectar
signal count: i32 = 0;
signal name: String = "default";
```

### Type Annotations

Type annotations follow the variable name after a colon. They are optional when the type can be inferred:

```nectar
let x: i32 = 42;      // explicit type
let y = 42;            // inferred as i64
let z: f64 = 3.14;    // explicit type
```

### Ownership

Nectar uses an ownership system inspired by Rust. Every value has a single owner, and ownership can be transferred (moved) or borrowed:

```nectar
let a = "hello";
let b = a;           // a is moved to b; a can no longer be used

let c = "world";
let d = &c;          // d borrows c immutably; c is still valid
let e = &mut c;      // e borrows c mutably; no other borrows allowed
```

The `own` keyword can explicitly mark owned transfer:

```nectar
let data = own create_data();
```

### Destructuring

Variables can be destructured from tuples, arrays, and structs:

```nectar
// Tuple destructuring
let (x, y) = get_point();

// Array destructuring
let [first, second, ..] = items;

// Struct destructuring
let User { name, age, .. } = user;
```

---

## Functions

### Basic Functions

Functions are declared with the `fn` keyword:

```nectar
fn greet(name: String) -> String {
    return f"Hello, {name}!";
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

### Visibility

Functions can be made public with `pub`:

```nectar
pub fn api_handler(request: Request) -> Response {
    // accessible from other modules
}
```

### Async Functions

Prefix `fn` with `async` for asynchronous functions:

```nectar
async fn fetch_data(url: String) -> String {
    let response = await fetch(url);
    return response.json();
}
```

### Generic Functions

Functions can have type parameters:

```nectar
fn identity<T>(value: T) -> T {
    return value;
}

fn first<'a, T>(items: &'a [T]) -> &'a T {
    return items[0];
}
```

### Where Clauses (Trait Bounds)

Constrain type parameters with `where`:

```nectar
fn print_all<T>(items: [T]) where T: Display {
    for item in items {
        println(item.to_string());
    }
}
```

### Self Parameters

Methods take `self` as their first parameter, with optional borrowing:

```nectar
fn method(self)              // takes ownership
fn method(&self)             // immutable borrow
fn method(&mut self)         // mutable borrow
```

### Return Type

The return type follows `->`. Functions without an explicit return type return the unit type `()`. The last expression in a function body is implicitly returned:

```nectar
fn double(x: i32) -> i32 {
    x * 2   // implicit return
}
```

---

## Components

Components are first-class UI primitives in Nectar. They combine state, behavior, and rendering into a single declaration.

### Basic Component

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

### Props

Props are declared as parameters in parentheses after the component name. They are immutable by default and can have default values:

```nectar
component Button(label: String, disabled: bool = false) {
    render {
        <button disabled={disabled}>{label}</button>
    }
}
```

### State (let)

Local state is declared with `let` or `let mut` inside the component body:

```nectar
component Counter(initial: i32) {
    let mut count: i32 = initial;

    // ...
}
```

### Reactive State (signal)

Signals are reactive state variables that automatically update the DOM when changed:

```nectar
component UserProfile(id: String) {
    signal user_name: String = "Loading...";

    // When user_name changes, the DOM updates automatically
    render {
        <span>{self.user_name}</span>
    }
}
```

### Methods

Components can define methods for event handling and business logic:

```nectar
component Counter(initial: i32) {
    let mut count: i32 = initial;

    fn increment(&mut self) {
        self.count = self.count + 1;
    }

    fn decrement(&mut self) {
        self.count = self.count - 1;
    }

    render {
        <div>
            <span>{self.count}</span>
            <button on:click={self.increment}>"+1"</button>
            <button on:click={self.decrement}>"-1"</button>
        </div>
    }
}
```

### Scoped Styles

CSS styles are scoped to the component automatically. Styles never leak to parent or sibling components:

```nectar
component Card() {
    style {
        .card {
            padding: "16px";
            border-radius: "8px";
            box-shadow: "0 2px 8px rgba(0,0,0,0.1)";
        }
        .card h2 {
            color: "#1e293b";
            margin-bottom: "8px";
        }
    }

    render {
        <div class="card">
            <h2>"My Card"</h2>
        </div>
    }
}
```

### Critical Styles

When building with `nectar build --ssr --critical-css`, the compiler automatically determines which component styles are critical (needed for the initial above-the-fold render) and which can be deferred.

By default, all non-lazy component styles are treated as critical. Lazy component styles are deferred unless the component is the first route target in a router.

The following built-in utility classes are always inlined as critical CSS:

- `.nectar-skeleton` -- base skeleton loading placeholder with shimmer animation
- `.nectar-skeleton-text` -- text-shaped skeleton placeholder
- `.nectar-skeleton-avatar` -- circular avatar-shaped skeleton placeholder
- `.nectar-skeleton-rect` -- rectangular skeleton placeholder

These can be used directly in component templates to provide instant loading feedback during SSR hydration:

```nectar
component UserProfile(id: u32) {
    state user: Option<User> = None;

    render {
        <div class="profile">
            {match self.user {
                Some(u) => <span>{u.name}</span>,
                None => <div class="nectar-skeleton nectar-skeleton-text" />,
            }}
        </div>
    }
}
```

### Transitions

Declare CSS transitions on component properties:

```nectar
component FadeBox() {
    transition {
        opacity: "0.3s ease";
        transform: "0.5s cubic-bezier(0.4, 0, 0.2, 1)";
    }

    render {
        <div class="fade-box">"Content"</div>
    }
}
```

### Error Boundaries

Error boundaries catch rendering errors and display fallback UI:

```nectar
component SafeWidget() {
    error_boundary {
        fallback {
            <div class="error">"Something went wrong."</div>
        }
        {
            <RiskyComponent />
        }
    }

    render {
        <div>"Widget content"</div>
    }
}
```

### Skeleton Screens

Skeleton screens define placeholder UI that renders immediately (including during SSR) while the component's data is loading. The skeleton block is shown first and automatically replaced with the real `render` content once the component's signals change from their initial values.

```nectar
component UserProfile(id: u32) {
    signal user: Option<User> = None;

    skeleton {
        <div class="skeleton">
            <div class="skeleton-avatar" />
            <div class="skeleton-line" style="width: 60%" />
            <div class="skeleton-line" style="width: 40%" />
        </div>
    }

    render {
        <div class="profile">
            <img src={self.user.avatar} />
            <h1>{self.user.name}</h1>
        </div>
    }
}
```

**How it works:**

- During SSR, the skeleton HTML is rendered with a `data-nectar-skeleton` marker and a built-in `nectar-skeleton` CSS class that applies a pulse animation.
- On the client, the skeleton DOM is mounted first into the root element.
- An effect watches the component's signals. When any signal changes from its initial value, the skeleton fades out and the real `render` content fades in.
- Built-in CSS provides both a pulse animation and a shimmer effect for elements with `skeleton-` prefixed class names.

**Skeleton blocks are optional.** Components without a `skeleton` block render their `render` content immediately as before.

### Generic Components

Components can accept type parameters with optional trait bounds:

```nectar
component List<T>(items: [T]) where T: Display {
    render {
        <ul>
            {for item in items {
                <li>{item.to_string()}</li>
            }}
        </ul>
    }
}
```

### Lazy Components

Lazy components are only loaded when first rendered, enabling code splitting:

```nectar
lazy component HeavyChart(data: [f64]) {
    render {
        <canvas />
    }
}
```

---

## Stores

Stores provide global reactive state management, similar to Redux/Flux patterns. Any component can read from and dispatch actions to a store.

### Basic Store

```nectar
store CounterStore {
    signal count: i32 = 0;
    signal step: i32 = 1;

    action increment(&mut self) {
        self.count = self.count + self.step;
    }

    action decrement(&mut self) {
        self.count = self.count - self.step;
    }

    computed double_count(&self) -> i32 {
        self.count * 2
    }

    effect on_count_change(&self) {
        println(self.count);
    }
}
```

### Signal Fields

Store state is declared with `signal`. These are reactive: any component reading a signal will automatically re-render when it changes.

```nectar
signal count: i32 = 0;
signal user: Option<User> = None;
```

### Actions

Actions are methods that mutate store state. They can be synchronous or asynchronous:

```nectar
// Synchronous action
action increment(&mut self) {
    self.count = self.count + 1;
}

// Async action
async action fetch_user(&mut self, id: u32) {
    let response = await fetch(f"https://api.example.com/users/{id}");
    self.user = response.json();
}
```

### Computed Values

Computed values are derived from signals. They are cached and only recompute when their dependencies change:

```nectar
computed is_logged_in(&self) -> bool {
    match self.status {
        AuthStatus::LoggedIn(_) => true,
        _ => false,
    }
}
```

### Effects

Effects are side-effect callbacks that run whenever their signal dependencies change:

```nectar
effect on_auth_change(&self) {
    match self.status {
        AuthStatus::LoggedIn(user) => {
            println(f"User logged in: {user.name}");
        }
        _ => {}
    }
}
```

### Using Stores from Components

Components access store state and dispatch actions using the `StoreName::` syntax:

```nectar
component Dashboard() {
    render {
        <div>
            <p>{f"Count: {CounterStore::get_count()}"}</p>
            <button on:click={CounterStore::increment}>"+"</button>
        </div>
    }
}
```

---

## Structs and Enums

### Struct Definition

Structs group named fields together:

```nectar
struct User {
    id: u32,
    name: String,
    email: String,
}

pub struct Point<T> {
    pub x: T,
    pub y: T,
}
```

Fields can be marked `pub` for public visibility. Structs support lifetimes and generic type parameters:

```nectar
struct Ref<'a, T> {
    value: &'a T,
}
```

### Struct Initialization

Create struct instances with field-value syntax:

```nectar
let user = User {
    id: 1,
    name: "Alice",
    email: "alice@example.com",
};
```

### Enum Definition

Enums define a type that can be one of several variants. Variants may carry data:

```nectar
enum Filter {
    All,
    Active,
    Completed,
}

enum AuthStatus {
    LoggedOut,
    Loading,
    LoggedIn(User),
    Error(String),
}

enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

### Impl Blocks

Add methods to structs and enums with `impl`:

```nectar
impl User {
    fn full_name(&self) -> String {
        return f"{self.first_name} {self.last_name}";
    }

    pub fn new(name: String, email: String) -> Self {
        return User { id: 0, name: name, email: email };
    }
}
```

### Trait Implementations

Implement traits for types with `impl Trait for Type`:

```nectar
impl Display for User {
    fn to_string(&self) -> String {
        return f"User({self.name})";
    }
}
```

---

## Traits

### Trait Definition

Traits define shared behavior (interfaces). Methods can have default implementations:

```nectar
trait Display {
    fn to_string(&self) -> String;
}

trait Drawable {
    fn draw(&self);

    fn bounds(&self) -> (f64, f64) {
        // default implementation
        return (0.0, 0.0);
    }
}
```

### Generic Traits

Traits can have type parameters:

```nectar
trait Container<T> {
    fn get(&self, index: i32) -> T;
    fn size(&self) -> i32;
}
```

### Trait Bounds

Use trait bounds to constrain generic type parameters:

```nectar
fn print_item<T>(item: T) where T: Display {
    println(item.to_string());
}
```

---

## Expressions

Nectar is expression-oriented. Most constructs produce a value.

### Arithmetic Expressions

```nectar
let sum = a + b;
let diff = a - b;
let product = a * b;
let quotient = a / b;
let remainder = a % b;
let negated = -x;
```

### Comparison Expressions

```nectar
a == b    // equal
a != b    // not equal
a < b     // less than
a > b     // greater than
a <= b    // less or equal
a >= b    // greater or equal
```

### Logical Expressions

```nectar
a && b    // logical AND
a || b    // logical OR
!a        // logical NOT
```

### Assignment Expressions

```nectar
x = 42;
x += 1;     // desugars to x = x + 1
x -= 1;
x *= 2;
x /= 2;
```

### Field Access and Method Calls

```nectar
user.name              // field access
user.full_name()       // method call
items.len()            // method call
items.push(42)         // method call with argument
```

### Function Calls

```nectar
greet("Alice")
add(1, 2)
Module::function(arg)
```

### Index Expressions

```nectar
items[0]
matrix[i][j]
```

### If/Else Expressions

`if`/`else` is an expression that produces a value:

```nectar
let max = if a > b { a } else { b };

if condition {
    do_something();
}

if x > 0 {
    "positive"
} else {
    "non-positive"
}
```

### Match Expressions

Pattern matching with `match`:

```nectar
match status {
    AuthStatus::LoggedIn(user) => show_dashboard(user),
    AuthStatus::Loading => show_spinner(),
    AuthStatus::Error(msg) => show_error(msg),
    _ => show_login(),
}
```

### For Loops

Iterate over collections:

```nectar
for item in items {
    process(item);
}

for todo in &mut self.todos {
    if todo.id == id {
        todo.done = !todo.done;
    }
}
```

### While Loops

```nectar
while count < 10 {
    count = count + 1;
}
```

### Closures

Closures (lambdas) capture variables from their environment:

```nectar
// With type annotations
let add = |a: i32, b: i32| a + b;

// Without type annotations
let double = |x| x * 2;

// No parameters
let greet = || println("hello");

// Block body
let process = |item: Item| {
    validate(item);
    save(item);
};
```

Closures can also be written with `fn` syntax in certain positions:

```nectar
items.filter(fn(t: &Todo) -> bool { !t.done })
```

### Await Expressions

Await an asynchronous operation:

```nectar
let response = await fetch("https://api.example.com/data");
let data = await process(response);
```

### Fetch Expressions

First-class HTTP communication:

```nectar
// Simple GET
let response = fetch("https://api.example.com/users");

// With options
let response = fetch("https://api.example.com/posts", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: json_string,
});
```

### Spawn and Channel Expressions

Concurrency primitives:

```nectar
// Spawn work on a background thread
spawn {
    heavy_computation()
}

// Create a typed channel
let ch = channel<i32>();

// Send and receive
ch.send(42);
let value = ch.recv();
```

### Parallel Expressions

Run multiple expressions concurrently:

```nectar
parallel {
    fetch_users(),
    fetch_posts(),
    fetch_comments(),
}
```

### Try/Catch Expressions

Structured error handling:

```nectar
try {
    let data = parse(input)?;
    process(data);
} catch err {
    log_error(err);
}
```

### Error Propagation (`?` Operator)

The `?` postfix operator unwraps a `Result` or `Option`, propagating the error on failure:

```nectar
fn load_config() -> Result<Config, String> {
    let text = read_file("config.toml")?;
    let config = parse_toml(text)?;
    return Ok(config);
}
```

### Navigate Expressions

Programmatic client-side navigation:

```nectar
navigate("/user/42");
navigate(f"/posts/{post_id}");
```

### Stream Expressions

Process async data as it arrives:

```nectar
for chunk in stream fetch("https://api.example.com/stream") {
    process_chunk(chunk);
}
```

### Suspend Expressions

Show fallback content while loading:

```nectar
suspend(<LoadingSpinner />) {
    <HeavyComponent />
}
```

### Animate Expressions

Trigger a named animation imperatively:

```nectar
animate(element, "fadeIn");
```

### Format Strings

Interpolate expressions into strings:

```nectar
let msg = f"Hello {name}, you have {count} items";
let url = f"https://api.example.com/users/{id}";
```

### Prompt Templates

AI prompt templates with interpolation:

```nectar
let p = prompt "Summarize the following document: {document}";
```

### Struct Initialization

Construct struct instances inline:

```nectar
let user = User { name: "Alice", age: 30 };
```

### Borrow and Mutable Borrow

```nectar
let r = &value;         // immutable borrow
let mr = &mut value;    // mutable borrow
```

### Block Expressions

Blocks are expressions that evaluate to their last expression:

```nectar
let result = {
    let x = compute();
    let y = transform(x);
    x + y
};
```

---

## Statements

### Let Statements

Bind a value to a name:

```nectar
let x = 42;
let mut name: String = "Nectar";
let (a, b) = get_pair();
let User { name, email, .. } = user;
```

### Signal Statements

Declare a reactive signal:

```nectar
signal count: i32 = 0;
signal visible: bool = true;
```

### Return Statements

Exit a function with an optional value:

```nectar
return;
return 42;
return Ok(result);
```

### Yield Statements

Emit a value from a stream:

```nectar
yield chunk;
yield f"data: {value}\n";
```

### Expression Statements

Any expression can appear as a statement. A trailing semicolon is optional:

```nectar
process(data);
self.count = self.count + 1;
```

---

## Patterns

Patterns are used in `match` arms, `let` destructuring, and `for` bindings.

### Wildcard Pattern

Matches anything, ignores the value:

```nectar
_ => default_action(),
```

### Identifier Pattern

Binds the matched value to a name:

```nectar
x => use_value(x),
```

### Literal Pattern

Matches a specific value:

```nectar
42 => handle_forty_two(),
"hello" => handle_greeting(),
true => handle_true(),
```

### Variant Pattern

Matches an enum variant, optionally binding inner fields:

```nectar
Some(value) => use_value(value),
AuthStatus::LoggedIn(user) => show_user(user),
None => show_empty(),
```

### Tuple Pattern

Destructure a tuple:

```nectar
let (x, y) = point;
(0, 0) => handle_origin(),
(x, _) => use_x_only(x),
```

### Struct Pattern

Destructure a struct, with an optional `..` to ignore remaining fields:

```nectar
let User { name, age, .. } = user;
```

### Array Pattern

Destructure an array:

```nectar
let [first, second, ..] = items;
```

---

## Modules

### Module Declaration

Declare an external module (loaded from a separate file):

```nectar
mod utils;          // loads ./utils.nectar or ./utils/mod.nectar
mod networking;     // loads ./networking.nectar
```

Declare an inline module:

```nectar
mod helpers {
    pub fn capitalize(s: String) -> String {
        // ...
    }
}
```

### Use/Import

Import items from other modules:

```nectar
// Import a single item
use std::string;

// Import with alias
use http::Client as HttpClient;

// Glob import (all public items)
use utils::*;

// Group import
use std::{string, collections, io};

// Group import with aliases
use models::{User, Post as BlogPost};
```

### Visibility

Items are private by default. Mark them `pub` for public access:

```nectar
pub struct User { ... }
pub fn create_user(...) { ... }

struct Internal { ... }  // private
```

---

## Templates

Templates are the JSX-like rendering syntax used in component `render` blocks.

### Elements

HTML elements with static attributes:

```nectar
<div class="container">
    <h1>"Title"</h1>
    <p>"Paragraph text"</p>
</div>
```

### Self-Closing Elements

```nectar
<input placeholder="Enter text" />
<br />
<NavBar />
```

### Static Attributes

String-valued attributes:

```nectar
<div class="card" id="main">
<input type="text" placeholder="Search..." />
```

### Dynamic Attributes

Expression-valued attributes use curly braces:

```nectar
<div class={dynamic_class}>
<span>{self.count}</span>
<img src={image_url} />
```

### Event Handlers

Event handlers use the `on:event` syntax:

```nectar
<button on:click={self.handle_click}>"Click me"</button>
<input on:submit={self.handle_submit} />
<div on:mouseover={self.show_tooltip} />
```

### Two-Way Bindings

The `bind:property` syntax creates two-way data binding between a signal and a form element:

```nectar
<input bind:value={search_query} />
<input type="checkbox" bind:checked={is_active} />
```

### ARIA Attributes

Accessibility attributes are first-class:

```nectar
<button aria-label="Close dialog" aria-expanded={is_open}>
<nav aria-hidden="true">
<div aria-live="polite" aria-describedby="description">
```

### Role Attributes

```nectar
<div role="button" tabindex="0">
<nav role="navigation">
```

### Text Content

Text content is written as string literals inside elements:

```nectar
<p>"This is text content."</p>
```

### Expression Interpolation

Expressions inside curly braces render their value:

```nectar
<span>{self.count}</span>
<p>{f"Total: {items.len()} items"}</p>
```

### Conditional Rendering

```nectar
{if self.loading {
    <div>"Loading..."</div>
}}

{if show_details {
    <Details data={self.data} />
} else {
    <Summary />
}}
```

### List Rendering

```nectar
{for item in self.items {
    <li>{item.name}</li>
}}

{for post in PostService::get_posts() {
    <article>
        <h3>{post.title}</h3>
        <p>{post.body}</p>
    </article>
}}
```

### Match in Templates

```nectar
{match status {
    Some(err) => <div class="error">{err.message}</div>,
    None => <span />,
}}
```

### Link Elements

The `<Link>` element provides client-side navigation:

```nectar
<Link to="/">"Home"</Link>
<Link to="/about">"About"</Link>
<Link to={f"/user/{id}"}>"Profile"</Link>
```

### Fragment

Group multiple elements without an extra wrapper node:

```nectar
<Fragment>
    <h1>"Title"</h1>
    <p>"Content"</p>
</Fragment>
```

### Child Components

Render other components as elements:

```nectar
<NavBar />
<Counter initial={0} />
<UserCard user={current_user} />
```

---

## Agents

Agents are first-class constructs for building AI-powered interactions. They wrap LLM communication with tool definitions and reactive UI.

### Agent Definition

```nectar
agent ChatBot {
    prompt system = "You are a helpful coding assistant.";

    signal messages: [Message] = [];
    signal input: String = "";
    signal streaming: bool = false;

    tool search_docs(query: String) -> String {
        let results = await fetch(f"https://api.example.com/search?q={query}");
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
        ai::chat_stream(self.messages, self.tools);
    }

    render {
        <div class="chat">
            <div class="messages">
                {for msg in self.messages {
                    <div class={msg.role}>{msg.content}</div>
                }}
            </div>
            <input value={self.input} on:submit={self.send} />
        </div>
    }
}
```

### System Prompt

Define the AI's system prompt:

```nectar
prompt system = "You are a helpful assistant specializing in data analysis.";
```

### Tools

Tools are functions the AI can call. They have typed parameters and return types:

```nectar
tool get_weather(city: String) -> String {
    let result = await fetch(f"https://api.example.com/weather?city={city}");
    return result.json().forecast;
}
```

---

## Routers

Routers map URL paths to components for client-side navigation.

### Router Definition

```nectar
router AppRouter {
    route "/" => Home,
    route "/about" => About,
    route "/user/:id" => UserProfile,
    route "/admin/*" => AdminPanel guard { AuthStore::is_logged_in() },
    fallback => NotFound,
}
```

### Route Patterns

- **Static**: `"/about"` -- matches exactly `/about`
- **Parameterized**: `"/user/:id"` -- captures `id` from the URL
- **Wildcard**: `"/admin/*"` -- matches any path under `/admin/`

### Route Guards

Guards are expressions that must evaluate to `true` for the route to be accessible:

```nectar
route "/admin/*" => AdminPanel guard { AuthStore::is_logged_in() },
```

### Fallback Route

The fallback component renders when no route matches:

```nectar
fallback => NotFound,
```

### Programmatic Navigation

Navigate from code:

```nectar
navigate("/user/42");
```

---

## Testing

### Test Blocks

Test blocks define named test cases:

```nectar
test "addition works" {
    let result = add(2, 3);
    assert_eq(result, 5);
}

test "user creation" {
    let user = User::new("Alice", "alice@test.com");
    assert(user.name == "Alice");
    assert_eq(user.email, "alice@test.com");
}
```

### Assertions

**`assert(condition)`** -- asserts that a condition is true:

```nectar
assert(x > 0);
assert(list.len() > 0, "list should not be empty");
```

**`assert_eq(left, right)`** -- asserts that two values are equal:

```nectar
assert_eq(result, 42);
assert_eq(name, "Alice", "names should match");
```

Both assertion forms accept an optional message string as the last argument.

### Running Tests

```sh
nectar test tests.nectar
nectar test tests.nectar --filter "addition"
nectar test tests.nectar --verbose
```

### Component Testing with the Test Renderer

Nectar includes a built-in test renderer that mounts components into a virtual DOM for testing without a browser. The `render()` function returns a `TestElement` with query and interaction methods.

#### Mounting a Component

```nectar
test "greeting renders correctly" {
    let el = render(<Greeting name="Nectar" />);
    let heading = el.findByText("Hello, Nectar!");
    assert(heading.exists());
}
```

#### Query Methods

- **`findByText(text)`** -- find a descendant element containing the given text
- **`findByRole(role)`** -- find an element with a matching ARIA `role` attribute
- **`findByAttribute(name, value)`** -- find an element with a specific attribute value
- **`children()`** -- get all direct child `TestElement`s
- **`getText()`** -- get the text content of the element and its descendants
- **`getAttribute(name)`** -- get a single attribute value
- **`exists()`** -- returns `true` if the element was found

#### Interaction Methods

- **`click()`** -- dispatch a click event on the element
- **`type(text)`** -- simulate text input (sets value, fires input and change events)

#### Simulating User Interaction

```nectar
test "counter increments on click" {
    let el = render(<Counter />);
    let btn = el.findByText("+1");
    let display = el.findByRole("counter");

    btn.click();
    assert_eq(display.getText(), "1");

    btn.click();
    btn.click();
    assert_eq(display.getText(), "3");
}
```

After each `click()` or `type()` call, the test renderer processes all reactive updates synchronously. Subsequent queries reflect the updated DOM state -- no manual flushing is required.

#### Testing Props and Defaults

```nectar
component Badge(label: String = "default") {
    render { <span>{self.label}</span> }
}

test "default prop is applied" {
    let el = render(<Badge />);
    assert_eq(el.findByText("default").getText(), "default");
}

test "explicit prop overrides default" {
    let el = render(<Badge label="custom" />);
    assert_eq(el.findByText("custom").getText(), "custom");
}
```

#### Testing Conditional and List Rendering

```nectar
test "conditional rendering" {
    let el = render(<Alert show={true} />);
    assert(el.findByText("Warning!").exists());
}

test "list rendering" {
    let el = render(<ItemList items={["a", "b", "c"]} />);
    assert(el.findByText("a").exists());
    assert(el.findByText("b").exists());
    assert(el.findByText("c").exists());
}
```

#### Testing Store Integration

Components that read from stores can be tested end-to-end:

```nectar
test "store-connected component updates on action" {
    let el = render(<StoreCounter />);
    let btn = el.findByText("+1");

    btn.click();

    let display = el.findByRole("display");
    assert_eq(display.getText(), "Store count: 1");
}
```

### Agent Testing

Agents are testable like components but have additional capabilities for verifying tool registration, tool dispatch, and AI interaction mocking.

#### Testing Tool Registration

```nectar
test "agent registers tools" {
    let tools = MyAgent::get_registered_tools();
    assert_eq(tools.len(), 2);
    assert_eq(tools[0].name, "search");
    assert_eq(tools[1].name, "calculate");
}
```

`get_registered_tools()` returns metadata about each `tool` block: name, parameter names and types, and return type.

#### Testing Tool Dispatch

```nectar
test "dispatch tool with typed args" {
    let result = await MyAgent::dispatch_tool("search", {
        query: "nectar language",
    });
    // Verify the tool executed correctly
}
```

`dispatch_tool(name, args)` invokes a tool by name with a typed argument object, simulating what the runtime does when the AI model requests a tool call.

#### Mocking AI Responses

The `ai::mock_response()` and `ai::mock_tool_call()` functions install canned responses for testing without a real LLM:

```nectar
test "mock a text response" {
    ai::mock_response("The answer is 42.");
    let response = await ai::chat_complete(messages);
    assert_eq(response.content, "The answer is 42.");
}

test "mock a tool call response" {
    ai::mock_tool_call("get_weather", { city: "Paris" });
    let response = await ai::chat_complete(messages);
    assert_eq(response.tool_calls[0].name, "get_weather");
}
```

#### Mocking Streaming Responses

```nectar
test "mock streaming tokens" {
    ai::mock_stream(["Hello", " ", "world"]);
    let mut text = "";
    for chunk in stream ai::chat_stream(messages) {
        text = text + chunk;
    }
    assert_eq(text, "Hello world");
}
```

### Async Test Patterns

Test blocks support `await` for testing asynchronous operations:

```nectar
test "async fetch in tests" {
    let response = await fetch("https://api.example.com/data");
    assert(response.status == 200 || response.status == 0);
}
```

In the test environment, HTTP imports are stubbed by the test runner. The `fetch` calls resolve immediately without hitting real endpoints. This allows testing the async control flow without external dependencies.

For sequential async operations:

```nectar
test "sequential async" {
    let a = await fetch("https://api.example.com/step1");
    let b = await fetch("https://api.example.com/step2");
    assert(true, "both requests completed");
}
```

### Test Organization Best Practices

**Use descriptive test names.** Test names appear in the output when tests fail. Use full sentences that describe the expected behavior:

```nectar
// Good
test "counter increments on click" { ... }
test "empty list renders zero total" { ... }
test "login fails with invalid credentials" { ... }

// Avoid
test "test1" { ... }
test "counter" { ... }
```

**One assertion focus per test.** Each test should verify one behavior. Multiple `assert` calls are fine when they verify facets of the same behavior:

```nectar
test "user creation sets all fields" {
    let user = User::new("Alice", "alice@test.com", 30);
    assert_eq(user.name, "Alice");
    assert_eq(user.email, "alice@test.com");
    assert_eq(user.age, 30);
}
```

**Organize tests near related code.** Place `test` blocks at the bottom of the file after the types and functions they test, or in dedicated test files:

```
examples/
    todo.nectar              # Application code
    tests.nectar             # Unit tests for logic
    component-tests.nectar   # Component integration tests
    agent-tests.nectar       # Agent behavior tests
```

**Use `--filter` for focused testing.** During development, run only the tests relevant to your current change:

```sh
nectar test tests.nectar --filter "fibonacci"
nectar test component-tests.nectar --filter "counter"
```

**Reset shared state between tests.** When testing stores or agents, call the `clear` or `reset` method at the start of each test to avoid state leaking between tests:

```nectar
test "store starts fresh" {
    MyStore::reset();
    assert_eq(MyStore::get_count(), 0);
}
```

**Test error paths, not just happy paths.** Use `Result`, `Option`, and `try/catch` to verify that error handling works correctly:

```nectar
test "division by zero returns error" {
    let result = divide(10.0, 0.0);
    match result {
        Result::Ok(_) => assert(false, "should not succeed"),
        Result::Err(e) => assert_eq(e, "division by zero"),
    }
}
```
