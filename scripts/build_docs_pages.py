#!/usr/bin/env python3
"""build_docs_pages.py

Convert every Markdown file in `docs/*.md` into a Nectar page under
`website/src/pages/docs/<slug>.nectar`. Also regenerates the docs hub
(`website/src/pages/docs.nectar`) and the application router
(`website/src/app.nectar`) so the new pages are reachable.

Run from the repo root (or anywhere — paths are resolved relative to
the repo root, which is detected from the script's location):

    python3 scripts/build_docs_pages.py

The generator is idempotent: rerun it after editing any `docs/*.md`
source and the website pages regenerate from scratch. Generated
`.nectar` pages contain a "DO NOT EDIT" header so humans don't hand-edit
them by mistake.
"""

from __future__ import annotations

import os
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parent.parent
DOCS_DIR = REPO_ROOT / "docs"
PAGES_DIR = REPO_ROOT / "website" / "src" / "pages"
DOCS_OUT_DIR = PAGES_DIR / "docs"
APP_FILE = REPO_ROOT / "website" / "src" / "app.nectar"
DOCS_HUB_FILE = PAGES_DIR / "docs.nectar"

# ---------------------------------------------------------------------------
# Doc registry — every Markdown file in docs/ that should appear on the site.
# `slug`     -> URL slug under /docs/<slug>
# `md`       -> source file in docs/
# `title`    -> page title (also nav label)
# `pascal`   -> PascalCase identifier for the Nectar `page` declaration
# `summary`  -> short blurb shown in the docs hub overview
# `group`    -> sidebar group name
# ---------------------------------------------------------------------------

@dataclass
class Doc:
    slug: str
    md: str
    title: str
    pascal: str
    summary: str
    group: str


DOC_REGISTRY: list[Doc] = [
    Doc(
        slug="getting-started",
        md="getting-started.md",
        title="Getting Started",
        pascal="GettingStartedDoc",
        summary="Install the compiler, scaffold a project, run the dev server, and ship your first build.",
        group="Getting Started",
    ),
    Doc(
        slug="language-reference",
        md="language-reference.md",
        title="Language Reference",
        pascal="LanguageReferenceDoc",
        summary="Full Nectar syntax: types, ownership, components, stores, traits, generics, every keyword.",
        group="Language",
    ),
    Doc(
        slug="examples",
        md="examples.md",
        title="Examples",
        pascal="ExamplesDoc",
        summary="Worked examples for every keyword and standard-library module, copy-pastable end-to-end.",
        group="Language",
    ),
    Doc(
        slug="render-modes",
        md="render-modes.md",
        title="Render Modes",
        pascal="RenderModesDoc",
        summary="DOM, Canvas (Honeycomb), and Hybrid render targets — when and why to use each.",
        group="Platform",
    ),
    Doc(
        slug="providers",
        md="providers.md",
        title="Providers",
        pascal="ProvidersDoc",
        summary="The keyword/stdlib/provider three-layer model, built-in providers, and how to add a new one.",
        group="Platform",
    ),
    Doc(
        slug="runtime-api",
        md="runtime-api.md",
        title="Runtime API",
        pascal="RuntimeApiDoc",
        summary="The JS syscall layer, command buffer, WASM imports — how Nectar talks to the browser.",
        group="Platform",
    ),
    Doc(
        slug="toolchain",
        md="toolchain.md",
        title="Toolchain",
        pascal="ToolchainDoc",
        summary="CLI reference, formatter, linter, LSP, package manager, dev server.",
        group="Toolchain",
    ),
    Doc(
        slug="nectar-for-ai",
        md="nectar-for-ai.md",
        title="AI Integration",
        pascal="NectarForAiDoc",
        summary="Agents, tools, prompts, and streaming — building AI-native applications in Nectar.",
        group="AI",
    ),
    Doc(
        slug="architecture",
        md="architecture.md",
        title="Architecture",
        pascal="ArchitectureDoc",
        summary="Compiler pipeline (lexer, parser, borrow check, type check, optimizer, codegen) and the Honeycomb runtime.",
        group="Internals",
    ),
    Doc(
        slug="whitepaper",
        md="whitepaper.md",
        title="Whitepaper",
        pascal="WhitepaperDoc",
        summary="Design rationale and the case for compiled, WASM-first frontends.",
        group="Internals",
    ),
]

DOC_BY_FILENAME: dict[str, Doc] = {d.md: d for d in DOC_REGISTRY}

GROUP_ORDER = [
    "Getting Started",
    "Language",
    "Platform",
    "Toolchain",
    "AI",
    "Internals",
]


# ---------------------------------------------------------------------------
# Markdown -> Nectar template render-tree converter.
#
# We intentionally support a focused subset of CommonMark — the subset our
# docs actually use. Anything we do not faithfully render is documented in
# the script header so callers know what to expect.
# ---------------------------------------------------------------------------


def slugify(text: str) -> str:
    """GitHub-style heading slug."""
    text = text.lower()
    text = re.sub(r"[^a-z0-9\s-]", "", text)
    text = re.sub(r"\s+", "-", text.strip())
    text = re.sub(r"-+", "-", text)
    return text


def nectar_string(s: str) -> str:
    """Escape a Python string for use as a Nectar string literal body
    (the bit between the surrounding double quotes)."""
    return s.replace("\\", "\\\\").replace("\"", "\\\"")


def quote(s: str) -> str:
    """Wrap a Python string as a Nectar double-quoted string literal."""
    return '"' + nectar_string(s) + '"'


# Markdown link rewriting: turn `docs/foo.md` (or `./foo.md`, `foo.md`) into
# the website route `/docs/<slug>` (with optional `#anchor`). External
# `http(s)://` links are passed through.
GITHUB_BASE = "https://github.com/HibiscusConsulting/nectar-lang/blob/main"


def rewrite_link(href: str) -> str:
    href = href.strip()
    if href.startswith(("http://", "https://", "mailto:")):
        return href
    if href.startswith("#"):
        # Normalize the fragment with our slugify so anchor links match the
        # IDs we generate on headings (collapses repeated dashes etc.).
        return "#" + slugify(href[1:])

    # Original href before stripping, used to detect "this points outside docs/".
    pointed_outside_docs = href.startswith("../")

    # Strip leading ./ or ../ segments
    cleaned = re.sub(r"^(\.\.?/)+", "", href)

    # Split off fragment
    frag = ""
    if "#" in cleaned:
        cleaned, frag = cleaned.split("#", 1)
        frag = "#" + frag

    # `docs/foo.md` style link from inside docs/
    if cleaned.startswith("docs/"):
        cleaned = cleaned[len("docs/") :]

    # Markdown doc link -> website route
    if cleaned.endswith(".md"):
        slug = cleaned[:-3]
        return f"/docs/{slug}{frag}"

    # If the original was `../something`, it's a path outside docs/ —
    # link to the source on GitHub.
    if pointed_outside_docs:
        return f"{GITHUB_BASE}/{cleaned}{frag}"

    if cleaned == "":
        return frag or "#"

    # Anything else (e.g. a bare filename `foo.png` or `examples/`) — assume
    # it's a repo-relative path the docs are pointing at, link to GitHub.
    return f"{GITHUB_BASE}/{cleaned}{frag}"


# ---------------------------------------------------------------------------
# Inline parsing — produces a list of "child fragments" for a paragraph.
# Each fragment is a dict {"kind": ..., ...}. The renderer turns each into
# a Nectar template node.
# ---------------------------------------------------------------------------


@dataclass
class Inline:
    kind: str
    text: str = ""
    href: str = ""
    children: list["Inline"] = field(default_factory=list)


def parse_inline(text: str) -> list[Inline]:
    """Parse a single paragraph's worth of inline markdown into Inline nodes."""
    out: list[Inline] = []
    i = 0
    n = len(text)
    buf = ""

    def flush_buf():
        nonlocal buf
        if buf:
            out.append(Inline("text", text=buf))
            buf = ""

    while i < n:
        ch = text[i]

        # Inline code: `code`
        if ch == "`":
            # Find closing backtick
            j = text.find("`", i + 1)
            if j != -1:
                flush_buf()
                out.append(Inline("code", text=text[i + 1 : j]))
                i = j + 1
                continue

        # Bold: **text** (greedy match)
        if ch == "*" and i + 1 < n and text[i + 1] == "*":
            j = text.find("**", i + 2)
            if j != -1:
                flush_buf()
                inner = text[i + 2 : j]
                out.append(Inline("strong", children=parse_inline(inner)))
                i = j + 2
                continue

        # Italic: *text* (single-asterisk; we deliberately allow only when not followed
        # by another * — so it doesn't collide with bold)
        if ch == "*" and (i + 1 >= n or text[i + 1] != "*"):
            j = text.find("*", i + 1)
            if j != -1 and j > i + 1:
                flush_buf()
                inner = text[i + 1 : j]
                out.append(Inline("em", children=parse_inline(inner)))
                i = j + 1
                continue

        # Image: ![alt](src)
        if ch == "!" and i + 1 < n and text[i + 1] == "[":
            m = re.match(r"!\[([^\]]*)\]\(([^)]+)\)", text[i:])
            if m:
                flush_buf()
                out.append(Inline("image", text=m.group(1), href=m.group(2)))
                i += m.end()
                continue

        # Link: [text](href)
        if ch == "[":
            m = re.match(r"\[([^\]]+)\]\(([^)]+)\)", text[i:])
            if m:
                flush_buf()
                inner = m.group(1)
                href = m.group(2)
                out.append(Inline("link", href=href, children=parse_inline(inner)))
                i += m.end()
                continue

        # Auto-link / bare URL — light-touch, only for clarity in tables
        if ch == "<":
            m = re.match(r"<((https?|mailto):[^>\s]+)>", text[i:])
            if m:
                flush_buf()
                href = m.group(1)
                out.append(Inline("link", href=href, children=[Inline("text", text=href)]))
                i += m.end()
                continue

        # Default: accumulate
        buf += ch
        i += 1

    flush_buf()
    return out


def render_inline(nodes: list[Inline]) -> str:
    """Render a list of Inline nodes as Nectar template fragments,
    suitable for use as the children of a block element."""
    parts: list[str] = []
    for node in nodes:
        if node.kind == "text":
            parts.append(quote(node.text))
        elif node.kind == "code":
            parts.append(f"<code>{quote(node.text)}</code>")
        elif node.kind == "strong":
            inner = render_inline(node.children) or quote("")
            parts.append(f"<strong>{inner}</strong>")
        elif node.kind == "em":
            inner = render_inline(node.children) or quote("")
            parts.append(f"<em>{inner}</em>")
        elif node.kind == "link":
            href = rewrite_link(node.href)
            inner = render_inline(node.children) or quote(node.href)
            parts.append(f"<a href={quote(href)}>{inner}</a>")
        elif node.kind == "image":
            src = rewrite_link(node.href)
            alt = node.text
            parts.append(f"<img src={quote(src)} alt={quote(alt)} />")
        else:
            parts.append(quote(""))
    return "".join(parts)


# ---------------------------------------------------------------------------
# Block parser — converts a full markdown document into render-tree
# Nectar template lines, plus a list of (level, text, slug) headings used
# by the on-page TOC.
# ---------------------------------------------------------------------------


@dataclass
class Block:
    kind: str
    # heading: level + text
    level: int = 0
    text: str = ""
    slug: str = ""
    # code: lang + lines
    lang: str = ""
    code: str = ""
    # paragraph: inline list
    inlines: list[Inline] = field(default_factory=list)
    # list: items (each item is its own list of blocks for nesting)
    ordered: bool = False
    items: list[list["Block"]] = field(default_factory=list)
    # table: header row + body rows (each cell is an inline list)
    headers: list[list[Inline]] = field(default_factory=list)
    rows: list[list[list[Inline]]] = field(default_factory=list)
    # blockquote: child blocks
    children: list["Block"] = field(default_factory=list)


def parse_markdown(text: str) -> tuple[list[Block], list[tuple[int, str, str]]]:
    lines = text.splitlines()
    blocks: list[Block] = []
    toc: list[tuple[int, str, str]] = []
    used_slugs: dict[str, int] = {}

    i = 0
    n = len(lines)

    def make_unique_slug(base: str) -> str:
        if not base:
            base = "section"
        if base not in used_slugs:
            used_slugs[base] = 1
            return base
        used_slugs[base] += 1
        return f"{base}-{used_slugs[base]}"

    while i < n:
        line = lines[i]
        stripped = line.rstrip()

        # Skip blank lines
        if not stripped.strip():
            i += 1
            continue

        # Fenced code block
        m = re.match(r"^```(\w*)\s*$", stripped)
        if m:
            lang = m.group(1)
            i += 1
            buf: list[str] = []
            while i < n and not re.match(r"^```\s*$", lines[i].rstrip()):
                buf.append(lines[i])
                i += 1
            if i < n:
                i += 1  # skip closing fence
            blocks.append(Block(kind="code", lang=lang, code="\n".join(buf)))
            continue

        # Heading
        m = re.match(r"^(#{1,6})\s+(.+?)\s*$", stripped)
        if m:
            level = len(m.group(1))
            heading_text = m.group(2)
            slug = make_unique_slug(slugify(heading_text))
            blocks.append(Block(kind="heading", level=level, text=heading_text, slug=slug))
            toc.append((level, heading_text, slug))
            i += 1
            continue

        # Horizontal rule
        if re.match(r"^(\*\s*\*\s*\*+|-\s*-\s*-+|_\s*_\s*_+)\s*$", stripped):
            blocks.append(Block(kind="hr"))
            i += 1
            continue

        # Table — simple GitHub-flavored: a header row followed by a separator row
        # | a | b | c |
        # |---|---|---|
        # | 1 | 2 | 3 |
        if stripped.startswith("|") and i + 1 < n and re.match(r"^\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)+\|?\s*$", lines[i + 1].strip()):
            header_cells = split_table_row(stripped)
            i += 2  # skip header + separator
            rows: list[list[list[Inline]]] = []
            while i < n and lines[i].strip().startswith("|"):
                rows.append([parse_inline(cell) for cell in split_table_row(lines[i])])
                i += 1
            blocks.append(
                Block(
                    kind="table",
                    headers=[parse_inline(cell) for cell in header_cells],
                    rows=rows,
                )
            )
            continue

        # Blockquote
        if stripped.startswith(">"):
            buf_lines: list[str] = []
            while i < n and lines[i].lstrip().startswith(">"):
                # Strip the leading "> " or ">"
                line_inner = re.sub(r"^\s*>\s?", "", lines[i])
                buf_lines.append(line_inner)
                i += 1
            inner_blocks, _ = parse_markdown("\n".join(buf_lines))
            blocks.append(Block(kind="blockquote", children=inner_blocks))
            continue

        # Unordered or ordered list
        if re.match(r"^(\s*)([-*+]|\d+\.)\s+", line):
            list_block, consumed = parse_list(lines, i)
            blocks.append(list_block)
            i = consumed
            continue

        # Paragraph — accumulate until blank line / next block
        buf_lines = [stripped]
        i += 1
        while i < n:
            nxt = lines[i].rstrip()
            if not nxt.strip():
                break
            if re.match(r"^#{1,6}\s+", nxt):
                break
            if nxt.startswith("```"):
                break
            if nxt.startswith(">"):
                break
            if re.match(r"^(\s*)([-*+]|\d+\.)\s+", lines[i]):
                break
            if nxt.startswith("|") and i + 1 < n and re.match(r"^\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)+\|?\s*$", lines[i + 1].strip()):
                break
            buf_lines.append(nxt)
            i += 1
        para_text = " ".join(s.strip() for s in buf_lines)
        blocks.append(Block(kind="paragraph", inlines=parse_inline(para_text)))

    return blocks, toc


def split_table_row(line: str) -> list[str]:
    s = line.strip()
    if s.startswith("|"):
        s = s[1:]
    if s.endswith("|"):
        s = s[:-1]
    return [c.strip() for c in s.split("|")]


def parse_list(lines: list[str], start: int) -> tuple[Block, int]:
    """Parse a (possibly nested) list starting at `start`. Returns the
    Block plus the index of the first line after the list."""
    n = len(lines)
    i = start
    first = lines[i]
    base_indent_match = re.match(r"^(\s*)", first)
    base_indent = len(base_indent_match.group(1)) if base_indent_match else 0
    is_ordered = bool(re.match(r"^\s*\d+\.\s+", first))

    items: list[list[Block]] = []
    while i < n:
        ln = lines[i]
        if not ln.strip():
            # Allow blank lines inside a list — peek ahead
            if i + 1 < n and re.match(r"^(\s*)([-*+]|\d+\.)\s+", lines[i + 1]):
                next_indent = len(re.match(r"^(\s*)", lines[i + 1]).group(1))
                if next_indent >= base_indent:
                    i += 1
                    continue
            break

        m = re.match(r"^(\s*)([-*+]|\d+\.)\s+(.*)$", ln)
        if not m:
            break
        indent = len(m.group(1))
        if indent < base_indent:
            break
        if indent > base_indent:
            # Nested list — should have been consumed by a deeper recursion
            break
        item_text = m.group(3)
        # Collect continuation lines (indented deeper than the marker)
        item_lines = [item_text]
        i += 1
        while i < n:
            cont = lines[i]
            if not cont.strip():
                # blank line — keep if next non-blank is a continuation
                lookahead = i + 1
                while lookahead < n and not lines[lookahead].strip():
                    lookahead += 1
                if lookahead < n:
                    cm = re.match(r"^(\s*)", lines[lookahead])
                    cont_indent = len(cm.group(1)) if cm else 0
                    nm = re.match(r"^(\s*)([-*+]|\d+\.)\s+", lines[lookahead])
                    if nm and len(nm.group(1)) <= base_indent:
                        break
                    if cont_indent > base_indent:
                        item_lines.append("")
                        i += 1
                        continue
                break
            cm = re.match(r"^(\s*)", cont)
            cont_indent = len(cm.group(1)) if cm else 0
            nm = re.match(r"^(\s*)([-*+]|\d+\.)\s+", cont)
            if nm and len(nm.group(1)) <= base_indent:
                break
            if cont_indent > base_indent:
                item_lines.append(cont[base_indent + 2 :] if len(cont) > base_indent + 2 else cont.lstrip())
                i += 1
                continue
            break
        item_blocks, _ = parse_markdown("\n".join(item_lines))
        items.append(item_blocks)

    return Block(kind="list", ordered=is_ordered, items=items), i


# ---------------------------------------------------------------------------
# Rendering — convert blocks to Nectar template lines.
# ---------------------------------------------------------------------------


def indent(s: str, n: int) -> str:
    pad = " " * n
    return "\n".join(pad + line if line else line for line in s.split("\n"))


def render_block(block: Block, depth: int = 0) -> str:
    pad = "    " * depth
    if block.kind == "heading":
        tag = f"h{min(block.level + 1, 6)}"  # markdown h1 -> page h2 (the page itself is h1)
        # Allow markdown h1 to remain as h1 only for the page title; we strip it before rendering.
        return f'{pad}<{tag} id="{block.slug}">{quote(block.text)}</{tag}>'
    if block.kind == "paragraph":
        inner = render_inline(block.inlines)
        if not inner:
            return ""
        return f"{pad}<p>{inner}</p>"
    if block.kind == "code":
        # Preserve whitespace and newlines inside <pre><code>
        body = nectar_string(block.code)
        # Use multi-string concatenation by joining lines with \n inside one string literal.
        # We keep it a single string literal so monospace rendering preserves exact spacing.
        # Note: Nectar attribute names must be plain identifiers (no dashes),
        # so we skip the language-tag attribute entirely. Language is shown
        # via a sibling label rather than a `data-lang` attribute.
        if block.lang:
            label = f'{pad}<div class="code-lang-label">{quote(block.lang)}</div>\n'
        else:
            label = ""
        return f'{label}{pad}<pre class="code-block"><code>"{body}"</code></pre>'
    if block.kind == "hr":
        return f"{pad}<hr />"
    if block.kind == "blockquote":
        inner_lines = [render_block(child, depth + 1) for child in block.children]
        inner = "\n".join(line for line in inner_lines if line)
        return f"{pad}<blockquote>\n{inner}\n{pad}</blockquote>"
    if block.kind == "list":
        tag = "ol" if block.ordered else "ul"
        item_lines: list[str] = []
        for item in block.items:
            # If the item is a single paragraph, render its inline content directly inside <li>
            if len(item) == 1 and item[0].kind == "paragraph":
                inner = render_inline(item[0].inlines) or quote("")
                item_lines.append(f"{pad}    <li>{inner}</li>")
            else:
                child_blocks = "\n".join(render_block(b, depth + 2) for b in item if render_block(b, depth + 2))
                item_lines.append(f"{pad}    <li>\n{child_blocks}\n{pad}    </li>")
        items_str = "\n".join(item_lines)
        return f"{pad}<{tag}>\n{items_str}\n{pad}</{tag}>"
    if block.kind == "table":
        thead_cells = "".join(f"<th>{render_inline(cell) or quote('')}</th>" for cell in block.headers)
        body_rows = []
        for row in block.rows:
            tds = "".join(f"<td>{render_inline(cell) or quote('')}</td>" for cell in row)
            body_rows.append(f"{pad}        <tr>{tds}</tr>")
        body_str = "\n".join(body_rows)
        return (
            f'{pad}<div class="doc-table-wrap">\n'
            f'{pad}    <table class="doc-table">\n'
            f'{pad}        <thead><tr>{thead_cells}</tr></thead>\n'
            f"{pad}        <tbody>\n{body_str}\n{pad}        </tbody>\n"
            f"{pad}    </table>\n"
            f"{pad}</div>"
        )
    return ""


# ---------------------------------------------------------------------------
# Page generation
# ---------------------------------------------------------------------------


HEADER = """// {file} — auto-generated from {source}
// DO NOT EDIT BY HAND. Run scripts/build_docs_pages.py to regenerate.
"""


SHARED_STYLES = """
        .doc-page { color: #e0e0e0; background: #1a1a2e; min-height: 100vh; }
        .doc-layout { display: grid; grid-template-columns: 260px 1fr; max-width: 1280px; margin: 0 auto; gap: 0; }
        .doc-sidebar { padding: 2rem 1.5rem; border-right: 1px solid #2a2a4e; position: sticky; top: 0; height: 100vh; overflow-y: auto; box-sizing: border-box; }
        .doc-sidebar-group { color: #e94560; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.12em; margin-top: 1.5rem; margin-bottom: 0.5rem; font-weight: 700; }
        .doc-sidebar-group:first-child { margin-top: 0; }
        .doc-sidebar a { display: block; color: #a0a0b0; text-decoration: none; padding: 0.3rem 0.5rem; font-size: 0.92rem; border-radius: 4px; }
        .doc-sidebar a:hover { color: #ffffff; background: #2a2a4e; }
        .doc-sidebar a.current { color: #ffffff; background: #16213e; border-left: 2px solid #e94560; padding-left: 0.4rem; }
        .doc-content { padding: 3rem 3rem 6rem; max-width: 860px; box-sizing: border-box; }
        .doc-content h1 { font-size: 2.5rem; color: #ffffff; margin-bottom: 1rem; line-height: 1.2; }
        .doc-content h2 { font-size: 1.6rem; color: #ffffff; margin-top: 2.5rem; margin-bottom: 1rem; padding-top: 0.5rem; border-top: 1px solid #2a2a4e; }
        .doc-content h3 { font-size: 1.25rem; color: #ffffff; margin-top: 2rem; margin-bottom: 0.75rem; }
        .doc-content h4 { font-size: 1.05rem; color: #d0d0d8; margin-top: 1.5rem; margin-bottom: 0.5rem; }
        .doc-content h5, .doc-content h6 { font-size: 0.95rem; color: #d0d0d8; margin-top: 1.25rem; margin-bottom: 0.4rem; text-transform: uppercase; letter-spacing: 0.05em; }
        .doc-content p { color: #c8c8d4; line-height: 1.7; margin-bottom: 1rem; }
        .doc-content a { color: #58a6ff; text-decoration: none; }
        .doc-content a:hover { text-decoration: underline; }
        .doc-content ul, .doc-content ol { color: #c8c8d4; line-height: 1.7; margin-bottom: 1rem; padding-left: 1.5rem; }
        .doc-content li { margin-bottom: 0.4rem; }
        .doc-content strong { color: #ffffff; }
        .doc-content em { color: #e0e0ea; }
        .doc-content code { font-family: 'JetBrains Mono', Menlo, Consolas, monospace; font-size: 0.88em; background: #0d1117; color: #e94560; padding: 0.15em 0.4em; border-radius: 4px; border: 1px solid #2a2a4e; }
        .doc-content pre.code-block { background: #0d1117; border: 1px solid #2a2a4e; border-radius: 8px; padding: 1.2rem 1.4rem; overflow-x: auto; margin: 1.25rem 0; }
        .doc-content pre.code-block code { background: transparent; color: #e6edf3; border: none; padding: 0; font-size: 0.88rem; line-height: 1.55; white-space: pre; display: block; }
        .doc-content blockquote { border-left: 3px solid #e94560; padding: 0.5rem 1rem; color: #b8b8c4; background: #16213e; margin: 1rem 0; border-radius: 0 6px 6px 0; }
        .doc-content blockquote p { margin: 0.4rem 0; }
        .doc-content hr { border: none; border-top: 1px solid #2a2a4e; margin: 2.5rem 0; }
        .doc-content .doc-table-wrap { overflow-x: auto; margin: 1.25rem 0; }
        .doc-content table.doc-table { width: 100%; border-collapse: collapse; font-size: 0.92rem; }
        .doc-content table.doc-table th, .doc-content table.doc-table td { padding: 0.6rem 0.9rem; text-align: left; border: 1px solid #2a2a4e; vertical-align: top; }
        .doc-content table.doc-table thead th { background: #16213e; color: #ffffff; font-weight: 600; }
        .doc-content table.doc-table tbody tr:nth-child(odd) { background: rgba(22, 33, 62, 0.4); }
        .doc-content img { max-width: 100%; height: auto; border-radius: 6px; margin: 1rem 0; }
        .doc-breadcrumbs { font-size: 0.85rem; color: #8b949e; margin-bottom: 0.5rem; }
        .doc-breadcrumbs a { color: #8b949e; }
        .doc-breadcrumbs a:hover { color: #ffffff; }
"""


def render_sidebar(current_slug: str | None) -> str:
    out: list[str] = []
    for group in GROUP_ORDER:
        out.append(f'                <div class="doc-sidebar-group">{quote(group)}</div>')
        for d in DOC_REGISTRY:
            if d.group != group:
                continue
            cls = ' class="current"' if d.slug == current_slug else ""
            out.append(f'                <a href="/docs/{d.slug}"{cls}>{quote(d.title)}</a>')
    return "\n".join(out)


def render_doc_page(doc: Doc, blocks: list[Block], toc: list[tuple[int, str, str]]) -> str:
    # The first H1 in the markdown is treated as the page heading and stripped
    # from the body so we don't get a duplicate.
    body_blocks: list[Block] = []
    page_title = doc.title
    seen_h1 = False
    for b in blocks:
        if not seen_h1 and b.kind == "heading" and b.level == 1:
            page_title = b.text
            seen_h1 = True
            continue
        body_blocks.append(b)

    body_lines: list[str] = []
    for b in body_blocks:
        rendered = render_block(b, depth=4)
        if rendered:
            body_lines.append(rendered)
    body_str = "\n".join(body_lines)

    sidebar = render_sidebar(doc.slug)

    description = doc.summary.replace("\"", "'")

    page_id = doc.pascal

    return f"""{HEADER.format(file=f"docs/{doc.slug}.nectar", source=f"docs/{doc.md}")}
page {page_id}() {{
    meta {{
        title: {quote(page_title + " — Nectar Documentation")},
        description: {quote(description)},
        canonical: {quote(f"/docs/{doc.slug}")},
    }}

    render {{
        <main class="doc-page">
            <Nav />

            <div class="doc-layout">
                <aside class="doc-sidebar">
{sidebar}
                </aside>

                <div class="doc-content">
                    <div class="doc-breadcrumbs">
                        <a href="/docs">"Documentation"</a>" / "{quote(doc.title)}
                    </div>
                    <h1>{quote(page_title)}</h1>
{body_str}
                </div>
            </div>

            <Footer />
        </main>
    }}

    style {{{SHARED_STYLES}    }}
}}
"""


def render_docs_hub() -> str:
    """The /docs landing page — overview + grouped sidebar."""
    sidebar = render_sidebar(None)

    cards: list[str] = []
    for group in GROUP_ORDER:
        group_docs = [d for d in DOC_REGISTRY if d.group == group]
        if not group_docs:
            continue
        cards.append(f'                    <h2 id={quote(slugify(group))}>{quote(group)}</h2>')
        cards.append('                    <div class="doc-cards">')
        for d in group_docs:
            cards.append('                        <a class="doc-card" href={href} aria-label={aria}>'
                         '<h3>{title}</h3><p>{summary}</p></a>'.format(
                             href=quote(f"/docs/{d.slug}"),
                             aria=quote(d.title),
                             title=quote(d.title),
                             summary=quote(d.summary),
                         ))
        cards.append('                    </div>')
    cards_str = "\n".join(cards)

    return f"""// docs.nectar — Documentation hub
// auto-generated from scripts/build_docs_pages.py.
// The hub overview + sidebar mirror DOC_REGISTRY in that script.

page Docs() {{
    meta {{
        title: "Nectar Documentation",
        description: "Learn how to build frontend applications with Nectar. Guides, language reference, architecture, and AI integration.",
        canonical: "/docs",
    }}

    render {{
        <main class="doc-page">
            <Nav />

            <div class="doc-layout">
                <aside class="doc-sidebar">
{sidebar}
                </aside>

                <div class="doc-content">
                    <h1>"Nectar Documentation"</h1>
                    <p>"Nectar is a compiled-to-WASM frontend language. It replaces JavaScript, React, and their entire ecosystem with a single compiler that produces safe, fast, native-feeling web applications."</p>
                    <p>"Browse the full documentation below — every guide, reference, and example. Pick a topic from the sidebar or jump in from the cards."</p>

{cards_str}
                </div>
            </div>

            <Footer />
        </main>
    }}

    style {{{SHARED_STYLES}        .doc-cards {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 1.25rem; margin: 1rem 0 2rem; }}
        .doc-card {{ display: block; background: #16213e; padding: 1.4rem; border-radius: 10px; border: 1px solid #2a2a4e; text-decoration: none; transition: border-color 0.2s, transform 0.2s; }}
        .doc-card:hover {{ border-color: #e94560; transform: translateY(-2px); text-decoration: none; }}
        .doc-card h3 {{ color: #ffffff; margin: 0 0 0.5rem; font-size: 1.1rem; }}
        .doc-card p {{ color: #a0a0b0; font-size: 0.92rem; margin: 0; line-height: 1.5; }}
    }}
}}
"""


def render_app_nectar() -> str:
    routes = [
        '        route "/" => Home,',
        '        route "/docs" => Docs,',
    ]
    for d in DOC_REGISTRY:
        routes.append(f'        route "/docs/{d.slug}" => {d.pascal},')
    routes.extend([
        '        route "/install" => Install,',
        '        route "/examples" => Examples,',
        '        route "/playground" => Playground,',
        '        fallback => NotFound,',
    ])
    routes_str = "\n".join(routes)

    return f"""// app.nectar — buildnectar.com main application
//
// This website is built with Nectar, proving the language works
// for real-world production use.
//
// Documentation routes are auto-generated. Edit DOC_REGISTRY in
// scripts/build_docs_pages.py and rerun the script to update them.

app BuildNectar {{
    manifest {{
        name: "Nectar — Build Better Frontends",
        short_name: "Nectar",
        theme_color: "#1a1a2e",
        background_color: "#1a1a2e",
        display: "minimal-ui",
    }}

    offline {{
        precache: ["/", "/docs", "/install", "/examples"],
        strategy: "stale-while-revalidate",
        fallback: OfflinePage,
    }}

    router SiteRouter {{
{routes_str}
    }}
}}
"""


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def main(argv: list[str]) -> int:
    if not DOCS_DIR.exists():
        print(f"docs/ directory not found at {DOCS_DIR}", file=sys.stderr)
        return 1

    DOCS_OUT_DIR.mkdir(parents=True, exist_ok=True)

    written: list[Path] = []

    for doc in DOC_REGISTRY:
        md_path = DOCS_DIR / doc.md
        if not md_path.exists():
            print(f"WARN: source markdown missing: {md_path}", file=sys.stderr)
            continue
        text = md_path.read_text(encoding="utf-8")
        blocks, toc = parse_markdown(text)
        out = render_doc_page(doc, blocks, toc)
        out_path = DOCS_OUT_DIR / f"{doc.slug}.nectar"
        out_path.write_text(out, encoding="utf-8")
        written.append(out_path)
        print(f"wrote {out_path.relative_to(REPO_ROOT)}")

    DOCS_HUB_FILE.write_text(render_docs_hub(), encoding="utf-8")
    written.append(DOCS_HUB_FILE)
    print(f"wrote {DOCS_HUB_FILE.relative_to(REPO_ROOT)}")

    APP_FILE.write_text(render_app_nectar(), encoding="utf-8")
    written.append(APP_FILE)
    print(f"wrote {APP_FILE.relative_to(REPO_ROOT)}")

    print(f"\nGenerated {len(written)} files.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
