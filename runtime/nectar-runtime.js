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
    };

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

    // Initialize stores — call any *_init exports
    const allExports = Object.keys(instance.exports);
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

// Export for use
if (typeof module !== "undefined") {
  module.exports = {
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
  };
}
if (typeof window !== "undefined") {
  window.NectarRuntime = NectarRuntime;
  window.AgentManager = AgentManager;
  window.Router = Router;
  window.createEffect = createEffect;
  window.createMemo = createMemo;
  window.batch = batch;
  window.hashString = hashString;
}
