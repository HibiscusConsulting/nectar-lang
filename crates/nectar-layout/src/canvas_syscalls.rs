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
