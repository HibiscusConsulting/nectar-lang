// runtime/modules/core.js — Core NectarRuntime (signals, DOM, memory management)
// This module is ALWAYS included in every build.

/** Currently executing effect, used for automatic dependency tracking */
let currentEffect = null;

/**
 * Effect tracks which signals are read during its execution and
 * re-runs automatically when any dependency changes.
 */
class Effect {
  constructor(fn) {
    this._fn = fn;
    this._dependencies = new Set();
    this._disposed = false;
    this.run();
  }

  run() {
    if (this._disposed) return;
    this._dependencies.forEach((signal) => {
      signal._subscribers.delete(this);
    });
    this._dependencies.clear();

    const prevEffect = currentEffect;
    currentEffect = this;
    try {
      this._fn();
    } finally {
      currentEffect = prevEffect;
    }
  }

  addDependency(signal) {
    this._dependencies.add(signal);
  }

  dispose() {
    this._disposed = true;
    this._dependencies.forEach((signal) => {
      signal._subscribers.delete(this);
    });
    this._dependencies.clear();
  }
}

class Scheduler {
  constructor() {
    this._pendingEffects = new Set();
    this._scheduled = false;
  }

  schedule(effect) {
    this._pendingEffects.add(effect);
    if (!this._scheduled) {
      this._scheduled = true;
      if (typeof requestAnimationFrame !== "undefined") {
        requestAnimationFrame(() => this.flush());
      } else {
        Promise.resolve().then(() => this.flush());
      }
    }
  }

  flush() {
    this._scheduled = false;
    const effects = [...this._pendingEffects];
    this._pendingEffects.clear();
    for (const effect of effects) {
      effect.run();
    }
  }
}

const globalScheduler = new Scheduler();

let batchDepth = 0;
const batchQueue = new Set();

function batch(fn) {
  batchDepth++;
  try {
    fn();
  } finally {
    batchDepth--;
    if (batchDepth === 0) {
      const queued = [...batchQueue];
      batchQueue.clear();
      for (const effect of queued) {
        globalScheduler.schedule(effect);
      }
      globalScheduler.flush();
    }
  }
}

function createEffect(fn) {
  const effect = new Effect(fn);
  return () => effect.dispose();
}

function createMemo(fn) {
  let cachedValue;
  let dirty = true;

  const memo = {
    _subscribers: new Set(),
    get() {
      if (currentEffect) {
        currentEffect.addDependency(memo);
        memo._subscribers.add(currentEffect);
      }
      if (dirty) {
        const prevEffect = currentEffect;
        currentEffect = memoEffect;
        try {
          cachedValue = fn();
        } finally {
          currentEffect = prevEffect;
        }
        dirty = false;
      }
      return cachedValue;
    },
  };

  const memoEffect = {
    _dependencies: new Set(),
    _disposed: false,
    run() {
      dirty = true;
      for (const sub of memo._subscribers) {
        if (batchDepth > 0) {
          batchQueue.add(sub);
        } else {
          globalScheduler.schedule(sub);
        }
      }
    },
    addDependency(signal) {
      this._dependencies.add(signal);
    },
    dispose() {
      this._disposed = true;
      this._dependencies.forEach((signal) => {
        signal._subscribers.delete(this);
      });
      this._dependencies.clear();
    },
  };

  const prevEffect = currentEffect;
  currentEffect = memoEffect;
  try {
    cachedValue = fn();
    dirty = false;
  } finally {
    currentEffect = prevEffect;
  }

  return memo;
}

function hashString(str) {
  let hash = 5381;
  for (let i = 0; i < str.length; i++) {
    hash = ((hash << 5) + hash + str.charCodeAt(i)) & 0x7fffffff;
  }
  return hash.toString(36);
}

class AgentManager {
  constructor(runtime) {
    this.runtime = runtime;
    this.tools = new Map();
    this.histories = new Map();
    this.streaming = false;
  }

  registerTool(name, description, schema, funcIdx) {
    this.tools.set(name, { description, schema, funcIdx });
  }

  getToolDefinitions() {
    const defs = [];
    for (const [name, tool] of this.tools) {
      defs.push({
        type: 'function',
        function: { name, description: tool.description, parameters: tool.schema },
      });
    }
    return defs;
  }

  dispatchToolCall(toolName, argsJson) {
    const tool = this.tools.get(toolName);
    if (!tool) { console.warn(`Agent tool not found: ${toolName}`); return null; }
    try {
      const args = typeof argsJson === 'string' ? JSON.parse(argsJson) : argsJson;
      const exportName = `__tool_${toolName}`;
      const allExports = Object.keys(this.runtime.instance.exports);
      const matchingExport = allExports.find(e => e.endsWith(exportName) || e.includes(`_${toolName}`));
      if (matchingExport) {
        const paramValues = Object.values(args).map(v => {
          if (typeof v === 'string') { const { ptr } = this.runtime.writeString(v); return ptr; }
          return typeof v === 'number' ? v : 0;
        });
        return this.runtime.instance.exports[matchingExport](...paramValues);
      }
    } catch (e) { console.error(`Error dispatching tool ${toolName}:`, e); }
    return null;
  }

  addMessage(agentName, role, content) {
    if (!this.histories.has(agentName)) this.histories.set(agentName, []);
    this.histories.get(agentName).push({ role, content });
  }

  getMessages(agentName, systemPrompt) {
    const history = this.histories.get(agentName) || [];
    if (systemPrompt) return [{ role: 'system', content: systemPrompt }, ...history];
    return [...history];
  }

  clearHistory(agentName) { this.histories.delete(agentName); }
}

class Router {
  constructor(runtime) {
    this.runtime = runtime;
    this.routes = [];
    this.fallbackMount = null;
    this.currentDispose = null;
    this.currentParams = {};
    this.currentPath = typeof location !== "undefined" ? location.pathname : "/";
    this.outletHandle = null;
  }

  _parsePattern(pattern) {
    const paramNames = [];
    let regexStr = "^";
    const segments = pattern.split("/");
    for (const seg of segments) {
      if (seg === "") continue;
      regexStr += "/";
      if (seg.startsWith(":")) { paramNames.push(seg.slice(1)); regexStr += "([^/]+)"; }
      else if (seg === "*") { paramNames.push("_wildcard"); regexStr += "(.*)"; }
      else { regexStr += seg.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"); }
    }
    if (regexStr === "^") regexStr += "/";
    regexStr += "$";
    return { regex: new RegExp(regexStr), paramNames };
  }

  registerRoute(path, mountFnIdx) {
    const { regex, paramNames } = this._parsePattern(path);
    this.routes.push({ path, regex, paramNames, mountFnName: `__route_mount_${mountFnIdx}` });
  }

  init(routesConfigJson) {
    if (typeof window !== "undefined") {
      window.addEventListener("popstate", () => { this.currentPath = location.pathname; this._matchAndMount(); });
    }
    this._matchAndMount();
  }

  navigate(path) {
    if (path === this.currentPath) return;
    this.currentPath = path;
    if (typeof history !== "undefined") history.pushState(null, "", path);
    this._matchAndMount();
  }

  getParam(name) { return this.currentParams[name] || ""; }

  async _matchAndMount() {
    const path = this.currentPath;
    for (const route of this.routes) {
      const match = path.match(route.regex);
      if (match) {
        this.currentParams = {};
        route.paramNames.forEach((name, i) => { this.currentParams[name] = match[i + 1] || ""; });
        this._unmountCurrent();
        const mountFn = this.runtime.instance?.exports[route.mountFnName];
        if (mountFn) {
          const outletEl = this.runtime.elements.get(this.outletHandle);
          if (outletEl) outletEl.innerHTML = "";
          mountFn(this.outletHandle || 1);
        }
        return;
      }
    }
    this._unmountCurrent();
    if (this.fallbackMount) {
      const fallbackFn = this.runtime.instance?.exports[this.fallbackMount];
      if (fallbackFn) {
        const outletEl = this.runtime.elements.get(this.outletHandle);
        if (outletEl) outletEl.innerHTML = "";
        fallbackFn(this.outletHandle || 1);
      }
    }
  }

  _unmountCurrent() {
    if (this.currentDispose) { this.currentDispose(); this.currentDispose = null; }
  }
}

class WorkerPool {
  constructor(wasmBytes, importTemplate, poolSize = 4) {
    this._wasmBytes = wasmBytes;
    this._importTemplate = importTemplate;
    this._poolSize = poolSize;
    this._workers = [];
    this._available = [];
    this._taskQueue = [];
    this._nextTaskId = 1;
    this._pendingTasks = new Map();
  }

  async _initWorker() {
    const workerScript = `
      let wasmInstance = null;
      let wasmMemory = null;
      self.onmessage = async function(e) {
        const { type, taskId, funcName, args, wasmBytes } = e.data;
        if (type === 'init') {
          const memory = new WebAssembly.Memory({ initial: 1, maximum: 100, shared: true });
          const importObject = { env: { memory } };
          const { instance } = await WebAssembly.instantiate(wasmBytes, importObject);
          wasmInstance = instance;
          wasmMemory = memory;
          self.postMessage({ type: 'ready' });
          return;
        }
        if (type === 'exec') {
          try {
            const fn = wasmInstance.exports[funcName];
            if (!fn) { self.postMessage({ type: 'error', taskId, error: 'Function not found: ' + funcName }); return; }
            const result = fn(...(args || []));
            self.postMessage({ type: 'result', taskId, result });
          } catch (err) { self.postMessage({ type: 'error', taskId, error: err.message }); }
          return;
        }
        if (type === 'execIdx') {
          try {
            const table = wasmInstance.exports.__indirect_function_table;
            const fn = table ? table.get(e.data.funcIdx) : null;
            if (!fn) { self.postMessage({ type: 'error', taskId, error: 'Function index not found' }); return; }
            const result = fn();
            self.postMessage({ type: 'result', taskId, result });
          } catch (err) { self.postMessage({ type: 'error', taskId, error: err.message }); }
          return;
        }
      };
    `;
    const blob = new Blob([workerScript], { type: "application/javascript" });
    const url = URL.createObjectURL(blob);
    const worker = new Worker(url);
    URL.revokeObjectURL(url);

    await new Promise((resolve, reject) => {
      worker.onmessage = (e) => { if (e.data.type === "ready") resolve(); };
      worker.onerror = reject;
      worker.postMessage({ type: "init", wasmBytes: this._wasmBytes });
    });

    worker.onmessage = (e) => {
      const { type, taskId, result, error } = e.data;
      if (type === "result" || type === "error") {
        const pending = this._pendingTasks.get(taskId);
        if (pending) {
          this._pendingTasks.delete(taskId);
          if (type === "result") pending.resolve(result);
          else pending.reject(new Error(error));
        }
        this._available.push(worker);
        this._drainQueue();
      }
    };
    return worker;
  }

  async _getWorker() {
    if (this._available.length > 0) return this._available.pop();
    if (this._workers.length < this._poolSize) {
      const worker = await this._initWorker();
      this._workers.push(worker);
      return worker;
    }
    return new Promise((resolve) => { this._taskQueue.push(resolve); });
  }

  _drainQueue() {
    while (this._taskQueue.length > 0 && this._available.length > 0) {
      const resolve = this._taskQueue.shift();
      resolve(this._available.pop());
    }
  }

  async spawn(funcName, args) {
    const worker = await this._getWorker();
    const taskId = this._nextTaskId++;
    return new Promise((resolve, reject) => {
      this._pendingTasks.set(taskId, { resolve, reject });
      worker.postMessage({ type: "exec", taskId, funcName, args });
    });
  }

  async spawnByIndex(funcIdx) {
    const worker = await this._getWorker();
    const taskId = this._nextTaskId++;
    return new Promise((resolve, reject) => {
      this._pendingTasks.set(taskId, { resolve, reject });
      worker.postMessage({ type: "execIdx", taskId, funcIdx });
    });
  }

  terminate() {
    for (const worker of this._workers) worker.terminate();
    this._workers = [];
    this._available = [];
    this._pendingTasks.clear();
  }
}

/**
 * NectarRuntime — the core runtime class that bridges WASM to the DOM.
 * Handles signals, DOM operations, memory management, HTTP, routing,
 * styles, animations, accessibility, streaming, media, and web APIs.
 */
class NectarRuntime {
  constructor() {
    this.instance = null;
    this.memory = null;
    this.elements = new Map();
    this.nextHandle = 1;
    this.signals = new Map();
    this.nextSignalId = 1;
    this.nextFetchId = 1;
    this.pendingFetches = new Map();
    this.workerPool = null;
    this.agentManager = new AgentManager(this);
    this.channels = new Map();
    this.nextChannelId = 1;
    this._wasmBytes = null;
    this.router = new Router(this);
  }

  _createSignal(initialValue) {
    const signal = {
      _value: initialValue,
      _subscribers: new Set(),
      get() {
        if (currentEffect) { currentEffect.addDependency(signal); signal._subscribers.add(currentEffect); }
        return signal._value;
      },
      set(newValue) {
        if (signal._value !== newValue) {
          signal._value = newValue;
          for (const sub of signal._subscribers) {
            if (batchDepth > 0) batchQueue.add(sub);
            else globalScheduler.schedule(sub);
          }
        }
      },
    };
    return signal;
  }

  readString(ptr, len) {
    const bytes = new Uint8Array(this.memory.buffer, ptr, len);
    return new TextDecoder().decode(bytes);
  }

  writeString(str) {
    const bytes = new TextEncoder().encode(str);
    const ptr = this._allocWasm(bytes.length);
    new Uint8Array(this.memory.buffer, ptr, bytes.length).set(bytes);
    return { ptr, len: bytes.length };
  }

  _allocWasm(size) {
    if (this.instance && this.instance.exports.alloc) return this.instance.exports.alloc(size);
    const ptr = this._heapPtr || 4096;
    this._heapPtr = ptr + size;
    const needed = Math.ceil((ptr + size) / 65536);
    const current = this.memory.buffer.byteLength / 65536;
    if (needed > current) this.memory.grow(needed - current);
    return ptr;
  }
}

const coreModule = {
  name: 'core',
  runtime: {
    NectarRuntime,
    AgentManager,
    WorkerPool,
    Router,
    Effect,
    Scheduler,
    createEffect,
    createMemo,
    batch,
    hashString,
    currentEffect: () => currentEffect,
    batchDepth: () => batchDepth,
    batchQueue,
    globalScheduler,
  },
  wasmImports: {}
};

if (typeof module !== "undefined") module.exports = coreModule;
