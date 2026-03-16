//! Hybrid render mode — hidden DOM for layout + canvas for paint.
//! Browser CSS engine computes positions. WASM reads them and paints.
//! Hidden DOM stays live for SEO, accessibility, Cmd+F, text selection.

use crate::canvas_syscalls::*;
use std::cell::RefCell;
use std::fmt::Write;

const CATS: [&str; 5] = ["Electronics", "Clothing", "Home", "Sports", "Books"];

struct HybridState {
    canvas_id: u32,
    vw: f32,
    vh: f32,
    scroll_y: f32,
    cart_count: u32,
    signal_fires: u32,
    active_cat: usize,
    sort_order: u8,
    // DOM element IDs for each product card
    card_dom_ids: Vec<u32>,
    // Cached layout rects (x, y, w, h) per card — read from DOM
    card_rects: Vec<[f32; 4]>,
    // Container DOM element
    container_id: u32,
    // Timing
    t_fetch: f32,
    t_dom: f32,
    t_layout: f32,
    // Scrollbar
    scrollbar_opacity: f32,
    scrollbar_fade_frame: u32,
}

thread_local! {
    static HYBRID: RefCell<HybridState> = RefCell::new(HybridState {
        canvas_id: 0, vw: 0.0, vh: 0.0, scroll_y: 0.0,
        cart_count: 0, signal_fires: 0, active_cat: 0, sort_order: 0,
        card_dom_ids: Vec::new(), card_rects: Vec::new(), container_id: 0,
        t_fetch: 0.0, t_dom: 0.0, t_layout: 0.0,
        scrollbar_opacity: 0.0, scrollbar_fade_frame: 0,
    });
}

/// Phase 1: Create canvas + build hidden DOM with 10K product cards.
/// The DOM is styled with CSS grid — browser handles layout.
#[no_mangle]
pub extern "C" fn hybrid_init(vw: f32, vh: f32, t_fetch: f32) {
    HYBRID.with(|h| {
        let mut state = h.borrow_mut();
        state.vw = vw;
        state.vh = vh;
        state.t_fetch = t_fetch;

        // Create canvas
        state.canvas_id = unsafe { canvas_init(vw, vh) };

        // Get root element
        state.container_id = unsafe { dom_get_root() };

        // Set container to CSS grid layout (browser does the layout math)
        let style_display = b"display";
        let style_grid = b"grid";
        let style_gtc = b"grid-template-columns";
        let style_gtc_val = b"repeat(auto-fill, minmax(260px, 1fr))";
        let style_gap = b"gap";
        let style_gap_val = b"16px";
        let style_pad = b"padding";
        let style_pad_val = b"40px";
        let style_opacity = b"opacity";
        let style_zero = b"0";
        let style_pos = b"position";
        let style_abs = b"absolute";
        let style_pe = b"pointer-events";
        let style_none = b"none";

        unsafe {
            dom_set_style_hybrid(state.container_id, style_display.as_ptr(), 7, style_grid.as_ptr(), 4);
            dom_set_style_hybrid(state.container_id, style_gtc.as_ptr(), 22, style_gtc_val.as_ptr(), 37);
            dom_set_style_hybrid(state.container_id, style_gap.as_ptr(), 3, style_gap_val.as_ptr(), 4);
            dom_set_style_hybrid(state.container_id, style_pad.as_ptr(), 7, style_pad_val.as_ptr(), 4);
            // Invisible but accessible — screen readers read it, crawlers index it
            dom_set_style_hybrid(state.container_id, style_opacity.as_ptr(), 7, style_zero.as_ptr(), 1);
            dom_set_style_hybrid(state.container_id, style_pos.as_ptr(), 8, style_abs.as_ptr(), 8);
            dom_set_style_hybrid(state.container_id, style_pe.as_ptr(), 14, style_none.as_ptr(), 4);
        }

        // Create 10K product card DOM elements
        let tag_div = b"div";
        let tag_img = b"img";
        let attr_src = b"src";
        let attr_class = b"class";
        let class_card = b"product-card";
        let attr_loading = b"loading";
        let attr_lazy = b"lazy";

        state.card_dom_ids.reserve(10000);

        for i in 0u32..10000 {
            let card_id = unsafe { dom_create_element(tag_div.as_ptr(), 3) };

            // Set class for CSS styling
            unsafe { dom_set_attr_hybrid(card_id, attr_class.as_ptr(), 5, class_card.as_ptr(), 12); }

            // Set a fixed height style so the browser can compute layout
            let style_h = b"height";
            let style_h_val = b"310px";
            let style_w = b"width";
            let style_w_val = b"260px";
            unsafe {
                dom_set_style_hybrid(card_id, style_h.as_ptr(), 6, style_h_val.as_ptr(), 5);
                dom_set_style_hybrid(card_id, style_w.as_ptr(), 5, style_w_val.as_ptr(), 5);
            }

            // Product name as text content (for Cmd+F search + screen readers)
            let mut name_buf = [0u8; 32];
            let name_len = {
                let mut cursor = std::io::Cursor::new(&mut name_buf[..]);
                let _ = std::io::Write::write_fmt(&mut cursor, format_args!("Product #{}", i));
                cursor.position() as usize
            };
            unsafe { dom_set_text_hybrid(card_id, name_buf.as_ptr(), name_len as u32); }

            // Image src attribute
            let mut src_buf = [0u8; 32];
            let src_len = {
                let mut cursor = std::io::Cursor::new(&mut src_buf[..]);
                let _ = std::io::Write::write_fmt(&mut cursor, format_args!("img/p{}.jpg", i));
                cursor.position() as usize
            };
            unsafe { dom_set_attr_hybrid(card_id, attr_src.as_ptr(), 3, src_buf.as_ptr(), src_len as u32); }

            unsafe { dom_append_child_hybrid(state.container_id, card_id); }
            state.card_dom_ids.push(card_id);
        }

        state.card_rects.resize(10000, [0.0; 4]);
    });
}

/// Phase 2: Read layout positions from the hidden DOM.
/// Browser CSS engine already computed the grid layout.
#[no_mangle]
pub extern "C" fn hybrid_read_layout() {
    HYBRID.with(|h| {
        let mut state = h.borrow_mut();
        let mut rect_buf = [0.0f32; 4];
        let dom_ids: Vec<u32> = state.card_dom_ids.clone();
        for (i, dom_id) in dom_ids.iter().enumerate() {
            unsafe { dom_get_rect(*dom_id, rect_buf.as_mut_ptr()); }
            state.card_rects[i] = rect_buf;
        }
    });
}

/// Phase 3: Paint to canvas using DOM-computed positions.
#[no_mangle]
pub extern "C" fn hybrid_render() {
    HYBRID.with(|h| {
        let state = h.borrow();
        let id = state.canvas_id;
        let vw = state.vw;
        let vh = state.vh;
        let sy = state.scroll_y;

        unsafe {
            canvas_clear(id);
            canvas_fill_rect(id, 0.0, 0.0, vw, vh, 11, 14, 20, 255);

            // Header
            canvas_fill_rect(id, 0.0, 0.0, vw, 60.0, 19, 23, 32, 255);
            let title = b"Nectar Hybrid Mode";
            canvas_fill_text(id, title.as_ptr(), title.len() as u32, vw/2.0 - 120.0, 38.0, 249, 115, 22, 24.0, 1);
            let sub = b"Hidden DOM layout + Canvas paint + Full SEO";
            canvas_fill_text(id, sub.as_ptr(), sub.len() as u32, vw/2.0 - 180.0, 55.0, 139, 148, 158, 12.0, 0);

            // Paint product cards at DOM-computed positions
            let mut drawn = 0u32;
            for i in 0..state.card_rects.len() {
                let [x, y_raw, w, h_val] = state.card_rects[i];
                if w == 0.0 { continue; } // not laid out yet
                let y = y_raw - sy;

                if y + h_val < 60.0 || y > vh { continue; }
                drawn += 1;

                // Card background
                canvas_round_rect(id, x, y, w, h_val, 12.0, 26, 31, 46, 255);
                canvas_stroke_rect(id, x, y, w, h_val, 42, 47, 62, 255, 1.0);

                // Image
                let mut src_buf = [0u8; 32];
                let src_len = {
                    let mut c = std::io::Cursor::new(&mut src_buf[..]);
                    let _ = std::io::Write::write_fmt(&mut c, format_args!("img/p{}.jpg", i));
                    c.position() as usize
                };
                canvas_draw_image(id, src_buf.as_ptr(), src_len as u32, x, y, w, 180.0, 12.0);

                // Stock badge
                let si = i % 10;
                let stock: &[u8] = if si == 0 { b"OUT OF STOCK" } else if si == 5 { b"LOW STOCK" } else { b"IN STOCK" };
                canvas_fill_rect(id, x, y + 162.0, w, 18.0, 63, 185, 80, 220);
                canvas_fill_text(id, stock.as_ptr(), stock.len() as u32, x + 8.0, y + 175.0, 0, 0, 0, 9.0, 1);

                // Name
                let mut name_buf = [0u8; 32];
                let name_len = {
                    let mut c = std::io::Cursor::new(&mut name_buf[..]);
                    let _ = std::io::Write::write_fmt(&mut c, format_args!("Product #{}", i));
                    c.position() as usize
                };
                canvas_fill_text(id, name_buf.as_ptr(), name_len as u32, x + 14.0, y + 200.0, 230, 237, 243, 13.0, 1);

                // Category
                let cat = CATS[i % 5].as_bytes();
                canvas_fill_text(id, cat.as_ptr(), cat.len() as u32, x + 14.0, y + 216.0, 110, 118, 129, 10.0, 0);

                // Stars
                let stars = "★★★★☆";
                canvas_fill_text(id, stars.as_ptr(), stars.len() as u32, x + 14.0, y + 234.0, 249, 115, 22, 12.0, 0);

                // Price
                let pc = (i as u32 * 7 + 499) % 10000;
                let mut price_buf = [0u8; 16];
                let price_len = {
                    let mut c = std::io::Cursor::new(&mut price_buf[..]);
                    let _ = std::io::Write::write_fmt(&mut c, format_args!("${}.{:02}", pc/100, pc%100));
                    c.position() as usize
                };
                canvas_fill_text(id, price_buf.as_ptr(), price_len as u32, x + 14.0, y + 258.0, 63, 185, 80, 18.0, 1);

                // Add to Cart button
                canvas_round_rect(id, x + 14.0, y + 268.0, w - 28.0, 30.0, 8.0, 249, 115, 22, 255);
                let btn = b"Add to Cart";
                canvas_fill_text(id, btn.as_ptr(), 11, x + w/2.0 - 32.0, y + 288.0, 0, 0, 0, 12.0, 1);
            }

            // ── Apple-style scrollbar ────────────────────
            let content_h = if !state.card_rects.is_empty() {
                let last = state.card_rects.last().unwrap();
                last[1] + last[3] + 40.0
            } else { vh };

            if content_h > vh {
                let track_h = vh - 70.0; // below header
                let thumb_h = (vh / content_h * track_h).max(40.0);
                let thumb_y = 65.0 + (sy / (content_h - vh)) * (track_h - thumb_h);
                let thumb_x = vw - 10.0;

                // Thin rounded thumb — Apple style
                let alpha = if state.scrollbar_opacity > 0.0 { (state.scrollbar_opacity * 180.0) as u32 } else { 80 };
                canvas_round_rect(id, thumb_x, thumb_y, 6.0, thumb_h, 3.0, 255, 255, 255, alpha);
            }

            // Stats
            canvas_fill_rect(id, vw - 320.0, vh - 26.0, 312.0, 20.0, 11, 14, 20, 220);
            let mut fbuf = [0u8; 80];
            let flen = {
                let mut c = std::io::Cursor::new(&mut fbuf[..]);
                let _ = std::io::Write::write_fmt(&mut c, format_args!("{} visible | hybrid DOM+canvas | scroll {}", drawn, sy as u32));
                c.position() as usize
            };
            canvas_fill_text(id, fbuf.as_ptr(), flen as u32, vw - 314.0, vh - 12.0, 110, 118, 129, 11.0, 0);
        }
    });
}

#[no_mangle]
pub extern "C" fn hybrid_scroll(delta: f32) {
    HYBRID.with(|h| {
        let mut state = h.borrow_mut();
        let content_h = if !state.card_rects.is_empty() {
            let last = state.card_rects.last().unwrap();
            last[1] + last[3] + 40.0
        } else { state.vh };
        let max = (content_h - state.vh).max(0.0);
        state.scroll_y = (state.scroll_y + delta).clamp(0.0, max);
        state.scrollbar_opacity = 1.0;
        state.scrollbar_fade_frame = 0;
    });
}

#[no_mangle]
pub extern "C" fn hybrid_resize(vw: f32, vh: f32) {
    HYBRID.with(|h| {
        let mut state = h.borrow_mut();
        state.vw = vw;
        state.vh = vh;
    });
}

#[no_mangle]
pub extern "C" fn hybrid_click(mx: f32, my: f32) {
    HYBRID.with(|h| {
        let mut state = h.borrow_mut();
        let sy = state.scroll_y;
        for i in 0..state.card_rects.len() {
            let [x, y_raw, w, h_val] = state.card_rects[i];
            let y = y_raw - sy;
            // Add to Cart button
            if mx >= x+14.0 && mx <= x+w-14.0 && my >= y+268.0 && my <= y+298.0 {
                state.cart_count += 1;
                state.signal_fires += 1;
                return;
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn hybrid_cursor(mx: f32, my: f32) -> u32 {
    HYBRID.with(|h| {
        let state = h.borrow();
        let sy = state.scroll_y;
        for i in 0..state.card_rects.len() {
            let [x, y_raw, w, _] = state.card_rects[i];
            let y = y_raw - sy;
            if mx >= x+14.0 && mx <= x+w-14.0 && my >= y+268.0 && my <= y+298.0 {
                return 1; // pointer
            }
        }
        0
    })
}

#[no_mangle]
pub extern "C" fn hybrid_set_timings(t_dom: f32, t_layout: f32) {
    HYBRID.with(|h| {
        let mut state = h.borrow_mut();
        state.t_dom = t_dom;
        state.t_layout = t_layout;
    });
}
