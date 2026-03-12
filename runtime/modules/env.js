// runtime/modules/env.js — Environment variable runtime

const EnvRuntime = {
  _vars: {},

  init(vars) { EnvRuntime._vars = vars || {}; },

  get(readString, namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    return EnvRuntime._vars[name] || '';
  },
};

const envModule = {
  name: 'env',
  runtime: EnvRuntime,
  wasmImports: {
    nectarenv: {
      init: EnvRuntime.init,
      get: EnvRuntime.get,
    }
  }
};

if (typeof module !== "undefined") module.exports = envModule;
