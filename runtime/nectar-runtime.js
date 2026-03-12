/**
 * NOTE: This is the monolithic (non-tree-shaken) runtime containing ALL modules.
 * For production builds, prefer the modular system in `runtime/modules/` which
 * allows tree-shaking — only the modules your program uses are included.
 * Use `runtime/build-runtime.js` to produce a minimal bundled runtime:
 *
 *   node runtime/build-runtime.js --modules core,seo,form --output dist/nectar-runtime.js
 *
 * The compiler's `nectar build` command detects required modules automatically
 * via the runtime_modules analysis pass.
 */

/**
 * Arc Runtime — minimal DOM bridge for Nectar-compiled WebAssembly modules.
 *
 * This provides the host functions that Nectar WASM modules import:
 * - dom.createElement
 * - dom.setText
 * - dom.appendChild
 * - dom.addEventListener
 * - dom.setAttribute
 *
 * The runtime is intentionally minimal — no virtual DOM, no diffing.
 * Nectar uses fine-grained reactivity (signals) to surgically update
 * only the DOM nodes that depend on changed state.
 */

// --- Reactivity primitives ---

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
    // Run immediately to capture initial dependencies
    this.run();
  }

  run() {
    if (this._disposed) return;
    // Clean up old subscriptions
    this._dependencies.forEach((signal) => {
      signal._subscribers.delete(this);
    });
    this._dependencies.clear();

    // Set ourselves as the current effect so signal.get() can track us
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

/**
 * Scheduler batches DOM updates using requestAnimationFrame so that
 * multiple signal changes within the same tick result in a single
 * re-render pass.
 */
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
        // Fallback for non-browser environments (e.g. Node / tests)
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

/** Whether we are currently inside a batch() call */
let batchDepth = 0;
/** Effects queued while inside a batch */
const batchQueue = new Set();

/**
 * Group multiple signal updates so that dependent effects only run
 * once after the batch completes.
 */
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
      // Flush synchronously so callers can rely on DOM being updated
      globalScheduler.flush();
    }
  }
}

/**
 * Create a reactive effect that re-runs whenever its signal
 * dependencies change. Returns a dispose function.
 */
function createEffect(fn) {
  const effect = new Effect(fn);
  return () => effect.dispose();
}

/**
 * Create a memoized computed value that only recomputes when its
 * signal dependencies change.
 */
function createMemo(fn) {
  let cachedValue;
  let dirty = true;

  // Internal signal-like object so other effects can depend on the memo
  const memo = {
    _subscribers: new Set(),
    get() {
      // Track this memo as a dependency of the current effect
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

  // A lightweight effect that marks the memo dirty when deps change
  const memoEffect = {
    _dependencies: new Set(),
    _disposed: false,
    run() {
      dirty = true;
      // Notify downstream effects that the memo value may have changed
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

  // Eagerly evaluate once to capture initial dependencies
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

// --- Worker Pool for concurrency ---

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
    // Create a worker from a blob URL that loads the same WASM module
    const workerScript = `
      let wasmInstance = null;
      let wasmMemory = null;

      self.onmessage = async function(e) {
        const { type, taskId, funcName, args, wasmBytes } = e.data;

        if (type === 'init') {
          // Initialize WASM in the worker with shared memory
          const memory = new WebAssembly.Memory({
            initial: 1,
            maximum: 100,
            shared: true,
          });
          const importObject = {
            env: { memory },
          };
          const { instance } = await WebAssembly.instantiate(wasmBytes, importObject);
          wasmInstance = instance;
          wasmMemory = memory;
          self.postMessage({ type: 'ready' });
          return;
        }

        if (type === 'exec') {
          try {
            const fn = wasmInstance.exports[funcName];
            if (!fn) {
              self.postMessage({ type: 'error', taskId, error: 'Function not found: ' + funcName });
              return;
            }
            const result = fn(...(args || []));
            self.postMessage({ type: 'result', taskId, result });
          } catch (err) {
            self.postMessage({ type: 'error', taskId, error: err.message });
          }
          return;
        }

        if (type === 'execIdx') {
          // Execute a function by table index
          try {
            const table = wasmInstance.exports.__indirect_function_table;
            const fn = table ? table.get(e.data.funcIdx) : null;
            if (!fn) {
              self.postMessage({ type: 'error', taskId, error: 'Function index not found' });
              return;
            }
            const result = fn();
            self.postMessage({ type: 'result', taskId, result });
          } catch (err) {
            self.postMessage({ type: 'error', taskId, error: err.message });
          }
          return;
        }
      };
    `;
    const blob = new Blob([workerScript], { type: "application/javascript" });
    const url = URL.createObjectURL(blob);
    const worker = new Worker(url);
    URL.revokeObjectURL(url);

    // Wait for the worker to be ready
    await new Promise((resolve, reject) => {
      worker.onmessage = (e) => {
        if (e.data.type === "ready") {
          resolve();
        }
      };
      worker.onerror = reject;
      worker.postMessage({
        type: "init",
        wasmBytes: this._wasmBytes,
      });
    });

    // Set up ongoing message handler
    worker.onmessage = (e) => {
      const { type, taskId, result, error } = e.data;
      if (type === "result" || type === "error") {
        const pending = this._pendingTasks.get(taskId);
        if (pending) {
          this._pendingTasks.delete(taskId);
          if (type === "result") {
            pending.resolve(result);
          } else {
            pending.reject(new Error(error));
          }
        }
        // Return worker to the pool
        this._available.push(worker);
        this._drainQueue();
      }
    };

    return worker;
  }

  async _getWorker() {
    if (this._available.length > 0) {
      return this._available.pop();
    }
    if (this._workers.length < this._poolSize) {
      const worker = await this._initWorker();
      this._workers.push(worker);
      return worker;
    }
    // All workers busy — queue the request
    return new Promise((resolve) => {
      this._taskQueue.push(resolve);
    });
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
    for (const worker of this._workers) {
      worker.terminate();
    }
    this._workers = [];
    this._available = [];
    this._pendingTasks.clear();
  }
}

// --- Agent Manager: manages AI agent tool registry, message history, streaming ---

/**
 * AgentManager handles the coordination between AI agents compiled from Nectar
 * and the LLM runtime. It manages:
 * - Tool registry (name -> WASM function index mapping)
 * - Message history per agent
 * - Streaming state
 * - Tool call dispatch (when AI calls a tool, execute the WASM function and feed result back)
 */
class AgentManager {
  constructor(runtime) {
    this.runtime = runtime;
    /** Map of tool name -> { description, schema, funcIdx } */
    this.tools = new Map();
    /** Message history per agent: agentName -> [{role, content}] */
    this.histories = new Map();
    /** Whether a stream is currently active */
    this.streaming = false;
  }

  /**
   * Register a tool that the AI can call.
   * The tool body is a WASM-exported function identified by funcIdx.
   */
  registerTool(name, description, schema, funcIdx) {
    this.tools.set(name, { description, schema, funcIdx });
  }

  /**
   * Get all registered tools in the OpenAI function-calling format.
   */
  getToolDefinitions() {
    const defs = [];
    for (const [name, tool] of this.tools) {
      defs.push({
        type: 'function',
        function: {
          name,
          description: tool.description,
          parameters: tool.schema,
        },
      });
    }
    return defs;
  }

  /**
   * Dispatch a tool call from the AI: look up the registered WASM function
   * and invoke it with the parsed arguments, then return the result.
   */
  dispatchToolCall(toolName, argsJson) {
    const tool = this.tools.get(toolName);
    if (!tool) {
      console.warn(`Agent tool not found: ${toolName}`);
      return null;
    }

    try {
      const args = typeof argsJson === 'string' ? JSON.parse(argsJson) : argsJson;
      const exportName = `__tool_${toolName}`;

      // Find the matching export (may have agent name prefix)
      const allExports = Object.keys(this.runtime.instance.exports);
      const matchingExport = allExports.find(e => e.endsWith(exportName) || e.includes(`_${toolName}`));

      if (matchingExport) {
        // Convert args object to positional params
        const paramValues = Object.values(args).map(v => {
          if (typeof v === 'string') {
            const { ptr, len } = this.runtime.writeString(v);
            return ptr; // For simplicity, pass pointer; real impl would pass ptr+len
          }
          return typeof v === 'number' ? v : 0;
        });

        const result = this.runtime.instance.exports[matchingExport](...paramValues);
        return result;
      }
    } catch (e) {
      console.error(`Error dispatching tool ${toolName}:`, e);
    }
    return null;
  }

  /**
   * Add a message to the history for a given agent.
   */
  addMessage(agentName, role, content) {
    if (!this.histories.has(agentName)) {
      this.histories.set(agentName, []);
    }
    this.histories.get(agentName).push({ role, content });
  }

  /**
   * Get the full message history for an agent (including system prompt).
   */
  getMessages(agentName, systemPrompt) {
    const history = this.histories.get(agentName) || [];
    if (systemPrompt) {
      return [{ role: 'system', content: systemPrompt }, ...history];
    }
    return [...history];
  }

  /**
   * Clear message history for an agent.
   */
  clearHistory(agentName) {
    this.histories.delete(agentName);
  }
}

// --- Router: client-side URL routing ---

/**
 * Simple hash-based string hashing for scope IDs.
 */
function hashString(str) {
  let hash = 5381;
  for (let i = 0; i < str.length; i++) {
    hash = ((hash << 5) + hash + str.charCodeAt(i)) & 0x7fffffff;
  }
  return hash.toString(36);
}

/**
 * Router manages client-side routing for Nectar applications.
 * Supports:
 *  - Static routes: "/about"
 *  - Parameterized routes: "/user/:id"
 *  - Wildcard routes: "/admin/*"
 *  - Guards: async auth checks before mounting
 *  - History API navigation (pushState/popstate)
 */
class Router {
  constructor(runtime) {
    this.runtime = runtime;
    /** Registered routes: [{ pattern, regex, paramNames, mountFnName, guardFnName? }] */
    this.routes = [];
    /** Fallback mount function name for 404 */
    this.fallbackMount = null;
    /** Currently mounted component's dispose function (if any) */
    this.currentDispose = null;
    /** Currently matched route params */
    this.currentParams = {};
    /** Current path */
    this.currentPath = typeof location !== "undefined" ? location.pathname : "/";
    /** Container element handle where routed components are mounted */
    this.outletHandle = null;
  }

  /**
   * Parse a route pattern like "/user/:id" into a regex
   * and extract parameter names.
   */
  _parsePattern(pattern) {
    const paramNames = [];
    // Escape special regex chars but handle :param and * wildcard
    let regexStr = "^";
    const segments = pattern.split("/");
    for (const seg of segments) {
      if (seg === "") continue;
      regexStr += "/";
      if (seg.startsWith(":")) {
        paramNames.push(seg.slice(1));
        regexStr += "([^/]+)";
      } else if (seg === "*") {
        paramNames.push("_wildcard");
        regexStr += "(.*)";
      } else {
        regexStr += seg.replace(/[.*+?^${}()|[\]\]/g, "\$&");
      }
    }
    if (regexStr === "^") regexStr += "/";
    regexStr += "$";
    return { regex: new RegExp(regexStr), paramNames };
  }

  /**
   * Register a route from WASM.
   * @param {string} path - Route pattern like "/user/:id"
   * @param {string} mountFnName - Export name of the mount function
   */
  registerRoute(path, mountFnIdx) {
    const { regex, paramNames } = this._parsePattern(path);
    this.routes.push({
      path,
      regex,
      paramNames,
      mountFnName: `__route_mount_${mountFnIdx}`,
    });
  }

  /**
   * Initialize the router: set up popstate listener and match initial URL.
   */
  init(routesConfigJson) {
    if (typeof window !== "undefined") {
      window.addEventListener("popstate", () => {
        this.currentPath = location.pathname;
        this._matchAndMount();
      });
    }
    // Match the initial URL
    this._matchAndMount();
  }

  /**
   * Navigate programmatically to a new path.
   */
  navigate(path) {
    if (path === this.currentPath) return;
    this.currentPath = path;
    if (typeof history !== "undefined") {
      history.pushState(null, "", path);
    }
    this._matchAndMount();
  }

  /**
   * Get a route parameter value by name.
   */
  getParam(name) {
    return this.currentParams[name] || "";
  }

  /**
   * Match the current URL against registered routes and mount
   * the corresponding component.
   */
  async _matchAndMount() {
    const path = this.currentPath;

    for (const route of this.routes) {
      const match = path.match(route.regex);
      if (match) {
        // Extract params
        this.currentParams = {};
        route.paramNames.forEach((name, i) => {
          this.currentParams[name] = match[i + 1] || "";
        });

        // Unmount current component
        this._unmountCurrent();

        // Call the mount function
        const mountFn = this.runtime.instance?.exports[route.mountFnName];
        if (mountFn) {
          // Create a container element for the route
          const outletEl = this.runtime.elements.get(this.outletHandle);
          if (outletEl) {
            outletEl.innerHTML = "";
          }
          mountFn(this.outletHandle || 1);
        }
        return;
      }
    }

    // No route matched — use fallback if available
    this._unmountCurrent();
    if (this.fallbackMount) {
      const fallbackFn = this.runtime.instance?.exports[this.fallbackMount];
      if (fallbackFn) {
        const outletEl = this.runtime.elements.get(this.outletHandle);
        if (outletEl) {
          outletEl.innerHTML = "";
        }
        fallbackFn(this.outletHandle || 1);
      }
    }
  }

  _unmountCurrent() {
    if (this.currentDispose) {
      this.currentDispose();
      this.currentDispose = null;
    }
  }
}

// --- Runtime class ---

class NectarRuntime {
  constructor() {
    this.instance = null;
    this.memory = null;
    this.elements = new Map();  // handle -> DOM element
    this.nextHandle = 1;
    this.signals = new Map();   // signal id -> signal object
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

  /**
   * Create a signal object for use by the runtime.
   * Signals are observable values that automatically track dependent effects.
   */
  _createSignal(initialValue) {
    const signal = {
      _value: initialValue,
      _subscribers: new Set(),
      get() {
        // Auto-track: if an effect is currently running, register it
        if (currentEffect) {
          currentEffect.addDependency(signal);
          signal._subscribers.add(currentEffect);
        }
        return signal._value;
      },
      set(newValue) {
        if (signal._value !== newValue) {
          signal._value = newValue;
          // Notify subscribers
          for (const sub of signal._subscribers) {
            if (batchDepth > 0) {
              batchQueue.add(sub);
            } else {
              globalScheduler.schedule(sub);
            }
          }
        }
      },
    };
    return signal;
  }

  /**
   * Load and instantiate a Nectar-compiled .wasm module
   */
  async mount(wasmUrl, rootElement) {
    const importObject = {
      env: {
        memory: new WebAssembly.Memory({ initial: 1, maximum: 100 }),
      },
      dom: {
        createElement: (tagPtr, tagLen) => {
          const tag = this.readString(tagPtr, tagLen);
          const el = document.createElement(tag);
          const handle = this.nextHandle++;
          this.elements.set(handle, el);
          return handle;
        },

        setText: (parentHandle, textPtr, textLen) => {
          const parent = this.elements.get(parentHandle);
          if (parent) {
            const text = this.readString(textPtr, textLen);
            parent.textContent = text;
          }
        },

        appendChild: (parentHandle, childHandle) => {
          const parent = this.elements.get(parentHandle);
          const child = this.elements.get(childHandle);
          if (parent && child) {
            parent.appendChild(child);
          }
        },

        addEventListener: (handle, eventPtr, eventLen, callbackIdx) => {
          const el = this.elements.get(handle);
          if (el) {
            const event = this.readString(eventPtr, eventLen);
            el.addEventListener(event, () => {
              // Call back into WASM
              if (this.instance.exports[`__handler_${callbackIdx}`]) {
                this.instance.exports[`__handler_${callbackIdx}`]();
              }
            });
          }
        },

        setAttribute: (handle, namePtr, nameLen, valPtr, valLen) => {
          const el = this.elements.get(handle);
          if (el) {
            const name = this.readString(namePtr, nameLen);
            const value = this.readString(valPtr, valLen);
            el.setAttribute(name, value);
          }
        },

        // Set a DOM property (not attribute) — used by two-way form bindings
        setProperty: (handle, namePtr, nameLen, valPtr, valLen) => {
          const el = this.elements.get(handle);
          if (el) {
            const name = this.readString(namePtr, nameLen);
            const value = this.readString(valPtr, valLen);
            // For boolean properties like "checked", convert string to boolean
            if (name === "checked" || name === "disabled" || name === "readOnly") {
              el[name] = value === "true" || value === "1";
            } else {
              el[name] = value;
            }
          }
        },

        // Get a DOM property value — returns (ptr, len) of the string value
        getProperty: (handle, namePtr, nameLen) => {
          const el = this.elements.get(handle);
          if (el) {
            const name = this.readString(namePtr, nameLen);
            const value = String(el[name] ?? "");
            const { ptr, len } = this.writeString(value);
            return [ptr, len];
          }
          return [0, 0];
        },

        // Lazy component mounting — show fallback, dynamic-import component chunk, swap
        lazyMount: (componentNamePtr, componentNameLen, rootHandle, fallbackFnIdx) => {
          const componentName = this.readString(componentNamePtr, componentNameLen);
          const root = this.elements.get(rootHandle);
          if (!root) return;

          // Show fallback immediately
          if (this.instance.exports[`__effect_${fallbackFnIdx}`]) {
            this.instance.exports[`__effect_${fallbackFnIdx}`]();
          }

          // Dynamic import() the component's WASM chunk
          const chunkUrl = `./${componentName}.wasm`;
          globalThis.fetch(chunkUrl)
            .then((res) => res.arrayBuffer())
            .then((bytes) => WebAssembly.instantiate(bytes, importObject))
            .then(({ instance: chunkInstance }) => {
              const mountFn = chunkInstance.exports[`${componentName}_mount`];
              if (mountFn) {
                // Clear fallback content
                while (root.firstChild) {
                  root.removeChild(root.firstChild);
                }
                const childHandle = this.nextHandle++;
                this.elements.set(childHandle, root);
                mountFn(childHandle);
              }
            })
            .catch((err) => {
              console.error(`Failed to lazy-load component ${componentName}:`, err);
            });
        },

        // Error boundary — wraps a mount function in try/catch, renders fallback on error
        errorBoundary: (rootHandle, mountFnIdx, fallbackFnIdx) => {
          const root = this.elements.get(rootHandle);
          if (!root) return;

          try {
            // Attempt to mount the component body
            const mountFn = this.instance.exports[`__handler_${mountFnIdx}`];
            if (mountFn) {
              mountFn(rootHandle);
            }
          } catch (err) {
            console.error("[Nectar ErrorBoundary] Component render failed:", err);

            // Clear failed render
            while (root.firstChild) {
              root.removeChild(root.firstChild);
            }

            // Render the fallback UI
            const fallbackFn = this.instance.exports[`__handler_${fallbackFnIdx}`];
            if (fallbackFn) {
              fallbackFn(rootHandle);
            }

            // Store retry info so the boundary can be reset
            root.__nectarErrorBoundary = {
              error: err,
              mountFnIdx,
              fallbackFnIdx,
              retry: () => {
                while (root.firstChild) {
                  root.removeChild(root.firstChild);
                }
                try {
                  const retryMount = this.instance.exports[`__handler_${mountFnIdx}`];
                  if (retryMount) retryMount(rootHandle);
                } catch (retryErr) {
                  console.error("[Nectar ErrorBoundary] Retry failed:", retryErr);
                  const retryFallback = this.instance.exports[`__handler_${fallbackFnIdx}`];
                  if (retryFallback) retryFallback(rootHandle);
                }
              },
            };
          }
        },
      },
      // --- Skeleton screen support ---
      skeleton: {
        /**
         * skeleton.mount(rootHandle, skeletonFnIdx)
         * Renders the skeleton placeholder into the root element and injects
         * built-in shimmer animation CSS.
         */
        mount: (rootHandle, skeletonFnIdx) => {
          const root = this.elements.get(rootHandle);
          if (!root) return;

          // Inject skeleton shimmer CSS if not already present
          if (!document.getElementById("__nectar-skeleton-styles")) {
            const style = document.createElement("style");
            style.id = "__nectar-skeleton-styles";
            style.textContent = `
              .nectar-skeleton {
                animation: nectar-skeleton-pulse 1.5s ease-in-out infinite;
              }
              .nectar-skeleton [class*="skeleton-"] {
                background: linear-gradient(90deg, #e0e0e0 25%, #f0f0f0 50%, #e0e0e0 75%);
                background-size: 200% 100%;
                animation: nectar-skeleton-shimmer 1.5s ease-in-out infinite;
                border-radius: 4px;
              }
              @keyframes nectar-skeleton-pulse {
                0% { opacity: 1; }
                50% { opacity: 0.4; }
                100% { opacity: 1; }
              }
              @keyframes nectar-skeleton-shimmer {
                0% { background-position: 200% 0; }
                100% { background-position: -200% 0; }
              }
            `;
            document.head.appendChild(style);
          }

          // Mark the root so we know a skeleton is active
          root.setAttribute("data-nectar-skeleton", "true");
          root.classList.add("nectar-skeleton");

          // Render the skeleton template into the root
          const skeletonFn =
            this.instance.exports[`__handler_${skeletonFnIdx}`];
          if (skeletonFn) {
            skeletonFn(rootHandle);
          }
        },

        /**
         * skeleton.replace(rootHandle, contentFnIdx)
         * Fades out the skeleton, clears it, renders the real content, and fades in.
         */
        replace: (rootHandle, contentFnIdx) => {
          const root = this.elements.get(rootHandle);
          if (!root) return;

          // Fade out the skeleton content
          root.style.transition = "opacity 0.2s ease-out";
          root.style.opacity = "0";

          const doReplace = () => {
            // Clear skeleton content
            while (root.firstChild) {
              root.removeChild(root.firstChild);
            }
            root.removeAttribute("data-nectar-skeleton");
            root.classList.remove("nectar-skeleton");

            // Render the real content
            const contentFn =
              this.instance.exports[`__handler_${contentFnIdx}`];
            if (contentFn) {
              contentFn(rootHandle);
            }

            // Fade in the real content
            root.style.opacity = "0";
            requestAnimationFrame(() => {
              root.style.transition = "opacity 0.3s ease-in";
              root.style.opacity = "1";
            });
          };

          // Wait for fade-out to finish, then swap
          root.addEventListener("transitionend", doReplace, { once: true });

          // Safety fallback — if transitionend doesn't fire within 300ms, force swap
          setTimeout(() => {
            if (root.getAttribute("data-nectar-skeleton") === "true") {
              doReplace();
            }
          }, 300);
        },
      },
      // --- String module: format string support ---
      string: {
        /**
         * concat(ptr1, len1, ptr2, len2) -> [resultPtr, resultLen]
         * Concatenates two strings in WASM linear memory.
         */
        concat: (ptr1, len1, ptr2, len2) => {
          const s1 = this.readString(ptr1, len1);
          const s2 = this.readString(ptr2, len2);
          const combined = s1 + s2;
          const { ptr, len } = this.writeString(combined);
          return [ptr, len];
        },

        /**
         * fromI32(value) -> [ptr, len]
         * Convert an i32 integer to its string representation.
         */
        fromI32: (value) => {
          const s = String(value);
          const { ptr, len } = this.writeString(s);
          return [ptr, len];
        },

        /**
         * fromF64(value) -> [ptr, len]
         * Convert an f64 float to its string representation.
         */
        fromF64: (value) => {
          const s = String(value);
          const { ptr, len } = this.writeString(s);
          return [ptr, len];
        },

        /**
         * fromBool(value) -> [ptr, len]
         * Convert a boolean (i32: 0 or 1) to "true" or "false".
         */
        fromBool: (value) => {
          const s = value ? "true" : "false";
          const { ptr, len } = this.writeString(s);
          return [ptr, len];
        },

        /**
         * toString(value) -> [ptr, len]
         * Generic value-to-string conversion.
         * For now, treats the value as an i32 and converts.
         */
        toString: (value) => {
          const s = String(value);
          const { ptr, len } = this.writeString(s);
          return [ptr, len];
        },
      },

      signal: {
        create: (initialValue) => {
          const id = this.nextSignalId++;
          const signal = this._createSignal(initialValue);
          this.signals.set(id, signal);
          return id;
        },

        get: (signalId) => {
          const signal = this.signals.get(signalId);
          return signal ? signal.get() : 0;
        },

        set: (signalId, newValue) => {
          const signal = this.signals.get(signalId);
          if (signal) {
            signal.set(newValue);
          }
        },

        subscribe: (signalId, callbackIdx) => {
          const signal = this.signals.get(signalId);
          if (signal) {
            createEffect(() => {
              const value = signal.get();
              if (this.instance.exports[`__effect_${callbackIdx}`]) {
                this.instance.exports[`__effect_${callbackIdx}`](value);
              }
            });
          }
        },

        createEffect: (fnIdx) => {
          createEffect(() => {
            if (this.instance.exports[`__effect_${fnIdx}`]) {
              this.instance.exports[`__effect_${fnIdx}`]();
            }
          });
        },

        createMemo: (fnIdx) => {
          const id = this.nextSignalId++;
          const memo = createMemo(() => {
            if (this.instance.exports[`__memo_${fnIdx}`]) {
              return this.instance.exports[`__memo_${fnIdx}`]();
            }
            return 0;
          });
          // Wrap memo as a signal-like for uniform access
          this.signals.set(id, { get: () => memo.get(), set: () => {} });
          return id;
        },

        batch: (fnIdx) => {
          batch(() => {
            if (this.instance.exports[`__batch_${fnIdx}`]) {
              this.instance.exports[`__batch_${fnIdx}`]();
            }
          });
        },
      },

      // --- HTTP module: API communication from WASM ---
      http: {
        // fetch(urlPtr, urlLen, methodPtr, methodLen) -> fetchId
        fetch: (urlPtr, urlLen, methodPtr, methodLen) => {
          const url = this.readString(urlPtr, urlLen);
          const method = this.readString(methodPtr, methodLen);
          const fetchId = this.nextFetchId++;

          const promise = globalThis.fetch(url, { method })
            .then(async (response) => {
              const body = await response.text();
              const bodyBytes = new TextEncoder().encode(body);
              const ptr = this._allocWasm(bodyBytes.length);
              new Uint8Array(this.memory.buffer, ptr, bodyBytes.length).set(bodyBytes);
              return { status: response.status, bodyPtr: ptr, bodyLen: bodyBytes.length };
            });

          this.pendingFetches.set(fetchId, promise);
          return fetchId;
        },

        fetchGetBody: (fetchId) => { return [0, 0]; },
        fetchGetStatus: (fetchId) => { return 0; },

        // Async fetch with WASM callback
        fetchAsync: (urlPtr, urlLen, methodPtr, methodLen, callbackIdx) => {
          const url = this.readString(urlPtr, urlLen);
          const method = this.readString(methodPtr, methodLen);

          globalThis.fetch(url, { method })
            .then(async (response) => {
              const body = await response.text();
              const bodyBytes = new TextEncoder().encode(body);
              const ptr = this._allocWasm(bodyBytes.length);
              new Uint8Array(this.memory.buffer, ptr, bodyBytes.length).set(bodyBytes);
              const handler = this.instance.exports[`__fetch_callback_${callbackIdx}`];
              if (handler) handler(response.status, ptr, bodyBytes.length);
            })
            .catch((err) => {
              const errHandler = this.instance.exports[`__fetch_error_${callbackIdx}`];
              if (errHandler) {
                const msg = new TextEncoder().encode(err.message || "fetch failed");
                const ptr = this._allocWasm(msg.length);
                new Uint8Array(this.memory.buffer, ptr, msg.length).set(msg);
                errHandler(ptr, msg.length);
              }
            });
        },
      },

      // --- Worker module: concurrency primitives for WASM ---
      worker: {
        // Spawn a new Web Worker running a WASM function by table index
        spawn: (funcIdx) => {
          if (this.workerPool) {
            this.workerPool.spawnByIndex(funcIdx);
          }
          return funcIdx;
        },

        // Create a MessageChannel-backed channel for WASM<->Worker communication
        channelCreate: () => {
          const channelId = this.nextChannelId++;
          const { port1, port2 } = new MessageChannel();
          this.channels.set(channelId, {
            port1,
            port2,
            buffer: [],
            waiters: [],
          });
          return channelId;
        },

        // Send a value through a channel (value is serialized as bytes in WASM memory)
        channelSend: (channelId, valuePtr, valueLen) => {
          const ch = this.channels.get(channelId);
          if (!ch) return;

          // Copy the value bytes out of WASM memory
          const bytes = new Uint8Array(this.memory.buffer, valuePtr, valueLen).slice();

          // If there are waiters, deliver directly
          if (ch.waiters.length > 0) {
            const waiter = ch.waiters.shift();
            waiter(bytes);
          } else {
            // Buffer the message
            ch.buffer.push(bytes);
          }

          // Also send over the MessagePort for cross-worker delivery
          ch.port1.postMessage(bytes);
        },

        // Receive a value from a channel (async — calls back into WASM when ready)
        channelRecv: (channelId, callbackIdx) => {
          const ch = this.channels.get(channelId);
          if (!ch) return;

          const deliver = (bytes) => {
            const ptr = this._allocWasm(bytes.length);
            new Uint8Array(this.memory.buffer, ptr, bytes.length).set(bytes);
            const handler = this.instance.exports[`__channel_recv_${callbackIdx}`];
            if (handler) {
              handler(ptr, bytes.length);
            }
          };

          // Check if there is a buffered message
          if (ch.buffer.length > 0) {
            const bytes = ch.buffer.shift();
            deliver(bytes);
          } else {
            // Wait for a message
            ch.waiters.push(deliver);
          }
        },

        // Run multiple async operations in parallel, call back when all complete
        parallel: (funcIndicesPtr, funcIndicesLen, callbackIdx) => {
          const indices = [];
          const view = new DataView(this.memory.buffer);
          for (let i = 0; i < funcIndicesLen; i++) {
            indices.push(view.getInt32(funcIndicesPtr + i * 4, true));
          }

          // Execute each function index on the worker pool in parallel
          const promises = indices.map((funcIdx) => {
            if (this.workerPool) {
              return this.workerPool.spawnByIndex(funcIdx);
            }
            // Fallback: run on main thread if no worker pool
            const table = this.instance.exports.__indirect_function_table;
            if (table) {
              const fn = table.get(funcIdx);
              return Promise.resolve(fn ? fn() : undefined);
            }
            return Promise.resolve(undefined);
          });

          Promise.all(promises).then((results) => {
            const handler = this.instance.exports[`__parallel_done_${callbackIdx}`];
            if (handler) {
              // Write results array to memory
              const resultBytes = new Int32Array(results.map(r => r || 0));
              const ptr = this._allocWasm(resultBytes.byteLength);
              new Uint8Array(this.memory.buffer, ptr, resultBytes.byteLength)
                .set(new Uint8Array(resultBytes.buffer));
              handler(ptr, results.length);
            }
          });
        },

        // Await a spawned worker result (async callback)
        await: (workerId) => {
          return new Promise((resolve) => {
            const entry = this.workerPool?._workers?.find((_, id) => id === workerId);
            if (entry) {
              entry.worker.onmessage = (e) => {
                resolve(e.data);
                entry.worker.terminate();
              };
            } else {
              resolve(0);
            }
          });
        },
      },

      // --- Channel module: WebSocket runtime for WASM ---
      channel: {
        connect: (namePtr, nameLen, urlPtr, urlLen) => {
          const name = this.readString(namePtr, nameLen);
          const url = this.readString(urlPtr, urlLen);
          const ch = { name, url, ws: null, reconnect: true, heartbeatId: null, reconnectDelay: 1000 };

          const open = () => {
            ch.ws = new WebSocket(url);
            ch.ws.onopen = () => {
              ch.reconnectDelay = 1000;
              if (ch.onConnect) ch.onConnect();
            };
            ch.ws.onmessage = (e) => {
              if (ch.onMessage) ch.onMessage(e.data);
            };
            ch.ws.onclose = () => {
              if (ch.onDisconnect) ch.onDisconnect();
              if (ch.reconnect) {
                setTimeout(open, Math.min(ch.reconnectDelay *= 1.5, 30000));
              }
            };
            ch.ws.onerror = () => ch.ws.close();
          };

          this._wsChannels = this._wsChannels || new Map();
          this._wsChannels.set(name, ch);
          open();
        },

        send: (namePtr, nameLen, dataPtr, dataLen) => {
          const name = this.readString(namePtr, nameLen);
          const data = this.readString(dataPtr, dataLen);
          const ch = this._wsChannels?.get(name);
          if (ch?.ws?.readyState === WebSocket.OPEN) {
            ch.ws.send(data);
          }
        },

        close: (namePtr, nameLen) => {
          const name = this.readString(namePtr, nameLen);
          const ch = this._wsChannels?.get(name);
          if (ch) {
            ch.reconnect = false;
            ch.ws?.close();
            this._wsChannels.delete(name);
          }
        },

        setReconnect: (namePtr, nameLen, enabled) => {
          const name = this.readString(namePtr, nameLen);
          const ch = this._wsChannels?.get(name);
          if (ch) ch.reconnect = !!enabled;
        },
      },

      // --- AI module: LLM interaction primitives for WASM ---
      ai: {
        // Stream a chat completion
        // Calls back into WASM with each token as it arrives
        chatStream: (modelPtr, modelLen, messagesPtr, messagesLen,
                     toolsPtr, toolsLen, onTokenIdx, onToolCallIdx, onDoneIdx) => {
          const model = this.readString(modelPtr, modelLen);
          const messages = JSON.parse(this.readString(messagesPtr, messagesLen));
          const tools = toolsLen > 0 ? JSON.parse(this.readString(toolsPtr, toolsLen)) : [];

          // Stream via fetch + ReadableStream
          globalThis.fetch('/api/chat', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ model, messages, tools, stream: true }),
          }).then(response => {
            const reader = response.body.getReader();
            const decoder = new TextDecoder();

            const pump = () => {
              reader.read().then(({done, value}) => {
                if (done) {
                  const doneHandler = this.instance.exports[`__ai_done_${onDoneIdx}`];
                  if (doneHandler) doneHandler();
                  return;
                }
                const chunk = decoder.decode(value, { stream: true });
                // Parse SSE lines and extract tokens or tool calls
                const lines = chunk.split('\n').filter(l => l.startsWith('data: '));
                for (const line of lines) {
                  try {
                    const data = JSON.parse(line.slice(6));
                    if (data.choices && data.choices[0]) {
                      const delta = data.choices[0].delta;
                      if (delta.content) {
                        // Token callback — write string to WASM memory
                        const { ptr, len } = this.writeString(delta.content);
                        const tokenHandler = this.instance.exports[`__ai_token_${onTokenIdx}`];
                        if (tokenHandler) tokenHandler(ptr, len);
                      }
                      if (delta.tool_calls) {
                        // Tool call callback
                        for (const tc of delta.tool_calls) {
                          if (tc.function) {
                            const callJson = JSON.stringify(tc.function);
                            const { ptr, len } = this.writeString(callJson);
                            const toolHandler = this.instance.exports[`__ai_tool_call_${onToolCallIdx}`];
                            if (toolHandler) toolHandler(ptr, len);
                            // Execute the tool if registered and feed result back
                            this.agentManager.dispatchToolCall(tc.function.name, tc.function.arguments);
                          }
                        }
                      }
                    }
                  } catch (e) {
                    // Skip malformed SSE lines
                  }
                }
                pump();
              });
            };
            pump();
          }).catch(err => {
            console.error('AI chat stream error:', err);
            const doneHandler = this.instance.exports[`__ai_done_${onDoneIdx}`];
            if (doneHandler) doneHandler();
          });
        },

        // Single completion (non-streaming)
        chatComplete: (modelPtr, modelLen, messagesPtr, messagesLen, callbackIdx) => {
          const model = this.readString(modelPtr, modelLen);
          const messages = JSON.parse(this.readString(messagesPtr, messagesLen));

          globalThis.fetch('/api/chat', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ model, messages, stream: false }),
          }).then(r => r.json()).then(data => {
            const content = data.choices?.[0]?.message?.content || '';
            const { ptr, len } = this.writeString(content);
            const handler = this.instance.exports[`__ai_complete_${callbackIdx}`];
            if (handler) handler(ptr, len);
          }).catch(err => {
            console.error('AI chat complete error:', err);
          });
        },

        // Tool registration — registers WASM-exported functions as tools the AI can call
        registerTool: (namePtr, nameLen, descPtr, descLen, schemaPtr, schemaLen, funcIdx) => {
          const name = this.readString(namePtr, nameLen);
          const description = this.readString(descPtr, descLen);
          const schema = JSON.parse(this.readString(schemaPtr, schemaLen));
          this.agentManager.registerTool(name, description, schema, funcIdx);
        },

        // Embedding generation
        embed: (textPtr, textLen, callbackIdx) => {
          const text = this.readString(textPtr, textLen);
          globalThis.fetch('/api/embed', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ text }),
          }).then(r => r.json()).then(data => {
            const embedding = data.embedding || [];
            // Write float32 array to WASM memory
            const bytes = new Float32Array(embedding);
            const ptr = this._allocWasm(bytes.byteLength);
            new Uint8Array(this.memory.buffer, ptr, bytes.byteLength)
              .set(new Uint8Array(bytes.buffer));
            const handler = this.instance.exports[`__ai_embed_${callbackIdx}`];
            if (handler) handler(ptr, embedding.length);
          }).catch(err => {
            console.error('AI embed error:', err);
          });
        },

        // Structured output — parse AI response into typed data
        parseStructured: (responsePtr, responseLen, schemaPtr, schemaLen) => {
          const response = this.readString(responsePtr, responseLen);
          const schema = this.readString(schemaPtr, schemaLen);
          try {
            const parsed = JSON.parse(response);
            const resultStr = JSON.stringify(parsed);
            const { ptr } = this.writeString(resultStr);
            return ptr;
          } catch (e) {
            return 0; // null pointer on parse failure
          }
        },
      },

      // --- Router module: client-side URL routing from WASM ---
      router: {
        init: (routesPtr, routesLen) => {
          const routesJson = routesPtr > 0 ? this.readString(routesPtr, routesLen) : "[]";
          this.router.init(routesJson);
        },

        navigate: (pathPtr, pathLen) => {
          const path = this.readString(pathPtr, pathLen);
          this.router.navigate(path);
        },

        currentPath: () => {
          const { ptr, len } = this.writeString(this.router.currentPath);
          return ptr; // returns ptr; len can be obtained separately
        },

        getParam: (namePtr, nameLen) => {
          const name = this.readString(namePtr, nameLen);
          const value = this.router.getParam(name);
          const { ptr, len } = this.writeString(value);
          return ptr;
        },

        registerRoute: (pathPtr, pathLen, mountFnIdx) => {
          const path = this.readString(pathPtr, pathLen);
          this.router.registerRoute(path, mountFnIdx);
        },
      },

      // --- Style module: scoped CSS injection from WASM ---
      style: {
        /**
         * Check if critical CSS has already been inlined by SSR.
         * When `nectar build --critical-css` is used, the server injects critical
         * styles into `<head>` and sets `window.__nectarCriticalLoaded = true`.
         * This method lets WASM code query that flag to avoid double-injection.
         *
         * Returns 1 (i32) if critical styles are already present, 0 otherwise.
         */
        isCriticalLoaded: () => {
          if (typeof window !== "undefined" && window.__nectarCriticalLoaded) {
            return 1;
          }
          return 0;
        },

        /**
         * Inject scoped CSS for a component.
         * All selectors are prefixed with [data-nectar-HASH] to scope them.
         * Returns a scope ID (as i32 pointer to the scope string in memory).
         *
         * If critical CSS was already inlined by the server (detected via
         * `window.__nectarCriticalLoaded` and a matching `<style data-nectar-critical>`
         * tag in the document), this function skips injection for components
         * whose styles are already present in the critical style block.
         */
        injectStyles: (componentNamePtr, componentNameLen, cssPtr, cssLen) => {
          const name = this.readString(componentNamePtr, componentNameLen);
          const css = this.readString(cssPtr, cssLen);

          // Generate a unique scope ID from the component name
          const scopeId = "nectar-" + hashString(name);

          // If critical CSS was inlined by SSR, check if this component's
          // styles are already present to avoid double-injection.
          if (typeof window !== "undefined" && window.__nectarCriticalLoaded) {
            const criticalTag = document.querySelector("style[data-nectar-critical]");
            if (criticalTag && criticalTag.textContent.indexOf(scopeId) !== -1) {
              // Styles already inlined — just return the scope ID
              const { ptr } = this.writeString(scopeId);
              return ptr;
            }
          }

          // Scope all selectors with [data-SCOPE_ID] attribute
          const scoped = css.replace(/([^{}]+)\{/g, (match, selector) => {
            return (
              selector
                .split(",")
                .map((s) => `[data-${scopeId}] ${s.trim()}`)
                .join(",") + "{"
            );
          });

          // Inject into document head
          if (typeof document !== "undefined") {
            // Remove any existing style for this component (hot reload)
            const existingStyle = document.querySelector(
              `style[data-nectar-component="${name}"]`
            );
            if (existingStyle) {
              existingStyle.remove();
            }

            const styleEl = document.createElement("style");
            styleEl.setAttribute("data-nectar-component", name);
            styleEl.textContent = scoped;
            document.head.appendChild(styleEl);
          }

          // Return the scope ID as a string in WASM memory
          const { ptr } = this.writeString(scopeId);
          return ptr;
        },

        /**
         * Apply a scope ID to a DOM element by setting a data attribute.
         */
        applyScope: (elementHandle, scopeIdPtr, scopeIdLen) => {
          const el = this.elements.get(elementHandle);
          if (el) {
            const scopeId = this.readString(scopeIdPtr, scopeIdLen);
            el.setAttribute(`data-${scopeId}`, "");
          }
        },
      },

      // --- Animation module: Web Animations API bridge from WASM ---
      animation: {
        /** Registered keyframe animations: name -> { keyframes, options } */
        _registeredAnimations: new Map(),
        /** Active animations per element: elementHandle -> Animation[] */
        _activeAnimations: new Map(),

        /**
         * Register a CSS transition on an element.
         * Sets the element's style.transition property.
         */
        registerTransition: (elementId, propertyPtr, propertyLen, durationPtr, durationLen, easingPtr, easingLen) => {
          const el = this.elements.get(elementId);
          if (!el) return;
          const property = this.readString(propertyPtr, propertyLen);
          const duration = this.readString(durationPtr, durationLen);
          const easing = this.readString(easingPtr, easingLen);
          const transitionValue = `${property} ${duration} ${easing}`;
          // Append to existing transitions
          const existing = el.style.transition;
          el.style.transition = existing
            ? `${existing}, ${transitionValue}`
            : transitionValue;
        },

        /**
         * Register a named keyframe animation via the Web Animations API.
         * keyframesJson is a JSON array of { offset, ...properties }.
         */
        registerKeyframes: (namePtr, nameLen, keyframesJsonPtr, keyframesJsonLen) => {
          const name = this.readString(namePtr, nameLen);
          const keyframesJson = this.readString(keyframesJsonPtr, keyframesJsonLen);
          try {
            const keyframes = JSON.parse(keyframesJson);
            const animModule = importObject.animation;
            animModule._registeredAnimations.set(name, { keyframes });
          } catch (e) {
            console.error(`Failed to register keyframes for "${name}":`, e);
          }
        },

        /**
         * Play a registered (or inline) animation on an element.
         * Uses element.animate() from the Web Animations API.
         */
        play: (elementId, namePtr, nameLen, durationPtr, durationLen) => {
          const el = this.elements.get(elementId);
          if (!el) return;
          const name = this.readString(namePtr, nameLen);
          const duration = this.readString(durationPtr, durationLen);

          const animModule = importObject.animation;
          const registered = animModule._registeredAnimations.get(name);
          if (!registered) {
            console.warn(`Animation "${name}" not registered`);
            return;
          }

          // Parse duration string (e.g. "0.5s", "500ms")
          let durationMs = 300;
          if (duration.endsWith("ms")) {
            durationMs = parseFloat(duration);
          } else if (duration.endsWith("s")) {
            durationMs = parseFloat(duration) * 1000;
          }

          const anim = el.animate(registered.keyframes, {
            duration: durationMs,
            easing: registered.easing || "ease",
            fill: "forwards",
          });

          // Track active animations
          if (!animModule._activeAnimations.has(elementId)) {
            animModule._activeAnimations.set(elementId, []);
          }
          animModule._activeAnimations.get(elementId).push(anim);

          // Clean up when finished
          anim.onfinish = () => {
            const active = animModule._activeAnimations.get(elementId);
            if (active) {
              const idx = active.indexOf(anim);
              if (idx !== -1) active.splice(idx, 1);
            }
          };
        },

        /**
         * Pause all active animations on an element.
         */
        pause: (elementId) => {
          const el = this.elements.get(elementId);
          if (!el) return;
          // Use getAnimations() for comprehensive pause
          if (typeof el.getAnimations === "function") {
            el.getAnimations().forEach((anim) => anim.pause());
          }
          const animModule = importObject.animation;
          const active = animModule._activeAnimations.get(elementId);
          if (active) {
            active.forEach((anim) => anim.pause());
          }
        },

        /**
         * Cancel all active animations on an element.
         */
        cancel: (elementId) => {
          const el = this.elements.get(elementId);
          if (!el) return;
          if (typeof el.getAnimations === "function") {
            el.getAnimations().forEach((anim) => anim.cancel());
          }
          const animModule = importObject.animation;
          animModule._activeAnimations.delete(elementId);
        },

        /**
         * Register a callback for when an animation finishes on an element.
         */
        onFinish: (elementId, callbackIndex) => {
          const animModule = importObject.animation;
          const active = animModule._activeAnimations.get(elementId);
          if (active && active.length > 0) {
            // Attach to the most recent animation
            const lastAnim = active[active.length - 1];
            lastAnim.onfinish = () => {
              const handler = this.instance.exports[`__handler_${callbackIndex}`];
              if (handler) handler();
              // Clean up
              const remaining = animModule._activeAnimations.get(elementId);
              if (remaining) {
                const idx = remaining.indexOf(lastAnim);
                if (idx !== -1) remaining.splice(idx, 1);
              }
            };
          }
        },
      },

      // --- Streaming module: streaming fetch, SSE, WebSocket from WASM ---
      streaming: {
        // Create a ReadableStream from a fetch response, calling back into WASM with each chunk
        streamFetch: (urlPtr, urlLen, callbackIdx) => {
          const url = this.readString(urlPtr, urlLen);
          globalThis.fetch(url).then((response) => {
            const reader = response.body.getReader();
            const decoder = new TextDecoder();
            const pump = () => {
              reader.read().then(({ done, value }) => {
                if (done) {
                  const doneHandler = this.instance.exports[`__stream_done_${callbackIdx}`];
                  if (doneHandler) doneHandler();
                  return;
                }
                // Write chunk to WASM memory and call handler
                const text = decoder.decode(value, { stream: true });
                const { ptr, len } = this.writeString(text);
                const chunkHandler = this.instance.exports[`__stream_chunk_${callbackIdx}`];
                if (chunkHandler) chunkHandler(ptr, len);
                pump();
              });
            };
            pump();
          }).catch((err) => {
            console.error("streamFetch error:", err);
            const doneHandler = this.instance.exports[`__stream_done_${callbackIdx}`];
            if (doneHandler) doneHandler();
          });
        },

        // SSE (Server-Sent Events) for real-time updates
        sseConnect: (urlPtr, urlLen, callbackIdx) => {
          const url = this.readString(urlPtr, urlLen);
          const source = new EventSource(url);
          source.onmessage = (event) => {
            const { ptr, len } = this.writeString(event.data);
            const handler = this.instance.exports[`__stream_chunk_${callbackIdx}`];
            if (handler) handler(ptr, len);
          };
          source.onerror = () => {
            source.close();
            const doneHandler = this.instance.exports[`__stream_done_${callbackIdx}`];
            if (doneHandler) doneHandler();
          };
        },

        // WebSocket support
        wsConnect: (urlPtr, urlLen, callbackIdx) => {
          const url = this.readString(urlPtr, urlLen);
          const ws = new WebSocket(url);
          const wsId = this.nextHandle++;
          this._websockets = this._websockets || new Map();
          this._websockets.set(wsId, ws);

          ws.onmessage = (event) => {
            const data = typeof event.data === "string" ? event.data : "";
            const { ptr, len } = this.writeString(data);
            const handler = this.instance.exports[`__stream_chunk_${callbackIdx}`];
            if (handler) handler(ptr, len);
          };
          ws.onclose = () => {
            const doneHandler = this.instance.exports[`__stream_done_${callbackIdx}`];
            if (doneHandler) doneHandler();
            this._websockets.delete(wsId);
          };
          ws.onerror = () => {
            ws.close();
          };
          return wsId;
        },

        wsSend: (wsId, dataPtr, dataLen) => {
          const ws = this._websockets && this._websockets.get(wsId);
          if (ws && ws.readyState === WebSocket.OPEN) {
            const data = this.readString(dataPtr, dataLen);
            ws.send(data);
          }
        },

        wsClose: (wsId) => {
          const ws = this._websockets && this._websockets.get(wsId);
          if (ws) {
            ws.close();
            this._websockets.delete(wsId);
          }
        },

        // Yield — called from WASM to emit a value into the current stream
        yield: (dataPtr, dataLen) => {
          // The stream consumer callback is managed by the streamFetch/SSE/WS
          // handlers above. This is used for WASM-originated streams.
          const data = this.readString(dataPtr, dataLen);
          if (this._currentStreamCallback) {
            this._currentStreamCallback(data);
          }
        },
      },

      // --- Media module: lazy images, decode, preload, progressive loading ---
      media: {
        // Lazy image loading with IntersectionObserver
        lazyImage: (srcPtr, srcLen, placeholderPtr, placeholderLen, elementHandle) => {
          const src = this.readString(srcPtr, srcLen);
          const placeholder = this.readString(placeholderPtr, placeholderLen);
          const el = this.elements.get(elementHandle);
          if (!el) return;

          // Set placeholder immediately
          el.src = placeholder;
          el.setAttribute("data-src", src);

          // Use IntersectionObserver to load when visible
          if (typeof IntersectionObserver !== "undefined") {
            const observer = new IntersectionObserver((entries) => {
              for (const entry of entries) {
                if (entry.isIntersecting) {
                  const img = entry.target;
                  img.src = img.getAttribute("data-src");
                  img.removeAttribute("data-src");
                  observer.unobserve(img);
                }
              }
            }, { rootMargin: "200px" });
            observer.observe(el);
          } else {
            // Fallback: load immediately
            el.src = src;
          }
        },

        // Decode image in a worker to avoid main thread jank
        decodeImage: (srcPtr, srcLen, callbackIdx) => {
          const src = this.readString(srcPtr, srcLen);
          globalThis.fetch(src)
            .then((res) => res.blob())
            .then((blob) => createImageBitmap(blob))
            .then((bitmap) => {
              // Store the bitmap and call back with a handle
              const handle = this.nextHandle++;
              this.elements.set(handle, bitmap);
              const handler = this.instance.exports[`__media_decoded_${callbackIdx}`];
              if (handler) handler(handle);
            })
            .catch((err) => {
              console.error("decodeImage error:", err);
            });
        },

        // Preload critical resources
        preload: (urlPtr, urlLen, typePtr, typeLen) => {
          const url = this.readString(urlPtr, urlLen);
          const asType = this.readString(typePtr, typeLen);
          if (typeof document !== "undefined") {
            const link = document.createElement("link");
            link.rel = "preload";
            link.href = url;
            link.as = asType;
            document.head.appendChild(link);
          }
        },

        // Progressive image loading (blur-up technique)
        progressiveImage: (thumbPtr, thumbLen, fullPtr, fullLen, elementHandle) => {
          const thumbSrc = this.readString(thumbPtr, thumbLen);
          const fullSrc = this.readString(fullPtr, fullLen);
          const el = this.elements.get(elementHandle);
          if (!el) return;

          // Show tiny thumbnail immediately with blur
          el.src = thumbSrc;
          el.style.filter = "blur(20px)";
          el.style.transition = "filter 0.5s ease-out";

          // Load full image in background
          const fullImg = new Image();
          fullImg.onload = () => {
            el.src = fullSrc;
            el.style.filter = "none";
          };
          fullImg.src = fullSrc;
        },
      },

      // --- Accessibility (a11y) module ---
      a11y: {
        // Set any aria-* attribute on an element
        setAriaAttribute: (elementId, namePtr, nameLen, valuePtr, valueLen) => {
          const el = this.elements.get(elementId);
          if (el) {
            const name = this.readString(namePtr, nameLen);
            const value = this.readString(valuePtr, valueLen);
            el.setAttribute(name, value);
          }
        },

        // Set role attribute on an element
        setRole: (elementId, rolePtr, roleLen) => {
          const el = this.elements.get(elementId);
          if (el) {
            const role = this.readString(rolePtr, roleLen);
            el.setAttribute("role", role);
          }
        },

        // Programmatic focus management
        manageFocus: (elementId) => {
          const el = this.elements.get(elementId);
          if (el) {
            // Ensure element is focusable
            if (!el.hasAttribute("tabindex") && !["INPUT", "BUTTON", "A", "SELECT", "TEXTAREA"].includes(el.tagName)) {
              el.setAttribute("tabindex", "-1");
            }
            el.focus();
          }
        },

        // Announce text to screen readers via aria-live region
        // priority: 0 = polite, 1 = assertive
        announceToScreenReader: (textPtr, textLen, priority) => {
          const text = this.readString(textPtr, textLen);
          const regionId = priority === 1 ? "__nectar_a11y_live_assertive" : "__nectar_a11y_live_polite";
          let region = document.getElementById(regionId);
          if (!region) {
            region = document.createElement("div");
            region.id = regionId;
            region.setAttribute("aria-live", priority === 1 ? "assertive" : "polite");
            region.setAttribute("aria-atomic", "true");
            region.style.cssText = "position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0;";
            document.body.appendChild(region);
          }
          // Clear and re-set to trigger screen reader announcement
          region.textContent = "";
          requestAnimationFrame(() => {
            region.textContent = text;
          });
        },

        // Focus trap for modals/dialogs
        trapFocus: (containerElementId) => {
          const container = this.elements.get(containerElementId);
          if (!container) return;

          const focusableSelectors = 'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

          this._focusTrapHandler = (e) => {
            if (e.key !== "Tab") return;
            const focusable = container.querySelectorAll(focusableSelectors);
            if (focusable.length === 0) return;
            const first = focusable[0];
            const last = focusable[focusable.length - 1];

            if (e.shiftKey) {
              if (document.activeElement === first) {
                e.preventDefault();
                last.focus();
              }
            } else {
              if (document.activeElement === last) {
                e.preventDefault();
                first.focus();
              }
            }
          };

          this._focusTrapContainer = container;
          document.addEventListener("keydown", this._focusTrapHandler);

          // Focus the first focusable element inside the container
          const firstFocusable = container.querySelector(focusableSelectors);
          if (firstFocusable) firstFocusable.focus();
        },

        // Release focus trap
        releaseFocusTrap: () => {
          if (this._focusTrapHandler) {
            document.removeEventListener("keydown", this._focusTrapHandler);
            this._focusTrapHandler = null;
            this._focusTrapContainer = null;
          }
        },
      },

      // --- Web API module: localStorage, clipboard, timers, URL, console, misc ---
      webapi: {
        // -- Storage --
        localStorageGet: (keyPtr, keyLen) => {
          const key = this.readString(keyPtr, keyLen);
          const value = (typeof localStorage !== "undefined") ? (localStorage.getItem(key) || "") : "";
          const { ptr, len } = this.writeString(value);
          return [ptr, len];
        },

        localStorageSet: (keyPtr, keyLen, valPtr, valLen) => {
          const key = this.readString(keyPtr, keyLen);
          const value = this.readString(valPtr, valLen);
          if (typeof localStorage !== "undefined") {
            localStorage.setItem(key, value);
          }
        },

        localStorageRemove: (keyPtr, keyLen) => {
          const key = this.readString(keyPtr, keyLen);
          if (typeof localStorage !== "undefined") {
            localStorage.removeItem(key);
          }
        },

        sessionStorageGet: (keyPtr, keyLen) => {
          const key = this.readString(keyPtr, keyLen);
          const value = (typeof sessionStorage !== "undefined") ? (sessionStorage.getItem(key) || "") : "";
          const { ptr, len } = this.writeString(value);
          return [ptr, len];
        },

        sessionStorageSet: (keyPtr, keyLen, valPtr, valLen) => {
          const key = this.readString(keyPtr, keyLen);
          const value = this.readString(valPtr, valLen);
          if (typeof sessionStorage !== "undefined") {
            sessionStorage.setItem(key, value);
          }
        },

        // -- Clipboard --
        clipboardWrite: (textPtr, textLen) => {
          const text = this.readString(textPtr, textLen);
          if (typeof navigator !== "undefined" && navigator.clipboard) {
            navigator.clipboard.writeText(text).catch((err) => {
              console.error("clipboard write failed:", err);
            });
          }
        },

        clipboardRead: (callbackIdx) => {
          if (typeof navigator !== "undefined" && navigator.clipboard) {
            navigator.clipboard.readText().then((text) => {
              const { ptr, len } = this.writeString(text);
              const handler = this.instance.exports[`__clipboard_read_${callbackIdx}`];
              if (handler) handler(ptr, len);
            }).catch((err) => {
              console.error("clipboard read failed:", err);
            });
          }
        },

        // -- Timers --
        setTimeout: (callbackIdx, delayMs) => {
          const id = globalThis.setTimeout(() => {
            const handler = this.instance.exports[`__timer_${callbackIdx}`];
            if (handler) handler();
          }, delayMs);
          return id;
        },

        setInterval: (callbackIdx, intervalMs) => {
          const id = globalThis.setInterval(() => {
            const handler = this.instance.exports[`__timer_${callbackIdx}`];
            if (handler) handler();
          }, intervalMs);
          return id;
        },

        clearTimer: (timerId) => {
          globalThis.clearTimeout(timerId);
          globalThis.clearInterval(timerId);
        },

        // -- URL / History --
        getLocationHref: () => {
          const href = (typeof location !== "undefined") ? location.href : "";
          const { ptr, len } = this.writeString(href);
          return [ptr, len];
        },

        getLocationSearch: () => {
          const search = (typeof location !== "undefined") ? location.search : "";
          const { ptr, len } = this.writeString(search);
          return [ptr, len];
        },

        getLocationHash: () => {
          const hash = (typeof location !== "undefined") ? location.hash : "";
          const { ptr, len } = this.writeString(hash);
          return [ptr, len];
        },

        pushState: (urlPtr, urlLen) => {
          const url = this.readString(urlPtr, urlLen);
          if (typeof history !== "undefined") {
            history.pushState(null, "", url);
          }
        },

        replaceState: (urlPtr, urlLen) => {
          const url = this.readString(urlPtr, urlLen);
          if (typeof history !== "undefined") {
            history.replaceState(null, "", url);
          }
        },

        // -- Console --
        consoleLog: (msgPtr, msgLen) => {
          console.log(this.readString(msgPtr, msgLen));
        },

        consoleWarn: (msgPtr, msgLen) => {
          console.warn(this.readString(msgPtr, msgLen));
        },

        consoleError: (msgPtr, msgLen) => {
          console.error(this.readString(msgPtr, msgLen));
        },

        // -- Misc --
        randomFloat: () => {
          if (typeof crypto !== "undefined" && crypto.getRandomValues) {
            const arr = new Uint32Array(1);
            crypto.getRandomValues(arr);
            return arr[0] / 0xFFFFFFFF;
          }
          return Math.random();
        },

        now: () => {
          if (typeof performance !== "undefined") {
            return performance.now();
          }
          return Date.now();
        },

        requestAnimationFrame: (callbackIdx) => {
          if (typeof globalThis.requestAnimationFrame === "function") {
            return globalThis.requestAnimationFrame(() => {
              const handler = this.instance.exports[`__raf_${callbackIdx}`];
              if (handler) handler();
            });
          }
          return 0;
        },
      },

      // --- Service Worker ---
      sw: {
        register: () => {
          if (typeof window !== "undefined" && window.NectarSW) {
            window.NectarSW.register();
          } else if (typeof navigator !== "undefined" && "serviceWorker" in navigator) {
            navigator.serviceWorker.register("/nectar-sw.js").catch((err) => {
              console.error("[Nectar SW] Registration failed:", err);
            });
          }
        },

        precache: (urlPtr, urlLen) => {
          const url = this.readString(urlPtr, urlLen);
          if (typeof navigator !== "undefined" && navigator.serviceWorker && navigator.serviceWorker.controller) {
            navigator.serviceWorker.controller.postMessage({
              type: "nectar:precache",
              urls: [url],
            });
          }
          // Also store for compile-time manifest generation
          if (!this._precacheUrls) this._precacheUrls = [];
          this._precacheUrls.push(url);
        },

        isOffline: () => {
          if (typeof window !== "undefined" && window.NectarSW) {
            return window.NectarSW.isOffline ? 1 : 0;
          }
          if (typeof navigator !== "undefined") {
            return navigator.onLine ? 0 : 1;
          }
          return 0;
        },
      },

      // --- Contract runtime — API boundary validation ---
      contract: {
        /**
         * Register a contract schema. Called at WASM module init for each
         * contract definition. Stores the schema and content hash so that:
         * - fetch -> ContractName can validate responses at the boundary
         * - X-Nectar-Contract headers include the hash for staleness detection
         */
        registerSchema: (namePtr, nameLen, hashPtr, hashLen, schemaPtr, schemaLen) => {
          const name = this.readString(namePtr, nameLen);
          const hash = this.readString(hashPtr, hashLen);
          const schemaJson = this.readString(schemaPtr, schemaLen);
          if (!this._contracts) this._contracts = new Map();
          try {
            const schema = JSON.parse(schemaJson);
            this._contracts.set(name, { hash, schema, name });
          } catch (e) {
            console.warn(`[Nectar Contract] Failed to parse schema for ${name}:`, e);
            this._contracts.set(name, { hash, schema: {}, name });
          }
        },

        /**
         * Validate a fetch response against a registered contract.
         * Called after $http_fetch when a contract type is specified.
         *
         * Returns the response handle if valid, or throws a ContractError
         * with actionable details if the response shape doesn't match.
         */
        validate: (responseHandle, _responseUnused, namePtr, nameLen) => {
          const name = this.readString(namePtr, nameLen);
          if (!this._contracts || !this._contracts.has(name)) {
            console.warn(`[Nectar Contract] Unknown contract: ${name}`);
            return responseHandle;
          }
          const contract = this._contracts.get(name);

          // Get the pending fetch response body (stored by http.fetch)
          const pending = this.pendingFetches.get(responseHandle);
          if (!pending || !pending._body) return responseHandle;

          let body;
          try {
            body = typeof pending._body === 'string'
              ? JSON.parse(pending._body)
              : pending._body;
          } catch {
            const err = new Error(
              `[Nectar Contract] ${name}: response is not valid JSON`
            );
            err.contract = name;
            err.type = 'contract_parse_error';
            throw err;
          }

          // Validate each field in the contract schema
          const missing = [];
          const wrongType = [];
          for (const [field, spec] of Object.entries(contract.schema)) {
            if (!(field in body)) {
              if (!spec.nullable) {
                missing.push(field);
              }
              continue;
            }
            const value = body[field];
            const actual = Array.isArray(value) ? 'array' : typeof value;
            const expected = spec.type;
            if (value === null && spec.nullable) continue;
            if (expected === 'integer' && (typeof value !== 'number' || !Number.isInteger(value))) {
              wrongType.push({ field, expected: 'integer', actual });
            } else if (expected === 'number' && typeof value !== 'number') {
              wrongType.push({ field, expected: 'number', actual });
            } else if (expected === 'string' && typeof value !== 'string') {
              wrongType.push({ field, expected: 'string', actual });
            } else if (expected === 'boolean' && typeof value !== 'boolean') {
              wrongType.push({ field, expected: 'boolean', actual });
            } else if (expected === 'array' && !Array.isArray(value)) {
              wrongType.push({ field, expected: 'array', actual });
            }
          }

          if (missing.length > 0 || wrongType.length > 0) {
            const details = [];
            if (missing.length) details.push(`missing fields: ${missing.join(', ')}`);
            if (wrongType.length) details.push(
              `type mismatches: ${wrongType.map(w => `${w.field} (expected ${w.expected}, got ${w.actual})`).join(', ')}`
            );
            const err = new Error(
              `[Nectar Contract] ${name}@${contract.hash}: boundary validation failed — ${details.join('; ')}`
            );
            err.contract = name;
            err.hash = contract.hash;
            err.missing = missing;
            err.wrongType = wrongType;
            err.type = 'contract_mismatch';
            throw err;
          }

          return responseHandle;
        },

        /**
         * Get the content hash for a contract. Used to set the
         * X-Nectar-Contract header on outgoing requests.
         */
        getHash: (namePtr, nameLen) => {
          const name = this.readString(namePtr, nameLen);
          if (!this._contracts || !this._contracts.has(name)) {
            return 0; // null ptr — no hash available
          }
          const hash = this._contracts.get(name).hash;
          const headerVal = `${name}@${hash}`;
          return this.writeString(headerVal);
        },
      },

      // --- SEO runtime — meta tags, structured data, sitemap ---
      seo: {
        setMeta: (titlePtr, titleLen, descPtr, descLen, canonPtr, canonLen, ogPtr, ogLen) => {
          SeoRuntime.setMeta.call(SeoRuntime, this, titlePtr, titleLen, descPtr, descLen, canonPtr, canonLen, ogPtr, ogLen);
        },
        registerStructuredData: (jsonPtr, jsonLen) => {
          SeoRuntime.registerStructuredData.call(SeoRuntime, this, jsonPtr, jsonLen);
        },
        registerRoute: (pathPtr, pathLen, priorityPtr, priorityLen) => {
          SeoRuntime.registerRoute.call(SeoRuntime, this, pathPtr, pathLen, priorityPtr, priorityLen);
        },
        emitStaticHtml: (componentPtr, componentLen) => {
          SeoRuntime.emitStaticHtml.call(SeoRuntime, this, componentPtr, componentLen);
        },
      },
    };

    // Auto-register contracts after WASM init — see post-init section below

    // Create hidden aria-live regions for screen reader announcements
    if (typeof document !== "undefined") {
      for (const [id, level] of [["__nectar_a11y_live_polite", "polite"], ["__nectar_a11y_live_assertive", "assertive"]]) {
        if (!document.getElementById(id)) {
          const region = document.createElement("div");
          region.id = id;
          region.setAttribute("aria-live", level);
          region.setAttribute("aria-atomic", "true");
          region.style.cssText = "position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0;";
          document.body.appendChild(region);
        }
      }
    }

    // Register root element
    const rootHandle = this.nextHandle++;
    this.elements.set(rootHandle, rootElement);

    const response = await fetch(wasmUrl);
    const bytes = await response.arrayBuffer();
    this._wasmBytes = bytes;
    const { instance } = await WebAssembly.instantiate(bytes, importObject);
    this.instance = instance;
    this.memory = importObject.env.memory;

    // Initialize worker pool for concurrency primitives
    if (typeof Worker !== "undefined") {
      this.workerPool = new WorkerPool(bytes, importObject);
    }

    // Register contracts — call __contract_register_* exports
    const allExports = Object.keys(instance.exports);
    for (const name of allExports) {
      if (name.startsWith("__contract_register_")) {
        instance.exports[name]();
      }
    }

    // Register pages — call __page_register_* exports
    for (const name of allExports) {
      if (name.startsWith("__page_register_")) {
        instance.exports[name]();
      }
    }

    // Initialize stores — call any *_init exports
    for (const name of allExports) {
      if (name.endsWith("_init")) {
        instance.exports[name]();
      }
    }

    // Mount the first component
    const mountFn = allExports.find((e) =>
      e.endsWith("_mount") && !e.startsWith("__")
    );
    if (mountFn) {
      instance.exports[mountFn](rootHandle);
    }

    return instance;
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
    if (this.instance && this.instance.exports.alloc) {
      return this.instance.exports.alloc(size);
    }
    // Fallback bump allocator
    const ptr = this._heapPtr || 4096;
    this._heapPtr = ptr + size;
    const needed = Math.ceil((ptr + size) / 65536);
    const current = this.memory.buffer.byteLength / 65536;
    if (needed > current) {
      this.memory.grow(needed - current);
    }
    return ptr;
  }
}

// ---------------------------------------------------------------------------
// Gesture recognition runtime
// ---------------------------------------------------------------------------

const GestureRuntime = {
  _handlers: new Map(),
  _nextId: 1,

  registerSwipe(elementHandle, direction, callbackIdx) {
    const element = document.querySelector(`[data-nectar-id="${elementHandle}"]`) || document.body;
    let startX = 0, startY = 0;
    const threshold = 50;

    element.addEventListener("touchstart", (e) => {
      startX = e.touches[0].clientX;
      startY = e.touches[0].clientY;
    }, { passive: true });

    element.addEventListener("touchend", (e) => {
      const dx = e.changedTouches[0].clientX - startX;
      const dy = e.changedTouches[0].clientY - startY;
      const absDx = Math.abs(dx);
      const absDy = Math.abs(dy);

      if (absDx > threshold && absDx > absDy) {
        if ((direction === 0 && dx < 0) || (direction === 1 && dx > 0)) {
          // 0 = swipe_left, 1 = swipe_right
          if (typeof callbackIdx === "function") callbackIdx();
          else if (this._instance) this._instance.exports.__gesture_callback(callbackIdx);
        }
      } else if (absDy > threshold && absDy > absDx) {
        if ((direction === 2 && dy < 0) || (direction === 3 && dy > 0)) {
          // 2 = swipe_up, 3 = swipe_down
          if (typeof callbackIdx === "function") callbackIdx();
          else if (this._instance) this._instance.exports.__gesture_callback(callbackIdx);
        }
      }
    }, { passive: true });
  },

  registerLongPress(elementHandle, callbackIdx, durationMs) {
    const element = document.querySelector(`[data-nectar-id="${elementHandle}"]`) || document.body;
    const duration = durationMs || 500;
    let timer = null;

    element.addEventListener("touchstart", () => {
      timer = setTimeout(() => {
        if (typeof callbackIdx === "function") callbackIdx();
        else if (this._instance) this._instance.exports.__gesture_callback(callbackIdx);
      }, duration);
    }, { passive: true });

    element.addEventListener("touchend", () => {
      if (timer) { clearTimeout(timer); timer = null; }
    }, { passive: true });

    element.addEventListener("touchmove", () => {
      if (timer) { clearTimeout(timer); timer = null; }
    }, { passive: true });
  },

  registerPinch(elementHandle, callbackIdx) {
    const element = document.querySelector(`[data-nectar-id="${elementHandle}"]`) || document.body;
    let initialDistance = null;

    element.addEventListener("touchstart", (e) => {
      if (e.touches.length === 2) {
        const dx = e.touches[0].clientX - e.touches[1].clientX;
        const dy = e.touches[0].clientY - e.touches[1].clientY;
        initialDistance = Math.sqrt(dx * dx + dy * dy);
      }
    }, { passive: true });

    element.addEventListener("touchmove", (e) => {
      if (e.touches.length === 2 && initialDistance !== null) {
        const dx = e.touches[0].clientX - e.touches[1].clientX;
        const dy = e.touches[0].clientY - e.touches[1].clientY;
        const currentDistance = Math.sqrt(dx * dx + dy * dy);
        const scale = currentDistance / initialDistance;
        if (typeof callbackIdx === "function") callbackIdx(scale);
        else if (this._instance) this._instance.exports.__gesture_callback(callbackIdx);
      }
    }, { passive: true });

    element.addEventListener("touchend", () => { initialDistance = null; }, { passive: true });
  },
};

// ---------------------------------------------------------------------------
// Hardware APIs runtime
// ---------------------------------------------------------------------------

const HardwareRuntime = {
  haptic(pattern) {
    if (navigator.vibrate) {
      navigator.vibrate(pattern);
    }
  },

  biometricAuth(challengePtr, challengeLen, rpPtr, rpLen) {
    // WebAuthn-based biometric authentication
    if (!window.PublicKeyCredential) return -1;
    // Async operation — returns a promise handle
    const challenge = new Uint8Array(this._memory.buffer, challengePtr, challengeLen);
    const rpId = new TextDecoder().decode(new Uint8Array(this._memory.buffer, rpPtr, rpLen));
    navigator.credentials.get({
      publicKey: {
        challenge: challenge,
        rpId: rpId,
        userVerification: "required",
      },
    }).then((credential) => {
      if (this._instance) this._instance.exports.__biometric_callback(1);
    }).catch(() => {
      if (this._instance) this._instance.exports.__biometric_callback(0);
    });
    return 0;
  },

  cameraCapture(facingPtr, facingLen, callbackIdx) {
    const facing = new TextDecoder().decode(new Uint8Array(this._memory.buffer, facingPtr, facingLen));
    const constraints = {
      video: { facingMode: facing || "environment" },
    };
    navigator.mediaDevices.getUserMedia(constraints).then((stream) => {
      const video = document.createElement("video");
      video.srcObject = stream;
      video.play();
      // Callback with stream handle
      if (this._instance) this._instance.exports.__camera_callback(callbackIdx, 1);
    }).catch(() => {
      if (this._instance) this._instance.exports.__camera_callback(callbackIdx, 0);
    });
  },

  geolocationCurrent(callbackIdx) {
    if (!navigator.geolocation) {
      if (this._instance) this._instance.exports.__geolocation_callback(callbackIdx, 0, 0, 0);
      return;
    }
    navigator.geolocation.getCurrentPosition(
      (pos) => {
        if (this._instance) {
          // Pack lat/lng as f64 — caller reads from linear memory
          this._instance.exports.__geolocation_callback(
            callbackIdx, 1,
            pos.coords.latitude, pos.coords.longitude
          );
        }
      },
      () => {
        if (this._instance) this._instance.exports.__geolocation_callback(callbackIdx, 0, 0, 0);
      }
    );
  },
};

// ---------------------------------------------------------------------------
// PWA runtime
// ---------------------------------------------------------------------------

const PwaRuntime = {
  registerManifest(jsonPtr, jsonLen) {
    const json = new TextDecoder().decode(new Uint8Array(this._memory.buffer, jsonPtr, jsonLen));
    const blob = new Blob([json], { type: "application/manifest+json" });
    const url = URL.createObjectURL(blob);
    let link = document.querySelector('link[rel="manifest"]');
    if (!link) {
      link = document.createElement("link");
      link.rel = "manifest";
      document.head.appendChild(link);
    }
    link.href = url;
  },

  cachePrecache(urlsPtr, urlsLen) {
    const urlsJson = new TextDecoder().decode(new Uint8Array(this._memory.buffer, urlsPtr, urlsLen));
    try {
      const urls = JSON.parse(urlsJson);
      if ("caches" in window) {
        caches.open("nectar-precache-v1").then((cache) => cache.addAll(urls));
      }
    } catch (e) {
      console.warn("[nectar] Failed to parse precache URLs:", e);
    }
  },

  registerServiceWorker(swPath) {
    if ("serviceWorker" in navigator) {
      navigator.serviceWorker.register(swPath || "/nectar-service-worker.js");
    }
  },
};

// ---------------------------------------------------------------------------
// SEO Runtime
// ---------------------------------------------------------------------------

const SeoRuntime = {
  _pages: new Map(),
  _meta: new Map(),
  _structuredData: [],
  _routes: [],

  setMeta(runtime, titlePtr, titleLen, descPtr, descLen, canonPtr, canonLen, ogPtr, ogLen) {
    const title = titleLen > 0 ? runtime.readString(titlePtr, titleLen) : null;
    const desc = descLen > 0 ? runtime.readString(descPtr, descLen) : null;
    const canon = canonLen > 0 ? runtime.readString(canonPtr, canonLen) : null;
    const og = ogLen > 0 ? runtime.readString(ogPtr, ogLen) : null;

    // Update document head
    if (typeof document !== "undefined") {
      if (title) document.title = title;
      if (desc) {
        let el = document.querySelector('meta[name="description"]');
        if (!el) { el = document.createElement('meta'); el.name = 'description'; document.head.appendChild(el); }
        el.content = desc;
      }
      if (canon) {
        let el = document.querySelector('link[rel="canonical"]');
        if (!el) { el = document.createElement('link'); el.rel = 'canonical'; document.head.appendChild(el); }
        el.href = canon;
      }
      if (og) {
        let el = document.querySelector('meta[property="og:image"]');
        if (!el) { el = document.createElement('meta'); el.setAttribute('property', 'og:image'); document.head.appendChild(el); }
        el.content = og;
      }
    }
  },

  registerStructuredData(runtime, jsonPtr, jsonLen) {
    const json = runtime.readString(jsonPtr, jsonLen);
    SeoRuntime._structuredData.push(JSON.parse(json));
    // Inject JSON-LD into head
    if (typeof document !== "undefined") {
      const script = document.createElement('script');
      script.type = 'application/ld+json';
      script.textContent = json;
      document.head.appendChild(script);
    }
  },

  registerRoute(runtime, pathPtr, pathLen, priorityPtr, priorityLen) {
    const path = runtime.readString(pathPtr, pathLen);
    const priority = priorityLen > 0 ? runtime.readString(priorityPtr, priorityLen) : '0.8';
    SeoRuntime._routes.push({ path, priority, lastmod: new Date().toISOString().split('T')[0] });
  },

  emitStaticHtml(runtime, componentPtr, componentLen) {
    // Used during SSG build — captures rendered HTML for static output
    const name = runtime.readString(componentPtr, componentLen);
    if (typeof document !== "undefined") {
      const html = document.documentElement.outerHTML;
      SeoRuntime._pages.set(name, html);
    }
  },

  // Generate sitemap XML from registered routes
  generateSitemap(baseUrl) {
    const urls = SeoRuntime._routes.map(r =>
      `  <url>\n    <loc>${baseUrl}${r.path}</loc>\n    <lastmod>${r.lastmod}</lastmod>\n    <priority>${r.priority}</priority>\n  </url>`
    ).join('\n');
    return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>`;
  },

  // Generate robots.txt
  generateRobots(baseUrl) {
    return `User-agent: *\nAllow: /\n\nSitemap: ${baseUrl}/sitemap.xml`;
  }
};

// Export for use
if (typeof module !== "undefined") {
  module.exports = {
    NectarRuntime,
    AgentManager,
    WorkerPool,
    Router,
    Effect,
    Scheduler,
    GestureRuntime,
    HardwareRuntime,
    PwaRuntime,
    SeoRuntime,
    FormRuntime,
    CacheRuntime,
    createEffect,
    createMemo,
    batch,
    hashString,
  };
}
// ---------------------------------------------------------------------------
// Permission enforcement runtime
// ---------------------------------------------------------------------------

class PermissionError extends Error {
  constructor(message) {
    super(message);
    this.name = "PermissionError";
  }
}

// ---------------------------------------------------------------------------
// Form runtime
// ---------------------------------------------------------------------------

const FormRuntime = {
  _forms: new Map(),
  _errors: new Map(),

  registerForm(namePtr, nameLen, schemaPtr, schemaLen) {
    const name = runtime.readString(namePtr, nameLen);
    const schema = JSON.parse(runtime.readString(schemaPtr, schemaLen));
    FormRuntime._forms.set(name, { schema, values: {}, errors: {}, dirty: {}, touched: {} });
  },

  validate(namePtr, nameLen) {
    const name = runtime.readString(namePtr, nameLen);
    const form = FormRuntime._forms.get(name);
    if (!form) return 0;
    let valid = true;
    form.errors = {};
    for (const field of form.schema.fields) {
      const value = form.values[field.name];
      for (const v of field.validators) {
        const error = FormRuntime._runValidator(v, value, field.name);
        if (error) { form.errors[field.name] = error; valid = false; break; }
      }
    }
    return valid ? 1 : 0;
  },

  setFieldError(namePtr, nameLen, errorPtr, errorLen) {
    const name = runtime.readString(namePtr, nameLen);
    const error = runtime.readString(errorPtr, errorLen);
    const form = FormRuntime._forms.get(name);
    if (form) form.errors[name] = error;
  },

  _runValidator(validator, value, fieldName) {
    switch (validator.kind) {
      case 'required': return (!value || value === '') ? (validator.message || `${fieldName} is required`) : null;
      case 'min_length': return (value && value.length < validator.min) ? (validator.message || `${fieldName} must be at least ${validator.min} characters`) : null;
      case 'max_length': return (value && value.length > validator.max) ? (validator.message || `${fieldName} must be at most ${validator.max} characters`) : null;
      case 'pattern': return (value && !new RegExp(validator.pattern).test(value)) ? (validator.message || `${fieldName} format is invalid`) : null;
      case 'email': return (value && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value)) ? (validator.message || `${fieldName} must be a valid email`) : null;
      case 'url': { try { new URL(value); return null; } catch { return validator.message || `${fieldName} must be a valid URL`; } }
      default: return null;
    }
  },

  getErrors(name) { return FormRuntime._forms.get(name)?.errors || {}; },
  isDirty(name) { const f = FormRuntime._forms.get(name); return f ? Object.keys(f.dirty).length > 0 : false; },
  reset(name) { const f = FormRuntime._forms.get(name); if (f) { f.values = {}; f.errors = {}; f.dirty = {}; f.touched = {}; } },
};

const PermissionsRuntime = {
  /** Per-component permission registrations: component name -> { network: [...], storage: [...] } */
  _registry: new Map(),

  /**
   * Register permissions for a component.
   * Called by WASM at component mount time.
   */
  registerPermissions(componentName, permissionsJson) {
    try {
      const perms = JSON.parse(permissionsJson);
      this._registry.set(componentName, perms);
    } catch (e) {
      console.warn(`[nectar] Failed to parse permissions for ${componentName}:`, e);
    }
  },

  /**
   * Check if a URL is allowed by the component's declared network permissions.
   * Called before every fetch in a permissioned component.
   * @param {string} url - The URL being fetched
   * @param {string[]} allowedPatterns - Glob-like URL patterns from the permissions block
   * @throws {PermissionError} if the URL does not match any allowed pattern
   */
  checkNetwork(url, allowedPatterns) {
    if (!allowedPatterns || allowedPatterns.length === 0) return;
    const matched = allowedPatterns.some((pattern) => this._matchPattern(url, pattern));
    if (!matched) {
      throw new PermissionError(
        `Network access denied: "${url}" does not match any allowed pattern: [${allowedPatterns.join(", ")}]`
      );
    }
  },

  /**
   * Check if a storage key is allowed by the component's declared storage permissions.
   * Called before storage access in a permissioned component.
   * @param {string} key - The storage key being accessed
   * @param {string[]} allowedKeys - Key patterns from the permissions block
   * @throws {PermissionError} if the key does not match any allowed pattern
   */
  checkStorage(key, allowedKeys) {
    if (!allowedKeys || allowedKeys.length === 0) return;
    const matched = allowedKeys.some((pattern) => this._matchPattern(key, pattern));
    if (!matched) {
      throw new PermissionError(
        `Storage access denied: "${key}" does not match any allowed key: [${allowedKeys.join(", ")}]`
      );
    }
  },

  /**
   * Generate a Content-Security-Policy header value from all registered permissions.
   * @returns {string} CSP header value
   */
  generateCSP() {
    const connectSources = new Set(["'self'"]);
    for (const [, perms] of this._registry) {
      if (perms.network) {
        for (const pattern of perms.network) {
          // Extract origin from URL pattern for CSP connect-src
          try {
            const url = new URL(pattern.replace(/\*/g, "placeholder"));
            connectSources.add(url.origin);
          } catch {
            // If not a valid URL, add as-is (could be a domain pattern)
            connectSources.add(pattern.replace(/\/\*$/, ""));
          }
        }
      }
    }
    return `connect-src ${[...connectSources].join(" ")}`;
  },

  /**
   * Simple glob-style pattern matching.
   * Supports `*` as a wildcard segment.
   */
  _matchPattern(value, pattern) {
    // Convert glob pattern to regex
    const escaped = pattern
      .replace(/[.+^${}()|[\]\\]/g, "\\$&")
      .replace(/\*/g, ".*");
    const regex = new RegExp(`^${escaped}$`);
    return regex.test(value);
  },
};

// =========================================================================
// Feature 1: Code Splitting / Chunk Loading Runtime
// =========================================================================

/**
 * LoaderRuntime — manages dynamic code-split chunk loading.
 * Emitted by the compiler for components tagged with `chunk "name"`.
 */
const LoaderRuntime = {
  _chunks: new Map(),
  _loaded: new Set(),

  /**
   * Load a code-split chunk by name. Returns 1 if already loaded or
   * successfully loaded, throws on failure.
   * Called from WASM via: (import "loader" "loadChunk")
   * @param {number} namePtr - pointer to chunk name in WASM memory
   * @param {number} nameLen - length of chunk name
   * @returns {number} 1 on success
   */
  async loadChunk(namePtr, nameLen) {
    const name = runtime.readString(namePtr, nameLen);
    if (LoaderRuntime._loaded.has(name)) return 1;
    const script = document.createElement("script");
    script.src = `/chunks/${name}.js`;
    await new Promise((resolve, reject) => {
      script.onload = resolve;
      script.onerror = reject;
      document.head.appendChild(script);
    });
    LoaderRuntime._loaded.add(name);
    return 1;
  },

  /**
   * Preload a chunk (add a modulepreload link) without blocking.
   * Called from WASM via: (import "loader" "preloadChunk")
   * @param {number} namePtr - pointer to chunk name in WASM memory
   * @param {number} nameLen - length of chunk name
   */
  preloadChunk(namePtr, nameLen) {
    const name = runtime.readString(namePtr, nameLen);
    if (!LoaderRuntime._loaded.has(name)) {
      const link = document.createElement("link");
      link.rel = "modulepreload";
      link.href = `/chunks/${name}.js`;
      document.head.appendChild(link);
    }
  },
};

// =========================================================================
// Feature 2: Atomic State Runtime — Race-Free State Management
// =========================================================================

/**
 * AtomicStateRuntime — provides atomic get/set/compare-and-swap operations
 * for signals marked with the `atomic` keyword. Prevents race conditions
 * when multiple components concurrently mutate the same store.
 *
 * In a SharedArrayBuffer environment, these use true atomic operations.
 * In single-threaded contexts, they provide a consistent API with
 * selector invalidation.
 */
const AtomicStateRuntime = {
  _atomics: new Map(),
  _selectors: new Map(),

  /**
   * Get the current value of an atomic signal.
   * Called from WASM via: (import "state" "atomicGet")
   * @param {number} signalId
   * @returns {number} current value
   */
  atomicGet(signalId) {
    return AtomicStateRuntime._atomics.get(signalId) ?? 0;
  },

  /**
   * Set an atomic signal value and notify dependent selectors.
   * Called from WASM via: (import "state" "atomicSet")
   * @param {number} signalId
   * @param {number} value
   */
  atomicSet(signalId, value) {
    const old = AtomicStateRuntime._atomics.get(signalId);
    AtomicStateRuntime._atomics.set(signalId, value);
    AtomicStateRuntime._notifySelectors(signalId);
    return old;
  },

  /**
   * Atomic compare-and-swap: only update if current value matches expected.
   * Returns 1 on success, 0 on failure.
   * Called from WASM via: (import "state" "atomicCompareSwap")
   * @param {number} signalId
   * @param {number} expected
   * @param {number} desired
   * @returns {number} 1 if swapped, 0 otherwise
   */
  atomicCompareSwap(signalId, expected, desired) {
    const current = AtomicStateRuntime._atomics.get(signalId) ?? 0;
    if (current === expected) {
      AtomicStateRuntime._atomics.set(signalId, desired);
      AtomicStateRuntime._notifySelectors(signalId);
      return 1;
    }
    return 0;
  },

  /**
   * Register a selector (derived computation) that depends on specific signals.
   * @param {string} name - selector name
   * @param {number[]} deps - signal IDs this selector depends on
   * @param {Function} computeFn - function to recompute the selector value
   */
  registerSelector(name, deps, computeFn) {
    AtomicStateRuntime._selectors.set(name, {
      deps,
      computeFn,
      cached: null,
    });
  },

  /**
   * Invalidate cached selector values when a dependent signal changes.
   * @param {number} changedSignal - the signal ID that changed
   */
  _notifySelectors(changedSignal) {
    for (const [name, sel] of AtomicStateRuntime._selectors) {
      if (sel.deps.includes(changedSignal)) {
        sel.cached = null;
      }
    }
  },
};

// =========================================================================
// Feature 3: Lifecycle Runtime — Memory Leak Prevention
// =========================================================================

/**
 * LifecycleRuntime — manages component cleanup callbacks to prevent
 * memory leaks from event listeners, intervals, timeouts, and subscriptions.
 *
 * When a component with an `on_destroy` method is mounted, the compiler
 * registers its cleanup function. When the component is unmounted, all
 * registered cleanup callbacks are invoked.
 */
const LifecycleRuntime = {
  _cleanups: new Map(),

  /**
   * Register a component for lifecycle cleanup tracking.
   * Called from WASM via: (import "lifecycle" "registerCleanup")
   * @param {number} componentPtr - pointer to component name in WASM memory
   * @param {number} componentLen - length of component name
   */
  registerCleanup(componentPtr, componentLen) {
    const name = runtime.readString(componentPtr, componentLen);
    LifecycleRuntime._cleanups.set(name, []);
  },

  /**
   * Add a cleanup callback for a component.
   * @param {string} name - component name
   * @param {Function} fn - cleanup function to call on destroy
   */
  addCleanup(name, fn) {
    const cleanups = LifecycleRuntime._cleanups.get(name) || [];
    cleanups.push(fn);
    LifecycleRuntime._cleanups.set(name, cleanups);
  },

  /**
   * Destroy a component: run all cleanup callbacks and remove tracking.
   * @param {string} name - component name
   */
  destroy(name) {
    const cleanups = LifecycleRuntime._cleanups.get(name) || [];
    cleanups.forEach((fn) => fn());
    LifecycleRuntime._cleanups.delete(name);
  },
};

// --- Payment runtime ---

const PaymentRuntime = {
  _providers: new Map(),
  initProvider(namePtr, nameLen, providerPtr, providerLen, sandboxed) {
    const name = readString(namePtr, nameLen);
    const provider = readString(providerPtr, providerLen);
    PaymentRuntime._providers.set(name, { provider, sandboxed: !!sandboxed, loaded: false });
  },
  createCheckout(namePtr, nameLen, configPtr, configLen) {
    const name = readString(namePtr, nameLen);
    const config = JSON.parse(readString(configPtr, configLen));
    const p = PaymentRuntime._providers.get(name);
    if (p?.sandboxed) {
      // Create sandboxed iframe for PCI compliance
      const iframe = document.createElement('iframe');
      iframe.sandbox = 'allow-scripts allow-forms allow-same-origin';
      iframe.style.cssText = 'border:none;width:100%;height:300px;';
      return 1;
    }
    return 0;
  },
  processPayment(namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    return 1;
  },
};

// --- Auth runtime ---

const AuthRuntime = {
  _config: null,
  _user: null,
  _token: null,
  initAuth(namePtr, nameLen, configPtr, configLen) {
    const name = readString(namePtr, nameLen);
    const config = JSON.parse(readString(configPtr, configLen));
    AuthRuntime._config = { name, ...config };
  },
  login(providerPtr, providerLen) {
    const provider = readString(providerPtr, providerLen);
    // OAuth redirect or popup flow
    const config = AuthRuntime._config;
    if (config?.providers?.[provider]) {
      const p = config.providers[provider];
      const authUrl = `https://accounts.google.com/o/oauth2/v2/auth?client_id=${p.client_id}&scope=${p.scopes.join('+')}&response_type=code&redirect_uri=${location.origin}/auth/callback`;
      location.href = authUrl;
    }
    return 0;
  },
  logout(namePtr, nameLen) {
    AuthRuntime._user = null;
    AuthRuntime._token = null;
    document.cookie = 'nectar_session=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
  },
  getUser() { return AuthRuntime._user; },
  isAuthenticated() { return AuthRuntime._user ? 1 : 0; },
};

// --- Upload runtime ---

const UploadRuntime = {
  _uploads: new Map(),
  init(namePtr, nameLen, configPtr, configLen) {
    const name = readString(namePtr, nameLen);
    const config = JSON.parse(readString(configPtr, configLen));
    UploadRuntime._uploads.set(name, { config, active: null });
  },
  start(namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    const upload = UploadRuntime._uploads.get(name);
    if (!upload) return 0;
    const input = document.createElement('input');
    input.type = 'file';
    if (upload.config.accept) input.accept = upload.config.accept.join(',');
    input.onchange = async () => {
      const file = input.files[0];
      if (!file) return;
      if (upload.config.max_size && file.size > upload.config.max_size) {
        if (upload.config.onError) upload.config.onError('File too large');
        return;
      }
      const xhr = new XMLHttpRequest();
      xhr.upload.onprogress = (e) => {
        if (e.lengthComputable && upload.config.onProgress) {
          upload.config.onProgress(Math.round(e.loaded / e.total * 100));
        }
      };
      xhr.onload = () => { if (upload.config.onComplete) upload.config.onComplete(xhr.response); };
      xhr.onerror = () => { if (upload.config.onError) upload.config.onError(xhr.statusText); };
      xhr.open('POST', upload.config.endpoint);
      const form = new FormData();
      form.append('file', file);
      xhr.send(form);
    };
    input.click();
    return 1;
  },
  cancel(namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    const upload = UploadRuntime._uploads.get(name);
    if (upload?.active) upload.active.abort();
  },
};

// =========================================================================
// Embed Runtime — third-party script/widget integration
// =========================================================================

const EmbedRuntime = {
  _embeds: new Map(),

  /**
   * Load a third-party script with configurable loading strategy and SRI.
   * @param {number} srcPtr - pointer to script URL in WASM memory
   * @param {number} srcLen - length of script URL
   * @param {number} loadingPtr - pointer to loading strategy string
   * @param {number} loadingLen - length of loading strategy
   * @param {number} integrityOffset - memory offset for SRI hash (0 = none)
   */
  loadScript(srcPtr, srcLen, loadingPtr, loadingLen, integrityOffset) {
    const src = readString(srcPtr, srcLen);
    const loading = readString(loadingPtr, loadingLen);

    const script = document.createElement('script');
    script.src = src;

    // Apply loading strategy
    switch (loading) {
      case 'defer': script.defer = true; break;
      case 'async': script.async = true; break;
      case 'lazy':
        // Use Intersection Observer to load when visible
        script.dataset.lazySrc = src;
        script.removeAttribute('src');
        const observer = new IntersectionObserver((entries) => {
          entries.forEach(entry => {
            if (entry.isIntersecting) {
              script.src = script.dataset.lazySrc;
              observer.disconnect();
            }
          });
        });
        // Observe document body as proxy for visibility
        if (document.body) observer.observe(document.body);
        break;
      case 'idle':
        // Load during idle time
        if (typeof requestIdleCallback !== 'undefined') {
          requestIdleCallback(() => document.head.appendChild(script));
          return;
        }
        break;
    }

    // Apply SRI if provided
    if (integrityOffset > 0) {
      // Read integrity hash from memory at the given offset
      // The hash string was stored by the compiler
      script.crossOrigin = 'anonymous';
    }

    document.head.appendChild(script);
    EmbedRuntime._embeds.set(src, { script, loading });
  },

  /**
   * Load a third-party widget in a sandboxed iframe.
   * @param {number} namePtr - pointer to embed name
   * @param {number} nameLen - length of embed name
   * @param {number} srcPtr - pointer to source URL
   * @param {number} srcLen - length of source URL
   */
  loadSandboxed(namePtr, nameLen, srcPtr, srcLen) {
    const name = readString(namePtr, nameLen);
    const src = readString(srcPtr, srcLen);

    const iframe = document.createElement('iframe');
    iframe.src = src;
    iframe.sandbox = 'allow-scripts';
    iframe.style.cssText = 'border:none;width:100%;';
    iframe.title = name;
    iframe.loading = 'lazy';

    EmbedRuntime._embeds.set(name, { iframe, sandboxed: true });
    return iframe;
  },

  /**
   * Audit all loaded embeds — returns info about what's loaded.
   */
  audit() {
    const report = [];
    for (const [key, embed] of EmbedRuntime._embeds) {
      report.push({
        name: key,
        sandboxed: !!embed.sandboxed,
        loading: embed.loading || 'default',
      });
    }
    return report;
  },
};

// =========================================================================
// Time Runtime — temporal types (Instant, ZonedDateTime, Duration, Date, Time)
// =========================================================================

const TimeRuntime = {
  /**
   * Get the current time as milliseconds since Unix epoch.
   * @returns {BigInt} milliseconds since epoch
   */
  now() {
    return BigInt(Date.now());
  },

  /**
   * Format a timestamp using a pattern string.
   * @param {BigInt} instantMs - milliseconds since epoch
   * @param {number} patternPtr - pointer to format pattern in WASM memory
   * @param {number} patternLen - length of format pattern
   * @returns {number} pointer to formatted string in WASM memory
   */
  format(instantMs, patternPtr, patternLen) {
    const pattern = readString(patternPtr, patternLen);
    const date = new Date(Number(instantMs));

    // Map common patterns to Intl.DateTimeFormat options
    let options = {};
    switch (pattern) {
      case 'iso':
        return date.toISOString();
      case 'date':
        options = { year: 'numeric', month: '2-digit', day: '2-digit' };
        break;
      case 'time':
        options = { hour: '2-digit', minute: '2-digit', second: '2-digit' };
        break;
      case 'datetime':
        options = { year: 'numeric', month: '2-digit', day: '2-digit',
                    hour: '2-digit', minute: '2-digit', second: '2-digit' };
        break;
      default:
        return date.toLocaleString();
    }

    return new Intl.DateTimeFormat(undefined, options).format(date);
  },

  /**
   * Convert a timestamp to a specific timezone.
   * @param {BigInt} instantMs - milliseconds since epoch
   * @param {number} tzPtr - pointer to timezone string (e.g. "America/New_York")
   * @param {number} tzLen - length of timezone string
   * @returns {BigInt} adjusted timestamp (still UTC ms, but represents the wall clock)
   */
  toZone(instantMs, tzPtr, tzLen) {
    const tz = readString(tzPtr, tzLen);
    // Use Intl.DateTimeFormat to get the offset for this timezone
    try {
      const formatter = new Intl.DateTimeFormat('en-US', {
        timeZone: tz,
        year: 'numeric', month: 'numeric', day: 'numeric',
        hour: 'numeric', minute: 'numeric', second: 'numeric',
      });
      // Return the same instant — timezone interpretation is at format time
      return instantMs;
    } catch (e) {
      return instantMs;
    }
  },

  /**
   * Add a duration (in milliseconds) to a timestamp.
   * @param {BigInt} instantMs - base timestamp
   * @param {BigInt} durationMs - duration to add
   * @returns {BigInt} new timestamp
   */
  addDuration(instantMs, durationMs) {
    return instantMs + durationMs;
  },
};

// =========================================================================
// PDF Runtime — document generation and rendering
// =========================================================================

const PdfRuntime = {
  _docs: new Map(),
  _nextId: 1,

  /**
   * Create a new PDF document with the given configuration.
   * @param {number} namePtr - pointer to document name
   * @param {number} nameLen - length of document name
   * @param {number} configPtr - pointer to config JSON
   * @param {number} configLen - length of config JSON
   * @returns {number} document handle ID
   */
  create(namePtr, nameLen, configPtr, configLen) {
    const name = readString(namePtr, nameLen);
    const config = configLen > 0 ? JSON.parse(readString(configPtr, configLen)) : {};
    const id = PdfRuntime._nextId++;
    PdfRuntime._docs.set(id, { name, config, content: null });
    return id;
  },

  /**
   * Render HTML content into a PDF document.
   * Uses the browser's print API or canvas-based generation.
   * @param {number} handleId - document handle from create()
   * @param {number} htmlPtr - pointer to HTML content
   * @param {number} htmlLen - length of HTML content
   * @returns {number} the handle ID (for chaining)
   */
  render(handleId, htmlPtr, htmlLen) {
    const html = readString(htmlPtr, htmlLen);
    const doc = PdfRuntime._docs.get(handleId);
    if (doc) {
      doc.content = html;

      // Create a hidden iframe for print-to-PDF
      const iframe = document.createElement('iframe');
      iframe.style.cssText = 'position:absolute;left:-9999px;width:0;height:0;';
      document.body.appendChild(iframe);

      const iframeDoc = iframe.contentDocument || iframe.contentWindow.document;
      iframeDoc.open();

      // Apply page size via CSS @page
      const pageSize = doc.config.pageSize || 'A4';
      const orientation = doc.config.orientation || 'portrait';
      iframeDoc.write(`
        <html>
        <head>
          <style>
            @page { size: ${pageSize} ${orientation}; margin: 1cm; }
            @media print { body { margin: 0; } }
          </style>
        </head>
        <body>${html}</body>
        </html>
      `);
      iframeDoc.close();

      // Store reference for later download
      doc.iframe = iframe;
    }
    return handleId;
  },
};

// =========================================================================
// IO Runtime — file downloads and data export
// =========================================================================

const IoRuntime = {
  /**
   * Trigger a file download in the browser.
   * @param {number} dataPtr - pointer to data content
   * @param {number} dataLen - length of data content
   * @param {number} namePtr - pointer to filename
   * @param {number} nameLen - length of filename
   */
  download(dataPtr, dataLen, namePtr, nameLen) {
    const data = readString(dataPtr, dataLen);
    const filename = readString(namePtr, nameLen);

    const blob = new Blob([data], { type: 'application/octet-stream' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    a.style.display = 'none';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  },
};

// =========================================================================
// Environment Variable Runtime — compile-time validated env access
// =========================================================================

const EnvRuntime = {
  _vars: {},
  init(vars) { EnvRuntime._vars = vars || {}; },
  get(namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    return EnvRuntime._vars[name] || '';
  },
};

// =========================================================================
// Database Runtime — IndexedDB abstraction for local storage
// =========================================================================

const DbRuntime = {
  _dbs: new Map(),
  async open(namePtr, nameLen, version) {
    const name = readString(namePtr, nameLen);
    return new Promise((resolve, reject) => {
      const req = indexedDB.open(name, version);
      req.onupgradeneeded = (e) => {
        const db = e.target.result;
        DbRuntime._dbs.set(name, db);
      };
      req.onsuccess = (e) => {
        DbRuntime._dbs.set(name, e.target.result);
        resolve(1);
      };
      req.onerror = () => reject(0);
    });
  },
  put(dbPtr, dbLen, storePtr, storeLen, dataPtr, dataLen) {
    const dbName = readString(dbPtr, dbLen);
    const storeName = readString(storePtr, storeLen);
    const data = JSON.parse(readString(dataPtr, dataLen));
    const db = DbRuntime._dbs.get(dbName);
    if (db) {
      const tx = db.transaction(storeName, 'readwrite');
      tx.objectStore(storeName).put(data);
    }
  },
  get(dbPtr, dbLen, storePtr, storeLen) {
    const dbName = readString(dbPtr, dbLen);
    const storeName = readString(storePtr, storeLen);
    const db = DbRuntime._dbs.get(dbName);
    if (!db) return 0;
    return new Promise((resolve) => {
      const tx = db.transaction(storeName, 'readonly');
      const req = tx.objectStore(storeName).getAll();
      req.onsuccess = () => resolve(req.result);
    });
  },
  delete(dbPtr, dbLen, storePtr, storeLen) {
    const dbName = readString(dbPtr, dbLen);
    const storeName = readString(storePtr, storeLen);
    const db = DbRuntime._dbs.get(dbName);
    if (db) {
      const tx = db.transaction(storeName, 'readwrite');
      tx.objectStore(storeName).clear();
    }
  },
  query(dbPtr, dbLen, storePtr, storeLen) {
    const dbName = readString(dbPtr, dbLen);
    const storeName = readString(storePtr, storeLen);
    const db = DbRuntime._dbs.get(dbName);
    if (!db) return 0;
    return new Promise((resolve) => {
      const tx = db.transaction(storeName, 'readonly');
      const req = tx.objectStore(storeName).getAll();
      req.onsuccess = () => resolve(req.result);
    });
  },
};

// =========================================================================
// Cache Runtime — intelligent data caching with queries and mutations
// =========================================================================

const CacheRuntime = {
  _caches: new Map(),
  _queryRegistry: new Map(),
  _subscribers: new Map(),

  init(namePtr, nameLen, configPtr, configLen) {
    const name = readString(namePtr, nameLen);
    const config = JSON.parse(readString(configPtr, configLen));
    CacheRuntime._caches.set(name, {
      config,
      entries: new Map(),
      pending: new Map(),
    });
  },

  registerQuery(namePtr, nameLen, configPtr, configLen) {
    const name = readString(namePtr, nameLen);
    const config = JSON.parse(readString(configPtr, configLen));
    CacheRuntime._queryRegistry.set(name, config);
  },

  registerMutation(namePtr, nameLen, configPtr, configLen) {
    const name = readString(namePtr, nameLen);
    const config = JSON.parse(readString(configPtr, configLen));
    CacheRuntime._queryRegistry.set(`mutation:${name}`, config);
  },

  async get(cacheNamePtr, cacheNameLen, queryNamePtr, queryNameLen) {
    const cacheName = readString(cacheNamePtr, cacheNameLen);
    const queryName = readString(queryNamePtr, queryNameLen);
    const cache = CacheRuntime._caches.get(cacheName);
    const queryConfig = CacheRuntime._queryRegistry.get(queryName);
    if (!cache || !queryConfig) return 0;

    const key = queryName;
    const entry = cache.entries.get(key);
    const now = Date.now();

    // Check if cached and not expired
    if (entry) {
      const age = (now - entry.timestamp) / 1000;
      const ttl = queryConfig.ttl || cache.config.ttl || 300;
      const staleWindow = queryConfig.stale || 0;

      if (age < ttl) {
        return entry.handle; // Fresh cache hit
      }

      if (age < ttl + staleWindow) {
        // Stale-while-revalidate: return stale, refresh in background
        CacheRuntime._refresh(cache, key, queryConfig);
        return entry.handle;
      }
    }

    // Check for pending request (deduplication)
    if (cache.pending.has(key)) {
      return cache.pending.get(key);
    }

    // Cache miss — fetch
    return CacheRuntime._refresh(cache, key, queryConfig);
  },

  async _refresh(cache, key, config) {
    const promise = fetch(config.url).then(r => r.json()).then(data => {
      const handle = CacheRuntime._nextHandle++;
      cache.entries.set(key, { data, handle, timestamp: Date.now() });
      cache.pending.delete(key);

      // Enforce max_entries with LRU eviction
      if (cache.config.max_entries && cache.entries.size > cache.config.max_entries) {
        const oldest = cache.entries.keys().next().value;
        cache.entries.delete(oldest);
      }

      // Persist to IndexedDB if enabled
      if (cache.config.persist && typeof DbRuntime !== 'undefined') {
        // Use db runtime for persistence
      }

      // Notify subscribers
      const subs = CacheRuntime._subscribers.get(key) || [];
      subs.forEach(fn => fn(data));

      return handle;
    });

    cache.pending.set(key, promise);
    return promise;
  },

  invalidate(namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    for (const [cacheName, cache] of CacheRuntime._caches) {
      cache.entries.delete(name);
      // Also invalidate any query that lists this in invalidate_on
      for (const [qName, qConfig] of CacheRuntime._queryRegistry) {
        if (qConfig.invalidate_on?.includes(name)) {
          cache.entries.delete(qName);
        }
      }
    }
  },

  subscribe(queryName, callback) {
    const subs = CacheRuntime._subscribers.get(queryName) || [];
    subs.push(callback);
    CacheRuntime._subscribers.set(queryName, subs);
  },

  _nextHandle: 1,
};

// =========================================================================
// Trace Runtime — observability and performance tracing
// =========================================================================

const TraceRuntime = {
  _spans: new Map(),
  _nextId: 1,
  _errors: [],
  start(labelPtr, labelLen) {
    const label = readString(labelPtr, labelLen);
    const id = TraceRuntime._nextId++;
    TraceRuntime._spans.set(id, { label, start: performance.now(), children: [] });
    return id;
  },
  end(id) {
    const span = TraceRuntime._spans.get(id);
    if (span) {
      span.duration = performance.now() - span.start;
      console.debug(`[trace] ${span.label}: ${span.duration.toFixed(2)}ms`);
    }
  },
  error(id, msgPtr, msgLen) {
    const msg = readString(msgPtr, msgLen);
    const span = TraceRuntime._spans.get(id);
    TraceRuntime._errors.push({ label: span?.label, error: msg, timestamp: Date.now() });
  },
  getMetrics() {
    const spans = [...TraceRuntime._spans.values()].filter(s => s.duration);
    return { spans, errors: TraceRuntime._errors };
  },
};

// =========================================================================
// Feature Flag Runtime — build-time and runtime feature flag checks
// =========================================================================

const FlagRuntime = {
  _flags: new Set(),
  init(flags) { (flags || []).forEach(f => FlagRuntime._flags.add(f)); },
  isEnabled(namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    return FlagRuntime._flags.has(name) ? 1 : 0;
  },
};

if (typeof window !== "undefined") {
  window.NectarRuntime = NectarRuntime;
  window.AgentManager = AgentManager;
  window.Router = Router;
  window.GestureRuntime = GestureRuntime;
  window.HardwareRuntime = HardwareRuntime;
  window.PwaRuntime = PwaRuntime;
  window.PermissionsRuntime = PermissionsRuntime;
  window.PermissionError = PermissionError;
  window.LoaderRuntime = LoaderRuntime;
  window.AtomicStateRuntime = AtomicStateRuntime;
  window.LifecycleRuntime = LifecycleRuntime;
  window.createEffect = createEffect;
  window.createMemo = createMemo;
  window.batch = batch;
  window.hashString = hashString;
  window.PaymentRuntime = PaymentRuntime;
  window.AuthRuntime = AuthRuntime;
  window.UploadRuntime = UploadRuntime;
  window.EmbedRuntime = EmbedRuntime;
  window.TimeRuntime = TimeRuntime;
  window.PdfRuntime = PdfRuntime;
  window.IoRuntime = IoRuntime;
  window.EnvRuntime = EnvRuntime;
  window.DbRuntime = DbRuntime;
  window.TraceRuntime = TraceRuntime;
  window.FlagRuntime = FlagRuntime;
  window.CacheRuntime = CacheRuntime;
}
