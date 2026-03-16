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
    pub properties: HashMap<String, String>,
    pub layout: LayoutNode,
    pub scroll_offset: (f32, f32),
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
            properties: HashMap::new(),
            layout: LayoutNode::default(),
            scroll_offset: (0.0, 0.0),
        }
    }

    pub fn is_editable(&self) -> bool {
        matches!(self.tag.as_str(), "input" | "textarea")
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

    pub fn create(&mut self, tag: &str) -> u32 {
        let el = Element::new(tag);
        // Find free slot or push
        for (i, slot) in self.elements.iter_mut().enumerate() {
            if i < 2 { continue; }
            if slot.is_none() {
                *slot = Some(el);
                self.dirty = true;
                return i as u32;
            }
        }
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
}
