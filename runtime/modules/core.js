// runtime/modules/core.js — Nectar unified syscall layer
// ONE file. ONLY browser APIs that WASM physically cannot call.
// Everything else (logic, state, routing, components, formatting, crypto) is pure Rust/WASM.
//
// WASM-internal (no JS bridge): signal, string, flags, cache, permissions, form, lifecycle,
// contract, gesture, shortcuts, virtual scroll, style injection, animation, a11y, theme,
// seo, trace, dnd, media lazy/preload, router matching, mem ops, atomic state, animate
// (rAF + SET_STYLE opcodes), pdf (createElement + srcdoc + dom.print), io (dom.download),
// env (dom.envGet), share/perf (merged into webapi), random (WASM PRNG), URL parsing (WASM)
//
// DOM strategy:
//   - Initial render: WASM builds HTML string in linear memory, single mount() sets innerHTML
//   - Updates: WASM writes batched opcodes into linear memory, single flush() call per frame
//   - This collapses ~50 individual WASM→JS boundary crossings into 1-2 per frame

// ── Flush opcodes ────────────────────────────────────────────────────────────
const OP_SET_TEXT       = 1;
const OP_SET_ATTR       = 2;
const OP_REMOVE_ATTR    = 3;
const OP_APPEND_CHILD   = 4;
const OP_REMOVE_CHILD   = 5;
const OP_INSERT_BEFORE  = 6;
const OP_SET_STYLE      = 7;
const OP_CLASS_ADD      = 8;
const OP_CLASS_REMOVE   = 9;
const OP_CLASS_TOGGLE   = 10;
const OP_SET_INNER_HTML = 11;
const OP_ADD_EVENT      = 12;
const OP_REMOVE_EVENT   = 13;
const OP_FOCUS          = 14;
const OP_BLUR           = 15;
const OP_SET_PROPERTY   = 16;

// ── Runtime object pool ──────────────────────────────────────────────────────
const NectarRuntime = {
  __elements: [null],   // index 0 = null sentinel
  __objects: [null],     // WebSocket, Worker, XHR, IDB, Observer, etc.
  __callbacks: [],
  __memory: null,
  __instance: null,
  __decoder: new TextDecoder(),
  __encoder: new TextEncoder(),

  __registerElement(el) {
    if (!el) return 0;
    this.__elements.push(el);
    return this.__elements.length - 1;
  },
  __getElement(id) { return this.__elements[id]; },

  __registerObject(obj) {
    if (!obj) return 0;
    this.__objects.push(obj);
    return this.__objects.length - 1;
  },
  __getObject(id) { return this.__objects[id]; },

  __getString(ptr, len) {
    return this.__decoder.decode(new Uint8Array(this.__memory.buffer, ptr, len));
  },
  __allocString(str) {
    const bytes = this.__encoder.encode(str);
    const ptr = this.__instance.exports.alloc(bytes.length);
    new Uint8Array(this.__memory.buffer, ptr, bytes.length).set(bytes);
    return ptr;
  },
  __allocStringWithLen(str) {
    const bytes = this.__encoder.encode(str);
    const ptr = this.__instance.exports.alloc(bytes.length);
    new Uint8Array(this.__memory.buffer, ptr, bytes.length).set(bytes);
    return { ptr, len: bytes.length };
  },
  __cb(idx) { this.__instance.exports.__callback(idx); },
  __cbData(idx, ptr) { this.__instance.exports.__callback_with_data(idx, ptr); },

  // Read flat key-value pairs from WASM memory. WASM writes:
  // [key_ptr:i32, key_len:i32, val_ptr:i32, val_len:i32, ...] terminated by (0,0).
  // Returns a JS object. Same pattern as flush() — structured memory read.
  __readOpts(ptr) {
    const dv = new DataView(this.__memory.buffer);
    const o = {};
    let i = ptr;
    while (true) {
      const kp = dv.getInt32(i, true); i += 4;
      const kl = dv.getInt32(i, true); i += 4;
      if (kp === 0 && kl === 0) break;
      const vp = dv.getInt32(i, true); i += 4;
      const vl = dv.getInt32(i, true); i += 4;
      o[this.__getString(kp, kl)] = this.__getString(vp, vl);
    }
    return o;
  },

  __init(instance) {
    this.__instance = instance;
    this.__memory = instance.exports.memory;
  },
};

// ── Shorthand ────────────────────────────────────────────────────────────────
const R = NectarRuntime;

// ══════════════════════════════════════════════════════════════════════════════
//  WASM IMPORTS — organized by namespace to match codegen.rs import declarations
//  15 namespaces. Every function calls a browser API WASM physically cannot.
// ══════════════════════════════════════════════════════════════════════════════

export const name = 'core';
export const runtime = ``;
export const wasmImports = {

  // ── DOM: mount/flush command buffer + element queries ────────────────────
  dom: {
    mount(containerElId, htmlPtr, htmlLen) {
      R.__getElement(containerElId).innerHTML = R.__getString(htmlPtr, htmlLen);
    },

    hydrateRefs(containerElId) {
      const container = R.__getElement(containerElId);
      const nodes = container.querySelectorAll('[data-nid]');
      let count = 0;
      for (let i = 0; i < nodes.length; i++) {
        const nid = parseInt(nodes[i].getAttribute('data-nid'), 10);
        while (R.__elements.length <= nid) R.__elements.push(null);
        R.__elements[nid] = nodes[i];
        count++;
      }
      return count;
    },

    flush(bufPtr, bufLen) {
      const mem = R.__memory.buffer;
      const buf = new Uint32Array(mem, bufPtr, bufLen >>> 2);
      const els = R.__elements;
      const dec = R.__decoder;
      const inst = R.__instance;
      let i = 0;
      const end = buf.length;

      while (i < end) {
        const op = buf[i++];
        switch (op) {
          case OP_SET_TEXT: {
            const id = buf[i++], p = buf[i++], l = buf[i++];
            els[id].textContent = dec.decode(new Uint8Array(mem, p, l));
            break;
          }
          case OP_SET_ATTR: {
            const id = buf[i++], kp = buf[i++], kl = buf[i++], vp = buf[i++], vl = buf[i++];
            els[id].setAttribute(dec.decode(new Uint8Array(mem, kp, kl)), dec.decode(new Uint8Array(mem, vp, vl)));
            break;
          }
          case OP_REMOVE_ATTR: {
            const id = buf[i++], kp = buf[i++], kl = buf[i++];
            els[id].removeAttribute(dec.decode(new Uint8Array(mem, kp, kl)));
            break;
          }
          case OP_APPEND_CHILD: { els[buf[i++]].appendChild(els[buf[i++]]); break; }
          case OP_REMOVE_CHILD: { els[buf[i++]].removeChild(els[buf[i++]]); break; }
          case OP_INSERT_BEFORE: {
            const pid = buf[i++], nid = buf[i++], rid = buf[i++];
            els[pid].insertBefore(els[nid], els[rid]);
            break;
          }
          case OP_SET_STYLE: {
            const id = buf[i++], pp = buf[i++], pl = buf[i++], vp = buf[i++], vl = buf[i++];
            els[id].style.setProperty(dec.decode(new Uint8Array(mem, pp, pl)), dec.decode(new Uint8Array(mem, vp, vl)));
            break;
          }
          case OP_CLASS_ADD: { const id = buf[i++], p = buf[i++], l = buf[i++]; els[id].classList.add(dec.decode(new Uint8Array(mem, p, l))); break; }
          case OP_CLASS_REMOVE: { const id = buf[i++], p = buf[i++], l = buf[i++]; els[id].classList.remove(dec.decode(new Uint8Array(mem, p, l))); break; }
          case OP_CLASS_TOGGLE: { const id = buf[i++], p = buf[i++], l = buf[i++]; els[id].classList.toggle(dec.decode(new Uint8Array(mem, p, l))); break; }
          case OP_SET_INNER_HTML: {
            const id = buf[i++], p = buf[i++], l = buf[i++];
            els[id].innerHTML = dec.decode(new Uint8Array(mem, p, l));
            break;
          }
          case OP_ADD_EVENT: {
            const id = buf[i++], ep = buf[i++], el = buf[i++], cb = buf[i++];
            const handler = () => inst.exports.__callback(cb);
            R.__callbacks[cb] = handler;
            els[id].addEventListener(dec.decode(new Uint8Array(mem, ep, el)), handler);
            break;
          }
          case OP_REMOVE_EVENT: {
            const id = buf[i++], ep = buf[i++], el = buf[i++], cb = buf[i++];
            els[id].removeEventListener(dec.decode(new Uint8Array(mem, ep, el)), R.__callbacks[cb]);
            break;
          }
          case OP_FOCUS: { els[buf[i++]].focus(); break; }
          case OP_BLUR: { els[buf[i++]].blur(); break; }
          case OP_SET_PROPERTY: {
            const id = buf[i++], pp = buf[i++], pl = buf[i++], vp = buf[i++], vl = buf[i++];
            els[id][dec.decode(new Uint8Array(mem, pp, pl))] = dec.decode(new Uint8Array(mem, vp, vl));
            break;
          }
          default:
            console.error('[nectar] unknown flush opcode:', op, 'at index', i - 1);
            return;
        }
      }
    },

    getElementById(ptr, len) { return R.__registerElement(document.getElementById(R.__getString(ptr, len))); },
    querySelector(ptr, len) { return R.__registerElement(document.querySelector(R.__getString(ptr, len))); },
    createElement(ptr, len) { return R.__registerElement(document.createElement(R.__getString(ptr, len))); },
    createTextNode(ptr, len) { return R.__registerElement(document.createTextNode(R.__getString(ptr, len))); },
    getBody() { return R.__registerElement(document.body); },
    getHead() { return R.__registerElement(document.head); },
    getRoot() { return R.__registerElement(document.getElementById('app') || document.body); },
    getDocumentElement() { return R.__registerElement(document.documentElement); },

    addEventListener(elId, evtPtr, evtLen, cbIdx) {
      const handler = (e) => {
        if (R.__instance.exports.__event_data_ptr) {
          const dv = new DataView(R.__memory.buffer);
          const base = R.__instance.exports.__event_data_ptr();
          dv.setFloat64(base, e.clientX || 0, true);
          dv.setFloat64(base + 8, e.clientY || 0, true);
          dv.setInt32(base + 16, e.keyCode || 0, true);
          dv.setInt32(base + 20, (e.ctrlKey ? 1 : 0) | (e.shiftKey ? 2 : 0) | (e.altKey ? 4 : 0) | (e.metaKey ? 8 : 0), true);
          if (e.key) {
            const s = R.__allocStringWithLen(e.key);
            dv.setInt32(base + 24, s.ptr, true);
            dv.setInt32(base + 28, s.len, true);
          }
          if (e.dataTransfer) {
            R.__objects[0] = e; // stash event for getDragData/setDragData
          }
        }
        R.__cb(cbIdx);
      };
      R.__callbacks[cbIdx] = handler;
      R.__getElement(elId).addEventListener(R.__getString(evtPtr, evtLen), handler);
    },

    removeEventListener(elId, evtPtr, evtLen, cbIdx) {
      R.__getElement(elId).removeEventListener(R.__getString(evtPtr, evtLen), R.__callbacks[cbIdx]);
    },

    lazyMount(containerElId, urlPtr, urlLen, cbIdx) {
      const url = R.__getString(urlPtr, urlLen);
      import(url).then((mod) => {
        if (mod && mod.default) mod.default(R.__getElement(containerElId));
        R.__cb(cbIdx);
      });
    },

    setTitle(ptr, len) { document.title = R.__getString(ptr, len); },

    // Read-only DOM measurements (cannot go through flush — need return values)
    getScrollTop(elId) { return R.__getElement(elId).scrollTop; },
    getScrollLeft(elId) { return R.__getElement(elId).scrollLeft; },
    getClientHeight(elId) { return R.__getElement(elId).clientHeight; },
    getClientWidth(elId) { return R.__getElement(elId).clientWidth; },
    getWindowWidth() { return window.innerWidth; },
    getWindowHeight() { return window.innerHeight; },
    getOuterHtml() { return R.__allocString(document.documentElement.outerHTML); },

    // Drag data transfer (on stashed event from addEventListener)
    setDragData(fmtPtr, fmtLen, dataPtr, dataLen) {
      const e = R.__objects[0];
      if (e && e.dataTransfer) e.dataTransfer.setData(R.__getString(fmtPtr, fmtLen), R.__getString(dataPtr, dataLen));
    },
    getDragData(fmtPtr, fmtLen) {
      const e = R.__objects[0];
      if (e && e.dataTransfer) return R.__allocString(e.dataTransfer.getData(R.__getString(fmtPtr, fmtLen)));
      return 0;
    },
    preventDefault() {
      const e = R.__objects[0];
      if (e && e.preventDefault) e.preventDefault();
    },

    // Absorbed from embed — script loading needs onload callback bridge
    loadScript(urlPtr, urlLen, cbIdx) {
      const script = document.createElement('script');
      script.src = R.__getString(urlPtr, urlLen);
      script.onload = () => R.__cb(cbIdx);
      script.onerror = () => R.__cb(cbIdx);
      document.head.appendChild(script);
    },

    // Absorbed from loader — dynamic chunk loading needs onload + Promise
    loadChunk(urlPtr, urlLen) {
      const script = document.createElement('script');
      script.src = R.__getString(urlPtr, urlLen);
      document.head.appendChild(script);
      return R.__registerObject(new Promise((resolve) => { script.onload = resolve; }));
    },

    // Absorbed from media — Image() constructor + .decode() are browser APIs
    decodeImage(srcPtr, srcLen, cbIdx) {
      const img = new Image();
      img.src = R.__getString(srcPtr, srcLen);
      img.decode().then(() => R.__cb(cbIdx)).catch(() => R.__cb(cbIdx));
    },

    // Absorbed from media — Image() onload for progressive upgrade
    progressiveImage(elId, lowPtr, lowLen, highPtr, highLen) {
      const el = R.__getElement(elId);
      el.src = R.__getString(lowPtr, lowLen);
      const img = new Image();
      img.src = R.__getString(highPtr, highLen);
      img.onload = () => { el.src = img.src; };
    },

    // Absorbed from pdf — contentWindow.print() is a browser API
    print(elId) { R.__getElement(elId).contentWindow.print(); },

    // Hot reload — WebAssembly.compile + instantiate cannot be called from WASM
    reloadModule(urlPtr, urlLen, cbIdx) {
      fetch(R.__getString(urlPtr, urlLen), { cache: 'no-store' })
        .then(r => r.arrayBuffer())
        .then(b => WebAssembly.compile(b))
        .then(m => WebAssembly.instantiate(m, wasmImports))
        .then(inst => { R.__init(inst); R.__cb(cbIdx); })
        .catch(() => R.__cb(cbIdx));
    },

    // Absorbed from io — Blob + URL.createObjectURL are browser APIs
    download(dataPtr, dataLen, namePtr, nameLen) {
      const data = R.__getString(dataPtr, dataLen);
      const name = R.__getString(namePtr, nameLen);
      const blob = new Blob([data], { type: 'application/octet-stream' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = name;
      a.click();
      URL.revokeObjectURL(url);
    },
  },

  // ── Timers ───────────────────────────────────────────────────────────────
  timer: {
    setTimeout(cbIdx, ms) { return setTimeout(() => R.__cb(cbIdx), ms); },
    clearTimeout(id) { clearTimeout(id); },
    setInterval(cbIdx, ms) { return setInterval(() => R.__cb(cbIdx), ms); },
    clearInterval(id) { clearInterval(id); },
    requestAnimationFrame(cbIdx) { return requestAnimationFrame(() => R.__cb(cbIdx)); },
    cancelAnimationFrame(id) { cancelAnimationFrame(id); },
    now() { return performance.now(); },
  },

  // ── webapi — storage, clipboard, history, console, random ──────────────
  webapi: {
    // Storage
    localStorageGet(kp, kl) { const v = localStorage.getItem(R.__getString(kp, kl)); return v !== null ? R.__allocString(v) : 0; },
    localStorageSet(kp, kl, vp, vl) { localStorage.setItem(R.__getString(kp, kl), R.__getString(vp, vl)); },
    localStorageRemove(kp, kl) { localStorage.removeItem(R.__getString(kp, kl)); },
    sessionStorageGet(kp, kl) { const v = sessionStorage.getItem(R.__getString(kp, kl)); return v !== null ? R.__allocString(v) : 0; },
    sessionStorageSet(kp, kl, vp, vl) { sessionStorage.setItem(R.__getString(kp, kl), R.__getString(vp, vl)); },
    // Clipboard
    clipboardWrite(ptr, len) { navigator.clipboard.writeText(R.__getString(ptr, len)).catch(() => {}); },
    clipboardRead(cbIdx) { navigator.clipboard.readText().then(t => { R.__cbData(cbIdx, R.__allocString(t)); }).catch(() => R.__cbData(cbIdx, 0)); },
    // URL / History
    getLocationHref() { return R.__allocString(location.href); },
    getLocationSearch() { return R.__allocString(location.search); },
    getLocationHash() { return R.__allocString(location.hash); },
    getLocationPathname() { return R.__allocString(location.pathname); },
    pushState(urlPtr, urlLen) { history.pushState(null, '', R.__getString(urlPtr, urlLen)); },
    replaceState(urlPtr, urlLen) { history.replaceState(null, '', R.__getString(urlPtr, urlLen)); },
    // Console
    consoleLog(ptr, len) { console.log(R.__getString(ptr, len)); },
    consoleWarn(ptr, len) { console.warn(R.__getString(ptr, len)); },
    consoleError(ptr, len) { console.error(R.__getString(ptr, len)); },
    // Absorbed from router — popstate is a window event WASM can't listen to
    onPopState(cbIdx) { window.addEventListener('popstate', () => R.__cb(cbIdx)); },
    // Absorbed from env — window.__env property access
    envGet(namePtr, nameLen) {
      const name = R.__getString(namePtr, nameLen);
      const val = (typeof window !== 'undefined' && window.__env && window.__env[name]) || '';
      return R.__allocString(val);
    },
    // Absorbed from share — navigator.share
    canShare() { return navigator.share ? 1 : 0; },
    nativeShare(titlePtr, titleLen, textPtr, textLen, urlPtr, urlLen) {
      if (!navigator.share) return 0;
      navigator.share({
        title: R.__getString(titlePtr, titleLen),
        text: R.__getString(textPtr, textLen),
        url: R.__getString(urlPtr, urlLen),
      }).catch(() => {});
      return 1;
    },
    // Absorbed from perf — performance.mark/measure
    perfMark(namePtr, nameLen) { performance.mark(R.__getString(namePtr, nameLen)); },
    perfMeasure(namePtr, nameLen, startPtr, startLen, endPtr, endLen) {
      performance.measure(R.__getString(namePtr, nameLen), R.__getString(startPtr, startLen), R.__getString(endPtr, endLen));
    },
  },

  // ── HTTP — fetch() is a browser API ────────────────────────────────────
  http: {
    _h: null,
    setMethod(ptr, len) { this._m = R.__getString(ptr, len); },
    setBody(ptr, len) { this._b = R.__getString(ptr, len); },
    addHeader(kp, kl, vp, vl) { if (!this._h) this._h = {}; this._h[R.__getString(kp, kl)] = R.__getString(vp, vl); },
    fetch(urlPtr, urlLen) {
      const o = { method: this._m || 'GET', headers: this._h || undefined, body: this._b || undefined };
      this._m = null; this._b = null; this._h = null;
      return R.__registerObject(fetch(R.__getString(urlPtr, urlLen), o));
    },
  },

  // ── Observer — IntersectionObserver, matchMedia ────────────────────────
  observe: {
    matchMedia(qPtr, qLen) { return matchMedia(R.__getString(qPtr, qLen)).matches ? 1 : 0; },
    intersectionObserver(cbIdx, optsPtr) {
      const opts = optsPtr ? R.__readOpts(optsPtr) : {};
      return R.__registerObject(new IntersectionObserver(() => R.__cb(cbIdx), opts));
    },
    observe(obsId, elId) { R.__getObject(obsId).observe(R.__getElement(elId)); },
    unobserve(obsId, elId) { R.__getObject(obsId).unobserve(R.__getElement(elId)); },
    disconnect(obsId) { R.__getObject(obsId).disconnect(); },
  },

  // ── WebSocket ──────────────────────────────────────────────────────────
  ws: {
    connect(urlPtr, urlLen) { return R.__registerObject(new WebSocket(R.__getString(urlPtr, urlLen))); },
    send(wsId, dataPtr, dataLen) { R.__getObject(wsId).send(R.__getString(dataPtr, dataLen)); },
    sendBinary(wsId, ptr, len) { R.__getObject(wsId).send(new Uint8Array(R.__memory.buffer, ptr, len)); },
    close(wsId) { R.__getObject(wsId).close(); },
    closeWithCode(wsId, code, rPtr, rLen) { R.__getObject(wsId).close(code, R.__getString(rPtr, rLen)); },
    onOpen(wsId, cbIdx) { R.__getObject(wsId).addEventListener('open', () => R.__cb(cbIdx)); },
    onMessage(wsId, cbIdx) { R.__getObject(wsId).addEventListener('message', (e) => R.__cbData(cbIdx, R.__allocString(e.data))); },
    onClose(wsId, cbIdx) { R.__getObject(wsId).addEventListener('close', () => R.__cb(cbIdx)); },
    onError(wsId, cbIdx) { R.__getObject(wsId).addEventListener('error', () => R.__cb(cbIdx)); },
    getReadyState(wsId) { return R.__getObject(wsId).readyState; },
  },

  // ── IndexedDB — pure syscalls, no logic. WASM handles serialization. ──
  db: {
    open(namePtr, nameLen, version, cbIdx) {
      const req = indexedDB.open(R.__getString(namePtr, nameLen), version || 1);
      req.onupgradeneeded = (e) => { e.target.result.createObjectStore('default', { keyPath: 'id' }); };
      req.onsuccess = (e) => { R.__cbData(cbIdx, R.__registerObject(e.target.result)); };
      req.onerror = () => { R.__cbData(cbIdx, 0); };
    },
    put(dbId, storePtr, storeLen, keyPtr, keyLen, valPtr, valLen) {
      const store = R.__getString(storePtr, storeLen);
      const tx = R.__getObject(dbId).transaction(store, 'readwrite');
      tx.objectStore(store).put({ id: R.__getString(keyPtr, keyLen), v: R.__getString(valPtr, valLen) });
    },
    get(dbId, storePtr, storeLen, keyPtr, keyLen, cbIdx) {
      const store = R.__getString(storePtr, storeLen);
      const tx = R.__getObject(dbId).transaction(store, 'readonly');
      const req = tx.objectStore(store).get(R.__getString(keyPtr, keyLen));
      req.onsuccess = () => { R.__cbData(cbIdx, req.result ? R.__allocString(req.result.v) : 0); };
    },
    delete(dbId, storePtr, storeLen, keyPtr, keyLen) {
      const store = R.__getString(storePtr, storeLen);
      R.__getObject(dbId).transaction(store, 'readwrite').objectStore(store).delete(R.__getString(keyPtr, keyLen));
    },
    getAll(dbId, storePtr, storeLen, cbIdx) {
      const store = R.__getString(storePtr, storeLen);
      const tx = R.__getObject(dbId).transaction(store, 'readonly');
      const req = tx.objectStore(store).getAll();
      req.onsuccess = () => {
        const items = req.result;
        const count = items.length;
        for (let i = 0; i < count; i++) R.__cbData(cbIdx, R.__allocString(items[i].v));
        R.__cbData(cbIdx, 0);
      };
    },
  },

  // ── Web Workers ────────────────────────────────────────────────────────
  worker: {
    spawn(codePtr, codeLen) {
      const blob = new Blob([R.__getString(codePtr, codeLen)], { type: 'application/javascript' });
      const url = URL.createObjectURL(blob);
      const w = new Worker(url);
      URL.revokeObjectURL(url);
      return R.__registerObject(w);
    },
    channelCreate() {
      const { port1, port2 } = new MessageChannel();
      const id1 = R.__registerObject(port1);
      const id2 = R.__registerObject(port2);
      return id1; // WASM convention: port2 = id1 + 1
    },
    channelSend(portId, dataPtr, dataLen) {
      R.__getObject(portId).postMessage(R.__getString(dataPtr, dataLen));
    },
    channelRecv(portId, cbIdx) {
      const port = R.__getObject(portId);
      port.addEventListener('message', (e) => R.__cbData(cbIdx, R.__allocString(typeof e.data === 'string' ? e.data : '')));
      port.start();
    },
    parallel(fnPtrsPtr, fnCount, cbIdx) { R.__cb(cbIdx); },
    await(promiseId) { return R.__getObject(promiseId); },
    postMessage(workerId, dataPtr, dataLen) { R.__getObject(workerId).postMessage(R.__getString(dataPtr, dataLen)); },
    onMessage(workerId, cbIdx) { R.__getObject(workerId).addEventListener('message', (e) => R.__cbData(cbIdx, R.__allocString(typeof e.data === 'string' ? e.data : ''))); },
    terminate(workerId) { R.__getObject(workerId).terminate(); },
  },

  // ── PWA — Service Worker, Push, Caching ────────────────────────────────
  pwa: {
    cachePrecache(namePtr, nameLen) {
      return caches.open(R.__getString(namePtr, nameLen));
    },
    registerPush(optsPtr) {
      if (!('serviceWorker' in navigator) || !('PushManager' in window)) return 0;
      return navigator.serviceWorker.ready.then(reg =>
        reg.pushManager.subscribe(optsPtr ? R.__readOpts(optsPtr) : {})
      );
    },
    registerServiceWorker(pathPtr, pathLen, cbIdx) {
      if (!('serviceWorker' in navigator)) { R.__cb(cbIdx); return; }
      navigator.serviceWorker.register(R.__getString(pathPtr, pathLen)).then(() => R.__cb(cbIdx));
    },
  },

  // ── Hardware APIs ──────────────────────────────────────────────────────
  hardware: {
    haptic(pattern) { if (navigator.vibrate) navigator.vibrate(pattern); },
    biometricAuth(optsPtr, successCb, failCb) {
      if (!navigator.credentials) { R.__cb(failCb); return; }
      navigator.credentials.get(optsPtr ? R.__readOpts(optsPtr) : {})
        .then(() => R.__cb(successCb))
        .catch(() => R.__cb(failCb));
    },
    cameraCapture(constraintsPtr, cbIdx) {
      if (!navigator.mediaDevices) { R.__cb(cbIdx); return; }
      navigator.mediaDevices.getUserMedia(constraintsPtr ? R.__readOpts(constraintsPtr) : {})
        .then(stream => { R.__cbData(cbIdx, R.__registerObject(stream)); });
    },
    geolocationCurrent(cbIdx) {
      navigator.geolocation.getCurrentPosition(
        pos => {
          const dv = new DataView(R.__memory.buffer);
          const base = R.__instance.exports.__geo_data_ptr ? R.__instance.exports.__geo_data_ptr() : 0;
          if (base) {
            dv.setFloat64(base, pos.coords.latitude, true);
            dv.setFloat64(base + 8, pos.coords.longitude, true);
          }
          R.__cb(cbIdx);
        },
        () => R.__cb(cbIdx)
      );
    },
  },

  // ── Payment — only processPayment needs JS (contentWindow.postMessage) ─
  // initProvider → use dom.loadScript. createCheckout → WASM uses
  // dom.createElement + SET_ATTR/SET_STYLE/APPEND_CHILD opcodes.
  payment: {
    processPayment(elId, msgPtr, msgLen, cbIdx) {
      window.addEventListener('message', (e) => {
        R.__cbData(cbIdx, R.__allocString(typeof e.data === 'string' ? e.data : ''));
      }, { once: true });
      R.__getElement(elId).contentWindow.postMessage(R.__getString(msgPtr, msgLen), '*');
    },
  },

  // ── Auth — pure syscalls, no logic. WASM parses cookies. ────────────────
  auth: {
    login(urlPtr, urlLen) { location.href = R.__getString(urlPtr, urlLen); },
    logout(urlPtr, urlLen) { if (urlPtr) location.href = R.__getString(urlPtr, urlLen); },
    getRawCookies() { return R.__allocString(document.cookie); },
    setCookie(ptr, len) { document.cookie = R.__getString(ptr, len); },
  },

  // ── Upload — file picker + XHR ─────────────────────────────────────────
  upload: {
    init(acceptPtr, acceptLen, multiple, cbIdx) {
      const input = document.createElement('input');
      input.type = 'file';
      if (acceptPtr) input.accept = R.__getString(acceptPtr, acceptLen);
      if (multiple) input.multiple = true;
      input.addEventListener('change', () => {
        R.__registerObject(input.files);
        R.__cb(cbIdx);
      });
      input.click();
    },
    start(urlPtr, urlLen, cbIdx) {
      const xhr = new XMLHttpRequest();
      xhr.open('POST', R.__getString(urlPtr, urlLen));
      const id = R.__registerObject(xhr);
      xhr.upload.addEventListener('progress', (e) => {
        if (R.__instance.exports.__upload_progress) {
          R.__instance.exports.__upload_progress(id, e.loaded, e.total);
        }
      });
      xhr.addEventListener('load', () => R.__cbData(cbIdx, xhr.status));
      return id;
    },
    cancel(xhrId) { R.__getObject(xhrId).abort(); },
  },

  // ── Time — Intl.DateTimeFormat, timezone ────────────────────────────────
  time: {
    now() { return Date.now(); },
    format(ms, localePtr, localeLen) {
      const locale = R.__getString(localePtr, localeLen) || undefined;
      return R.__allocString(new Intl.DateTimeFormat(locale).format(new Date(ms)));
    },
    getTimezoneOffset() { return new Date().getTimezoneOffset(); },
    formatDate(ms, localePtr, localeLen, optsPtr) {
      const locale = localePtr ? R.__getString(localePtr, localeLen) : undefined;
      const opts = optsPtr ? R.__readOpts(optsPtr) : {};
      return R.__allocString(new Intl.DateTimeFormat(locale, opts).format(new Date(ms)));
    },
  },

  // ── Streaming — SSE + streaming fetch ──────────────────────────────────
  streaming: {
    streamFetch(urlPtr, urlLen, cbIdx) {
      fetch(R.__getString(urlPtr, urlLen)).then(res => {
        const reader = res.body.getReader();
        const pump = () => reader.read().then(({ done, value }) => {
          if (done) { R.__cb(cbIdx); return; }
          const ptr = R.__allocString(new TextDecoder().decode(value));
          R.__cbData(cbIdx, ptr);
          pump();
        });
        pump();
      });
    },
    sseConnect(urlPtr, urlLen, cbIdx) {
      const es = new EventSource(R.__getString(urlPtr, urlLen));
      es.onmessage = (e) => R.__cbData(cbIdx, R.__allocString(e.data));
      return R.__registerObject(es);
    },
  },
};

// ── WASM instantiation helper ────────────────────────────────────────────────
export async function instantiate(wasmUrl, extraImports = {}) {
  const merged = {};
  for (const [ns, fns] of Object.entries(wasmImports)) {
    merged[ns] = { ...fns, ...(extraImports[ns] || {}) };
  }
  const { instance } = await WebAssembly.instantiateStreaming(fetch(wasmUrl), merged);
  NectarRuntime.__init(instance);
  return instance;
}

// ══════════════════════════════════════════════════════════════════════════════
//  HYDRATION — thin bootstrap, WASM controls event delegation + state restore
// ══════════════════════════════════════════════════════════════════════════════

export function hydrate(wasmInstance, rootElement) {
  const rootId = NectarRuntime.__registerElement(rootElement);
  if (wasmInstance.exports.__hydrate) wasmInstance.exports.__hydrate(rootId);
}

// SW registration: use pwa.registerServiceWorker syscall from WASM.
// Update detection, offline listeners — all WASM-internal via existing
// dom.addEventListener + pwa namespace syscalls.

// Hot reload: WASM controls the WebSocket connection (ws.connect + ws.onMessage),
// CSS updates (dom.querySelector + SET_STYLE/SET_INNER_HTML opcodes), and
// reconnection (timer.setTimeout). Only reloadModule needs JS — WebAssembly
// cannot compile/instantiate itself.

if (typeof module !== "undefined") module.exports = { name, runtime, wasmImports, NectarRuntime, instantiate, hydrate };
