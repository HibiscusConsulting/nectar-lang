# Nectar Runtime API Reference

This document describes every WASM import module and function provided by the Nectar runtime (`nectar-runtime.js`). These are the host functions that compiled Arc modules call to interact with the browser environment.

---

## Table of Contents

1. [dom](#dom) -- DOM manipulation
2. [signal](#signal) -- Reactive state primitives
3. [http](#http) -- HTTP communication
4. [worker](#worker) -- Concurrency primitives
5. [ai](#ai) -- AI/LLM interaction
6. [streaming](#streaming) -- Streaming data
7. [media](#media) -- Image and resource loading
8. [router](#router) -- Client-side routing
9. [style](#style) -- Scoped CSS injection
10. [animation](#animation) -- Web Animations API bridge
11. [a11y](#a11y) -- Accessibility
12. [webapi](#webapi) -- Web platform APIs
13. [string](#string) -- String operations
14. [test](#test) -- Testing support
15. [sw](#sw) -- Service worker

---

## dom

DOM manipulation functions for creating and updating the document tree.

### createElement

```
dom.createElement(tagPtr: i32, tagLen: i32) -> i32
```

Creates a DOM element with the given tag name. Returns an element handle (integer) used to reference the element in subsequent calls.

**Parameters:**
- `tagPtr` -- pointer to the tag name string in WASM memory
- `tagLen` -- length of the tag name string

**Returns:** Element handle (i32)

### setText

```
dom.setText(parentHandle: i32, textPtr: i32, textLen: i32)
```

Sets the `textContent` of the element identified by `parentHandle`.

### appendChild

```
dom.appendChild(parentHandle: i32, childHandle: i32)
```

Appends the child element to the parent element in the DOM tree.

### addEventListener

```
dom.addEventListener(handle: i32, eventPtr: i32, eventLen: i32, callbackIdx: i32)
```

Adds an event listener to the element. When the event fires, the WASM function exported as `__handler_{callbackIdx}` is called.

**Parameters:**
- `handle` -- element handle
- `eventPtr`, `eventLen` -- event name (e.g., "click", "submit", "input")
- `callbackIdx` -- index of the handler function in the WASM exports

### setAttribute

```
dom.setAttribute(handle: i32, namePtr: i32, nameLen: i32, valPtr: i32, valLen: i32)
```

Sets an HTML attribute on the element (e.g., `class`, `id`, `placeholder`).

### setProperty

```
dom.setProperty(handle: i32, namePtr: i32, nameLen: i32, valPtr: i32, valLen: i32)
```

Sets a DOM property (not an HTML attribute) on the element. Used by two-way form bindings (`bind:value`, `bind:checked`). Boolean properties like `checked`, `disabled`, and `readOnly` are converted from string to boolean automatically.

### getProperty

```
dom.getProperty(handle: i32, namePtr: i32, nameLen: i32) -> (i32, i32)
```

Gets a DOM property value from the element. Returns a `(ptr, len)` pair pointing to the string value in WASM memory.

### lazyMount

```
dom.lazyMount(componentNamePtr: i32, componentNameLen: i32, rootHandle: i32, fallbackFnIdx: i32)
```

Mounts a lazy-loaded component. Shows the fallback content immediately, then dynamically fetches and instantiates the component's WASM chunk (from `./{componentName}.wasm`). Once loaded, the fallback is replaced with the actual component.

### errorBoundary

```
dom.errorBoundary(rootHandle: i32, mountFnIdx: i32, fallbackFnIdx: i32)
```

Wraps a component mount in an error boundary. Attempts to mount the component using `__handler_{mountFnIdx}`. If an exception is thrown, clears the failed render and mounts the fallback UI using `__handler_{fallbackFnIdx}`. Stores retry information on the element for potential recovery.

---

## signal

Reactive state management primitives. Signals are the foundation of Nectar's fine-grained reactivity system.

### create

```
signal.create(initialValue: i32) -> i32
```

Creates a new reactive signal with the given initial value. Returns a signal ID for use with `get`, `set`, and `subscribe`.

### get

```
signal.get(signalId: i32) -> i32
```

Reads the current value of a signal. If called inside an effect, the effect is automatically registered as a dependency -- it will re-run when this signal changes.

### set

```
signal.set(signalId: i32, newValue: i32)
```

Updates the signal value. If the new value differs from the current value, all subscribed effects are scheduled for re-execution.

### subscribe

```
signal.subscribe(signalId: i32, callbackIdx: i32)
```

Registers an effect that runs whenever the signal value changes. The WASM function `__effect_{callbackIdx}` is called with the new signal value as its parameter.

### createEffect

```
signal.createEffect(fnIdx: i32)
```

Creates a reactive effect. The WASM function `__effect_{fnIdx}` is called immediately to capture initial dependencies, then re-run automatically whenever any signal it reads changes.

### createMemo

```
signal.createMemo(fnIdx: i32) -> i32
```

Creates a memoized computed value. The WASM function `__memo_{fnIdx}` is called to compute the initial value. The result is cached and only recomputed when its signal dependencies change. Returns a signal ID that can be read with `signal.get`.

### batch

```
signal.batch(fnIdx: i32)
```

Executes the WASM function `__batch_{fnIdx}` inside a batch. Multiple signal updates within the batch are grouped so that dependent effects only run once after the batch completes. The batch flushes synchronously.

---

## http

HTTP communication from WASM.

### fetch

```
http.fetch(urlPtr: i32, urlLen: i32, methodPtr: i32, methodLen: i32) -> i32
```

Initiates an HTTP request. Returns a fetch ID that can be used with `fetchGetBody` and `fetchGetStatus` to retrieve the response.

**Parameters:**
- `urlPtr`, `urlLen` -- request URL
- `methodPtr`, `methodLen` -- HTTP method ("GET", "POST", etc.)

### fetchGetBody

```
http.fetchGetBody(fetchId: i32) -> (i32, i32)
```

Retrieves the response body for a completed fetch. Returns `(ptr, len)` pointing to the response body in WASM memory.

### fetchGetStatus

```
http.fetchGetStatus(fetchId: i32) -> i32
```

Returns the HTTP status code for a completed fetch.

### fetchAsync

```
http.fetchAsync(urlPtr: i32, urlLen: i32, methodPtr: i32, methodLen: i32, callbackIdx: i32)
```

Initiates an asynchronous HTTP request. When the response arrives, calls `__fetch_callback_{callbackIdx}(status, bodyPtr, bodyLen)`. On error, calls `__fetch_error_{callbackIdx}(errorPtr, errorLen)`.

---

## worker

Concurrency primitives using Web Workers.

### spawn

```
worker.spawn(funcIdx: i32) -> i32
```

Spawns a WASM function on a background Web Worker by function table index. Returns the function index. The worker pool automatically manages worker lifecycle and reuse.

### channelCreate

```
worker.channelCreate() -> i32
```

Creates a new message channel for communication between the main thread and workers. Returns a channel ID. Internally uses `MessageChannel` for cross-worker delivery.

### channelSend

```
worker.channelSend(channelId: i32, valuePtr: i32, valueLen: i32)
```

Sends a value through a channel. The value bytes are copied from WASM memory. If a receiver is waiting, the value is delivered immediately; otherwise it is buffered.

### channelRecv

```
worker.channelRecv(channelId: i32, callbackIdx: i32)
```

Receives a value from a channel asynchronously. When a value is available, calls `__channel_recv_{callbackIdx}(ptr, len)` with the value written to WASM memory. If a value is already buffered, delivery is immediate.

### parallel

```
worker.parallel(funcIndicesPtr: i32, funcIndicesLen: i32, callbackIdx: i32)
```

Runs multiple WASM functions in parallel on the worker pool. `funcIndicesPtr` points to an array of i32 function table indices. When all functions complete, calls `__parallel_done_{callbackIdx}(resultsPtr, resultsLen)` with the collected results.

If no worker pool is available, functions fall back to sequential execution on the main thread.

---

## ai

AI/LLM interaction primitives for building intelligent applications.

### chatStream

```
ai.chatStream(
    modelPtr: i32, modelLen: i32,
    messagesPtr: i32, messagesLen: i32,
    toolsPtr: i32, toolsLen: i32,
    onTokenIdx: i32,
    onToolCallIdx: i32,
    onDoneIdx: i32
)
```

Initiates a streaming chat completion. Sends a POST request to `/api/chat` with the model, message history, and tool definitions. As tokens arrive via Server-Sent Events:

- **Token callback**: `__ai_token_{onTokenIdx}(ptr, len)` is called with each content token
- **Tool call callback**: `__ai_tool_call_{onToolCallIdx}(ptr, len)` is called with the tool call JSON. The runtime automatically dispatches the tool call to the registered WASM function and feeds the result back
- **Done callback**: `__ai_done_{onDoneIdx}()` is called when the stream ends

### chatComplete

```
ai.chatComplete(
    modelPtr: i32, modelLen: i32,
    messagesPtr: i32, messagesLen: i32,
    callbackIdx: i32
)
```

Non-streaming chat completion. Sends the request and waits for the full response. Calls `__ai_complete_{callbackIdx}(ptr, len)` with the complete response content.

### registerTool

```
ai.registerTool(
    namePtr: i32, nameLen: i32,
    descPtr: i32, descLen: i32,
    schemaPtr: i32, schemaLen: i32,
    funcIdx: i32
)
```

Registers a tool that the AI model can call. The tool body is a WASM-exported function at `funcIdx`. Parameters:

- `name` -- tool name (matches what the AI sees)
- `description` -- human-readable description
- `schema` -- JSON schema for the tool parameters
- `funcIdx` -- WASM function table index

### embed

```
ai.embed(textPtr: i32, textLen: i32, callbackIdx: i32)
```

Generates an embedding vector for the given text. Sends a POST to `/api/embed`. Calls `__ai_embed_{callbackIdx}(ptr, len)` with a Float32Array of the embedding written to WASM memory.

### parseStructured

```
ai.parseStructured(responsePtr: i32, responseLen: i32, schemaPtr: i32, schemaLen: i32) -> i32
```

Parses an AI response string as structured JSON data. Returns a pointer to the parsed JSON string in WASM memory, or 0 (null) on parse failure.

---

## streaming

Streaming data sources: fetch streams, Server-Sent Events, and WebSockets.

### streamFetch

```
streaming.streamFetch(urlPtr: i32, urlLen: i32, callbackIdx: i32)
```

Creates a streaming fetch that processes the response body as a `ReadableStream`. For each chunk:

- **Chunk callback**: `__stream_chunk_{callbackIdx}(ptr, len)` is called with the decoded text
- **Done callback**: `__stream_done_{callbackIdx}()` is called when the stream ends

### sseConnect

```
streaming.sseConnect(urlPtr: i32, urlLen: i32, callbackIdx: i32)
```

Connects to a Server-Sent Events endpoint. Each incoming event triggers `__stream_chunk_{callbackIdx}(ptr, len)` with the event data. On error or close, `__stream_done_{callbackIdx}()` is called.

### wsConnect

```
streaming.wsConnect(urlPtr: i32, urlLen: i32, callbackIdx: i32) -> i32
```

Opens a WebSocket connection. Returns a WebSocket handle ID. Incoming messages trigger `__stream_chunk_{callbackIdx}(ptr, len)`. On close, `__stream_done_{callbackIdx}()` is called.

### wsSend

```
streaming.wsSend(wsId: i32, dataPtr: i32, dataLen: i32)
```

Sends a text message through an open WebSocket connection.

### wsClose

```
streaming.wsClose(wsId: i32)
```

Closes a WebSocket connection.

### yield

```
streaming.yield(dataPtr: i32, dataLen: i32)
```

Emits a value from a WASM-originated stream. Used internally when Nectar code uses the `yield` statement inside a streaming context.

---

## media

Image and resource loading utilities.

### lazyImage

```
media.lazyImage(srcPtr: i32, srcLen: i32, placeholderPtr: i32, placeholderLen: i32, elementHandle: i32)
```

Implements lazy image loading with `IntersectionObserver`. The placeholder image is shown immediately. When the element scrolls into view (with a 200px root margin), the full image source is loaded.

Falls back to immediate loading if `IntersectionObserver` is not available.

### decodeImage

```
media.decodeImage(srcPtr: i32, srcLen: i32, callbackIdx: i32)
```

Decodes an image off the main thread using `createImageBitmap`. When decoding completes, calls `__media_decoded_{callbackIdx}(handle)` with an element handle for the decoded bitmap.

### preload

```
media.preload(urlPtr: i32, urlLen: i32, typePtr: i32, typeLen: i32)
```

Preloads a critical resource by injecting a `<link rel="preload">` element into the document head. The `type` parameter maps to the `as` attribute (e.g., "image", "script", "style", "font").

### progressiveImage

```
media.progressiveImage(thumbPtr: i32, thumbLen: i32, fullPtr: i32, fullLen: i32, elementHandle: i32)
```

Implements progressive image loading (blur-up technique). The tiny thumbnail is shown immediately with a CSS blur filter. Once the full-resolution image loads in the background, it replaces the thumbnail and the blur is removed with a smooth transition.

---

## router

Client-side URL routing.

### init

```
router.init(routesPtr: i32, routesLen: i32)
```

Initializes the router. Sets up the `popstate` event listener for browser back/forward navigation and matches the initial URL against registered routes.

### navigate

```
router.navigate(pathPtr: i32, pathLen: i32)
```

Programmatically navigates to a new URL path. Uses `history.pushState` to update the browser URL without a full page reload, then matches the new path against routes and mounts the corresponding component.

### currentPath

```
router.currentPath() -> i32
```

Returns a pointer to the current URL path string in WASM memory.

### getParam

```
router.getParam(namePtr: i32, nameLen: i32) -> i32
```

Returns the value of a named route parameter (extracted from `:param` segments in the route pattern). Returns a pointer to the parameter value string in WASM memory.

### registerRoute

```
router.registerRoute(pathPtr: i32, pathLen: i32, mountFnIdx: i32)
```

Registers a route pattern with a mount function index. The pattern supports:

- **Static segments**: `"/about"` -- exact match
- **Parameters**: `"/user/:id"` -- captures `id`
- **Wildcards**: `"/admin/*"` -- matches anything under `/admin/`

---

## style

Scoped CSS injection and management.

### isCriticalLoaded

```
style.isCriticalLoaded() -> i32
```

Checks whether critical CSS has already been inlined by the server during SSR with `--critical-css`. Returns `1` if `window.__nectarCriticalLoaded` is set, `0` otherwise.

This is used internally by the runtime to avoid double-injecting styles that the SSR pass has already inlined in a `<style data-nectar-critical>` tag.

### injectStyles

```
style.injectStyles(componentNamePtr: i32, componentNameLen: i32, cssPtr: i32, cssLen: i32) -> i32
```

Injects scoped CSS for a component. The runtime:

1. Generates a unique scope ID from the component name (hash-based)
2. **If critical CSS was inlined by SSR** (`window.__nectarCriticalLoaded` is set), checks whether this component's scoped styles already exist in the `<style data-nectar-critical>` tag. If so, skips injection and returns the scope ID immediately -- avoiding double-injection of styles.
3. Prefixes all CSS selectors with `[data-nectar-HASH]` for scoping
4. Creates a `<style>` element in the document head
5. Removes any existing style for the same component (supporting hot reload)

Returns a pointer to the scope ID string in WASM memory.

### applyScope

```
style.applyScope(elementHandle: i32, scopeIdPtr: i32, scopeIdLen: i32)
```

Applies a scope ID to a DOM element by setting a `data-nectar-HASH` attribute. This ensures the scoped CSS selectors match only elements within this component.

---

## animation

Web Animations API bridge for transitions and keyframe animations.

### registerTransition

```
animation.registerTransition(
    elementId: i32,
    propertyPtr: i32, propertyLen: i32,
    durationPtr: i32, durationLen: i32,
    easingPtr: i32, easingLen: i32
)
```

Registers a CSS transition on an element by setting its `style.transition` property. Multiple transitions can be registered on the same element (they are appended with commas).

### registerKeyframes

```
animation.registerKeyframes(
    namePtr: i32, nameLen: i32,
    keyframesJsonPtr: i32, keyframesJsonLen: i32
)
```

Registers a named keyframe animation. The keyframes are provided as a JSON array of objects with `offset` and CSS properties. The animation can later be played with `animation.play`.

### play

```
animation.play(
    elementId: i32,
    namePtr: i32, nameLen: i32,
    durationPtr: i32, durationLen: i32
)
```

Plays a registered animation on an element using `Element.animate()`. Supports duration strings like `"0.5s"` or `"500ms"`. The animation fills forward by default.

### pause

```
animation.pause(elementId: i32)
```

Pauses all active animations on an element, including both tracked animations and any animations returned by `Element.getAnimations()`.

### cancel

```
animation.cancel(elementId: i32)
```

Cancels all active animations on an element and clears the tracking state.

### onFinish

```
animation.onFinish(elementId: i32, callbackIndex: i32)
```

Registers a callback for when the most recent animation on an element finishes. Calls `__handler_{callbackIndex}()` when the animation completes.

---

## a11y

Accessibility utilities for building inclusive applications.

### setAriaAttribute

```
a11y.setAriaAttribute(elementId: i32, namePtr: i32, nameLen: i32, valuePtr: i32, valueLen: i32)
```

Sets any `aria-*` attribute on an element. The `name` parameter should include the `aria-` prefix (e.g., `"aria-label"`, `"aria-hidden"`).

### setRole

```
a11y.setRole(elementId: i32, rolePtr: i32, roleLen: i32)
```

Sets the `role` attribute on an element (e.g., `"button"`, `"navigation"`, `"dialog"`).

### manageFocus

```
a11y.manageFocus(elementId: i32)
```

Programmatically moves focus to an element. If the element is not natively focusable (not `<input>`, `<button>`, `<a>`, `<select>`, or `<textarea>`), a `tabindex="-1"` attribute is added automatically before focusing.

### announceToScreenReader

```
a11y.announceToScreenReader(textPtr: i32, textLen: i32, priority: i32)
```

Announces text to screen readers using an `aria-live` region. The priority parameter controls urgency:

- `0` -- polite (waits for current speech to finish)
- `1` -- assertive (interrupts current speech)

The runtime maintains hidden `aria-live` regions in the DOM. Text is cleared and re-set to trigger a fresh announcement.

### trapFocus

```
a11y.trapFocus(containerElementId: i32)
```

Creates a focus trap within a container element (useful for modals and dialogs). When the user presses Tab at the last focusable element, focus wraps to the first; Shift+Tab at the first element wraps to the last. The first focusable element inside the container receives focus immediately.

Focusable elements include: `a[href]`, `button:not([disabled])`, `textarea:not([disabled])`, `input:not([disabled])`, `select:not([disabled])`, and `[tabindex]:not([tabindex="-1"])`.

### releaseFocusTrap

```
a11y.releaseFocusTrap()
```

Releases the current focus trap, allowing normal tab navigation.

---

## webapi

General web platform APIs.

### localStorage

```
webapi.localStorageGet(keyPtr: i32, keyLen: i32) -> (i32, i32)
webapi.localStorageSet(keyPtr: i32, keyLen: i32, valPtr: i32, valLen: i32)
webapi.localStorageRemove(keyPtr: i32, keyLen: i32)
```

Read, write, and delete values in `localStorage`. `localStorageGet` returns `(ptr, len)` of the stored value (empty string if not found).

### sessionStorage

```
webapi.sessionStorageGet(keyPtr: i32, keyLen: i32) -> (i32, i32)
webapi.sessionStorageSet(keyPtr: i32, keyLen: i32, valPtr: i32, valLen: i32)
```

Read and write values in `sessionStorage`.

### Clipboard

```
webapi.clipboardWrite(textPtr: i32, textLen: i32)
webapi.clipboardRead(callbackIdx: i32)
```

Write text to the system clipboard (`navigator.clipboard.writeText`) and read text from the clipboard. `clipboardRead` is asynchronous -- calls `__clipboard_read_{callbackIdx}(ptr, len)` when the text is available.

### Timers

```
webapi.setTimeout(callbackIdx: i32, delayMs: i32) -> i32
webapi.setInterval(callbackIdx: i32, intervalMs: i32) -> i32
webapi.clearTimer(timerId: i32)
```

Standard timer functions. Callbacks invoke `__timer_{callbackIdx}()`. `clearTimer` clears both timeouts and intervals.

### URL and History

```
webapi.getLocationHref() -> (i32, i32)
webapi.getLocationSearch() -> (i32, i32)
webapi.getLocationHash() -> (i32, i32)
webapi.pushState(urlPtr: i32, urlLen: i32)
webapi.replaceState(urlPtr: i32, urlLen: i32)
```

Access the current URL components and manipulate browser history.

### Console

```
webapi.consoleLog(msgPtr: i32, msgLen: i32)
webapi.consoleWarn(msgPtr: i32, msgLen: i32)
webapi.consoleError(msgPtr: i32, msgLen: i32)
```

Log messages to the browser developer console at different severity levels.

### Miscellaneous

```
webapi.randomFloat() -> f64
```

Returns a cryptographically random float between 0 and 1 (using `crypto.getRandomValues` when available, falling back to `Math.random`).

```
webapi.now() -> f64
```

Returns a high-resolution timestamp in milliseconds (using `performance.now` when available, falling back to `Date.now`).

```
webapi.requestAnimationFrame(callbackIdx: i32) -> i32
```

Schedules a callback before the next repaint. Calls `__raf_{callbackIdx}()`. Returns the animation frame request ID.

---

## string

String manipulation functions for building strings in WASM linear memory.

### concat

```
string.concat(ptr1: i32, len1: i32, ptr2: i32, len2: i32) -> (i32, i32)
```

Concatenates two strings. Returns `(ptr, len)` of the resulting string in WASM memory.

### fromI32

```
string.fromI32(value: i32) -> (i32, i32)
```

Converts an `i32` integer to its decimal string representation.

### fromF64

```
string.fromF64(value: f64) -> (i32, i32)
```

Converts an `f64` float to its string representation.

### fromBool

```
string.fromBool(value: i32) -> (i32, i32)
```

Converts a boolean (i32: 0 or 1) to `"true"` or `"false"`.

---

## test

Testing support functions used by the test runner.

### pass

Reports a test as passed.

### fail

Reports a test as failed with an error message.

### summary

Prints the test summary (total passed, total failed).

Note: The current test runner validates tests through the full compilation pipeline (lex, parse, borrow check, type check, codegen) rather than executing WASM. Tests that compile without errors are reported as passing.

---

## sw

Service worker management for offline-first applications. Arc ships a built-in service worker (`nectar-service-worker.js`) and client registration script (`nectar-sw-register.js`).

### register

```
sw.register()
```

Registers the Arc service worker at `/nectar-sw.js`. If the `NectarSW` client library is loaded on the page, delegates to it for update detection and lifecycle management. Otherwise, performs a direct `navigator.serviceWorker.register()` call.

### precache

```
sw.precache(urlPtr: i32, urlLen: i32)
```

Adds a URL to the precache list. If a service worker is already active, sends it a message to cache the URL immediately. The URL is also stored on the runtime instance for compile-time manifest generation.

**Parameters:**
- `urlPtr` -- pointer to the URL string in WASM memory
- `urlLen` -- length of the URL string

### isOffline

```
sw.isOffline() -> i32
```

Returns the current offline status. Uses `NectarSW.isOffline` if the client library is loaded, otherwise falls back to `navigator.onLine`.

**Returns:**
- `1` if the browser is offline
- `0` if the browser is online

### Service Worker Features

The built-in service worker (`nectar-service-worker.js`) provides:

- **Cache-first for app shell** -- HTML, CSS, JS, images, and fonts are served from cache when available
- **Network-first for API calls** -- requests to `/api/*` try the network first, falling back to cached responses
- **Aggressive WASM caching** -- `.wasm` files are stored in a dedicated cache and served cache-first
- **Offline fallback** -- navigation requests that fail show a cached offline page
- **Auto-versioning** -- the `CACHE_VERSION` constant is stamped by the compiler at build time; old caches are purged on activation

### Client Registration Script

The client script (`nectar-sw-register.js`) exposes a global `NectarSW` object:

- `NectarSW.register(swUrl?)` -- registers the service worker (default path: `/nectar-sw.js`)
- `NectarSW.update()` -- forces a waiting service worker to activate (triggers page reload)
- `NectarSW.updateAvailable` -- boolean, true when a new version is waiting
- `NectarSW.isOffline` -- boolean, reactive offline status
- `NectarSW.on("update", fn)` -- listen for update availability
- `NectarSW.on("offline", fn)` -- listen for online/offline state changes
