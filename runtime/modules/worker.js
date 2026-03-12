// runtime/modules/worker.js — Web Worker concurrency runtime
// Note: WorkerPool is defined in core.js; this module provides the WASM import bindings.

const WorkerRuntime = {
  spawn(runtime, funcIdx) {
    if (runtime.workerPool) {
      runtime.workerPool.spawnByIndex(funcIdx);
    }
    return funcIdx;
  },

  channelCreate(runtime) {
    const channelId = runtime.nextChannelId++;
    const { port1, port2 } = new MessageChannel();
    runtime.channels.set(channelId, { port1, port2, buffer: [], waiters: [] });
    return channelId;
  },

  channelSend(runtime, channelId, valuePtr, valueLen) {
    const ch = runtime.channels.get(channelId);
    if (!ch) return;
    const bytes = new Uint8Array(runtime.memory.buffer, valuePtr, valueLen).slice();
    if (ch.waiters.length > 0) {
      const waiter = ch.waiters.shift();
      waiter(bytes);
    } else {
      ch.buffer.push(bytes);
    }
    ch.port1.postMessage(bytes);
  },

  channelRecv(runtime, channelId, callbackIdx) {
    const ch = runtime.channels.get(channelId);
    if (!ch) return;
    const deliver = (bytes) => {
      const ptr = runtime._allocWasm(bytes.length);
      new Uint8Array(runtime.memory.buffer, ptr, bytes.length).set(bytes);
      const handler = runtime.instance.exports[`__channel_recv_${callbackIdx}`];
      if (handler) handler(ptr, bytes.length);
    };
    if (ch.buffer.length > 0) {
      deliver(ch.buffer.shift());
    } else {
      ch.waiters.push(deliver);
    }
  },

  parallel(runtime, funcIndicesPtr, funcIndicesLen, callbackIdx) {
    const indices = [];
    const view = new DataView(runtime.memory.buffer);
    for (let i = 0; i < funcIndicesLen; i++) {
      indices.push(view.getInt32(funcIndicesPtr + i * 4, true));
    }
    const promises = indices.map((funcIdx) => {
      if (runtime.workerPool) return runtime.workerPool.spawnByIndex(funcIdx);
      const table = runtime.instance.exports.__indirect_function_table;
      if (table) { const fn = table.get(funcIdx); return Promise.resolve(fn ? fn() : undefined); }
      return Promise.resolve(undefined);
    });
    Promise.all(promises).then((results) => {
      const handler = runtime.instance.exports[`__parallel_done_${callbackIdx}`];
      if (handler) {
        const resultBytes = new Int32Array(results.map(r => r || 0));
        const ptr = runtime._allocWasm(resultBytes.byteLength);
        new Uint8Array(runtime.memory.buffer, ptr, resultBytes.byteLength)
          .set(new Uint8Array(resultBytes.buffer));
        handler(ptr, results.length);
      }
    });
  },
};

const workerModule = {
  name: 'worker',
  runtime: WorkerRuntime,
  wasmImports: {
    worker: {
      spawn: WorkerRuntime.spawn,
      channelCreate: WorkerRuntime.channelCreate,
      channelSend: WorkerRuntime.channelSend,
      channelRecv: WorkerRuntime.channelRecv,
      parallel: WorkerRuntime.parallel,
    }
  }
};

if (typeof module !== "undefined") module.exports = workerModule;
