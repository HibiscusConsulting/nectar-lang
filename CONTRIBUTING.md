# Contributing to Nectar — Architecture Rules

**Read this before making ANY changes to the Nectar codebase.**

This document defines the hard constraints for the Nectar project. These are not suggestions. They are invariants. If a change violates any of these rules, it must be rejected.

---

## Rule 1: Rust/WASM First, Always

Every feature, function, algorithm, data structure, and computation MUST be implemented in Rust, compiled to WASM. This includes:

- All standard library features (crypto, formatting, collections, search, etc.)
- All reactive state management (signals, effects, memos)
- All routing logic
- All form validation
- All data transformation
- All string manipulation
- All component lifecycle management
- All scheduling and batching
- All animation math (spring, easing, interpolation)

**There are ZERO exceptions for computation.** If it can be expressed as a function that takes inputs and returns outputs, it is Rust.

---

## Rule 2: Prefer WASM-Internal Over JS Bridges

Before creating a JS bridge function in core.js, ask: **can this live entirely in WASM?**

WASM modules can call their own internal functions. If a feature is pure computation + state management, it should be WASM-internal functions — NOT imported from JS. Only cross the WASM→JS boundary when you physically need a browser API.

**WASM-internal** (no JS needed):
- Reactive signals (dependency graph, effect scheduling, batching)
- Feature flags (compile-time constants in WASM data section)
- Validation (schema checking, form validation)
- Caching logic (LRU, TTL, invalidation)
- Gesture math (velocity, direction, distance calculations)
- Permissions enforcement
- State management (atomic operations on shared memory)

**JS bridge required** (browser API):
- DOM manipulation (createElement, innerHTML, addEventListener)
- Network (fetch, WebSocket, EventSource)
- Storage (localStorage, IndexedDB, cookies)
- Hardware (geolocation, camera, vibration)
- Navigation (history.pushState, location.href)

The test: if your function takes inputs and returns outputs without touching a browser API, it's WASM-internal. Period.

---

## Rule 3: JavaScript Exists Only for Browser API Syscalls

JavaScript is permitted ONLY for browser APIs that WebAssembly physically cannot call. These are:

- **DOM access** — `document.getElementById`, `innerHTML`, `addEventListener`, etc.
- **WebSocket** — `new WebSocket()`
- **IndexedDB** — `indexedDB.open()`
- **Clipboard** — `navigator.clipboard`
- **Web Workers** — `new Worker()`
- **Service Workers** — `navigator.serviceWorker.register()`
- **Geolocation** — `navigator.geolocation`
- **Camera/Mic** — `navigator.mediaDevices.getUserMedia()`
- **Vibration** — `navigator.vibrate()`
- **localStorage/sessionStorage** — `localStorage.getItem()`
- **Cookies** — `document.cookie`
- **Fetch** — `fetch()` (network requests)
- **History API** — `history.pushState()`
- **Print** — `window.print()`
- **Blob/URL** — `URL.createObjectURL()`
- **Intl API** — `Intl.DateTimeFormat` (locale-aware formatting only)
- **Performance API** — `performance.mark()`, `performance.measure()`
- **EventSource** — `new EventSource()` (SSE)
- **WebRTC** — `RTCPeerConnection`, `RTCDataChannel`, `getUserMedia()`, `getDisplayMedia()`

If it's not on this list, it doesn't get a JS implementation. Period.

---

## Rule 4: DOM Manipulation Goes Through the Command Buffer

Nectar uses a mount/flush opcode architecture for DOM updates:

- **Initial render**: WASM builds an HTML string in linear memory. A single `mount()` call sets `innerHTML`. One JS call.
- **Updates**: WASM writes opcodes (SET_TEXT, SET_ATTR, SET_STYLE, CLASS_ADD, APPEND_CHILD, etc.) into a command buffer in linear memory. A single `flush()` call per animation frame reads and executes them all. One JS call.

Do NOT add individual DOM manipulation syscalls unless:
1. The operation needs a **return value** (e.g., `getElementById` returns an element handle)
2. The operation cannot be expressed as a batch opcode

Adding a new opcode to `flush()` is almost always better than adding a new syscall.

---

## Rule 5: One JS File

All browser API syscalls live in `runtime/modules/core.js`. That is the ONLY runtime JS file.

Other JS files in `runtime/` are permitted ONLY for:
- **Service workers** (the SW spec requires JavaScript)
- **Hot reload client** (dev-mode only, connects to dev server WebSocket)
- **Hydration** (attaches WASM to server-rendered DOM)

These are infrastructure files, not runtime. They do not contain application logic or std lib features.

---

## Rule 6: No Node.js Tooling

The `nectar` binary (Rust) handles ALL tooling:
- Compiler (`nectar build`)
- Test runner (`nectar test`)
- Dev server (`nectar dev`)
- Formatter (`nectar fmt`)
- Linter (`nectar lint`)
- LSP server (`nectar --lsp`)
- Package manager (`nectar install`)
- SSR (`nectar build --ssr`)

Do NOT create Node.js scripts, npm packages, or JavaScript CLI tools. Everything is one Rust binary.

---

## Rule 7: No Logic in JavaScript

If you find yourself writing an `if` statement, a loop, a string transformation, or any conditional logic in a `.js` file, stop. That logic belongs in Rust/WASM.

JavaScript functions in `core.js` should be 1-3 lines each:
```js
// GOOD — pure syscall, no logic
setTitle(ptr, len) { document.title = R.__getString(ptr, len); }

// BAD — logic in JS (bucketing, formatting, conditionals)
formatTime(ms) {
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return seconds + 's';  // NO. This is computation.
  // ...
}
```

The WASM module does all computation and passes the result. JavaScript just bridges it to the browser API.

---

## Rule 8: Standard Library is Pure WASM

Every std lib namespace compiles to WASM instructions. No JS bridges, no thin wrappers, no "syscall helpers":

| Namespace | Implementation |
|---|---|
| `crypto` | Rust (sha2, aes-gcm, ed25519-dalek) → WASM |
| `format` | Rust (locale tables compiled in) → WASM |
| `collections` | Rust (BTreeMap, HashSet, etc.) → WASM |
| `BigDecimal` | Rust (arbitrary precision) → WASM |
| `url` | Rust (parser) → WASM |
| `search` | Rust (fuzzy matching) → WASM |
| `csv` | Rust (parser/serializer) → WASM |
| `chart` | Rust (SVG generation) → WASM |
| `toast`, `skeleton`, `datepicker`, etc. | Rust → WASM (use core DOM opcodes for rendering) |

If a std lib feature needs to render UI, it builds HTML strings in WASM and uses the existing `mount()`/`flush()` opcodes. It does NOT get its own JS module.

---

## Decision Flowchart

When implementing a new feature, follow this:

```
Can it be a WASM-internal function (no browser API needed)?
  → Yes: Implement in Rust, compile to WASM-internal function. No JS bridge.

Is it computation (math, string ops, data transformation, logic)?
  → Rust/WASM. No exceptions.

Does it need to read/write the DOM?
  → Can it use existing flush() opcodes (SET_TEXT, SET_ATTR, SET_STYLE, etc.)?
    → Yes: Do it in WASM by writing opcodes to the command buffer.
    → No: Does it need a return value from the DOM?
      → Yes: Add a syscall to the dom namespace in core.js (1-2 lines).
      → No: Add a new opcode to flush(). Still WASM.

Does it need a browser API that WASM can't call?
  → Add a syscall to the appropriate namespace in core.js (1-2 lines).
  → All logic stays in WASM. The syscall is a pure bridge.

None of the above?
  → It's Rust/WASM. Always.
```

---

## For AI Assistants

If you are an AI generating or modifying Nectar code:

1. **NEVER create new `.js` files.** The answer is always Rust/WASM.
2. **NEVER add logic to `core.js`.** Syscalls only. 1-3 lines per function.
3. **NEVER use `npm`, `node`, `package.json`, or any Node.js tooling.**
4. **NEVER create "thin JS bridges" or "JS helpers".** If you think you need one, you're wrong. Do it in WASM.
5. **ALWAYS check if an existing flush() opcode handles your need** before adding a new syscall.
6. **ALWAYS implement std lib features as pure Rust** compiled to WASM.
7. **ALWAYS prefer one WASM→JS boundary crossing over many.** Batch operations into the command buffer.
8. **ALWAYS prefer WASM-internal functions over JS bridges.** If a feature doesn't need a browser API, it has zero JS. The signal runtime, feature flags, validation, caching — all WASM-internal.
9. **Test your assumption**: If you think something needs JS, explain which specific browser API it calls. If you can't name one, it's WASM-internal.

The guiding principle: **JavaScript is an impedance mismatch we minimize. Rust/WASM is the platform.**
