# Arc Architecture & Internals

This document describes the internal architecture of the Nectar compiler and runtime for contributors who want to understand, modify, or extend the system.

---

## Table of Contents

1. [Compiler Pipeline Overview](#compiler-pipeline-overview)
2. [Lexer](#lexer)
3. [Parser](#parser)
4. [Borrow Checker](#borrow-checker)
5. [Type Checker](#type-checker)
6. [Optimizer](#optimizer)
7. [Code Generation -- WAT](#code-generation----wat)
8. [Code Generation -- Binary WASM](#code-generation----binary-wasm)
9. [Runtime Bridge](#runtime-bridge)
10. [Server-Side Rendering (SSR)](#server-side-rendering-ssr)
11. [Module Resolution](#module-resolution)
12. [Source Map](#source-map)

---

## Compiler Pipeline Overview

Nectar's compiler is a traditional multi-pass compiler written in Rust. Source code flows through the following stages:

```
                    Arc Source (.nectar)
                         |
                    +----v----+
                    |  Lexer  |  tokenize()
                    +----+----+
                         |
                   Token Stream
                         |
                    +----v----+
                    | Parser  |  parse_program_recovering()
                    +----+----+
                         |
                   AST (Program)
                         |
              +----------+----------+
              |                     |
       +------v------+    +--------v--------+
       |   Module    |    | Borrow Checker  |
       |   Loader    |    +---------+-------+
       +------+------+             |
              |               +----v----+
              +------+------->|  Type   |
                     |        | Checker |
                     |        +----+----+
                     |             |
               +-----v-----+      |
               | Exhaustive |<-----+
               |   Check    |
               +-----+------+
                     |
               +-----v------+
               | Optimizer   |
               | (O0/O1/O2) |
               +-----+------+
                     |
           +---------+---------+
           |         |         |
     +-----v--+ +---v----+ +--v-----+
     |  WAT   | | Binary | |  SSR   |
     | Codegen| |  WASM  | | Codegen|
     +--------+ +--------+ +--------+
           |         |         |
        .wat      .wasm     .ssr.js
```

All compiler modules live in `compiler/src/`:

| Module | File | Purpose |
|---|---|---|
| Lexer | `lexer.rs` | Tokenization |
| Token definitions | `token.rs` | Token types, spans |
| AST definitions | `ast.rs` | All AST node types |
| Parser | `parser.rs` | Recursive descent + Pratt parsing |
| Borrow checker | `borrow_checker.rs` | Ownership and lifetime validation |
| Type checker | `type_checker.rs` | Hindley-Milner type inference |
| Exhaustiveness | `exhaustiveness.rs` | Match pattern coverage checking |
| Optimizer | `optimizer.rs` | Pass manager |
| Constant folding | `const_fold.rs` | Compile-time expression evaluation |
| Dead code elimination | `dce.rs` | Remove unreachable statements |
| Tree shaking | `tree_shake.rs` | Remove unused top-level items |
| WASM optimization | `wasm_opt.rs` | Peephole optimization on WAT output |
| WAT codegen | `codegen.rs` | AST to WebAssembly Text Format |
| Binary WASM codegen | `wasm_binary.rs` | AST to binary .wasm |
| SSR codegen | `ssr.rs` | AST to server-side JS |
| Module resolver | `module_resolver.rs` | File path resolution for modules |
| Module loader | `module_loader.rs` | Multi-file compilation |
| Package manager | `package.rs` | Nectar.toml parsing, lockfile |
| Registry client | `registry.rs` | Dependency download |
| Dependency resolver | `resolver.rs` | Version resolution |
| Formatter | `formatter.rs` | Canonical source formatting |
| Linter | `linter.rs` | Static analysis rules |
| LSP server | `lsp.rs` | Language Server Protocol |
| Dev server | `devserver.rs` | HTTP + WebSocket hot-reload server |
| Source maps | `sourcemap.rs` | Debug mapping generation |
| Standard library | `stdlib.rs` | Built-in types and functions |
| CLI entry point | `main.rs` | Command-line interface |

---

## Lexer

**File**: `compiler/src/lexer.rs`

The lexer converts raw source text into a stream of tokens. It is implemented as a hand-written scanner that processes one character at a time.

### Token Types

Tokens are defined in `token.rs` as the `TokenKind` enum. Major categories:

- **Literals**: `Integer(i64)`, `Float(f64)`, `StringLit(String)`, `Bool(bool)`, `FormatString(Vec<FormatStringPart>)`
- **Identifiers**: `Ident(String)`
- **Keywords**: 50+ reserved words (`let`, `fn`, `component`, `store`, `agent`, `match`, etc.)
- **Type keywords**: `I32`, `I64`, `F32`, `F64`, `U32`, `U64`, `Bool_`, `StringType`
- **Symbols**: all operators and punctuation (`+`, `-`, `->`, `=>`, `::`, etc.)
- **Lifetimes**: `Lifetime(String)` for `'a`, `'static`, etc.
- **Special**: `Eof`

### Format Strings

The lexer handles `f"..."` format strings specially. It splits the content into alternating literal and expression segments:

```
f"Hello {name}, you are {age} years old"
```

Produces:

```
FormatString([
    Lit("Hello "),
    Expr("name"),
    Lit(", you are "),
    Expr("age"),
    Lit(" years old"),
])
```

### Spans

Every token carries a `Span` with byte offsets and line/column positions for error reporting:

```rust
pub struct Span {
    pub start: usize,  // byte offset
    pub end: usize,
    pub line: u32,
    pub col: u32,
}
```

---

## Parser

**File**: `compiler/src/parser.rs`

The parser is a hand-written recursive descent parser with Pratt parsing for expressions. It produces a typed AST defined in `ast.rs`.

### Error Recovery

The parser implements error recovery so that multiple errors can be reported in a single compilation. When a parse error occurs:

1. The error is recorded in `self.errors`
2. The parser calls `synchronize()` to skip tokens until a recovery point
3. Parsing continues from the next valid position

Three synchronization contexts are used:

- **TopLevel**: skip to the next top-level keyword (`fn`, `component`, `struct`, etc.)
- **Statement**: skip to the next semicolon or statement-starting keyword
- **Block**: skip to the matching closing brace (counting nesting)

The public API returns both the (partial) AST and collected errors:

```rust
pub fn parse_program_recovering(&mut self) -> (Program, Vec<ParseError>)
```

### Top-Level Items

The parser dispatches on the first token to determine the item type:

| First Token(s) | Parsed As |
|---|---|
| `fn` | `Item::Function` |
| `async fn` | `Item::Function` (async) |
| `component` | `Item::Component` |
| `struct` | `Item::Struct` |
| `enum` | `Item::Enum` |
| `impl` | `Item::Impl` |
| `trait` | `Item::Trait` |
| `use` | `Item::Use` |
| `mod` | `Item::Mod` |
| `store` | `Item::Store` |
| `agent` | `Item::Agent` |
| `router` | `Item::Router` |
| `lazy component` | `Item::LazyComponent` |
| `test` | `Item::Test` |
| `pub` (prefix) | Sets `is_pub` on the following item |

### Expression Parsing (Pratt Parser)

Expressions are parsed using a Pratt (top-down operator precedence) parser. The precedence levels from lowest to highest:

| Level | Operations | Method |
|---|---|---|
| 1 | Assignment (`=`, `+=`, `-=`, `*=`, `/=`) | `parse_assignment()` |
| 2 | Logical OR (`\|\|`) | `parse_or()` |
| 3 | Logical AND (`&&`) | `parse_and()` |
| 4 | Equality (`==`, `!=`) | `parse_equality()` |
| 5 | Comparison (`<`, `>`, `<=`, `>=`) | `parse_comparison()` |
| 6 | Additive (`+`, `-`) | `parse_additive()` |
| 7 | Multiplicative (`*`, `/`, `%`) | `parse_multiplicative()` |
| 8 | Unary (`-`, `!`, `&`, `&mut`) | `parse_unary()` |
| 9 | Postfix (`.`, `()`, `[]`, `?`) | `parse_postfix()` |
| 10 | Primary (literals, idents, control flow) | `parse_primary()` |

### Template Parsing

Template nodes are parsed in `parse_template_node()`. The parser handles:

- **Elements**: `<tag attr="val">children</tag>` and self-closing `<tag />`
- **Text literals**: `"string content"`
- **Expressions**: `{expr}` embedded in templates
- **Link elements**: `<Link to="/path">text</Link>` special handling
- **Attributes**: static, dynamic, event handlers (`on:event`), ARIA, role, bind

Mismatched closing tags are detected and reported as errors.

### Component Body Parsing

Inside a component body, the parser expects one of:

- `let` / `let mut` -- state field
- `signal` -- reactive signal field
- `fn` -- method
- `style` -- scoped CSS block
- `transition` -- CSS transition declarations
- `render` -- template render block
- `error_boundary` -- error handling with fallback UI

### Store Body Parsing

Inside a store body:

- `signal` -- reactive state field
- `action` / `async action` -- mutation methods
- `computed` -- derived values
- `effect` -- side-effect callbacks

---

## Borrow Checker

**File**: `compiler/src/borrow_checker.rs`

Arc implements a simplified version of Rust's borrow checker to catch memory safety issues at compile time.

### Ownership Model

Every value has a single owner. When a value is assigned to a new binding, ownership is **moved** and the original binding becomes invalid:

```nectar
let a = create_data();
let b = a;          // ownership moves to b
// a is no longer valid
```

### Borrow Rules

The borrow checker enforces these rules:

1. **At any time, you can have either** one mutable reference (`&mut`) **or** any number of immutable references (`&`), but not both simultaneously.
2. **References must always be valid** -- a reference cannot outlive the data it points to.
3. **No use after move** -- once a value is moved, the original binding cannot be used.

### Error Types

| Error Kind | Description |
|---|---|
| `UseAfterMove` | Value used after being moved to another binding |
| `DoubleMutBorrow` | Two simultaneous mutable borrows of the same value |
| `MutBorrowWhileImmBorrowed` | Mutable borrow while immutable borrows exist |
| `ImmBorrowWhileMutBorrowed` | Immutable borrow while a mutable borrow exists |
| `BorrowOutlivesScope` | Reference outlives the scope of the borrowed value |
| `AssignWhileBorrowed` | Assignment to a variable that is currently borrowed |
| `LifetimeViolation` | Named lifetime constraint is violated |
| `MissingLifetimeAnnotation` | Lifetime annotation required but not provided |

### Implementation

The checker maintains a `VarState` for each variable binding:

- **Owned** -- the variable owns its value, not borrowed
- **Moved** -- the value has been moved elsewhere
- **Borrowed { count }** -- immutably borrowed `count` times
- **MutBorrowed** -- mutably borrowed

It walks the AST, updating states as it encounters let bindings, assignments, borrows, and function calls. Scope boundaries trigger cleanup of borrows created within that scope.

### Lifetime Validation

When functions have lifetime parameters (e.g., `<'a>`), the checker validates that:

- Returned references have a lifetime that matches or outlives the function's declared lifetime
- References stored in data structures have appropriate lifetime annotations
- Lifetime elision rules are applied correctly for common patterns

---

## Type Checker

**File**: `compiler/src/type_checker.rs`

Nectar uses Hindley-Milner type inference with unification.

### Internal Type Representation

Types during inference are represented as `Ty`:

```rust
enum Ty {
    Var(TypeId),        // unresolved type variable
    I32, I64, U32, U64, F32, F64, Bool, String_, Unit,
    Array(Box<Ty>),
    Option_(Box<Ty>),
    Tuple(Vec<Ty>),
    Function { params: Vec<Ty>, ret: Box<Ty> },
    Reference { mutable: bool, lifetime: Option<String>, inner: Box<Ty> },
    Struct(String),
    Enum(String),
    Iterator(Box<Ty>),
    Result_ { ok: Box<Ty>, err: Box<Ty> },
    TypeParam(String),
    SelfType,
    Error,              // sentinel to prevent cascading errors
}
```

### Inference Algorithm

1. **Fresh type variables** are created for each expression and binding without an explicit type annotation.
2. **Constraint generation** walks the AST, generating equality constraints between types (e.g., the condition in an `if` must be `Bool`, both branches must have the same type).
3. **Unification** solves constraints by substituting type variables with concrete types. When `Ty::Var(a)` must equal `Ty::I32`, the substitution `a -> I32` is recorded.
4. **Occurs check** prevents infinite types (e.g., `T = Array<T>`).

### Substitution Table

A substitution table maps `TypeId` to `Ty`. Looking up a type variable follows the chain until a concrete type or an unresolved variable is found.

### Type Checking Phases

1. **Collect definitions** -- register all structs, enums, traits, and function signatures
2. **Check item bodies** -- infer types within function bodies, component methods, store actions, etc.
3. **Verify trait implementations** -- ensure all required methods are implemented with matching signatures
4. **Report errors** -- unresolved type variables or unsatisfied constraints produce error messages

### Exhaustiveness Checking

**File**: `compiler/src/exhaustiveness.rs`

After type checking, a separate pass checks that `match` expressions cover all possible patterns. Non-exhaustive matches produce warnings (not errors) to avoid blocking compilation.

---

## Optimizer

**File**: `compiler/src/optimizer.rs`

The optimizer runs after type checking and before code generation. It operates on the AST.

### Optimization Levels

| Level | Passes |
|---|---|
| O0 | None |
| O1 | Constant folding + Dead code elimination |
| O2 | O1 + Tree shaking + WASM-level peephole optimization |

### Constant Folding

**File**: `compiler/src/const_fold.rs`

Evaluates compile-time constant expressions:

```nectar
// Before optimization
let x = 2 + 3 * 4;

// After constant folding
let x = 14;
```

Handles arithmetic on integer and float literals, boolean logic, and string operations.

### Dead Code Elimination (DCE)

**File**: `compiler/src/dce.rs`

Removes unreachable code:

- Statements after `return` in the same block
- Branches of `if` where the condition is a constant `true`/`false`
- Unused variable bindings (when the initializer has no side effects)

### Tree Shaking

**File**: `compiler/src/tree_shake.rs`

Removes unused top-level items. Starting from entry points (components with `render` blocks, exported functions), it traces which functions, structs, enums, and stores are reachable. Unreachable items are removed from the AST.

### WASM-Level Optimization

**File**: `compiler/src/wasm_opt.rs`

At `-O2`, a post-codegen pass performs peephole optimizations on the emitted WAT:

- Redundant `local.get`/`local.set` pairs
- Constant instruction sequences that can be simplified
- Dead code within WASM function bodies

Statistics are reported:

```
arc: wasm optimization: 12 patterns optimized, 340 bytes saved
```

---

## Code Generation -- WAT

**File**: `compiler/src/codegen.rs`

The primary code generator emits WebAssembly Text Format (WAT). WAT is human-readable and can be converted to binary WASM by external tools or Nectar's built-in binary emitter.

### Architecture

`WasmCodegen` maintains:

- **Output buffer** -- the accumulated WAT string
- **Locals table** -- local variables in the current function scope
- **String interning table** -- deduplicated string constants with their memory offsets
- **Closure counter** -- for generating unique closure function names
- **Function table** -- for indirect calls (closures, event handlers)

### Import Generation

The codegen emits imports for all runtime modules:

```wat
(import "env" "memory" (memory 1))
(import "dom" "createElement" (func $dom_createElement (param i32 i32) (result i32)))
(import "signal" "create" (func $signal_create (param i32) (result i32)))
(import "http" "fetch" (func $http_fetch (param i32 i32 i32 i32) (result i32)))
;; ... and many more
```

### Component Compilation

For each component, the codegen produces:

1. A **mount function** (`ComponentName_mount`) that creates DOM elements, sets up signals, registers event handlers, and builds the initial DOM tree
2. **Handler functions** (`__handler_N`) for each event handler
3. **Effect functions** (`__effect_N`) for reactive signal subscriptions
4. An **init function** (`ComponentName_init`) for store initialization

### Signal Compilation

Signal state fields are compiled to WASM globals backed by runtime signal objects:

1. `signal.create(initialValue)` creates the signal at initialization
2. `signal.get(id)` reads the value (with automatic dependency tracking)
3. `signal.set(id, newValue)` updates the value (triggering re-renders)
4. `signal.subscribe(id, effectFnIdx)` registers an effect that re-runs when the signal changes

### String Interning

String literals are stored in the WASM linear memory data section. The codegen assigns each unique string a memory offset and emits `(data ...)` segments. At runtime, strings are referenced by `(ptr, len)` pairs.

### Closure Compilation

Closures are compiled to standalone WASM functions with captured variables passed as parameters. They are registered in the function table for indirect calling.

---

## Code Generation -- Binary WASM

**File**: `compiler/src/wasm_binary.rs`

The binary emitter produces valid `.wasm` files directly, without going through WAT text.

### Binary Format

The emitter writes the standard WebAssembly binary format:

1. **Magic number**: `\0asm` (4 bytes)
2. **Version**: 1 (4 bytes)
3. **Sections** in order:
   - Type section (function signatures)
   - Import section (runtime imports)
   - Function section (type indices for each function)
   - Memory section
   - Global section
   - Export section (mounted functions, handlers)
   - Code section (function bodies)
   - Data section (interned strings)

### LEB128 Encoding

All integer values in the binary format use LEB128 (Little-Endian Base 128) variable-length encoding, which the emitter handles in dedicated `write_leb128_u32` and `write_leb128_i32` methods.

### Function Bodies

Function bodies are compiled to WASM bytecode opcodes:

| Opcode | Hex | Description |
|---|---|---|
| `unreachable` | `0x00` | Trap |
| `nop` | `0x01` | No operation |
| `block` | `0x02` | Begin block |
| `loop` | `0x03` | Begin loop |
| `if` | `0x04` | Conditional |
| `else` | `0x05` | Else branch |
| `end` | `0x0B` | End block/if/loop |
| `br` | `0x0C` | Branch |
| `br_if` | `0x0D` | Conditional branch |
| `return` | `0x0F` | Return from function |
| `local.get` | `0x20` | Get local variable |
| `local.set` | `0x21` | Set local variable |
| `i32.const` | `0x41` | Push i32 constant |
| `i32.add` | `0x6A` | Integer addition |
| ... | ... | (full WASM instruction set) |

---

## Runtime Bridge

**File**: `runtime/nectar-runtime.js`

The JavaScript runtime provides host functions that WASM modules import. It bridges the gap between WASM's linear memory model and browser APIs.

### Initialization Flow

1. `NectarRuntime.mount(wasmUrl, rootElement)` is called with the `.wasm` URL and a root DOM element
2. The runtime creates a `WebAssembly.Memory` and builds the import object with all host function modules
3. The WASM module is instantiated with the import object
4. Store `*_init` exports are called to initialize global stores
5. The first `*_mount` export is called to render the initial component

### Signal System

The runtime implements a pull-based reactivity system:

- **Signals** are observable values with `get()` and `set()` methods
- **Effects** are functions that automatically re-run when their signal dependencies change
- **Dependency tracking** works via a global `currentEffect` variable: when an effect runs, any `signal.get()` calls automatically register the effect as a subscriber
- **Scheduling** uses `requestAnimationFrame` to batch multiple signal updates into a single DOM update pass
- **Batching** groups multiple signal updates so effects only fire once after the batch completes

### Worker Pool

For concurrency (`spawn`, `parallel`), the runtime maintains a pool of Web Workers:

- Each worker loads a copy of the WASM module
- Tasks are distributed to available workers
- Results are communicated back via `postMessage`
- The pool size defaults to 4 workers

### Agent Manager

The `AgentManager` class coordinates AI agent interactions:

- Maintains a tool registry mapping tool names to WASM function indices
- Manages message history per agent
- Dispatches tool calls from the AI to WASM-exported functions
- Handles streaming responses

### Router

The client-side router:

- Parses route patterns into regex with named capture groups
- Listens to `popstate` events for browser back/forward
- Uses `history.pushState` for programmatic navigation
- Supports guards (auth checks) before mounting routes
- Manages component mounting/unmounting when routes change

---

## Server-Side Rendering (SSR)

**File**: `compiler/src/ssr.rs`

The SSR codegen emits a JavaScript module that can render components to HTML strings on the server.

### How It Works

1. Components are compiled to JavaScript functions that build HTML strings
2. Signal reads return initial values (no reactivity on the server)
3. Event handlers are omitted (they are only needed on the client)
4. The output is a complete HTML fragment ready to be embedded in a page

### Hydration

The hydration bundle (`--hydrate`) generates a lightweight client module that:

1. Walks the existing server-rendered DOM
2. Attaches event handlers to existing elements (instead of recreating them)
3. Sets up reactive signal subscriptions
4. Takes over from the static HTML seamlessly

---

## Module Resolution

**Files**: `compiler/src/module_resolver.rs`, `compiler/src/module_loader.rs`

### File Lookup

When the parser encounters `mod foo;`, the module loader searches for:

1. `./foo.nectar` (sibling file)
2. `./foo/mod.nectar` (directory module)

relative to the file containing the `mod` declaration.

### Multi-File Compilation

The `ModuleLoader` performs multi-file compilation:

1. Scans the root AST for `mod` declarations
2. Recursively loads and parses each external module
3. Merges all module ASTs into a single `Program`
4. The merged program is then borrow-checked, type-checked, and compiled as a unit

### Dependency Graph

For packages with `Nectar.toml`, the resolver builds a dependency graph:

1. Parse `Nectar.toml` to get declared dependencies
2. Resolve version constraints against the registry
3. Download packages to the local cache (`~/.nectar/cache/`)
4. Write `Nectar.lock` with pinned versions
5. Make dependency source files available for module loading

---

## Source Map

**File**: `compiler/src/sourcemap.rs`

The source map module generates debug mapping information that connects WASM bytecode positions back to Nectar source locations. This enables:

- Browser DevTools to show Nectar source when debugging WASM
- Error stack traces with Nectar file names and line numbers
- Breakpoint setting in Nectar source files
