/**
 * Nectar Hydration Runtime — attaches interactivity to server-rendered HTML.
 *
 * Instead of creating DOM from scratch, the hydration runtime walks the
 * existing server-rendered DOM, matches elements bearing `data-nectar-hydrate`
 * and `data-nectar-key` markers to their component definitions, and attaches
 * signal bindings and event handlers to the pre-existing nodes.
 *
 * After hydration the app becomes fully interactive with the same
 * fine-grained reactivity as a client-rendered Nectar app.
 */

// --- Event delegation ---

/** Map of data-nectar-key -> { event -> handler } for delegated events */
const __delegatedHandlers = new Map();

/**
 * Single root-level event listener that dispatches to the correct handler
 * based on the nearest `data-nectar-key` attribute on the event target.
 */
function __initEventDelegation(root) {
  const DELEGATED_EVENTS = [
    'click', 'input', 'change', 'submit', 'keydown', 'keyup',
    'keypress', 'focus', 'blur', 'mousedown', 'mouseup', 'mouseover',
    'mouseout', 'touchstart', 'touchend', 'touchmove',
  ];

  for (const eventName of DELEGATED_EVENTS) {
    root.addEventListener(eventName, (event) => {
      let target = event.target;
      while (target && target !== root) {
        const key = target.getAttribute('data-nectar-key');
        if (key !== null) {
          const handlers = __delegatedHandlers.get(key);
          if (handlers && handlers[eventName]) {
            handlers[eventName](event);
            return;
          }
        }
        target = target.parentElement;
      }
    });
  }
}

// --- Signal / reactivity integration (from nectar-runtime.js) ---

/** Reference to the runtime's Signal class, injected during hydrate() */
let Signal = null;
let Effect = null;

function __createSignal(initialValue) {
  return new Signal(initialValue);
}

function __createEffect(fn) {
  return new Effect(fn);
}

// --- Hydration core ---

/**
 * Hydrate a server-rendered DOM tree, attaching component interactivity.
 *
 * @param {object} wasmModule — instantiated Nectar WASM module (or JS runtime bridge)
 * @param {HTMLElement} rootElement — the DOM root containing server-rendered HTML
 */
function hydrate(wasmModule, rootElement) {
  // Restore store state from the server-serialized blob
  if (window.__NECTAR_STATE__) {
    _restoreStoreState(wasmModule, window.__NECTAR_STATE__);
  }

  // Grab runtime primitives from the module if available
  if (wasmModule.__Signal) Signal = wasmModule.__Signal;
  if (wasmModule.__Effect) Effect = wasmModule.__Effect;

  // Set up event delegation on the root
  __initEventDelegation(rootElement);

  // Walk the DOM and find all hydration roots
  const hydrateRoots = rootElement.querySelectorAll('[data-nectar-hydrate]');
  for (const el of hydrateRoots) {
    const componentName = el.getAttribute('data-nectar-hydrate');
    _hydrateComponent(wasmModule, el, componentName);
  }
}

/**
 * Hydrate a single component — attach signals, effects, and event handlers
 * to the existing DOM without recreating it.
 */
function _hydrateComponent(wasmModule, element, componentName) {
  // Look up the component's hydration descriptor from the WASM module
  const descriptor = wasmModule[componentName + '_hydrate']
    || wasmModule[componentName + '_mount'];

  if (!descriptor) {
    console.warn(`[nectar-hydrate] No hydration descriptor for component: ${componentName}`);
    return;
  }

  // If the module exposes a dedicated hydrate function, call it with the element
  if (typeof wasmModule[componentName + '_hydrate'] === 'function') {
    wasmModule[componentName + '_hydrate'](element);
    return;
  }

  // Otherwise, walk the keyed elements and attach bindings
  const keyedElements = element.querySelectorAll('[data-nectar-key]');
  for (const keyedEl of keyedElements) {
    hydrateElement(wasmModule, keyedEl, componentName);
  }
}

/**
 * Recursively attach bindings to a single DOM element identified by its
 * hydration key.
 *
 * @param {object} wasmModule
 * @param {HTMLElement} element — the DOM node with a `data-nectar-key`
 * @param {string} componentName
 */
function hydrateElement(wasmModule, element, componentName) {
  const key = element.getAttribute('data-nectar-key');
  if (key === null) return;

  // Look up bindings for this key from the module
  const bindings = wasmModule[componentName + '_bindings'];
  if (!bindings || !bindings[key]) return;

  const binding = bindings[key];

  // Attach reactive text content
  if (binding.text) {
    const signal = binding.text;
    __createEffect(() => {
      element.textContent = signal.get();
    });
  }

  // Attach reactive attributes
  if (binding.attrs) {
    for (const [attrName, signal] of Object.entries(binding.attrs)) {
      __createEffect(() => {
        element.setAttribute(attrName, signal.get());
      });
    }
  }

  // Register event handlers via delegation
  if (binding.events) {
    __delegatedHandlers.set(key, binding.events);
  }

  // Attach reactive class bindings
  if (binding.classes) {
    for (const [className, signal] of Object.entries(binding.classes)) {
      __createEffect(() => {
        element.classList.toggle(className, !!signal.get());
      });
    }
  }

  // Attach reactive style bindings
  if (binding.style) {
    for (const [prop, signal] of Object.entries(binding.style)) {
      __createEffect(() => {
        element.style[prop] = signal.get();
      });
    }
  }
}

/**
 * Restore store state from the server-serialized JSON blob.
 */
function _restoreStoreState(wasmModule, state) {
  for (const [storeName, storeData] of Object.entries(state)) {
    const initFn = wasmModule[storeName + '_init'];
    if (typeof initFn === 'function') {
      initFn(storeData);
    } else {
      // Fall back to setting individual signals
      for (const [key, value] of Object.entries(storeData)) {
        const setter = wasmModule[storeName + '_set_' + key];
        if (typeof setter === 'function') {
          setter(value);
        }
      }
    }
  }
}

// --- Exports ---

if (typeof module !== 'undefined' && module.exports) {
  module.exports = { hydrate, hydrateElement };
}

if (typeof window !== 'undefined') {
  window.__nectarHydrate = { hydrate, hydrateElement };
}
