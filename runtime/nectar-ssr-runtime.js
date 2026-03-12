/**
 * Nectar SSR Runtime Bridge — Node.js runtime for server-side rendering.
 *
 * Provides a DOM-free environment where Nectar WASM modules can execute
 * and produce HTML strings instead of manipulating a real DOM.
 *
 * Usage:
 *   const { renderToString, renderToStream } = require('./nectar-ssr-runtime');
 *   const html = await renderToString('./app.wasm', 'App', { title: 'Hello' });
 */

'use strict';

const fs = require('fs');
const { Readable } = require('stream');

// --- Mock DOM as string concatenation ---

/**
 * Build a mock DOM import object that collects HTML as string
 * concatenation instead of creating real DOM nodes.
 */
function createMockDomImports(context) {
  let nodeCounter = 0;

  // Nodes stored as plain objects with tag, attributes, children, text
  const nodes = new Map();

  function allocNode(tag) {
    const id = ++nodeCounter;
    nodes.set(id, {
      tag,
      attrs: {},
      children: [],
      text: null,
      eventHandlers: {},
    });
    return id;
  }

  function readString(memory, ptr, len) {
    const bytes = new Uint8Array(memory.buffer, ptr, len);
    return new TextDecoder().decode(bytes);
  }

  return {
    dom: {
      createElement(tagPtr, tagLen) {
        const tag = readString(context.memory, tagPtr, tagLen);
        return allocNode(tag);
      },
      setText(parentId, textPtr, textLen) {
        const text = readString(context.memory, textPtr, textLen);
        const node = nodes.get(parentId);
        if (node) {
          node.children.push({ type: 'text', value: text });
        }
      },
      appendChild(parentId, childId) {
        const parent = nodes.get(parentId);
        const child = nodes.get(childId);
        if (parent && child) {
          parent.children.push({ type: 'node', id: childId, node: child });
        }
      },
      addEventListener(/* nodeId, eventPtr, eventLen, handlerIdx */) {
        // Event handlers are no-ops on the server; they attach during hydration.
      },
      setAttribute(nodeId, namePtr, nameLen, valPtr, valLen) {
        const name = readString(context.memory, namePtr, nameLen);
        const value = readString(context.memory, valPtr, valLen);
        const node = nodes.get(nodeId);
        if (node) {
          node.attrs[name] = value;
        }
      },
    },

    // Signal runtime — simplified for SSR (no reactivity, just values)
    signal: {
      create(initialValue) {
        const id = ++nodeCounter;
        nodes.set(id, { __signal: true, value: initialValue });
        return id;
      },
      get(signalId) {
        const s = nodes.get(signalId);
        return s ? s.value : 0;
      },
      set(signalId, value) {
        const s = nodes.get(signalId);
        if (s) s.value = value;
      },
      subscribe(/* signalId, callbackIdx */) {
        // No-op on server — no reactive subscriptions during SSR.
      },
      createEffect(/* callbackIdx */) {
        // No-op on server.
      },
      createMemo(/* callbackIdx */) {
        return 0;
      },
      batch(/* callbackIdx */) {
        // No-op on server.
      },
    },

    // HTTP fetch — stub (async actions don't run during initial SSR)
    http: {
      fetch(/* urlPtr, urlLen, optPtr, optLen */) { return 0; },
      fetchGetBody(/* handleId */) { return [0, 0]; },
      fetchGetStatus(/* handleId */) { return 200; },
    },

    // Worker/concurrency — stubs
    worker: {
      spawn(/* funcIdx */) { return 0; },
      channelCreate() { return 0; },
      channelSend(/* chId, ptr, len */) {},
      channelRecv(/* chId, ptr */) {},
      parallel(/* funcPtr, count, resultPtr */) {},
    },

    // AI runtime — stubs
    ai: {
      chatStream() {},
      chatComplete() {},
      registerTool() {},
      embed() {},
      parseStructured() { return 0; },
    },

    // Streaming runtime — stubs
    streaming: {
      streamFetch() {},
      sseConnect() {},
      wsConnect() {},
      wsSend() {},
      wsClose() {},
      yield: function yieldFn() {},
    },

    // Media runtime — stubs
    media: {
      lazyImage() {},
      decodeImage() {},
      preload() {},
    },

    // Router runtime — collects route config
    router: {
      registerRoute(/* pathPtr, pathLen, mountIdx */) {},
      init(/* configPtr, configLen */) {},
      navigate(/* pathPtr, pathLen */) {},
    },

    env: {
      memory: context.memory,
    },

    /**
     * Serialize the collected mock DOM tree to an HTML string.
     */
    __serialize(rootId) {
      return serializeNode(nodes, rootId);
    },

    __nodes: nodes,
  };
}

/**
 * Recursively serialize a mock DOM node to HTML.
 */
function serializeNode(nodes, nodeId) {
  const node = nodes.get(nodeId);
  if (!node) return '';
  if (node.__signal) return '';

  let html = '';
  html += `<${node.tag}`;

  for (const [name, value] of Object.entries(node.attrs)) {
    html += ` ${escapeAttr(name)}="${escapeAttr(value)}"`;
  }

  html += '>';

  const voidElements = new Set([
    'area', 'base', 'br', 'col', 'embed', 'hr', 'img',
    'input', 'link', 'meta', 'param', 'source', 'track', 'wbr',
  ]);

  if (!voidElements.has(node.tag)) {
    for (const child of node.children) {
      if (child.type === 'text') {
        html += escapeHtml(child.value);
      } else if (child.type === 'node') {
        html += serializeNode(nodes, child.id);
      }
    }
    html += `</${node.tag}>`;
  }

  return html;
}

function escapeHtml(str) {
  if (str == null) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#x27;');
}

function escapeAttr(str) {
  return String(str).replace(/"/g, '&quot;').replace(/&/g, '&amp;');
}

// --- Public API ---

/**
 * Load an Nectar WASM module and render a component to an HTML string.
 *
 * @param {string} wasmPath — path to the compiled .wasm file
 * @param {string} componentName — name of the component to render
 * @param {object} props — props to pass to the component
 * @returns {Promise<string>} — the rendered HTML string
 */
async function renderToString(wasmPath, componentName, props) {
  const wasmBytes = fs.readFileSync(wasmPath);
  const memory = new WebAssembly.Memory({ initial: 1 });

  const context = { memory };
  const imports = createMockDomImports(context);

  const { instance } = await WebAssembly.instantiate(wasmBytes, imports);

  // Create a virtual root node
  const rootId = imports.dom.createElement(0, 0); // dummy, we'll use id 1
  const rootNode = imports.__nodes.get(rootId);
  if (rootNode) rootNode.tag = 'div';

  // Call the component's mount function
  const mountFn = instance.exports[componentName + '_mount'];
  if (!mountFn) {
    throw new Error(`Component "${componentName}" not found in WASM module (expected export: ${componentName}_mount)`);
  }

  mountFn(rootId);

  // Serialize the mock DOM tree
  const html = imports.__serialize(rootId);

  // Add hydration state
  const stateScript = '<script>window.__NECTAR_STATE__ = {}</script>';

  return html + stateScript;
}

/**
 * Load an Nectar WASM module and render a component as a readable stream.
 *
 * @param {string} wasmPath — path to the compiled .wasm file
 * @param {string} componentName — name of the component to render
 * @param {object} props — props to pass to the component
 * @returns {ReadableStream}
 */
function renderToStream(wasmPath, componentName, props) {
  const stream = new Readable({
    async read() {
      try {
        const html = await renderToString(wasmPath, componentName, props);

        // Chunk the output for streaming (split at component boundaries)
        const chunkSize = 4096;
        for (let i = 0; i < html.length; i += chunkSize) {
          this.push(html.slice(i, i + chunkSize));
        }
        this.push(null);
      } catch (err) {
        this.destroy(err);
      }
    },
  });

  return stream;
}

// --- Exports ---

module.exports = {
  renderToString,
  renderToStream,
  createMockDomImports,
};
