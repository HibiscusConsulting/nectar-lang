# Nectar

**A programming language that compiles to WebAssembly, built for the next era of web development.**

![License: BSL 1.1](https://img.shields.io/badge/license-BSL%201.1-blue.svg)
![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)
![WASM](https://img.shields.io/badge/target-WebAssembly-654ff0.svg)

Nectar combines Rust's ownership model with declarative UI primitives, compiling everything to WebAssembly. No garbage collector. No virtual DOM. No JavaScript dependencies. Fine-grained signals update exactly the DOM nodes that changed — in O(1) time.

```nectar
component Counter(initial: i32) {
    let mut count: i32 = initial;

    fn increment(&mut self) {
        self.count = self.count + 1;
    }

    render {
        <div>
            <span>{self.count}</span>
            <button on:click={self.increment}>"+1"</button>
        </div>
    }
}
```

## Install

### From source (Rust toolchain required)

```bash
cargo install nectar-lang
```

### Homebrew (macOS/Linux)

```bash
brew install hibiscus-consulting/tap/nectar
```

### Binary download

Pre-built binaries for macOS, Linux, and Windows are available on the [Releases page](https://github.com/HibiscusConsulting/nectar-lang/releases).

## Usage

```bash
# Compile to WebAssembly
nectar build app.nectar --emit-wasm

# Start dev server with hot reload
nectar dev --src . --port 3000

# Format, lint, test
nectar fmt app.nectar
nectar lint app.nectar
nectar test app.nectar

# Type-check and borrow-check without building
nectar check app.nectar
```

### Use in your app

`nectar build` outputs `app.wasm` and bundles `core.js` (~3 KB gzip). Include both in your HTML:

```html
<!DOCTYPE html>
<html>
<head>
    <script type="module">
        import { instantiate } from './core.js';
        const app = await instantiate('./app.wasm');
        app.exports.main();
    </script>
</head>
<body>
    <div id="app"></div>
</body>
</html>
```

That's it. No bundler, no node_modules, no build pipeline.

## Deploy

Nectar compiles to static files (WASM + JS). Deploy anywhere you'd deploy a website.

| Platform | How |
|---|---|
| **AWS** | Upload to S3, serve via CloudFront |
| **GCP** | Upload to Cloud Storage, serve via Cloud CDN |
| **Azure** | Azure Static Web Apps, or Blob Storage + CDN |
| **Render** | Create a Static Site, point to your build directory |
| **Vercel** | `vercel deploy` with the build output directory |
| **Netlify** | Drag and drop, or connect your repo |
| **Cloudflare Pages** | Connect repo, set build command to `nectar build` |
| **GitHub Pages** | Push build output to `gh-pages` branch |

For SSR (`nectar build --ssr`), deploy to any platform that runs a web server (Render Web Service, AWS Lambda, Cloud Run, etc.).

## What You Get

**Language features** — components, stores, routers, signals, structs, enums, traits, generics, ownership, borrowing, pattern matching, async/await, auto a11y, layout primitives, view transitions

**Built-in keywords** — `page` (SEO), `form` (validation), `channel` (WebSocket/WebRTC), `auth`, `payment`, `upload`, `db`, `cache`, `embed`, `pdf`, `theme`, `app` (PWA), `agent` (AI), `crypto` (pure WASM)

**Standard library** — `debounce`, `throttle`, `BigDecimal`, `format`, `collections`, `url`, `mask`, `search`, `toast`, `skeleton`, `pagination`, `crypto`, `chart`, `csv` — all auto-included, no imports needed

**Security** — XSS structurally impossible, `secret` types, capability-based `permissions`, zero JS dependencies, no `node_modules`

**Toolchain** — compiler, formatter (`nectar fmt`), linter (`nectar lint`), test runner, dev server, package manager, LSP — one binary

## How It Works

```
  .nectar source
       |
  Compiler (Rust)
  |- Parse -> AST
  |- Type check + borrow check
  |- Codegen -> WAT
  '- Binary emit -> .wasm
       |
  Browser loads .wasm + single JS syscall file (~3 KB gzip)
       |
  mount() -> innerHTML from WASM-built string (1 call)
  flush() -> batched DOM ops from command buffer (1 call/frame)
```

Initial renders use `innerHTML` from a WASM-built HTML string. Updates write opcodes into a command buffer in linear memory — a single `flush()` call per frame executes them all. The JS layer is one file with browser API syscalls that WASM physically cannot call (DOM, WebSocket, IndexedDB, clipboard, etc.). All logic runs in WASM.

## Performance

| | React | Nectar |
|---|---|---|
| Runtime (gzip) | ~42 KB | ~2.8 KB |
| Re-render (1K items) | ~4 ms (VDOM diff) | ~0.3 ms (signal) |
| GC pauses | Yes | None (WASM linear memory) |
| Update complexity | O(n) tree walk | O(1) per binding |

## Examples

See [`examples/`](examples/) for complete working apps:

| Example | What it shows |
|---|---|
| [counter.nectar](examples/counter.nectar) | State, events, render |
| [todo.nectar](examples/todo.nectar) | Structs, enums, filtering |
| [ai-chat.nectar](examples/ai-chat.nectar) | Agent, tool, prompt |
| [pwa-app.nectar](examples/pwa-app.nectar) | Offline, push, install |
| [crypto.nectar](examples/crypto.nectar) | Hash, encrypt, sign |
| [std-lib.nectar](examples/std-lib.nectar) | Standard library usage |

[See all 39 examples ->](examples/)

## Documentation

| Doc | Contents |
|---|---|
| [Getting Started](docs/getting-started.md) | Install, first app, dev server |
| [Language Reference](docs/language-reference.md) | Full syntax, types, ownership, components, stores |
| [Architecture](docs/architecture.md) | Compiler pipeline, runtime, WASM bridge |
| [Runtime API](docs/runtime-api.md) | JS syscall layer, command buffer, WASM imports |
| [Toolchain](docs/toolchain.md) | CLI commands, formatter, linter, LSP |
| [AI Integration](docs/nectar-for-ai.md) | Agents, tools, prompts, streaming |

## License

Business Source License 1.1 — see [LICENSE](LICENSE).

You can use Nectar for any purpose, including production apps. The BSL restriction only prevents offering Nectar itself as a hosted compiler service or selling it as a standalone product. On the Change Date (2030-03-12), the license converts to Apache 2.0.
