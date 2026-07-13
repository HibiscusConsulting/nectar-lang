"""Mock Windchill/Codebeamer-style BOM REST API + static file server.

Endpoints:
  GET /api/bom/children?node=N   -> JSON array of the node's direct children
                                    (lazy-load path, ~120ms simulated latency)
  GET /api/bom/tree              -> the ENTIRE BOM as one JSON array
                                    (~100K nodes, the "multi-level BOM report"
                                    bulk path; JSON cached at startup)
  GET /api/part/thumb-N.svg      -> procedural CAD-style rendering for part N
                                    (deterministic wireframe, like a Windchill
                                    visualization thumbnail)

Child count per node is 4-6, derived from a hash of the node id, so the
client genuinely cannot know the structure without calling the API.
Child ids use the 6-ary arithmetic id space (parent*6 + c + 1).
"""
import json
import time
import re
from http.server import SimpleHTTPRequestHandler, HTTPServer

MAX_DEPTH = 7
LEVELS = ["Top Assembly", "Major Assembly", "Sub-Assembly", "Component",
          "Sub-Component", "Detail Part", "Fastener Group", "Fastener"]
LATENCY_S = 0.12

def depth_of(idx: int) -> int:
    d = 0
    while idx > 0:
        idx = (idx - 1) // 6
        d += 1
    return d

def child_count(idx: int) -> int:
    # 4..6 children, deterministic per node
    return 4 + ((idx * 2654435761) >> 7) % 3

def node_name(idx: int, d: int) -> str:
    return f"{LEVELS[d].lower().replace(' ', '_')}_{idx}"

def children_of(node: int):
    d = depth_of(node)
    out = []
    if d < MAX_DEPTH:
        cd = d + 1
        for c in range(child_count(node)):
            cid = node * 6 + c + 1
            out.append({"id": cid, "name": node_name(cid, cd), "leaf": 1 if cd >= MAX_DEPTH else 0})
    return out

# ── Precompute the full tree JSON once (BFS over actual variable-arity tree) ──
def build_full_tree():
    nodes = [{"id": 0, "name": node_name(0, 0), "leaf": 0}]
    frontier = [0]
    while frontier:
        nxt = []
        for n in frontier:
            for ch in children_of(n):
                nodes.append(ch)
                if not ch["leaf"]:
                    nxt.append(ch["id"])
        frontier = nxt
    return nodes

print("precomputing full BOM tree…")
_t0 = time.time()
FULL_TREE = build_full_tree()
FULL_TREE_JSON = json.dumps(FULL_TREE).encode()
print(f"  {len(FULL_TREE)} nodes, {len(FULL_TREE_JSON)/1e6:.1f} MB JSON, {time.time()-_t0:.1f}s")

# ── Procedural CAD-style SVG renderer ─────────────────────────────────────
# Light "engineering paper" theme. Deterministic per part id, but with real
# variety: four part archetypes (block, shaft, plate, bracket) chosen by id
# hash; assemblies (non-leaf) render as composed multi-part scenes.
# Interactive views like a real CAD web viewer: v=iso|front|top, r=0..7
# (45° yaw steps), z=1..3 zoom. The client requests a new server render per
# view change — the same model Creo View / Windchill visualization uses.
import math

INK = "#1f2933"
HIDDEN = "#b0b8c0"
GRID = "#eef1f4"
NOTE = "#6a737d"
ACCENTS = ["#0969da", "#1a7f37", "#9a6700", "#cf222e"]

def _r(h, n, lo, hi):
    return lo + ((h >> n) % 1000) / 1000.0 * (hi - lo)

class Iso:
    def __init__(self, cx, cy, yaw_steps, scale):
        self.cx, self.cy = cx, cy
        a = math.radians(30 + yaw_steps * 45)
        self.ca, self.sa = math.cos(a), math.sin(a)
        self.s = scale
    def pt(self, x, y, z):
        rx = x * self.ca - y * self.sa
        ry = x * self.sa + y * self.ca
        return (self.cx + (rx - ry) * 0.866 * self.s,
                self.cy + (rx + ry) * 0.5 * self.s - z * self.s)

def _line(p, q, col=INK, sw=1.3, dash=""):
    d = f' stroke-dasharray="{dash}"' if dash else ""
    return f'<line x1="{p[0]:.1f}" y1="{p[1]:.1f}" x2="{q[0]:.1f}" y2="{q[1]:.1f}" stroke="{col}" stroke-width="{sw}"{d}/>'

def _poly(pts, fill, op, col=INK, sw=1.1):
    s = " ".join(f"{p[0]:.1f},{p[1]:.1f}" for p in pts)
    return f'<polygon points="{s}" fill="{fill}" fill-opacity="{op}" stroke="{col}" stroke-width="{sw}"/>'

def _box(iso, w, d, hgt, accent, z0=0, x0=0, y0=0):
    c = {k: iso.pt(x0 + dx, y0 + dy, z0 + dz) for k, (dx, dy, dz) in {
        'a': (-w/2, -d/2, 0), 'b': (w/2, -d/2, 0), 'c': (w/2, d/2, 0), 'd': (-w/2, d/2, 0),
        'A': (-w/2, -d/2, hgt), 'B': (w/2, -d/2, hgt), 'C': (w/2, d/2, hgt), 'D': (-w/2, d/2, hgt)}.items()}
    out = [_poly([c['A'], c['B'], c['C'], c['D']], accent, 0.14),
           _poly([c['b'], c['B'], c['C'], c['c']], accent, 0.07),
           _poly([c['a'], c['b'], c['B'], c['A']], accent, 0.22)]
    for e in ["ab", "bc", "aA", "bB", "cC", "AB", "BC", "CD", "DA"]:
        out.append(_line(c[e[0]], c[e[1]]))
    for e in ["cd", "da", "dD"]:
        out.append(_line(c[e[0]], c[e[1]], HIDDEN, 1.0, "4,3"))
    return out

def _cyl(iso, rad, hgt, accent, z0=0, x0=0, y0=0):
    top, bot = iso.pt(x0, y0, z0 + hgt), iso.pt(x0, y0, z0)
    rx, ry = rad * iso.s * 1.2, rad * iso.s * 0.6
    out = [f'<ellipse cx="{bot[0]:.1f}" cy="{bot[1]:.1f}" rx="{rx:.1f}" ry="{ry:.1f}" fill="{accent}" fill-opacity="0.07" stroke="{INK}" stroke-width="1.1"/>',
           f'<rect x="{top[0]-rx:.1f}" y="{top[1]:.1f}" width="{2*rx:.1f}" height="{bot[1]-top[1]:.1f}" fill="{accent}" fill-opacity="0.12" stroke="none"/>',
           _line((top[0]-rx, top[1]), (bot[0]-rx, bot[1])), _line((top[0]+rx, top[1]), (bot[0]+rx, bot[1])),
           f'<ellipse cx="{top[0]:.1f}" cy="{top[1]:.1f}" rx="{rx:.1f}" ry="{ry:.1f}" fill="{accent}" fill-opacity="0.16" stroke="{INK}" stroke-width="1.2"/>',
           f'<ellipse cx="{top[0]:.1f}" cy="{top[1]:.1f}" rx="{rx*0.4:.1f}" ry="{ry*0.4:.1f}" fill="none" stroke="{INK}" stroke-width="1.0"/>']
    return out

def _archetype(h):
    return (h >> 21) % 4

def _part_geo(iso, h, accent, scale=1.0):
    a = _archetype(h)
    if a == 0:   # block with bore
        return _box(iso, _r(h,3,55,105)*scale, _r(h,7,45,85)*scale, _r(h,11,35,90)*scale, accent)
    if a == 1:   # shaft: stacked cylinders
        out = _cyl(iso, _r(h,3,18,30)*scale, _r(h,7,55,95)*scale, accent)
        out += _cyl(iso, _r(h,11,32,44)*scale, _r(h,13,14,24)*scale, accent, z0=0)
        return out
    if a == 2:   # flat plate with hole pattern
        w, d = _r(h,3,90,130)*scale, _r(h,7,70,110)*scale
        out = _box(iso, w, d, 12*scale, accent)
        for ix in (-1, 1):
            for iy in (-1, 1):
                p = iso.pt(ix*w*0.32, iy*d*0.32, 12*scale)
                out.append(f'<ellipse cx="{p[0]:.1f}" cy="{p[1]:.1f}" rx="{7*iso.s:.1f}" ry="{3.5*iso.s:.1f}" fill="none" stroke="{INK}" stroke-width="1.0"/>')
        return out
    # 3: L-bracket
    out = _box(iso, _r(h,3,80,120)*scale, _r(h,7,55,80)*scale, 14*scale, accent)
    out += _box(iso, 14*scale, _r(h,7,55,80)*scale, _r(h,13,50,85)*scale, accent,
                x0=-_r(h,3,80,120)*scale/2 + 7*scale)
    return out

def part_svg(pid: int, view: str = "iso", rot: int = 0, zoom: int = 1) -> bytes:
    h = pid * 2654435761 & 0xffffffff
    d = depth_of(pid)
    accent = ACCENTS[pid % 4]
    W, H = 460, 320
    scale = 0.75 * (1.25 ** (zoom - 1))
    is_asm = d < MAX_DEPTH
    svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
           f'<rect width="{W}" height="{H}" fill="#ffffff"/>']
    for gx in range(0, W, 20):
        svg.append(f'<line x1="{gx}" y1="0" x2="{gx}" y2="{H}" stroke="{GRID}" stroke-width="1"/>')
    for gy in range(0, H, 20):
        svg.append(f'<line x1="0" y1="{gy}" x2="{W}" y2="{gy}" stroke="{GRID}" stroke-width="1"/>')

    if view == "iso":
        iso = Iso(W/2, H/2 + 28, rot, scale)
        if is_asm and pid != 0:
            # assembly: compose children archetypes around the origin
            kids = children_of(pid)[:3]
            offs = [(-70, -40), (60, -25), (-5, 55)]
            for (kid, off) in zip(kids, offs):
                kh = kid["id"] * 2654435761 & 0xffffffff
                sub = Iso(W/2 + off[0]*scale, H/2 + 20 + off[1]*scale*0.5, rot, scale*0.62)
                svg += _part_geo(sub, kh, ACCENTS[kid["id"] % 4])
        elif pid == 0:
            svg += _box(iso, 150, 100, 60, accent)
            svg += _cyl(iso, 25, 95, accent, x0=-30, y0=-10)
            svg += _box(iso, 40, 90, 95, accent, x0=62)
        else:
            svg += _part_geo(iso, h, accent, 1.15)
    else:
        # orthographic front/top: dimensioned 2D outline
        w2, h2 = _r(h,3,120,190)*scale, _r(h,11,80,150)*scale
        x0, y0 = W/2 - w2/2, H/2 - h2/2 + 12
        svg.append(f'<rect x="{x0:.1f}" y="{y0:.1f}" width="{w2:.1f}" height="{h2:.1f}" fill="{accent}" fill-opacity="0.10" stroke="{INK}" stroke-width="1.4"/>')
        if _archetype(h) in (0, 2):
            svg.append(f'<circle cx="{W/2:.1f}" cy="{y0 + h2/2:.1f}" r="{min(w2,h2)*0.22:.1f}" fill="none" stroke="{INK}" stroke-width="1.2"/>')
            svg.append(_line((W/2 - w2*0.55, y0 + h2/2), (W/2 + w2*0.55, y0 + h2/2), "#8a939c", 0.8, "12,4,2,4"))
            svg.append(_line((W/2, y0 - h2*0.12), (W/2, y0 + h2*1.12), "#8a939c", 0.8, "12,4,2,4"))
        else:
            svg.append(f'<rect x="{x0 + w2*0.12:.1f}" y="{y0 + h2*0.15:.1f}" width="{w2*0.3:.1f}" height="{h2*0.7:.1f}" fill="none" stroke="{INK}" stroke-width="1.1"/>')
        # dimension line
        dy = y0 + h2 + 22
        svg.append(_line((x0, dy), (x0 + w2, dy), NOTE, 0.9))
        svg.append(_line((x0, dy - 5), (x0, dy + 5), NOTE, 0.9))
        svg.append(_line((x0 + w2, dy - 5), (x0 + w2, dy + 5), NOTE, 0.9))
        svg.append(f'<text x="{x0 + w2/2:.1f}" y="{dy - 5:.1f}" fill="{NOTE}" font-family="monospace" font-size="10" text-anchor="middle">{w2/scale:.0f} mm</text>')

    kind = "assembly" if is_asm else "part"
    svg.append(f'<text x="12" y="20" fill="{NOTE}" font-family="monospace" font-size="12">PN-{pid} · rev A.1 · {view}{" · yaw " + str(rot*45) + "°" if view == "iso" else ""} · {zoom}x</text>')
    svg.append(f'<text x="12" y="{H-12}" fill="#9aa3ab" font-family="monospace" font-size="10">{LEVELS[d]} ({kind}) · server-rendered mock CAD view</text>')
    svg.append('</svg>')
    return "".join(svg).encode()

class Handler(SimpleHTTPRequestHandler):
    def _send(self, body: bytes, ctype: str):
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        m = re.match(r'^/api/bom/children\?node=(\d+)$', self.path)
        if m:
            time.sleep(LATENCY_S)
            return self._send(json.dumps(children_of(int(m.group(1)))).encode(), "application/json")

        if self.path == '/api/bom/tree':
            time.sleep(LATENCY_S)  # one round-trip's worth; payload transfer is the real cost
            return self._send(FULL_TREE_JSON, "application/json")

        m = re.match(r'^/api/part/thumb-(\d+)-v(iso|front|top)-r(\d)-z(\d)\.svg$', self.path)
        if m:
            return self._send(part_svg(int(m.group(1)), m.group(2), int(m.group(3)), int(m.group(4))), "image/svg+xml")
        m = re.match(r'^/api/part/thumb-(\d+)\.svg$', self.path)
        if m:
            return self._send(part_svg(int(m.group(1))), "image/svg+xml")

        return super().do_GET()

    def log_message(self, fmt, *args):
        pass

if __name__ == "__main__":
    import sys
    import os
    from http.server import ThreadingHTTPServer
    # Cloud Run provides PORT and needs 0.0.0.0; locally default to loopback :8123
    env_port = os.environ.get("PORT")
    port = int(sys.argv[1]) if len(sys.argv) > 1 else (int(env_port) if env_port else 8123)
    host = "0.0.0.0" if env_port else "127.0.0.1"
    print(f"serving on {host}:{port}")
    ThreadingHTTPServer((host, port), Handler).serve_forever()
