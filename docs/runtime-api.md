# Nectar Runtime API

This document describes the complete runtime layer that bridges Nectar's WASM modules to browser APIs. Understanding this architecture is critical for compiler development and debugging.

## Architecture

Nectar's runtime is a single JavaScript file (`runtime/modules/core.js`, ~660 lines) that provides browser API syscalls to WASM. **All logic runs in WASM.** JavaScript functions are 1-3 lines each — pure bridges with zero computation.

### What Lives Where

| Layer | Runs in | Examples |
|---|---|---|
| **Application logic** | WASM | Components, stores, routing, validation, crypto, formatting |
| **Reactive system** | WASM | Signals, effects, memos, dependency tracking, batching |
| **String operations** | WASM | Concat, fromI32, fromF64, fromBool — all in linear memory |
| **Animation math** | WASM | Spring physics, easing, interpolation |
| **Browser syscalls** | JS (core.js) | DOM, fetch, WebSocket, IndexedDB, timers, clipboard |

### Key Patterns

**Command Buffer (mount/flush):**
- Initial render: WASM builds an HTML string in linear memory → single `mount()` call sets `innerHTML`
- Updates: WASM writes opcodes into a command buffer in linear memory → single `flush()` call per animation frame reads and executes them all
- This collapses ~50 individual WASM→JS boundary crossings into 1-2 per frame

**`__readOpts` pattern:** WASM writes flat (key_ptr, key_len, val_ptr, val_len) tuples terminated by (0,0) into linear memory. JS reads them to build option objects for browser APIs. Replaces all JSON.parse/stringify. Zero serialization overhead.

**Typed setters:** For complex APIs like fetch, WASM calls `setMethod()`, `setBody()`, `addHeader()` individually before triggering the call. No option object serialization.

**Callbacks:** WASM registers callback indices. JS calls `R.__cb(cbIdx)` when async operations complete. Data is written to WASM linear memory before the callback fires.

**Element registry:** DOM elements are registered by integer ID. WASM refers to elements by ID, never by reference. `R.__registerElement(el)` returns an integer handle.

---

## Flush Opcodes

The command buffer uses these opcodes for batched DOM updates:

| Opcode | Value | Operation |
|---|---|---|
| `OP_SET_TEXT` | 1 | Set textContent on element |
| `OP_SET_ATTR` | 2 | Set attribute (name, value) |
| `OP_REMOVE_ATTR` | 3 | Remove attribute |
| `OP_APPEND_CHILD` | 4 | Append child element |
| `OP_REMOVE_CHILD` | 5 | Remove child element |
| `OP_INSERT_BEFORE` | 6 | Insert element before reference |
| `OP_SET_STYLE` | 7 | Set inline style property |
| `OP_CLASS_ADD` | 8 | Add CSS class |
| `OP_CLASS_REMOVE` | 9 | Remove CSS class |
| `OP_CLASS_TOGGLE` | 10 | Toggle CSS class |
| `OP_SET_INNER_HTML` | 11 | Set innerHTML |
| `OP_ADD_EVENT` | 12 | Add event listener |
| `OP_REMOVE_EVENT` | 13 | Remove event listener |
| `OP_FOCUS` | 14 | Focus element |
| `OP_BLUR` | 15 | Blur element |
| `OP_SET_PROPERTY` | 16 | Set DOM property (value, checked, etc.) |

---

## Namespaces

core.js exports 16 namespaces via `wasmImports`. Each namespace contains only functions that call browser APIs WASM physically cannot invoke.

### dom

DOM manipulation, element queries, and rendering.

| Function | Parameters | Browser API |
|---|---|---|
| `mount` | containerElId, htmlPtr, htmlLen | `el.innerHTML = html` |
| `hydrateRefs` | containerElId | `querySelectorAll('[data-nid]')` |
| `flush` | bufPtr, bufLen | Reads opcode buffer, executes batched DOM ops |
| `getElementById` | ptr, len | `document.getElementById()` |
| `querySelector` | ptr, len | `document.querySelector()` |
| `createElement` | ptr, len | `document.createElement()` |
| `createTextNode` | ptr, len | `document.createTextNode()` |
| `getBody` | — | `document.body` |
| `getHead` | — | `document.head` |
| `getRoot` | — | `document.getElementById('app')` or `document.body` |
| `getDocumentElement` | — | `document.documentElement` |
| `addEventListener` | elId, evtPtr, evtLen, cbIdx | `el.addEventListener()` — marshals event data (clientX/Y, keyCode, modifiers, key, dataTransfer) |
| `removeEventListener` | elId, evtPtr, evtLen, cbIdx | `el.removeEventListener()` |
| `lazyMount` | containerElId, urlPtr, urlLen, cbIdx | `import(url)` dynamic module loading |
| `setTitle` | ptr, len | `document.title = str` |
| `getScrollTop` | elId | `el.scrollTop` |
| `getScrollLeft` | elId | `el.scrollLeft` |
| `getClientHeight` | elId | `el.clientHeight` |
| `getClientWidth` | elId | `el.clientWidth` |
| `getWindowWidth` | — | `window.innerWidth` |
| `getWindowHeight` | — | `window.innerHeight` |
| `getOuterHtml` | — | `document.documentElement.outerHTML` |
| `setDragData` | fmtPtr, fmtLen, dataPtr, dataLen | `dataTransfer.setData()` |
| `getDragData` | fmtPtr, fmtLen | `dataTransfer.getData()` |
| `preventDefault` | — | `event.preventDefault()` |
| `loadScript` | urlPtr, urlLen, cbIdx | Creates `<script>` element, sets src |
| `loadChunk` | urlPtr, urlLen | Creates `<script>`, returns promise ID |
| `decodeImage` | srcPtr, srcLen, cbIdx | `new Image()`, `img.decode()` |
| `progressiveImage` | elId, lowPtr, lowLen, highPtr, highLen | Sets low-res src, swaps to high-res on load |
| `print` | elId | `contentWindow.print()` |
| `reloadModule` | urlPtr, urlLen, cbIdx | `fetch()` → `WebAssembly.compile()` → `WebAssembly.instantiate()` |
| `download` | dataPtr, dataLen, namePtr, nameLen | `Blob` → `URL.createObjectURL()` → `<a>` click → `URL.revokeObjectURL()` |

### timer

Timing and animation frame scheduling.

| Function | Parameters | Browser API |
|---|---|---|
| `setTimeout` | cbIdx, ms | `setTimeout()` |
| `clearTimeout` | id | `clearTimeout()` |
| `setInterval` | cbIdx, ms | `setInterval()` |
| `clearInterval` | id | `clearInterval()` |
| `requestAnimationFrame` | cbIdx | `requestAnimationFrame()` |
| `cancelAnimationFrame` | id | `cancelAnimationFrame()` |
| `now` | — | `performance.now()` |

### webapi

General browser APIs — storage, clipboard, location, history, console, sharing, performance.

| Function | Parameters | Browser API |
|---|---|---|
| `localStorageGet` | keyPtr, keyLen | `localStorage.getItem()` |
| `localStorageSet` | keyPtr, keyLen, valPtr, valLen | `localStorage.setItem()` |
| `localStorageRemove` | keyPtr, keyLen | `localStorage.removeItem()` |
| `sessionStorageGet` | keyPtr, keyLen | `sessionStorage.getItem()` |
| `sessionStorageSet` | keyPtr, keyLen, valPtr, valLen | `sessionStorage.setItem()` |
| `clipboardWrite` | ptr, len | `navigator.clipboard.writeText()` |
| `clipboardRead` | cbIdx | `navigator.clipboard.readText()` |
| `getLocationHref` | — | `location.href` |
| `getLocationSearch` | — | `location.search` |
| `getLocationHash` | — | `location.hash` |
| `getLocationPathname` | — | `location.pathname` |
| `pushState` | urlPtr, urlLen | `history.pushState()` |
| `replaceState` | urlPtr, urlLen | `history.replaceState()` |
| `consoleLog` | ptr, len | `console.log()` |
| `consoleWarn` | ptr, len | `console.warn()` |
| `consoleError` | ptr, len | `console.error()` |
| `onPopState` | cbIdx | `addEventListener('popstate')` |
| `envGet` | namePtr, nameLen | `window.__env[name]` |
| `canShare` | — | `!!navigator.share` |
| `nativeShare` | titlePtr, titleLen, textPtr, textLen, urlPtr, urlLen | `navigator.share()` |
| `perfMark` | namePtr, nameLen | `performance.mark()` |
| `perfMeasure` | namePtr, nameLen, startPtr, startLen, endPtr, endLen | `performance.measure()` |

### http

HTTP communication via typed setters — no serialization.

| Function | Parameters | Browser API |
|---|---|---|
| `setMethod` | ptr, len | Stores method string for next fetch |
| `setBody` | ptr, len | Stores body string for next fetch |
| `addHeader` | keyPtr, keyLen, valPtr, valLen | Adds to headers for next fetch |
| `fetch` | urlPtr, urlLen | `fetch()` with stored method/body/headers, returns promise ID |

**Pattern:** WASM calls `setMethod("POST")`, `addHeader("Content-Type", "application/json")`, `setBody(json)`, then `fetch(url)`. Each call is a simple string store — zero serialization overhead.

### observe

Intersection, resize, and mutation observers.

| Function | Parameters | Browser API |
|---|---|---|
| `matchMedia` | queryPtr, queryLen | `matchMedia()` — returns boolean |
| `intersectionObserver` | cbIdx, optsPtr | `new IntersectionObserver()` — uses `__readOpts` for options |
| `observe` | obsId, elId | `observer.observe(el)` |
| `unobserve` | obsId, elId | `observer.unobserve(el)` |
| `disconnect` | obsId | `observer.disconnect()` |

### ws

WebSocket connections.

| Function | Parameters | Browser API |
|---|---|---|
| `connect` | urlPtr, urlLen | `new WebSocket(url)` |
| `send` | wsId, dataPtr, dataLen | `ws.send(str)` |
| `sendBinary` | wsId, ptr, len | `ws.send(Uint8Array)` |
| `close` | wsId | `ws.close()` |
| `closeWithCode` | wsId, code, reasonPtr, reasonLen | `ws.close(code, reason)` |
| `onOpen` | wsId, cbIdx | `ws.addEventListener('open')` |
| `onMessage` | wsId, cbIdx | `ws.addEventListener('message')` — writes data to WASM memory |
| `onClose` | wsId, cbIdx | `ws.addEventListener('close')` |
| `onError` | wsId, cbIdx | `ws.addEventListener('error')` |
| `getReadyState` | wsId | `ws.readyState` |

### db

IndexedDB operations.

| Function | Parameters | Browser API |
|---|---|---|
| `open` | namePtr, nameLen, version, cbIdx | `indexedDB.open()` — creates 'default' store on upgrade |
| `put` | dbId, storePtr, storeLen, keyPtr, keyLen, valPtr, valLen | `objectStore.put()` |
| `get` | dbId, storePtr, storeLen, keyPtr, keyLen, cbIdx | `objectStore.get()` |
| `delete` | dbId, storePtr, storeLen, keyPtr, keyLen | `objectStore.delete()` |
| `getAll` | dbId, storePtr, storeLen, cbIdx | `objectStore.getAll()` |

### worker

Web Workers and message channels.

| Function | Parameters | Browser API |
|---|---|---|
| `spawn` | codePtr, codeLen | `new Worker(Blob URL)` from code string |
| `channelCreate` | — | `new MessageChannel()` — returns port1 ID |
| `channelSend` | portId, dataPtr, dataLen | `port.postMessage()` |
| `channelRecv` | portId, cbIdx | `port.onmessage`, `port.start()` |
| `parallel` | fnPtrsPtr, fnCount, cbIdx | Callback dispatch (stub) |
| `await` | promiseId | Returns promise object |
| `postMessage` | workerId, dataPtr, dataLen | `worker.postMessage()` |
| `onMessage` | workerId, cbIdx | `worker.addEventListener('message')` |
| `terminate` | workerId | `worker.terminate()` |

### pwa

Service workers, push notifications, and caching.

| Function | Parameters | Browser API |
|---|---|---|
| `cachePrecache` | namePtr, nameLen | `caches.open()` |
| `registerPush` | optsPtr | `pushManager.subscribe()` — uses `__readOpts` for options |
| `registerServiceWorker` | pathPtr, pathLen, cbIdx | `navigator.serviceWorker.register()` |

### hardware

Device hardware access.

| Function | Parameters | Browser API |
|---|---|---|
| `haptic` | pattern | `navigator.vibrate()` |
| `biometricAuth` | optsPtr, successCb, failCb | `navigator.credentials.get()` — uses `__readOpts` |
| `cameraCapture` | constraintsPtr, cbIdx | `navigator.mediaDevices.getUserMedia()` — uses `__readOpts` |
| `geolocationCurrent` | cbIdx | `navigator.geolocation.getCurrentPosition()` — writes lat/lon to WASM memory |

### payment

Payment processing via sandboxed iframes.

| Function | Parameters | Browser API |
|---|---|---|
| `processPayment` | elId, msgPtr, msgLen, cbIdx | `contentWindow.postMessage()`, listens for response via `window.addEventListener('message')` |

### auth

OAuth and credential management.

| Function | Parameters | Browser API |
|---|---|---|
| `login` | urlPtr, urlLen | `location.href = url` (redirect to OAuth provider) |
| `logout` | urlPtr, urlLen | `location.href = url` (redirect to logout endpoint) |
| `getRawCookies` | — | `document.cookie` |
| `setCookie` | ptr, len | `document.cookie = str` |

### upload

File input and transfer.

| Function | Parameters | Browser API |
|---|---|---|
| `init` | acceptPtr, acceptLen, multiple, cbIdx | Creates `<input type="file">`, calls `click()` |
| `start` | urlPtr, urlLen, cbIdx | `new XMLHttpRequest()`, opens POST, tracks `progress`/`load` events |
| `cancel` | xhrId | `xhr.abort()` |

### time

Locale-aware date/time formatting (the only valid use of `Intl` from JS).

| Function | Parameters | Browser API |
|---|---|---|
| `now` | — | `Date.now()` |
| `format` | ms, localePtr, localeLen | `new Intl.DateTimeFormat(locale).format()` |
| `getTimezoneOffset` | — | `new Date().getTimezoneOffset()` |
| `formatDate` | ms, localePtr, localeLen, optsPtr | `new Intl.DateTimeFormat(locale, opts).format()` — uses `__readOpts` |

### streaming

Server-Sent Events and streaming fetch.

| Function | Parameters | Browser API |
|---|---|---|
| `streamFetch` | urlPtr, urlLen, cbIdx | `fetch()` → `body.getReader()` → pumps chunks with `read()` |
| `sseConnect` | urlPtr, urlLen, cbIdx | `new EventSource(url)`, listens for `message` events |

### rtc

WebRTC peer connections, data channels, and media tracks.

| Function | Parameters | Browser API |
|---|---|---|
| `createPeer` | optsPtr | `new RTCPeerConnection(opts)` |
| `createPeerWithIce` | urlsPtr, urlsLen | `new RTCPeerConnection({iceServers:[{urls}]})` |
| `createOffer` | pcId, cbIdx | `pc.createOffer()` |
| `createAnswer` | pcId, cbIdx | `pc.createAnswer()` |
| `setLocalDescription` | pcId, typePtr, typeLen, sdpPtr, sdpLen, cbIdx | `pc.setLocalDescription()` |
| `setRemoteDescription` | pcId, typePtr, typeLen, sdpPtr, sdpLen, cbIdx | `pc.setRemoteDescription()` |
| `addIceCandidate` | pcId, candPtr, candLen, midPtr, midLen, cbIdx | `pc.addIceCandidate()` |
| `createDataChannel` | pcId, labelPtr, labelLen, optsPtr | `pc.createDataChannel()` |
| `dataChannelSend` | dcId, dataPtr, dataLen | `dc.send(string)` |
| `dataChannelSendBinary` | dcId, ptr, len | `dc.send(Uint8Array)` |
| `dataChannelClose` | dcId | `dc.close()` |
| `dataChannelGetState` | dcId | `dc.readyState` |
| `onDataChannelMessage` | dcId, cbIdx | `dc.onmessage` |
| `onDataChannelOpen` | dcId, cbIdx | `dc.onopen` |
| `onDataChannelClose` | dcId, cbIdx | `dc.onclose` |
| `addTrack` | pcId, trackId, streamId | `pc.addTrack()` |
| `removeTrack` | pcId, senderId | `pc.removeTrack()` |
| `getStats` | pcId, cbIdx | `pc.getStats()` |
| `close` | pcId | `pc.close()` |
| `onIceCandidate` | pcId, cbIdx | `pc.onicecandidate` |
| `onIceCandidateFull` | pcId, cbIdx | `pc.onicecandidate` (full candidate object) |
| `onTrack` | pcId, cbIdx | `pc.ontrack` |
| `onDataChannel` | pcId, cbIdx | `pc.ondatachannel` |
| `onConnectionStateChange` | pcId, cbIdx | `pc.onconnectionstatechange` |
| `onIceConnectionStateChange` | pcId, cbIdx | `pc.oniceconnectionstatechange` |
| `onIceGatheringStateChange` | pcId, cbIdx | `pc.onicegatheringstatechange` |
| `onSignalingStateChange` | pcId, cbIdx | `pc.onsignalingstate` |
| `onNegotiationNeeded` | pcId, cbIdx | `pc.onnegotiationneeded` |
| `getConnectionState` | pcId | `pc.connectionState` |
| `getIceConnectionState` | pcId | `pc.iceConnectionState` |
| `getSignalingState` | pcId | `pc.signalingState` |
| `attachStream` | elId, streamId | `el.srcObject = stream` |
| `getUserMedia` | optsPtr, cbIdx | `navigator.mediaDevices.getUserMedia()` |
| `getDisplayMedia` | optsPtr, cbIdx | `navigator.mediaDevices.getDisplayMedia()` |
| `stopTrack` | trackId | `track.stop()` |
| `setTrackEnabled` | trackId, enabled | `track.enabled = bool` |
| `getTrackKind` | trackId | `track.kind` |

---

## WASM-Internal Features (No JS)

These features run entirely in WASM with zero JavaScript involvement:

- **Reactive signals** — dependency graph, effect scheduling, batching, memos
- **String runtime** — concat, fromI32, fromF64, fromBool, toString
- **Router matching** — URL parsing, pattern matching, parameter extraction
- **Form validation** — schema checking, field validation, error collection
- **Crypto** — SHA-256/512/384/1, AES-GCM/CBC/CTR, Ed25519, HMAC-SHA256/512, PBKDF2, HKDF, ECDH, UUID v4, random bytes — all pure WASM, zero JS
- **Formatting** — number/currency/date formatting with compiled-in locale tables
- **Collections** — BTreeMap, HashSet, Vec, etc.
- **BigDecimal** — arbitrary precision arithmetic
- **URL parsing** — RFC 3986 compliant parser
- **Fuzzy search** — approximate string matching
- **Feature flags** — compile-time constants in WASM data section
- **Caching logic** — LRU, TTL, invalidation
- **Permissions enforcement** — capability checks
- **Animation math** — spring physics, easing functions, interpolation
- **Gesture recognition** — velocity, direction, distance calculations
- **Virtual scroll** — visible range calculation
- **Style injection** — scoped CSS generation
- **A11y** — ARIA attribute management, focus trapping
- **Theming** — CSS custom property generation
- **SEO** — meta tag generation, JSON-LD, sitemap
- **Tracing/observability** — performance marks, spans
- **State management** — atomic operations on shared memory
- **Chart** — line, bar, pie, scatter chart generation
- **CSV** — parse, stringify, typed parsing, export
- **Date picker** — date selection, formatting, range validation
- **Toast** — notification queue, positioning, auto-dismiss
- **Debounce/throttle** — rate limiting for event handlers
- **Pagination** — page calculation, offset/limit generation
- **Input masking** — phone, currency, custom format masks

---

## Exported Functions

core.js also exports:

```javascript
export function instantiate(wasmUrl)  // Load and initialize a WASM module
export function hydrate(wasmInstance, rootElement)  // Attach WASM to server-rendered DOM
```

`instantiate()` is the main entry point — fetches the WASM file, compiles it, creates the runtime bridge, and calls the module's `main()` export.

`hydrate()` is the SSR entry point — registers the root element and calls the module's `__hydrate()` export if present.
