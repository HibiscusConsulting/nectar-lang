# {{PROJECT_NAME}} — Built with Nectar

## What is Nectar?

Nectar is a compiled-to-WebAssembly language. You write `.nectar` files; the compiler produces `.wasm` + a single JS syscall file (~3 KB). No JavaScript runtime, no virtual DOM, no node_modules, no bundler.

**NEVER write JavaScript.** Everything is Nectar, compiled to WASM. The only JS in the project is the compiler-generated `core.js` syscall layer — you do not edit it.

## Commands

```bash
nectar build app.nectar --emit-wasm  # Compile to WebAssembly
nectar dev --src . --port 3000       # Dev server with hot reload
nectar fmt app.nectar                # Format
nectar lint app.nectar               # Lint
nectar test app.nectar               # Test
nectar check app.nectar              # Type-check + borrow-check
```

## Language Quick Reference

### Core Constructs

| Construct | Purpose |
|---|---|
| `component` | UI building block — state, methods, styles, render |
| `store` | Global reactive state — signals, actions, computed, effects |
| `router` | Client-side routing with guards |
| `struct` / `enum` | Data types |
| `trait` / `impl` | Interfaces and implementations |
| `fn` / `async fn` | Functions |
| `test` | Test blocks |

### Built-in Domain Keywords

Use these instead of importing libraries:

| Keyword | Replaces | Purpose |
|---|---|---|
| `page` | Next.js pages | SEO-optimized routes with meta, structured data, SSR/SSG |
| `form` | React Hook Form | Declarative forms with built-in validators |
| `contract` | TypeScript interfaces | Type-safe API boundaries with runtime validation |
| `channel` | Socket.io | WebSocket with reconnect, heartbeat, typed messages |
| `auth` | NextAuth/Auth0 | OAuth providers, session management, lifecycle hooks |
| `payment` | Stripe.js | PCI-compliant payments via sandboxed iframes |
| `upload` | Dropzone/Uppy | File uploads with progress, validation, chunked transfer |
| `db` | Raw IndexedDB | Client-side database with declarative schema |
| `cache` | React Query/SWR | Stale-while-revalidate, TTL, optimistic updates |
| `embed` | `<script>` tags | Third-party scripts with sandbox and integrity |
| `pdf` | jsPDF | PDF generation from render blocks |
| `app` | Manual SW config | PWA manifest, offline, push notifications |
| `theme` | CSS variables | Design tokens for light/dark with zero flash |
| `agent` | Manual API calls | AI integration with tools and streaming |
| `spring` / `keyframes` / `stagger` | Framer Motion | Physics-based and keyframe animations |
| `breakpoints` | Media queries | Responsive design tokens |
| `crypto::` | Web Crypto API | SHA-256/512, AES, HMAC, Ed25519, PBKDF2 — pure WASM |
| `channel` + `rtc::` | Raw WebRTC | Peer connections, data channels, media tracks |

### Reactive State

```nectar
// In components/stores:
signal count: i32 = 0;        // reactive — DOM auto-updates when changed
let mut local: String = "";    // local mutable state
```

### Template Syntax (in render blocks)

```nectar
<div class="card">"text"</div>              // element + text
<span>{self.count}</span>                   // expression interpolation
<button on:click={self.handler}>"Go"</button> // event
<input bind:value={self.query} />           // two-way binding
{if cond { <A /> } else { <B /> }}          // conditional
{for item in items { <li>{item}</li> }}     // loop
{match opt { Some(v) => <V />, _ => <X /> }} // pattern match
<Child prop={value} />                      // child component
<Link to="/path">"Nav"</Link>              // client-side navigation
```

### Ownership

One owner per value. `&val` immutable borrow. `&mut val` exclusive mutable borrow. Assignment moves.

### Standard Library (auto-included, no imports)

`crypto`, `format`, `collections`, `BigDecimal`, `url`, `search`, `debounce`, `throttle`, `toast`, `skeleton`, `pagination`, `mask`, `csv`, `chart`, `datepicker`

## Testing

Every feature must have tests:

```nectar
test "counter increments" {
    let el = render(<Counter initial={0} />);
    el.findByText("+").click();
    assert_eq(el.findByRole("display").getText(), "1");
}
```

Run: `nectar test file.nectar`

## Full Documentation

- [Language Reference](https://github.com/HibiscusConsulting/nectar-lang/blob/main/docs/language-reference.md) — Complete syntax
- [AI Reference](https://github.com/HibiscusConsulting/nectar-lang/blob/main/docs/nectar-for-ai.md) — Every keyword with examples
- [Architecture](https://github.com/HibiscusConsulting/nectar-lang/blob/main/docs/architecture.md) — Compiler pipeline and runtime
- [Examples](https://github.com/HibiscusConsulting/nectar-lang/tree/main/examples) — 39 complete working apps
