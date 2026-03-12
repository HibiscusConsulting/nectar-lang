/**
 * Nectar Hot Reload Client — development-mode module hot replacement.
 *
 * Connects to the Nectar dev server via WebSocket, listens for file-change
 * notifications, and swaps WASM modules or CSS without a full page reload.
 * Signal state is serialized before the swap and restored afterward so that
 * application state survives a hot reload.
 */

export class HotReloadClient {
  /**
   * @param {object} options
   * @param {string}   options.url       - WebSocket URL (default: ws://localhost:3000)
   * @param {object}   options.runtime   - Reference to the NectarRuntime instance
   * @param {function} [options.onReload] - Optional callback after a successful reload
   * @param {function} [options.onError]  - Optional callback on reload failure
   */
  constructor(options = {}) {
    this.url = options.url || 'ws://localhost:3000';
    this.runtime = options.runtime || null;
    this.onReload = options.onReload || (() => {});
    this.onError = options.onError || ((err) => console.error('[nectar-hot-reload]', err));

    /** @type {WebSocket|null} */
    this._ws = null;

    /** @type {Map<string, WebAssembly.Module>} loaded modules by path */
    this._modules = new Map();

    /** Reconnection state */
    this._reconnectTimer = null;
    this._reconnectDelay = 1000;
    this._maxReconnectDelay = 30000;
  }

  // -----------------------------------------------------------------------
  // Public API
  // -----------------------------------------------------------------------

  /** Open the WebSocket connection to the dev server. */
  connect() {
    if (this._ws) {
      this._ws.close();
    }

    try {
      this._ws = new WebSocket(this.url);
    } catch (e) {
      this.onError(e);
      this._scheduleReconnect();
      return;
    }

    this._ws.onopen = () => {
      console.log('[nectar-hot-reload] connected to', this.url);
      this._reconnectDelay = 1000; // reset backoff
    };

    this._ws.onmessage = (event) => {
      this._handleMessage(event.data);
    };

    this._ws.onerror = (event) => {
      this.onError(event);
    };

    this._ws.onclose = () => {
      console.log('[nectar-hot-reload] disconnected, will reconnect...');
      this._scheduleReconnect();
    };
  }

  /** Disconnect and stop reconnecting. */
  disconnect() {
    if (this._reconnectTimer) {
      clearTimeout(this._reconnectTimer);
      this._reconnectTimer = null;
    }
    if (this._ws) {
      this._ws.close();
      this._ws = null;
    }
  }

  // -----------------------------------------------------------------------
  // Message handling
  // -----------------------------------------------------------------------

  /**
   * @param {string} data - Raw WebSocket message (JSON).
   */
  async _handleMessage(data) {
    let message;
    try {
      message = JSON.parse(data);
    } catch {
      console.warn('[nectar-hot-reload] received non-JSON message:', data);
      return;
    }

    switch (message.type) {
      case 'reload':
        await this._handleReload(message);
        break;
      case 'css':
        this._handleCssReload(message);
        break;
      case 'error':
        this.onError(new Error(message.message || 'Dev server error'));
        break;
      default:
        console.log('[nectar-hot-reload] unknown message type:', message.type);
    }
  }

  /**
   * Handle a WASM module reload.
   *
   * 1. Serialize current signal state.
   * 2. Fetch the new .wasm module.
   * 3. Swap the module in the running runtime.
   * 4. Restore signal state.
   *
   * @param {{ files: string[] }} message
   */
  async _handleReload(message) {
    const files = message.files || [];
    console.log('[nectar-hot-reload] reloading:', files);

    for (const filePath of files) {
      try {
        if (filePath.endsWith('.css')) {
          this._hotReloadCss(filePath);
          continue;
        }

        // Determine the .wasm URL from the source path.
        const wasmUrl = this._sourceToWasmUrl(filePath);

        // Step 1: Serialize signal state.
        const savedState = this._serializeSignals();

        // Step 2: Fetch the new module.
        const response = await fetch(wasmUrl, { cache: 'no-store' });
        if (!response.ok) {
          throw new Error(`Failed to fetch ${wasmUrl}: ${response.status}`);
        }
        const bytes = await response.arrayBuffer();

        // Step 3: Compile and swap.
        const newModule = await WebAssembly.compile(bytes);
        this._modules.set(filePath, newModule);

        if (this.runtime) {
          await this._swapModule(newModule);
        }

        // Step 4: Restore signal state.
        this._restoreSignals(savedState);

        console.log('[nectar-hot-reload] reloaded:', filePath);
        this.onReload({ file: filePath });
      } catch (err) {
        this.onError(err);
      }
    }
  }

  // -----------------------------------------------------------------------
  // WASM module swap
  // -----------------------------------------------------------------------

  /**
   * Swap the WASM module inside the running NectarRuntime without a full
   * page reload. The new module is instantiated with the same import
   * object, and its exported `$init` (or similar entry point) is called.
   *
   * @param {WebAssembly.Module} newModule
   */
  async _swapModule(newModule) {
    const runtime = this.runtime;
    if (!runtime) return;

    // Build the import object from the runtime (same as initial load).
    const importObject = runtime.importObject || runtime._importObject || {};

    const instance = await WebAssembly.instantiate(newModule, importObject);

    // Replace the instance reference in the runtime.
    if (runtime._instance !== undefined) {
      runtime._instance = instance;
    }
    if (runtime.instance !== undefined) {
      runtime.instance = instance;
    }
    if (runtime._wasm !== undefined) {
      runtime._wasm = instance;
    }

    // Call the init/start export if present.
    const exports = instance.exports;
    if (typeof exports._start === 'function') {
      exports._start();
    } else if (typeof exports.$init === 'function') {
      exports.$init();
    } else if (typeof exports.main === 'function') {
      exports.main();
    }
  }

  // -----------------------------------------------------------------------
  // Signal state preservation
  // -----------------------------------------------------------------------

  /**
   * Serialize all signal values from the runtime so they can survive a
   * module swap.
   *
   * @returns {Map<number, any>} Map from signal ID to current value.
   */
  _serializeSignals() {
    const state = new Map();

    if (!this.runtime) return state;

    // If the runtime exposes a signals registry, iterate it.
    const signals =
      this.runtime._signals ||
      this.runtime.signals ||
      (this.runtime._signalRegistry && this.runtime._signalRegistry._signals);

    if (signals instanceof Map) {
      for (const [id, signal] of signals) {
        try {
          const value = typeof signal.peek === 'function'
            ? signal.peek()
            : signal._value !== undefined
              ? signal._value
              : signal.value;
          state.set(id, value);
        } catch {
          // Skip signals that can't be read.
        }
      }
    } else if (Array.isArray(signals)) {
      for (let i = 0; i < signals.length; i++) {
        const signal = signals[i];
        if (signal != null) {
          try {
            state.set(i, signal._value !== undefined ? signal._value : signal.value);
          } catch {
            // skip
          }
        }
      }
    }

    return state;
  }

  /**
   * Restore previously serialized signal values after a module swap.
   *
   * @param {Map<number, any>} savedState
   */
  _restoreSignals(savedState) {
    if (!this.runtime || savedState.size === 0) return;

    const signals =
      this.runtime._signals ||
      this.runtime.signals ||
      (this.runtime._signalRegistry && this.runtime._signalRegistry._signals);

    if (signals instanceof Map) {
      for (const [id, value] of savedState) {
        const signal = signals.get(id);
        if (signal) {
          if (typeof signal.set === 'function') {
            signal.set(value);
          } else if (signal._value !== undefined) {
            signal._value = value;
          }
        }
      }
    } else if (Array.isArray(signals)) {
      for (const [id, value] of savedState) {
        const signal = signals[id];
        if (signal) {
          if (typeof signal.set === 'function') {
            signal.set(value);
          } else if (signal._value !== undefined) {
            signal._value = value;
          }
        }
      }
    }
  }

  // -----------------------------------------------------------------------
  // CSS hot reload
  // -----------------------------------------------------------------------

  /**
   * Handle a CSS-only update: re-inject scoped styles without touching WASM.
   *
   * @param {{ files: string[], css?: Object<string, string> }} message
   */
  _handleCssReload(message) {
    const cssMap = message.css || {};

    for (const [selector, styles] of Object.entries(cssMap)) {
      this._injectScopedStyle(selector, styles);
    }

    // Also handle file-based CSS reload.
    const files = message.files || [];
    for (const filePath of files) {
      if (filePath.endsWith('.css')) {
        this._hotReloadCss(filePath);
      }
    }
  }

  /**
   * Replace a scoped style tag in the document. Nectar components have
   * `<style data-nectar-scope="ComponentName">` tags; we find and replace them.
   *
   * @param {string} scope - Component scope identifier.
   * @param {string} cssText - New CSS text.
   */
  _injectScopedStyle(scope, cssText) {
    // Find existing style tag for this scope.
    let styleEl = document.querySelector(`style[data-nectar-scope="${scope}"]`);

    if (!styleEl) {
      styleEl = document.createElement('style');
      styleEl.setAttribute('data-nectar-scope', scope);
      document.head.appendChild(styleEl);
    }

    styleEl.textContent = cssText;
  }

  /**
   * Hot-reload a CSS file by cache-busting its link tag.
   *
   * @param {string} filePath
   */
  _hotReloadCss(filePath) {
    const links = document.querySelectorAll('link[rel="stylesheet"]');
    const fileName = filePath.split('/').pop();

    for (const link of links) {
      if (link.href && link.href.includes(fileName)) {
        const url = new URL(link.href);
        url.searchParams.set('_nectar_reload', Date.now().toString());
        link.href = url.toString();
      }
    }
  }

  // -----------------------------------------------------------------------
  // Helpers
  // -----------------------------------------------------------------------

  /**
   * Convert a source file path (e.g., "src/app.nectar") to a .wasm URL
   * served by the dev server (e.g., "/app.wasm").
   */
  _sourceToWasmUrl(filePath) {
    const stem = filePath.split('/').pop().replace(/\.[^.]+$/, '');
    return `/${stem}.wasm`;
  }

  _scheduleReconnect() {
    if (this._reconnectTimer) return;

    this._reconnectTimer = setTimeout(() => {
      this._reconnectTimer = null;
      this._reconnectDelay = Math.min(this._reconnectDelay * 2, this._maxReconnectDelay);
      this.connect();
    }, this._reconnectDelay);
  }
}
