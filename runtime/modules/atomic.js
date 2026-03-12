// runtime/modules/atomic.js — Atomic state runtime (race-free state management)

const AtomicStateRuntime = {
  _atomics: new Map(),
  _selectors: new Map(),

  atomicGet(signalId) {
    return AtomicStateRuntime._atomics.get(signalId) ?? 0;
  },

  atomicSet(signalId, value) {
    const old = AtomicStateRuntime._atomics.get(signalId);
    AtomicStateRuntime._atomics.set(signalId, value);
    AtomicStateRuntime._notifySelectors(signalId);
    return old;
  },

  atomicCompareSwap(signalId, expected, desired) {
    const current = AtomicStateRuntime._atomics.get(signalId) ?? 0;
    if (current === expected) {
      AtomicStateRuntime._atomics.set(signalId, desired);
      AtomicStateRuntime._notifySelectors(signalId);
      return 1;
    }
    return 0;
  },

  registerSelector(name, deps, computeFn) {
    AtomicStateRuntime._selectors.set(name, { deps, computeFn, cached: null });
  },

  _notifySelectors(changedSignal) {
    for (const [name, sel] of AtomicStateRuntime._selectors) {
      if (sel.deps.includes(changedSignal)) {
        sel.cached = null;
      }
    }
  },
};

const atomicModule = {
  name: 'atomic',
  runtime: AtomicStateRuntime,
  wasmImports: {
    state: {
      atomicGet: AtomicStateRuntime.atomicGet,
      atomicSet: AtomicStateRuntime.atomicSet,
      atomicCompareSwap: AtomicStateRuntime.atomicCompareSwap,
    }
  }
};

if (typeof module !== "undefined") module.exports = atomicModule;
