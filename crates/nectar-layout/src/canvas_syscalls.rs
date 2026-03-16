//! Canvas 2D syscalls — imported from the browser host.
//! These are the ONLY JS bridge functions. Each is 1-2 lines in core.js.

#[cfg(target_arch = "wasm32")]
extern "C" {
    // Canvas lifecycle
    pub fn canvas_init(width: f32, height: f32) -> u32;
    pub fn canvas_clear(id: u32);
    pub fn canvas_request_frame(cb_idx: u32);

    // Drawing
    pub fn canvas_fill_rect(id: u32, x: f32, y: f32, w: f32, h: f32, r: u32, g: u32, b: u32, a: u32);
    pub fn canvas_stroke_rect(id: u32, x: f32, y: f32, w: f32, h: f32, r: u32, g: u32, b: u32, a: u32, line_width: f32);
    pub fn canvas_round_rect(id: u32, x: f32, y: f32, w: f32, h: f32, radius: f32, r: u32, g: u32, b: u32, a: u32);
    pub fn canvas_fill_text(id: u32, ptr: *const u8, len: u32, x: f32, y: f32, r: u32, g: u32, b: u32, size: f32, bold: u32);
    pub fn canvas_draw_image(id: u32, src_ptr: *const u8, src_len: u32, x: f32, y: f32, w: f32, h: f32, clip_radius: f32);
    pub fn canvas_measure_text(id: u32, ptr: *const u8, len: u32, size: f32) -> f32;

    // Viewport
    pub fn canvas_get_width() -> f32;
    pub fn canvas_get_height() -> f32;

    // Clipboard
    pub fn clipboard_write(ptr: *const u8, len: u32);
    pub fn clipboard_read(buf_ptr: *mut u8, buf_cap: u32) -> u32;

    // Form input overlay — positions a real <input> element over canvas region
    pub fn input_overlay_show(x: f32, y: f32, w: f32, h: f32, value_ptr: *const u8, value_len: u32, element_id: u32);
    pub fn input_overlay_hide();
    pub fn input_overlay_get_value(buf_ptr: *mut u8, buf_cap: u32) -> u32;

    // Browser search (Cmd+F) — scroll hidden DOM element into view
    pub fn search_scroll_to(element_index: u32);

    // Hybrid mode — read browser-computed layout from hidden DOM
    // Writes x, y, w, h as f32s into the provided buffer (16 bytes)
    pub fn dom_get_rect(element_id: u32, out_ptr: *mut f32);

    // Hybrid mode — DOM element creation (mirrors core.js dom namespace)
    pub fn dom_create_element(tag_ptr: *const u8, tag_len: u32) -> u32;
    pub fn dom_set_text_hybrid(el_id: u32, text_ptr: *const u8, text_len: u32);
    pub fn dom_set_attr_hybrid(el_id: u32, name_ptr: *const u8, name_len: u32, val_ptr: *const u8, val_len: u32);
    pub fn dom_set_style_hybrid(el_id: u32, prop_ptr: *const u8, prop_len: u32, val_ptr: *const u8, val_len: u32);
    pub fn dom_append_child_hybrid(parent_id: u32, child_id: u32);
    pub fn dom_get_root() -> u32;
}

// Stubs for non-WASM targets (native tests)
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn canvas_init(_w: f32, _h: f32) -> u32 { 1 }
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn canvas_clear(_id: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn canvas_request_frame(_cb: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn canvas_fill_rect(_id: u32, _x: f32, _y: f32, _w: f32, _h: f32, _r: u32, _g: u32, _b: u32, _a: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn canvas_stroke_rect(_id: u32, _x: f32, _y: f32, _w: f32, _h: f32, _r: u32, _g: u32, _b: u32, _a: u32, _lw: f32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn canvas_round_rect(_id: u32, _x: f32, _y: f32, _w: f32, _h: f32, _r: f32, _cr: u32, _cg: u32, _cb: u32, _ca: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn canvas_fill_text(_id: u32, _p: *const u8, _l: u32, _x: f32, _y: f32, _r: u32, _g: u32, _b: u32, _s: f32, _b2: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn canvas_draw_image(_id: u32, _sp: *const u8, _sl: u32, _x: f32, _y: f32, _w: f32, _h: f32, _cr: f32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn canvas_measure_text(_id: u32, _p: *const u8, _l: u32, _s: f32) -> f32 { 0.0 }
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn canvas_get_width() -> f32 { 1400.0 }
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn canvas_get_height() -> f32 { 900.0 }
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn clipboard_write(_p: *const u8, _l: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn clipboard_read(_p: *mut u8, _c: u32) -> u32 { 0 }
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn input_overlay_show(_x: f32, _y: f32, _w: f32, _h: f32, _vp: *const u8, _vl: u32, _id: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn input_overlay_hide() {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn input_overlay_get_value(_p: *mut u8, _c: u32) -> u32 { 0 }
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn search_scroll_to(_i: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn dom_get_rect(_id: u32, _out: *mut f32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn dom_create_element(_p: *const u8, _l: u32) -> u32 { 0 }
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn dom_set_text_hybrid(_id: u32, _p: *const u8, _l: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn dom_set_attr_hybrid(_id: u32, _np: *const u8, _nl: u32, _vp: *const u8, _vl: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn dom_set_style_hybrid(_id: u32, _pp: *const u8, _pl: u32, _vp: *const u8, _vl: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn dom_append_child_hybrid(_p: u32, _c: u32) {}
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn dom_get_root() -> u32 { 1 }
