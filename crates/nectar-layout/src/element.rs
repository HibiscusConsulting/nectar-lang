//! Element tree — a flat pool of elements indexed by handle.
//! Mirrors both the browser DOM (core.js __elements) and
//! nectar-runtime's ElementTree.

use std::collections::{HashMap, HashSet};
use crate::layout::LayoutNode;

/// A UI element — the universal building block.
pub struct Element {
    pub tag: String,
    pub text: Option<String>,
    pub inner_html: Option<String>,
    pub attributes: HashMap<String, String>,
    pub styles: HashMap<String, String>,
    pub classes: HashSet<String>,
    pub children: Vec<u32>,
    pub parent: Option<u32>,
    pub event_listeners: HashMap<String, u32>,
    pub focused: bool,
    pub properties: HashMap<String, String>,
    pub layout: LayoutNode,
    pub scroll_offset: (f32, f32),
    /// Cursor position in text (byte offset). Used for text input elements.
    pub cursor_pos: usize,
    /// Selection start (byte offset). If Some, selection spans from selection_start to cursor_pos.
    pub selection_start: Option<usize>,
    /// Undo stack: previous (value, cursor_pos) states.
    pub undo_stack: Vec<(String, usize)>,
    /// Redo stack: states popped by undo, restored by redo.
    pub redo_stack: Vec<(String, usize)>,
    /// Frame number of last scroll activity (for overlay scrollbar fade).
    pub last_scroll_frame: u32,
    /// Fast path: fixed dimensions bypass style parsing.
    pub fixed_width: Option<f32>,
    pub fixed_height: Option<f32>,
}

impl Element {
    pub fn new(tag: &str) -> Self {
        Self {
            tag: tag.to_string(),
            text: None,
            inner_html: None,
            attributes: HashMap::new(),
            styles: HashMap::new(),
            classes: HashSet::new(),
            children: Vec::new(),
            parent: None,
            event_listeners: HashMap::new(),
            focused: false,
            properties: HashMap::new(),
            layout: LayoutNode::default(),
            scroll_offset: (0.0, 0.0),
            cursor_pos: 0,
            selection_start: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_scroll_frame: 0,
            fixed_width: None,
            fixed_height: None,
        }
    }

    /// Minimal constructor — zero HashMap allocation. For batch creation.
    pub fn new_minimal(tag: &str) -> Self {
        let mut el = Self::new(tag);
        el.attributes = HashMap::with_capacity(0);
        el.styles = HashMap::with_capacity(0);
        el.classes = HashSet::with_capacity(0);
        el.event_listeners = HashMap::with_capacity(0);
        el.properties = HashMap::with_capacity(0);
        el
    }

    pub fn is_editable(&self) -> bool {
        matches!(self.tag.as_str(), "input" | "textarea")
            || self.attributes.get("contenteditable").map(|v| v == "true").unwrap_or(false)
    }

    pub fn input_value(&self) -> &str {
        self.properties.get("value").map(|s| s.as_str()).unwrap_or("")
    }

    pub fn set_input_value(&mut self, val: String) {
        self.properties.insert("value".to_string(), val);
    }

    pub fn push_undo(&mut self) {
        let val = self.input_value().to_string();
        self.undo_stack.push((val, self.cursor_pos));
        self.redo_stack.clear();
    }

    pub fn is_focusable(&self) -> bool {
        self.is_editable()
            || matches!(self.tag.as_str(), "button" | "a" | "select")
            || self.attributes.contains_key("tabindex")
    }

}

/// The element tree — a flat pool of elements indexed by handle.
pub struct ElementTree {
    elements: Vec<Option<Element>>,
    pub focused_element: Option<u32>,
    pub dirty: bool,
}

impl ElementTree {
    pub fn new() -> Self {
        let mut root = Element::new("body");
        root.styles.insert("width".into(), "100%".into());
        root.styles.insert("height".into(), "100%".into());

        Self {
            elements: vec![None, Some(root)], // 0 = null, 1 = root
            focused_element: None,
            dirty: true,
        }
    }

    pub fn create_element(&mut self, tag: &str) -> u32 {
        let id = self.elements.len() as u32;
        self.elements.push(Some(Element::new(tag)));
        self.dirty = true;
        id
    }

    pub fn create_text_node(&mut self, text: &str) -> u32 {
        let id = self.elements.len() as u32;
        let mut el = Element::new("#text");
        el.text = Some(text.to_string());
        self.elements.push(Some(el));
        self.dirty = true;
        id
    }

    pub fn ensure_capacity(&mut self, id: u32) {
        while self.elements.len() <= id as usize {
            self.elements.push(None);
        }
    }

    pub fn set_element(&mut self, id: u32, element: Element) {
        self.ensure_capacity(id);
        self.elements[id as usize] = Some(element);
        self.dirty = true;
    }

    pub fn move_element(&mut self, from: u32, to: u32) {
        self.ensure_capacity(to);
        if let Some(el) = self.elements[from as usize].take() {
            self.elements[to as usize] = Some(el);
        }
        self.dirty = true;
    }

    pub fn set_attr(&mut self, id: u32, key: &str, value: &str) {
        if let Some(el) = self.get_mut(id) {
            el.attributes.insert(key.to_string(), value.to_string());
        }
    }

    pub fn remove_attr(&mut self, id: u32, key: &str) {
        if let Some(el) = self.get_mut(id) {
            el.attributes.remove(key);
        }
    }

    pub fn class_add(&mut self, id: u32, class: &str) {
        if let Some(el) = self.get_mut(id) {
            el.classes.insert(class.to_string());
        }
    }

    pub fn class_remove(&mut self, id: u32, class: &str) {
        if let Some(el) = self.get_mut(id) {
            el.classes.remove(class);
        }
    }

    pub fn class_toggle(&mut self, id: u32, class: &str) {
        if let Some(el) = self.get_mut(id) {
            if !el.classes.remove(class) {
                el.classes.insert(class.to_string());
            }
        }
    }

    pub fn insert_before(&mut self, parent_id: u32, new_id: u32, ref_id: u32) {
        if let Some(parent) = self.get_mut(parent_id) {
            if let Some(pos) = parent.children.iter().position(|&c| c == ref_id) {
                parent.children.insert(pos, new_id);
            } else {
                parent.children.push(new_id);
            }
        }
        if let Some(child) = self.get_mut(new_id) {
            child.parent = Some(parent_id);
        }
    }

    pub fn set_property(&mut self, id: u32, key: &str, value: &str) {
        if let Some(el) = self.get_mut(id) {
            el.properties.insert(key.to_string(), value.to_string());
        }
    }

    pub fn add_event(&mut self, id: u32, event: &str, cb_index: u32) {
        if let Some(el) = self.get_mut(id) {
            el.event_listeners.insert(event.to_string(), cb_index);
        }
    }

    pub fn remove_event(&mut self, id: u32, event: &str) {
        if let Some(el) = self.get_mut(id) {
            el.event_listeners.remove(event);
        }
    }

    pub fn find_by_id(&self, id_attr: &str) -> Option<u32> {
        for (i, slot) in self.elements.iter().enumerate() {
            if let Some(el) = slot {
                if el.attributes.get("id").map(|s| s.as_str()) == Some(id_attr) {
                    return Some(i as u32);
                }
            }
        }
        None
    }

    pub fn query_selector(&self, selector: &str) -> Option<u32> {
        if let Some(id) = selector.strip_prefix('#') {
            return self.find_by_id(id);
        }
        if let Some(class) = selector.strip_prefix('.') {
            for (i, slot) in self.elements.iter().enumerate() {
                if let Some(el) = slot {
                    if el.classes.contains(class) {
                        return Some(i as u32);
                    }
                }
            }
            return None;
        }
        for (i, slot) in self.elements.iter().enumerate() {
            if let Some(el) = slot {
                if el.tag == selector {
                    return Some(i as u32);
                }
            }
        }
        None
    }

    pub fn root(&self) -> Option<&Element> { self.get(1) }

    pub fn element_count(&self) -> usize {
        self.elements.iter().filter(|e| e.is_some()).count()
    }

    pub fn reserve(&mut self, additional: usize) {
        self.elements.reserve(additional);
    }

    pub fn create(&mut self, tag: &str) -> u32 {
        let el = Element::new(tag);
        let id = self.elements.len() as u32;
        self.elements.push(Some(el));
        self.dirty = true;
        id
    }

    /// Batch-create N identical elements with fixed dimensions.
    /// Returns the first ID. All elements are sequential: first_id..first_id+count.
    pub fn batch_create_fixed(&mut self, tag: &str, width: f32, height: f32, count: usize) -> u32 {
        let first_id = self.elements.len() as u32;
        let tag_string = tag.to_string();
        for _ in 0..count {
            let mut el = Element::new(&tag_string);
            el.fixed_width = Some(width);
            el.fixed_height = Some(height);
            self.elements.push(Some(el));
        }
        self.dirty = true;
        first_id
    }

    /// Create an element with fixed width/height — avoids HashMap allocation.
    pub fn create_fixed(&mut self, tag: &str, width: f32, height: f32) -> u32 {
        let mut el = Element::new_minimal(tag);
        el.fixed_width = Some(width);
        el.fixed_height = Some(height);
        let id = self.elements.len() as u32;
        self.elements.push(Some(el));
        self.dirty = true;
        id
    }

    pub fn get(&self, id: u32) -> Option<&Element> {
        self.elements.get(id as usize).and_then(|s| s.as_ref())
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Element> {
        self.elements.get_mut(id as usize).and_then(|s| s.as_mut())
    }

    pub fn append_child(&mut self, parent_id: u32, child_id: u32) {
        // Remove from old parent
        if let Some(old_parent) = self.get(child_id).and_then(|e| e.parent) {
            if let Some(p) = self.get_mut(old_parent) {
                p.children.retain(|&c| c != child_id);
            }
        }
        if let Some(child) = self.get_mut(child_id) {
            child.parent = Some(parent_id);
        }
        if let Some(parent) = self.get_mut(parent_id) {
            if !parent.children.contains(&child_id) {
                parent.children.push(child_id);
            }
        }
        self.dirty = true;
    }

    pub fn remove_child(&mut self, parent_id: u32, child_id: u32) {
        if let Some(parent) = self.get_mut(parent_id) {
            parent.children.retain(|&c| c != child_id);
        }
        if let Some(child) = self.get_mut(child_id) {
            child.parent = None;
        }
        self.dirty = true;
    }

    pub fn remove(&mut self, id: u32) {
        if id < 2 { return; } // don't remove null or root
        self.elements[id as usize] = None;
        self.dirty = true;
    }

    pub fn set_text(&mut self, id: u32, text: &str) {
        if let Some(el) = self.get_mut(id) {
            el.text = Some(text.to_string());
            self.dirty = true;
        }
    }

    pub fn set_attribute(&mut self, id: u32, name: &str, value: &str) {
        if let Some(el) = self.get_mut(id) {
            el.attributes.insert(name.to_string(), value.to_string());
            self.dirty = true;
        }
    }

    pub fn set_style(&mut self, id: u32, prop: &str, value: &str) {
        if let Some(el) = self.get_mut(id) {
            el.styles.insert(prop.to_string(), value.to_string());
            self.dirty = true;
        }
    }

    pub fn set_inner_html(&mut self, id: u32, html: &str) {
        if let Some(el) = self.get_mut(id) {
            el.inner_html = Some(html.to_string());
            el.children.clear();
            self.dirty = true;
        }
    }

    /// Hit test: find the deepest element at (x, y) that has an event listener.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<u32> {
        self.hit_test_node(1, x, y)
    }

    fn hit_test_node(&self, id: u32, x: f32, y: f32) -> Option<u32> {
        let el = self.get(id)?;
        let l = &el.layout;

        // Check if point is inside this element
        if x < l.x || x > l.x + l.width || y < l.y || y > l.y + l.height {
            return None;
        }

        // Check children in reverse order (last child = top of z-order)
        for &child_id in el.children.iter().rev() {
            // Adjust coordinates for scroll
            let adj_x = x + el.scroll_offset.0;
            let adj_y = y + el.scroll_offset.1;
            if let Some(hit) = self.hit_test_node(child_id, adj_x, adj_y) {
                return Some(hit);
            }
        }

        // If this element has a click listener, it's a hit
        if el.event_listeners.contains_key("click") {
            return Some(id);
        }

        None
    }

    /// Iterate all elements with their IDs.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &Element)> {
        self.elements.iter().enumerate().filter_map(|(i, slot)| {
            slot.as_ref().map(|el| (i as u32, el))
        })
    }

    pub fn root_id(&self) -> u32 { 1 }
    pub fn len(&self) -> usize { self.elements.iter().filter(|s| s.is_some()).count() }

    pub fn focus(&mut self, id: u32) {
        if let Some(old) = self.focused_element {
            if let Some(el) = self.get_mut(old) { el.focused = false; }
        }
        if let Some(el) = self.get_mut(id) { el.focused = true; }
        self.focused_element = Some(id);
        self.dirty = true;
    }

    pub fn blur(&mut self, id: u32) {
        if let Some(el) = self.get_mut(id) { el.focused = false; }
        if self.focused_element == Some(id) { self.focused_element = None; }
        self.dirty = true;
    }

    /// Return focusable element IDs in tree order.
    pub fn focusable_elements_in_order(&self) -> Vec<u32> {
        let mut result = Vec::new();
        self.collect_focusable(1, &mut result);
        result
    }

    fn collect_focusable(&self, id: u32, out: &mut Vec<u32>) {
        if let Some(el) = self.get(id) {
            if el.is_focusable() { out.push(id); }
            for &child in &el.children {
                self.collect_focusable(child, out);
            }
        }
    }
}
