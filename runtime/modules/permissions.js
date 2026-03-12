// runtime/modules/permissions.js — Permission enforcement runtime

class PermissionError extends Error {
  constructor(message) {
    super(message);
    this.name = "PermissionError";
  }
}

const PermissionsRuntime = {
  _registry: new Map(),

  registerPermissions(componentName, permissionsJson) {
    try {
      const perms = JSON.parse(permissionsJson);
      this._registry.set(componentName, perms);
    } catch (e) {
      console.warn(`[nectar] Failed to parse permissions for ${componentName}:`, e);
    }
  },

  checkNetwork(url, allowedPatterns) {
    if (!allowedPatterns || allowedPatterns.length === 0) return;
    const matched = allowedPatterns.some((pattern) => this._matchPattern(url, pattern));
    if (!matched) {
      throw new PermissionError(
        `Network access denied: "${url}" does not match any allowed pattern: [${allowedPatterns.join(", ")}]`
      );
    }
  },

  checkStorage(key, allowedKeys) {
    if (!allowedKeys || allowedKeys.length === 0) return;
    const matched = allowedKeys.some((pattern) => this._matchPattern(key, pattern));
    if (!matched) {
      throw new PermissionError(
        `Storage access denied: "${key}" does not match any allowed key: [${allowedKeys.join(", ")}]`
      );
    }
  },

  generateCSP() {
    const connectSources = new Set(["'self'"]);
    for (const [, perms] of this._registry) {
      if (perms.network) {
        for (const pattern of perms.network) {
          try {
            const url = new URL(pattern.replace(/\*/g, "placeholder"));
            connectSources.add(url.origin);
          } catch {
            connectSources.add(pattern.replace(/\/\*$/, ""));
          }
        }
      }
    }
    return `connect-src ${[...connectSources].join(" ")}`;
  },

  _matchPattern(value, pattern) {
    const escaped = pattern
      .replace(/[.+^${}()|[\]\\]/g, "\\$&")
      .replace(/\*/g, ".*");
    const regex = new RegExp(`^${escaped}$`);
    return regex.test(value);
  },
};

const permissionsModule = {
  name: 'permissions',
  runtime: { PermissionsRuntime, PermissionError },
  wasmImports: {
    permissions: {
      registerPermissions: PermissionsRuntime.registerPermissions,
      checkNetwork: PermissionsRuntime.checkNetwork,
      checkStorage: PermissionsRuntime.checkStorage,
      generateCSP: PermissionsRuntime.generateCSP,
    }
  }
};

if (typeof module !== "undefined") module.exports = permissionsModule;
