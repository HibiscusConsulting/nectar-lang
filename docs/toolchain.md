# Nectar Toolchain Reference

This document covers every command, flag, and configuration option in the Nectar toolchain.

---

## Table of Contents

1. [Overview](#overview)
2. [nectar build](#arc-build)
3. [nectar check](#arc-check)
4. [nectar test](#arc-test)
5. [nectar fmt](#arc-fmt)
6. [nectar lint](#arc-lint)
7. [nectar dev](#arc-dev)
8. [nectar init](#arc-init)
9. [nectar add](#arc-add)
10. [nectar install](#arc-install)
11. [--lsp (Language Server)](#--lsp-language-server)
12. [--ssr / --hydrate (Server-Side Rendering)](#--ssr----hydrate-server-side-rendering)
13. [Direct Compilation Mode](#direct-compilation-mode)
14. [Nectar.toml Configuration](#arctoml-configuration)

---

## Overview

The `nectar` command-line tool is the central entry point for compiling, testing, formatting, linting, and running Nectar programs. It also includes a development server, package manager, and language server.

```
nectar [COMMAND] [OPTIONS] [FILE]

Commands:
  build     Compile the project (and its dependencies)
  check     Type-check, borrow-check, and lint without codegen (fast)
  test      Compile and run test blocks
  fmt       Format Nectar source files
  lint      Run the linter on Nectar source files
  dev       Start the development server with hot reload
  init      Initialize a new Nectar project
  add       Add a dependency to Nectar.toml
  install   Resolve and download all dependencies

Flags:
  --lsp     Start the Language Server Protocol server
  --help    Show help information
  --version Show version information
```

---

## nectar build

Compile Nectar source files to WebAssembly.

### Usage

```sh
nectar build <input> [OPTIONS]
```

### Arguments

| Argument | Description |
|---|---|
| `<input>` | Source file to compile (`.nectar`) |

### Flags

| Flag | Description |
|---|---|
| `-o`, `--output <file>` | Output file path (default: `<input>.wat` or `<input>.wasm`) |
| `--emit-wasm` | Emit binary `.wasm` instead of `.wat` text format |
| `--ssr` | Emit SSR (server-side rendering) JavaScript module |
| `--hydrate` | Emit client hydration bundle |
| `--no-check` | Skip borrow checker and type checker |
| `-O`, `--optimize <level>` | Optimization level: 0, 1, or 2 (default: 0) |
| `--critical-css` | Extract and inline critical CSS for SSR builds (use with `--ssr`) |
| `--sw` | Generate service worker with precache manifest |

### Optimization Levels

| Level | Description | Passes |
|---|---|---|
| `-O0` | No optimizations (default) | None |
| `-O1` | Basic optimizations | Constant folding + Dead code elimination |
| `-O2` | Full optimizations | All of `-O1` + Tree shaking + WASM-level peephole optimization |

### Output Formats

| Format | Flag | Extension | Description |
|---|---|---|---|
| WAT | (default) | `.wat` | WebAssembly Text Format -- human-readable, useful for debugging |
| WASM | `--emit-wasm` | `.wasm` | Binary WebAssembly -- production-ready, smaller size |
| SSR | `--ssr` | `.ssr.js` | JavaScript module for server-side rendering |
| Hydrate | `--hydrate` | `.hydrate.wat` | Client-side hydration bundle |

### Examples

```sh
# Basic compilation to WAT
nectar build app.nectar

# Compile to binary WASM with full optimization
nectar build app.nectar --emit-wasm -O2

# Compile with custom output path
nectar build src/main.nectar -o dist/app.wasm --emit-wasm

# Server-side rendering
nectar build app.nectar --ssr

# Client hydration bundle
nectar build app.nectar --hydrate

# Skip checks for faster iteration
nectar build app.nectar --no-check

# SSR with critical CSS inlining
nectar build app.nectar --ssr --critical-css

# Generate with service worker and precache manifest
nectar build app.nectar --emit-wasm -O2 --sw
```

### Service Worker Generation (`--sw`)

When `--sw` is passed to `nectar build`, the compiler:

1. **Copies `nectar-service-worker.js`** to the output directory as `nectar-sw.js`
2. **Stamps `CACHE_VERSION`** with a hash derived from the build output (ensures cache busting on new deploys)
3. **Generates a precache manifest** listing all output files (`.wasm`, `.js`, `.css`, `.html`) and injects it as `self.__ARC_PRECACHE_MANIFEST__` at the top of the service worker
4. **Copies `nectar-sw-register.js`** to the output directory for client-side registration
5. **Injects a registration snippet** into the output HTML (if `--emit-wasm` produces an HTML shell):
   ```html
   <script src="nectar-sw-register.js"></script>
   <script>NectarSW.register();</script>
   ```

The generated service worker is fully self-contained and requires no configuration. It uses cache-first for static assets, network-first for API calls, and includes an offline fallback.

```sh
# Build with service worker
nectar build app.nectar --emit-wasm --sw

# Output directory will contain:
#   app.wasm
#   nectar-sw.js          (service worker with stamped version + manifest)
#   nectar-sw-register.js (client registration script)
```

### Compilation Pipeline

When you run `nectar build`, the compiler performs these steps in order:

1. **Dependency resolution** -- resolves `Nectar.toml` dependencies (if present)
2. **Lexing** -- tokenizes the source file
3. **Parsing** -- builds an AST with error recovery
4. **Module loading** -- resolves and loads `mod` declarations
5. **Borrow checking** -- validates ownership rules (unless `--no-check`)
6. **Type checking** -- Hindley-Milner type inference (unless `--no-check`)
7. **Exhaustiveness checking** -- warns about non-exhaustive match patterns
8. **Optimization** -- runs enabled optimization passes
9. **Code generation** -- emits WAT, WASM, SSR JS, or hydration bundle

---

## nectar check

Run all analysis passes (type checking, borrow checking, and linting) without generating any WASM output. This is significantly faster than `nectar build` because it skips codegen, optimization, and WASM emission entirely.

### Usage

```sh
nectar check <input>
```

### Arguments

| Argument | Description |
|---|---|
| `<input>` | Source file to check (`.nectar`) |

### Analysis Pipeline

When you run `nectar check`, the compiler performs these steps in order:

1. **Lexing** -- tokenizes the source file
2. **Parsing** -- builds an AST with error recovery
3. **Module loading** -- resolves and loads `mod` declarations (if present)
4. **Type checking** -- Hindley-Milner type inference
5. **Borrow checking** -- validates ownership and borrowing rules
6. **Exhaustiveness checking** -- warns about non-exhaustive match patterns
7. **Linting** -- runs all lint rules

Notably absent: optimization, codegen, and WASM emission. This makes `nectar check` the fastest way to verify correctness.

### Output Format

All diagnostics use the standard `file:line:col` format:

```
app.nectar:12:5: type error: expected `i32`, found `String`
app.nectar:20:9: borrow error: cannot borrow `x` as mutable; already borrowed as immutable
app.nectar:31:1: warning [unused-variable] variable `tmp` is declared but never used
```

If no issues are found:

```
nectar check: app.nectar is clean — no errors, no warnings
```

### Exit Codes

| Code | Meaning |
|---|---|
| 0 | No errors (warnings may be present) |
| 1 | One or more errors found |

### When to Use

- **During development** -- run `nectar check` for rapid feedback without waiting for WASM generation
- **In CI pipelines** -- use `nectar check` as a fast gate before the slower `nectar build` step
- **Editor integration** -- wire `nectar check` to your save hook for instant error reporting
- **Pre-commit hooks** -- validate correctness before committing

### Examples

```sh
# Check a single file
nectar check app.nectar

# Use in CI (fails on errors, warnings are non-fatal)
nectar check src/main.nectar || exit 1

# Quick feedback loop during development
nectar check src/main.nectar && nectar build src/main.nectar --emit-wasm
```

---

## nectar test

Compile and run test blocks defined with the `test` keyword.

### Usage

```sh
nectar test <input> [OPTIONS]
```

### Arguments

| Argument | Description |
|---|---|
| `<input>` | Source file containing tests (`.nectar`) |

### Flags

| Flag | Description |
|---|---|
| `--filter <pattern>` | Only run tests whose name contains `<pattern>` |
| `--watch` | Re-run tests automatically whenever the source file changes |

### Test Discovery

The test runner finds all top-level `test "name" { ... }` blocks in the specified file. Tests are validated through the full compilation pipeline (lex, parse, borrow check, type check, codegen).

### Test Output

```
running 3 tests
  test addition works ... ok
  test string concat ... ok
  test user creation ... ok

test result: ok. 3 passed; 0 failed
```

### Filtering Tests

```sh
# Run only tests containing "user" in their name
nectar test tests.nectar --filter "user"
```

### Watch Mode

When `--watch` is passed, Nectar monitors the source file for changes and automatically re-runs the test suite on each save. The console is cleared between runs, and each run is stamped with a timestamp. A 200ms debounce prevents double-fires from editors that perform multiple writes on save. Press **Ctrl+C** to stop.

```sh
# Watch mode — re-runs on every file change
nectar test tests.nectar --watch

# Combine watch mode with a filter
nectar test tests.nectar --watch --filter "auth"
```

Watch output:

```
[14:32:07] Running tests...

running 3 tests
  test addition works ... ok
  test string concat ... ok
  test user creation ... ok

test result: ok. 3 passed; 0 failed
```

The JavaScript test runner (`nectar-test-runner.js`) also supports watch mode for compiled `.wasm` test modules:

```sh
node nectar-test-runner.js tests.wasm --watch
node nectar-test-runner.js tests.wasm --watch --source-dir ./src
```

### Examples

```sh
# Run all tests
nectar test tests.nectar

# Filter by name
nectar test tests.nectar --filter "auth"

# Watch mode
nectar test tests.nectar --watch

# Watch + filter
nectar test tests.nectar --watch --filter "auth"
```

---

## nectar fmt

Format Nectar source files according to canonical style.

### Usage

```sh
nectar fmt [OPTIONS] [<input>]
```

### Flags

| Flag | Description |
|---|---|
| `<input>` | Source file to format (`.nectar`) |
| `--check` | Check formatting without writing changes. Exits with code 1 if reformatting is needed |
| `--stdin` | Read source from stdin instead of a file (output goes to stdout) |

### Formatting Rules

The formatter applies these canonical style rules:

- **Indentation**: 4 spaces
- **Braces**: opening brace on the same line as the declaration
- **Trailing commas**: added after the last item in lists
- **Line length**: long expressions are wrapped at reasonable widths
- **Blank lines**: one blank line between top-level items
- **Semicolons**: consistent semicolon placement for statements

### Editor Integration

**VS Code**: Install the Nectar extension (which uses `--lsp`) for format-on-save, or configure `nectar fmt --stdin` as an external formatter.

**Neovim**: Configure in your `init.lua`:

```lua
vim.api.nvim_create_autocmd("BufWritePre", {
  pattern = "*.nectar",
  callback = function()
    vim.cmd("silent !nectar fmt " .. vim.fn.expand("%"))
    vim.cmd("edit")
  end,
})
```

### CI Integration

Use `--check` in continuous integration to verify formatting:

```sh
nectar fmt --check src/main.nectar || (echo "Run 'nectar fmt' to fix formatting" && exit 1)
```

### Examples

```sh
# Format a file in place
nectar fmt app.nectar

# Check without modifying
nectar fmt --check app.nectar

# Format from stdin (e.g., pipe from another command)
cat app.nectar | nectar fmt --stdin
```

---

## nectar lint

Run static analysis on Nectar source files.

### Usage

```sh
nectar lint <input> [OPTIONS]
```

### Flags

| Flag | Description |
|---|---|
| `<input>` | Source file to lint (`.nectar`) |
| `--fix` | Attempt to auto-fix warnings (where supported) |

### Lint Rules

The linter checks for 10 rules, all enabled by default:

#### 1. `unused-variable` (Warning)

Detects variables that are declared but never used. Prefix with `_` to suppress.

```nectar
// Warning: variable `x` is declared but never used
let x = 42;

// OK: prefixed with underscore
let _x = 42;
```

#### 2. `unused-function` (Warning)

Detects private functions that are defined but never called.

```nectar
// Warning: function `helper` is defined but never called
fn helper() { }
```

#### 3. `unused-import` (Warning)

Detects imported names that are never referenced in the file.

```nectar
// Warning: imported name `utils` is never used
use std::utils;
```

#### 4. `mutable-not-mutated` (Warning)

Detects variables declared as `mut` but never assigned to after declaration.

```nectar
// Warning: variable `count` is declared as `mut` but is never mutated
let mut count = 0;
println(count);  // only read, never written
```

#### 5. `empty-block` (Warning)

Detects functions, if blocks, or else blocks with empty bodies.

```nectar
// Warning: function `todo` has an empty body
fn todo() { }

// Warning: if block has an empty body
if condition { }
```

#### 6. `snake-case-functions` (Warning)

Functions and methods should use `snake_case` naming.

```nectar
// Warning: function `myFunction` should use snake_case naming
fn myFunction() { }

// OK
fn my_function() { }
```

#### 7. `pascal-case-types` (Warning)

Types (structs, enums, components, stores, traits) should use `PascalCase` naming.

```nectar
// Warning: struct `my_struct` should use PascalCase naming
struct my_struct { }

// OK
struct MyStruct { }
```

#### 8. `unreachable-code` (Warning)

Detects code after a `return` statement in the same block.

```nectar
fn example() -> i32 {
    return 42;
    let x = 10;  // Warning: unreachable code after return statement
}
```

#### 9. `single-match` (Info)

Suggests using `if let` when a `match` has only one non-wildcard arm.

```nectar
// Info: this match has a single non-wildcard arm; consider using `if let`
match value {
    Some(x) => use(x),
    _ => {},
}
```

#### 10. `redundant-clone` (Info)

Flags `.clone()` calls that may be unnecessary if the source variable is not used afterwards.

```nectar
// Info: `data.clone()` may be redundant -- consider moving instead
let copy = data.clone();
```

### Output Format

Lint warnings follow this format:

```
<file>:<line>:<column>: <severity> [<rule>] <message>
```

Example:

```
app.nectar:12:5: warning [unused-variable] variable `x` is declared but never used
app.nectar:20:1: warning [snake-case-functions] function `myHandler` should use snake_case naming
```

### Exit Codes

| Code | Meaning |
|---|---|
| 0 | No warnings or errors |
| 1 | One or more warnings or errors found |

### Examples

```sh
# Lint a file
nectar lint app.nectar

# Lint with auto-fix
nectar lint app.nectar --fix
```

---

## nectar dev

Start a development server with hot reload.

### Usage

```sh
nectar dev [OPTIONS]
```

### Flags

| Flag | Default | Description |
|---|---|---|
| `--src <dir>` | `.` | Source directory to watch |
| `--build-dir <dir>` | `./build` | Build output directory |
| `-p`, `--port <port>` | `3000` | Port to serve on |

### How It Works

The dev server:

1. **Starts an HTTP server** on the specified port, serving the build directory
2. **Watches `.nectar` files** in the source directory using filesystem polling
3. **Recompiles on change** when a source file is modified
4. **Notifies the browser** via WebSocket to hot-reload the updated WASM module

### WebSocket Protocol

The dev server communicates with the browser runtime using a simple WebSocket protocol:

- **Server to Client**: `"reload"` -- signals that the WASM module has been recompiled and should be reloaded
- The client runtime reconnects automatically if the WebSocket connection drops

### Examples

```sh
# Start with defaults (port 3000, watch current directory)
nectar dev

# Custom port and source directory
nectar dev --src src --port 8080

# Custom build directory
nectar dev --build-dir dist --port 4000
```

---

## nectar init

Initialize a new Nectar project by creating an `Nectar.toml` manifest.

### Usage

```sh
nectar init [OPTIONS]
```

### Flags

| Flag | Description |
|---|---|
| `--name <name>` | Project name (defaults to the current directory name) |

### Generated File

`nectar init` creates an `Nectar.toml` file in the current directory:

```toml
[package]
name = "my-project"
version = "0.1.0"

[dependencies]
```

If an `Nectar.toml` already exists, the command fails with an error.

### Example

```sh
mkdir my-app
cd my-app
nectar init --name my-app
```

---

## nectar add

Add a dependency to `Nectar.toml`.

### Usage

```sh
nectar add <package> [OPTIONS]
```

### Arguments

| Argument | Description |
|---|---|
| `<package>` | Package name to add |

### Flags

| Flag | Default | Description |
|---|---|---|
| `--version <req>` | `*` (latest) | Version requirement (e.g., `^1.0`, `~2.3`, `=1.2.3`) |
| `--path <dir>` | (none) | Local path dependency |
| `--features <list>` | (none) | Comma-separated list of features to enable |

### Dependency Formats

**Simple version dependency**:

```sh
nectar add my-lib --version "^1.0"
```

Adds to `Nectar.toml`:

```toml
[dependencies]
my-lib = "^1.0"
```

**Detailed dependency with features**:

```sh
nectar add ui-kit --version "^2.0" --features "animations,themes"
```

Adds to `Nectar.toml`:

```toml
[dependencies.ui-kit]
version = "^2.0"
features = ["animations", "themes"]
```

**Local path dependency**:

```sh
nectar add shared-lib --path "../shared-lib"
```

Adds to `Nectar.toml`:

```toml
[dependencies.shared-lib]
path = "../shared-lib"
```

---

## nectar install

Resolve and download all dependencies declared in `Nectar.toml`.

### Usage

```sh
nectar install
```

### Behavior

1. Reads `Nectar.toml` from the current directory
2. Resolves the dependency graph (fetching version metadata from the registry)
3. Downloads packages to the local cache
4. Writes `Nectar.lock` with pinned versions and checksums

If no `Nectar.toml` exists or there are no dependencies, the command succeeds silently.

### Output

```
resolved 3 dependencies
  http-client v1.2.0
  json-parser v0.8.3
  ui-components v2.1.0
```

### Nectar.lock

The lockfile (`Nectar.lock`) pins exact versions for reproducible builds:

```toml
version = 1

[[packages]]
name = "http-client"
version = "1.2.0"
source = "registry+~/.nectar/cache/http-client-1.2.0"
```

Commit `Nectar.lock` to version control for reproducible builds.

---

## --lsp (Language Server)

Start the Language Server Protocol (LSP) server for editor integration.

### Usage

```sh
nectar --lsp
```

### LSP Capabilities

The Nectar language server provides:

- **Diagnostics** -- real-time error and warning reporting as you type
- **Go to Definition** -- jump to the definition of functions, types, and variables
- **Hover Information** -- type information and documentation on hover
- **Completion** -- context-aware code completion for keywords, types, and identifiers
- **Formatting** -- document formatting using the built-in formatter

### VS Code Setup

Install the Nectar VS Code extension, or configure manually in `.vscode/settings.json`:

```json
{
  "nectar.serverPath": "/path/to/arc",
  "nectar.serverArgs": ["--lsp"]
}
```

### Neovim Setup (nvim-lspconfig)

```lua
local lspconfig = require('lspconfig')

lspconfig.nectar = {
  default_config = {
    cmd = { 'nectar', '--lsp' },
    filetypes = { 'nectar' },
    root_dir = lspconfig.util.root_pattern('Nectar.toml', '.git'),
  },
}

lspconfig.nectar.setup({})
```

### Other Editors

Any editor supporting LSP can use Nectar's language server. Point the editor's LSP client to `nectar --lsp` as the server command.

---

## --ssr / --hydrate (Server-Side Rendering)

Nectar supports server-side rendering (SSR) with client-side hydration for fast initial page loads.

### SSR Workflow

**Step 1: Generate the SSR bundle**

```sh
nectar build app.nectar --ssr
```

This produces `app.ssr.js`, a JavaScript module that renders your components to HTML strings on the server.

**Step 2: Generate the hydration bundle**

```sh
nectar build app.nectar --hydrate
```

This produces `app.hydrate.wat` (or `.wasm` with `--emit-wasm`), a lightweight client bundle that attaches event handlers and reactivity to the server-rendered HTML without re-rendering.

**Step 3: Serve from your backend**

```javascript
// Node.js example
const { render } = require('./app.ssr.js');
const html = render({ props: { /* ... */ } });

res.send(`
<!DOCTYPE html>
<html>
<body>
  <div id="app">${html}</div>
  <script src="nectar-runtime.js"></script>
  <script>
    const runtime = new NectarRuntime();
    runtime.mount('app.hydrate.wasm', document.getElementById('app'));
  </script>
</body>
</html>
`);
```

### Benefits

- **Faster First Paint** -- users see content immediately from the server-rendered HTML
- **SEO Friendly** -- search engines can index the server-rendered content
- **Smaller Client Bundle** -- the hydration bundle skips initial DOM creation

### Critical CSS Inlining (`--critical-css`)

When building with `--ssr --critical-css`, the compiler extracts styles that are needed for the initial render and inlines them directly into the SSR output. This eliminates the flash of unstyled content (FOUC) and provides instant visual feedback.

**How it works:**

1. The compiler analyzes all components in the program and classifies their styles as **critical** or **deferred**:
   - **Critical**: styles from non-lazy components, styles from the first route's component in each router, built-in skeleton and reset CSS
   - **Deferred**: styles from `lazy component` definitions and components not on the initial route
2. Critical CSS is inlined in a `<style data-nectar-critical>` tag in the `<head>`
3. Deferred CSS is written to a separate `.css` file and loaded asynchronously using the `media="print"` / `onload` pattern
4. A `window.__nectarCriticalLoaded` flag is set so the client-side runtime knows not to double-inject styles that are already present

**Usage:**

```sh
# Build SSR module with critical CSS extraction
nectar build app.nectar --ssr --critical-css
```

This produces:
- `app.ssr.js` -- SSR module with critical CSS inlined in the output
- `app.ssr.css` -- deferred CSS loaded asynchronously by the client

**What is always inlined:**

The following base styles are always included in the critical CSS, even if no component explicitly declares them:

- **Nectar reset** -- minimal box-sizing reset and hydration transition
- **Skeleton loading** -- `.nectar-skeleton`, `.nectar-skeleton-text`, `.nectar-skeleton-avatar`, `.nectar-skeleton-rect` with shimmer animation

**Server integration:**

```javascript
const { renderApp } = require('./app.ssr.js');

// renderApp automatically includes:
// - <style data-nectar-critical>...</style> in <head>
// - <link rel="stylesheet" href="styles.css" media="print" onload="this.media='all'">
// - <script>window.__nectarCriticalLoaded = true;</script>
const html = renderApp('App', {});
```

---

## Direct Compilation Mode

For quick one-off compilations, you can pass a file directly to `nectar` without a subcommand:

```sh
arc app.nectar [OPTIONS]
```

This is equivalent to `nectar build app.nectar` but also supports debug flags:

| Flag | Description |
|---|---|
| `--emit-tokens` | Print the token stream and exit (for debugging the lexer) |
| `--emit-ast` | Print the AST and exit (for debugging the parser) |
| `--emit-wasm` | Emit binary `.wasm` |
| `--ssr` | Emit SSR JavaScript module |
| `--hydrate` | Emit hydration bundle |
| `--no-check` | Skip borrow checker and type checker |
| `-O <level>` | Optimization level |
| `-o <file>` | Output file path |

### Examples

```sh
# Debug: see all tokens
arc app.nectar --emit-tokens

# Debug: see the full AST
arc app.nectar --emit-ast

# Quick compile
arc app.nectar --emit-wasm -O2 -o dist/app.wasm
```

---

## Nectar.toml Configuration

`Nectar.toml` is the project manifest, similar to `Cargo.toml` or `package.json`.

### Structure

```toml
[package]
name = "my-app"
version = "0.1.0"

[dependencies]
http-client = "^1.0"
json-parser = "~0.8"

[dependencies.ui-kit]
version = "^2.0"
features = ["animations", "themes"]

[dependencies.shared-lib]
path = "../shared-lib"
```

### Package Section

| Field | Type | Description |
|---|---|---|
| `name` | String | Project name |
| `version` | String | Project version (semver) |

### Dependencies Section

Dependencies can be specified in two forms:

**Simple**: just a version string.

```toml
[dependencies]
my-lib = "^1.0"
```

**Detailed**: version, features, path, or registry URL.

```toml
[dependencies.my-lib]
version = "^2.0"
features = ["feature-a", "feature-b"]
path = "../my-lib"             # local path (optional)
registry_url = "https://..."   # custom registry (optional)
```

### Version Requirements

| Syntax | Meaning |
|---|---|
| `"^1.0"` | Compatible with 1.0 (>=1.0.0, <2.0.0) |
| `"~1.2"` | Approximately 1.2 (>=1.2.0, <1.3.0) |
| `"=1.2.3"` | Exactly 1.2.3 |
| `"*"` | Any version (latest) |
