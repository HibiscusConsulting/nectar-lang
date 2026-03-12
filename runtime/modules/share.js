// runtime/modules/share.js — Web Share API syscall (navigator.share)
// Logic (validation, fallback) lives in WASM. This is the browser API call only.

const wasmImports = {
  share: {
    canShare() { return navigator.share ? 1 : 0; },
    nativeShare(titlePtr, titleLen, textPtr, textLen, urlPtr, urlLen) {
      if (!navigator.share) return 0;
      const title = NectarRuntime.__getString(titlePtr, titleLen);
      const text = NectarRuntime.__getString(textPtr, textLen);
      const url = NectarRuntime.__getString(urlPtr, urlLen);
      navigator.share({ title, text, url }).catch(() => {});
      return 1;
    },
  },
};

module.exports = { name: 'share', runtime: {}, wasmImports };
