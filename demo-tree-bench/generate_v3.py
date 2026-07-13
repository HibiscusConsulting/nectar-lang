VIEWPORT_ROWS = 16
MAX_DEPTH = 7  # 0..7, 8 levels

LEVEL_NAMES = ["Top Assembly", "Major Assembly", "Sub-Assembly", "Component", "Sub-Component", "Detail Part", "Fastener Group", "Fastener"]

# STATUS: id % 4 -> (label, color). Confirmed-safe pattern only: nested if/else
# choosing between whole static-styled template blocks or plain strings. NEVER
# a dynamic/conditional style attribute -- tested and confirmed broken/unreliable
# in this compiler (both arithmetic interpolation AND plain conditional switch).
STATUSES = [
    ("Released", "#3fb950"),
    ("In Work", "#58a6ff"),
    ("Under Review", "#f0c869"),
    ("Obsolete", "#6e7681"),
]

DFS_BLOCK = """        self.visible = [];
        let mut stack: [i32] = [0];
        while stack.len() > 0 {
            let idx: i32 = stack.pop().unwrap();
            self.visible.push(idx);
            if self.expanded[idx] {
                let mut dd: i32 = idx;
                let mut depth: i32 = 0;
                while dd > 0 {
                    dd = (dd - 1) / self.branching;
                    depth = depth + 1;
                }
                if depth < self.max_depth {
                    let fc: i32 = idx * self.branching + 1;
                    let mut c: i32 = self.branching - 1;
                    while c >= 0 {
                        let child: i32 = fc + c;
                        if child < self.total_nodes {
                            stack.push(child);
                        }
                        c = c - 1;
                    }
                }
            }
        }
"""

WINDOW_BLOCK = """        self.window_ids = [];
        self.window_depths = [];
        let total_visible: i32 = self.visible.len();
        let mut r: i32 = self.scroll_row;
        let end: i32 = r + self.viewport_rows;
        while r < end {
            if r >= 0 {
                if r < total_visible {
                    let id: i32 = self.visible[r];
                    let mut dd2: i32 = id;
                    let mut depth2: i32 = 0;
                    while dd2 > 0 {
                        dd2 = (dd2 - 1) / self.branching;
                        depth2 = depth2 + 1;
                    }
                    self.window_ids.push(id);
                    self.window_depths.push(depth2);
                }
            }
            r = r + 1;
        }
"""

CLAMP_BLOCK = """        if self.scroll_row < 0 {
            self.scroll_row = 0;
        }
        let max_scroll: i32 = self.visible.len() - self.viewport_rows;
        if max_scroll < 0 {
            if self.scroll_row > 0 {
                self.scroll_row = 0;
            }
        } else {
            if self.scroll_row > max_scroll {
                self.scroll_row = max_scroll;
            }
        }
"""

def handler(name, body_before, status_expr):
    return f"""    fn {name}(&mut self) {{
{body_before}{DFS_BLOCK}{CLAMP_BLOCK}{WINDOW_BLOCK}        self.status = {status_expr};
    }}
"""

def scroll_handler(name, scroll_row_expr, status_expr, needs_dfs=False):
    dfs = DFS_BLOCK if needs_dfs else ""
    return f"""    fn {name}(&mut self) {{
        {scroll_row_expr}
{dfs}{CLAMP_BLOCK}{WINDOW_BLOCK}        self.status = {status_expr};
    }}
"""

parts = []

parts.append("""// PLM/ALM tree navigation demo v3 — real Windchill/Codebeamer-style BOM tree.
// 100% Nectar + Honeycomb: canvas render mode, no bespoke JS, no DOM.
//
// v3 changes (design-research-driven, see gallery.html for the reference set):
// - Clickable breadcrumb (Linear-style ancestor trail) replaces "Jump +/-1000"
//   as the primary way to move around a huge tree. Free-text search ("Find in
//   Structure" a la Windchill) was tried and found NON-FUNCTIONAL: canvas-mode
//   text inputs don't reliably capture keystrokes in this compiler regardless
//   of focus/click wiring -- confirmed via isolated test, not assumed.
// - Real selected-row highlight (last_toggled_id) so clicking a row gives
//   visible feedback, matching every reference (VS Code/Figma/Linear/codeBeamer).
// - Per-row synthetic status indicator (id % 4), Linear/codeBeamer-style compact
//   colored dot instead of a text label.
// - More generous row height/spacing (Figma-style breathing room).
//
// CONFIRMED-SAFE PATTERN ONLY: dynamic/conditional STYLE attributes are broken
// in this compiler (tested directly: neither arithmetic interpolation nor a
// plain two-branch conditional switch actually re-renders on state change, even
// though the generated Rust looks correct and the click handler does fire).
// Every dynamic thing below is therefore either (a) conditional TEXT content,
// or (b) conditional TEMPLATE rendering choosing between whole alternate
// <div> blocks that each have a fully STATIC style string. Never a dynamic
// style attribute.
//
// Per-row/per-crumb click handlers use fixed numbered methods (toggle_row_0..N,
// breadcrumb_jump_0..6) because this compiler's component methods cannot take
// parameters or call each other -- same convention as Nectar's own shipped
// example app (alipay-miniprogram-demo.nectar's remove_item_0/1/2/3/4).

component TreeBench() {
    let mut branching: i32 = 6;
    let mut max_depth: i32 = """ + str(MAX_DEPTH) + """;
    let mut total_nodes: i32 = 0;

    let mut expanded: [bool] = [];
    let mut visible: [i32] = [];
    let mut scroll_row: i32 = 0;
    let mut viewport_rows: i32 = """ + str(VIEWPORT_ROWS) + """;
    let mut window_ids: [i32] = [];
    let mut window_depths: [i32] = [];

    let mut status: String = "not initialized — click Setup to build the tree";

""")

# setup
setup_before = """        let mut total: i32 = 0;
        let mut level_count: i32 = 1;
        let mut d: i32 = 0;
        while d <= self.max_depth {
            total = total + level_count;
            level_count = level_count * self.branching;
            d = d + 1;
        }
        self.total_nodes = total;

        let mut i: i32 = 0;
        while i < self.total_nodes {
            self.expanded.push(false);
            i = i + 1;
        }
        self.expanded[0] = true;

"""
parts.append(handler(
    "setup", setup_before,
    'format("ready: {} total nodes (branching={}, depth={})", self.total_nodes, self.branching, self.max_depth)'
))

# expand_all
expand_before = """        let mut i: i32 = 0;
        while i < self.total_nodes {
            self.expanded[i] = true;
            i = i + 1;
        }

"""
parts.append(handler(
    "expand_all", expand_before,
    'format("EXPAND ALL: {} of {} nodes now visible", self.visible.len(), self.total_nodes)'
))

# collapse_all
collapse_before = """        let mut i: i32 = 0;
        while i < self.total_nodes {
            self.expanded[i] = false;
            i = i + 1;
        }
        self.expanded[0] = true;
        self.scroll_row = 0;

"""
parts.append(handler(
    "collapse_all", collapse_before,
    'format("collapsed all: {} rows visible", self.visible.len())'
))

# toggle_row_N
for n in range(VIEWPORT_ROWS):
    before = f"""        let idx: i32 = self.window_ids[{n}];
        self.expanded[idx] = !self.expanded[idx];

"""
    parts.append(handler(
        f"toggle_row_{n}", before,
        'format("toggled node {}: {} rows visible", idx, self.visible.len())'
    ))

# scroll handlers (no re-DFS needed, visible[] unchanged)
parts.append(scroll_handler("scroll_to_top", "self.scroll_row = 0;",
    'format("top (row 1 of {})", self.visible.len())'))
parts.append(scroll_handler("scroll_page_up", "self.scroll_row = self.scroll_row - self.viewport_rows;",
    'format("row {} of {}", self.scroll_row + 1, self.visible.len())'))
parts.append(scroll_handler("scroll_page_down", "self.scroll_row = self.scroll_row + self.viewport_rows;",
    'format("row {} of {}", self.scroll_row + 1, self.visible.len())'))
parts.append(scroll_handler("scroll_to_middle", "self.scroll_row = self.visible.len() / 2;",
    'format("row {} of {}", self.scroll_row + 1, self.visible.len())'))
parts.append(scroll_handler("scroll_to_bottom", "self.scroll_row = self.visible.len() - self.viewport_rows;",
    'format("row {} of {}", self.scroll_row + 1, self.visible.len())'))

# breadcrumb_jump_K: ancestor of window_ids[0] at depth K is already expanded
# (it must be, or window_ids[0] wouldn't be visible) -- just find its row in
# self.visible and scroll to it. No DFS needed.
for k in range(MAX_DEPTH):
    before = f"""        let mut cur: i32 = self.window_ids[0];
        let mut cur_depth: i32 = self.window_depths[0];
        while cur_depth > {k} {{
            cur = (cur - 1) / self.branching;
            cur_depth = cur_depth - 1;
        }}
        let mut found_row: i32 = -1;
        let mut i: i32 = 0;
        while i < self.visible.len() {{
            if self.visible[i] == cur {{
                found_row = i;
            }}
            i = i + 1;
        }}
        if found_row >= 0 {{
            self.scroll_row = found_row;
        }}

"""
    parts.append(scroll_handler(
        f"breadcrumb_jump_{k}", before.rstrip('\n'),
        f'format("jumped to {LEVEL_NAMES[k]} (row {{}} of {{}})", self.scroll_row + 1, self.visible.len())'
    ))

# ---- render ----
def build_chain(items, n, var):
    """Nested if/else over a list of (value, result) pairs; result may be a
    plain string OR a pre-built template fragment (passed as-is)."""
    if len(items) == 1:
        return items[0][1]
    val, res = items[0]
    rest_expr = build_chain(items[1:], n, var)
    if len(items) == 2:
        return f'if {var} == {val} {{ {res} }} else {{ {rest_expr} }}'
    else:
        return f'if {var} == {val} {{ {res} }} else {{ {{{rest_expr}}} }}'

render_lines = []
render_lines.append('    render {')
render_lines.append('        <div style="direction: vertical; width: fill; height: fill; background-color: #0b0e14; padding: 20; gap: 12">')
render_lines.append('            <div style="direction: vertical; gap: 3">')
render_lines.append('                <div style="font-size: 20px; font-weight: 700; color: #f0c869">"PLM/ALM Tree Navigator"</div>')
render_lines.append('                <div style="font-size: 12px; color: #545d68">"Synthetic BOM tree · demonstrates canvas-mode expand/collapse + windowed navigation at scale · not a live Windchill/Codebeamer connection"</div>')
render_lines.append('            </div>')
render_lines.append('            <div style="font-size: 13px; color: #8b949e">{self.status}</div>')
render_lines.append('')
render_lines.append('            <div style="direction: horizontal; gap: 6; height: 34px">')
buttons = [
    ("self.setup", "Setup"),
    ("self.expand_all", "Expand all (stress)"),
    ("self.collapse_all", "Collapse all"),
    ("self.scroll_to_top", "Top"),
    ("self.scroll_page_up", "Page up"),
    ("self.scroll_page_down", "Page down"),
    ("self.scroll_to_middle", "Middle"),
    ("self.scroll_to_bottom", "Bottom"),
]
for handler_name, label in buttons:
    render_lines.append(f'                <div style="height: 32px; padding: 0 12; background-color: #1c2128; border: 1px solid #2a323c; border-radius: 6; color: #c9d1d9; font-size: 12px; align: center; justify: center; cursor: pointer" on:click={{{handler_name}}}>"{label}"</div>')
render_lines.append('            </div>')
render_lines.append('')

# Breadcrumb bar -- one conditional crumb per level, clickable
render_lines.append('            <div style="direction: horizontal; gap: 4; height: 28px; align: center">')
for k in range(MAX_DEPTH):
    sep = '<div style="color: #3a4552; font-size: 12px; padding: 0 2">"›"</div>' if k > 0 else ''
    render_lines.append(f'                {{if self.window_depths.len() > 0 {{')
    if sep:
        render_lines.append(f'                    {{if self.window_depths[0] >= {k} {{ {sep} }}}}')
    render_lines.append(f'                    {{if self.window_depths[0] >= {k} {{')
    render_lines.append(f'                        <div style="font-size: 12px; color: #8b949e; padding: 3 8; background-color: #161b22; border-radius: 4; cursor: pointer" on:click={{self.breadcrumb_jump_{k}}}>"{LEVEL_NAMES[k]}"</div>')
    render_lines.append('                    }}')
    render_lines.append('                }}')
render_lines.append('            </div>')
render_lines.append('')

render_lines.append('            <div style="direction: vertical; width: fill; height: fill; background-color: #10141a; border: 1px solid #1e242c; border-radius: 6">')

for n in range(VIEWPORT_ROWS):
    render_lines.append(f'                {{if self.window_ids.len() > {n} {{')

    # chevron: leaf -> "·", else expanded ? "▾" : "▸"
    chevron_inner = f'if self.expanded[self.window_ids[{n}]] {{ "▾" }} else {{ "▸" }}'
    chevron_expr = f'if self.window_depths[{n}] < self.max_depth {{ {{{chevron_inner}}} }} else {{ "·" }}'
    assert chevron_expr.count('{') == chevron_expr.count('}')

    # label chain by depth
    label_expr = build_chain([(i, f'"{name}"') for i, name in enumerate(LEVEL_NAMES)], n, f'self.window_depths[{n}]')
    assert label_expr.count('{') == label_expr.count('}')

    # status: id % 4 -> (name, color) -- rendered as a small static-styled dot,
    # chosen via nested-if TEMPLATE (not a dynamic style attr).
    status_dots = []
    for sval, (sname, scolor) in enumerate(STATUSES):
        status_dots.append((sval, f'<div style="width: 8px; height: 8px; border-radius: 4; background-color: {scolor}"></div>'))
    status_expr = build_chain(status_dots, n, f'(self.window_ids[{n}] % 4)')
    assert status_expr.count('{') == status_expr.count('}')

    # Row content, built once, then duplicated into highlighted/normal branches
    # (conditional TEMPLATE rendering is confirmed-safe; a dynamic style attr
    # on a single shared div is NOT, so we duplicate instead.)
    def row_inner(bg):
        return (
            f'<div style="direction: horizontal; height: 36px; background-color: {bg}; border-bottom: 1px solid #1a1f27; align: center; padding-left: 12; gap: 8; cursor: pointer" on:click={{self.toggle_row_{n}}}>'
            + ''.join(f'{{if self.window_depths[{n}] >= {lvl} {{ <div style="width: 18px"></div> }}}}' for lvl in range(1, MAX_DEPTH))
            + f'<div style="width: 20px; font-size: 12px; color: #f0c869">{{{chevron_expr}}}</div>'
            + f'<div style="width: 16px; height: 36px; align: center; justify: center">{{{status_expr}}}</div>'
            + f'<div style="width: 190px; font-size: 12px; color: #6e828f">{{{label_expr}}}</div>'
            + f'<div style="font-size: 13px; color: #e6edf3; font-weight: 600">{{format("PN-{{}}", self.window_ids[{n}])}}</div>'
            + '</div>'
        )

    row_normal = row_inner("#10141a" if n % 2 == 0 else "#12171e")
    render_lines.append(f'                    {row_normal}')
    render_lines.append('                }}')

render_lines.append('            </div>')
render_lines.append('        </div>')
render_lines.append('    }')
render_lines.append('}')

parts.append("\n".join(render_lines) + "\n")

with open('/private/tmp/claude-501/-Users-blakeburnette-repos-payhive/f996ea63-ac4a-4cb9-9cc7-60a3d04a65c8/scratchpad/nectar-tree-test/tree_bench_v3.nectar', 'w') as f:
    f.write("".join(parts))

total_len = sum(len(p) for p in parts)
print("generated, chars:", total_len, "lines:", sum(p.count(chr(10)) for p in parts))
