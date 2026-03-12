/**
 * Nectar Component Test Renderer — a virtual DOM test renderer for
 * component testing in Node.js.
 *
 * Usage:
 *   import { render } from './nectar-test-renderer.js';
 *   const el = await render('./component.wasm', { title: 'Hello' });
 *   const heading = el.findByText('Hello');
 *   heading.click();
 *   expect(el.findByRole('counter').getText()).toBe('1');
 */

import { readFile } from 'node:fs/promises';

// ---------------------------------------------------------------------------
// Minimal Virtual DOM — used instead of jsdom for lightweight testing
// ---------------------------------------------------------------------------

let nextNodeId = 1;

class VNode {
  constructor(tag, attributes = {}) {
    this.id = nextNodeId++;
    this.tag = tag;
    this.attributes = { ...attributes };
    this.children = [];
    this.text = '';
    this.eventHandlers = {};
    this.parent = null;
  }

  setAttribute(name, value) {
    this.attributes[name] = value;
  }

  getAttribute(name) {
    return this.attributes[name] ?? null;
  }

  appendChild(child) {
    child.parent = this;
    this.children.push(child);
  }

  setText(text) {
    this.text = text;
  }

  addEventListener(event, handler) {
    if (!this.eventHandlers[event]) {
      this.eventHandlers[event] = [];
    }
    this.eventHandlers[event].push(handler);
  }

  dispatchEvent(event) {
    const handlers = this.eventHandlers[event] || [];
    for (const handler of handlers) {
      handler();
    }
  }
}

// ---------------------------------------------------------------------------
// TestElement — query and interaction wrapper
// ---------------------------------------------------------------------------

export class TestElement {
  /**
   * @param {VNode} vnode - The underlying virtual DOM node
   * @param {object} wasmInstance - The WASM instance for calling handlers
   * @param {WebAssembly.Memory} memory - WASM linear memory
   */
  constructor(vnode, wasmInstance, memory) {
    this._node = vnode;
    this._instance = wasmInstance;
    this._memory = memory;
  }

  /**
   * Find a descendant element containing the given text.
   * @param {string} text
   * @returns {TestElement}
   */
  findByText(text) {
    const found = this._findBy((node) => {
      if (node.text && node.text.includes(text)) return true;
      // Also check direct text children
      return node.children.some(
        (c) => c.tag === '#text' && c.text.includes(text)
      );
    });
    if (!found) {
      throw new Error(`Could not find element with text "${text}"`);
    }
    return new TestElement(found, this._instance, this._memory);
  }

  /**
   * Find a descendant element with the given ARIA role.
   * @param {string} role
   * @returns {TestElement}
   */
  findByRole(role) {
    const found = this._findBy(
      (node) => node.attributes.role === role
    );
    if (!found) {
      throw new Error(`Could not find element with role "${role}"`);
    }
    return new TestElement(found, this._instance, this._memory);
  }

  /**
   * Find a descendant element with a specific attribute value.
   * @param {string} name
   * @param {string} value
   * @returns {TestElement}
   */
  findByAttribute(name, value) {
    const found = this._findBy(
      (node) => node.attributes[name] === value
    );
    if (!found) {
      throw new Error(
        `Could not find element with attribute ${name}="${value}"`
      );
    }
    return new TestElement(found, this._instance, this._memory);
  }

  /**
   * Simulate a click event on the element.
   */
  click() {
    this._node.dispatchEvent('click');
  }

  /**
   * Simulate text input on the element.
   * @param {string} text
   */
  type(text) {
    // Set the value attribute and fire input/change events
    this._node.attributes.value = text;
    this._node.dispatchEvent('input');
    this._node.dispatchEvent('change');
  }

  /**
   * Get the text content of this element and its descendants.
   * @returns {string}
   */
  getText() {
    return this._collectText(this._node);
  }

  /**
   * Get the value of an attribute on this element.
   * @param {string} name
   * @returns {string|null}
   */
  getAttribute(name) {
    return this._node.getAttribute(name);
  }

  /**
   * Check if this element exists (always true for found elements).
   * @returns {boolean}
   */
  exists() {
    return this._node !== null;
  }

  /**
   * Get all child TestElements.
   * @returns {TestElement[]}
   */
  children() {
    return this._node.children.map(
      (c) => new TestElement(c, this._instance, this._memory)
    );
  }

  /**
   * Get the tag name.
   * @returns {string}
   */
  get tagName() {
    return this._node.tag;
  }

  // -- internal helpers --

  _findBy(predicate) {
    return this._dfs(this._node, predicate);
  }

  _dfs(node, predicate) {
    if (predicate(node)) return node;
    for (const child of node.children) {
      const found = this._dfs(child, predicate);
      if (found) return found;
    }
    return null;
  }

  _collectText(node) {
    let text = node.text || '';
    for (const child of node.children) {
      text += this._collectText(child);
    }
    return text;
  }
}

// ---------------------------------------------------------------------------
// render() — mount a component WASM into a virtual DOM and return it
// ---------------------------------------------------------------------------

/**
 * Render an Nectar component into a virtual DOM for testing.
 * @param {string|Uint8Array} componentWasm - Path to .wasm file or raw bytes
 * @param {object} props - Props to pass to the component
 * @returns {Promise<TestElement>}
 */
export async function render(componentWasm, props = {}) {
  const wasmBytes =
    typeof componentWasm === 'string'
      ? await readFile(componentWasm)
      : componentWasm;

  // Reset node ID counter
  nextNodeId = 1;

  // Create the virtual DOM root
  const root = new VNode('div', { id: 'test-root' });

  // Node registry: id -> VNode
  const nodes = new Map();
  nodes.set(root.id, root);

  // Memory for the WASM module
  const memory = new WebAssembly.Memory({ initial: 1 });

  // Encode props into memory if needed
  // (simplified: store as key=value pairs at known offset)
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  function readString(ptr, len) {
    if (len === 0) return '';
    return decoder.decode(new Uint8Array(memory.buffer, ptr, len));
  }

  function writeString(str, offset) {
    const bytes = encoder.encode(str);
    new Uint8Array(memory.buffer, offset, bytes.length).set(bytes);
    return bytes.length;
  }

  // Registered event handler function refs
  const handlerFunctions = [];

  const importObject = {
    env: { memory },
    dom: {
      createElement: (tagPtr, tagLen) => {
        const tag = readString(tagPtr, tagLen);
        const node = new VNode(tag);
        nodes.set(node.id, node);
        return node.id;
      },
      setText: (nodeId, textPtr, textLen) => {
        const node = nodes.get(nodeId);
        if (node) {
          node.setText(readString(textPtr, textLen));
        }
      },
      appendChild: (parentId, childId) => {
        const parent = nodes.get(parentId);
        const child = nodes.get(childId);
        if (parent && child) {
          parent.appendChild(child);
        }
      },
      addEventListener: (nodeId, eventPtr, eventLen, handlerIdx) => {
        const node = nodes.get(nodeId);
        if (node) {
          const event = readString(eventPtr, eventLen);
          node.addEventListener(event, () => {
            // Call the WASM handler function
            const handlerName = `__handler_${handlerIdx}`;
            if (instance.exports[handlerName]) {
              instance.exports[handlerName]();
            }
          });
        }
      },
      setAttribute: (nodeId, namePtr, nameLen, valuePtr) => {
        const node = nodes.get(nodeId);
        if (node) {
          const name = readString(namePtr, nameLen);
          // valuePtr is a string (ptr,len) pair — simplified
          node.setAttribute(name, String(valuePtr));
        }
      },
      lazyMount: () => {},
    },
    signal: {
      create: (initialValue) => {
        // Create a simple signal store
        const signalId = nextNodeId++;
        nodes.set(signalId, { type: 'signal', value: initialValue });
        return signalId;
      },
      get: (signalId) => {
        const sig = nodes.get(signalId);
        return sig?.value ?? 0;
      },
      set: (signalId, value) => {
        const sig = nodes.get(signalId);
        if (sig) sig.value = value;
      },
      subscribe: () => {},
      createEffect: () => {},
      createMemo: () => 0,
      batch: () => {},
    },
    test: {
      pass: () => {},
      fail: () => {},
      summary: () => {},
    },
    http: { fetch: () => 0, fetchGetBody: () => 0, fetchGetStatus: () => 0 },
    worker: {
      spawn: () => 0,
      channelCreate: () => 0,
      channelSend: () => {},
      channelRecv: () => {},
      parallel: () => {},
    },
    ai: {
      chatStream: () => {},
      chatComplete: () => {},
      registerTool: () => {},
      embed: () => {},
      parseStructured: () => 0,
    },
    streaming: {
      streamFetch: () => {},
      sseConnect: () => {},
      wsConnect: () => {},
      wsSend: () => {},
      wsClose: () => {},
      yield: () => {},
    },
    media: {
      lazyImage: () => {},
      decodeImage: () => {},
      preload: () => {},
      progressiveImage: () => {},
    },
    router: {
      init: () => {},
      navigate: () => {},
      currentPath: () => 0,
      getParam: () => 0,
      registerRoute: () => {},
    },
    style: {
      injectStyles: () => 0,
      applyScope: () => {},
    },
    a11y: {
      setAriaAttribute: () => {},
      setRole: () => {},
      manageFocus: () => {},
      announceToScreenReader: () => {},
      trapFocus: () => {},
      releaseFocusTrap: () => {},
    },
  };

  const { instance } = await WebAssembly.instantiate(wasmBytes, importObject);

  // Find and call the mount function (convention: ComponentName_mount)
  for (const [name, fn] of Object.entries(instance.exports)) {
    if (name.endsWith('_mount') && typeof fn === 'function') {
      fn(root.id);
      break;
    }
  }

  return new TestElement(root, instance, memory);
}
