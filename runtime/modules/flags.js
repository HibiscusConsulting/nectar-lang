// runtime/modules/flags.js — Feature flag runtime

const FlagRuntime = {
  _flags: new Set(),

  init(flags) { (flags || []).forEach(f => FlagRuntime._flags.add(f)); },

  isEnabled(readString, namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    return FlagRuntime._flags.has(name) ? 1 : 0;
  },
};

const flagsModule = {
  name: 'flags',
  runtime: FlagRuntime,
  wasmImports: {
    flags: {
      init: FlagRuntime.init,
      isEnabled: FlagRuntime.isEnabled,
    }
  }
};

if (typeof module !== "undefined") module.exports = flagsModule;
