//! Canvas app — all computation in WASM. Zero JS logic.
//! Uses nectar-layout element tree + layout engine. No hardcoded positions.

use crate::canvas_syscalls::*;
use crate::element::ElementTree;
use crate::layout;
use crate::measure::EstimateMeasurer;
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

fn build_product(i: u32) -> Product {
    let cat_idx = (i % 5) as usize;
    let pc = (i * 7 + 499) % 10000;
    let mut price_display = String::with_capacity(8);
    let _ = write!(price_display, "${}.{:02}", pc / 100, pc % 100);
    let mut img_src = String::with_capacity(16);
    let _ = write!(img_src, "img/p{}.jpg", i);
    let mut name = String::with_capacity(14);
    let _ = write!(name, "Product #{}", i);
    let si = i % 10;
    let stock = if si == 0 { "OUT OF STOCK" } else if si == 5 { "LOW STOCK" } else { "IN STOCK" };
    Product { name, category: CATS[cat_idx], price_cents: pc, price_display, img_src, stock }
}

// ── App state ────────────────────────────────────────────────

struct AppState {
    canvas_id: u32,
    products: Vec<Product>,
    tree: ElementTree,
    measurer: EstimateMeasurer,
    active_cat: usize,
    sort_order: u8,
    cart_count: u32,
    signal_fires: u32,
    scroll_y: f32,
    vw: f32,
    vh: f32,
    t_fetch: f32,
    t_tree: f32,
    t_layout: f32,
    t_total: f32,
    // Element IDs for key UI parts
    root_id: u32,
    grid_id: u32,
    card_ids: Vec<u32>,     // product card container IDs
    name_ids: Vec<u32>,     // product name text element IDs
    cat_ids: Vec<u32>,      // product category text element IDs
    price_ids: Vec<u32>,    // product price text element IDs
    img_ids: Vec<u32>,      // product image element IDs
    // Text selection
    sel_element: i32,       // element ID being selected (-1 = none)
    sel_start_char: u32,
    sel_end_char: u32,
    sel_dragging: bool,
}

thread_local! {
    static STATE: RefCell<Option<AppState>> = RefCell::new(None);
}

fn with_state<F, R>(f: F) -> R where F: FnOnce(&mut AppState) -> R {
    STATE.with(|s| {
        let mut opt = s.borrow_mut();
        f(opt.as_mut().expect("app not initialized"))
    })
}

// ── Helper: create styled element ────────────────────────────

fn add_el(tree: &mut ElementTree, tag: &str, parent: u32) -> u32 {
    let id = tree.create(tag);
    tree.append_child(parent, id);
    id
}

fn set_style(tree: &mut ElementTree, id: u32, prop: &str, val: &str) {
    if let Some(el) = tree.get_mut(id) {
        el.styles.insert(prop.into(), val.into());
    }
}

fn set_text(tree: &mut ElementTree, id: u32, text: &str) {
    if let Some(el) = tree.get_mut(id) {
        el.text = Some(text.into());
    }
}

// ── Build UI tree ────────────────────────────────────────────

fn build_ui(state: &mut AppState) {
    let tree = &mut state.tree;
    let root = tree.root_id(); // ID 1

    // Root: vertical stack, full width
    set_style(tree, root, "direction", "vertical");
    set_style(tree, root, "width", &format!("{}px", state.vw));
    set_style(tree, root, "gap", "16");
    set_style(tree, root, "padding", "20");
    state.root_id = root;

    // ── Header ───────────────────────────────────
    let header = add_el(tree, "div", root);
    set_style(tree, header, "direction", "vertical");
    set_style(tree, header, "padding", "16");
    set_style(tree, header, "background-color", "#131720");

    let title = add_el(tree, "div", header);
    set_text(tree, title, "Nectar Canvas Mode");
    set_style(tree, title, "font-size", "24px");
    set_style(tree, title, "font-weight", "bold");
    set_style(tree, title, "color", "#f97316");

    let subtitle = add_el(tree, "div", header);
    set_text(tree, subtitle, "WASM Layout Engine · Canvas 2D · Zero DOM");
    set_style(tree, subtitle, "font-size", "12px");
    set_style(tree, subtitle, "color", "#8b949e");

    // ── Category pills row ───────────────────────
    let pills_row = add_el(tree, "div", root);
    set_style(tree, pills_row, "direction", "horizontal");
    set_style(tree, pills_row, "gap", "10");
    set_style(tree, pills_row, "wrap", "true");

    let pill_labels = ["All", "Electronics", "Clothing", "Home", "Sports", "Books"];
    for label in &pill_labels {
        let pill = add_el(tree, "div", pills_row);
        set_text(tree, pill, label);
        set_style(tree, pill, "font-size", "13px");
        set_style(tree, pill, "padding", "8");
        set_style(tree, pill, "width", "hug");
        set_style(tree, pill, "height", "hug");
    }

    // ── Sort row ─────────────────────────────────
    let sort_row = add_el(tree, "div", root);
    set_style(tree, sort_row, "direction", "horizontal");
    set_style(tree, sort_row, "gap", "8");

    let sort_label = add_el(tree, "div", sort_row);
    set_text(tree, sort_label, "Sort:");
    set_style(tree, sort_label, "font-size", "13px");
    set_style(tree, sort_label, "color", "#8b949e");
    set_style(tree, sort_label, "width", "hug");
    set_style(tree, sort_label, "height", "hug");

    for label in &["Price ^", "Price v", "Name A-Z"] {
        let btn = add_el(tree, "div", sort_row);
        set_text(tree, btn, label);
        set_style(tree, btn, "font-size", "12px");
        set_style(tree, btn, "padding", "6");
        set_style(tree, btn, "width", "hug");
        set_style(tree, btn, "height", "hug");
    }

    // ── Metrics grid ─────────────────────────────
    let metrics = add_el(tree, "div", root);
    set_style(tree, metrics, "direction", "horizontal");
    set_style(tree, metrics, "gap", "12");
    set_style(tree, metrics, "wrap", "true");

    let metric_labels = ["FETCH + COMPILE", "TREE BUILD", "LAYOUT", "TOTAL", "PRODUCTS", "CATEGORY", "CART", "SORT", "SIGNAL FIRES"];
    for label in &metric_labels {
        let card = add_el(tree, "div", metrics);
        set_style(tree, card, "direction", "vertical");
        set_style(tree, card, "width", "200px");
        set_style(tree, card, "height", "40px");
        set_style(tree, card, "padding", "8");
        set_style(tree, card, "background-color", "#1a1f2e");

        let lbl = add_el(tree, "div", card);
        set_text(tree, lbl, label);
        set_style(tree, lbl, "font-size", "9px");
        set_style(tree, lbl, "color", "#8b949e");
        set_style(tree, lbl, "height", "hug");

        let val = add_el(tree, "div", card);
        set_text(tree, val, "—");
        set_style(tree, val, "font-size", "14px");
        set_style(tree, val, "font-weight", "bold");
        set_style(tree, val, "color", "#58a6ff");
        set_style(tree, val, "height", "hug");
    }

    // ── Product grid ─────────────────────────────
    let grid = add_el(tree, "div", root);
    set_style(tree, grid, "direction", "horizontal");
    set_style(tree, grid, "gap", "16");
    set_style(tree, grid, "wrap", "true");
    state.grid_id = grid;

    state.card_ids.clear();
    state.name_ids.clear();
    state.cat_ids.clear();
    state.price_ids.clear();
    state.img_ids.clear();

    for i in 0..state.products.len() {
        let p = &state.products[i];

        let card = add_el(tree, "div", grid);
        set_style(tree, card, "direction", "vertical");
        set_style(tree, card, "width", "260px");
        set_style(tree, card, "height", "310px");
        set_style(tree, card, "background-color", "#1a1f2e");
        state.card_ids.push(card);

        // Image placeholder
        let img = add_el(tree, "div", card);
        set_style(tree, img, "height", "180px");
        if let Some(el) = tree.get_mut(img) {
            el.attributes.insert("src".into(), p.img_src.clone());
        }
        state.img_ids.push(img);

        // Name
        let name_el = add_el(tree, "div", card);
        set_text(tree, name_el, &p.name);
        set_style(tree, name_el, "font-size", "13px");
        set_style(tree, name_el, "font-weight", "bold");
        set_style(tree, name_el, "color", "#e6edf3");
        set_style(tree, name_el, "padding", "4");
        set_style(tree, name_el, "height", "hug");
        state.name_ids.push(name_el);

        // Category
        let cat_el = add_el(tree, "div", card);
        set_text(tree, cat_el, p.category);
        set_style(tree, cat_el, "font-size", "10px");
        set_style(tree, cat_el, "color", "#6e7681");
        set_style(tree, cat_el, "height", "hug");
        state.cat_ids.push(cat_el);

        // Price
        let price_el = add_el(tree, "div", card);
        set_text(tree, price_el, &p.price_display);
        set_style(tree, price_el, "font-size", "18px");
        set_style(tree, price_el, "font-weight", "bold");
        set_style(tree, price_el, "color", "#3fb950");
        set_style(tree, price_el, "padding", "4");
        set_style(tree, price_el, "height", "hug");
        state.price_ids.push(price_el);

        // Add to Cart button
        let btn = add_el(tree, "div", card);
        set_text(tree, btn, "Add to Cart");
        set_style(tree, btn, "font-size", "12px");
        set_style(tree, btn, "font-weight", "bold");
        set_style(tree, btn, "background-color", "#f97316");
        set_style(tree, btn, "color", "#000000");
        set_style(tree, btn, "padding", "8");
        set_style(tree, btn, "height", "30px");
        set_style(tree, btn, "align", "center");
    }
}

// ── Init ─────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn app_init(vw: f32, vh: f32, t_fetch: f32) {
    let canvas_id = unsafe { canvas_init(vw, vh) };

    let mut products = Vec::with_capacity(10000);
    for i in 0u32..10000 {
        products.push(build_product(i));
    }

    let mut state = AppState {
        canvas_id,
        products,
        tree: ElementTree::new(),
        measurer: EstimateMeasurer,
        active_cat: 0,
        sort_order: 0,
        cart_count: 0,
        signal_fires: 0,
        scroll_y: 0.0,
        vw, vh,
        t_fetch, t_tree: 0.0, t_layout: 0.0, t_total: 0.0,
        root_id: 1,
        grid_id: 0,
        card_ids: Vec::new(),
        name_ids: Vec::new(),
        cat_ids: Vec::new(),
        price_ids: Vec::new(),
        img_ids: Vec::new(),
        sel_element: -1,
        sel_start_char: 0,
        sel_end_char: 0,
        sel_dragging: false,
    };

    build_ui(&mut state);

    // Run layout
    layout::compute(&mut state.tree, vw, vh * 200.0, &mut state.measurer);

    STATE.with(|s| { *s.borrow_mut() = Some(state); });
}

// ── Lazy init (rAF drain) ────────────────────────────────────

#[no_mangle]
pub extern "C" fn app_lazy_init(vw: f32, vh: f32, t_fetch: f32) {
    // For lazy mode, init with fewer products — caller uses app_lazy_build_batch
    let canvas_id = unsafe { canvas_init(vw, vh) };

    let cols = ((vw - 80.0 + 16.0) / 276.0).max(1.0) as u32;
    let visible_rows = ((vh - 300.0) / 316.0).ceil() as u32 + 1;
    let initial_count = (cols * visible_rows).min(10000);

    let mut products = Vec::with_capacity(10000);
    for i in 0..initial_count {
        products.push(build_product(i));
    }

    let mut state = AppState {
        canvas_id,
        products,
        tree: ElementTree::new(),
        measurer: EstimateMeasurer,
        active_cat: 0, sort_order: 0, cart_count: 0, signal_fires: 0,
        scroll_y: 0.0, vw, vh,
        t_fetch, t_tree: 0.0, t_layout: 0.0, t_total: 0.0,
        root_id: 1, grid_id: 0,
        card_ids: Vec::new(), name_ids: Vec::new(), cat_ids: Vec::new(),
        price_ids: Vec::new(), img_ids: Vec::new(),
        sel_element: -1, sel_start_char: 0, sel_end_char: 0, sel_dragging: false,
    };

    build_ui(&mut state);
    layout::compute(&mut state.tree, vw, vh * 200.0, &mut state.measurer);
    STATE.with(|s| { *s.borrow_mut() = Some(state); });
}

#[no_mangle]
pub extern "C" fn app_lazy_build_batch() -> u32 {
    with_state(|state| {
        let current = state.products.len() as u32;
        if current >= 10000 { return 0; }
        let batch_end = (current + 500).min(10000);
        for i in current..batch_end {
            state.products.push(build_product(i));
        }
        // Rebuild UI with new products
        state.tree = ElementTree::new();
        build_ui(state);
        layout::compute(&mut state.tree, state.vw, state.vh * 200.0, &mut state.measurer);
        if batch_end >= 10000 { 0 } else { 1 }
    })
}

#[no_mangle]
pub extern "C" fn app_lazy_product_count() -> u32 {
    with_state(|state| state.products.len() as u32)
}

#[no_mangle]
pub extern "C" fn app_set_timings(t_tree: f32, t_layout: f32, t_total: f32) {
    with_state(|state| {
        state.t_tree = t_tree;
        state.t_layout = t_layout;
        state.t_total = t_total;
    });
}

// ── Render: walk tree, paint to canvas ───────────────────────

#[no_mangle]
pub extern "C" fn app_render() {
    with_state(|state| {
        let id = state.canvas_id;
        let vw = state.vw;
        let vh = state.vh;
        let sy = state.scroll_y;

        unsafe {
            canvas_clear(id);
            canvas_fill_rect(id, 0.0, 0.0, vw, vh, 11, 14, 20, 255);

            // Walk tree depth-first, paint each element
            render_node(id, &state.tree, state.root_id, sy, vw, vh, &state.products, &state.img_ids);
        }
    });
}

/// Recursive tree renderer — paints each element at its layout position
unsafe fn render_node(
    cvs: u32,
    tree: &ElementTree,
    node_id: u32,
    sy: f32,
    vw: f32,
    vh: f32,
    products: &[Product],
    img_ids: &[u32],
) {
    let el = match tree.get(node_id) {
        Some(e) => e,
        None => return,
    };

    let x = el.layout.x;
    let y = el.layout.y - sy;
    let w = el.layout.width;
    let h = el.layout.height;

    // Cull off-screen
    if y + h < -50.0 || y > vh + 50.0 { return; }

    // Background
    if let Some(bg) = el.styles.get("background-color") {
        let (r, g, b) = parse_hex_color(bg);
        canvas_round_rect(cvs, x, y, w, h, 8.0, r, g, b, 255);
    }

    // Border (for metric cards)
    if el.styles.get("background-color").is_some() && w > 50.0 {
        canvas_stroke_rect(cvs, x, y, w, h, 42, 47, 62, 255, 1.0);
    }

    // Image (if element has src attribute)
    if let Some(src) = el.attributes.get("src") {
        canvas_draw_image(cvs, src.as_ptr(), src.len() as u32, x, y, w, h, 8.0);

        // Stock badge (for product images)
        if h > 100.0 {
            // Find which product this image belongs to
            for (i, &img_id) in img_ids.iter().enumerate() {
                if img_id == node_id && i < products.len() {
                    let stock = products[i].stock;
                    canvas_fill_rect(cvs, x, y + h - 18.0, w, 18.0, 63, 185, 80, 220);
                    canvas_fill_text(cvs, stock.as_ptr(), stock.len() as u32, x + 8.0, y + h - 5.0, 0, 0, 0, 9.0, 1);
                    break;
                }
            }
        }
    }

    // Text
    if let Some(ref text) = el.text {
        let font_size = el.styles.get("font-size")
            .and_then(|v| v.trim_end_matches("px").parse::<f32>().ok())
            .unwrap_or(14.0);
        let bold = el.styles.get("font-weight").map(|v| v == "bold").unwrap_or(false);
        let (cr, cg, cb) = el.styles.get("color")
            .map(|c| parse_hex_color(c))
            .unwrap_or((230, 237, 243));

        let text_y = y + el.layout.padding.top + font_size;
        let text_x = x + el.layout.padding.left;
        canvas_fill_text(cvs, text.as_ptr(), text.len() as u32, text_x, text_y, cr, cg, cb, font_size, if bold { 1 } else { 0 });
    }

    // Recurse children
    for &child_id in &el.children {
        render_node(cvs, tree, child_id, sy, vw, vh, products, img_ids);
    }
}

fn parse_hex_color(hex: &str) -> (u32, u32, u32) {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u32::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u32::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u32::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        (r, g, b)
    } else {
        (200, 200, 200)
    }
}

// ── Scrollbar ────────────────────────────────────────────────

// Rendered after the tree walk in app_render — TODO: add Apple-style scrollbar

// ── Events ───────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn app_scroll(delta: f32) {
    with_state(|state| {
        let root = state.tree.get(state.root_id);
        let content_h = root.map(|e| e.layout.height).unwrap_or(state.vh);
        let max = (content_h - state.vh).max(0.0);
        state.scroll_y = (state.scroll_y + delta).clamp(0.0, max);
    });
}

#[no_mangle]
pub extern "C" fn app_click(mx: f32, my: f32) {
    with_state(|state| {
        let hit = state.tree.hit_test(mx, my + state.scroll_y);
        if let Some(_hit_id) = hit {
            // TODO: dispatch click handlers
            state.signal_fires += 1;
        }
    });
}

#[no_mangle]
pub extern "C" fn app_resize(vw: f32, vh: f32) {
    with_state(|state| {
        state.vw = vw;
        state.vh = vh;
        // Re-layout
        set_style(&mut state.tree, state.root_id, "width", &format!("{}px", vw));
        layout::compute(&mut state.tree, vw, vh * 200.0, &mut state.measurer);
    });
}

#[no_mangle]
pub extern "C" fn app_get_back_clicked(mx: f32, my: f32) -> u32 {
    if mx < 200.0 && my < 50.0 { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn app_cursor(mx: f32, my: f32) -> u32 {
    if mx < 200.0 && my < 50.0 { return 1; } // back link
    with_state(|state| {
        let hit = state.tree.hit_test(mx, my + state.scroll_y);
        if let Some(hit_id) = hit {
            if let Some(el) = state.tree.get(hit_id) {
                if el.text.is_some() { return 2; } // text cursor
                if el.styles.get("background-color").map(|c| c == "#f97316").unwrap_or(false) { return 1; } // button
            }
        }
        0
    })
}

#[no_mangle]
pub extern "C" fn app_mousedown(mx: f32, my: f32, _click_count: u32) {
    // TODO: text selection using tree hit test
}

#[no_mangle]
pub extern "C" fn app_mousemove(mx: f32, my: f32, buttons: u32) {
    // TODO: extend text selection
}

#[no_mangle]
pub extern "C" fn app_mouseup(_mx: f32, _my: f32) {
    with_state(|state| { state.sel_dragging = false; });
}

#[no_mangle]
pub extern "C" fn app_get_selection(buf_ptr: *mut u8, buf_cap: u32) -> u32 { 0 }

#[no_mangle]
pub extern "C" fn app_search_sync(product_index: u32) {
    // TODO: scroll to product
}

#[no_mangle]
pub extern "C" fn app_build_a11y_dom() {
    // TODO: build hidden accessibility DOM
}

#[no_mangle]
pub extern "C" fn app_contextmenu(_mx: f32, _my: f32) -> u32 { u32::MAX }

#[no_mangle]
pub extern "C" fn app_keydown(_key_code: u32, _char_ptr: *const u8, _char_len: u32, _modifiers: u32) {}

#[no_mangle]
pub extern "C" fn app_is_vim_mode() -> u32 { 0 }

#[no_mangle]
pub extern "C" fn app_debug_sel() -> i32 { -1 }
