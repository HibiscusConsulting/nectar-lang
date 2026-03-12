# Changelog

All notable changes to Nectar will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-03-12

### Added
- Core language: components, functions, structs, enums, traits, impls, modules
- Reactive signals with O(1) DOM updates via WASM linear memory
- Store-based state management with actions, effects, and computed values
- Router with declarative route definitions
- Contract system for API boundary safety (compile-time field checking, runtime WASM validation, SHA-256 wire-level staleness detection)
- Security: secret types, capability-based permissions, automatic CSP generation
- XSS structurally impossible (no innerHTML/eval/document.write)
- Prototype pollution impossible (WASM linear memory)
- Zero JavaScript dependencies — flat WASM binary output
- Progressive Web App support: app keyword, manifest, offline caching, push notifications
- Gesture recognition: swipe, pinch, long-press as language constructs
- Hardware access: haptic, biometric, camera, geolocation
- SEO/AAIO: page keyword with meta blocks, structured data (JSON-LD), auto sitemap/robots
- Static site generation (SSG), server-side rendering (SSR), hybrid build targets
- Semantic HTML enforcement via compiler warnings
- Declarative forms with built-in validation
- Real-time channels with WebSocket, auto-reconnect, typed messages
- Transparent concurrency: spawn (Web Workers), parallel execution
- Exhaustive error handling: must_use, mandatory Result/Option matching
- Bundle splitting via chunk keyword and dynamic imports
- Atomic signals for race-free shared state
- Memory leak detection for event listeners, intervals, subscriptions
- AI agent integration with tool definitions and prompt blocks
- Borrow checker for ownership and lifetime tracking
- Tree shaking and dead code elimination
- Code formatter (nectar fmt) and linter (nectar lint)
- Critical CSS extraction
- Source map generation
- Package management (nectar add, nectar install)
- Third-party embed management: `embed` keyword with sandbox isolation, loading strategies (defer, async, lazy, idle), and subresource integrity (SRI)
- First-class time types: `Instant`, `ZonedDateTime`, `Duration`, `Date`, `Time` with DST-safe arithmetic and explicit timezone conversions
- PDF generation: `pdf` keyword renders components to PDF, `download()` builtin triggers file save
- Payment integration: `payment` keyword with PCI-compliant sandboxed iframes, provider configuration
- Built-in authentication: `auth` keyword with declarative OAuth providers, session management
- File uploads: `upload` keyword with progress tracking, MIME type and size validation, chunked/resumable uploads
- Local database: `db` keyword wrapping IndexedDB with declarative schema, stores, and indexes
- Observability: `trace` blocks for automatic performance measurement, error tracking with context
- Feature flags: `flag()` builtin with compile-time dead code elimination
- Environment variables: `env()` builtin with compile-time validation
- `nectar audit` command for third-party embed security auditing
- Dev server `--flags` option for enabling feature flags during development
- Dev server `--tunnel` option for exposing local server via public URL
- Data caching: `cache` keyword with queries, mutations, stale-while-revalidate, optimistic updates, persistent IndexedDB cache, compile-time request deduplication
- Runtime tree-shaking: compiler detects used features and emits only the needed code in a single `core.js` runtime file (~3KB core-only build)
- Automatic accessibility: compiler generates ARIA attributes, roles, keyboard navigation, focus management, skip-nav links, live regions. Three modes: auto (default), hybrid, manual
- Cryptography: `crypto` namespace with sha256, sha512, sha384, sha1, hmac, aes-gcm/cbc/ctr, ed25519, pbkdf2, hkdf, ecdh, random_uuid, random_bytes — all pure WASM, zero JavaScript
- Theming: `theme` keyword with light/dark/auto modes, CSS custom properties, zero-flash toggle (~200 bytes runtime)
- Responsive design: `breakpoints` keyword, `fluid()` function compiling to CSS `clamp()` — zero JS overhead
- Clipboard API: `clipboard.copy`, `clipboard.paste`, `clipboard.copy_image` builtins
- Drag and drop: `draggable`/`droppable` keywords with touch, mouse, and keyboard support, accessible by default
- Animations: `spring` (physics-based), `keyframes` (CSS @keyframes), `stagger` (sequential list animation) — all respect `prefers-reduced-motion`
- Keyboard shortcuts: `shortcut` keyword with cross-platform key mapping (Cmd → Ctrl), component-scoped, accessible
- Virtualized lists: `virtual` keyword renders only visible items + buffer, 100K items with ~30 DOM nodes
- WebRTC: `rtc` namespace with peer connections, data channels, media tracks, screen sharing — JS bridges browser APIs, all orchestration in WASM
- Layout primitives: `<Stack>`, `<Row>`, `<Grid>`, `<Center>`, `<Cluster>`, `<Sidebar>`, `<Switcher>` — compile-time CSS, zero runtime cost
- Router layouts: persistent layout shells with `<Outlet />` for content swapping, view transitions between pages
- **Standard Library** (curated, security-reviewed, auto-included — no imports needed):
  - `debounce()` and `throttle()` — event handler operators, no lodash
  - `BigDecimal` type — arbitrary-precision arithmetic, no floating-point errors
  - `format` namespace — `number`, `currency`, `percent`, `bytes`, `compact`, `ordinal`, `relative_time` with locale support
  - `collections` namespace — `group_by`, `sort_by`, `uniq_by`, `chunk`, `flatten`, `zip`, `partition`
  - `url` namespace — `parse`, `build`, `query_get`, `query_set` for URL manipulation
  - `mask` namespace — `phone`, `credit_card`, `currency`, `pattern` for input formatting
  - `search` namespace — `create_index` and `query` for client-side fuzzy search
  - `toast` — `success`, `error`, `warning`, `info` notification system
  - `skeleton` — `text`, `circle`, `rect`, `card` loading placeholders with shimmer
  - `pagination` — `paginate`, `page_numbers`, `infinite_scroll` helpers

### Build targets
- `--target pwa` — Progressive Web App
- `--target twa` — Trusted Web Activity (Android Play Store)
- `--target capacitor` — iOS/Android native wrapper
- `--target ssg` — Static site generation
- `--target ssr` — Server-side rendering
- `--target hybrid` — SSG + SSR

[0.1.0]: https://github.com/HibiscusConsulting/nectar-lang/releases/tag/v0.1.0
