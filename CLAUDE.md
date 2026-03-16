# Nectar — Claude Code Instructions

## Core Thesis

Nectar exists to prove that the web does not need JavaScript. It is a compiled-to-WASM language where **all logic, all computation, all state management, all rendering decisions run in Rust/WASM**. JavaScript is treated as a thin, unavoidable syscall layer — an impedance mismatch we minimize, not a tool we reach for.

**NEVER reach for JavaScript.** When implementing any feature, the answer is Rust/WASM. If you think something needs JS, you are almost certainly wrong. The only valid reason for JS is a browser API that WASM physically cannot call (DOM, WebSocket, IndexedDB, fetch, etc.), and even then, the JS function is 1-3 lines with zero logic — a pure bridge.

This is not a guideline. This is the entire point of the language.

**We are building a system for others to use.** Every architectural decision must work at scale, from first principles, without shortcuts. No hardcoded pixel positions when a layout engine should handle it. No JS workarounds when WASM can do it. No "fix it later" — other developers will build on this foundation. The layout engine, the signal system, the rendering pipeline — these are the primitives of a new web platform. They must be correct, fast, and composable. If something takes 450ms when it should take 10ms, that's a bug in the engine, not a reason to bypass it.

## Project Overview

- Users write `.nectar` files; the Rust compiler produces `.wasm` + a single JS syscall file (`core.js`, ~3 KB gzip)
- No garbage collector, no virtual DOM, no JavaScript dependencies, no node_modules
- One Rust binary (`nectar`) handles everything: compile, format, lint, test, dev server, LSP, package management, SSR
- Ownership model inspired by Rust — borrow checking, lifetimes, move semantics
- Declarative UI with fine-grained signals — O(1) updates per binding, no VDOM diffing
- Built-in language keywords for common web patterns: `component`, `store`, `router`, `page`, `form`, `channel`, `auth`, `payment`, `upload`, `db`, `cache`, `agent`, `theme`, `app`
- Standard library auto-included (no imports needed): `crypto`, `format`, `collections`, `BigDecimal`, `url`, `search`, `debounce`, `throttle`, `pagination`, `toast`, `skeleton`, `mask`, `chart`, `csv`, `datepicker`
- **Repo**: `git@github.com:HibiscusConsulting/nectar-lang.git`
- **License**: BSL 1.1 — free for all use except reselling the compiler or hosting it as a service. Converts to Apache 2.0 on 2030-03-12.
- **Owner**: Blake Burnette (jbburnette2@gmail.com) / Hibiscus Consulting

## Build & Test

```bash
# Build the compiler
cargo build                          # from /compiler or workspace root

# Run all tests
cargo test                           # 2248 tests

# Run tests for a specific module
cargo test --lib parser              # just parser tests
cargo test --lib codegen             # just codegen tests

# Test coverage (requires cargo-tarpaulin)
cargo tarpaulin --out json --output-dir .
```

The binary is `nectar` (defined in `compiler/src/main.rs`). CLI commands:

```bash
nectar build app.nectar --emit-wasm  # Compile to WebAssembly
nectar dev --src . --port 3000       # Dev server with hot reload
nectar fmt app.nectar                # Format
nectar lint app.nectar               # Lint
nectar test app.nectar               # Test
nectar check app.nectar              # Type-check + borrow-check without building
nectar build --ssr                   # Server-side rendering
nectar install                       # Package manager
nectar --lsp                         # LSP server
```

## Architecture

```
nectar-lang/
├── compiler/src/              # Rust compiler — single binary, 87K+ lines
│   ├── main.rs                # CLI entry point (clap)
│   ├── lexer.rs               # Tokenizer
│   ├── token.rs               # Token types
│   ├── parser.rs              # Parser → AST (~10K lines)
│   ├── ast.rs                 # AST node types (~1370 lines)
│   ├── type_checker.rs        # Type checking (~8700 lines)
│   ├── borrow_checker.rs      # Ownership/borrowing rules (~4200 lines)
│   ├── codegen.rs             # AST → WAT (~25K lines, largest file)
│   ├── wasm_binary.rs         # WAT → .wasm binary (~3200 lines)
│   ├── wasm_opt.rs            # WASM optimization passes
│   ├── const_fold.rs          # Constant folding
│   ├── dce.rs                 # Dead code elimination
│   ├── tree_shake.rs          # Tree shaking
│   ├── exhaustiveness.rs      # Pattern match exhaustiveness checking
│   ├── contract_infer.rs      # Contract shape inference from fetch responses
│   ├── contract_verify.rs     # Compile-time contract validation
│   ├── formatter.rs           # nectar fmt (~4200 lines)
│   ├── linter.rs              # nectar lint (~3800 lines)
│   ├── lsp.rs                 # Language server protocol
│   ├── ssr.rs                 # Server-side rendering (~1900 lines)
│   ├── ssr_server.rs          # SSR HTTP server (~930 lines)
│   ├── devserver.rs           # nectar dev (hot reload)
│   ├── stdlib.rs              # Standard library definitions (~4300 lines)
│   ├── package.rs             # Package management
│   ├── registry.rs            # Package registry client
│   ├── resolver.rs            # Dependency resolution
│   ├── critical_css.rs        # Critical CSS extraction
│   ├── runtime_modules.rs     # Runtime module embedding
│   ├── module_loader.rs       # Module loading
│   ├── module_resolver.rs     # Module resolution
│   ├── optimizer.rs           # Optimization coordinator
│   └── sourcemap.rs           # Source map generation
├── runtime/modules/
│   └── core.js                # THE ONLY JS file — ~890 lines, 16 namespaces, browser API syscalls ONLY
├── examples/                  # 37 .nectar example files
├── docs/                      # Language reference, architecture, runtime API, toolchain, AI integration
├── website/                   # Project website (written in Nectar)
├── CONTRIBUTING.md            # Architecture rules — READ THIS BEFORE ANY CHANGES
└── scripts/                   # Release scripts
```

### Compiler Pipeline

```
.nectar source
     │
Lexer → Token stream
     │
Parser → AST
     │
Type checker + Borrow checker + Contract inference/verification
     │
Optimizations (const folding, DCE, tree shaking)
     │
Codegen → WAT (WebAssembly Text Format)
  ├── Signal subscriptions (signal_subscribe with function table indices)
  ├── Reactive conditionals (updater functions for {if signal ...} blocks)
  ├── Lazy for-loops (initial batch of 20, IntersectionObserver pagination)
  ├── WASM JSON parser ($json_parse, $json_get_field — no JS JSON.parse)
  ├── Contract validation ($__contract_validate_<Name>)
  └── Function table for callbacks (call_indirect)
     │
wasm_binary → .wasm
     │
Browser loads .wasm + core.js (~3 KB gzip)
     │
mount() → innerHTML from WASM-built string (1 call)
flush() → batched DOM ops from command buffer (1 call/frame)
```

### Runtime Architecture (core.js)

`core.js` has 16 namespaces — each one exists because it wraps a browser API that WASM physically cannot call:

| Namespace | Browser APIs |
|---|---|
| `dom` | createElement, innerHTML, addEventListener, getElementById, querySelector, scrollTo, focus, blur, print, title, drag/drop |
| `timer` | setTimeout, setInterval, clearTimeout, clearInterval, requestAnimationFrame |
| `webapi` | history.pushState, location, navigator, clipboard, share, performance, geolocation, vibrate |
| `http` | fetch() with typed setters (setMethod, setBody, addHeader) |
| `observe` | IntersectionObserver, ResizeObserver, MutationObserver |
| `ws` | new WebSocket(), send, close, onmessage |
| `db` | indexedDB.open, objectStore CRUD, cursors |
| `worker` | new Worker(), postMessage, MessageChannel |
| `pwa` | serviceWorker.register, PushManager, caches |
| `hardware` | getUserMedia, geolocation, vibrate, biometricAuth |
| `payment` | Sandbox iframe postMessage |
| `auth` | OAuth popups, credential storage |
| `upload` | File input, FileReader, drag-and-drop files |
| `time` | Intl.DateTimeFormat (locale-aware formatting only) |
| `streaming` | EventSource (SSE), ReadableStream |
| `rtc` | RTCPeerConnection, data channels, getUserMedia, getDisplayMedia |

### WASM→JS Boundary Patterns

- **`__readOpts`**: WASM writes flat (key_ptr, key_len, val_ptr, val_len) tuples terminated by (0,0) into linear memory. JS reads them to build option objects for browser APIs. Replaces all JSON.parse/stringify.
- **Typed setters**: For complex APIs like fetch, WASM calls `setMethod()`, `setBody()`, `addHeader()` individually before triggering the call. No serialization.
- **Command buffer**: DOM updates are batched as opcodes (SET_TEXT, SET_ATTR, SET_STYLE, CLASS_ADD, APPEND_CHILD, etc.) in linear memory. One `flush()` per animation frame executes them all.
- **Callbacks**: WASM registers callback indices. JS calls `R.__cb(cbIdx)` when async operations complete.

## Critical Rules — Non-Negotiable

**Read `CONTRIBUTING.md` before making ANY changes.** The rules there are invariants, not suggestions.

### The JavaScript Rule

**NEVER create new `.js` files. NEVER add logic to `core.js`. NEVER use npm, node, package.json, or any Node.js tooling. NEVER create "thin JS bridges" or "JS helpers."** If you find yourself writing an `if` statement, a loop, a string transformation, or any conditional in JavaScript — stop. That belongs in Rust/WASM.

The test: if your function takes inputs and returns outputs without touching a browser API, it is WASM-internal. Period. If you think something needs JS, name the specific browser API it calls. If you can't name one, it's Rust/WASM.

### Architecture Rules

1. **Rust/WASM first, always.** Every computation, algorithm, data structure → Rust → WASM. Zero exceptions.
2. **JS exists only for browser API syscalls.** `core.js` is the ONLY runtime JS file. Functions are 1-3 lines, pure bridges to DOM/WebSocket/IndexedDB/etc.
3. **No logic in JavaScript.** No `if` statements, loops, string ops, or conditionals in `.js` files. WASM does all computation.
4. **DOM updates go through the command buffer.** Initial render: `mount()` with `innerHTML`. Updates: WASM writes opcodes into a command buffer in linear memory. A single `flush()` call per animation frame executes them all. One JS call per frame.
5. **No Node.js tooling.** Everything is the single `nectar` Rust binary. No npm, no package.json, no webpack, no bundler.
6. **No new .js files.** Ever. The only permitted JS files are `core.js` (runtime syscalls) and service worker infrastructure (SW spec requires JS).
7. **Prefer WASM-internal over JS bridges.** Signals, feature flags, validation, caching, gesture math, permissions, routing logic, form validation, animation math, state management — all WASM-internal. Zero JS.
8. **`__readOpts` over JSON.** No JSON.parse or JSON.stringify anywhere in the codebase. Use flat memory reads for structured data across the boundary.
9. **Typed setters over serialization.** For complex browser APIs (fetch, etc.), WASM calls individual setters (setMethod, setBody, addHeader) rather than serializing an options object.
10. **One WASM→JS boundary crossing, not many.** Batch operations into the command buffer. Prefer one `flush()` per frame over individual DOM syscalls.
11. **Standard library is pure WASM.** Every std lib namespace (crypto, format, collections, BigDecimal, url, search, csv, chart, toast, skeleton, datepicker) compiles to WASM instructions. No JS bridges, no thin wrappers.

### Decision Flowchart

When implementing a new feature:

```
Can it be a WASM-internal function (no browser API needed)?
  → Yes: Implement in Rust, compile to WASM. No JS bridge. Done.

Is it computation (math, string ops, data transformation, logic)?
  → Rust/WASM. No exceptions. Done.

Does it need to read/write the DOM?
  → Can it use existing flush() opcodes?
    → Yes: Write opcodes to the command buffer from WASM. Done.
    → No: Does it need a return value from the DOM?
      → Yes: Add a syscall to the dom namespace in core.js (1-2 lines).
      → No: Add a new opcode to flush(). Still WASM.

Does it need a browser API that WASM can't call?
  → Add a syscall to the appropriate namespace in core.js (1-2 lines).
  → All logic stays in WASM. The syscall is a pure bridge.

None of the above?
  → It's Rust/WASM. Always.
```

## Testing — Required

**Every change must have test coverage. No exceptions.**

- Tests are inline Rust tests (`#[cfg(test)] mod tests { ... }`) in each source file
- The codebase has 2248 tests. Do not reduce this number.
- **Every new feature, bug fix, or refactor must include corresponding tests**
- **Every code path must be tested** — happy paths, edge cases, error conditions, boundary values
- Match the existing test style in that file
- Use descriptive test names: `test_parse_component_with_props`, not `test1`
- Run `cargo test` and confirm zero failures before committing. Broken tests are not acceptable.
- Use `cargo tarpaulin` to verify coverage. Target 100% coverage on new code. Never decrease overall coverage.

### Test commands

```bash
cargo test                           # Run all — must pass
cargo test --lib <module>            # Run one module
cargo tarpaulin --out json           # Coverage report
```

### What to test

| Module | Test focus |
|---|---|
| `lexer.rs` | Every token type, edge cases (unicode, unterminated strings, nested comments) |
| `parser.rs` | Every AST node, malformed input, error recovery |
| `type_checker.rs` | Type inference, unification, generics, trait bounds, error messages |
| `borrow_checker.rs` | Ownership moves, borrows, lifetimes, use-after-move, double-borrow |
| `codegen.rs` | WAT output for every language feature, import signatures, opcode generation |
| `wasm_binary.rs` | Binary encoding of every WASM section and instruction |
| `formatter.rs` | Formatting every syntax construct, idempotency |
| `linter.rs` | Every lint rule fires correctly, no false positives |
| `const_fold.rs` | Arithmetic, boolean, string folding, overflow |
| `dce.rs` | Dead branches, unused variables, unreachable code |
| `tree_shake.rs` | Unused functions/types removed, used ones preserved |
| `exhaustiveness.rs` | Pattern completeness for enums, structs, nested patterns |
| `ssr.rs` | Server rendering output, hydration markers |
| `ssr_server.rs` | SSR HTTP server responses, streaming |
| `contract_infer.rs` | Shape inference from fetch patterns, field extraction |
| `contract_verify.rs` | Compile-time contract validation, type mismatches |
| `codegen.rs` (JSON) | WASM JSON parser output ($json_parse, $json_get_field), contract parse codegen |
| `codegen.rs` (lazy for) | Lazy for-loop batching, sentinel/observer setup, batch function emission |
| `codegen.rs` (reactive) | Reactive conditional updaters, signal subscription, function table entries |
| `codegen.rs` (callbacks) | Parameterized callback codegen, call_indirect with captured args |
| `package.rs` | Manifest parsing, validation |
| `registry.rs` | Package fetch, version resolution |
| `resolver.rs` | Dependency graphs, conflict resolution |

## What Actually Works vs Aspirational

Features are parsed and have codegen at different levels of maturity. This is the honest status.

### Works end-to-end (parse → type-check → codegen → WASM runs in browser)

- Components with props, signals, methods, render blocks, scoped styles
- Stores with signals, actions, computed values, effects
- Routers with static/parameterized/wildcard routes, fallback, guards, layouts, call_indirect navigation
- Contracts with field validation, WASM JSON parser, schema registration
- Signals with DOM subscriptions (signal_subscribe + function table updaters)
- Reactive conditionals ({if signal ...} in templates with live DOM updates)
- Lazy for-loops (initial batch of 20, IntersectionObserver-driven pagination)
- For/while loops, if/else, match (Ok/Err/Some/None + custom enum variants with limitations)
- String operations: len, push, contains, trim, to_upper, to_lower, split
- Array operations: len, push, contains, map, filter, reduce
- Range expressions (start..end) in for loops
- Format strings f"..." and format() function
- Arithmetic, comparison, logical, compound assignment operators
- Struct/enum definitions, impl blocks, field access, method calls
- Array indexing (items[i])
- Function table for callbacks (on:click with call_indirect)
- ? operator (parsed as Expr::Try, codegen exists)
- Try/catch (parsed and has limited codegen)
- Result<T,E> and Option<T> types
- Pages with meta/schema blocks
- Forms with field validation
- Channels (WebSocket), Auth, Payment, Upload, Db, Cache, Embed, Pdf, App/PWA, Theme
- Animations: spring, keyframes, stagger
- Lazy components
- navigate() for programmatic routing
- Borrow checker, ownership, move semantics
- Dead code elimination, tree shaking, constant folding
- SSR with hydration markers, critical CSS extraction
- WASM binary encoding

### Parsed but aspirational (no working runtime or incomplete codegen)

- `async fn` / `await` -- parsed, no async runtime in WASM
- Generic types / monomorphization -- parsed, no codegen
- Trait / impl dispatch -- parsed, no vtable generation
- Tuple types and destructuring -- parsed, no codegen
- Struct destructuring -- parsed, no codegen
- `break` / `continue` in loops -- not in codegen
- `spawn {}` / `parallel {}` -- parsed, no Web Worker runtime
- `channel<T>()` concurrency primitives -- parsed, no concurrency runtime
- `yield` / generator streams -- parsed, no generator runtime
- `for chunk in stream fetch(url)` -- parsed, no streaming runtime
- `suspend(<Fallback />) { ... }` -- parsed, limited codegen
- `bind:value={signal}` -- parsed and codegen exists, less tested than on:click
- Dynamic imports `import("./module")` -- parsed, limited codegen
- `prompt "..."` AI templates -- parsed, no AI runtime
- WebRTC -- parsed, runtime imports exist but untested end-to-end
- Full lifetime annotations -- parsed, limited enforcement
- Closures with environment capture -- parsed, limited codegen

### Important gotchas for AI sessions

- Use `format("{}", value)` not `value.to_string()` for int-to-string conversion
- No `String::from()` -- string literals are directly usable
- No `Vec::new()` -- use `[]` for empty arrays
- `match` on strings does not work -- use `if/else` chains
- No `println!` macro -- use `webapi.consoleLog()` or render in templates
- map/filter/reduce return arrays directly, no `.collect()`
- Modulo: `%` operator exists but use `i - (i / n) * n` if unsure
- Custom enum variant matching is limited compared to Ok/Err/Some/None

## Git Conventions

- **Remote**: SSH (`git@github.com:HibiscusConsulting/nectar-lang.git`)
- **Branch protection**: PRs required on `main`, 1 approval needed
- **Commits**: Use Blake's identity — `git -c user.name="Blake Burnette" -c user.email="jbburnette2@gmail.com" commit`
- **No Claude/AI references** in commits, code, or comments
- Commit messages should describe what changed and why, not how

## Allowed Browser API Syscalls

This is the exhaustive list of browser APIs that justify JS code. If a browser API is not on this list, it does not get a JS implementation:

- **DOM** — document.getElementById, innerHTML, addEventListener, createElement, etc.
- **WebSocket** — new WebSocket()
- **IndexedDB** — indexedDB.open()
- **Clipboard** — navigator.clipboard
- **Web Workers** — new Worker()
- **Service Workers** — navigator.serviceWorker.register()
- **Geolocation** — navigator.geolocation
- **Camera/Mic** — navigator.mediaDevices.getUserMedia()
- **Vibration** — navigator.vibrate()
- **localStorage/sessionStorage** — localStorage.getItem()
- **Cookies** — document.cookie
- **Fetch** — fetch()
- **History API** — history.pushState()
- **Print** — window.print()
- **Blob/URL** — URL.createObjectURL()
- **Intl API** — Intl.DateTimeFormat (locale-aware formatting only)
- **Performance API** — performance.mark(), performance.measure()
- **EventSource** — new EventSource() (SSE)
- **Payment Request** — new PaymentRequest()
- **Credential Management** — navigator.credentials
- **Intersection/Resize/Mutation Observer** — new IntersectionObserver(), etc.
- **WebRTC** — RTCPeerConnection, RTCDataChannel, getUserMedia(), getDisplayMedia()
- **requestAnimationFrame** — window.requestAnimationFrame()

If it's not on this list, the answer is Rust/WASM.
