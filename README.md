# Nectar

**A programming language that compiles to WebAssembly, built for the next era of web development.**

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
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

`nectar build` outputs `app.wasm` and bundles `core.js` (~10 KB gzip). Include both in your HTML:

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

**Nectar Deploy** (managed platform — in development):

```bash
nectar deploy --project my-app
```

One command provisions hosting, database, auth, payments, caching, file storage, and WebSocket channels based on the keywords in your source code. See [Platform](#platform) below.

**Self-host** — deploy the static output anywhere:

| Platform | How |
|---|---|
| **AWS** | Upload to S3, serve via CloudFront |
| **GCP** | Upload to Cloud Storage, serve via Cloud CDN |
| **Azure** | Azure Static Web Apps, or Blob Storage + CDN |
| **Render** | Create a Static Site, point to your build directory |
| **Vercel** | `vercel deploy` with the build output directory |
| **Netlify** | Drag and drop, or connect your repo |
| **GitHub Pages** | Push build output to `gh-pages` branch |

For SSR (`nectar build --ssr`), deploy to any platform that runs a web server (Render Web Service, AWS Lambda, Cloud Run, etc.).

## Architecture (Three Layers)

Nectar is built in three composable layers. Application code touches keywords and standard-library calls; provider implementations are hot-swappable behind the scenes.

### 1. Keywords (language primitives)

80+ reserved keywords baked into the lexer/parser. Application surfaces are first-class language constructs, not library imports.

- **Structure** — `component`, `store`, `router`, `route`, `page`, `layout`, `outlet`
- **State + reactivity** — `signal`, `action`, `computed`, `effect`, `selector`, `atomic`
- **App surfaces** — `form`, `channel`, `agent`, `app` (PWA), `theme`, `auth`, `payment`, `banking`, `upload`, `embed`, `pdf`, `db`, `cache`, `map`, `crypto`, `miniprogram`
- **Device + platform** — `clipboard`, `draggable`, `droppable`, `download`, `haptic`, `biometric`, `camera`, `geolocation`, `push`
- **Safety + ops** — `secret`, `contract`, `guard`, `flag`, `trace`, `env`, `must_use`, `validate`, `schema`, `optimistic`, `invalidate`

### 2. Standard library (auto-included interfaces)

43 registered modules exposing the function surface for each keyword domain. No imports needed — every Nectar program has the full standard library available at compile time.

- **Core types** — `Vec`, `HashMap`, `Option`, `Result`, `String`, iterator trait, `BigDecimal`
- **Utilities** — `format`, `collections`, `url`, `mask`, `search`, `theme`, `debounce`, `throttle`, `crypto`, `csv`
- **UI components** — `data_table`, `datepicker`, `combobox`, `chart`, `editor`, `image`, `qr`, `share`, `wizard`, `toast`, `skeleton`, `pagination`
- **App services** — `auth`, `db`, `upload`, `payment`, `maps`, `media`, `rtc`, `gpu`, `miniprogram`
- **Animation + layout** — `animate`, `responsive`, `syntax`

### 3. Providers (concrete service integrations)

Pluggable JS modules in [`providers/`](providers/) that fulfill standard-library interfaces. Application code calls keyword/stdlib APIs (e.g. `payment::charge`, `mp::tradePay`); the provider layer translates to vendor-specific HTTP/SDK calls.

| Provider | Domain | Backs |
|---|---|---|
| [moov.js](providers/moov.js) | Banking, ACH, wallets, transfers | `banking`, `payment` |
| [stripe.js](providers/stripe.js) | Card payments, Connect | `payment` |
| [plaid.js](providers/plaid.js) | Bank account linking, transactions | `banking` |
| [alipay.js](providers/alipay.js) | Chinese super-app payments | `miniprogram` (`mp::*`) |
| [mapbox.js](providers/mapbox.js) | Maps, geocoding, directions | `map`, `maps` |

Provider contract: implement the interface defined in `compiler/src/stdlib.rs`. Switching from Stripe to Moov is a config flip — application code does not change.

See [Providers](docs/providers.md) for the integration model and how to add a new one.

## What You Get

**Language features** — components, stores, routers, signals, structs, enums, traits, generics, ownership, borrowing, pattern matching, async/await, auto a11y, layout primitives, view transitions

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
  Browser loads .wasm + single JS syscall file (~10 KB gzip)
       |
  mount() -> innerHTML from WASM-built string (1 call)
  flush() -> batched DOM ops from command buffer (1 call/frame)
```

Initial renders use `innerHTML` from a WASM-built HTML string. Updates write opcodes into a command buffer in linear memory — a single `flush()` call per frame executes them all. The JS layer is one file with browser API syscalls that WASM physically cannot call (DOM, WebSocket, IndexedDB, clipboard, etc.). All logic runs in WASM.

## Performance

| | React | Nectar |
|---|---|---|
| Runtime (gzip) | ~42 KB | ~10 KB |
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

[See all 60+ examples ->](examples/)

## Platform

Nectar is one part of a larger ecosystem:

| Component | Status | Description |
|---|---|---|
| **Nectar** | Beta | The language and compiler — `.nectar` to `.wasm` |
| **Honeycomb** | Beta | Canvas rendering engine — replaces the browser's DOM/CSS/paint pipeline with a WASM-native element tree, stack-based layout engine, and Canvas 2D renderer |
| **Pollen** | In Development | Native desktop/mobile runtime — replaces Electron with a lightweight WASM-first shell (no V8, no Chromium, no GC) |
| **Bloom** | Planned | WASM-first browser — executes WASM natively without a JS engine intermediary, eliminating the WASM-to-JS bridge entirely |
| **Nectar Deploy** | In Development | Managed hosting + services platform — language keywords (`auth`, `db`, `payment`, `cache`, `channel`, `upload`) map directly to managed infrastructure provisioned on deploy |

### Render Modes

The same `.nectar` source compiles to two rendering backends:

```bash
nectar build app.nectar --render=dom      # Browser DOM (default) — SSR, SEO, accessibility
nectar build app.nectar --render=canvas   # Honeycomb — WASM layout + Canvas 2D, zero DOM nodes
```

See [Render Modes](docs/render-modes.md) for details.

## Documentation

| Doc | Contents |
|---|---|
| [Getting Started](docs/getting-started.md) | Install, first app, dev server |
| [Language Reference](docs/language-reference.md) | Full syntax, types, ownership, components, stores, keywords |
| [Architecture](docs/architecture.md) | Compiler pipeline, runtime, WASM bridge |
| [Providers](docs/providers.md) | Provider model, built-in providers, adding a new provider |
| [Render Modes](docs/render-modes.md) | DOM, Canvas, and Hybrid rendering modes |
| [Runtime API](docs/runtime-api.md) | JS syscall layer, command buffer, WASM imports |
| [Toolchain](docs/toolchain.md) | CLI commands, formatter, linter, LSP |
| [AI Integration](docs/nectar-for-ai.md) | Agents, tools, prompts, streaming |
| [Examples](docs/examples.md) | Worked examples for every keyword and stdlib module |
| [Whitepaper](docs/whitepaper.md) | Design rationale and the case for compiled WASM-first frontends |

## Documentation Site

The Markdown files in `docs/` are the source of truth. They are surfaced on
[buildnectar.com/docs](https://buildnectar.com/docs) by `scripts/build_docs_pages.py`,
which converts each `docs/*.md` file into a corresponding Nectar page under
`website/src/pages/docs/`.

When you edit anything in `docs/`, regenerate the website pages:

```bash
python3 scripts/build_docs_pages.py
```

The script also rewrites `website/src/pages/docs.nectar` (the docs hub) and
`website/src/app.nectar` (the router) so the new pages are reachable.
Generated `.nectar` files carry a "DO NOT EDIT" header — make all content
changes in `docs/*.md` and rerun the script.

To add a brand-new top-level doc, add the source file to `docs/` and append
an entry to `DOC_REGISTRY` in the script.

## License

MIT License — see [LICENSE](LICENSE).

Nectar is free and open source. You can use, modify, and distribute it for any purpose.
