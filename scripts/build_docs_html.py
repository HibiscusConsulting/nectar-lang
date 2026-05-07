#!/usr/bin/env python3
"""Build static HTML doc pages for buildnectar.com from docs/*.md.

Uses pandoc to render markdown, wraps each in a Nectar-themed page
template with sidebar nav, and writes to website/dist/docs/.

Usage:
    python3 scripts/build_docs_html.py
"""

from __future__ import annotations
import re
import shutil
import subprocess
from pathlib import Path
from typing import Iterable

REPO = Path(__file__).resolve().parent.parent
DOCS_DIR = REPO / "docs"
STATIC_DIR = REPO / "website" / "static"
DIST_DIR = REPO / "website" / "dist"
DOCS_OUT = DIST_DIR / "docs"

# Pre-built demo apps tracked at the repo root. Each entry maps a source
# directory to the site URL path where its index.html (and assets) should live.
DEMO_APPS: list[tuple[str, str]] = [
    ("canvas_app-build", "app/canvas"),  # 10K-products canvas demo (incl. #data-table)
    ("comparison", "app/svelte"),         # Nectar vs Svelte 5 comparison
]

# (slug, title, group)
DOCS: list[tuple[str, str, str]] = [
    ("getting-started", "Getting Started", "Getting Started"),
    ("language-reference", "Language Reference", "Language"),
    ("providers", "Providers", "Platform"),
    ("render-modes", "Render Modes", "Platform"),
    ("toolchain", "Toolchain", "Toolchain"),
    ("runtime-api", "Runtime API", "Toolchain"),
    ("nectar-for-ai", "AI Integration", "AI"),
    ("architecture", "Architecture", "Internals"),
    ("examples", "Examples", "Internals"),
    ("whitepaper", "Whitepaper", "Internals"),
]

GROUP_ORDER = ["Getting Started", "Language", "Platform", "Toolchain", "AI", "Internals"]

CSS = """
*,*::before,*::after{box-sizing:border-box;margin:0;padding:0}
:root{
  --bg:#0b0e14;--surface:#131720;--surface2:#1a1f2e;--border:#2a2f3e;
  --text:#e6edf3;--text2:#8b949e;--text3:#6e7681;
  --accent:#f97316;--accent2:#fb923c;--green:#3fb950;--red:#f85149;
  --blue:#58a6ff;--purple:#bc8cff;
}
html{scroll-behavior:smooth}
body{background:var(--bg);color:var(--text);font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Helvetica,Arial,sans-serif;line-height:1.6}
a{color:var(--accent);text-decoration:none}
a:hover{text-decoration:underline}
nav.site{position:fixed;top:0;left:0;width:100%;height:48px;background:#0d1117;border-bottom:1px solid #21262d;display:flex;align-items:center;padding:0 16px;z-index:100;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}
.nav-brand{color:var(--accent);font-size:16px;font-weight:700;margin-right:auto}
.nav-links{display:flex;gap:4px}
.nav-links a{color:var(--text2);font-size:13px;font-weight:500;padding:6px 14px;border-radius:6px;transition:color 150ms,background 150ms}
.nav-links a:hover{color:var(--text);text-decoration:none;background:rgba(255,255,255,0.06)}
.nav-links a.active{color:var(--text);background:rgba(249,115,22,0.12)}
.nav-links a.ext{color:var(--text3);font-size:12px}
.docs-layout{display:grid;grid-template-columns:280px 1fr;max-width:1400px;margin:48px auto 0;min-height:calc(100vh - 48px)}
.docs-sidebar{padding:32px 24px;border-right:1px solid var(--border);position:sticky;top:48px;height:calc(100vh - 48px);overflow-y:auto;background:var(--surface)}
.docs-sidebar h3{color:var(--accent);font-size:11px;text-transform:uppercase;letter-spacing:1.2px;margin-top:24px;margin-bottom:8px;font-weight:700}
.docs-sidebar h3:first-child{margin-top:0}
.docs-sidebar a{display:block;color:var(--text2);padding:6px 8px;font-size:14px;border-radius:6px;transition:color 120ms,background 120ms}
.docs-sidebar a:hover{color:var(--text);text-decoration:none;background:rgba(255,255,255,0.04)}
.docs-sidebar a.current{color:var(--text);background:rgba(249,115,22,0.10);border-left:2px solid var(--accent);padding-left:6px}
.doc-content{padding:48px 64px;max-width:920px}
.doc-content h1{font-size:2.4rem;font-weight:800;color:var(--text);margin-bottom:24px;letter-spacing:-0.5px;line-height:1.2}
.doc-content h2{font-size:1.6rem;font-weight:700;color:var(--text);margin-top:40px;margin-bottom:16px;padding-bottom:8px;border-bottom:1px solid var(--border)}
.doc-content h3{font-size:1.2rem;font-weight:700;color:var(--text);margin-top:28px;margin-bottom:12px}
.doc-content h4{font-size:1.05rem;font-weight:600;color:var(--text);margin-top:20px;margin-bottom:8px}
.doc-content p{color:var(--text);margin-bottom:14px;line-height:1.7}
.doc-content ul,.doc-content ol{margin-left:24px;margin-bottom:14px;color:var(--text)}
.doc-content li{margin-bottom:6px;line-height:1.7}
.doc-content code{background:var(--surface2);color:var(--accent2);padding:2px 6px;border-radius:4px;font-family:"SF Mono",SFMono-Regular,Consolas,monospace;font-size:0.92em}
.doc-content pre{background:#0d1117;border:1px solid var(--border);border-radius:8px;padding:16px 20px;overflow-x:auto;margin-bottom:18px;font-family:"SF Mono",SFMono-Regular,Consolas,monospace;font-size:0.92em;line-height:1.55}
.doc-content pre code{background:transparent;color:var(--text);padding:0;font-size:inherit}
.doc-content table{width:100%;border-collapse:collapse;margin-bottom:18px;font-size:0.95em}
.doc-content table th{text-align:left;padding:10px 12px;background:var(--surface2);border-bottom:2px solid var(--border);color:var(--text);font-weight:700}
.doc-content table td{padding:10px 12px;border-bottom:1px solid var(--border);color:var(--text)}
.doc-content blockquote{border-left:3px solid var(--accent);padding:8px 16px;margin-bottom:18px;color:var(--text2);background:rgba(249,115,22,0.04)}
.doc-content hr{border:none;border-top:1px solid var(--border);margin:28px 0}
.doc-content a{color:var(--blue)}
.doc-content img{max-width:100%;border-radius:8px}
@media (max-width:900px){
  .docs-layout{grid-template-columns:1fr;margin:48px 0 0}
  .docs-sidebar{position:static;height:auto;border-right:none;border-bottom:1px solid var(--border)}
  .doc-content{padding:32px 20px}
}
"""


def md_to_html(md_path: Path) -> str:
    """Run pandoc to convert a markdown file to a fragment of HTML."""
    proc = subprocess.run(
        [
            "pandoc",
            "-f", "markdown+pipe_tables+fenced_code_blocks+backtick_code_blocks",
            "-t", "html5",
            "--standalone=false",
            "--no-highlight",
            str(md_path),
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    html = proc.stdout
    # Rewrite inter-doc markdown links: docs/foo.md -> /docs/foo, foo.md -> /docs/foo
    html = re.sub(r'href="(?:\.\./)?docs/([^"#]+)\.md(#[^"]*)?"', r'href="/docs/\1\2"', html)
    html = re.sub(r'href="\./([^"#]+)\.md(#[^"]*)?"', r'href="/docs/\1\2"', html)
    # Rewrite ../ relative links to repo files (e.g. ../providers/moov.js) to GitHub
    html = re.sub(
        r'href="(\.\./[^"]+)"',
        lambda m: f'href="https://github.com/HibiscusConsulting/nectar-lang/blob/main/{m.group(1).removeprefix("../")}"',
        html,
    )
    return html


def render_sidebar(current_slug: str) -> str:
    by_group: dict[str, list[tuple[str, str]]] = {g: [] for g in GROUP_ORDER}
    for slug, title, group in DOCS:
        by_group[group].append((slug, title))
    parts = []
    for group in GROUP_ORDER:
        items = by_group.get(group, [])
        if not items:
            continue
        parts.append(f'<h3>{group}</h3>')
        for slug, title in items:
            cls = ' class="current"' if slug == current_slug else ''
            parts.append(f'<a href="/docs/{slug}"{cls}>{title}</a>')
    return "\n".join(parts)


def render_top_nav(current_section: str = "docs") -> str:
    items = [
        ("/", "Home", "home"),
        ("/docs/getting-started", "Docs", "docs"),
        ("https://github.com/HibiscusConsulting/nectar-lang", "GitHub", "github"),
    ]
    links = []
    for href, label, key in items:
        cls = ""
        if key == current_section:
            cls = " active"
        elif key == "github":
            cls = " ext"
        links.append(f'<a href="{href}" class="{cls.strip()}">{label}</a>')
    return f"""
<nav class="site">
  <a href="/" class="nav-brand">Nectar</a>
  <div class="nav-links">{''.join(links)}</div>
</nav>
"""


def page_template(title: str, body: str, current_slug: str) -> str:
    sidebar = render_sidebar(current_slug)
    nav = render_top_nav("docs")
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title} — Nectar Docs</title>
<meta name="description" content="{title} — official documentation for the Nectar programming language.">
<meta name="theme-color" content="#0b0e14">
<link rel="icon" type="image/svg+xml" href="/favicon.svg">
<style>{CSS}</style>
</head>
<body>
{nav}
<div class="docs-layout">
<aside class="docs-sidebar">
{sidebar}
</aside>
<main class="doc-content">
{body}
</main>
</div>
</body>
</html>
"""


def docs_index_page() -> str:
    """Generate /docs/index.html — landing page with overview + sidebar."""
    sidebar = render_sidebar("")
    nav = render_top_nav("docs")
    body = """
<h1>Nectar Documentation</h1>
<p>Nectar is a compiled-to-WebAssembly language for the next era of web development. One language, one binary, zero dependencies. Pick a section to get started.</p>

<h2>Start Here</h2>
<ul>
  <li><a href="/docs/getting-started">Getting Started</a> — Install Nectar, scaffold a project, run your first app.</li>
  <li><a href="/docs/language-reference">Language Reference</a> — Full syntax, types, ownership, components, stores, every keyword.</li>
  <li><a href="/docs/examples">Examples</a> — Worked examples for every keyword and stdlib module.</li>
</ul>

<h2>Platform</h2>
<ul>
  <li><a href="/docs/providers">Providers</a> — How keywords, stdlib, and concrete service integrations (Moov, Stripe, Plaid, Alipay, Mapbox) compose.</li>
  <li><a href="/docs/render-modes">Render Modes</a> — DOM, Canvas, and Hybrid rendering modes.</li>
</ul>

<h2>Toolchain</h2>
<ul>
  <li><a href="/docs/toolchain">Toolchain</a> — CLI commands, formatter, linter, LSP.</li>
  <li><a href="/docs/runtime-api">Runtime API</a> — JS syscall layer, command buffer, WASM imports.</li>
</ul>

<h2>AI</h2>
<ul>
  <li><a href="/docs/nectar-for-ai">AI Integration</a> — Agents, tools, prompts, streaming.</li>
</ul>

<h2>Internals</h2>
<ul>
  <li><a href="/docs/architecture">Architecture</a> — Compiler pipeline, runtime, WASM bridge.</li>
  <li><a href="/docs/whitepaper">Whitepaper</a> — Design rationale and the case for compiled WASM-first frontends.</li>
</ul>
"""
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Documentation — Nectar</title>
<meta name="description" content="The official documentation for Nectar — a compiled-to-WebAssembly language for the next era of web development.">
<meta name="theme-color" content="#0b0e14">
<link rel="icon" type="image/svg+xml" href="/favicon.svg">
<style>{CSS}</style>
</head>
<body>
{nav}
<div class="docs-layout">
<aside class="docs-sidebar">
{sidebar}
</aside>
<main class="doc-content">
{body}
</main>
</div>
</body>
</html>
"""


def copy_static_assets():
    """Copy everything in website/static/ verbatim into dist/."""
    if not STATIC_DIR.exists():
        return
    DIST_DIR.mkdir(parents=True, exist_ok=True)
    for src in STATIC_DIR.rglob("*"):
        if src.is_dir():
            continue
        rel = src.relative_to(STATIC_DIR)
        dst = DIST_DIR / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
        print(f"  copied {rel} -> dist/{rel}")


def copy_demo_apps():
    """Copy each pre-built demo app into dist/<url-path>/."""
    for src_name, dest_path in DEMO_APPS:
        src = REPO / src_name
        dst = DIST_DIR / dest_path
        if not src.exists():
            print(f"  skip {src_name} (not in repo)")
            continue
        if dst.exists():
            shutil.rmtree(dst)
        shutil.copytree(src, dst, ignore=shutil.ignore_patterns("generated.rs", "*.wat", src_name))
        n = sum(1 for _ in dst.rglob("*") if _.is_file())
        print(f"  copied {src_name}/ -> dist/{dest_path}/ ({n} files)")


def build():
    DOCS_OUT.mkdir(parents=True, exist_ok=True)
    print(f"Building site into {DIST_DIR}")
    print("Copying static assets:")
    copy_static_assets()
    print("Copying demo apps:")
    copy_demo_apps()
    print("Rendering doc pages:")
    for slug, title, group in DOCS:
        md = DOCS_DIR / f"{slug}.md"
        if not md.exists():
            print(f"  skip {slug} (missing {md.name})")
            continue
        body = md_to_html(md)
        html = page_template(title, body, slug)
        out = DOCS_OUT / f"{slug}.html"
        out.write_text(html)
        print(f"  wrote {out.relative_to(REPO)} ({len(html):,} bytes)")
    index = DOCS_OUT / "index.html"
    index.write_text(docs_index_page())
    print(f"  wrote {index.relative_to(REPO)} ({len(index.read_text()):,} bytes)")
    print("Done.")


if __name__ == "__main__":
    build()
