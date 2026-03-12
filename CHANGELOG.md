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

### Build targets
- `--target pwa` — Progressive Web App
- `--target twa` — Trusted Web Activity (Android Play Store)
- `--target capacitor` — iOS/Android native wrapper
- `--target ssg` — Static site generation
- `--target ssr` — Server-side rendering
- `--target hybrid` — SSG + SSR

[0.1.0]: https://github.com/BlakeBurnette/nectar-lang/releases/tag/v0.1.0
