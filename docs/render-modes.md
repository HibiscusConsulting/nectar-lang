# Render Modes

Nectar supports three render modes. Each page in your application can choose its own mode based on its requirements.

## Quick Start

```bash
# DOM mode (default) — traditional browser rendering
nectar build app.nectar --render=dom

# Canvas mode — WASM layout + Canvas 2D paint, zero DOM
nectar build app.nectar --render=canvas

# Hybrid mode — Canvas paint + hidden DOM for SEO/accessibility
nectar build app.nectar --render=hybrid
```

## Per-Page Render Mode

```nectar
// Marketing page — needs SEO, accessibility
page Home {
    render: "dom"

    render {
        <div class="hero">
            <h1>"Welcome to our store"</h1>
        </div>
    }
}

// Dashboard — behind auth, performance-critical
page Dashboard {
    render: "canvas"

    render {
        <stack direction="horizontal" gap={16}>
            <stack width={300}><Chart data={self.metrics} /></stack>
            <stack fill>{for item in self.feed { <FeedItem data={item} /> }}</stack>
        </stack>
    }
}

// Product catalog — needs both speed and SEO
page Catalog {
    render: "hybrid"

    render {
        <stack direction="horizontal" wrap={true} gap={16} pad={40}>
            {for product in self.products {
                <ProductCard data={product} />
            }}
        </stack>
    }
}
```

The template syntax is identical across all three modes. The compiler generates different output based on the render mode.

## Mode Comparison

| Feature | DOM | Canvas | Hybrid |
|---|---|---|---|
| 10K item render | 250-320ms | **25ms** | ~30ms |
| Reactive updates | 0.10ms | 0.10ms | 0.10ms |
| SEO / crawlers | Full | None | Full |
| Screen readers | Native | Via hidden DOM | Via hidden DOM |
| Text selection | Native | Via hidden DOM | Via hidden DOM |
| Cmd+F search | Native | Via hidden DOM | Via hidden DOM |
| Form autofill | Native | Input overlay | Native |
| Tab navigation | Native | WASM-driven | WASM-driven |
| Bundle size | 48 KB | 165 KB | 165 KB |
| DOM nodes | All visible | 1 (`<canvas>`) | All hidden |
| Layout engine | Browser CSS | Honeycomb (WASM) | Browser CSS |
| Paint engine | Browser | Canvas 2D | Canvas 2D |

## When to Use Each Mode

### DOM Mode (default)

Use for any page where SEO, accessibility, or standard browser behavior matters. This is the default and covers most use cases.

- Marketing pages and landing pages
- Blog and content pages
- Product pages that need Google indexing
- Any page with forms that need autofill
- Pages that must meet WCAG accessibility standards

### Canvas Mode

Use for performance-critical pages behind authentication where SEO doesn't matter.

- Admin dashboards with real-time data
- Data visualization and charting
- Internal tools and back-office applications
- Pages with 1000+ interactive elements
- Games and interactive media

Canvas mode builds a hidden accessibility DOM via `requestIdleCallback` after first paint, so screen readers still work. Cmd+F searches the hidden DOM and scrolls the canvas to match.

### Hybrid Mode

Use when you need both rendering speed and full SEO/accessibility.

- E-commerce product catalogs
- Search results pages
- Directory listings
- Any page with large lists that must be crawlable

Hybrid mode uses the browser's CSS engine for layout computation (via a hidden DOM), then paints the visual layer to canvas. The hidden DOM stays live for crawlers, screen readers, and browser search.

## Layout System

### DOM Mode — CSS

DOM mode uses standard CSS. Nectar's style system provides:

- **Scoped styles**: Class names automatically prefixed with component name (`ProductCard__title`)
- **Themes**: CSS custom properties via `theme { }` blocks
- **Breakpoints**: Named responsive ranges via `breakpoints { }` blocks
- **Critical CSS**: Automatic extraction for SSR (`--critical-css`)

```nectar
component ProductCard {
    styles {
        .card {
            display: grid;
            gap: 16px;
            border-radius: 12px;
        }
        .title {
            font-size: 1.2rem;
            font-weight: bold;
        }
    }

    render {
        <div class="card">
            <h3 class="title">{self.name}</h3>
        </div>
    }
}
```

### Canvas/Hybrid Mode — Stacks

Canvas and hybrid modes use Honeycomb's stack-based layout engine. Three layout directions, three sizing policies:

**Directions:**
- `Vertical` — children stack top to bottom (like `flex-direction: column`)
- `Horizontal` — children stack left to right (like `flex-direction: row`)
- `Layer` — children overlap on z-axis (like `position: absolute`)

**Sizing:**
- `Fill(weight)` — take available space, split with siblings (like `flex: 1`)
- `Hug` — shrink to fit content (like `width: fit-content`)
- `Fixed(px)` — exact pixel size (like `width: 260px`)

**Properties:**
- `gap` — space between children
- `pad` — space inside element (padding)
- `align` — cross-axis alignment (Start, Center, End, Stretch)
- `justify` — main-axis distribution (Start, Center, End, SpaceBetween)
- `wrap` — wrap children to next line
- `scroll` — enable scroll container
- `min_width`, `max_width`, `min_height`, `max_height` — constraints

**CSS compatibility:** The layout engine accepts CSS property names directly. `flex-direction: row` maps to `direction: horizontal`. `flex: 1` maps to `Fill(1.0)`. Existing CSS-style properties work without changes.

```nectar
// These are equivalent:
<div style="display:flex;flex-direction:row;gap:16px;padding:40px;flex-wrap:wrap">
<stack direction="horizontal" gap={16} pad={40} wrap={true}>
```

### Layout Algorithm

The layout engine uses a two-pass algorithm:

1. **Measure pass (bottom-up):** Compute the intrinsic size each element wants if given infinite space. `Fill` elements contribute 0 on their fill axis. Text nodes are measured via the platform's text measurer.

2. **Layout pass (top-down):** Given available space from the parent, compute final position and size. Distribute `Fill` weights proportionally. Align and justify children. Recursively layout children with constrained space.

This is the same layout algorithm used by Honeycomb, Nectar's canvas rendering engine. The same Rust code compiles to:
- WASM (for browser canvas/hybrid mode)
- Native binary (planned — for desktop apps via Pollen runtime and wgpu)

## Style Reuse Across Modes

Themes, breakpoints, and component styles are portable:

| Feature | DOM | Canvas/Hybrid |
|---|---|---|
| `theme { accent: "#f97316" }` | CSS custom property `var(--accent)` | Resolved at compile time |
| `breakpoints { tablet: 768 }` | `@media (min-width: 768px)` | `canvas_get_width() >= 768` |
| Scoped `.title` class | `ProductCard__title` CSS class | Direct style lookup by component + class |
| `style="color: red"` | Inline CSS | `resolve_style()` parses at layout time |

The developer writes styles once. The compiler translates to the appropriate target.

## Architecture

```
.nectar source
     |
  Compiler (--render flag)
     |
  ┌──┴──────────────┬──────────────────┐
  DOM               Canvas             Hybrid
  |                 |                  |
  dom_createElement  cvs_add            dom_createElement (hidden)
  dom_setAttr        canvas_fillRect    + canvas_fillRect (visible)
  dom_setText        canvas_fillText    + dom_get_rect (sync)
  |                 |                  |
  Browser CSS       Honeycomb (WASM)    Browser CSS (layout)
  Browser paint     Canvas 2D (paint)   Canvas 2D (paint)
```

All three modes share:
- Same `.nectar` source files
- Same component model (signals, props, methods)
- Same reactive system (O(1) updates per binding)
- Same event handling (on:click, on:input)
- Same template syntax ({for ...}, {if ...}, {lazy for ...})

Only the rendering backend changes.

## Accessibility

### DOM Mode
Fully accessible out of the box. Browser handles focus management, screen reader announcements, keyboard navigation.

### Canvas Mode
WASM builds a hidden accessibility DOM (`opacity: 0; pointer-events: none`) after first paint via `requestIdleCallback`. This DOM is:
- Invisible to the user (canvas covers it)
- Visible to screen readers (opacity:0 preserves accessibility tree)
- Searchable by Cmd+F (browser searches real DOM text)
- Indexed by crawlers (real HTML elements)

### Hybrid Mode
The hidden DOM used for layout computation doubles as the accessibility layer. No extra work needed — the elements already exist.

## Vim Mode (Canvas/Hybrid)

Canvas and hybrid modes include an optional vim-style keyboard navigation:

- `Ctrl+Shift+V` — toggle vim mode on/off
- `h/j/k/l` — scroll (left/down/up/right)
- `g` — jump to top
- `G` — jump to bottom
- `Esc` — exit vim mode

When vim mode is off, all keys type normally. A green "VIM MODE" badge appears when active.
