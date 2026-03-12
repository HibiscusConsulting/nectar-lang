// runtime/modules/embed.js — Third-party script/widget integration

const EmbedRuntime = {
  _embeds: new Map(),

  loadScript(readString, srcPtr, srcLen, loadingPtr, loadingLen, integrityOffset) {
    const src = readString(srcPtr, srcLen);
    const loading = readString(loadingPtr, loadingLen);

    const script = document.createElement('script');
    script.src = src;

    switch (loading) {
      case 'defer': script.defer = true; break;
      case 'async': script.async = true; break;
      case 'lazy':
        script.dataset.lazySrc = src;
        script.removeAttribute('src');
        const observer = new IntersectionObserver((entries) => {
          entries.forEach(entry => {
            if (entry.isIntersecting) {
              script.src = script.dataset.lazySrc;
              observer.disconnect();
            }
          });
        });
        if (document.body) observer.observe(document.body);
        break;
      case 'idle':
        if (typeof requestIdleCallback !== 'undefined') {
          requestIdleCallback(() => document.head.appendChild(script));
          return;
        }
        break;
    }

    if (integrityOffset > 0) {
      script.crossOrigin = 'anonymous';
    }

    document.head.appendChild(script);
    EmbedRuntime._embeds.set(src, { script, loading });
  },

  loadSandboxed(readString, namePtr, nameLen, srcPtr, srcLen) {
    const name = readString(namePtr, nameLen);
    const src = readString(srcPtr, srcLen);

    const iframe = document.createElement('iframe');
    iframe.src = src;
    iframe.sandbox = 'allow-scripts';
    iframe.style.cssText = 'border:none;width:100%;';
    iframe.title = name;
    iframe.loading = 'lazy';

    EmbedRuntime._embeds.set(name, { iframe, sandboxed: true });
    return iframe;
  },

  audit() {
    const report = [];
    for (const [key, embed] of EmbedRuntime._embeds) {
      report.push({ name: key, sandboxed: !!embed.sandboxed, loading: embed.loading || 'default' });
    }
    return report;
  },
};

const embedModule = {
  name: 'embed',
  runtime: EmbedRuntime,
  wasmImports: {
    embed: {
      loadScript: EmbedRuntime.loadScript,
      loadSandboxed: EmbedRuntime.loadSandboxed,
    }
  }
};

if (typeof module !== "undefined") module.exports = embedModule;
