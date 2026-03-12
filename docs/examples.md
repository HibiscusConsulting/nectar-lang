# Nectar Examples Guide

This document walks through each example program in the `examples/` directory, explaining the concepts demonstrated and how they fit together.

---

## Table of Contents

1. [hello.nectar -- Hello World](#helloarc----hello-world)
2. [counter.nectar -- Stateful Counter](#counterarc----stateful-counter)
3. [todo.nectar -- Todo Application](#todoarc----todo-application)
4. [api.nectar -- API Communication](#apiarc----api-communication)
5. [store.nectar -- Global State Management](#storearc----global-state-management)
6. [app.nectar -- Routed Application with Styles](#apparc----routed-application-with-styles)
7. [ai-chat.nectar -- AI Chat Interface](#ai-chatarc----ai-chat-interface)
8. [tests.nectar -- Comprehensive Test Patterns](#testsarc----comprehensive-test-patterns)
9. [component-tests.nectar -- Component Testing Patterns](#component-testsarc----component-testing-patterns)
10. [agent-tests.nectar -- Agent Testing Patterns](#agent-testsarc----agent-testing-patterns)

---

## hello.nectar -- Hello World

**Concepts**: components, props, render templates

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

This is the simplest possible Nectar program. It demonstrates:

- **Component declaration**: `component Hello(...)` defines a reusable UI building block. The component name must be PascalCase.
- **Props**: `name: String` declares a property that the parent passes in when using `<Hello name="World" />`.
- **Render block**: Every component must have a `render { ... }` block that describes its DOM output.
- **Template syntax**: Nectar uses a JSX-like syntax. Static text is written in double quotes (`"Hello from Nectar!"`), and dynamic expressions are wrapped in curly braces (`{name}`).

**To compile and run:**

```sh
nectar build examples/hello.nectar --emit-wasm
```

---

## counter.nectar -- Stateful Counter

**Concepts**: mutable state, methods, event handlers, ownership

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

This example introduces interactivity:

- **Mutable state**: `let mut count: i32 = initial;` declares a state variable that can change over time. The initial value comes from the `initial` prop.
- **Methods**: `fn increment(&mut self)` and `fn decrement(&mut self)` are component methods. They take `&mut self` (a mutable borrow of the component) because they modify `self.count`.
- **Event handlers**: `on:click={self.increment}` binds the button's click event to the method. Nectar's reactivity system ensures that when `self.count` changes, only the `<span>` displaying the count is updated in the DOM -- no virtual DOM diffing is needed.
- **Ownership**: The `&mut self` parameter signals that these methods borrow the component mutably. Nectar's borrow checker ensures you cannot hold other borrows while calling these methods.

---

## todo.nectar -- Todo Application

**Concepts**: structs, enums, ownership, collections, pattern matching, closures

This is a more complete application demonstrating data modeling and business logic.

### Data Model

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

- **Structs** define product types -- `Todo` groups an ID, text, and completion status.
- **Enums** define sum types -- `Filter` can be one of three variants.

### Component State

```nectar
component TodoApp() {
    let mut todos: [Todo] = [];
    let mut next_id: u32 = 0;
    let mut filter: Filter = Filter::All;
```

The component maintains three pieces of state:
- A dynamic array of `Todo` items
- An auto-incrementing ID counter
- The current filter selection

### Adding Todos

```nectar
    fn add_todo(&mut self, text: String) {
        let todo = Todo {
            id: self.next_id,
            text: text,
            done: false,
        };
        self.next_id = self.next_id + 1;
        self.todos.push(todo);
    }
```

Key ownership concept: when `todo` is pushed into `self.todos`, ownership is **moved**. The local variable `todo` is no longer accessible after the push. This is how Nectar prevents use-after-free bugs at compile time.

### Toggling Completion

```nectar
    fn toggle(&mut self, id: u32) {
        for todo in &mut self.todos {
            if todo.id == id {
                todo.done = !todo.done;
            }
        }
    }
```

The `&mut self.todos` borrows the array mutably, giving each `todo` in the loop a mutable reference. This allows in-place modification without cloning.

### Filtering with Pattern Matching

```nectar
    fn visible_todos(&self) -> [&Todo] {
        match self.filter {
            Filter::All => &self.todos,
            Filter::Active => self.todos.iter().filter(fn(t: &Todo) -> bool { !t.done }),
            Filter::Completed => self.todos.iter().filter(fn(t: &Todo) -> bool { t.done }),
        }
    }
```

- **Pattern matching**: `match` exhaustively handles all `Filter` variants.
- **Borrowing**: `&self` means this is a read-only method. The return type `[&Todo]` returns borrowed references, not copies.
- **Closures**: `fn(t: &Todo) -> bool { !t.done }` is a typed closure used as a filter predicate.

---

## api.nectar -- API Communication

**Concepts**: stores, async actions, HTTP fetch, error handling, computed values

This example shows how to build a data-driven application that communicates with a REST API.

### Data Types

```nectar
struct Post {
    id: u32,
    title: String,
    body: String,
    user_id: u32,
}

struct ApiError {
    status: u32,
    message: String,
}
```

### Store with Async Actions

```nectar
store PostService {
    signal posts: [Post] = [];
    signal loading: bool = false;
    signal error: Option<ApiError> = None;
```

The store uses three signals to track loading state, data, and errors. Any component reading these signals will automatically re-render when they change.

### GET Request

```nectar
    async action fetch_posts(&mut self) {
        self.loading = true;
        self.error = None;

        let response = await fetch("https://jsonplaceholder.typicode.com/posts");

        if response.status == 200 {
            self.posts = response.json();
        } else {
            self.error = Some(ApiError {
                status: response.status,
                message: "Failed to fetch posts",
            });
        }
        self.loading = false;
    }
```

- **`async action`** declares an asynchronous store action.
- **`await fetch(...)`** makes an HTTP GET request and waits for the response.
- **`response.json()`** parses the response body as JSON into the typed `[Post]` array.
- **Error handling** uses `Option<ApiError>` to represent the presence or absence of an error.

### POST Request with Body

```nectar
    async action create_post(&mut self, title: String, body: String) {
        self.loading = true;

        let response = await fetch("https://jsonplaceholder.typicode.com/posts", {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
            },
            body: format("{\"title\": \"{}\", \"body\": \"{}\", \"userId\": 1}", title, body),
        });
```

The second argument to `fetch` is an options object with `method`, `headers`, and `body` fields.

### Computed Values

```nectar
    computed post_count(&self) -> u32 {
        self.posts.len()
    }
```

Computed values are derived from signals and cached. `post_count` automatically updates whenever `self.posts` changes.

### Using the Store from a Component

```nectar
component PostList() {
    render {
        <div>
            {if PostService::get_loading() {
                <div>"Loading..."</div>
            }}

            {for post in PostService::get_posts() {
                <li>
                    <h3>{post.title}</h3>
                    <button on:click={PostService::delete_post(post.id)}>"Delete"</button>
                </li>
            }}

            <p>{format("Total: {} posts", PostService::post_count())}</p>
        </div>
    }
}
```

Components access store state via `StoreName::get_field()` and dispatch actions via `StoreName::action_name(args)`. The reactive system ensures the UI stays in sync.

---

## store.nectar -- Global State Management

**Concepts**: Flux/Redux pattern, multiple stores, auth flow, effects

This example demonstrates more advanced store patterns.

### Auth Store with Multiple States

```nectar
enum AuthStatus {
    LoggedOut,
    Loading,
    LoggedIn(User),
    Error(String),
}

store AuthStore {
    signal status: AuthStatus = AuthStatus::LoggedOut;
    signal token: String = "";
```

The auth status is modeled as an enum with four states. This is more robust than using separate boolean flags.

### Async Login Flow

```nectar
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
```

The login action transitions through `Loading` to either `LoggedIn` or `Error`, and the UI reactively updates at each step.

### Computed Values

```nectar
    computed is_logged_in(&self) -> bool {
        match self.status {
            AuthStatus::LoggedIn(_) => true,
            _ => false,
        }
    }
```

This computed value can be used as a route guard or in conditional rendering. It only recomputes when `self.status` changes.

### Effects (Side Effects)

```nectar
    effect on_auth_change(&self) {
        match self.status {
            AuthStatus::LoggedIn(user) => {
                println(format("User logged in: {}", user.name));
            }
            AuthStatus::Error(msg) => {
                println(format("Auth error: {}", msg));
            }
            _ => {}
        }
    }
```

Effects run automatically whenever their signal dependencies change. They are used for side effects like logging, analytics, or syncing with external systems.

### Multiple Stores

The example also defines a `CounterStore` to show that applications can have multiple independent stores:

```nectar
store CounterStore {
    signal count: i32 = 0;
    signal step: i32 = 1;

    action increment(&mut self) {
        self.count = self.count + self.step;
    }

    computed double_count(&self) -> i32 {
        self.count * 2
    }
}
```

Components can read from and dispatch to any number of stores simultaneously.

---

## app.nectar -- Routed Application with Styles

**Concepts**: router definition, parameterized routes, guards, scoped CSS, Link navigation, programmatic navigation

This is the most architecturally complete example, showing how to build a multi-page application.

### Store for Route Guards

```nectar
store AuthStore {
    signal is_logged_in: bool = false;
    signal username: String = "";

    action login(&mut self, user: String) {
        self.is_logged_in = true;
        self.username = user;
    }
}
```

### Scoped Styles

Each component declares its own CSS that is automatically scoped:

```nectar
component NavBar() {
    style {
        .navbar {
            display: "flex";
            gap: "16px";
            padding: "12px 24px";
            background: "#1e293b";
            color: "white";
        }
        .navbar a {
            color: "#93c5fd";
            text-decoration: "none";
        }
    }

    render {
        <nav class="navbar">
            <Link to="/">"Home"</Link>
            <Link to="/about">"About"</Link>
        </nav>
    }
}
```

Key style features:
- Styles are declared inside `style { ... }` blocks within the component
- CSS properties are written as `property: "value";` pairs
- Selectors can be nested (`.navbar a`)
- All styles are automatically scoped so they never affect other components
- The runtime generates unique scope attributes and prefixes selectors

### Link Navigation

`<Link to="/path">` creates client-side navigation links that update the URL and mount the corresponding component without a full page reload:

```nectar
<Link to="/">"Home"</Link>
<Link to="/about">"About"</Link>
<Link to="/user/42">"Profile"</Link>
```

### Parameterized Routes

Components can receive route parameters as props:

```nectar
component UserProfile(id: String) {
    signal user_name: String = "Loading...";

    render {
        <div class="profile">
            <h2>{self.user_name}</h2>
            <span>{format("User ID: {}", self.id)}</span>
        </div>
    }
}
```

The `id` parameter is extracted from the URL pattern `/user/:id`.

### Programmatic Navigation

Components can navigate programmatically using the `navigate()` function:

```nectar
component NotFound() {
    fn go_home(&self) {
        navigate("/");
    }

    render {
        <div>
            <h1>"404"</h1>
            <button on:click={self.go_home}>"Go Home"</button>
        </div>
    }
}
```

### Router Definition

The router maps URL patterns to components:

```nectar
router AppRouter {
    route "/" => Home,
    route "/about" => About,
    route "/user/:id" => UserProfile,
    route "/admin/*" => AdminPanel guard { AuthStore::is_logged_in() },
    fallback => NotFound,
}
```

Key routing features:
- **Static routes**: `"/"`, `"/about"` -- exact matches
- **Parameterized routes**: `"/user/:id"` -- captures `id` from the URL
- **Wildcard routes**: `"/admin/*"` -- matches any sub-path under `/admin/`
- **Guards**: `guard { AuthStore::is_logged_in() }` -- the route is only accessible when the guard expression evaluates to `true`
- **Fallback**: `fallback => NotFound` -- rendered when no route matches (404 page)

---

## ai-chat.nectar -- AI Chat Interface

**Concepts**: agents, system prompts, tool definitions, streaming, reactive UI

This example demonstrates Nectar's first-class AI interaction primitives.

### Agent Declaration

```nectar
agent ChatBot {
    prompt system = "You are a helpful coding assistant.";

    signal messages: [Message] = [];
    signal input: String = "";
    signal streaming: bool = false;
```

The `agent` keyword defines a special component type that wraps LLM interaction. It combines:
- A system prompt
- Reactive state (signals)
- Tool definitions
- Methods
- A render block

### Tool Definitions

```nectar
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

    tool get_weather(city: String) -> String {
        let result = await fetch(format("https://api.example.com/weather?city={}", city));
        return result.json().forecast;
    }
```

Tools are functions that the AI model can call during a conversation. They have:
- **Typed parameters** (used to generate JSON schemas for the AI)
- **Return types** (the result is fed back to the AI)
- **Async bodies** that can make HTTP requests or perform computation

When the AI decides to call a tool, the runtime:
1. Parses the tool call from the streaming response
2. Dispatches to the corresponding WASM-exported function
3. Sends the result back to the AI for continued reasoning

### Streaming Chat

```nectar
    fn send(&mut self) {
        let msg = Message { role: "user", content: self.input };
        self.messages.push(msg);
        self.input = "";
        self.streaming = true;

        ai::chat_stream(self.messages, self.tools);
    }
```

`ai::chat_stream` initiates a streaming completion. Tokens arrive one at a time, and the UI updates reactively:

```nectar
    fn on_stream_token(&mut self, token: String) {
        let last = self.messages.len() - 1;
        if self.messages[last].role == "assistant" {
            self.messages[last].content = self.messages[last].content + token;
        } else {
            self.messages.push(Message { role: "assistant", content: token });
        }
    }
```

Each incoming token triggers a signal update, which triggers a DOM update, giving the user a real-time streaming experience.

### Reactive Chat UI

```nectar
    render {
        <div class="chat">
            <div class="messages">
                {for msg in self.messages {
                    <div class={msg.role}>
                        <span class="role-label">{msg.role}</span>
                        <div class="content">{msg.content}</div>
                    </div>
                }}
                {if self.streaming {
                    <div class="typing">
                        <span class="dot">"."</span>
                        <span class="dot">"."</span>
                        <span class="dot">"."</span>
                    </div>
                }}
            </div>
            <div class="input-area">
                <input value={self.input} placeholder="Ask me anything..." on:submit={self.send} />
                <button on:click={self.clear_history}>"Clear"</button>
            </div>
        </div>
    }
```

The template demonstrates:
- **List rendering** with `for msg in self.messages`
- **Dynamic classes** with `class={msg.role}`
- **Conditional rendering** with `if self.streaming`
- **Event binding** on both the input (`on:submit`) and button (`on:click`)

The entire chat interface is reactive. When a new message is added or a streaming token appends content, only the affected DOM nodes update.

---

## tests.nectar -- Comprehensive Test Patterns

**Concepts**: test blocks, assertions, testing functions, structs, enums, pattern matching, ownership, async, error handling, computed values, closures

This file demonstrates every major testing pattern in Nectar. Tests are defined with `test "name" { ... }` blocks and use `assert()` and `assert_eq()` for verification.

### Basic Assertions

```nectar
test "assert with boolean condition" {
    assert(true);
    assert(1 + 1 == 2);
    assert(10 > 5);
}

test "assert_eq with custom message" {
    let result = fibonacci(6);
    assert_eq(result, 8, "6th fibonacci number should be 8");
}
```

- **`assert(condition)`** verifies a boolean expression is true.
- **`assert_eq(left, right)`** verifies two values are equal.
- Both accept an optional trailing message string for better failure diagnostics.

### Testing Functions (Pure Logic)

```nectar
test "fibonacci sequence" {
    assert_eq(fibonacci(0), 0);
    assert_eq(fibonacci(1), 1);
    assert_eq(fibonacci(10), 55);
}
```

Pure functions are the simplest to test. Call the function, check the return value. No setup or teardown needed.

### Testing Structs and Enums

```nectar
test "struct methods" {
    let origin = Point::new(0.0, 0.0);
    let target = Point::new(3.0, 4.0);
    let dist = origin.distance(&target);
    assert_eq(dist, 5.0);
}

test "enum method — shape areas" {
    let circle = Shape::Circle(1.0);
    assert(circle.area() > 3.14);
    assert(circle.area() < 3.15);
}
```

Construct instances, call methods, and verify results. Enum tests should cover each variant.

### Testing Pattern Matching

```nectar
test "match on enum variants" {
    let shape = Shape::Circle(2.0);
    let label = match shape {
        Shape::Circle(r) => format("circle with radius {}", r),
        Shape::Rectangle(w, h) => format("{}x{} rectangle", w, h),
        Shape::Triangle(_, _, _) => "triangle",
    };
    assert_eq(label, "circle with radius 2");
}
```

Verify that `match` arms bind variables correctly and that wildcard patterns work as expected.

### Testing Ownership and Borrowing

```nectar
test "borrowing preserves original" {
    let data = [1, 2, 3];
    let borrowed = &data;
    assert_eq(borrowed.len(), 3);
    assert_eq(data.len(), 3);  // still accessible
}

test "mutable borrow allows modification" {
    let mut items: [i32] = [1, 2, 3];
    let borrowed = &mut items;
    borrowed.push(4);
    assert_eq(items.len(), 4);
}
```

Tests can verify that ownership moves and borrows behave correctly at runtime. Immutable borrows preserve access to the original; mutable borrows allow modification.

### Testing Async Operations

```nectar
test "async fetch returns response" {
    let response = await fetch("https://api.example.com/users/1");
    assert(response.status == 200 || response.status == 0);
}
```

The test runner stubs HTTP imports, so `await fetch(...)` resolves immediately. This verifies that async/await syntax works in test blocks without hitting real endpoints.

### Testing Error Handling

```nectar
test "try/catch captures error" {
    let result = try {
        let val = divide(10.0, 0.0);
        match val {
            Result::Ok(v) => v,
            Result::Err(e) => { throw e; }
        }
    } catch err {
        -1.0
    };
    assert_eq(result, -1.0);
}
```

Use `try { ... } catch err { ... }` to verify that error paths produce the expected fallback values.

### Testing Computed Values (Stores)

```nectar
test "store computed values reflect state" {
    assert_eq(TestCounterStore::double_count(), 0);
    TestCounterStore::increment();
    assert_eq(TestCounterStore::double_count(), 2);
    assert(TestCounterStore::is_positive());
}
```

Store signals and computed values can be tested by dispatching actions and checking the derived state.

### Testing Closures

```nectar
test "closure captures value" {
    let multiplier = 3;
    let triple = fn(x: i32) -> i32 { x * multiplier };
    assert_eq(triple(5), 15);
}

test "closure as filter predicate" {
    let numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let evens = numbers.iter().filter(fn(n: &i32) -> bool { n % 2 == 0 });
    assert_eq(evens.len(), 5);
}
```

Closures capture variables from their surrounding scope. They work naturally as arguments to higher-order functions like `filter`, `map`, and `sort`.

---

## component-tests.nectar -- Component Testing Patterns

**Concepts**: test renderer, mounting, click simulation, props, signals, conditional rendering, list rendering, event handlers, store integration

This file uses Nectar's test renderer to mount components into a virtual DOM and verify their behavior without a browser.

### Mounting and Checking Initial Render

```nectar
test "greeting renders with default prop" {
    let el = render(<Greeting />);
    let heading = el.findByText("Hello, World!");
    assert(heading.exists());
}

test "counter renders initial value" {
    let el = render(<Counter />);
    let display = el.findByRole("counter");
    assert_eq(display.getText(), "0");
}
```

`render(<Component />)` returns a `TestElement` that provides query methods:

- **`findByText(text)`** -- find a descendant containing the given text
- **`findByRole(role)`** -- find by ARIA role attribute
- **`findByAttribute(name, value)`** -- find by any attribute

### Simulating Clicks and Verifying State Changes

```nectar
test "counter increments on click" {
    let el = render(<Counter />);
    let inc_btn = el.findByText("+1");
    let display = el.findByRole("counter");

    inc_btn.click();
    assert_eq(display.getText(), "1");
}
```

Call `.click()` on a `TestElement` to dispatch a click event. The component's reactive state updates, and subsequent queries reflect the new DOM.

### Testing Props with Default Values

```nectar
test "default props are applied" {
    let el = render(<Greeting />);
    let heading = el.findByText("Hello, World!");
    assert(heading.exists());
}

test "explicit props override defaults" {
    let el = render(<Counter initial={100} />);
    let display = el.findByRole("counter");
    assert_eq(display.getText(), "100");
}
```

Components with `prop_name: Type = default` apply the default when no value is passed. Explicit values override the default.

### Testing Signal Updates and DOM Reactivity

```nectar
test "theme toggle updates displayed text" {
    let el = render(<ThemeToggle />);
    let status = el.findByRole("status");
    assert_eq(status.getText(), "Light Mode");

    let toggle_btn = el.findByText("Toggle Theme");
    toggle_btn.click();
    assert_eq(status.getText(), "Dark Mode");
}
```

After a click triggers a signal update, all DOM queries return the updated content. No manual "flush" or "tick" is needed -- the test renderer processes updates synchronously.

### Testing Conditional Rendering

```nectar
test "conditional rendering shows message when true" {
    let el = render(<ConditionalMessage show={true} />);
    let message = el.findByText("This message is visible");
    assert(message.exists());
}
```

Conditional `{if ... { ... }}` blocks in templates are tested by mounting with different props and verifying which elements are present.

### Testing List Rendering

```nectar
test "number list renders all items" {
    let el = render(<NumberList items={[10, 20, 30]} />);
    assert(el.findByText("10").exists());
    assert(el.findByText("20").exists());
    assert(el.findByText("30").exists());
}
```

`{for item in collection { ... }}` loops produce one element per item. Verify each rendered item exists in the virtual DOM.

### Testing Event Handlers

```nectar
test "todo list add button creates item" {
    let el = render(<TodoList />);
    let input = el.findByAttribute("placeholder", "What needs to be done?");
    let add_btn = el.findByText("Add");

    input.type("Buy groceries");
    add_btn.click();

    let todo = el.findByText("Buy groceries");
    assert(todo.exists());
}
```

Use `.type(text)` to simulate text input and `.click()` to trigger buttons. Then query the DOM to verify the handler's effect.

### Testing Store Integration from Components

```nectar
test "store counter increment updates display" {
    let el = render(<StoreCounter />);
    let inc_btn = el.findByText("+1");
    let display = el.findByRole("display");

    inc_btn.click();
    assert_eq(display.getText(), "Store count: 1");
}
```

Components that read from stores via `StoreName::get_field()` and dispatch via `StoreName::action()` can be tested end-to-end. The test renderer processes store updates synchronously.

---

## agent-tests.nectar -- Agent Testing Patterns

**Concepts**: tool registration, tool dispatch, message history, AI response mocking, streaming

This file tests AI agents -- their tool definitions, message management, and mocked AI interactions.

### Testing Tool Registration

```nectar
test "agent registers expected tools" {
    let tools = TestAssistant::get_registered_tools();
    assert_eq(tools.len(), 3);
    assert_eq(tools[0].name, "search_docs");
    assert_eq(tools[1].name, "calculate");
    assert_eq(tools[2].name, "get_weather");
}
```

`AgentName::get_registered_tools()` returns metadata about all `tool` blocks defined in the agent, including parameter names, types, and return types.

### Testing Tool Dispatch with Typed Args

```nectar
test "dispatch search_docs tool" {
    TestAssistant::clear_history();
    let result = await TestAssistant::dispatch_tool("search_docs", {
        query: "nectar language tutorial",
    });
    let log = TestAssistant::get_tool_call_log();
    assert_eq(log[0], "search_docs: arc language tutorial");
}
```

`dispatch_tool(name, args)` invokes a tool by name with a typed argument object. This simulates what the runtime does when the AI model requests a tool call.

### Testing Message History Management

```nectar
test "messages preserve role and content" {
    TestAssistant::clear_history();
    TestAssistant::add_user_message("What is Nectar?");
    TestAssistant::add_assistant_message("Nectar is a programming language.");

    let messages = TestAssistant::get_messages();
    assert_eq(messages[0].role, "user");
    assert_eq(messages[1].role, "assistant");
}
```

Agent state (messages, tool call logs) is managed through methods defined in the agent. Tests verify the history tracks roles, ordering, and content correctly.

### Mocking AI Responses

```nectar
test "mock chat_complete returns canned response" {
    TestAssistant::clear_history();
    TestAssistant::add_user_message("What is 2+2?");
    ai::mock_response("The answer is 4.");

    let response = await ai::chat_complete(TestAssistant::get_messages());
    assert_eq(response.content, "The answer is 4.");
}
```

The `ai::mock_response()` function installs a canned response that `ai::chat_complete` returns instead of calling a real LLM. Use `ai::mock_tool_call()` to simulate the AI requesting a tool, and `ai::mock_stream()` to test token-by-token streaming.

---

## Running the Examples

All examples can be compiled from the repository root:

```sh
# Compile to WAT (human-readable)
nectar build examples/hello.nectar

# Compile to binary WASM
nectar build examples/counter.nectar --emit-wasm

# Compile with optimizations
nectar build examples/app.nectar --emit-wasm -O2

# Start the dev server for interactive development
nectar dev --src examples --port 3000
```

For the AI chat example, you will need an LLM API endpoint at `/api/chat` that accepts OpenAI-compatible requests. The runtime handles the streaming protocol automatically.

### Running Tests

```sh
# Run all tests in a file
nectar test examples/tests.nectar

# Run with verbose output
nectar test examples/tests.nectar --verbose

# Filter tests by name
nectar test examples/tests.nectar --filter "fibonacci"
nectar test examples/component-tests.nectar --filter "counter"
nectar test examples/agent-tests.nectar --filter "tool"

# Run all test files at once
nectar test examples/tests.nectar examples/component-tests.nectar examples/agent-tests.nectar
```
