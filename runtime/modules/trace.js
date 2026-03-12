// runtime/modules/trace.js — Observability and performance tracing runtime

const TraceRuntime = {
  _spans: new Map(),
  _nextId: 1,
  _errors: [],

  start(readString, labelPtr, labelLen) {
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

  error(readString, id, msgPtr, msgLen) {
    const msg = readString(msgPtr, msgLen);
    const span = TraceRuntime._spans.get(id);
    TraceRuntime._errors.push({ label: span?.label, error: msg, timestamp: Date.now() });
  },

  getMetrics() {
    const spans = [...TraceRuntime._spans.values()].filter(s => s.duration);
    return { spans, errors: TraceRuntime._errors };
  },
};

const traceModule = {
  name: 'trace',
  runtime: TraceRuntime,
  wasmImports: {
    trace: {
      start: TraceRuntime.start,
      end: TraceRuntime.end,
      error: TraceRuntime.error,
      getMetrics: TraceRuntime.getMetrics,
    }
  }
};

if (typeof module !== "undefined") module.exports = traceModule;
