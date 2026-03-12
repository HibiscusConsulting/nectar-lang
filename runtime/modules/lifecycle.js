// runtime/modules/lifecycle.js — Component lifecycle and cleanup runtime

const LifecycleRuntime = {
  _cleanups: new Map(),

  registerCleanup(readString, componentPtr, componentLen) {
    const name = readString(componentPtr, componentLen);
    LifecycleRuntime._cleanups.set(name, []);
  },

  addCleanup(name, fn) {
    const cleanups = LifecycleRuntime._cleanups.get(name) || [];
    cleanups.push(fn);
    LifecycleRuntime._cleanups.set(name, cleanups);
  },

  destroy(name) {
    const cleanups = LifecycleRuntime._cleanups.get(name) || [];
    cleanups.forEach((fn) => fn());
    LifecycleRuntime._cleanups.delete(name);
  },
};

const lifecycleModule = {
  name: 'lifecycle',
  runtime: LifecycleRuntime,
  wasmImports: {
    lifecycle: {
      registerCleanup: LifecycleRuntime.registerCleanup,
      addCleanup: LifecycleRuntime.addCleanup,
      destroy: LifecycleRuntime.destroy,
    }
  }
};

if (typeof module !== "undefined") module.exports = lifecycleModule;
