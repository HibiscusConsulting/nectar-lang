// runtime/modules/seo.js — SEO runtime (meta tags, structured data, sitemap)

const SeoRuntime = {
  _pages: new Map(),
  _meta: new Map(),
  _structuredData: [],
  _routes: [],

  setMeta(runtime, titlePtr, titleLen, descPtr, descLen, canonPtr, canonLen, ogPtr, ogLen) {
    const title = titleLen > 0 ? runtime.readString(titlePtr, titleLen) : null;
    const desc = descLen > 0 ? runtime.readString(descPtr, descLen) : null;
    const canon = canonLen > 0 ? runtime.readString(canonPtr, canonLen) : null;
    const og = ogLen > 0 ? runtime.readString(ogPtr, ogLen) : null;

    if (typeof document !== "undefined") {
      if (title) document.title = title;
      if (desc) {
        let el = document.querySelector('meta[name="description"]');
        if (!el) { el = document.createElement('meta'); el.name = 'description'; document.head.appendChild(el); }
        el.content = desc;
      }
      if (canon) {
        let el = document.querySelector('link[rel="canonical"]');
        if (!el) { el = document.createElement('link'); el.rel = 'canonical'; document.head.appendChild(el); }
        el.href = canon;
      }
      if (og) {
        let el = document.querySelector('meta[property="og:image"]');
        if (!el) { el = document.createElement('meta'); el.setAttribute('property', 'og:image'); document.head.appendChild(el); }
        el.content = og;
      }
    }
  },

  registerStructuredData(runtime, jsonPtr, jsonLen) {
    const json = runtime.readString(jsonPtr, jsonLen);
    SeoRuntime._structuredData.push(JSON.parse(json));
    if (typeof document !== "undefined") {
      const script = document.createElement('script');
      script.type = 'application/ld+json';
      script.textContent = json;
      document.head.appendChild(script);
    }
  },

  registerRoute(runtime, pathPtr, pathLen, priorityPtr, priorityLen) {
    const path = runtime.readString(pathPtr, pathLen);
    const priority = priorityLen > 0 ? runtime.readString(priorityPtr, priorityLen) : '0.8';
    SeoRuntime._routes.push({ path, priority, lastmod: new Date().toISOString().split('T')[0] });
  },

  emitStaticHtml(runtime, componentPtr, componentLen) {
    const name = runtime.readString(componentPtr, componentLen);
    if (typeof document !== "undefined") {
      const html = document.documentElement.outerHTML;
      SeoRuntime._pages.set(name, html);
    }
  },

  generateSitemap(baseUrl) {
    const urls = SeoRuntime._routes.map(r =>
      `  <url>\n    <loc>${baseUrl}${r.path}</loc>\n    <lastmod>${r.lastmod}</lastmod>\n    <priority>${r.priority}</priority>\n  </url>`
    ).join('\n');
    return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>`;
  },

  generateRobots(baseUrl) {
    return `User-agent: *\nAllow: /\n\nSitemap: ${baseUrl}/sitemap.xml`;
  }
};

const seoModule = {
  name: 'seo',
  runtime: SeoRuntime,
  wasmImports: {
    seo: {
      setMeta: SeoRuntime.setMeta,
      registerStructuredData: SeoRuntime.registerStructuredData,
      registerRoute: SeoRuntime.registerRoute,
      emitStaticHtml: SeoRuntime.emitStaticHtml,
    }
  }
};

if (typeof module !== "undefined") module.exports = seoModule;
