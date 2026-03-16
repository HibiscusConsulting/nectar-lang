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
    pill_ids: Vec<u32>,     // category pill element IDs (6)
    sort_ids: Vec<u32>,     // sort button element IDs (3)
    cart_text_id: u32,      // cart button text element ID
    metric_val_ids: Vec<u32>, // metric value element IDs (9)
    card_ids: Vec<u32>,     // product card container IDs
    name_ids: Vec<u32>,     // product name text element IDs
    cat_ids: Vec<u32>,      // product category text element IDs
    price_ids: Vec<u32>,    // product price text element IDs
    img_ids: Vec<u32>,      // product image element IDs
    // Pending navigation (set by click, read by JS)
    nav_url: Option<String>,
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
    let root = tree.root_id();

    // Root: vertical, full width, hug height, centered children
    set_style(tree, root, "direction", "vertical");
    set_style(tree, root, "width", &format!("{}px", state.vw));
    set_style(tree, root, "height", "hug");
    set_style(tree, root, "gap", "16");
    set_style(tree, root, "padding", "24");
    set_style(tree, root, "align", "center");
    state.root_id = root;

    // ══ Header: "Demo App" + subtitle ══
    let header = add_el(tree, "div", root);
    set_style(tree, header, "direction", "vertical");
    set_style(tree, header, "height", "hug");
    set_style(tree, header, "align", "center");
    set_style(tree, header, "padding", "16");

    let app_title = add_el(tree, "div", header);
    set_text(tree, app_title, "Demo App");
    set_style(tree, app_title, "font-size", "28px");
    set_style(tree, app_title, "font-weight", "bold");
    set_style(tree, app_title, "color", "#e6edf3");
    set_style(tree, app_title, "height", "hug");
    set_style(tree, app_title, "width", "hug");

    let app_sub = add_el(tree, "div", header);
    set_text(tree, app_sub, "WebAssembly + Signals — buildnectar.com");
    set_style(tree, app_sub, "font-size", "13px");
    set_style(tree, app_sub, "color", "#8b949e");
    set_style(tree, app_sub, "height", "hug");
    set_style(tree, app_sub, "width", "hug");

    // ══ Nav tabs ══
    let nav = add_el(tree, "div", root);
    set_style(tree, nav, "direction", "horizontal");
    set_style(tree, nav, "gap", "8");
    set_style(tree, nav, "wrap", "true");
    set_style(tree, nav, "height", "hug");
    set_style(tree, nav, "justify", "center");

    let nav_tabs = [
        ("Home", "/", "#3fb950"),
        ("E-Commerce", "/app/", "#f97316"),
        ("Canvas*", "#", "#3fb950"),
        ("React 18", "/app/react.html", "#58a6ff"),
        ("Svelte 5", "/app/svelte.html", "#ff3e00"),
    ];
    for (label, _href, color) in &nav_tabs {
        let tab = add_el(tree, "div", nav);
        set_text(tree, tab, label);
        set_style(tree, tab, "font-size", "13px");
        set_style(tree, tab, "font-weight", "bold");
        set_style(tree, tab, "padding", "10");
        set_style(tree, tab, "width", "hug");
        set_style(tree, tab, "height", "hug");
        set_style(tree, tab, "border-radius", "10");
        set_style(tree, tab, "border", &format!("2px solid {}", color));
        set_style(tree, tab, "color", color);
    }

    // ══ Section card (the big rounded container, max-width centered) ══
    let section = add_el(tree, "div", root);
    set_style(tree, section, "direction", "vertical");
    set_style(tree, section, "height", "hug");
    set_style(tree, section, "max-width", "1400px");
    set_style(tree, section, "padding", "32");
    set_style(tree, section, "gap", "20");
    set_style(tree, section, "background-color", "#131720");
    set_style(tree, section, "border-radius", "16");

    // ── Title row: "E-Commerce Product Grid" + badge ──
    let title_row = add_el(tree, "div", section);
    set_style(tree, title_row, "direction", "horizontal");
    set_style(tree, title_row, "gap", "12");
    set_style(tree, title_row, "height", "hug");
    set_style(tree, title_row, "align", "center");

    let title = add_el(tree, "div", title_row);
    set_text(tree, title, "E-Commerce Product Grid");
    set_style(tree, title, "font-size", "26px");
    set_style(tree, title, "font-weight", "bold");
    set_style(tree, title, "color", "#f97316");
    set_style(tree, title, "height", "hug");
    set_style(tree, title, "width", "hug");

    let badge = add_el(tree, "div", title_row);
    set_text(tree, badge, "per-card signals");
    set_style(tree, badge, "font-size", "11px");
    set_style(tree, badge, "font-weight", "bold");
    set_style(tree, badge, "color", "#3fb950");
    set_style(tree, badge, "background-color", "#1a2e1a");
    set_style(tree, badge, "padding", "4");
    set_style(tree, badge, "height", "hug");
    set_style(tree, badge, "width", "hug");
    set_style(tree, badge, "border-radius", "6");

    // ── Controls row: search + cart + clear ──
    let controls = add_el(tree, "div", section);
    set_style(tree, controls, "direction", "horizontal");
    set_style(tree, controls, "gap", "12");
    set_style(tree, controls, "height", "hug");
    set_style(tree, controls, "align", "center");

    let search = add_el(tree, "div", controls);
    set_text(tree, search, "Search products...");
    set_style(tree, search, "font-size", "14px");
    set_style(tree, search, "color", "#6e7681");
    set_style(tree, search, "background-color", "#1a1f2e");
    set_style(tree, search, "padding", "12");
    set_style(tree, search, "height", "44px");
    set_style(tree, search, "border-radius", "10");

    let cart_btn = add_el(tree, "div", controls);
    let mut cart_label_buf = [0u8; 16];
    let cart_label_len = {
        let mut c = std::io::Cursor::new(&mut cart_label_buf[..]);
        let _ = std::io::Write::write_fmt(&mut c, format_args!("Cart {}", state.cart_count));
        c.position() as usize
    };
    let cart_text = std::str::from_utf8(&cart_label_buf[..cart_label_len]).unwrap_or("Cart 0");
    set_text(tree, cart_btn, cart_text);
    state.cart_text_id = cart_btn;
    set_style(tree, cart_btn, "font-size", "14px");
    set_style(tree, cart_btn, "font-weight", "bold");
    set_style(tree, cart_btn, "color", "#000000");
    set_style(tree, cart_btn, "background-color", "#f97316");
    set_style(tree, cart_btn, "padding", "12");
    set_style(tree, cart_btn, "height", "44px");
    set_style(tree, cart_btn, "width", "120px");
    set_style(tree, cart_btn, "border-radius", "10");

    let clear_btn = add_el(tree, "div", controls);
    set_text(tree, clear_btn, "Clear");
    set_style(tree, clear_btn, "font-size", "14px");
    set_style(tree, clear_btn, "font-weight", "bold");
    set_style(tree, clear_btn, "color", "#8b949e");
    set_style(tree, clear_btn, "background-color", "#1a1f2e");
    set_style(tree, clear_btn, "padding", "12");
    set_style(tree, clear_btn, "height", "44px");
    set_style(tree, clear_btn, "width", "80px");
    set_style(tree, clear_btn, "border-radius", "10");

    // ── Category pills ──
    let pills_row = add_el(tree, "div", section);
    set_style(tree, pills_row, "direction", "horizontal");
    set_style(tree, pills_row, "gap", "10");
    set_style(tree, pills_row, "wrap", "true");
    set_style(tree, pills_row, "height", "hug");

    state.pill_ids.clear();
    let pill_labels = ["All", "Electronics", "Clothing", "Home", "Sports", "Books"];
    for (idx, label) in pill_labels.iter().enumerate() {
        let pill = add_el(tree, "div", pills_row);
        state.pill_ids.push(pill);
        set_text(tree, pill, label);
        set_style(tree, pill, "font-size", "13px");
        set_style(tree, pill, "font-weight", "bold");
        set_style(tree, pill, "padding", "8");
        set_style(tree, pill, "width", "hug");
        set_style(tree, pill, "height", "36px");
        set_style(tree, pill, "border-radius", "20");
        if idx == state.active_cat {
            set_style(tree, pill, "background-color", "#f97316");
            set_style(tree, pill, "color", "#000000");
        } else {
            set_style(tree, pill, "color", "#8b949e");
            set_style(tree, pill, "border", "2px solid #2a2f3e");
        }
    }

    // ── Sort row ──
    let sort_row = add_el(tree, "div", section);
    set_style(tree, sort_row, "direction", "horizontal");
    set_style(tree, sort_row, "gap", "8");
    set_style(tree, sort_row, "height", "hug");
    set_style(tree, sort_row, "align", "center");

    let sort_label = add_el(tree, "div", sort_row);
    set_text(tree, sort_label, "Sort:");
    set_style(tree, sort_label, "font-size", "13px");
    set_style(tree, sort_label, "color", "#8b949e");
    set_style(tree, sort_label, "width", "hug");
    set_style(tree, sort_label, "height", "hug");

    state.sort_ids.clear();
    for (i, label) in ["Price ↑", "Price ↓", "Name A→Z"].iter().enumerate() {
        let btn = add_el(tree, "div", sort_row);
        state.sort_ids.push(btn);
        set_text(tree, btn, label);
        set_style(tree, btn, "font-size", "12px");
        set_style(tree, btn, "font-weight", "bold");
        set_style(tree, btn, "padding", "8");
        set_style(tree, btn, "width", "hug");
        set_style(tree, btn, "height", "32px");
        set_style(tree, btn, "border-radius", "8");
        if state.sort_order == (i + 1) as u8 {
            set_style(tree, btn, "background-color", "#f97316");
            set_style(tree, btn, "color", "#000000");
        } else {
            set_style(tree, btn, "color", "#8b949e");
            set_style(tree, btn, "border", "1px solid #2a2f3e");
        }
    }

    // ── Metrics grid (3 columns via wrap) ──
    let metrics = add_el(tree, "div", section);
    set_style(tree, metrics, "direction", "horizontal");
    set_style(tree, metrics, "gap", "12");
    set_style(tree, metrics, "wrap", "true");
    set_style(tree, metrics, "height", "hug");

    state.metric_val_ids.clear();
    let metric_defs: [(&str, &str, &str); 9] = [
        ("FETCH + COMPILE", "—", "#58a6ff"),
        ("TREE BUILD", "—", "#3fb950"),
        ("LAYOUT", "—", "#f97316"),
        ("TOTAL", "—", "#bc8cff"),
        ("PRODUCTS", "10000", "#58a6ff"),
        ("CATEGORY", "all", "#3fb950"),
        ("CART", "0", "#f97316"),
        ("SORT", "default", "#bc8cff"),
        ("SIGNAL FIRES", "0", "#bc8cff"),
    ];
    // Width = (section_width - padding*2 - gap*2) / 3
    let metric_w = ((state.vw - 64.0 - 24.0) / 3.0).floor().max(100.0);
    for (label, value, color) in &metric_defs {
        let card = add_el(tree, "div", metrics);
        set_style(tree, card, "direction", "vertical");
        set_style(tree, card, "width", &format!("{}px", metric_w));
        set_style(tree, card, "height", "80px");
        set_style(tree, card, "padding", "16");
        set_style(tree, card, "background-color", "#1a1f2e");
        set_style(tree, card, "border-radius", "12");
        set_style(tree, card, "align", "center");

        let lbl = add_el(tree, "div", card);
        set_text(tree, lbl, label);
        set_style(tree, lbl, "font-size", "10px");
        set_style(tree, lbl, "font-weight", "bold");
        set_style(tree, lbl, "color", "#8b949e");
        set_style(tree, lbl, "height", "hug");

        let val = add_el(tree, "div", card);
        set_text(tree, val, value);
        set_style(tree, val, "font-size", "22px");
        set_style(tree, val, "font-weight", "bold");
        set_style(tree, val, "color", color);
        set_style(tree, val, "height", "hug");
        state.metric_val_ids.push(val);
    }

    // ── Product grid ──
    let grid = add_el(tree, "div", section);
    set_style(tree, grid, "direction", "horizontal");
    set_style(tree, grid, "gap", "16");
    set_style(tree, grid, "wrap", "true");
    set_style(tree, grid, "height", "hug");
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
        set_style(tree, card, "height", "340px");
        set_style(tree, card, "background-color", "#1a1f2e");
        set_style(tree, card, "border-radius", "12");
        state.card_ids.push(card);

        // Image
        let img = add_el(tree, "div", card);
        set_style(tree, img, "height", "180px");
        set_style(tree, img, "border-radius", "12");
        if let Some(el) = tree.get_mut(img) {
            el.attributes.insert("src".into(), p.img_src.clone());
        }
        state.img_ids.push(img);

        // Body (below image)
        let body = add_el(tree, "div", card);
        set_style(tree, body, "direction", "vertical");
        set_style(tree, body, "padding", "12");
        set_style(tree, body, "gap", "4");
        set_style(tree, body, "height", "hug");

        // Name
        let name_el = add_el(tree, "div", body);
        set_text(tree, name_el, &p.name);
        set_style(tree, name_el, "font-size", "14px");
        set_style(tree, name_el, "font-weight", "bold");
        set_style(tree, name_el, "color", "#e6edf3");
        set_style(tree, name_el, "height", "hug");
        state.name_ids.push(name_el);

        // Category
        let cat_el = add_el(tree, "div", body);
        set_text(tree, cat_el, p.category);
        set_style(tree, cat_el, "font-size", "10px");
        set_style(tree, cat_el, "color", "#6e7681");
        set_style(tree, cat_el, "height", "hug");
        state.cat_ids.push(cat_el);

        // Stars
        let stars_el = add_el(tree, "div", body);
        set_text(tree, stars_el, "★★★★☆");
        set_style(tree, stars_el, "font-size", "12px");
        set_style(tree, stars_el, "color", "#f97316");
        set_style(tree, stars_el, "height", "hug");

        // Price
        let price_el = add_el(tree, "div", body);
        set_text(tree, price_el, &p.price_display);
        set_style(tree, price_el, "font-size", "20px");
        set_style(tree, price_el, "font-weight", "bold");
        set_style(tree, price_el, "color", "#3fb950");
        set_style(tree, price_el, "height", "hug");
        state.price_ids.push(price_el);

        // Add to Cart button
        let btn = add_el(tree, "div", body);
        set_text(tree, btn, "Add to Cart");
        set_style(tree, btn, "font-size", "13px");
        set_style(tree, btn, "font-weight", "bold");
        set_style(tree, btn, "background-color", "#f97316");
        set_style(tree, btn, "color", "#000000");
        set_style(tree, btn, "padding", "10");
        set_style(tree, btn, "height", "36px");
        set_style(tree, btn, "border-radius", "8");
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
        pill_ids: Vec::new(),
        sort_ids: Vec::new(),
        cart_text_id: 0,
        metric_val_ids: Vec::new(),
        card_ids: Vec::new(),
        name_ids: Vec::new(),
        cat_ids: Vec::new(),
        price_ids: Vec::new(),
        img_ids: Vec::new(),
        nav_url: None,
        sel_element: -1,
        sel_start_char: 0,
        sel_end_char: 0,
        sel_dragging: false,
    };

    build_ui(&mut state);

    // Run layout
    layout::compute(&mut state.tree, vw, 999999.0, &mut state.measurer);

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
        pill_ids: Vec::new(), sort_ids: Vec::new(), cart_text_id: 0, metric_val_ids: Vec::new(),
        card_ids: Vec::new(), name_ids: Vec::new(), cat_ids: Vec::new(),
        price_ids: Vec::new(), img_ids: Vec::new(),
        nav_url: None,
        sel_element: -1, sel_start_char: 0, sel_end_char: 0, sel_dragging: false,
    };

    build_ui(&mut state);
    layout::compute(&mut state.tree, vw, 999999.0, &mut state.measurer);
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
        layout::compute(&mut state.tree, state.vw, state.vh, &mut state.measurer);
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

        // Update metric value elements — O(1) per metric
        if state.metric_val_ids.len() >= 9 {
            let mut buf = [0u8; 32];

            // 0: FETCH + COMPILE
            let len = fmt_buf(&mut buf, format_args!("{:.2}ms", state.t_fetch));
            set_text(&mut state.tree, state.metric_val_ids[0], std::str::from_utf8(&buf[..len]).unwrap_or("?"));

            // 1: TREE BUILD
            let len = fmt_buf(&mut buf, format_args!("{:.2}ms", t_tree));
            set_text(&mut state.tree, state.metric_val_ids[1], std::str::from_utf8(&buf[..len]).unwrap_or("?"));

            // 2: LAYOUT
            let len = fmt_buf(&mut buf, format_args!("{:.2}ms", t_layout));
            set_text(&mut state.tree, state.metric_val_ids[2], std::str::from_utf8(&buf[..len]).unwrap_or("?"));

            // 3: TOTAL
            let len = fmt_buf(&mut buf, format_args!("{:.2}ms", t_total));
            set_text(&mut state.tree, state.metric_val_ids[3], std::str::from_utf8(&buf[..len]).unwrap_or("?"));

            // 4: PRODUCTS
            let len = fmt_buf(&mut buf, format_args!("{}", state.products.len()));
            set_text(&mut state.tree, state.metric_val_ids[4], std::str::from_utf8(&buf[..len]).unwrap_or("?"));

            // 5: CATEGORY
            let cat = if state.active_cat == 0 { "all" } else { CATS[state.active_cat - 1] };
            set_text(&mut state.tree, state.metric_val_ids[5], cat);

            // 6: CART
            let len = fmt_buf(&mut buf, format_args!("{}", state.cart_count));
            set_text(&mut state.tree, state.metric_val_ids[6], std::str::from_utf8(&buf[..len]).unwrap_or("?"));

            // 7: SORT
            let sort = match state.sort_order { 1 => "price-asc", 2 => "price-desc", 3 => "name-asc", _ => "default" };
            set_text(&mut state.tree, state.metric_val_ids[7], sort);

            // 8: SIGNAL FIRES
            let len = fmt_buf(&mut buf, format_args!("{}", state.signal_fires));
            set_text(&mut state.tree, state.metric_val_ids[8], std::str::from_utf8(&buf[..len]).unwrap_or("?"));
        }
    });
}

fn update_dynamic_metrics(state: &mut AppState) {
    if state.metric_val_ids.len() < 9 { return; }
    let mut buf = [0u8; 32];

    let cat = if state.active_cat == 0 { "all" } else { CATS[state.active_cat.min(5) - 1] };
    set_text(&mut state.tree, state.metric_val_ids[5], cat);

    let len = fmt_buf(&mut buf, format_args!("{}", state.cart_count));
    set_text(&mut state.tree, state.metric_val_ids[6], std::str::from_utf8(&buf[..len]).unwrap_or("?"));

    let sort = match state.sort_order { 1 => "price-asc", 2 => "price-desc", 3 => "name-asc", _ => "default" };
    set_text(&mut state.tree, state.metric_val_ids[7], sort);

    let len = fmt_buf(&mut buf, format_args!("{}", state.signal_fires));
    set_text(&mut state.tree, state.metric_val_ids[8], std::str::from_utf8(&buf[..len]).unwrap_or("?"));
}

fn fmt_buf(buf: &mut [u8], args: std::fmt::Arguments) -> usize {
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    let _ = std::io::Write::write_fmt(&mut cursor, args);
    cursor.position() as usize
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
            render_node(id, &state.tree, state.root_id, sy, vw, vh, &state.products, &state.img_ids,
                state.sel_element, state.sel_start_char, state.sel_end_char);
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
    sel_element: i32,
    sel_start: u32,
    sel_end: u32,
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

    // Border radius
    let radius = el.styles.get("border-radius")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);

    // Background
    if let Some(bg) = el.styles.get("background-color") {
        let (r, g, b) = parse_hex_color(bg);
        if radius > 0.0 {
            canvas_round_rect(cvs, x, y, w, h, radius, r, g, b, 255);
        } else {
            canvas_fill_rect(cvs, x, y, w, h, r, g, b, 255);
        }
    }

    // Border
    if let Some(border) = el.styles.get("border") {
        // Parse "2px solid #2a2f3e"
        let parts: Vec<&str> = border.split_whitespace().collect();
        if parts.len() >= 3 {
            let lw = parts[0].trim_end_matches("px").parse::<f32>().unwrap_or(1.0);
            let (br, bg, bb) = parse_hex_color(parts[2]);
            canvas_stroke_rect(cvs, x, y, w, h, br, bg, bb, 255, lw);
        }
    } else if el.styles.get("background-color").is_some() && w > 50.0 && h > 30.0 {
        // Default subtle border on bg-colored elements
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
        let char_w = font_size * 0.6;

        // Selection highlight
        if sel_element == node_id as i32 && sel_start != sel_end {
            let s = sel_start.min(sel_end);
            let e = sel_start.max(sel_end);
            let hx = text_x + s as f32 * char_w;
            let hw = (e - s) as f32 * char_w;
            canvas_fill_rect(cvs, hx, text_y - font_size, hw, font_size + 4.0, 65, 140, 255, 100);
        }

        canvas_fill_text(cvs, text.as_ptr(), text.len() as u32, text_x, text_y, cr, cg, cb, font_size, if bold { 1 } else { 0 });
    }

    // Recurse children
    for &child_id in &el.children {
        render_node(cvs, tree, child_id, sy, vw, vh, products, img_ids, sel_element, sel_start, sel_end);
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
        let hit_id = match hit {
            Some(id) => id,
            None => return,
        };

        // Read the clicked element's text — walk up to parent if no text
        let mut check_id = hit_id;
        let mut text = String::new();
        for _ in 0..5 {
            if let Some(el) = state.tree.get(check_id) {
                if let Some(ref t) = el.text {
                    text = t.clone();
                    break;
                }
                // Walk to parent
                if let Some(pid) = el.parent {
                    check_id = pid;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Nav tab routing — call navigate syscall directly
        let nav_routes: [(&str, &str); 5] = [
            ("Home", "/"),
            ("E-Commerce", "/app/"),
            ("Canvas*", "/app/canvas-test.html"),
            ("React 18", "/app/react.html"),
            ("Svelte 5", "/app/svelte.html"),
        ];
        for (label, url) in &nav_routes {
            if text == *label {
                unsafe { navigate(url.as_ptr(), url.len() as u32); }
                return;
            }
        }

        // Category pills — O(1) style swap, no rebuild
        let pill_map = [("All", 0usize), ("Electronics", 1), ("Clothing", 2), ("Home", 3), ("Sports", 4), ("Books", 5)];
        for (label, idx) in &pill_map {
            if text == *label && state.pill_ids.len() == 6 {
                // Deactivate old pill
                let old = state.active_cat;
                if old < 6 {
                    let old_id = state.pill_ids[old];
                    set_style(&mut state.tree, old_id, "background-color", "");
                    set_style(&mut state.tree, old_id, "color", "#8b949e");
                    set_style(&mut state.tree, old_id, "border", "2px solid #2a2f3e");
                }
                // Activate new pill
                state.active_cat = *idx;
                let new_id = state.pill_ids[*idx];
                set_style(&mut state.tree, new_id, "background-color", "#f97316");
                set_style(&mut state.tree, new_id, "color", "#000000");
                set_style(&mut state.tree, new_id, "border", "");
                state.signal_fires += 1;
                update_dynamic_metrics(state);
                return;
            }
        }

        // Sort buttons — O(1) style swap
        let sort_map = [("Price ↑", 0usize, 1u8), ("Price ↓", 1, 2), ("Name A→Z", 2, 3)];
        for (label, si, order) in &sort_map {
            if text == *label && state.sort_ids.len() == 3 {
                // Deactivate old
                for (j, &sid) in state.sort_ids.iter().enumerate() {
                    if j == *si { continue; }
                    set_style(&mut state.tree, sid, "background-color", "");
                    set_style(&mut state.tree, sid, "color", "#8b949e");
                    set_style(&mut state.tree, sid, "border", "1px solid #2a2f3e");
                }
                // Activate new
                let new_id = state.sort_ids[*si];
                set_style(&mut state.tree, new_id, "background-color", "#f97316");
                set_style(&mut state.tree, new_id, "color", "#000000");
                set_style(&mut state.tree, new_id, "border", "");
                state.sort_order = *order;
                state.signal_fires += 1;
                update_dynamic_metrics(state);
                return;
            }
        }

        // Add to Cart — O(1) text update
        if text == "Add to Cart" {
            state.cart_count += 1;
            state.signal_fires += 1;
            let cart_id = state.cart_text_id;
            let mut buf = [0u8; 16];
            let len = {
                let mut c = std::io::Cursor::new(&mut buf[..]);
                let _ = std::io::Write::write_fmt(&mut c, format_args!("Cart {}", state.cart_count));
                c.position() as usize
            };
            set_text(&mut state.tree, cart_id, std::str::from_utf8(&buf[..len]).unwrap_or("Cart ?"));
            update_dynamic_metrics(state);
            return;
        }

        // Cart button (reset) — O(1) text update
        if text.starts_with("Cart") {
            state.cart_count = 0;
            state.signal_fires += 1;
            set_text(&mut state.tree, state.cart_text_id, "Cart 0");
            update_dynamic_metrics(state);
            return;
        }

        // Clear button — O(1) text update
        if text == "Clear" {
            state.cart_count = 0;
            state.signal_fires += 1;
            set_text(&mut state.tree, state.cart_text_id, "Cart 0");
            update_dynamic_metrics(state);
            return;
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
        layout::compute(&mut state.tree, vw, 999999.0, &mut state.measurer);
    });
}

#[no_mangle]
pub extern "C" fn app_get_back_clicked(mx: f32, my: f32) -> u32 {
    if mx < 200.0 && my < 50.0 { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn app_cursor(mx: f32, my: f32) -> u32 {
    with_state(|state| {
        let hit = state.tree.hit_test(mx, my + state.scroll_y);
        if let Some(hit_id) = hit {
            if let Some(el) = state.tree.get(hit_id) {
                let text = el.text.as_deref().unwrap_or("");
                let bg = el.styles.get("background-color").map(|s| s.as_str()).unwrap_or("");
                let has_border = el.styles.contains_key("border");

                // Pointer on buttons (orange bg, bordered pills, sort buttons, nav tabs)
                if bg == "#f97316" || bg == "#1a2e1a" { return 1; }
                if has_border && !text.is_empty() { return 1; }
                if text == "Add to Cart" || text.starts_with("Cart") || text == "Clear" { return 1; }

                // Text cursor on product text
                if !text.is_empty() && !has_border && bg != "#f97316" { return 2; }
            }
        }
        0
    })
}

#[no_mangle]
pub extern "C" fn app_mousedown(mx: f32, my: f32, click_count: u32) {
    with_state(|state| {
        let hit = state.tree.hit_test(mx, my + state.scroll_y);
        let hit_id = match hit { Some(id) => id, None => { state.sel_element = -1; return; } };
        let el = match state.tree.get(hit_id) { Some(e) => e, None => return };
        let text = match &el.text { Some(t) => t.clone(), None => { state.sel_element = -1; return; } };

        // Calculate character position from x offset
        let font_size = el.styles.get("font-size").and_then(|v| v.trim_end_matches("px").parse::<f32>().ok()).unwrap_or(14.0);
        let char_w = font_size * 0.6;
        let text_x = el.layout.x + el.layout.padding.left;
        let char_pos = ((mx - text_x) / char_w).max(0.0) as u32;
        let char_pos = char_pos.min(text.len() as u32);

        state.sel_element = hit_id as i32;
        state.sel_dragging = true;

        match click_count {
            2 => {
                // Double click: select word
                let pos = char_pos as usize;
                let bytes = text.as_bytes();
                let mut start = pos;
                while start > 0 && bytes.get(start - 1).map(|b| b.is_ascii_alphanumeric()).unwrap_or(false) { start -= 1; }
                let mut end = pos;
                while end < bytes.len() && bytes.get(end).map(|b| b.is_ascii_alphanumeric()).unwrap_or(false) { end += 1; }
                if start == end && pos < bytes.len() { end = pos + 1; }
                state.sel_start_char = start as u32;
                state.sel_end_char = end as u32;
            }
            3 => {
                // Triple click: select all
                state.sel_start_char = 0;
                state.sel_end_char = text.len() as u32;
            }
            _ => {
                state.sel_start_char = char_pos;
                state.sel_end_char = char_pos;
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn app_mousemove(mx: f32, _my: f32, buttons: u32) {
    if buttons & 1 == 0 { return; }
    with_state(|state| {
        if state.sel_element < 0 || !state.sel_dragging { return; }
        let el = match state.tree.get(state.sel_element as u32) { Some(e) => e, None => return };
        let text_len = el.text.as_ref().map(|t| t.len()).unwrap_or(0);
        let font_size = el.styles.get("font-size").and_then(|v| v.trim_end_matches("px").parse::<f32>().ok()).unwrap_or(14.0);
        let char_w = font_size * 0.6;
        let text_x = el.layout.x + el.layout.padding.left;
        let char_pos = ((mx - text_x) / char_w).max(0.0) as u32;
        state.sel_end_char = char_pos.min(text_len as u32);
    });
}

#[no_mangle]
pub extern "C" fn app_mouseup(_mx: f32, _my: f32) {
    with_state(|state| { state.sel_dragging = false; });
}

#[no_mangle]
pub extern "C" fn app_get_selection(buf_ptr: *mut u8, buf_cap: u32) -> u32 {
    with_state(|state| {
        if state.sel_element < 0 { return 0; }
        if state.sel_start_char == state.sel_end_char { return 0; }
        let el = match state.tree.get(state.sel_element as u32) { Some(e) => e, None => return 0 };
        let text = match &el.text { Some(t) => t, None => return 0 };
        let start = state.sel_start_char.min(state.sel_end_char) as usize;
        let end = state.sel_start_char.max(state.sel_end_char) as usize;
        let start = start.min(text.len());
        let end = end.min(text.len());
        let selected = &text[start..end];
        let len = selected.len().min(buf_cap as usize);
        unsafe { std::ptr::copy_nonoverlapping(selected.as_ptr(), buf_ptr, len); }
        len as u32
    })
}

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

/// Debug: dump first N elements' layout to console via canvas_fill_text
#[no_mangle]
pub extern "C" fn app_debug_layout(count: u32) {
    with_state(|state| {
        for (id, el) in state.tree.iter() {
            if id as u32 >= count + 2 { break; }
            if id < 2 { continue; }
            let l = &el.layout;
            let tag = &el.tag;
            let text = el.text.as_deref().unwrap_or("");
            let txt = if text.len() > 20 { &text[..20] } else { text };
            // Print to stderr (visible in browser console as WASM stderr)
            let mut buf = [0u8; 128];
            let len = {
                let mut c = std::io::Cursor::new(&mut buf[..]);
                let _ = std::io::Write::write_fmt(&mut c, format_args!(
                    "id={} tag={} x={:.0} y={:.0} w={:.0} h={:.0} text=\"{}\"",
                    id, tag, l.x, l.y, l.width, l.height, txt
                ));
                c.position() as usize
            };
            // Draw it as text on the canvas at y=id*14
            unsafe {
                canvas_fill_text(state.canvas_id, buf.as_ptr(), len as u32, 10.0, (id as f32) * 14.0, 255, 255, 0, 10.0, 0);
            }
        }
    });
}
