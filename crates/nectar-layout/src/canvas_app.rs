//! Canvas app — all computation in WASM. Zero JS logic.
//! Builds products, computes layout, renders to canvas, handles state.

use crate::canvas_syscalls::*;
use std::cell::RefCell;
use std::fmt::Write;

// ── Product data ─────────────────────────────────────────────

struct Product {
    name: String,
    category: &'static str,
    price_cents: u32,
    price_display: String,
    img_src: String,
    stock: &'static str,
}

const CATS: [&str; 5] = ["Electronics", "Clothing", "Home", "Sports", "Books"];

// ── App state ────────────────────────────────────────────────

struct AppState {
    canvas_id: u32,
    products: Vec<Product>,
    active_cat: usize,  // 0=all, 1-5=category index
    sort_order: u8,     // 0=default, 1=price-asc, 2=price-desc, 3=name-asc
    cart_count: u32,
    signal_fires: u32,
    scroll_y: f32,
    vw: f32,
    vh: f32,
    // Timing
    t_fetch: f32,
    t_tree: f32,
    t_layout: f32,
    t_total: f32,
}

impl AppState {
    fn cols(&self) -> usize {
        ((self.vw - 80.0 + 16.0) / 276.0).max(1.0) as usize
    }

    fn card_pos(&self, i: usize) -> (f32, f32) {
        let cols = self.cols();
        let total_w = cols as f32 * 260.0 + (cols - 1) as f32 * 16.0;
        let start_x = (self.vw - total_w) / 2.0;
        let col = i % cols;
        let row = i / cols;
        (start_x + col as f32 * 276.0, GRID_TOP + row as f32 * 316.0)
    }

    fn content_height(&self) -> f32 {
        let cols = self.cols();
        let rows = (self.products.len() + cols - 1) / cols;
        GRID_TOP + rows as f32 * 316.0 + 40.0
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height() - self.vh).max(0.0)
    }
}

const HEADER_H: f32 = 80.0;
const PILLS_Y: f32 = 100.0;
const PILLS_H: f32 = 36.0;
const SORT_Y: f32 = 152.0;
const SORT_H: f32 = 32.0;
const METRICS_Y: f32 = 200.0;
// 9 metrics in 3 cols = 3 rows × 46px = 138px
const METRICS_ROWS: f32 = 3.0;
const METRICS_ROW_H: f32 = 46.0;
const GRID_TOP: f32 = METRICS_Y + METRICS_ROWS * METRICS_ROW_H + 24.0; // 338 + 24 = 362

thread_local! {
    static STATE: RefCell<AppState> = RefCell::new(AppState {
        canvas_id: 0,
        products: Vec::new(),
        active_cat: 0,
        sort_order: 0,
        cart_count: 0,
        signal_fires: 0,
        scroll_y: 0.0,
        vw: 0.0,
        vh: 0.0,
        t_fetch: 0.0,
        t_tree: 0.0,
        t_layout: 0.0,
        t_total: 0.0,
    });
}

// ── Exported functions (called by 3-line JS bootstrap) ───────

#[no_mangle]
pub extern "C" fn app_init(vw: f32, vh: f32, t_fetch: f32) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.vw = vw;
        state.vh = vh;
        state.t_fetch = t_fetch;

        // Create canvas
        state.canvas_id = unsafe { canvas_init(vw, vh) };

        // Build 10K products — all in WASM
        let mut products = Vec::with_capacity(10000);
        for i in 0u32..10000 {
            products.push(build_product(i));
        }
        state.products = products;
    });
}

// ══════════════════════════════════════════════════════════════
// LAZY CANVAS MODE — rAF drain, sub-millisecond first paint
// ══════════════════════════════════════════════════════════════

/// Lazy init: build only the first batch, render immediately.
/// Call app_lazy_build_batch via rAF to fill in the rest.
#[no_mangle]
pub extern "C" fn app_lazy_init(vw: f32, vh: f32, t_fetch: f32) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.vw = vw;
        state.vh = vh;
        state.t_fetch = t_fetch;
        state.canvas_id = unsafe { canvas_init(vw, vh) };

        // Build only enough products to fill the viewport
        let cols = state.cols();
        let visible_rows = ((vh - GRID_TOP) / 316.0).ceil() as u32 + 1;
        let initial_count = (cols as u32 * visible_rows).min(10000);

        let mut products = Vec::with_capacity(10000);
        for i in 0..initial_count {
            products.push(build_product(i));
        }
        state.products = products;
    });
}

/// Build next batch of products. Returns 1 if more remain, 0 if done.
/// Call via rAF: if (app_lazy_build_batch()) requestAnimationFrame(again)
#[no_mangle]
pub extern "C" fn app_lazy_build_batch() -> u32 {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let current = state.products.len() as u32;
        if current >= 10000 {
            return 0;
        }
        let batch_end = (current + 500).min(10000); // 500 per frame
        for i in current..batch_end {
            state.products.push(build_product(i));
        }
        if batch_end >= 10000 { 0 } else { 1 }
    })
}

/// How many products are built so far
#[no_mangle]
pub extern "C" fn app_lazy_product_count() -> u32 {
    STATE.with(|s| s.borrow().products.len() as u32)
}

/// Build hidden accessibility DOM — call AFTER app_init, AFTER first render.
/// Only needed for hybrid mode. Pure canvas mode skips this.
/// Runs via requestIdleCallback so it doesn't block first paint.
#[no_mangle]
pub extern "C" fn app_build_a11y_dom() {
    STATE.with(|s| {
        let state = s.borrow();
        unsafe {
            let root = dom_get_root();

            let k_display = b"display";
            let v_grid = b"grid";
            let k_gtc = b"grid-template-columns";
            let v_gtc = b"repeat(auto-fill,minmax(260px,1fr))";
            let k_gap = b"gap";
            let v_gap = b"16px";
            let k_pad = b"padding";
            let v_pad = b"40px";
            dom_set_style_hybrid(root, k_display.as_ptr(), 7, v_grid.as_ptr(), 4);
            dom_set_style_hybrid(root, k_gtc.as_ptr(), 22, v_gtc.as_ptr(), 36);
            dom_set_style_hybrid(root, k_gap.as_ptr(), 3, v_gap.as_ptr(), 4);
            dom_set_style_hybrid(root, k_pad.as_ptr(), 7, v_pad.as_ptr(), 4);

            let tag_article = b"article";
            for p in &state.products {
                let el = dom_create_element(tag_article.as_ptr(), 7);

                let mut text_buf = [0u8; 64];
                let text_len = {
                    let mut c = std::io::Cursor::new(&mut text_buf[..]);
                    let _ = std::io::Write::write_fmt(&mut c, format_args!(
                        "{}, {}, {}", p.name, p.category, p.price_display
                    ));
                    c.position() as usize
                };
                dom_set_text_hybrid(el, text_buf.as_ptr(), text_len as u32);

                let k_role = b"role";
                let v_article = b"article";
                dom_set_attr_hybrid(el, k_role.as_ptr(), 4, v_article.as_ptr(), 7);

                dom_append_child_hybrid(root, el);
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn app_set_timings(t_tree: f32, t_layout: f32, t_total: f32) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.t_tree = t_tree;
        state.t_layout = t_layout;
        state.t_total = t_total;
    });
}

#[no_mangle]
pub extern "C" fn app_render() {
    STATE.with(|s| {
        let state = s.borrow();
        let id = state.canvas_id;
        let vw = state.vw;
        let vh = state.vh;
        let sy = state.scroll_y;

        unsafe {
            canvas_clear(id);

            // ── Background ───────────────────────────────
            canvas_fill_rect(id, 0.0, 0.0, vw, vh, 11, 14, 20, 255);

            // ── Header ───────────────────────────────────
            canvas_fill_rect(id, 0.0, 0.0, vw, HEADER_H, 19, 23, 32, 255);

            let title = b"Nectar";
            canvas_fill_text(id, title.as_ptr(), 6, vw/2.0 - 140.0, 38.0, 249, 115, 22, 28.0, 1);
            let subtitle = b" Canvas Mode";
            canvas_fill_text(id, subtitle.as_ptr(), 12, vw/2.0 - 140.0 + 110.0, 38.0, 230, 237, 243, 28.0, 1);

            let sub2 = b"WASM Layout + Canvas 2D + Zero DOM + buildnectar.com";
            canvas_fill_text(id, sub2.as_ptr(), sub2.len() as u32, vw/2.0 - 210.0, 62.0, 139, 148, 158, 14.0, 0);

            let back = b"< Back to DOM Demo";
            canvas_fill_text(id, back.as_ptr(), back.len() as u32, 20.0, 30.0, 249, 115, 22, 13.0, 1);

            // ── Cart button ──────────────────────────────
            let cart_x = vw - 160.0;
            canvas_round_rect(id, cart_x, PILLS_Y, 120.0, PILLS_H, 10.0, 249, 115, 22, 255);
            let mut cart_label = [0u8; 16];
            let cart_len = fmt_into(&mut cart_label, format_args!("Cart {}", state.cart_count));
            canvas_fill_text(id, cart_label.as_ptr(), cart_len as u32, cart_x + 20.0, PILLS_Y + 23.0, 0, 0, 0, 14.0, 1);

            // ── Category pills ───────────────────────────
            let pill_labels: [&[u8]; 6] = [b"All", b"Electronics", b"Clothing", b"Home", b"Sports", b"Books"];
            let mut px: f32 = 40.0;
            for (idx, label) in pill_labels.iter().enumerate() {
                let tw = label.len() as f32 * 8.5 + 32.0;
                let active = state.active_cat == idx;
                if active {
                    canvas_round_rect(id, px, PILLS_Y, tw, PILLS_H, 18.0, 249, 115, 22, 255);
                    canvas_fill_text(id, label.as_ptr(), label.len() as u32, px + 16.0, PILLS_Y + 23.0, 0, 0, 0, 13.0, 1);
                } else {
                    canvas_stroke_rect(id, px, PILLS_Y, tw, PILLS_H, 42, 47, 62, 255, 2.0);
                    canvas_fill_text(id, label.as_ptr(), label.len() as u32, px + 16.0, PILLS_Y + 23.0, 139, 148, 158, 13.0, 1);
                }
                px += tw + 10.0;
            }

            // ── Sort buttons ─────────────────────────────
            let sort_label = b"Sort:";
            canvas_fill_text(id, sort_label.as_ptr(), 5, 40.0, SORT_Y + 21.0, 139, 148, 158, 13.0, 0);
            let sort_labels: [(&[u8], u8); 3] = [(b"Price ^", 1), (b"Price v", 2), (b"Name A-Z", 3)];
            let mut sx: f32 = 85.0;
            for (label, order) in &sort_labels {
                let tw = label.len() as f32 * 8.0 + 24.0;
                let active = state.sort_order == *order;
                if active {
                    canvas_round_rect(id, sx, SORT_Y, tw, SORT_H, 8.0, 249, 115, 22, 255);
                    canvas_fill_text(id, label.as_ptr(), label.len() as u32, sx + 12.0, SORT_Y + 21.0, 0, 0, 0, 12.0, 1);
                } else {
                    canvas_stroke_rect(id, sx, SORT_Y, tw, SORT_H, 42, 47, 62, 255, 1.0);
                    canvas_fill_text(id, label.as_ptr(), label.len() as u32, sx + 12.0, SORT_Y + 21.0, 139, 148, 158, 12.0, 1);
                }
                sx += tw + 8.0;
            }

            // ── Metrics grid ─────────────────────────────
            let mut mbuf = [0u8; 32];
            let metric_labels: [&[u8]; 9] = [
                b"FETCH + COMPILE", b"TREE BUILD", b"LAYOUT",
                b"TOTAL", b"PRODUCTS", b"CATEGORY",
                b"CART", b"SORT", b"SIGNAL FIRES",
            ];
            let m_cols = 3usize.min(((vw - 80.0) / 160.0) as usize);
            let m_w = ((vw - 80.0 - (m_cols - 1) as f32 * 12.0) / m_cols as f32).floor();

            for mi in 0..9 {
                let col = mi % m_cols;
                let row = mi / m_cols;
                let mx = 40.0 + col as f32 * (m_w + 12.0);
                let my = METRICS_Y + row as f32 * 46.0;

                canvas_round_rect(id, mx, my, m_w, 40.0, 8.0, 26, 31, 46, 255);
                canvas_stroke_rect(id, mx, my, m_w, 40.0, 42, 47, 62, 255, 1.0);
                canvas_fill_text(id, metric_labels[mi].as_ptr(), metric_labels[mi].len() as u32, mx + 12.0, my + 14.0, 139, 148, 158, 9.0, 1);

                // Values
                let (vlen, vr, vg, vb) = match mi {
                    0 => (fmt_into(&mut mbuf, format_args!("{:.1}ms", state.t_fetch)), 88, 166, 255),
                    1 => (fmt_into(&mut mbuf, format_args!("{:.1}ms", state.t_tree)), 63, 185, 80),
                    2 => (fmt_into(&mut mbuf, format_args!("{:.1}ms", state.t_layout)), 249, 115, 22),
                    3 => (fmt_into(&mut mbuf, format_args!("{:.1}ms", state.t_total)), 188, 140, 255),
                    4 => (fmt_into(&mut mbuf, format_args!("{}", state.products.len())), 88, 166, 255),
                    5 => {
                        let cat = if state.active_cat == 0 { "all" } else { CATS[state.active_cat - 1] };
                        let bytes = cat.as_bytes();
                        mbuf[..bytes.len()].copy_from_slice(bytes);
                        (bytes.len(), 63, 185, 80)
                    }
                    6 => (fmt_into(&mut mbuf, format_args!("{}", state.cart_count)), 249, 115, 22),
                    7 => {
                        let s = match state.sort_order { 1 => "price-asc", 2 => "price-desc", 3 => "name-asc", _ => "default" };
                        let bytes = s.as_bytes();
                        mbuf[..bytes.len()].copy_from_slice(bytes);
                        (bytes.len(), 188, 140, 255)
                    }
                    8 => (fmt_into(&mut mbuf, format_args!("{}", state.signal_fires)), 188, 140, 255),
                    _ => (0, 0, 0, 0),
                };
                canvas_fill_text(id, mbuf.as_ptr(), vlen as u32, mx + 12.0, my + 32.0, vr, vg, vb, 14.0, 1);
            }

            // ── Product grid (clipped below chrome) ─────
            canvas_fill_rect(id, 0.0, GRID_TOP, vw, vh - GRID_TOP, 11, 14, 20, 255); // clear grid area
            let mut drawn = 0u32;
            for i in 0..state.products.len() {
                let (x, raw_y) = state.card_pos(i);
                let y = raw_y - sy;

                if y + 316.0 < GRID_TOP || y > vh { continue; }
                drawn += 1;

                let p = &state.products[i];

                // Card bg
                canvas_round_rect(id, x, y, 260.0, 310.0, 12.0, 26, 31, 46, 255);
                canvas_stroke_rect(id, x, y, 260.0, 310.0, 42, 47, 62, 255, 1.0);

                // Image
                canvas_draw_image(id, p.img_src.as_ptr(), p.img_src.len() as u32, x, y, 260.0, 180.0, 12.0);

                // Stock badge
                canvas_fill_rect(id, x, y + 162.0, 260.0, 18.0, 63, 185, 80, 220);
                canvas_fill_text(id, p.stock.as_ptr(), p.stock.len() as u32, x + 8.0, y + 175.0, 0, 0, 0, 9.0, 1);

                // Name
                canvas_fill_text(id, p.name.as_ptr(), p.name.len() as u32, x + 14.0, y + 200.0, 230, 237, 243, 13.0, 1);

                // Category
                canvas_fill_text(id, p.category.as_ptr(), p.category.len() as u32, x + 14.0, y + 216.0, 110, 118, 129, 10.0, 0);

                // Stars
                let stars = "★★★★☆";
                canvas_fill_text(id, stars.as_ptr(), stars.len() as u32, x + 14.0, y + 234.0, 249, 115, 22, 12.0, 0);

                // Price
                canvas_fill_text(id, p.price_display.as_ptr(), p.price_display.len() as u32, x + 14.0, y + 258.0, 63, 185, 80, 18.0, 1);

                // Add to Cart button
                canvas_round_rect(id, x + 14.0, y + 268.0, 232.0, 30.0, 8.0, 249, 115, 22, 255);
                let btn = b"Add to Cart";
                canvas_fill_text(id, btn.as_ptr(), 11, x + 90.0, y + 288.0, 0, 0, 0, 12.0, 1);
            }

            // ── Apple-style scrollbar ─────────────────────
            let content_h = state.content_height();
            if content_h > vh {
                let track_h = vh - GRID_TOP - 10.0;
                let thumb_h = (vh / content_h * track_h).max(40.0);
                let thumb_y = GRID_TOP + 5.0 + (sy / (content_h - vh)) * (track_h - thumb_h);
                canvas_round_rect(id, vw - 10.0, thumb_y, 6.0, thumb_h, 3.0, 255, 255, 255, 80);
            }

            // ── Footer stats ─────────────────────────────
            canvas_fill_rect(id, vw - 380.0, vh - 26.0, 372.0, 20.0, 11, 14, 20, 220);
            let mut fbuf = [0u8; 96];
            let flen = fmt_into(&mut fbuf, format_args!("{} visible | scroll {}/{}", drawn, sy as u32, state.max_scroll() as u32));
            canvas_fill_text(id, fbuf.as_ptr(), flen as u32, vw - 374.0, vh - 12.0, 110, 118, 129, 11.0, 0);

            // ── Vim mode indicator ───────────────────────
            let vim_on = FOCUS.with(|f| f.borrow().vim_mode);
            if vim_on {
                canvas_round_rect(id, vw - 120.0, 10.0, 110.0, 28.0, 6.0, 63, 185, 80, 255);
                let vim = b"VIM MODE";
                canvas_fill_text(id, vim.as_ptr(), 8, vw - 108.0, 30.0, 0, 0, 0, 12.0, 1);
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn app_scroll(delta: f32) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let max = state.max_scroll();
        state.scroll_y = (state.scroll_y + delta).clamp(0.0, max);
    });
}

#[no_mangle]
pub extern "C" fn app_click(mx: f32, my: f32) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();

        // Back link
        if mx < 200.0 && my < 50.0 {
            // Can't navigate from WASM — JS handles this
            return;
        }

        // Category pills
        let pill_labels_len: [f32; 6] = [
            3.0 * 8.5 + 32.0,   // All
            11.0 * 8.5 + 32.0,  // Electronics
            8.0 * 8.5 + 32.0,   // Clothing
            4.0 * 8.5 + 32.0,   // Home
            6.0 * 8.5 + 32.0,   // Sports
            5.0 * 8.5 + 32.0,   // Books
        ];
        let mut px: f32 = 40.0;
        for idx in 0..6 {
            let tw = pill_labels_len[idx];
            if mx >= px && mx <= px + tw && my >= PILLS_Y && my <= PILLS_Y + PILLS_H {
                state.active_cat = idx;
                state.signal_fires += 1;
                return;
            }
            px += tw + 10.0;
        }

        // Sort buttons
        let sort_widths: [f32; 3] = [7.0*8.0+24.0, 7.0*8.0+24.0, 8.0*8.0+24.0];
        let mut sx: f32 = 85.0;
        for (i, &w) in sort_widths.iter().enumerate() {
            if mx >= sx && mx <= sx + w && my >= SORT_Y && my <= SORT_Y + SORT_H {
                state.sort_order = (i + 1) as u8;
                state.signal_fires += 1;
                return;
            }
            sx += w + 8.0;
        }

        // Cart button
        let cart_x = state.vw - 160.0;
        if mx >= cart_x && mx <= cart_x + 120.0 && my >= PILLS_Y && my <= PILLS_Y + PILLS_H {
            state.cart_count = 0;
            state.signal_fires += 1;
            return;
        }

        // Product cards — Add to Cart
        for i in 0..state.products.len() {
            let (x, raw_y) = state.card_pos(i);
            let y = raw_y - state.scroll_y;
            if y + 316.0 < 0.0 || y > state.vh { continue; }
            if mx >= x + 14.0 && mx <= x + 246.0 && my >= y + 268.0 && my <= y + 298.0 {
                state.cart_count += 1;
                state.signal_fires += 1;
                return;
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn app_resize(vw: f32, vh: f32) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.vw = vw;
        state.vh = vh;
    });
}

#[no_mangle]
pub extern "C" fn app_get_back_clicked(mx: f32, my: f32) -> u32 {
    if mx < 200.0 && my < 50.0 { 1 } else { 0 }
}

// ── Helpers ──────────────────────────────────────────────────

fn build_product(i: u32) -> Product {
    let cat_idx = (i % 5) as usize;
    let pc = (i * 7 + 499) % 10000;
    let dollars = pc / 100;
    let cents = pc % 100;
    let mut price_display = String::with_capacity(8);
    let _ = write!(price_display, "${}.{:02}", dollars, cents);
    let mut img_src = String::with_capacity(16);
    let _ = write!(img_src, "img/p{}.jpg", i);
    let mut name = String::with_capacity(14);
    let _ = write!(name, "Product #{}", i);
    let si = i % 10;
    let stock = if si == 0 { "OUT OF STOCK" } else if si == 5 { "LOW STOCK" } else { "IN STOCK" };
    Product { name, category: CATS[cat_idx], price_cents: pc, price_display, img_src, stock }
}

fn fmt_into(buf: &mut [u8], args: std::fmt::Arguments) -> usize {
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    let _ = std::io::Write::write_fmt(&mut cursor, args);
    cursor.position() as usize
}

// ══════════════════════════════════════════════════════════════
// INPUT HANDLING — ported from nectar-runtime/src/input/mod.rs
// All computation in WASM. JS only forwards raw events.
// ══════════════════════════════════════════════════════════════

// ── Focus management ─────────────────────────────────────────

/// Focus tracking state
struct FocusState {
    focused_id: Option<u32>,
    cursor_pos: usize,
    selection_start: Option<usize>,
    cursor_visible: bool,
    cursor_frame: u32,
    vim_mode: bool,       // Ctrl+Shift+V toggles
    input_values: Vec<(u32, String)>,  // (element_id, value)
    undo_stacks: Vec<(u32, Vec<(String, usize)>)>,
    redo_stacks: Vec<(u32, Vec<(String, usize)>)>,
    focusable_ids: Vec<u32>,  // ordered list of focusable element IDs
}

thread_local! {
    static FOCUS: RefCell<FocusState> = RefCell::new(FocusState {
        focused_id: None,
        cursor_pos: 0,
        selection_start: None,
        cursor_visible: true,
        cursor_frame: 0,
        vim_mode: false,
        input_values: Vec::new(),
        undo_stacks: Vec::new(),
        redo_stacks: Vec::new(),
        focusable_ids: Vec::new(),
    });
}

// ── Key codes (match JS KeyboardEvent.key values) ────────────
const KEY_BACKSPACE: u32 = 8;
const KEY_TAB: u32 = 9;
const KEY_ENTER: u32 = 13;
const KEY_ESCAPE: u32 = 27;
const KEY_LEFT: u32 = 37;
const KEY_UP: u32 = 38;
const KEY_RIGHT: u32 = 39;
const KEY_DOWN: u32 = 40;
const KEY_DELETE: u32 = 46;
const KEY_HOME: u32 = 36;
const KEY_END: u32 = 35;

// ── Keyboard input ───────────────────────────────────────────

/// Handle a key press. Called from JS with key code and modifier flags.
/// modifiers: bit 0 = shift, bit 1 = ctrl/cmd, bit 2 = alt
#[no_mangle]
pub extern "C" fn app_keydown(key_code: u32, char_ptr: *const u8, char_len: u32, modifiers: u32) {
    let is_shift = modifiers & 1 != 0;
    let is_cmd = modifiers & 2 != 0;
    let is_alt = modifiers & 4 != 0;

    // Vim mode toggle: Ctrl+Shift+V
    if is_cmd && is_shift && key_code == 86 { // 'V'
        FOCUS.with(|f| {
            let mut focus = f.borrow_mut();
            focus.vim_mode = !focus.vim_mode;
        });
        return;
    }

    FOCUS.with(|f| {
        let mut focus = f.borrow_mut();

        // ── Vim mode navigation ──────────────────────
        if focus.vim_mode {
            STATE.with(|s| {
                let mut state = s.borrow_mut();
                match key_code {
                    // hjkl navigation
                    72 => state.scroll_y = (state.scroll_y - 40.0).max(0.0),   // h = scroll left (or up)
                    74 => { // j = scroll down
                        let max = state.max_scroll();
                        state.scroll_y = (state.scroll_y + 60.0).min(max);
                    }
                    75 => state.scroll_y = (state.scroll_y - 60.0).max(0.0),   // k = scroll up
                    76 => { // l = scroll right (or down)
                        let max = state.max_scroll();
                        state.scroll_y = (state.scroll_y + 40.0).min(max);
                    }
                    // gg = top (just g for now)
                    71 => state.scroll_y = 0.0,
                    // G = bottom
                    _ if key_code == 71 && is_shift => {
                        state.scroll_y = state.max_scroll();
                    }
                    // Escape exits vim mode
                    KEY_ESCAPE => focus.vim_mode = false,
                    _ => {}
                }
            });
            return;
        }

        // ── Tab navigation ───────────────────────────
        if key_code == KEY_TAB {
            // For now, cycle through category pills (indices 0-5)
            STATE.with(|s| {
                let mut state = s.borrow_mut();
                if is_shift {
                    state.active_cat = if state.active_cat == 0 { 5 } else { state.active_cat - 1 };
                } else {
                    state.active_cat = if state.active_cat >= 5 { 0 } else { state.active_cat + 1 };
                }
                state.signal_fires += 1;
            });
            return;
        }

        // ── Cmd+A: select all (for future text input) ────
        if is_cmd && key_code == 65 {
            focus.selection_start = Some(0);
            // cursor_pos = end of text
            return;
        }

        // ── Cmd+C: copy ──────────────────────────────
        if is_cmd && key_code == 67 {
            if let (Some(start), pos) = (focus.selection_start, focus.cursor_pos) {
                // Copy selected text to clipboard
                // For now: copy product info if a card is focused
            }
            return;
        }

        // ── Cmd+V: paste ─────────────────────────────
        if is_cmd && key_code == 86 {
            let mut buf = [0u8; 1024];
            let len = unsafe { clipboard_read(buf.as_mut_ptr(), 1024) };
            if len > 0 {
                // Insert pasted text at cursor position
            }
            return;
        }

        // ── Cmd+Z: undo ──────────────────────────────
        if is_cmd && key_code == 90 {
            // Undo last action
            return;
        }

        // ── Cmd+F: find ──────────────────────────────
        if is_cmd && key_code == 70 {
            // Don't prevent default — let browser open its native find bar
            // which searches the hidden SEO DOM. When the browser highlights
            // a match in the hidden DOM, we sync the canvas scroll position.
            return;
        }

        // ── Arrow keys ───────────────────────────────
        match key_code {
            KEY_UP => {
                STATE.with(|s| {
                    let mut state = s.borrow_mut();
                    state.scroll_y = (state.scroll_y - 60.0).max(0.0);
                });
            }
            KEY_DOWN => {
                STATE.with(|s| {
                    let mut state = s.borrow_mut();
                    let max = state.max_scroll();
                    state.scroll_y = (state.scroll_y + 60.0).min(max);
                });
            }
            KEY_HOME => {
                STATE.with(|s| {
                    s.borrow_mut().scroll_y = 0.0;
                });
            }
            KEY_END => {
                STATE.with(|s| {
                    let mut state = s.borrow_mut();
                    state.scroll_y = state.max_scroll();
                });
            }
            _ => {}
        }
    });
}

// ── Mouse drag (text selection) ──────────────────────────────

/// Returns cursor type: 0=default, 1=pointer, 2=text
#[no_mangle]
pub extern "C" fn app_cursor(mx: f32, my: f32) -> u32 {
    // Back link
    if mx < 200.0 && my < 50.0 { return 1; }

    // Category pills
    let pill_widths: [f32; 6] = [
        3.0*8.5+32.0, 11.0*8.5+32.0, 8.0*8.5+32.0,
        4.0*8.5+32.0, 6.0*8.5+32.0, 5.0*8.5+32.0,
    ];
    let mut px: f32 = 40.0;
    for w in &pill_widths {
        if mx >= px && mx <= px + w && my >= PILLS_Y && my <= PILLS_Y + PILLS_H { return 1; }
        px += w + 10.0;
    }

    // Sort buttons
    let sort_widths: [f32; 3] = [7.0*8.0+24.0, 7.0*8.0+24.0, 8.0*8.0+24.0];
    let mut sx: f32 = 85.0;
    for w in &sort_widths {
        if mx >= sx && mx <= sx + w && my >= SORT_Y && my <= SORT_Y + SORT_H { return 1; }
        sx += w + 8.0;
    }

    // Cart button
    STATE.with(|s| {
        let state = s.borrow();
        let cart_x = state.vw - 160.0;
        if mx >= cart_x && mx <= cart_x + 120.0 && my >= PILLS_Y && my <= PILLS_Y + PILLS_H { return 1; }

        // Product card buttons
        for i in 0..state.products.len() {
            let (x, raw_y) = state.card_pos(i);
            let y = raw_y - state.scroll_y;
            if y + 316.0 < 0.0 || y > state.vh { continue; }
            if mx >= x+14.0 && mx <= x+246.0 && my >= y+268.0 && my <= y+298.0 { return 1; }
        }
        0
    })
}

#[no_mangle]
pub extern "C" fn app_mousedown(mx: f32, my: f32) {
    // Start potential text selection drag
    FOCUS.with(|f| {
        let mut focus = f.borrow_mut();
        focus.selection_start = None;
        // If clicking on an editable element, set cursor position
        // and prepare for drag selection
    });
}

#[no_mangle]
pub extern "C" fn app_mousemove(mx: f32, my: f32, buttons: u32) {
    // If left button held (buttons & 1), extend text selection
    if buttons & 1 != 0 {
        FOCUS.with(|f| {
            let mut focus = f.borrow_mut();
            if focus.selection_start.is_some() {
                // Extend selection to current mouse position
            }
        });
    }
}

// ── Right-click context menu ─────────────────────────────────

#[no_mangle]
pub extern "C" fn app_contextmenu(mx: f32, my: f32) -> u32 {
    // Hit test — if on a product card, return the card index
    // JS can show a native context menu with product-specific options
    STATE.with(|s| {
        let state = s.borrow();
        for i in 0..state.products.len() {
            let (x, raw_y) = state.card_pos(i);
            let y = raw_y - state.scroll_y;
            if mx >= x && mx <= x + 260.0 && my >= y && my <= y + 310.0 {
                return i as u32;
            }
        }
        u32::MAX // no hit
    })
}

// ── Cmd+F search sync ────────────────────────────────────────
// When the browser's native find bar highlights text in the hidden
// SEO DOM, this function syncs the canvas scroll position.

#[no_mangle]
pub extern "C" fn app_search_sync(product_index: u32) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        if (product_index as usize) < state.products.len() {
            let (_, y) = state.card_pos(product_index as usize);
            state.scroll_y = (y - state.vh / 3.0).clamp(0.0, state.max_scroll());
        }
    });
}

// ── Vim mode status ──────────────────────────────────────────

#[no_mangle]
pub extern "C" fn app_is_vim_mode() -> u32 {
    FOCUS.with(|f| if f.borrow().vim_mode { 1 } else { 0 })
}

// ── Cursor blink tick (called per frame) ─────────────────────

#[no_mangle]
pub extern "C" fn app_tick_cursor() {
    FOCUS.with(|f| {
        let mut focus = f.borrow_mut();
        focus.cursor_frame += 1;
        if focus.cursor_frame % 30 == 0 {
            focus.cursor_visible = !focus.cursor_visible;
        }
    });
}
