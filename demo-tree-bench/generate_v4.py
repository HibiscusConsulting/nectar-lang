VIEWPORT_ROWS = 22
MAX_DEPTH = 7  # 0..7, 8 levels
BRANCHING = 6  # id-space arity (API may return fewer children per node)

LEVEL_NAMES = ["Top Assembly", "Major Assembly", "Sub-Assembly", "Component", "Sub-Component", "Detail Part", "Fastener Group", "Fastener"]

STATUSES = [
    ("Released", "#1a7f37"),
    ("In Work", "#0969da"),
    ("Under Review", "#9a6700"),
    ("Obsolete", "#6e7781"),
]

# DFS over the lazily-loaded structure: descend only into nodes that are BOTH
# expanded AND loaded, using the child count the API reported (4-6 per node,
# unknowable client-side). Child ids live in the 6-ary arithmetic id space.
DFS_BLOCK = """        self.visible = [];
        let mut stack: [i32] = [0];
        while stack.len() > 0 {
            let idx: i32 = stack.pop().unwrap();
            self.visible.push(idx);
            if self.expanded[idx] {
                if self.loaded[idx] {
                    let cc: i32 = self.children_count[idx];
                    let fc: i32 = idx * self.branching + 1;
                    let mut c: i32 = cc - 1;
                    while c >= 0 {
                        stack.push(fc + c);
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

SELECT_DERIVE_BLOCK = """        let mut sd: i32 = self.selected_id;
        let mut sdepth: i32 = 0;
        while sd > 0 {
            sd = (sd - 1) / self.branching;
            sdepth = sdepth + 1;
        }
        self.selected_depth = sdepth;
        self.uses_ids = [];
        if self.loaded[self.selected_id] {
            let cc: i32 = self.children_count[self.selected_id];
            let fc: i32 = self.selected_id * self.branching + 1;
            let mut c: i32 = 0;
            while c < cc {
                self.uses_ids.push(fc + c);
                c = c + 1;
            }
        }
"""

def handler(name, body_before, status_expr):
    return f"""    fn {name}(&mut self) {{
        if self.capacity == 0 {{
            self.status = "not connected — click Connect first";
        }} else {{
{body_before}{DFS_BLOCK}{CLAMP_BLOCK}{WINDOW_BLOCK}            self.status = {status_expr};
        }}
    }}
"""

def scroll_handler(name, scroll_row_expr, status_expr):
    return f"""    fn {name}(&mut self) {{
        if self.capacity == 0 {{
            self.status = "not connected — click Connect first";
        }} else {{
            {scroll_row_expr}
{CLAMP_BLOCK}{WINDOW_BLOCK}            self.status = {status_expr};
        }}
    }}
"""

parts = []

parts.append("""// PLM/ALM tree navigator v4 — three-pane Windchill-style workbench over a
// REAL (mock) BOM REST API. 100% Nectar + Honeycomb canvas mode; the only
// JS involved is the compiler's own syscall glue.
//
// DATA GENUINELY COMES FROM THE API: each node's children (count 4-6 AND
// their names) are decided by the server (mock_bom_server.py, simulating a
// Windchill/Codebeamer REST endpoint with ~120ms latency). The client cannot
// know a node's children without GET /api/bom/children?node=N. Chevron-click
// on an unloaded assembly fires the fetch, shows a loading state, and the
// on_response callback parses the JSON, registers the children, expands the
// node, and re-renders. This is the exact lazy-load pattern a real PLM
// frontend uses -- nobody downloads a 300K-node BOM upfront.
//
// Windchill-reference layout: left dense STRUCTURE tree (24px rows, single
// identity string per row), right ATTRIBUTES pane for the selected part,
// bottom USES table (direct children, real columns). Chevron = expand/
// lazy-load; row body = select.
//
// CONFIRMED-SAFE compiler patterns only (each proven in isolation this
// session): conditional TEXT; len()-guarded conditional blocks; nested-if
// chains picking static-styled templates; format() text; numbered per-row
// handlers; mp::request dynamic-URL fetch + on_response JSON parse into the
// first [struct] state field (REQUIRES the freshly-built compiler -- the
// March binary predates the mp:: mapping entirely).
// BROKEN, do not use: dynamic/conditional STYLE attribute values; free-text
// <input> keystrokes; equality-keyed dual-branch row-highlight templates.

struct BomNode {
    id: i32,
    name: String,
    leaf: i32,
    state: i32,
    qty: i32,
}

component TreeBench {
    let mut branching: i32 = """ + str(BRANCHING) + """;
    let mut max_depth: i32 = """ + str(MAX_DEPTH) + """;
    let mut capacity: i32 = 0;

    let mut fetched: [BomNode] = [];
    let mut names: [String] = [];
    let mut hay: [String] = [];
    let mut eff_state: [i32] = [];
    let mut qty: [i32] = [];
    let mut loaded: [bool] = [];
    let mut children_count: [i32] = [];
    let mut expanded: [bool] = [];
    let mut pending_parent: i32 = -1;
    let mut api_calls: i32 = 0;
    let mut loaded_count: i32 = 0;
    let mut view_name: String = "iso";
    let mut view_rot: i32 = 0;
    let mut view_zoom: i32 = 1;
    let mut filter_label: String = "full structure";
    let mut query: String = "";
    let mut last_url: String = "—";

    let mut visible: [i32] = [];
    let mut scroll_row: i32 = 0;
    let mut viewport_rows: i32 = """ + str(VIEWPORT_ROWS) + """;
    let mut window_ids: [i32] = [];
    let mut window_depths: [i32] = [];

    let mut selected_id: i32 = 0;
    let mut selected_depth: i32 = 0;
    let mut uses_ids: [i32] = [];

    let mut status: String = "not connected — click Connect to load the root assembly from the BOM API";

""")

# --- on_response: MUST be the first non-init method (callback index 0). ---
# The compiler parses the fetch body into `fetched` (first [struct] field)
# before invoking this.
on_response_body = """        let t0: f64 = performance_now();
        let parent: i32 = self.pending_parent;
        let count: i32 = self.fetched.len();
        self.api_calls = self.api_calls + 1;
        let mut is_ack: i32 = 0;
        let mut ack_id: i32 = 0;
        let mut st_label: String = format("");
        if parent == 0 - 3 {
            if count > 0 {
                ack_id = self.fetched[0].id;
                self.eff_state[ack_id] = self.fetched[0].state;
                self.qty[ack_id] = self.fetched[0].qty;
                is_ack = 1;
                if self.eff_state[ack_id] == 0 { st_label = format("Released"); }
                if self.eff_state[ack_id] == 1 { st_label = format("In Work"); }
                if self.eff_state[ack_id] == 2 { st_label = format("Under Review"); }
                if self.eff_state[ack_id] == 3 { st_label = format("Obsolete"); }
            }
        } else {
        if parent == 0 - 2 {
            let mut i: i32 = 0;
            while i < self.capacity {
                self.children_count[i] = 0;
                i = i + 1;
            }
            i = 0;
            while i < count {
                let node_id: i32 = self.fetched[i].id;
                self.names[node_id] = format("{}", self.fetched[i].name);
                self.hay[node_id] = format("pn-{} {}", node_id, self.fetched[i].name.to_lowercase());
                self.eff_state[node_id] = self.fetched[i].state;
                self.qty[node_id] = self.fetched[i].qty;
                self.loaded[node_id] = true;
                self.expanded[node_id] = true;
                if node_id > 0 {
                    let p: i32 = (node_id - 1) / self.branching;
                    self.children_count[p] = self.children_count[p] + 1;
                }
                i = i + 1;
            }
            self.loaded_count = count;
        } else {
            self.children_count[parent] = count;
            self.loaded[parent] = true;
            self.expanded[parent] = true;
            self.loaded_count = self.loaded_count + count;
            let mut i: i32 = 0;
            while i < count {
                let child_id: i32 = self.fetched[i].id;
                self.names[child_id] = format("{}", self.fetched[i].name);
                self.hay[child_id] = format("pn-{} {}", child_id, self.fetched[i].name.to_lowercase());
                self.eff_state[child_id] = self.fetched[i].state;
                self.qty[child_id] = self.fetched[i].qty;
                i = i + 1;
            }
        }
        }
        let merge_ms: f64 = performance_now() - t0;
""" + SELECT_DERIVE_BLOCK + """        let mut status_msg: String = format("API: {} nodes merged in {:.1}ms — {} rows visible, {} calls total", count, merge_ms, self.visible.len(), self.api_calls);
        if is_ack == 1 {
            status_msg = format("server committed update: PN-{} → state {}, qty {} (ack merged in {:.1}ms)", ack_id, st_label, self.qty[ack_id], merge_ms);
        }
"""
parts.append(handler(
    "on_response", on_response_body,
    'format("{}", status_msg)'
))

# --- init_connect: LANGUAGE LIFECYCLE HOOK. Any method named init* runs
# automatically inside nectar_init() before first mount (and is excluded from
# callback numbering, so on_response stays cb 0). Allocates the id-space
# arrays and fires the bulk BOM fetch — the app connects itself on page load,
# no button click needed. Connect/Import buttons stay as guarded no-ops.
parts.append("""    fn init_connect(&mut self) {
        let mut total: i32 = 0;
        let mut level_count: i32 = 1;
        let mut d: i32 = 0;
        while d <= self.max_depth {
            total = total + level_count;
            level_count = level_count * self.branching;
            d = d + 1;
        }
        self.capacity = total;
        let mut i: i32 = 0;
        while i < self.capacity {
            self.expanded.push(false);
            self.loaded.push(false);
            self.children_count.push(0);
            self.names.push(format(""));
            self.hay.push(format(""));
            self.eff_state.push(0);
            self.qty.push(1);
            i = i + 1;
        }
        self.names[0] = "Turbofan Engine HBT-9000";
        self.hay[0] = format("pn-0 turbofan engine hbt-9000");
        self.loaded_count = 1;
        self.selected_id = 0;
        self.pending_parent = 0 - 2;
        self.status = "auto-loading full BOM: GET /api/bom/tree … (71,284 nodes, ~4MB JSON)";
        self.last_url = "GET /api/bom/tree";
        mp::request(format("/api/bom/tree{}", ""), "GET", "", 0);
    }
""")

# --- setup: allocate id-space arrays, fetch root children from the API ---
parts.append("""    fn setup(&mut self) {
        if self.capacity > 0 {
            self.status = format("already connected ({} API calls so far)", self.api_calls);
            return;
        }
        let mut total: i32 = 0;
        let mut level_count: i32 = 1;
        let mut d: i32 = 0;
        while d <= self.max_depth {
            total = total + level_count;
            level_count = level_count * self.branching;
            d = d + 1;
        }
        self.capacity = total;

        let mut i: i32 = 0;
        while i < self.capacity {
            self.expanded.push(false);
            self.loaded.push(false);
            self.children_count.push(0);
            self.names.push(format(""));
            self.hay.push(format(""));
            self.eff_state.push(0);
            self.qty.push(1);
            i = i + 1;
        }
        self.names[0] = "Turbofan Engine HBT-9000";
        self.hay[0] = format("pn-0 turbofan engine hbt-9000");
        self.loaded_count = 1;
        self.selected_id = 0;
        self.pending_parent = 0;
        self.status = "GET /api/bom/children?node=0 …";
        self.last_url = "GET /api/bom/children?node=0";
        mp::request(format("/api/bom/children?node={}", 0), "GET", "", 0);
    }
""")

# --- import_all: one bulk call for the ENTIRE ~100K-node BOM ---
parts.append("""    fn import_all(&mut self) {
        if self.capacity == 0 {
            self.status = "not connected — click Connect first";
            return;
        }
        self.pending_parent = 0 - 2;
        self.status = "GET /api/bom/tree … (full multi-level BOM, ~100K nodes, ~4MB JSON)";
        self.last_url = "GET /api/bom/tree";
        mp::request(format("/api/bom/tree{}", ""), "GET", "", 0);
    }
""")

# --- expand loaded / collapse all ---
expand_before = """        let mut i: i32 = 0;
        while i < self.capacity {
            if self.loaded[i] {
                self.expanded[i] = true;
            }
            i = i + 1;
        }

"""
parts.append(handler(
    "expand_loaded", expand_before,
    'format("expanded all {} loaded nodes: {} rows visible (drill or Import full BOM to load more)", self.loaded_count, self.visible.len())'
))

collapse_before = """        let mut i: i32 = 0;
        while i < self.capacity {
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

# --- toggle_row_N: chevron click. Loaded -> instant local toggle. Unloaded
#     assembly -> fire the API call and wait for on_response. ---
for n in range(VIEWPORT_ROWS):
    parts.append(f"""    fn toggle_row_{n}(&mut self) {{
        if self.window_ids.len() <= {n} {{
            self.status = "not connected — click Connect first";
            return;
        }}
        let idx: i32 = self.window_ids[{n}];
        let mut dd: i32 = idx;
        let mut depth: i32 = 0;
        while dd > 0 {{
            dd = (dd - 1) / self.branching;
            depth = depth + 1;
        }}
        if depth >= self.max_depth {{
            self.status = format("PN-{{}} is a leaf part — nothing to expand", idx);
        }} else {{
            if self.loaded[idx] {{
                self.expanded[idx] = !self.expanded[idx];
{DFS_BLOCK}{CLAMP_BLOCK}{WINDOW_BLOCK}                self.status = format("toggled PN-{{}} (local, no API call): {{}} rows visible", idx, self.visible.len());
            }} else {{
                self.pending_parent = idx;
                self.status = format("GET /api/bom/children?node={{}} …", idx);
                self.last_url = format("GET /api/bom/children?node={{}}", idx);
                mp::request(format("/api/bom/children?node={{}}", idx), "GET", "", 0);
            }}
        }}
    }}
""")

# --- select_row_N: row-body click, drives Attributes pane + Uses table ---
for n in range(VIEWPORT_ROWS):
    parts.append(f"""    fn select_row_{n}(&mut self) {{
        if self.window_ids.len() <= {n} {{
            self.status = "not connected — click Connect first";
            return;
        }}
        self.selected_id = self.window_ids[{n}];
{SELECT_DERIVE_BLOCK}        self.status = format("selected PN-{{}}", self.selected_id);
    }}
""")

# --- filters: full scans over every loaded node, timed. This is the
#     "stupid fast filter" demo — predicate over 71K+ nodes in WASM. A tree
#     interaction (toggle/expand/collapse) returns to the structure view. ---
FILTER_TAIL = CLAMP_BLOCK + WINDOW_BLOCK

def filter_handler(name, label, predicate):
    return f"""    fn {name}(&mut self) {{
        if self.capacity == 0 {{
            self.status = "not connected — click Connect first";
            return;
        }}
        let t0: f64 = performance_now();
        self.visible = [];
        let mut i: i32 = 0;
        while i < self.capacity {{
            if self.names[i].len() > 0 {{
                {predicate}
            }}
            i = i + 1;
        }}
        let scan_ms: f64 = performance_now() - t0;
        self.scroll_row = 0;
        self.filter_label = "{label}";
{FILTER_TAIL}        self.status = format("filter [{label}]: scanned {{}} nodes → {{}} matches in {{:.2}}ms", self.loaded_count, self.visible.len(), scan_ms);
    }}
"""

# live search: fires on EVERY keystroke via the new on:input language feature.
# Fuzzy: case-insensitive, space-separated tokens ALL must match (AND), each
# token checked against a precomputed lowercase "pn-{id} {name}" haystack —
# so "aft bearing", "BEARING", and "PN-12" all find what a user expects.
parts.append("""    fn on_search(&mut self) {
        self.query = input_text();
        if self.capacity == 0 {
            self.status = "not connected — click Connect first";
            return;
        }
        let ql: String = self.query.to_lowercase();
        let mut toks: [String] = [];
        let mut rest: String = format("{}", ql);
        while rest.len() > 0 {
            let sp: i32 = rest.index_of(" ");
            if sp < 0 {
                toks.push(format("{}", rest));
                rest = format("");
            } else {
                if sp > 0 {
                    toks.push(rest.slice(0, sp));
                }
                rest = rest.slice(sp + 1, rest.len());
            }
        }
        let t0: f64 = performance_now();
        if toks.len() == 0 {
""" + DFS_BLOCK + """            self.filter_label = "full structure";
        } else {
            self.visible = [];
            let mut i: i32 = 0;
            while i < self.capacity {
                if self.names[i].len() > 0 {
                    let mut hit: i32 = 0;
                    let mut t: i32 = 0;
                    while t < toks.len() {
                        if self.hay[i].contains(toks[t]) {
                            hit = hit + 1;
                        }
                        t = t + 1;
                    }
                    if hit == toks.len() {
                        self.visible.push(i);
                    }
                }
                i = i + 1;
            }
            self.filter_label = format("search \\"{}\\"", self.query);
        }
        let scan_ms: f64 = performance_now() - t0;
        self.scroll_row = 0;
""" + CLAMP_BLOCK + WINDOW_BLOCK + """        if self.loaded_count < 70000 {
            self.status = format("search \\"{}\\": {} matches in {:.2}ms — searched {} loaded nodes only (Import full BOM to search all 71,284)", self.query, self.visible.len(), scan_ms, self.loaded_count);
        } else {
            self.status = format("search \\"{}\\": scanned {} nodes → {} matches in {:.2}ms", self.query, self.loaded_count, self.visible.len(), scan_ms);
        }
    }
""")

parts.append(filter_handler("filter_released", "state = Released", "if self.eff_state[i] == 0 { self.visible.push(i); }"))
parts.append(filter_handler("filter_inwork", "state = In Work", "if self.eff_state[i] == 1 { self.visible.push(i); }"))
parts.append(filter_handler("filter_review", "state = Under Review", "if self.eff_state[i] == 2 { self.visible.push(i); }"))
parts.append(filter_handler("filter_obsolete", "state = Obsolete", "if self.eff_state[i] == 3 { self.visible.push(i); }"))

# --- WRITE PATH: edit lifecycle state / quantity on the selected part.  ---
# POSTs to the mock API; the server commits and responds with the updated
# record (same BomNode shape as every read), which on_response merges back
# via the pending_parent == -3 ack branch. Numbered handlers, one per state.
for sval, sname in [(0, "Released"), (1, "In Work"), (2, "Under Review"), (3, "Obsolete")]:
    parts.append("""    fn set_state_""" + str(sval) + """(&mut self) {
        if self.capacity == 0 {
            self.status = "not connected yet";
            return;
        }
        let sid: i32 = self.selected_id;
        self.pending_parent = 0 - 3;
        self.status = format("POST /api/part/update {{\\"id\\":{},\\"state\\":""" + str(sval) + """}} … (set """ + sname + """)", sid);
        self.last_url = format("POST /api/part/update (PN-{})", sid);
        mp::request(format("/api/part/update{}", ""), "POST", format("{{\\"id\\":{},\\"state\\":""" + str(sval) + """}}", sid), 0);
    }
""")

for hname, delta in [("qty_inc", 1), ("qty_dec", -1)]:
    parts.append("""    fn """ + hname + """(&mut self) {
        if self.capacity == 0 {
            self.status = "not connected yet";
            return;
        }
        let sid: i32 = self.selected_id;
        let mut q: i32 = self.qty[sid] + """ + str(delta) + """;
        if q < 1 {
            q = 1;
        }
        self.pending_parent = 0 - 3;
        self.status = format("POST /api/part/update {{\\"id\\":{},\\"qty\\":{}}} …", sid, q);
        self.last_url = format("POST /api/part/update (PN-{})", sid);
        mp::request(format("/api/part/update{}", ""), "POST", format("{{\\"id\\":{},\\"qty\\":{}}}", sid, q), 0);
    }
""")
parts.append(filter_handler("filter_fasteners", "type = Fastener (leaf parts)", """let mut dd: i32 = i;
                let mut depth: i32 = 0;
                while dd > 0 {
                    dd = (dd - 1) / self.branching;
                    depth = depth + 1;
                }
                if depth >= self.max_depth { self.visible.push(i); }"""))
parts.append(filter_handler("filter_assemblies", "type = assemblies only", """let mut dd: i32 = i;
                let mut depth: i32 = 0;
                while dd > 0 {
                    dd = (dd - 1) / self.branching;
                    depth = depth + 1;
                }
                if depth < self.max_depth { self.visible.push(i); }"""))

# "All" = back to the tree structure view
parts.append(handler(
    "filter_all", '        self.filter_label = "full structure";\n        self.scroll_row = 0;\n',
    'format("filter cleared: {} rows (tree view)", self.visible.len())'
))

# --- visualization view controls (server re-renders each view, like Creo View) ---
for name, body, msg in [
    ("view_iso",   'self.view_name = "iso";',   '"visualization: isometric view"'),
    ("view_front", 'self.view_name = "front";', '"visualization: front view"'),
    ("view_top",   'self.view_name = "top";',   '"visualization: top view"'),
    ("rot_left",   "self.view_rot = self.view_rot - 1;\n        if self.view_rot < 0 { self.view_rot = 7; }", 'format("visualization: yaw {}°", self.view_rot * 45)'),
    ("rot_right",  "self.view_rot = self.view_rot + 1;\n        if self.view_rot > 7 { self.view_rot = 0; }", 'format("visualization: yaw {}°", self.view_rot * 45)'),
    ("zoom_out",   "self.view_zoom = self.view_zoom - 1;\n        if self.view_zoom < 1 { self.view_zoom = 1; }", 'format("visualization: {}x zoom", self.view_zoom)'),
    ("zoom_in",    "self.view_zoom = self.view_zoom + 1;\n        if self.view_zoom > 3 { self.view_zoom = 3; }", 'format("visualization: {}x zoom", self.view_zoom)'),
]:
    parts.append(f"""    fn {name}(&mut self) {{
        {body}
        self.status = {msg};
    }}
""")

# --- scroll handlers ---
parts.append(scroll_handler("scroll_to_top", "self.scroll_row = 0;",
    'format("top (row 1 of {})", self.visible.len())'))
# wheel/touch-drag line scrolling -- invoked by the harness glue via __callback
# (see apply_patches.py, which resolves these handlers' callback indexes by
# name from generated.rs and wires wheel + touch-drag events to them)
parts.append(scroll_handler("scroll_line_up", "self.scroll_row = self.scroll_row - 3;",
    'format("row {} of {}", self.scroll_row + 1, self.visible.len())'))
parts.append(scroll_handler("scroll_line_down", "self.scroll_row = self.scroll_row + 3;",
    'format("row {} of {}", self.scroll_row + 1, self.visible.len())'))
parts.append(scroll_handler("scroll_page_up", "self.scroll_row = self.scroll_row - self.viewport_rows;",
    'format("row {} of {}", self.scroll_row + 1, self.visible.len())'))
parts.append(scroll_handler("scroll_page_down", "self.scroll_row = self.scroll_row + self.viewport_rows;",
    'format("row {} of {}", self.scroll_row + 1, self.visible.len())'))
parts.append(scroll_handler("scroll_to_middle", "self.scroll_row = self.visible.len() / 2;",
    'format("row {} of {}", self.scroll_row + 1, self.visible.len())'))
parts.append(scroll_handler("scroll_to_bottom", "self.scroll_row = self.visible.len() - self.viewport_rows;",
    'format("row {} of {}", self.scroll_row + 1, self.visible.len())'))

# --- breadcrumb_jump_K ---
for k in range(MAX_DEPTH + 1):
    before = f"""        if self.window_ids.len() == 0 {{
            self.status = "not connected — click Connect first";
            return;
        }}
        let mut cur: i32 = self.window_ids[0];
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
def build_chain(items, var):
    if len(items) == 1:
        return items[0][1]
    val, res = items[0]
    rest = build_chain(items[1:], var)
    if len(items) == 2:
        return f'if {var} == {val} {{ {res} }} else {{ {rest} }}'
    else:
        return f'if {var} == {val} {{ {res} }} else {{ {{{rest}}} }}'

render_lines = []
render_lines.append('    render {')
render_lines.append('        <div style="direction: vertical; width: fill; height: fill; background-color: #f6f8fa; padding: 14; gap: 8">')

render_lines.append('            <div style="direction: horizontal; gap: 12; align: center; height: 26px">')
render_lines.append('                <div style="font-size: 16px; font-weight: 700; color: #9a6700">"PLM/ALM Tree Navigator"</div>')
render_lines.append('                <div style="font-size: 11px; color: #6e7781">"live against a mock Windchill-style BOM REST API (~120ms/call) · lazy-loads each level on demand · Nectar/Honeycomb canvas mode"</div>')
render_lines.append('            </div>')
render_lines.append('            <div style="font-size: 12px; color: #57606a; height: 16px">{self.status}</div>')

render_lines.append('            <div style="direction: horizontal; gap: 4; height: 28px">')
buttons = [
    ("self.setup", "Connect"),
    ("self.import_all", "Import full BOM (~100K)"),
    ("self.expand_loaded", "Expand loaded"),
    ("self.collapse_all", "Collapse all"),
    ("self.scroll_to_top", "Top"),
]
for handler_name, label in buttons:
    render_lines.append(f'                <div style="height: 26px; padding: 0 10; background-color: #f6f8fa; border: 1px solid #d0d7de; border-radius: 4; color: #24292f; font-size: 11px; align: center; justify: center; cursor: pointer" on:click={{{handler_name}}}>"{label}"</div>')
render_lines.append('            </div>')
render_lines.append('            <div style="direction: horizontal; gap: 4; height: 26px; align: center">')
render_lines.append('                <div style="font-size: 11px; font-weight: 700; color: #6e7781; padding: 0 4">"SEARCH"</div>')
render_lines.append('                <input style="width: 220px; height: 24px; padding: 0 8; background-color: #ffffff; border: 1px solid #d0d7de; border-radius: 4; font-size: 12px; color: #1f2328; cursor: text" bind:value={query} on:input={self.on_search} />')
render_lines.append('                <div style="font-size: 11px; font-weight: 700; color: #6e7781; padding: 0 4">"FILTER"</div>')
filters = [
    ("self.filter_all", "All (tree)"),
    ("self.filter_released", "Released"),
    ("self.filter_inwork", "In Work"),
    ("self.filter_review", "Under Review"),
    ("self.filter_obsolete", "Obsolete"),
    ("self.filter_fasteners", "Fasteners"),
    ("self.filter_assemblies", "Assemblies"),
]
for handler_name, label in filters:
    render_lines.append(f'                <div style="height: 24px; padding: 0 10; background-color: #eef1f4; border: 1px solid #d0d7de; border-radius: 12; color: #24292f; font-size: 11px; align: center; justify: center; cursor: pointer" on:click={{{handler_name}}}>"{label}"</div>')
render_lines.append('                <div style="font-size: 11px; color: #6e7781; padding: 0 6">{format("→ {}", self.filter_label)}</div>')
render_lines.append('            </div>')

render_lines.append('            <div style="direction: horizontal; gap: 8; height: 22px; align: center">')
for k in range(MAX_DEPTH + 1):
    render_lines.append(f'                <div style="height: 18px; padding: 0 10; background-color: #eef1f4; border-radius: 3; cursor: pointer; align: center; justify: center; font-size: 11px; color: #57606a" on:click={{self.breadcrumb_jump_{k}}}>"{LEVEL_NAMES[k]}"</div>')
render_lines.append('            </div>')

# ===== main: tree pane + attributes pane =====
render_lines.append('            <div style="direction: horizontal; width: fill; height: 556px; gap: 8">')

render_lines.append('                <div style="direction: vertical; width: 620px; height: 556px; background-color: #ffffff; border: 1px solid #d0d7de; border-radius: 4">')
render_lines.append('                    <div style="height: 26px; background-color: #f6f8fa; border-bottom: 1px solid #d0d7de; padding: 0 10; align: center; direction: horizontal; gap: 10"><div style="font-size: 11px; font-weight: 700; color: #57606a">"STRUCTURE"</div><div style="font-size: 11px; color: #9a6700">{format("{} rows", self.visible.len())}</div><div style="font-size: 11px; color: #6e7781">{format("{} nodes loaded · {} API calls", self.loaded_count, self.api_calls)}</div></div>')

for n in range(VIEWPORT_ROWS):
    render_lines.append(f'                    {{if self.window_ids.len() > {n} {{')

    # chevron: leaf "·"; loaded+expanded "▾"; loaded collapsed "▸"; unloaded "+" (needs API call)
    loaded_inner = f'if self.expanded[self.window_ids[{n}]] {{ "▾" }} else {{ "▸" }}'
    unloaded_expr = f'if self.loaded[self.window_ids[{n}]] {{ {{{loaded_inner}}} }} else {{ "+" }}'
    chevron_expr = f'if self.window_depths[{n}] < self.max_depth {{ {{{unloaded_expr}}} }} else {{ "·" }}'
    assert chevron_expr.count('{') == chevron_expr.count('}')

    ext_expr = f'if self.window_depths[{n}] < self.max_depth {{ ".asm, A.1 (Design)" }} else {{ ".prt, A.1 (Design)" }}'

    status_dots = []
    for sval, (sname, scolor) in enumerate(STATUSES):
        status_dots.append((sval, f'<div style="width: 7px; height: 7px; border-radius: 4; background-color: {scolor}"></div>'))
    status_expr = build_chain(status_dots, f'(self.eff_state[self.window_ids[{n}]])')
    assert status_expr.count('{') == status_expr.count('}')

    row_bg = "#ffffff" if n % 2 == 0 else "#f9fafb"
    indent_spacers = ''.join(f'{{if self.window_depths[{n}] >= {lvl} {{ <div style="width: 14px"></div> }}}}' for lvl in range(1, MAX_DEPTH + 1))

    render_lines.append(f'                        <div style="direction: horizontal; height: 24px; background-color: {row_bg}; border-bottom: 1px solid #e6e8eb; align: center; padding-left: 6">')
    render_lines.append(f'                            {indent_spacers}')
    render_lines.append(f'                            <div style="width: 18px; height: 24px; align: center; justify: center; font-size: 10px; color: #9a6700; cursor: pointer" on:click={{self.toggle_row_{n}}}>{{{chevron_expr}}}</div>')
    render_lines.append(f'                            <div style="direction: horizontal; width: fill; height: 24px; align: center; gap: 5; cursor: pointer" on:click={{self.select_row_{n}}}>')
    render_lines.append(f'                                <div style="width: 12px; height: 24px; align: center; justify: center">{{{status_expr}}}</div>')
    render_lines.append(f'                                <div style="font-size: 12px; color: #1f2328">{{format("PN-{{}}", self.window_ids[{n}])}}</div>')
    render_lines.append(f'                                <div style="font-size: 12px; color: #57606a">{{format("{{}}", self.names[self.window_ids[{n}]])}}</div>')
    render_lines.append(f'                                <div style="font-size: 11px; color: #6e7781">{{{ext_expr}}}</div>')
    render_lines.append('                            </div>')
    render_lines.append('                        </div>')
    render_lines.append('                    }}')

render_lines.append('                </div>')

# --- right: Attributes pane ---
sel_type_expr = build_chain([(i, f'"{name}"') for i, name in enumerate(LEVEL_NAMES)], 'self.selected_depth')
sel_status_expr = build_chain([(i, f'"{s[0]}"') for i, s in enumerate(STATUSES)], '(self.eff_state[self.selected_id])')
sel_status_dot = build_chain(
    [(i, f'<div style="width: 8px; height: 8px; border-radius: 4; background-color: {s[1]}"></div>') for i, s in enumerate(STATUSES)],
    '(self.eff_state[self.selected_id])')
sel_ext_expr = 'if self.selected_depth < self.max_depth { ".asm" } else { ".prt" }'
sel_children_inner = 'if self.loaded[self.selected_id] { {format("{}", self.uses_ids.len())} } else { "not loaded — expand to fetch" }'
sel_children_expr = f'if self.loaded.len() > 0 {{ {{{sel_children_inner}}} }} else {{ "—" }}'

render_lines.append('                <div style="direction: vertical; width: fill; height: 556px; background-color: #ffffff; border: 1px solid #d0d7de; border-radius: 4">')
render_lines.append('                    <div style="height: 26px; background-color: #f6f8fa; border-bottom: 1px solid #d0d7de; padding: 0 10; align: center; direction: horizontal"><div style="font-size: 11px; font-weight: 700; color: #57606a">"ATTRIBUTES"</div></div>')
render_lines.append('                    <div style="direction: vertical; padding: 12; gap: 8">')
render_lines.append(f'                        <div style="direction: horizontal; gap: 8; align: center"><div style="font-size: 15px; font-weight: 700; color: #1f2328">{{format("PN-{{}}", self.selected_id)}}</div><div style="font-size: 13px; color: #57606a">{{{sel_ext_expr}}}</div></div>')
attr_rows = [
    ("Number", '{format("PN-{}", self.selected_id)}'),
    ("Name", '{if self.names.len() > 0 { {format("{}", self.names[self.selected_id])} } else { "—" }}'),
    ("Type", f'{{{sel_type_expr}}}'),
    ("Revision", '"A.1 (Design)"'),
    ("Lifecycle state", None),
    ("Direct children", f'{{{sel_children_expr}}}'),
    ("Depth in structure", '{format("Level {}", self.selected_depth)}'),
    ("Set lifecycle state", "EDIT_STATE"),
    ("Quantity (this usage)", "EDIT_QTY"),
]
for label, value in attr_rows:
    render_lines.append('                        <div style="direction: horizontal; height: 18px; align: center">')
    render_lines.append(f'                            <div style="width: 150px; font-size: 12px; color: #6e7781">"{label}"</div>')
    if label == "Lifecycle state":
        render_lines.append(f'                            <div style="direction: horizontal; gap: 6; align: center"><div style="width: 10px; height: 20px; align: center; justify: center">{{{sel_status_dot}}}</div><div style="font-size: 12px; color: #24292f; height: 20px">{{{sel_status_expr}}}</div></div>')
    elif value == "EDIT_STATE":
        render_lines.append('                            <div style="direction: horizontal; gap: 5; align: center">')
        for sval, (sname, _) in enumerate(STATUSES):
            render_lines.append(f'                                <div style="height: 18px; padding: 0 7; background-color: #f6f8fa; border: 1px solid #d0d7de; border-radius: 4; color: #24292f; font-size: 10px; align: center; justify: center; cursor: pointer" on:click={{self.set_state_{sval}}}>"{sname}"</div>')
        render_lines.append('                            </div>')
    elif value == "EDIT_QTY":
        render_lines.append('                            <div style="direction: horizontal; gap: 6; align: center">')
        render_lines.append('                                <div style="width: 18px; height: 18px; background-color: #f6f8fa; border: 1px solid #d0d7de; border-radius: 4; color: #24292f; font-size: 12px; align: center; justify: center; cursor: pointer" on:click={self.qty_dec}>"−"</div>')
        render_lines.append('                                <div style="width: 26px; font-size: 12px; color: #1f2328; align: center; justify: center">{format("{}", self.qty[self.selected_id])}</div>')
        render_lines.append('                                <div style="width: 18px; height: 18px; background-color: #f6f8fa; border: 1px solid #d0d7de; border-radius: 4; color: #24292f; font-size: 12px; align: center; justify: center; cursor: pointer" on:click={self.qty_inc}>"+"</div>')
        render_lines.append('                                <div style="font-size: 11px; color: #6e7781">"each · POSTs to /api/part/update"</div>')
        render_lines.append('                            </div>')
    else:
        render_lines.append(f'                            <div style="font-size: 12px; color: #24292f">{value}</div>')
    render_lines.append('                        </div>')
render_lines.append('                        <div style="height: 6px"></div>')
render_lines.append('                        <div style="direction: horizontal; gap: 10; align: center; height: 24px">')
render_lines.append('                            <div style="width: 100px; font-size: 11px; font-weight: 700; color: #57606a">"VISUALIZATION"</div>')
for hname, lbl in [("view_iso", "Iso"), ("view_front", "Front"), ("view_top", "Top"), ("rot_left", "⟲"), ("rot_right", "⟳"), ("zoom_out", "−"), ("zoom_in", "+")]:
    render_lines.append(f'                            <div style="height: 22px; padding: 0 9; background-color: #f6f8fa; border: 1px solid #d0d7de; border-radius: 4; color: #24292f; font-size: 11px; align: center; justify: center; cursor: pointer" on:click={{self.{hname}}}>"{lbl}"</div>')
render_lines.append('                        </div>')
render_lines.append('                        <img src={format("/api/part/thumb-{}-v{}-r{}-z{}.svg", self.selected_id, self.view_name, self.view_rot, self.view_zoom)} style="width: 400px; height: 236px; border: 1px solid #d0d7de; border-radius: 4" />')
render_lines.append('                    </div>')
render_lines.append('                </div>')

render_lines.append('            </div>')

# ===== bottom: Uses table =====
render_lines.append('            <div style="direction: horizontal; width: fill; height: 170px; gap: 8">')
render_lines.append('            <div style="direction: vertical; width: 760px; height: 170px; background-color: #ffffff; border: 1px solid #d0d7de; border-radius: 4">')
render_lines.append('                <div style="height: 26px; background-color: #f6f8fa; border-bottom: 1px solid #d0d7de; padding: 0 10; align: center; direction: horizontal; gap: 8"><div style="font-size: 11px; font-weight: 700; color: #57606a">"USES"</div><div style="font-size: 11px; color: #6e7781">{format("— direct children of PN-{}", self.selected_id)}</div></div>')
render_lines.append('                <div style="direction: horizontal; height: 22px; background-color: #f9fafb; border-bottom: 1px solid #d0d7de; align: center; padding-left: 10">')
for col, w in [("NUMBER", 120), ("NAME", 220), ("VERSION", 110), ("STATE", 130), ("QUANTITY", 80), ("UNIT", 60)]:
    render_lines.append(f'                    <div style="width: {w}px; font-size: 10px; font-weight: 700; color: #6e7781">"{col}"</div>')
render_lines.append('                </div>')

for k in range(BRANCHING):
    row_bg = "#ffffff" if k % 2 == 0 else "#f9fafb"
    child_status_expr = build_chain([(i, f'"{s[0]}"') for i, s in enumerate(STATUSES)], f'(self.eff_state[self.uses_ids[{k}]])')
    child_dot = build_chain(
        [(i, f'<div style="width: 7px; height: 7px; border-radius: 4; background-color: {s[1]}"></div>') for i, s in enumerate(STATUSES)],
        f'(self.eff_state[self.uses_ids[{k}]])')
    render_lines.append(f'                {{if self.uses_ids.len() > {k} {{')
    render_lines.append(f'                    <div style="direction: horizontal; height: 24px; background-color: {row_bg}; border-bottom: 1px solid #e6e8eb; align: center; padding-left: 10">')
    render_lines.append(f'                        <div style="width: 120px; font-size: 12px; color: #1f2328">{{format("PN-{{}}", self.uses_ids[{k}])}}</div>')
    render_lines.append(f'                        <div style="width: 220px; font-size: 12px; color: #57606a">{{format("{{}}", self.names[self.uses_ids[{k}]])}}</div>')
    render_lines.append(f'                        <div style="width: 110px; font-size: 12px; color: #57606a">"A.1 (Design)"</div>')
    render_lines.append(f'                        <div style="width: 130px; direction: horizontal; gap: 5; align: center"><div style="width: 9px; height: 24px; align: center; justify: center">{{{child_dot}}}</div><div style="font-size: 12px; color: #57606a">{{{child_status_expr}}}</div></div>')
    render_lines.append(f'                        <div style="width: 80px; font-size: 12px; color: #57606a">{{format("{{}}", self.qty[self.uses_ids[{k}]])}}</div>')
    render_lines.append(f'                        <div style="width: 60px; font-size: 12px; color: #57606a">"each"</div>')
    render_lines.append('                    </div>')
    render_lines.append('                }}')
render_lines.append('            </div>')

# RAW RESPONSE pane: the actual JSON the API returned (first entries)
render_lines.append('            <div style="direction: vertical; width: fill; height: 170px; background-color: #ffffff; border: 1px solid #d0d7de; border-radius: 4">')
render_lines.append('                <div style="height: 26px; background-color: #f6f8fa; border-bottom: 1px solid #d0d7de; padding: 0 10; align: center; direction: horizontal; gap: 8"><div style="font-size: 11px; font-weight: 700; color: #57606a">"RAW RESPONSE"</div><div style="font-size: 11px; color: #6e7781">{format("{} · {} nodes in last payload", self.last_url, self.fetched.len())}</div></div>')
render_lines.append('                <div style="direction: vertical; padding: 8 10; gap: 1">')
render_lines.append('                    <div style="font-size: 11px; color: #6e7781">"["</div>')
for k in range(5):
    render_lines.append(f'                    {{if self.fetched.len() > {k} {{')
    render_lines.append(f'                        <div style="font-size: 11px; color: #24292f">{{format("  {{{{\\"id\\": {{}}, \\"name\\": \\"{{}}\\", \\"leaf\\": {{}}, \\"state\\": {{}}, \\"qty\\": {{}}}}}},", self.fetched[{k}].id, self.fetched[{k}].name, self.fetched[{k}].leaf, self.fetched[{k}].state, self.fetched[{k}].qty)}}</div>')
    render_lines.append('                    }}')
render_lines.append('                    {if self.fetched.len() > 5 {')
render_lines.append('                        <div style="font-size: 11px; color: #6e7781">{format("  … {} more nodes in this response", self.fetched.len() - 5)}</div>')
render_lines.append('                    }}')
render_lines.append('                    <div style="font-size: 11px; color: #6e7781">"]"</div>')
render_lines.append('                </div>')
render_lines.append('            </div>')
render_lines.append('            </div>')

render_lines.append('        </div>')
render_lines.append('    }')
render_lines.append('}')

parts.append("\n".join(render_lines) + "\n")

out = "".join(parts)
assert out.count('{') == out.count('}'), f"unbalanced braces: {out.count(chr(123))} vs {out.count(chr(125))}"

import os
with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'tree_bench_v4.nectar'), 'w') as f:
    f.write(out)

print("generated, chars:", len(out), "lines:", out.count(chr(10)))
