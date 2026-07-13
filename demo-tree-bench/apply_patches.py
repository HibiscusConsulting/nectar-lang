"""Post-build patches for Nectar canvas builds (tree navigator).

Run: python3 apply_patches.py <build_dir>/index.html

1. Strip the hardcoded buildnectar.com nav bar (compiler has no flag for it).
2. Fix the touchend(0,0) coordinate bug (every mobile tap hits top-left).
3. Suppress tap-click after a touch DRAG (drag = scroll gesture, not a tap).
4. Wire mouse wheel + touch drag to the app's scroll_line_up/scroll_line_down
   handlers via the WASM __callback dispatcher. Indexes are resolved BY NAME
   from generated.rs in the same build dir, so this survives any reordering
   of component methods across rebuilds.

Root cause this works around: the engine's app_scroll only drives
engine-native scroll containers, and the compiler has no on:wheel handler
syntax, so an app that windows its own list (required for 100K+ node trees)
has no way to receive wheel/drag input from source alone. The upstream fix
is compiler-level on:wheel support; until then this glue patch is the
mechanism, applied identically to every build.
"""
import re
import sys
import os

path = sys.argv[1]
build_dir = os.path.dirname(path)

with open(path) as f:
    html = f.read()

if '__treeScroll' in html:
    print("already patched, skipping:", path)
    sys.exit(0)

# -- resolve scroll handler callback indexes from generated.rs --
gen_path = os.path.join(build_dir, 'generated.rs')
scroll_up_idx = scroll_down_idx = None
if os.path.exists(gen_path):
    with open(gen_path) as f:
        gen = f.read()
    m = re.search(r'(\d+)\s*=>\s*\w+_scroll_line_up\(\)', gen)
    if m:
        scroll_up_idx = int(m.group(1))
    m = re.search(r'(\d+)\s*=>\s*\w+_scroll_line_down\(\)', gen)
    if m:
        scroll_down_idx = int(m.group(1))

# -- 1. strip nav bar --
nav_block = re.search(r'<nav>.*?</nav>\n', html, re.S)
if nav_block:
    html = html.replace(nav_block.group(0), '')
html = html.replace(
    '.canvas-stack{position:fixed;top:48px;left:0;width:100%;height:calc(100% - 48px)}',
    '.canvas-stack{position:fixed;top:0;left:0;width:100%;height:100%}'
)
html = re.sub(r'(<div id="a11y"[^>]*style="position:fixed;top:)48px', r'\g<1>0', html)
html = html.replace('const navH = 48;', 'const navH = 0;')
# shareable page title (the compiler emits the source filename)
html = html.replace('<title>tree_bench_v4</title>', '<title>PLM/ALM Tree Navigator — Nectar/Honeycomb demo</title>')
# light theme: page body must match the app's light background
html = html.replace('body{background:#0b0e14}', 'body{background:#f6f8fa}')
# kill the /favicon.svg 404 console noise (file doesn't exist in builds)
html = html.replace('<link rel="icon" type="image/svg+xml" href="/favicon.svg">', '<link rel="icon" href="data:,">')

# -- 2+3. touch: real coordinates, and drag-vs-tap discrimination --
old_touchstart = "const t=e.touches[0]; if (W.app_touchstart) W.app_touchstart(t.clientX, t.clientY);"
new_touchstart = "const t=e.touches[0]; window._ltX=t.clientX; window._ltY=t.clientY; window._tDrag=0; window._tAcc=0; if (W.app_touchstart) W.app_touchstart(t.clientX, t.clientY);"
assert old_touchstart in html, "touchstart anchor missing"
html = html.replace(old_touchstart, new_touchstart)

old_touchmove = "const t=e.touches[0]; if (W.app_touchmove) W.app_touchmove(t.clientX, t.clientY); if (W.app_render) W.app_render();"
new_touchmove = ("const t=e.touches[0]; const dy=t.clientY-window._ltY; window._ltX=t.clientX; window._ltY=t.clientY; "
                 "if (Math.abs(dy)>2) window._tDrag=1; __treeScroll(-dy); "
                 "if (W.app_touchmove) W.app_touchmove(t.clientX, t.clientY); if (W.app_render) W.app_render();")
assert old_touchmove in html, "touchmove anchor missing"
html = html.replace(old_touchmove, new_touchmove)

old_touchend = "eventTarget.addEventListener('touchend', e => { if (W.app_touchend) W.app_touchend(0, 0); if (W.app_render) W.app_render(); });"
new_touchend = "eventTarget.addEventListener('touchend', e => { if (window._tDrag) { window._tDrag=0; if (W.app_render) W.app_render(); return; } if (W.app_touchend) W.app_touchend(window._ltX||0, window._ltY||0); if (W.app_render) W.app_render(); });"
assert old_touchend in html, "touchend anchor missing"
html = html.replace(old_touchend, new_touchend)

# -- 4. wheel -> scroll_line handlers via __callback --
if scroll_up_idx is not None and scroll_down_idx is not None:
    scroll_fn = f"""
// windowed-list scrolling: wheel/drag -> app scroll handlers (indexes resolved from generated.rs)
const SCROLL_UP_CB = {scroll_up_idx}, SCROLL_DOWN_CB = {scroll_down_idx};
let _wheelAcc = 0;
function __treeScroll(delta) {{
  _wheelAcc += delta;
  let fired = false;
  while (_wheelAcc >= 8) {{ if (W.__callback) W.__callback(SCROLL_DOWN_CB); _wheelAcc -= 8; fired = true; }}
  while (_wheelAcc <= -8) {{ if (W.__callback) W.__callback(SCROLL_UP_CB); _wheelAcc += 8; fired = true; }}
  return fired;
}}
"""
    old_wheel = "eventTarget.addEventListener('wheel', e => {\n  e.preventDefault();\n  if (W.app_mousemove) W.app_mousemove(e.offsetX, e.offsetY, 0);\n  if (W.app_scroll) W.app_scroll(e.deltaY);\n  if (W.app_render) W.app_render();\n}, { passive: false });"
    new_wheel = ("eventTarget.addEventListener('wheel', e => {\n  e.preventDefault();\n  if (W.app_mousemove) W.app_mousemove(e.offsetX, e.offsetY, 0);\n"
                 "  __treeScroll(e.deltaY);\n  if (W.app_render) W.app_render();\n}, { passive: false });")
    assert old_wheel in html, "wheel anchor missing"
    html = html.replace(old_wheel, new_wheel)

    anchor = "eventTarget.addEventListener('click', e => {"
    assert anchor in html
    html = html.replace(anchor, scroll_fn + "\n" + anchor, 1)
    print(f"wheel+drag scrolling wired: up=cb{scroll_up_idx}, down=cb{scroll_down_idx}")
else:
    html = html.replace("eventTarget.addEventListener('click', e => {",
                        "function __treeScroll(){return false;}\neventTarget.addEventListener('click', e => {", 1)
    print("WARNING: scroll_line_up/down not found in generated.rs -- wheel scrolling not wired")

with open(path, 'w') as f:
    f.write(html)
print("patched", path)
