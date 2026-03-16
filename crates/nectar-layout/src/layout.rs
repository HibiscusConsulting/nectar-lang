use crate::element::ElementTree;
use crate::measure::{TextMeasurer, TextStyle};

// ── Layout primitives ──────────────────────────────────────────────────────
// One layout model. No flex vs grid vs block vs inline vs float vs table.
// Just stacks. Boxes inside boxes.

/// The direction children are laid out within a stack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    /// Children stack top to bottom.
    Vertical,
    /// Children stack left to right.
    Horizontal,
    /// Children overlap on the z-axis. Last child paints on top.
    Layer,
}

/// How an element sizes itself along an axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizePolicy {
    /// Take available space, split with siblings by weight. `fill` = Fill(1.0).
    Fill(f32),
    /// Shrink to fit content.
    Hug,
    /// Exact pixel size.
    Fixed(f32),
}

/// Cross-axis alignment of children within a stack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Align {
    Start,
    Center,
    End,
    /// Stretch to fill cross-axis (default).
    Stretch,
}

/// Main-axis distribution of children within a stack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Anchor point within a Layer parent. Combines horizontal + vertical alignment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    TopLeft, TopCenter, TopRight,
    CenterLeft, Center, CenterRight,
    BottomLeft, BottomCenter, BottomRight,
}

/// Resolved layout style for one element — struct with named fields.
/// No HashMap lookups, no string parsing during layout. The style IS the struct.
#[derive(Debug, Clone)]
pub struct LayoutStyle {
    pub direction: Direction,
    pub gap: f32,
    pub pad: Edges,
    pub align: Align,
    pub justify: Justify,
    pub width: SizePolicy,
    pub height: SizePolicy,
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
    pub scroll: bool,
    pub wrap: bool,
    /// Anchor point within a Layer parent (default: TopLeft)
    pub anchor: Anchor,
    /// display: none — element contributes zero size and is skipped during layout.
    pub display_none: bool,
    /// white-space: nowrap — prevents text re-measurement with width constraint.
    pub white_space_nowrap: bool,
    /// Position offsets within a Layer parent.
    pub offset_top: Option<f32>,
    pub offset_right: Option<f32>,
    pub offset_bottom: Option<f32>,
    pub offset_left: Option<f32>,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        Self {
            direction: Direction::Vertical,
            gap: 0.0,
            pad: Edges::default(),
            align: Align::Stretch,
            justify: Justify::Start,
            width: SizePolicy::Fill(1.0),
            height: SizePolicy::Hug,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            scroll: false,
            wrap: false,
            anchor: Anchor::TopLeft,
            display_none: false,
            white_space_nowrap: false,
            offset_top: None,
            offset_right: None,
            offset_bottom: None,
            offset_left: None,
        }
    }
}

impl LayoutStyle {
    /// Apply a single CSS-style property to this layout style. Called eagerly on set_style.
    pub fn apply_property(&mut self, prop: &str, value: &str) {
        match prop {
            "direction" => {
                self.direction = match value {
                    "horizontal" | "row" => Direction::Horizontal,
                    "vertical" | "column" => Direction::Vertical,
                    "layer" | "stack" => Direction::Layer,
                    _ => Direction::Vertical,
                };
            }
            "flex-direction" => {
                self.direction = match value {
                    "row" | "row-reverse" => Direction::Horizontal,
                    _ => Direction::Vertical,
                };
            }
            "gap" => {
                if let Some(v) = parse_px(value) { self.gap = v; }
            }
            "pad" | "padding" => {
                self.pad = parse_edges(value);
            }
            "padding-top" => { if let Some(v) = parse_px(value) { self.pad.top = v; } }
            "padding-right" => { if let Some(v) = parse_px(value) { self.pad.right = v; } }
            "padding-bottom" => { if let Some(v) = parse_px(value) { self.pad.bottom = v; } }
            "padding-left" => { if let Some(v) = parse_px(value) { self.pad.left = v; } }
            "align" | "align-items" => {
                self.align = match value {
                    "start" | "flex-start" => Align::Start,
                    "center" => Align::Center,
                    "end" | "flex-end" => Align::End,
                    "stretch" => Align::Stretch,
                    _ => Align::Stretch,
                };
            }
            "justify" | "justify-content" => {
                self.justify = match value {
                    "start" | "flex-start" => Justify::Start,
                    "center" => Justify::Center,
                    "end" | "flex-end" => Justify::End,
                    "space-between" => Justify::SpaceBetween,
                    _ => Justify::Start,
                };
            }
            "width" => { self.width = parse_size_policy(value); }
            "height" => { self.height = parse_size_policy(value); }
            "size" => {
                let policy = parse_size_policy(value);
                self.width = policy;
                self.height = policy;
            }
            "min-width" => { self.min_width = parse_px(value); }
            "max-width" => { self.max_width = parse_px(value); }
            "min-height" => { self.min_height = parse_px(value); }
            "max-height" => { self.max_height = parse_px(value); }
            "scroll" => {
                self.scroll = value == "true" || value == "vertical" || value == "horizontal" || value == "both";
            }
            "overflow" => {
                self.scroll = value == "auto" || value == "scroll";
            }
            "wrap" | "flex-wrap" => {
                self.wrap = value == "true" || value == "wrap";
            }
            "anchor" => {
                self.anchor = match value {
                    "top-left" => Anchor::TopLeft,
                    "top-center" | "top" => Anchor::TopCenter,
                    "top-right" => Anchor::TopRight,
                    "center-left" | "left" => Anchor::CenterLeft,
                    "center" => Anchor::Center,
                    "center-right" | "right" => Anchor::CenterRight,
                    "bottom-left" => Anchor::BottomLeft,
                    "bottom-center" | "bottom" => Anchor::BottomCenter,
                    "bottom-right" => Anchor::BottomRight,
                    _ => Anchor::TopLeft,
                };
            }
            "display" => {
                self.display_none = value == "none";
            }
            "white-space" => {
                self.white_space_nowrap = value == "nowrap";
            }
            "top" => { self.offset_top = parse_px(value); }
            "right" => { self.offset_right = parse_px(value); }
            "bottom" => { self.offset_bottom = parse_px(value); }
            "left" => { self.offset_left = parse_px(value); }
            _ => {} // Visual-only properties (color, background-color, etc.) — not layout-relevant
        }
    }
}

/// Spacing edges — used for pad only. No margin. Margin is dead.
#[derive(Debug, Clone, Default)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// Computed layout for an element — position and size in pixels.
#[derive(Debug, Clone, Default)]
pub struct LayoutNode {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub padding: Edges,
}

impl LayoutNode {
    pub fn content_box(&self) -> (f32, f32, f32, f32) {
        (
            self.x + self.padding.left,
            self.y + self.padding.top,
            (self.width - self.padding.left - self.padding.right).max(0.0),
            (self.height - self.padding.top - self.padding.bottom).max(0.0),
        )
    }
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Compute layout for the entire element tree.
/// Two-pass: measure (bottom-up intrinsic sizes) then layout (top-down positioning).
pub fn compute(tree: &mut ElementTree, viewport_w: f32, viewport_h: f32, measurer: &mut dyn TextMeasurer) {
    #[cfg(not(target_arch = "wasm32"))]
    let t0 = std::time::Instant::now();

    // Phase 1: collect styles (just struct clones — no parsing) and children
    let ctx = LayoutContext::build(tree);

    #[cfg(not(target_arch = "wasm32"))]
    let t1 = std::time::Instant::now();

    // Phase 2: measure pass — compute intrinsic sizes bottom-up
    let cap = tree.capacity();
    let mut intrinsics: Vec<(f32, f32)> = vec![(0.0, 0.0); cap];
    let mut measured: Vec<bool> = vec![false; cap];
    measure_node(1, tree, &ctx, &mut intrinsics, &mut measured, measurer);

    #[cfg(not(target_arch = "wasm32"))]
    let t2 = std::time::Instant::now();

    // Phase 3: layout pass — assign positions and resolved sizes top-down
    layout_node(tree, &ctx, &intrinsics, 1, 0.0, 0.0, viewport_w, viewport_h, measurer);

    #[cfg(not(target_arch = "wasm32"))]
    let t3 = std::time::Instant::now();

    // Phase 4: propagate actual heights bottom-up for Hug containers
    propagate_hug_heights(tree, &ctx, 1);

    #[cfg(not(target_arch = "wasm32"))]
    {
        let t4 = std::time::Instant::now();
        eprintln!("  Phase 1 (context): {:?}", t1 - t0);
        eprintln!("  Phase 2 (measure): {:?}", t2 - t1);
        eprintln!("  Phase 3 (layout):  {:?}", t3 - t2);
        eprintln!("  Phase 4 (propagate): {:?}", t4 - t3);
    }
}

/// Bottom-up pass: recompute Hug container heights from actual child positions,
/// then reposition siblings. This is the fundamental fix:
/// in a vertical stack, every child pushes the next child down by its height.
fn propagate_hug_heights(tree: &mut ElementTree, ctx: &LayoutContext, id: u32) {
    let kids = ctx.children(id).to_vec();

    // First, recurse into all children (bottom-up)
    for &kid in &kids {
        propagate_hug_heights(tree, ctx, kid);
    }

    if kids.is_empty() { return; }

    let style = ctx.style(id);

    // For vertical non-wrapping containers: check if any child's height
    // was expanded by wrapping. If so, reposition all siblings.
    if matches!(style.direction, Direction::Vertical) && !style.wrap && matches!(style.height, SizePolicy::Hug) {
        let el_y = tree.get(id).map(|e| e.layout.y).unwrap_or(0.0);
        let gap = style.gap;

        // Check if repositioning is needed by computing expected vs actual total
        let mut cursor_y = el_y + style.pad.top;
        let mut needs_reposition = false;

        for (i, &kid) in kids.iter().enumerate() {
            if let Some(child) = tree.get(kid) {
                if (child.layout.y - cursor_y).abs() > 0.5 {
                    needs_reposition = true;
                }
                cursor_y += child.layout.height;
                if i < kids.len() - 1 { cursor_y += gap; }
            }
        }

        if needs_reposition {
            cursor_y = el_y + style.pad.top;
            for (i, &kid) in kids.iter().enumerate() {
                if let Some(child) = tree.get_mut(kid) {
                    child.layout.y = cursor_y;
                    cursor_y += child.layout.height;
                    if i < kids.len() - 1 { cursor_y += gap; }
                }
            }
        }

        // Update own height
        let new_h = (cursor_y - el_y) + style.pad.bottom;
        if let Some(el) = tree.get_mut(id) {
            el.layout.height = new_h;
        }
    } else if matches!(style.height, SizePolicy::Hug) {
        // For horizontal/layer/wrap containers: just update height from max child bottom
        let el_y = tree.get(id).map(|e| e.layout.y).unwrap_or(0.0);
        let mut max_bottom: f32 = 0.0;
        for &kid in &kids {
            if let Some(child) = tree.get(kid) {
                let child_bottom = child.layout.y + child.layout.height;
                max_bottom = max_bottom.max(child_bottom);
            }
        }
        let new_h = (max_bottom - el_y) + style.pad.bottom;
        if let Some(el) = tree.get_mut(id) {
            if new_h > el.layout.height {
                el.layout.height = new_h;
            }
        }
    }
}

// ── Context (pre-collected data to avoid borrow fights) ────────────────────
// Uses Vec indexed by element ID for O(1) access — no HashMap hashing overhead.

struct LayoutContext {
    styles: Vec<LayoutStyle>,          // indexed by element ID
    children: Vec<Vec<u32>>,           // indexed by element ID
    present: Vec<bool>,                // which IDs have elements
}

impl LayoutContext {
    fn build(tree: &ElementTree) -> Self {
        let cap = tree.capacity();
        let mut styles = Vec::with_capacity(cap);
        let mut children = Vec::with_capacity(cap);
        let mut present = Vec::with_capacity(cap);

        // Pre-fill to capacity
        let default_style = LayoutStyle::default();
        styles.resize_with(cap, || default_style.clone());
        children.resize_with(cap, Vec::new);
        present.resize(cap, false);

        for (id, el) in tree.iter() {
            let idx = id as usize;
            // The style IS the struct — no parsing, no HashMap lookups, just clone.
            let mut style = el.style.clone();

            // Text nodes always hug
            if el.tag == "#text" {
                style.width = SizePolicy::Hug;
                style.height = SizePolicy::Hug;
            }

            styles[idx] = style;
            if !el.children.is_empty() {
                children[idx] = el.children.clone();
            }
            present[idx] = true;
        }

        Self { styles, children, present }
    }

    #[inline]
    fn style(&self, id: u32) -> &LayoutStyle {
        &self.styles[id as usize]
    }

    #[inline]
    fn children(&self, id: u32) -> &[u32] {
        &self.children[id as usize]
    }
}

// ── Measure pass (bottom-up) ───────────────────────────────────────────────
// Returns (intrinsic_width, intrinsic_height) — the size an element wants
// if given infinite space. Fill elements contribute 0 on their fill axis.

fn measure_node(
    id: u32,
    tree: &ElementTree,
    ctx: &LayoutContext,
    intrinsics: &mut Vec<(f32, f32)>,
    measured: &mut Vec<bool>,
    measurer: &mut dyn TextMeasurer,
) -> (f32, f32) {
    let idx = id as usize;
    if measured[idx] {
        return intrinsics[idx];
    }

    let style = ctx.style(id);

    // display: none — element contributes zero size
    if style.display_none {
        measured[idx] = true;
        return (0.0, 0.0);
    }
    let kids = ctx.children(id);

    // Leaf node: text content — measure intrinsic (unwrapped) size
    let el = tree.get(id);
    let text_size = el
        .and_then(|e| e.text.as_ref())
        .map(|t| {
            let text_style = resolve_text_style(el.unwrap());
            measurer.measure(t, &text_style, None)
        })
        .unwrap_or((0.0, 0.0));

    if kids.is_empty() {
        // Leaf: intrinsic size is text size or 0
        let w = match style.width {
            SizePolicy::Fixed(v) => v,
            SizePolicy::Hug => text_size.0 + style.pad.left + style.pad.right,
            SizePolicy::Fill(_) => 0.0, // fill contributes nothing to parent's intrinsic
        };
        let h = match style.height {
            SizePolicy::Fixed(v) => v,
            SizePolicy::Hug => text_size.1 + style.pad.top + style.pad.bottom,
            SizePolicy::Fill(_) => 0.0,
        };
        let result = (constrain(w, style.min_width, style.max_width),
                      constrain(h, style.min_height, style.max_height));
        intrinsics[idx] = result;
        measured[idx] = true;
        return result;
    }

    // Measure all children first
    let child_sizes: Vec<(f32, f32)> = kids
        .iter()
        .map(|&kid| measure_node(kid, tree, ctx, intrinsics, measured, measurer))
        .collect();

    let total_gap = if kids.len() > 1 {
        style.gap * (kids.len() - 1) as f32
    } else {
        0.0
    };

    let (content_w, content_h) = match style.direction {
        Direction::Vertical => {
            // Width: widest child. Height: sum of children + gaps.
            let w = child_sizes.iter().map(|s| s.0).fold(0.0f32, f32::max);
            let h: f32 = child_sizes.iter().map(|s| s.1).sum::<f32>() + total_gap;
            (w, h)
        }
        Direction::Horizontal => {
            // Width: sum of children + gaps. Height: tallest child.
            let w: f32 = child_sizes.iter().map(|s| s.0).sum::<f32>() + total_gap;
            let h = child_sizes.iter().map(|s| s.1).fold(0.0f32, f32::max);
            (w, h)
        }
        Direction::Layer => {
            // Both axes: largest child.
            let w = child_sizes.iter().map(|s| s.0).fold(0.0f32, f32::max);
            let h = child_sizes.iter().map(|s| s.1).fold(0.0f32, f32::max);
            (w, h)
        }
    };

    let w = match style.width {
        SizePolicy::Fixed(v) => v,
        SizePolicy::Hug => content_w + style.pad.left + style.pad.right,
        SizePolicy::Fill(_) => 0.0,
    };
    let h = match style.height {
        SizePolicy::Fixed(v) => v,
        SizePolicy::Hug => content_h + style.pad.top + style.pad.bottom,
        SizePolicy::Fill(_) => 0.0,
    };

    let result = (constrain(w, style.min_width, style.max_width),
                  constrain(h, style.min_height, style.max_height));
    intrinsics[idx] = result;
    measured[idx] = true;
    result
}

// ── Layout pass (top-down) ─────────────────────────────────────────────────
// Given resolved size from parent, position children.

fn layout_node(
    tree: &mut ElementTree,
    ctx: &LayoutContext,
    intrinsics: &[(f32, f32)],
    id: u32,
    x: f32,
    y: f32,
    available_w: f32,
    available_h: f32,
    measurer: &mut dyn TextMeasurer,
) {
    let style = ctx.style(id).clone();

    // display: none — skip layout entirely, zero out dimensions
    if style.display_none {
        if let Some(el) = tree.get_mut(id) {
            el.layout.x = x;
            el.layout.y = y;
            el.layout.width = 0.0;
            el.layout.height = 0.0;
        }
        return;
    }

    // Resolve own size (using intrinsic for Hug)
    let intrinsic = intrinsics[id as usize];
    let resolved_w = resolve_size_with_intrinsic(style.width, available_w, intrinsic.0, style.min_width, style.max_width);
    let mut resolved_h = resolve_size_with_intrinsic(style.height, available_h, intrinsic.1, style.min_height, style.max_height);

    // For text nodes with Hug height, re-measure with the resolved width constraint
    // so that text wraps properly and the height reflects wrapped lines.
    // Respect white-space: nowrap — skip re-measurement if wrapping is disabled.
    if let Some(el) = tree.get(id) {
        if el.tag == "#text" {
            if let Some(text) = &el.text {
                if matches!(style.height, SizePolicy::Hug) && !style.white_space_nowrap {
                    let text_style = resolve_text_style(el);
                    let content_w = (resolved_w - style.pad.left - style.pad.right).max(0.0);
                    let (_, wrapped_h) = measurer.measure(text, &text_style, Some(content_w));
                    resolved_h = constrain_opt(
                        wrapped_h + style.pad.top + style.pad.bottom,
                        style.min_height,
                        style.max_height,
                    );
                }
            }
        }
    }

    // Write layout to element
    if let Some(el) = tree.get_mut(id) {
        el.layout.x = x;
        el.layout.y = y;
        el.layout.width = resolved_w;
        el.layout.height = resolved_h;
        el.layout.padding = style.pad.clone();
    }
    let kids = ctx.children(id);
    if kids.is_empty() {
        return;
    }

    // Content area (inside padding)
    let content_x = x + style.pad.left;
    let content_y = y + style.pad.top;
    let content_w = (resolved_w - style.pad.left - style.pad.right).max(0.0);
    let content_h = (resolved_h - style.pad.top - style.pad.bottom).max(0.0);

    let mut wrap_cross_size: Option<f32> = None;

    match style.direction {
        Direction::Vertical => {
            if style.wrap {
                wrap_cross_size = Some(layout_wrap(tree, ctx, intrinsics, kids, true, content_x, content_y, content_w, content_h, style.gap, style.align, measurer));
            } else {
                layout_stack(tree, ctx, intrinsics, kids, true, content_x, content_y, content_w, content_h, style.gap, style.align, style.justify, measurer);
            }
        }
        Direction::Horizontal => {
            if style.wrap {
                wrap_cross_size = Some(layout_wrap(tree, ctx, intrinsics, kids, false, content_x, content_y, content_w, content_h, style.gap, style.align, measurer));
            } else {
                layout_stack(tree, ctx, intrinsics, kids, false, content_x, content_y, content_w, content_h, style.gap, style.align, style.justify, measurer);
            }
        }
        Direction::Layer => {
            // Every child gets the full content area, offset by top/left/right/bottom if present
            for &kid in kids {
                let kid_style = ctx.style(kid);
                let kid_w = resolve_size_with_intrinsic(kid_style.width, content_w, intrinsics[kid as usize].0, kid_style.min_width, kid_style.max_width);
                let kid_h = resolve_size_with_intrinsic(kid_style.height, content_h, intrinsics[kid as usize].1, kid_style.min_height, kid_style.max_height);

                // Position offsets within the layer (from LayoutStyle struct — no HashMap lookup)
                let offset_top = kid_style.offset_top;
                let offset_left = kid_style.offset_left;
                let offset_bottom = kid_style.offset_bottom;
                let offset_right = kid_style.offset_right;

                let (kid_x, kid_y) = if kid_style.anchor != Anchor::TopLeft {
                    // Anchor-based positioning
                    let (ax, ay) = match kid_style.anchor {
                        Anchor::TopLeft => (content_x, content_y),
                        Anchor::TopCenter => (content_x + (content_w - kid_w) / 2.0, content_y),
                        Anchor::TopRight => (content_x + content_w - kid_w, content_y),
                        Anchor::CenterLeft => (content_x, content_y + (content_h - kid_h) / 2.0),
                        Anchor::Center => (content_x + (content_w - kid_w) / 2.0, content_y + (content_h - kid_h) / 2.0),
                        Anchor::CenterRight => (content_x + content_w - kid_w, content_y + (content_h - kid_h) / 2.0),
                        Anchor::BottomLeft => (content_x, content_y + content_h - kid_h),
                        Anchor::BottomCenter => (content_x + (content_w - kid_w) / 2.0, content_y + content_h - kid_h),
                        Anchor::BottomRight => (content_x + content_w - kid_w, content_y + content_h - kid_h),
                    };
                    // Pixel offsets on top of anchor
                    let ax = if let Some(l) = offset_left { ax + l } else if let Some(r) = offset_right { ax - r } else { ax };
                    let ay = if let Some(t) = offset_top { ay + t } else if let Some(b) = offset_bottom { ay - b } else { ay };
                    (ax, ay)
                } else {
                    // Legacy CSS-style positioning (top/left/right/bottom from edges)
                    let kx = if let Some(left) = offset_left {
                        content_x + left
                    } else if let Some(right) = offset_right {
                        content_x + content_w - kid_w - right
                    } else {
                        content_x
                    };
                    let ky = if let Some(top) = offset_top {
                        content_y + top
                    } else if let Some(bottom) = offset_bottom {
                        content_y + content_h - kid_h - bottom
                    } else {
                        content_y
                    };
                    (kx, ky)
                };

                layout_node(tree, ctx, intrinsics, kid, kid_x, kid_y, kid_w, kid_h, measurer);
            }
        }
    }

    // For Hug-height parents with wrapping children, update height to actual wrapped content
    if let Some(cross) = wrap_cross_size {
        if matches!(style.height, SizePolicy::Hug) {
            let new_h = cross + style.pad.top + style.pad.bottom;
            if let Some(el) = tree.get_mut(id) {
                el.layout.height = new_h;
            }
        }
    }

    // For Hug-height parents with non-wrapping vertical children, recalculate
    // height from actual children heights (children may have been expanded by wrap)
    if matches!(style.height, SizePolicy::Hug) && wrap_cross_size.is_none() {
        let total_gap = if kids.len() > 1 { style.gap * (kids.len() - 1) as f32 } else { 0.0 };
        let children_size: f32 = match style.direction {
            Direction::Vertical => {
                kids.iter()
                    .filter_map(|&kid| tree.get(kid).map(|e| e.layout.height))
                    .sum::<f32>() + total_gap
            }
            Direction::Horizontal => {
                kids.iter()
                    .filter_map(|&kid| tree.get(kid).map(|e| e.layout.height))
                    .fold(0.0f32, f32::max)
            }
            Direction::Layer => {
                kids.iter()
                    .filter_map(|&kid| tree.get(kid).map(|e| e.layout.height))
                    .fold(0.0f32, f32::max)
            }
        };
        let new_h = children_size + style.pad.top + style.pad.bottom;
        if let Some(el) = tree.get_mut(id) {
            if new_h > el.layout.height {
                el.layout.height = new_h;
            }
        }
    }
}

/// Layout children along a single axis (vertical or horizontal).
fn layout_stack(
    tree: &mut ElementTree,
    ctx: &LayoutContext,
    intrinsics: &[(f32, f32)],
    kids: &[u32],
    is_vertical: bool,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    content_h: f32,
    gap: f32,
    align: Align,
    justify: Justify,
    measurer: &mut dyn TextMeasurer,
) {
    let available_main = if is_vertical { content_h } else { content_w };
    let available_cross = if is_vertical { content_w } else { content_h };

    let total_gap = if kids.len() > 1 { gap * (kids.len() - 1) as f32 } else { 0.0 };

    // Phase 1: Classify children and compute space taken by fixed/hug
    let mut fixed_total: f32 = 0.0;
    let mut fill_total_weight: f32 = 0.0;
    let mut child_main_sizes: Vec<f32> = Vec::with_capacity(kids.len());

    for &kid in kids {
        // display: none children contribute zero space
        let kid_style = ctx.style(kid);
        if kid_style.display_none {
            child_main_sizes.push(0.0);
            continue;
        }

        let kid_intrinsic = intrinsics[kid as usize];
        let kid_intrinsic_main = if is_vertical { kid_intrinsic.1 } else { kid_intrinsic.0 };

        let main_policy = if is_vertical { kid_style.height } else { kid_style.width };

        let main_size = match main_policy {
            SizePolicy::Fixed(v) => {
                let v = constrain_opt(v, if is_vertical { kid_style.min_height } else { kid_style.min_width }, if is_vertical { kid_style.max_height } else { kid_style.max_width });
                fixed_total += v;
                v
            }
            SizePolicy::Hug => {
                let v = constrain_opt(kid_intrinsic_main, if is_vertical { kid_style.min_height } else { kid_style.min_width }, if is_vertical { kid_style.max_height } else { kid_style.max_width });
                fixed_total += v;
                v
            }
            SizePolicy::Fill(weight) => {
                fill_total_weight += weight;
                -weight // negative = placeholder for fill
            }
        };
        child_main_sizes.push(main_size);
    }

    // Phase 2: Distribute remaining space to fill children
    let remaining = (available_main - total_gap - fixed_total).max(0.0);

    for size in &mut child_main_sizes {
        if *size < 0.0 {
            // It's a fill placeholder — weight is stored as negative
            let weight = -(*size);
            *size = if fill_total_weight > 0.0 {
                remaining * (weight / fill_total_weight)
            } else {
                0.0
            };
        }
    }

    // Phase 3: Compute justify offset and adjusted gap
    let total_children_main: f32 = child_main_sizes.iter().sum();
    let leftover = (available_main - total_children_main - total_gap).max(0.0);

    let (start_offset, effective_gap) = match justify {
        Justify::Start => (0.0, gap),
        Justify::Center => (leftover / 2.0, gap),
        Justify::End => (leftover, gap),
        Justify::SpaceBetween => {
            if kids.len() > 1 {
                (0.0, gap + leftover / (kids.len() - 1) as f32)
            } else {
                (leftover / 2.0, gap)
            }
        }
    };

    // Phase 4: Position each child
    let mut cursor = start_offset;

    for (i, &kid) in kids.iter().enumerate() {
        let kid_style = ctx.style(kid);
        let kid_intrinsic = intrinsics[kid as usize];
        let main_size = child_main_sizes[i];

        // Cross-axis sizing
        let cross_policy = if is_vertical { kid_style.width } else { kid_style.height };
        let kid_intrinsic_cross = if is_vertical { kid_intrinsic.0 } else { kid_intrinsic.1 };
        let min_cross = if is_vertical { kid_style.min_width } else { kid_style.min_height };
        let max_cross = if is_vertical { kid_style.max_width } else { kid_style.max_height };

        let cross_size = match align {
            Align::Stretch => {
                // Stretch overrides the child's cross size policy
                constrain_opt(available_cross, min_cross, max_cross)
            }
            _ => {
                match cross_policy {
                    SizePolicy::Fixed(v) => constrain_opt(v, min_cross, max_cross),
                    SizePolicy::Hug => constrain_opt(kid_intrinsic_cross, min_cross, max_cross),
                    SizePolicy::Fill(_) => constrain_opt(available_cross, min_cross, max_cross),
                }
            }
        };

        // Cross-axis position
        let cross_offset = match align {
            Align::Start | Align::Stretch => 0.0,
            Align::Center => (available_cross - cross_size) / 2.0,
            Align::End => available_cross - cross_size,
        };

        // Final position
        let (child_x, child_y, child_w, child_h) = if is_vertical {
            (content_x + cross_offset, content_y + cursor, cross_size, main_size)
        } else {
            (content_x + cursor, content_y + cross_offset, main_size, cross_size)
        };

        layout_node(tree, ctx, intrinsics, kid, child_x, child_y, child_w, child_h, measurer);

        cursor += main_size;
        if i < kids.len() - 1 {
            cursor += effective_gap;
        }
    }
}

/// Layout children with wrapping: when items exceed available main-axis space,
/// wrap to the next line along the cross-axis.
/// Returns total cross-axis size used (for updating parent's hug height)
fn layout_wrap(
    tree: &mut ElementTree,
    ctx: &LayoutContext,
    intrinsics: &[(f32, f32)],
    kids: &[u32],
    is_vertical: bool,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    content_h: f32,
    gap: f32,
    align: Align,
    measurer: &mut dyn TextMeasurer,
) -> f32 {
    let available_main = if is_vertical { content_h } else { content_w };

    // Phase 1: Break items into lines
    let mut lines: Vec<Vec<(u32, f32, f32)>> = Vec::new(); // each line: Vec<(kid_id, main_size, cross_size)>
    let mut current_line: Vec<(u32, f32, f32)> = Vec::new();
    let mut line_main_used: f32 = 0.0;

    for &kid in kids {
        let kid_style = ctx.style(kid);
        let kid_intrinsic = intrinsics[kid as usize];
        let kid_main = if is_vertical { kid_intrinsic.1 } else { kid_intrinsic.0 };
        let kid_cross = if is_vertical { kid_intrinsic.0 } else { kid_intrinsic.1 };

        let main_size = match if is_vertical { kid_style.height } else { kid_style.width } {
            SizePolicy::Fixed(v) => v,
            SizePolicy::Hug => kid_main,
            SizePolicy::Fill(_) => kid_main,
        };
        let cross_size = match if is_vertical { kid_style.width } else { kid_style.height } {
            SizePolicy::Fixed(v) => v,
            SizePolicy::Hug => kid_cross,
            SizePolicy::Fill(_) => kid_cross,
        };

        let gap_needed = if current_line.is_empty() { 0.0 } else { gap };
        if !current_line.is_empty() && line_main_used + gap_needed + main_size > available_main {
            lines.push(std::mem::take(&mut current_line));
            line_main_used = 0.0;
        }

        if !current_line.is_empty() {
            line_main_used += gap;
        }
        line_main_used += main_size;
        current_line.push((kid, main_size, cross_size));
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    // Phase 2: Position each line
    let mut cross_cursor: f32 = 0.0;

    for line in &lines {
        let line_cross_size = line.iter().map(|&(_, _, cs)| cs).fold(0.0f32, f32::max);

        let mut main_cursor: f32 = 0.0;
        for &(kid, main_size, cross_size) in line {
            let cross_offset = match align {
                Align::Start | Align::Stretch => 0.0,
                Align::Center => (line_cross_size - cross_size) / 2.0,
                Align::End => line_cross_size - cross_size,
            };

            let final_cross = if matches!(align, Align::Stretch) { line_cross_size } else { cross_size };

            let (child_x, child_y, child_w, child_h) = if is_vertical {
                (content_x + cross_cursor + cross_offset, content_y + main_cursor, final_cross, main_size)
            } else {
                (content_x + main_cursor, content_y + cross_cursor + cross_offset, main_size, final_cross)
            };

            layout_node(tree, ctx, intrinsics, kid, child_x, child_y, child_w, child_h, measurer);
            main_cursor += main_size + gap;
        }

        cross_cursor += line_cross_size + gap;
    }
    // Total cross-axis size (subtract trailing gap)
    if cross_cursor > 0.0 { cross_cursor - gap } else { 0.0 }
}

// ── Size resolution helpers ────────────────────────────────────────────────

fn resolve_size(policy: SizePolicy, available: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let v = match policy {
        SizePolicy::Fill(_) => available,
        SizePolicy::Fixed(v) => v,
        SizePolicy::Hug => available, // at top level, hug falls back to available
    };
    constrain_opt(v, min, max)
}

fn resolve_size_with_intrinsic(policy: SizePolicy, available: f32, intrinsic: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let v = match policy {
        SizePolicy::Fill(_) => available,
        SizePolicy::Fixed(v) => v,
        SizePolicy::Hug => intrinsic,
    };
    constrain_opt(v, min, max)
}

fn constrain(v: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    constrain_opt(v, min, max)
}

fn constrain_opt(v: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let mut v = v;
    if let Some(min) = min { v = v.max(min); }
    if let Some(max) = max { v = v.min(max); }
    v
}

// ── Style resolution ───────────────────────────────────────────────────────
// Parse from element's styles HashMap into structured LayoutStyle.
// Supports both Nectar-native properties and CSS-legacy for compatibility.

// resolve_style is no longer needed — LayoutStyle::apply_property is called eagerly
// on every set_style() call. LayoutContext::build just clones el.style.

fn parse_size_policy(value: &str) -> SizePolicy {
    let v = value.trim();
    match v {
        "fill" => SizePolicy::Fill(1.0),
        "hug" | "auto" | "fit-content" => SizePolicy::Hug,
        "100%" => SizePolicy::Fill(1.0),
        _ => {
            // fill(N)
            if let Some(inner) = v.strip_prefix("fill(").and_then(|s| s.strip_suffix(')')) {
                if let Ok(weight) = inner.parse::<f32>() {
                    return SizePolicy::Fill(weight);
                }
            }
            // Fixed pixel value
            if let Some(px) = parse_px(v) {
                return SizePolicy::Fixed(px);
            }
            // Percentage (treat as fill with that proportion)
            if let Some(pct) = v.strip_suffix('%').and_then(|s| s.parse::<f32>().ok()) {
                return SizePolicy::Fill(pct / 100.0);
            }
            SizePolicy::Hug
        }
    }
}

fn parse_edges(value: &str) -> Edges {
    let parts: Vec<f32> = value.split_whitespace()
        .filter_map(|v| parse_px(v))
        .collect();
    match parts.len() {
        1 => Edges { top: parts[0], right: parts[0], bottom: parts[0], left: parts[0] },
        2 => Edges { top: parts[0], right: parts[1], bottom: parts[0], left: parts[1] },
        3 => Edges { top: parts[0], right: parts[1], bottom: parts[2], left: parts[1] },
        4 => Edges { top: parts[0], right: parts[1], bottom: parts[2], left: parts[3] },
        _ => Edges::default(),
    }
}

fn parse_px(value: &str) -> Option<f32> {
    let v = value.trim();
    v.strip_suffix("px").and_then(|s| s.parse().ok())
        .or_else(|| v.parse().ok())
}

/// Resolve font properties from element styles into a TextStyle for measurement.
fn resolve_text_style(el: &crate::element::Element) -> TextStyle {
    let font_size = el.styles.get("font-size")
        .and_then(|v| parse_px(v))
        .unwrap_or(16.0);

    let line_height = el.styles.get("line-height")
        .and_then(|v| {
            if let Some(px) = parse_px(v) {
                Some(px)
            } else {
                v.trim().parse::<f32>().ok().map(|m| m * font_size)
            }
        })
        .unwrap_or(font_size * 1.2);

    let font_weight = el.styles.get("font-weight")
        .and_then(|v| match v.as_str() {
            "bold" => Some(700),
            "normal" => Some(400),
            "lighter" => Some(300),
            "bolder" => Some(800),
            _ => v.trim().parse::<u16>().ok(),
        })
        .unwrap_or(400);

    let font_family = el.styles.get("font-family")
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
        .unwrap_or_else(|| "system-ui".to_string());

    let italic = el.styles.get("font-style")
        .map(|v| matches!(v.as_str(), "italic" | "oblique"))
        .unwrap_or(false);

    TextStyle {
        font_size,
        line_height,
        font_weight,
        font_family,
        italic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::ElementTree;
    use crate::measure::EstimateMeasurer;

    /// Helper: create a tree, configure elements, run layout, return tree.
    fn layout_tree(tree: &mut ElementTree, vw: f32, vh: f32) {
        let mut measurer = EstimateMeasurer;
        compute(tree, vw, vh, &mut measurer);
    }

    // ── 1. Single root fills viewport ──────────────────────────────────

    #[test]
    fn test_single_root_fills_viewport() {
        let mut tree = ElementTree::new();
        layout_tree(&mut tree, 800.0, 600.0);

        let root = tree.get(1).unwrap();
        assert_eq!(root.layout.x, 0.0);
        assert_eq!(root.layout.y, 0.0);
        assert_eq!(root.layout.width, 800.0);
        assert_eq!(root.layout.height, 600.0);
    }

    // ── 2. Vertical stack ──────────────────────────────────────────────

    #[test]
    fn test_vertical_stack_children_top_to_bottom() {
        let mut tree = ElementTree::new();
        // Root is vertical by default
        let a = tree.create("div");
        let b = tree.create("div");
        tree.set_style(a, "height", "100px");
        tree.set_style(b, "height", "150px");
        tree.append_child(1, a);
        tree.append_child(1, b);

        layout_tree(&mut tree, 800.0, 600.0);

        let la = tree.get(a).unwrap().layout.clone();
        let lb = tree.get(b).unwrap().layout.clone();
        assert_eq!(la.y, 0.0);
        assert_eq!(la.height, 100.0);
        assert_eq!(lb.y, 100.0);
        assert_eq!(lb.height, 150.0);
        // Both should stretch to full width
        assert_eq!(la.width, 800.0);
        assert_eq!(lb.width, 800.0);
    }

    // ── 3. Horizontal stack ────────────────────────────────────────────

    #[test]
    fn test_horizontal_stack_children_left_to_right() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "direction", "horizontal");
        let a = tree.create("div");
        let b = tree.create("div");
        tree.set_style(a, "width", "200px");
        tree.set_style(b, "width", "300px");
        tree.append_child(1, a);
        tree.append_child(1, b);

        layout_tree(&mut tree, 800.0, 600.0);

        let la = tree.get(a).unwrap().layout.clone();
        let lb = tree.get(b).unwrap().layout.clone();
        assert_eq!(la.x, 0.0);
        assert_eq!(la.width, 200.0);
        assert_eq!(lb.x, 200.0);
        assert_eq!(lb.width, 300.0);
    }

    // ── 4. Layer — children overlap ────────────────────────────────────

    #[test]
    fn test_layer_children_overlap() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "direction", "layer");
        let a = tree.create("div");
        let b = tree.create("div");
        tree.set_style(a, "width", "100px");
        tree.set_style(a, "height", "100px");
        tree.set_style(b, "width", "50px");
        tree.set_style(b, "height", "50px");
        tree.append_child(1, a);
        tree.append_child(1, b);

        layout_tree(&mut tree, 800.0, 600.0);

        let la = tree.get(a).unwrap().layout.clone();
        let lb = tree.get(b).unwrap().layout.clone();
        assert_eq!(la.x, 0.0);
        assert_eq!(la.y, 0.0);
        assert_eq!(lb.x, 0.0);
        assert_eq!(lb.y, 0.0);
    }

    // ── 5. Gap — spacing between children ──────────────────────────────

    #[test]
    fn test_gap_spacing() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "gap", "10px");
        let a = tree.create("div");
        let b = tree.create("div");
        let c = tree.create("div");
        tree.set_style(a, "height", "50px");
        tree.set_style(b, "height", "50px");
        tree.set_style(c, "height", "50px");
        tree.append_child(1, a);
        tree.append_child(1, b);
        tree.append_child(1, c);

        layout_tree(&mut tree, 800.0, 600.0);

        let la = tree.get(a).unwrap().layout.clone();
        let lb = tree.get(b).unwrap().layout.clone();
        let lc = tree.get(c).unwrap().layout.clone();
        assert_eq!(la.y, 0.0);
        assert_eq!(lb.y, 60.0);  // 50 + 10 gap
        assert_eq!(lc.y, 120.0); // 50 + 10 + 50 + 10
    }

    // ── 6. Padding — content area reduced ──────────────────────────────

    #[test]
    fn test_padding_reduces_content_area() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "padding", "20px");
        let child = tree.create("div");
        tree.set_style(child, "height", "100px");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);

        let lchild = tree.get(child).unwrap().layout.clone();
        // Child should start after padding
        assert_eq!(lchild.x, 20.0);
        assert_eq!(lchild.y, 20.0);
        // Child width reduced by left+right padding
        assert_eq!(lchild.width, 760.0);
    }

    // ── 7. Align Start/Center/End/Stretch ──────────────────────────────

    #[test]
    fn test_align_start() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "align", "start");
        let child = tree.create("div");
        tree.set_style(child, "width", "100px");
        tree.set_style(child, "height", "50px");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);
        let lc = tree.get(child).unwrap().layout.clone();
        assert_eq!(lc.x, 0.0);
        assert_eq!(lc.width, 100.0);
    }

    #[test]
    fn test_align_center() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "align", "center");
        let child = tree.create("div");
        tree.set_style(child, "width", "100px");
        tree.set_style(child, "height", "50px");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);
        let lc = tree.get(child).unwrap().layout.clone();
        assert_eq!(lc.x, 350.0); // (800 - 100) / 2
        assert_eq!(lc.width, 100.0);
    }

    #[test]
    fn test_align_end() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "align", "end");
        let child = tree.create("div");
        tree.set_style(child, "width", "100px");
        tree.set_style(child, "height", "50px");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);
        let lc = tree.get(child).unwrap().layout.clone();
        assert_eq!(lc.x, 700.0); // 800 - 100
    }

    #[test]
    fn test_align_stretch() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "align", "stretch");
        let child = tree.create("div");
        tree.set_style(child, "height", "50px");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);
        let lc = tree.get(child).unwrap().layout.clone();
        assert_eq!(lc.width, 800.0);
    }

    // ── 8. Justify Start/Center/End/SpaceBetween ───────────────────────

    #[test]
    fn test_justify_center() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "justify", "center");
        let child = tree.create("div");
        tree.set_style(child, "height", "100px");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);
        let lc = tree.get(child).unwrap().layout.clone();
        assert_eq!(lc.y, 250.0); // (600 - 100) / 2
    }

    #[test]
    fn test_justify_end() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "justify", "end");
        let child = tree.create("div");
        tree.set_style(child, "height", "100px");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);
        let lc = tree.get(child).unwrap().layout.clone();
        assert_eq!(lc.y, 500.0); // 600 - 100
    }

    #[test]
    fn test_justify_space_between() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "justify", "space-between");
        let a = tree.create("div");
        let b = tree.create("div");
        tree.set_style(a, "height", "100px");
        tree.set_style(b, "height", "100px");
        tree.append_child(1, a);
        tree.append_child(1, b);

        layout_tree(&mut tree, 800.0, 600.0);
        let la = tree.get(a).unwrap().layout.clone();
        let lb = tree.get(b).unwrap().layout.clone();
        assert_eq!(la.y, 0.0);
        assert_eq!(lb.y, 500.0); // 600 - 100
    }

    // ── 9. SizePolicy::Fixed ───────────────────────────────────────────

    #[test]
    fn test_size_policy_fixed() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "align", "start");
        let child = tree.create("div");
        tree.set_style(child, "width", "200px");
        tree.set_style(child, "height", "150px");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);
        let lc = tree.get(child).unwrap().layout.clone();
        assert_eq!(lc.width, 200.0);
        assert_eq!(lc.height, 150.0);
    }

    // ── 10. SizePolicy::Hug ────────────────────────────────────────────

    #[test]
    fn test_size_policy_hug_empty() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "align", "start");
        let child = tree.create("div");
        tree.set_style(child, "width", "hug");
        tree.set_style(child, "height", "hug");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);
        let lc = tree.get(child).unwrap().layout.clone();
        // Empty hug element with no children has 0 intrinsic size
        assert_eq!(lc.width, 0.0);
    }

    // ── 11. SizePolicy::Fill ───────────────────────────────────────────

    #[test]
    fn test_size_policy_fill_expands() {
        let mut tree = ElementTree::new();
        let child = tree.create("div");
        tree.set_style(child, "height", "fill");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);
        let lc = tree.get(child).unwrap().layout.clone();
        assert_eq!(lc.height, 600.0);
    }

    // ── 12. Fill with weights ──────────────────────────────────────────

    #[test]
    fn test_fill_with_weights() {
        let mut tree = ElementTree::new();
        let a = tree.create("div");
        let b = tree.create("div");
        tree.set_style(a, "height", "fill(1)");
        tree.set_style(b, "height", "fill(2)");
        tree.append_child(1, a);
        tree.append_child(1, b);

        layout_tree(&mut tree, 800.0, 600.0);
        let la = tree.get(a).unwrap().layout.clone();
        let lb = tree.get(b).unwrap().layout.clone();
        assert_eq!(la.height, 200.0); // 600 * 1/3
        assert_eq!(lb.height, 400.0); // 600 * 2/3
    }

    // ── 13. min-width / max-width constraints ──────────────────────────

    #[test]
    fn test_min_width_constraint() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "align", "start");
        let child = tree.create("div");
        tree.set_style(child, "width", "hug");
        tree.set_style(child, "min-width", "200px");
        tree.set_style(child, "height", "50px");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);
        let lc = tree.get(child).unwrap().layout.clone();
        assert!(lc.width >= 200.0);
    }

    #[test]
    fn test_max_width_constraint() {
        let mut tree = ElementTree::new();
        let child = tree.create("div");
        tree.set_style(child, "width", "fill");
        tree.set_style(child, "max-width", "400px");
        tree.set_style(child, "height", "50px");
        tree.append_child(1, child);

        layout_tree(&mut tree, 800.0, 600.0);
        let lc = tree.get(child).unwrap().layout.clone();
        assert_eq!(lc.width, 400.0);
    }

    // ── 14. display: none ──────────────────────────────────────────────

    #[test]
    fn test_display_none_zero_size() {
        let mut tree = ElementTree::new();
        let a = tree.create("div");
        let b = tree.create("div");
        tree.set_style(a, "height", "100px");
        tree.set_style(a, "display", "none");
        tree.set_style(b, "height", "100px");
        tree.append_child(1, a);
        tree.append_child(1, b);

        layout_tree(&mut tree, 800.0, 600.0);
        let la = tree.get(a).unwrap().layout.clone();
        let lb = tree.get(b).unwrap().layout.clone();
        assert_eq!(la.width, 0.0);
        assert_eq!(la.height, 0.0);
        // b should start at y=0 since a contributes 0 height
        assert_eq!(lb.y, 0.0);
    }

    // ── 15. Flex-wrap ──────────────────────────────────────────────────

    #[test]
    fn test_flex_wrap_items_wrap_to_next_line() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "direction", "horizontal");
        tree.set_style(1, "wrap", "true");
        tree.set_style(1, "align", "start");

        let a = tree.create("div");
        let b = tree.create("div");
        let c = tree.create("div");
        tree.set_style(a, "width", "500px");
        tree.set_style(a, "height", "100px");
        tree.set_style(b, "width", "500px");
        tree.set_style(b, "height", "100px");
        tree.set_style(c, "width", "200px");
        tree.set_style(c, "height", "80px");
        tree.append_child(1, a);
        tree.append_child(1, b);
        tree.append_child(1, c);

        layout_tree(&mut tree, 800.0, 600.0);
        let la = tree.get(a).unwrap().layout.clone();
        let lb = tree.get(b).unwrap().layout.clone();
        let lc = tree.get(c).unwrap().layout.clone();

        // a (500px) fits alone on line 1.
        assert_eq!(la.x, 0.0);
        assert_eq!(la.y, 0.0);
        assert_eq!(la.width, 500.0);
        // b (500px) wraps to line 2 since 500+500=1000 > 800
        assert_eq!(lb.x, 0.0);
        assert_eq!(lb.y, 100.0);
        // c (200px) fits next to b on line 2
        assert_eq!(lc.x, 500.0);
        assert_eq!(lc.y, 100.0);
    }

    // ── 16. Layer with top/left/right/bottom offsets ───────────────────

    #[test]
    fn test_layer_with_offsets() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "direction", "layer");

        let a = tree.create("div");
        tree.set_style(a, "width", "100px");
        tree.set_style(a, "height", "100px");
        tree.set_style(a, "top", "10px");
        tree.set_style(a, "left", "20px");
        tree.append_child(1, a);

        let b = tree.create("div");
        tree.set_style(b, "width", "100px");
        tree.set_style(b, "height", "100px");
        tree.set_style(b, "bottom", "10px");
        tree.set_style(b, "right", "20px");
        tree.append_child(1, b);

        layout_tree(&mut tree, 800.0, 600.0);
        let la = tree.get(a).unwrap().layout.clone();
        let lb = tree.get(b).unwrap().layout.clone();

        assert_eq!(la.x, 20.0);
        assert_eq!(la.y, 10.0);
        assert_eq!(lb.x, 680.0); // 800 - 100 - 20
        assert_eq!(lb.y, 490.0); // 600 - 100 - 10
    }

    // ── Extra: LayoutNode::content_box ─────────────────────────────────

    #[test]
    fn test_layout_node_content_box() {
        let node = LayoutNode {
            x: 10.0,
            y: 20.0,
            width: 200.0,
            height: 100.0,
            padding: Edges { top: 5.0, right: 10.0, bottom: 5.0, left: 10.0 },
        };
        let (cx, cy, cw, ch) = node.content_box();
        assert_eq!(cx, 20.0);  // 10 + 10
        assert_eq!(cy, 25.0);  // 20 + 5
        assert_eq!(cw, 180.0); // 200 - 10 - 10
        assert_eq!(ch, 90.0);  // 100 - 5 - 5
    }

    // ── Canvas demo structure test ─────────────────────────────────
    #[test]
    fn test_vertical_hug_with_horizontal_children() {
        let mut tree = ElementTree::new();

        // Root: vertical, hug height, 1400px width
        tree.set_style(1, "direction", "vertical");
        tree.set_style(1, "width", "1400px");
        tree.set_style(1, "height", "hug");
        tree.set_style(1, "gap", "12");
        tree.set_style(1, "padding", "20");

        // Header: vertical, hug height
        let header = tree.create("div");
        tree.append_child(1, header);
        tree.set_style(header, "direction", "vertical");
        tree.set_style(header, "height", "hug");
        tree.set_style(header, "padding", "16");

        // Title text in header
        let title = tree.create("div");
        tree.append_child(header, title);
        tree.get_mut(title).unwrap().text = Some("E-Commerce Product Grid".into());
        tree.set_style(title, "font-size", "24px");
        tree.set_style(title, "height", "hug");

        // Pills row: horizontal, hug, wrap
        let pills = tree.create("div");
        tree.append_child(1, pills);
        tree.set_style(pills, "direction", "horizontal");
        tree.set_style(pills, "height", "hug");
        tree.set_style(pills, "gap", "10");
        tree.set_style(pills, "wrap", "true");

        // 6 pill children
        for label in &["All", "Electronics", "Clothing", "Home", "Sports", "Books"] {
            let pill = tree.create("div");
            tree.append_child(pills, pill);
            tree.get_mut(pill).unwrap().text = Some(label.to_string());
            tree.set_style(pill, "width", "hug");
            tree.set_style(pill, "height", "hug");
            tree.set_style(pill, "padding", "8");
        }

        // Product grid: horizontal, wrap, hug height
        let grid = tree.create("div");
        tree.append_child(1, grid);
        tree.set_style(grid, "direction", "horizontal");
        tree.set_style(grid, "height", "hug");
        tree.set_style(grid, "gap", "16");
        tree.set_style(grid, "wrap", "true");

        // 10 product cards
        for _ in 0..10 {
            let card = tree.create("div");
            tree.append_child(grid, card);
            tree.set_style(card, "width", "260px");
            tree.set_style(card, "height", "340px");
        }

        layout_tree(&mut tree, 1400.0, 999999.0);

        // Root should hug its content
        let root = tree.get(1).unwrap();
        println!("root: x={} y={} w={} h={}", root.layout.x, root.layout.y, root.layout.width, root.layout.height);
        assert!(root.layout.height > 0.0, "root height should be > 0");
        assert!(root.layout.height < 10000.0, "root height should be reasonable");

        // Header should be compact (not filling viewport)
        let h = tree.get(header).unwrap();
        println!("header: x={} y={} w={} h={}", h.layout.x, h.layout.y, h.layout.width, h.layout.height);
        assert!(h.layout.height < 100.0, "header should be compact, got {}", h.layout.height);

        // Pills should be compact
        let p = tree.get(pills).unwrap();
        println!("pills: x={} y={} w={} h={}", p.layout.x, p.layout.y, p.layout.width, p.layout.height);
        assert!(p.layout.height < 60.0, "pills should be compact, got {}", p.layout.height);

        // Grid should start below header + pills
        let g = tree.get(grid).unwrap();
        println!("grid: x={} y={} w={} h={}", g.layout.x, g.layout.y, g.layout.width, g.layout.height);
        assert!(g.layout.y > 50.0, "grid should start below chrome, got y={}", g.layout.y);
        assert!(g.layout.y < 200.0, "grid should not be too far down, got y={}", g.layout.y);

        // Cards should be in rows of 5 (1400px - 40padding = 1360 / 276 = 4.9 → 5 per row)
        // 10 cards = 2 rows × 340px + gap = ~696px
        assert!(g.layout.height > 600.0, "grid should contain 2 rows of cards, got h={}", g.layout.height);
        assert!(g.layout.height < 1200.0, "grid height should be ~1052px for 3 rows, got h={}", g.layout.height);

        // First card position
        let first_card_id = tree.get(grid).unwrap().children[0];
        let fc = tree.get(first_card_id).unwrap();
        println!("first card: x={} y={} w={} h={}", fc.layout.x, fc.layout.y, fc.layout.width, fc.layout.height);
        assert_eq!(fc.layout.width, 260.0);
        assert_eq!(fc.layout.height, 340.0);
    }

    #[test]
    fn test_simple_hug_vertical() {
        let mut tree = ElementTree::new();
        // Root: vertical, hug
        tree.set_style(1, "direction", "vertical");
        tree.set_style(1, "height", "hug");
        tree.set_style(1, "width", "800px");

        let child = tree.create("div");
        tree.append_child(1, child);
        tree.get_mut(child).unwrap().text = Some("Hello".into());
        tree.set_style(child, "height", "hug");
        tree.set_style(child, "font-size", "16px");

        layout_tree(&mut tree, 800.0, 999999.0);

        let r = tree.get(1).unwrap();
        let c = tree.get(child).unwrap();
        println!("root: w={} h={}", r.layout.width, r.layout.height);
        println!("child: w={} h={}", c.layout.width, c.layout.height);
        assert!(r.layout.height < 100.0, "root should hug child, got h={}", r.layout.height);
        assert!(c.layout.height < 50.0, "child should hug text, got h={}", c.layout.height);
    }

    #[test]
    fn test_horizontal_wrap_9_items_3_cols() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "direction", "vertical");
        tree.set_style(1, "width", "1000px");
        tree.set_style(1, "height", "hug");

        let wrap = tree.create("div");
        tree.append_child(1, wrap);
        tree.set_style(wrap, "direction", "horizontal");
        tree.set_style(wrap, "wrap", "true");
        tree.set_style(wrap, "gap", "12");
        tree.set_style(wrap, "height", "hug");

        // 9 items at 300px each = 3 per row (300*3 + 12*2 = 924 < 1000)
        for _ in 0..9 {
            let item = tree.create("div");
            tree.append_child(wrap, item);
            tree.set_style(item, "width", "300px");
            tree.set_style(item, "height", "80px");
        }

        layout_tree(&mut tree, 1000.0, 999999.0);

        let w = tree.get(wrap).unwrap();
        println!("wrap container: w={} h={}", w.layout.width, w.layout.height);
        // 3 rows of 80px + 2 gaps of 12 = 264
        assert!(w.layout.height > 200.0, "wrap should have 3 rows, got h={}", w.layout.height);
        assert!(w.layout.height < 300.0, "wrap height should be ~264, got h={}", w.layout.height);
    }

    #[test]
    fn test_demo_section_with_metrics_and_grid() {
        let mut tree = ElementTree::new();

        // Root: vertical, hug, 1400px
        tree.set_style(1, "direction", "vertical");
        tree.set_style(1, "width", "1400px");
        tree.set_style(1, "height", "hug");
        tree.set_style(1, "padding", "24");
        tree.set_style(1, "gap", "16");
        tree.set_style(1, "align", "center");

        // Section card
        let section = tree.create("div");
        tree.append_child(1, section);
        tree.set_style(section, "direction", "vertical");
        tree.set_style(section, "height", "hug");
        tree.set_style(section, "max-width", "1400px");
        tree.set_style(section, "padding", "32");
        tree.set_style(section, "gap", "20");

        // Title (hug)
        let title = tree.create("div");
        tree.append_child(section, title);
        tree.get_mut(title).unwrap().text = Some("E-Commerce".into());
        tree.set_style(title, "height", "hug");
        tree.set_style(title, "font-size", "24px");

        // Metrics: horizontal wrap, 9 items at ~430px each (3 per row)
        let metrics = tree.create("div");
        tree.append_child(section, metrics);
        tree.set_style(metrics, "direction", "horizontal");
        tree.set_style(metrics, "wrap", "true");
        tree.set_style(metrics, "gap", "12");
        tree.set_style(metrics, "height", "hug");

        let metric_w = ((1400.0 - 48.0f32).min(1400.0) - 64.0 - 24.0) / 3.0;
        for _ in 0..9 {
            let card = tree.create("div");
            tree.append_child(metrics, card);
            tree.set_style(card, "width", &format!("{}px", metric_w));
            tree.set_style(card, "height", "80px");
        }

        // Product grid: horizontal wrap, 10 cards at 260px
        let grid = tree.create("div");
        tree.append_child(section, grid);
        tree.set_style(grid, "direction", "horizontal");
        tree.set_style(grid, "wrap", "true");
        tree.set_style(grid, "gap", "16");
        tree.set_style(grid, "height", "hug");

        for _ in 0..10 {
            let card = tree.create("div");
            tree.append_child(grid, card);
            tree.set_style(card, "width", "260px");
            tree.set_style(card, "height", "340px");
        }

        layout_tree(&mut tree, 1400.0, 999999.0);

        let s = tree.get(section).unwrap();
        let t = tree.get(title).unwrap();
        let m = tree.get(metrics).unwrap();
        let g = tree.get(grid).unwrap();

        println!("section: y={} h={}", s.layout.y, s.layout.height);
        println!("title:   y={} h={}", t.layout.y, t.layout.height);
        println!("metrics: y={} h={}", m.layout.y, m.layout.height);
        println!("grid:    y={} h={}", g.layout.y, g.layout.height);

        // Metrics should have 3 rows: 3 * 80 + 2 * 12 = 264
        assert!(m.layout.height > 200.0, "metrics should be 3 rows, got h={}", m.layout.height);

        // Grid should start BELOW metrics (no overlap)
        let metrics_bottom = m.layout.y + m.layout.height;
        assert!(g.layout.y >= metrics_bottom, "grid y={} should be >= metrics bottom={}", g.layout.y, metrics_bottom);
    }

    #[test]
    fn bench_40k_elements_layout() {
        let mut tree = ElementTree::new();
        tree.set_style(1, "direction", "vertical");
        tree.set_style(1, "width", "1400px");
        tree.set_style(1, "height", "hug");

        let grid = tree.create("div");
        tree.append_child(1, grid);
        tree.set_style(grid, "direction", "horizontal");
        tree.set_style(grid, "wrap", "true");
        tree.set_style(grid, "gap", "16");
        tree.set_style(grid, "height", "hug");

        for i in 0..10000u32 {
            let card = tree.create("div");
            tree.append_child(grid, card);
            tree.set_style(card, "width", "260px");
            tree.set_style(card, "height", "340px");

            let name = tree.create("div");
            tree.append_child(card, name);
            tree.get_mut(name).unwrap().text = Some(format!("Product #{}", i));
            tree.set_style(name, "height", "hug");

            let price = tree.create("div");
            tree.append_child(card, price);
            tree.get_mut(price).unwrap().text = Some("$4.99".into());
            tree.set_style(price, "height", "hug");

            let btn = tree.create("div");
            tree.append_child(card, btn);
            tree.get_mut(btn).unwrap().text = Some("Add to Cart".into());
            tree.set_style(btn, "height", "36px");
        }

        println!("Tree: {} elements", tree.element_count());
        let t0 = std::time::Instant::now();
        let mut measurer = EstimateMeasurer;
        compute(&mut tree, 1400.0, 999999.0, &mut measurer);
        let elapsed = t0.elapsed();
        println!("Layout compute: {:?}", elapsed);
        assert!(elapsed.as_millis() < 100, "Layout of 40K should be <100ms, got {:?}", elapsed);
    }

    #[test]
    fn test_canvas_app_structure_metrics_then_grid() {
        // Reproduce exact canvas app structure: section with multiple children
        // including wrap metrics and wrap product grid
        let mut tree = ElementTree::new();

        // Root: vertical, hug
        tree.set_style(1, "direction", "vertical");
        tree.set_style(1, "width", "1400px");
        tree.set_style(1, "height", "hug");
        tree.set_style(1, "padding", "24");
        tree.set_style(1, "gap", "16");
        tree.set_style(1, "align", "center");

        // Section: vertical, hug, padding, gap
        let section = tree.create("div");
        tree.append_child(1, section);
        tree.set_style(section, "direction", "vertical");
        tree.set_style(section, "height", "hug");
        tree.set_style(section, "max-width", "1400px");
        tree.set_style(section, "padding", "32");
        tree.set_style(section, "gap", "20");

        // Title
        let title = tree.create("div");
        tree.append_child(section, title);
        tree.get_mut(title).unwrap().text = Some("Title".into());
        tree.set_style(title, "height", "hug");

        // Controls row
        let controls = tree.create("div");
        tree.append_child(section, controls);
        tree.set_style(controls, "direction", "horizontal");
        tree.set_style(controls, "height", "44px");

        // Pills row
        let pills = tree.create("div");
        tree.append_child(section, pills);
        tree.set_style(pills, "direction", "horizontal");
        tree.set_style(pills, "wrap", "true");
        tree.set_style(pills, "height", "hug");
        tree.set_style(pills, "gap", "10");
        for label in &["All", "Electronics", "Clothing"] {
            let pill = tree.create("div");
            tree.append_child(pills, pill);
            tree.get_mut(pill).unwrap().text = Some(label.to_string());
            tree.set_style(pill, "width", "hug");
            tree.set_style(pill, "height", "36px");
            tree.set_style(pill, "padding", "8");
        }

        // Sort row
        let sort = tree.create("div");
        tree.append_child(section, sort);
        tree.set_style(sort, "direction", "horizontal");
        tree.set_style(sort, "height", "32px");

        // Metrics: horizontal wrap, 9 items at ~420px each = 3 per row
        let metrics = tree.create("div");
        tree.append_child(section, metrics);
        tree.set_style(metrics, "direction", "horizontal");
        tree.set_style(metrics, "wrap", "true");
        tree.set_style(metrics, "gap", "12");
        tree.set_style(metrics, "height", "hug");
        let metric_w = ((1400.0f32 - 48.0 - 64.0 - 24.0) / 3.0).floor();
        for _ in 0..9 {
            let card = tree.create("div");
            tree.append_child(metrics, card);
            tree.set_style(card, "width", &format!("{}px", metric_w));
            tree.set_style(card, "height", "80px");
        }

        // Grid: horizontal wrap, 20 cards at 260px
        let grid = tree.create("div");
        tree.append_child(section, grid);
        tree.set_style(grid, "direction", "horizontal");
        tree.set_style(grid, "wrap", "true");
        tree.set_style(grid, "gap", "16");
        tree.set_style(grid, "height", "hug");
        for _ in 0..20 {
            let card = tree.create("div");
            tree.append_child(grid, card);
            tree.set_style(card, "width", "260px");
            tree.set_style(card, "height", "340px");
        }

        layout_tree(&mut tree, 1400.0, 999999.0);

        let m = tree.get(metrics).unwrap();
        let g = tree.get(grid).unwrap();
        println!("metrics: y={} h={}", m.layout.y, m.layout.height);
        println!("grid:    y={} h={}", g.layout.y, g.layout.height);

        let metrics_bottom = m.layout.y + m.layout.height;
        assert!(g.layout.y >= metrics_bottom,
            "grid y={} must be >= metrics bottom={} (metrics y={} h={})",
            g.layout.y, metrics_bottom, m.layout.y, m.layout.height);
    }
}
