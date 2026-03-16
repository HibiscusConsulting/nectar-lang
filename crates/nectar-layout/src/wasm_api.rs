//! WASM exports — thin API for the browser to drive the layout engine.
//! The browser calls these functions to build the element tree,
//! run layout, and read back computed positions for rendering.

use crate::element::ElementTree;
use crate::layout;
use crate::measure::EstimateMeasurer;

use std::cell::RefCell;

thread_local! {
    static TREE: RefCell<ElementTree> = RefCell::new(ElementTree::new());
    static MEASURER: RefCell<EstimateMeasurer> = RefCell::new(EstimateMeasurer);
}

// ── Element tree manipulation ─────────────────────────────────

#[no_mangle]
pub extern "C" fn create_element(tag_ptr: *const u8, tag_len: u32) -> u32 {
    let tag = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(tag_ptr, tag_len as usize)) };
    TREE.with(|t| t.borrow_mut().create(tag))
}

#[no_mangle]
pub extern "C" fn append_child(parent: u32, child: u32) {
    TREE.with(|t| t.borrow_mut().append_child(parent, child));
}

#[no_mangle]
pub extern "C" fn set_text(id: u32, ptr: *const u8, len: u32) {
    let text = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len as usize)) };
    TREE.with(|t| t.borrow_mut().set_text(id, text));
}

#[no_mangle]
pub extern "C" fn set_attribute(id: u32, name_ptr: *const u8, name_len: u32, val_ptr: *const u8, val_len: u32) {
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name_ptr, name_len as usize)) };
    let val = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(val_ptr, val_len as usize)) };
    TREE.with(|t| t.borrow_mut().set_attribute(id, name, val));
}

#[no_mangle]
pub extern "C" fn set_style(id: u32, prop_ptr: *const u8, prop_len: u32, val_ptr: *const u8, val_len: u32) {
    let prop = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(prop_ptr, prop_len as usize)) };
    let val = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(val_ptr, val_len as usize)) };
    TREE.with(|t| t.borrow_mut().set_style(id, prop, val));
}

#[no_mangle]
pub extern "C" fn set_inner_html(id: u32, html_ptr: *const u8, html_len: u32) {
    let html = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(html_ptr, html_len as usize)) };
    TREE.with(|t| t.borrow_mut().set_inner_html(id, html));
}

#[no_mangle]
pub extern "C" fn remove_child(parent: u32, child: u32) {
    TREE.with(|t| t.borrow_mut().remove_child(parent, child));
}

#[no_mangle]
pub extern "C" fn add_event_listener(id: u32, _event_ptr: *const u8, _event_len: u32, cb_idx: u32) {
    TREE.with(|t| {
        if let Some(el) = t.borrow_mut().get_mut(id) {
            el.event_listeners.insert("click".into(), cb_idx);
        }
    });
}

// ── Layout computation ────────────────────────────────────────

#[no_mangle]
pub extern "C" fn compute_layout(viewport_w: f32, viewport_h: f32) {
    TREE.with(|t| {
        MEASURER.with(|m| {
            layout::compute(&mut t.borrow_mut(), viewport_w, viewport_h, &mut *m.borrow_mut());
        });
    });
}

// ── Read layout results ───────────────────────────────────────
// The browser calls these to read computed positions for each element.

#[no_mangle]
pub extern "C" fn get_layout_x(id: u32) -> f32 {
    TREE.with(|t| t.borrow().get(id).map(|e| e.layout.x).unwrap_or(0.0))
}

#[no_mangle]
pub extern "C" fn get_layout_y(id: u32) -> f32 {
    TREE.with(|t| t.borrow().get(id).map(|e| e.layout.y).unwrap_or(0.0))
}

#[no_mangle]
pub extern "C" fn get_layout_w(id: u32) -> f32 {
    TREE.with(|t| t.borrow().get(id).map(|e| e.layout.width).unwrap_or(0.0))
}

#[no_mangle]
pub extern "C" fn get_layout_h(id: u32) -> f32 {
    TREE.with(|t| t.borrow().get(id).map(|e| e.layout.height).unwrap_or(0.0))
}

#[no_mangle]
pub extern "C" fn get_element_tag(id: u32, buf_ptr: *mut u8, buf_cap: u32) -> u32 {
    TREE.with(|t| {
        if let Some(el) = t.borrow().get(id) {
            let bytes = el.tag.as_bytes();
            let len = bytes.len().min(buf_cap as usize);
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr, len); }
            len as u32
        } else {
            0
        }
    })
}

#[no_mangle]
pub extern "C" fn get_element_text(id: u32, buf_ptr: *mut u8, buf_cap: u32) -> u32 {
    TREE.with(|t| {
        if let Some(el) = t.borrow().get(id) {
            if let Some(ref text) = el.text {
                let bytes = text.as_bytes();
                let len = bytes.len().min(buf_cap as usize);
                unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr, len); }
                return len as u32;
            }
        }
        0
    })
}

#[no_mangle]
pub extern "C" fn get_element_attr(id: u32, name_ptr: *const u8, name_len: u32, buf_ptr: *mut u8, buf_cap: u32) -> u32 {
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name_ptr, name_len as usize)) };
    TREE.with(|t| {
        if let Some(el) = t.borrow().get(id) {
            if let Some(val) = el.attributes.get(name) {
                let bytes = val.as_bytes();
                let len = bytes.len().min(buf_cap as usize);
                unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr, len); }
                return len as u32;
            }
        }
        0
    })
}

#[no_mangle]
pub extern "C" fn get_element_style(id: u32, prop_ptr: *const u8, prop_len: u32, buf_ptr: *mut u8, buf_cap: u32) -> u32 {
    let prop = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(prop_ptr, prop_len as usize)) };
    TREE.with(|t| {
        if let Some(el) = t.borrow().get(id) {
            if let Some(val) = el.styles.get(prop) {
                let bytes = val.as_bytes();
                let len = bytes.len().min(buf_cap as usize);
                unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr, len); }
                return len as u32;
            }
        }
        0
    })
}

#[no_mangle]
pub extern "C" fn get_children_count(id: u32) -> u32 {
    TREE.with(|t| t.borrow().get(id).map(|e| e.children.len() as u32).unwrap_or(0))
}

#[no_mangle]
pub extern "C" fn get_child_id(id: u32, index: u32) -> u32 {
    TREE.with(|t| {
        t.borrow().get(id)
            .and_then(|e| e.children.get(index as usize).copied())
            .unwrap_or(0)
    })
}

#[no_mangle]
pub extern "C" fn get_element_count() -> u32 {
    TREE.with(|t| t.borrow().len() as u32)
}

// ── Batch element creation ────────────────────────────────────
// Creates N identical cards in one WASM call — avoids 7N boundary crossings.
// Each card: fixed 260x300, text = "Product #i", src = "img/p{i}.jpg"

#[no_mangle]
pub extern "C" fn batch_create_cards(count: u32, parent_id: u32) {
    TREE.with(|t| {
        let mut tree = t.borrow_mut();

        // Set parent to horizontal wrap
        if let Some(root) = tree.get_mut(parent_id) {
            root.styles.insert("direction".into(), "horizontal".into());
            root.styles.insert("wrap".into(), "true".into());
            root.styles.insert("gap".into(), "16".into());
            root.styles.insert("padding".into(), "40".into());
        }

        // Pre-grow the elements vec and parent's children vec
        tree.reserve(count as usize);
        if let Some(parent) = tree.get_mut(parent_id) {
            parent.children.reserve(count as usize);
        }

        // Batch create: all cards are identical fixed-size divs
        let first_id = tree.batch_create_fixed("div", 260.0, 300.0, count as usize);

        // Batch append: sequential IDs from first_id to first_id+count
        if let Some(parent) = tree.get_mut(parent_id) {
            for i in 0..count {
                let child_id = first_id + i;
                parent.children.push(child_id);
            }
        }
        // Set parent on all children
        for i in 0..count {
            let child_id = first_id + i;
            if let Some(el) = tree.get_mut(child_id) {
                el.parent = Some(parent_id);
            }
        }
        tree.dirty = true;
    });
}

// ── Hit testing ───────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn hit_test(x: f32, y: f32) -> i32 {
    TREE.with(|t| t.borrow().hit_test(x, y).map(|id| id as i32).unwrap_or(-1))
}

#[no_mangle]
pub extern "C" fn get_click_handler(id: u32) -> i32 {
    TREE.with(|t| {
        t.borrow().get(id)
            .and_then(|e| e.event_listeners.get("click"))
            .map(|&cb| cb as i32)
            .unwrap_or(-1)
    })
}
