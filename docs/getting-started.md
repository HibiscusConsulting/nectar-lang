# Getting Started with Nectar

This guide walks you through installing Nectar, writing your first program, and building a complete application.

---

## Prerequisites

Nectar's compiler is written in Rust. You need:

- **Rust toolchain** (1.70 or later) -- install from [rustup.rs](https://rustup.rs)
- **A modern web browser** -- for viewing compiled output (Chrome, Firefox, Safari, or Edge)

Verify your Rust installation:

```sh
rustc --version
cargo --version
```

---

## Installation

### Build from Source

Clone the Nectar repository and build the compiler:

```sh
git clone https://github.com/HibiscusConsulting/nectar-lang.git
cd nectar-lang
cargo build --release
```

The compiled binary is at `target/release/nectar`. Add it to your PATH:

```sh
export PATH="$PWD/target/release:$PATH"
```

Verify the installation:

```sh
nectar --version
```

---

## Your First Nectar Program

Create a file called `hello.nectar`:

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

This defines a `Hello` component that takes a `name` prop and renders it inside a `<p>` tag.

### Compiling

Compile to WebAssembly Text Format (WAT):

```sh
nectar build hello.nectar
```

This produces `hello.wat`. To compile directly to binary WebAssembly:

```sh
nectar build hello.nectar --emit-wasm
```

This produces `hello.wasm`.

### Running in the Browser

Create an `index.html` file:

```html
<!DOCTYPE html>
<html>
<head><title>My Nectar App</title></head>
<body>
  <div id="app"></div>
  <script type="module">
    import { instantiate } from './core.js';
    instantiate('hello.wasm').then(inst => inst.exports.main());
  </script>
</body>
</html>
```

Copy `runtime/modules/core.js` alongside your HTML file, serve it with any static server, and open it in a browser.

Or use the built-in dev server:

```sh
nectar dev --src . --port 3000
```

Open `http://localhost:3000` to see your component.

---

## Adding State and Reactivity

Let's build a counter. Create `counter.nectar`:

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
            <h2>"Counter"</h2>
            <span>{self.count}</span>
            <button on:click={self.increment}>"+1"</button>
            <button on:click={self.decrement}>"-1"</button>
        </div>
    }
}
```

Key concepts:

- **`let mut count`** declares mutable state.
- **`fn increment(&mut self)`** is a method that mutates the component state. The `&mut self` parameter means it borrows the component mutably.
- **`on:click={self.increment}`** binds the click event to the method.
- **`{self.count}`** in the template reactively displays the current count. When `count` changes, only this DOM node updates -- no virtual DOM diffing needed.

Compile and run:

```sh
nectar build counter.nectar --emit-wasm
```

---

## Handling Events

Nectar uses the `on:event` syntax for DOM events. The handler is any expression, typically a method reference:

```nectar
component Form() {
    let mut value: String = "";

    fn handle_submit(&mut self) {
        // Process the form
        println(f"Submitted: {self.value}");
        self.value = "";
    }

    render {
        <div>
            <input
                value={self.value}
                placeholder="Enter text..."
            />
            <button on:click={self.handle_submit}>"Submit"</button>
        </div>
    }
}
```

Common events: `click`, `submit`, `input`, `change`, `mouseover`, `mouseout`, `keydown`, `keyup`.

---

## Fetching Data from an API

Nectar has first-class support for HTTP communication via the `fetch` keyword:

```nectar
struct Post {
    id: u32,
    title: String,
    body: String,
}

store PostService {
    signal posts: [Post] = [];
    signal loading: bool = false;

    async action fetch_posts(&mut self) {
        self.loading = true;

        let response = await fetch("https://jsonplaceholder.typicode.com/posts");

        if response.status == 200 {
            self.posts = response.json();
        }
        self.loading = false;
    }

    async action create_post(&mut self, title: String, body: String) {
        let response = await fetch("https://jsonplaceholder.typicode.com/posts", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: f"{{\"title\": \"{title}\", \"body\": \"{body}\"}}",
        });

        if response.status == 201 {
            let new_post: Post = response.json();
            self.posts.push(new_post);
        }
    }
}

component PostList() {
    render {
        <div>
            <h1>"Posts"</h1>
            {if PostService::get_loading() {
                <div>"Loading..."</div>
            }}
            <ul>
                {for post in PostService::get_posts() {
                    <li>
                        <h3>{post.title}</h3>
                        <p>{post.body}</p>
                    </li>
                }}
            </ul>
        </div>
    }
}
```

Key concepts:

- **`store`** defines global reactive state accessible from any component.
- **`async action`** declares an asynchronous action that can use `await`.
- **`fetch(url, options)`** makes HTTP requests with method, headers, and body.
- **Components read store state** with `StoreName::get_field()` and dispatch actions with `StoreName::action_name()`.

---

## Building a Complete Todo App

Let's walk through `todo.nectar`, a complete todo application demonstrating structs, enums, ownership, and components.

### Step 1: Define Data Types

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
```

`Todo` is a simple struct holding each item's data. `Filter` is an enum for the three filter states.

### Step 2: Build the Component

```nectar
component TodoApp() {
    let mut todos: [Todo] = [];
    let mut next_id: u32 = 0;
    let mut filter: Filter = Filter::All;
```

The component maintains a list of todos, a counter for generating unique IDs, and the current filter.

### Step 3: Add Business Logic

```nectar
    fn add_todo(&mut self, text: String) {
        let todo = Todo {
            id: self.next_id,
            text: text,
            done: false,
        };
        self.next_id = self.next_id + 1;
        // Ownership: todo is moved into the collection
        self.todos.push(todo);
    }

    fn toggle(&mut self, id: u32) {
        for todo in &mut self.todos {
            if todo.id == id {
                todo.done = !todo.done;
            }
        }
    }
```

Notice the ownership semantics: `todo` is **moved** into the `todos` array -- there is no implicit copy. The `toggle` method borrows the array mutably with `&mut self.todos` to modify items in place.

### Step 4: Filter with Pattern Matching

```nectar
    fn visible_todos(&self) -> [&Todo] {
        match self.filter {
            Filter::All => &self.todos,
            Filter::Active => self.todos.iter().filter(fn(t: &Todo) -> bool { !t.done }),
            Filter::Completed => self.todos.iter().filter(fn(t: &Todo) -> bool { t.done }),
        }
    }
```

Pattern matching on `self.filter` returns the appropriate subset. The return type `[&Todo]` signals that we return borrowed references, not copies.

### Step 5: Render the UI

```nectar
    render {
        <div>
            <h1>"Nectar Todo"</h1>
            <div>
                <input placeholder="What needs to be done?" />
                <button on:click={self.add_todo}>"Add"</button>
            </div>
            <ul>
                {self.visible_todos()}
            </ul>
        </div>
    }
}
```

The template renders the filtered todo list. Thanks to fine-grained reactivity, only the changed DOM nodes update when a todo is added or toggled.

### Compile and Run

```sh
nectar build todo.nectar --emit-wasm
nectar dev --port 3000
```

---

## Initializing a Project

For larger projects, use `nectar init` to create a project with dependency management:

```sh
mkdir my-app && cd my-app
nectar init --name my-app
```

This creates an `Nectar.toml` manifest:

```toml
[package]
name = "my-app"
version = "0.1.0"

[dependencies]
```

Add dependencies:

```sh
nectar add ui-components --version "^1.0"
nectar install
```

Build the project:

```sh
nectar build src/main.nectar --emit-wasm -O2
```

---

## Component Composition with Routers

Nectar supports nested routing with layout components using `<Outlet />`:

```nectar
component NavBar() {
    render {
        <nav>
            <a on:click={self.go_home}>"Home"</a>
            <a on:click={self.go_about}>"About"</a>
        </nav>
    }

    fn go_home(&mut self) {
        navigate("/");
    }

    fn go_about(&mut self) {
        navigate("/about");
    }
}

router AppRouter {
    layout {
        <div>
            <NavBar />
            <Outlet />
        </div>
    }

    route "/" => Home,
    route "/about" => About,
    fallback => NotFound,
}
```

The `<Outlet />` element marks where the routed page content renders. The surrounding layout (NavBar) persists across navigations without re-rendering.

---

## Generics and Monomorphization

Nectar supports generic functions that are specialized at compile time:

```nectar
fn max<T>(a: T, b: T) -> T where T: Ord {
    if a > b { a } else { b }
}

let bigger = max(10, 20);          // generates max__i32
let longer = max("abc", "xyz");    // generates max__String
```

The compiler creates a separate WASM function for each concrete type, eliminating runtime type dispatch.

---

## Next Steps

Now that you have the basics, explore these resources:

- **[Language Reference](language-reference.md)** -- complete syntax and semantics for every construct
- **[Toolchain Reference](toolchain.md)** -- all CLI commands, flags, and configuration
- **[Runtime API Reference](runtime-api.md)** -- every WASM import function available at runtime
- **[Architecture & Internals](architecture.md)** -- how the compiler works under the hood
- **[Examples Guide](examples.md)** -- detailed walkthroughs of all example programs
