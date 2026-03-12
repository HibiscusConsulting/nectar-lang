// runtime/modules/loader.js — Code splitting / chunk loading runtime

const LoaderRuntime = {
  _chunks: new Map(),
  _loaded: new Set(),

  async loadChunk(readString, namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    if (LoaderRuntime._loaded.has(name)) return 1;
    const script = document.createElement("script");
    script.src = `/chunks/${name}.js`;
    await new Promise((resolve, reject) => {
      script.onload = resolve;
      script.onerror = reject;
      document.head.appendChild(script);
    });
    LoaderRuntime._loaded.add(name);
    return 1;
  },

  preloadChunk(readString, namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    if (!LoaderRuntime._loaded.has(name)) {
      const link = document.createElement("link");
      link.rel = "modulepreload";
      link.href = `/chunks/${name}.js`;
      document.head.appendChild(link);
    }
  },
};

const loaderModule = {
  name: 'loader',
  runtime: LoaderRuntime,
  wasmImports: {
    loader: {
      loadChunk: LoaderRuntime.loadChunk,
      preloadChunk: LoaderRuntime.preloadChunk,
    }
  }
};

if (typeof module !== "undefined") module.exports = loaderModule;
