# Nectar Language — AI Quick Reference

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

## Component
```nectar
component Counter(initial: i32 = 0) {
  let mut count: i32 = initial;          // state
  signal label: String = "Count";        // reactive state

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
Blocks (all optional except render): props in parens, state, methods, style, transition, skeleton, error_boundary, render.

## Generic / Lazy Components
```nectar
component List<T>(items: [T]) where T: Display {
  render { <ul>{for item in items { <li>{item.to_string()}</li> }}</ul> }
}
lazy component HeavyChart(data: [f64]) { render { <canvas /> } }
```

## Store
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
// Usage from component: AppStore::increment(), AppStore::double()
```

## Agent
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

## Router
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

## Struct / Enum / Impl / Trait
```nectar
struct User { id: u32, name: String, email: String }
pub struct Point<T> { pub x: T, pub y: T }
enum Status { Active, Inactive, Error(String) }
impl User { pub fn new(n: String) -> Self { User { id: 0, name: n, email: "" } } }
impl Display for User { fn to_string(&self) -> String { self.name } }
trait Drawable { fn draw(&self); fn bounds(&self) -> (f64, f64) { (0.0, 0.0) } }
```

## Template Syntax (inside render blocks)
```nectar
<div class="static" id="x">"text content"</div>  // element + text
<img src={dynamic_url} />                         // dynamic attr
<button on:click={self.handle}>"Click"</button>   // event handler
<input bind:value={query} />                      // two-way bind
<button aria-label="Close" role="button" />       // accessibility
{if loading { <Spinner /> } else { <Content /> }} // conditional
{for item in items { <li>{item.name}</li> }}      // loop
{match s { Some(e) => <Err m={e} />, _ => <Ok /> }} // match
<UserCard user={u} />                             // child component
<Link to="/about">"About"</Link>                  // client-side nav
<Fragment><h1>"A"</h1><p>"B"</p></Fragment>       // fragment
```

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
fetch(url, { method: "POST" })           // HTTP request
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

## Modules and Imports
```nectar
mod utils;                              // external file
mod helpers { pub fn cap(s: String) -> String { /* ... */ } }
use std::collections;                   // single import
use http::Client as HttpClient;         // aliased import
use utils::*;                           // glob import
use models::{User, Post as BlogPost};   // group import
```

## Testing
```nectar
test "math works" {
  assert_eq(add(2, 3), 5);
  assert(10 > 0, "should be positive");
}
```
Run: `nectar test file.nectar` or `nectar test file.nectar --filter "math"`

## Ownership
One owner per value. `&val` immutable borrow (many OK). `&mut val` mutable borrow (exclusive). Assignment moves.

## Patterns (match / let destructuring)
`_` wildcard, `x` bind, `42`/`"hi"`/`true` literal, `Some(v)` variant, `(a, b)` tuple, `Name { f, .. }` struct, `[a, b, ..]` array.
