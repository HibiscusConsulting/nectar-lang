use std::collections::HashMap;

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

/// Resolved layout style for one element — parsed from element's styles map.
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
    // Phase 1: collect all styles and children (avoids borrow issues during traversal)
    let ctx = LayoutContext::build(tree);

    // Phase 2: measure pass — compute intrinsic sizes bottom-up
    let mut intrinsics: HashMap<u32, (f32, f32)> = HashMap::new();
    measure_node(1, tree, &ctx, &mut intrinsics, measurer);

    // Phase 3: layout pass — assign positions and resolved sizes top-down
    // Root gets the full viewport
    layout_node(tree, &ctx, &intrinsics, 1, 0.0, 0.0, viewport_w, viewport_h, measurer);
}

// ── Context (pre-collected data to avoid borrow fights) ────────────────────

struct LayoutContext {
    styles: HashMap<u32, LayoutStyle>,
    children: HashMap<u32, Vec<u32>>,
}

impl LayoutContext {
    fn build(tree: &ElementTree) -> Self {
        let mut styles = HashMap::new();
        let mut children = HashMap::new();

        for (id, el) in tree.iter() {
            styles.insert(id, resolve_style(el));
            children.insert(id, el.children.clone());
        }

        Self { styles, children }
    }

    fn style(&self, id: u32) -> &LayoutStyle {
        static DEFAULT: std::sync::LazyLock<LayoutStyle> = std::sync::LazyLock::new(LayoutStyle::default);
        self.styles.get(&id).unwrap_or(&DEFAULT)
    }

    fn children(&self, id: u32) -> &[u32] {
        self.children.get(&id).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

// ── Measure pass (bottom-up) ───────────────────────────────────────────────
// Returns (intrinsic_width, intrinsic_height) — the size an element wants
// if given infinite space. Fill elements contribute 0 on their fill axis.

fn measure_node(
    id: u32,
    tree: &ElementTree,
    ctx: &LayoutContext,
    intrinsics: &mut HashMap<u32, (f32, f32)>,
    measurer: &mut dyn TextMeasurer,
) -> (f32, f32) {
    if let Some(&cached) = intrinsics.get(&id) {
        return cached;
    }

    // display: none — element contributes zero size
    if let Some(el) = tree.get(id) {
        if el.styles.get("display").map(|v| v == "none").unwrap_or(false) {
            intrinsics.insert(id, (0.0, 0.0));
            return (0.0, 0.0);
        }
    }

    let style = ctx.style(id);
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
        intrinsics.insert(id, result);
        return result;
    }

    // Measure all children first
    let child_sizes: Vec<(f32, f32)> = kids
        .iter()
        .map(|&kid| measure_node(kid, tree, ctx, intrinsics, measurer))
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
    intrinsics.insert(id, result);
    result
}

// ── Layout pass (top-down) ─────────────────────────────────────────────────
// Given resolved size from parent, position children.

fn layout_node(
    tree: &mut ElementTree,
    ctx: &LayoutContext,
    intrinsics: &HashMap<u32, (f32, f32)>,
    id: u32,
    x: f32,
    y: f32,
    available_w: f32,
    available_h: f32,
    measurer: &mut dyn TextMeasurer,
) {
    // display: none — skip layout entirely, zero out dimensions
    if let Some(el) = tree.get(id) {
        if el.styles.get("display").map(|v| v == "none").unwrap_or(false) {
            if let Some(el) = tree.get_mut(id) {
                el.layout.x = x;
                el.layout.y = y;
                el.layout.width = 0.0;
                el.layout.height = 0.0;
            }
            return;
        }
    }

    let style = ctx.style(id).clone();

    // Resolve own size (using intrinsic for Hug)
    let intrinsic = intrinsics.get(&id).copied().unwrap_or((0.0, 0.0));
    let resolved_w = resolve_size_with_intrinsic(style.width, available_w, intrinsic.0, style.min_width, style.max_width);
    let mut resolved_h = resolve_size_with_intrinsic(style.height, available_h, intrinsic.1, style.min_height, style.max_height);

    // For text nodes with Hug height, re-measure with the resolved width constraint
    // so that text wraps properly and the height reflects wrapped lines.
    // Respect white-space: nowrap — skip re-measurement if wrapping is disabled.
    if let Some(el) = tree.get(id) {
        if el.tag == "#text" {
            if let Some(text) = &el.text {
                let nowrap = el.styles.get("white-space").map(|v| v == "nowrap").unwrap_or(false);
                if matches!(style.height, SizePolicy::Hug) && !nowrap {
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
                let kid_w = resolve_size_with_intrinsic(kid_style.width, content_w, intrinsics.get(&kid).map(|s| s.0).unwrap_or(0.0), kid_style.min_width, kid_style.max_width);
                let kid_h = resolve_size_with_intrinsic(kid_style.height, content_h, intrinsics.get(&kid).map(|s| s.1).unwrap_or(0.0), kid_style.min_height, kid_style.max_height);

                // Position offsets within the layer (for tooltip/dropdown positioning)
                let kid_el_styles = tree.get(kid).map(|e| &e.styles);
                let offset_top = kid_el_styles.and_then(|s| s.get("top")).and_then(|v| parse_px(v));
                let offset_left = kid_el_styles.and_then(|s| s.get("left")).and_then(|v| parse_px(v));
                let offset_bottom = kid_el_styles.and_then(|s| s.get("bottom")).and_then(|v| parse_px(v));
                let offset_right = kid_el_styles.and_then(|s| s.get("right")).and_then(|v| parse_px(v));

                let kid_x = if let Some(left) = offset_left {
                    content_x + left
                } else if let Some(right) = offset_right {
                    content_x + content_w - kid_w - right
                } else {
                    content_x
                };
                let kid_y = if let Some(top) = offset_top {
                    content_y + top
                } else if let Some(bottom) = offset_bottom {
                    content_y + content_h - kid_h - bottom
                } else {
                    content_y
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
    intrinsics: &HashMap<u32, (f32, f32)>,
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
        let is_hidden = tree.get(kid)
            .and_then(|el| el.styles.get("display"))
            .map(|v| v == "none")
            .unwrap_or(false);
        if is_hidden {
            child_main_sizes.push(0.0);
            continue;
        }

        let kid_style = ctx.style(kid);
        let kid_intrinsic = intrinsics.get(&kid).copied().unwrap_or((0.0, 0.0));
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
        let kid_intrinsic = intrinsics.get(&kid).copied().unwrap_or((0.0, 0.0));
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
    intrinsics: &HashMap<u32, (f32, f32)>,
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
        let kid_intrinsic = intrinsics.get(&kid).copied().unwrap_or((0.0, 0.0));
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

fn resolve_style(el: &crate::element::Element) -> LayoutStyle {
    let styles = &el.styles;
    let mut ls = LayoutStyle::default();

    // Fast path: fixed dimensions bypass full style parsing
    if let Some(w) = el.fixed_width {
        ls.width = SizePolicy::Fixed(w);
    }
    if let Some(h) = el.fixed_height {
        ls.height = SizePolicy::Fixed(h);
    }
    if el.fixed_width.is_some() && el.fixed_height.is_some() && styles.is_empty() {
        return ls;
    }

    // Direction: Nectar-native or CSS flex-direction
    if let Some(dir) = styles.get("direction") {
        ls.direction = match dir.as_str() {
            "horizontal" | "row" => Direction::Horizontal,
            "vertical" | "column" => Direction::Vertical,
            "layer" | "stack" => Direction::Layer,
            _ => Direction::Vertical,
        };
    } else if let Some(fd) = styles.get("flex-direction") {
        ls.direction = match fd.as_str() {
            "row" | "row-reverse" => Direction::Horizontal,
            _ => Direction::Vertical,
        };
    }

    // Gap
    if let Some(g) = styles.get("gap").and_then(|v| parse_px(v)) {
        ls.gap = g;
    }

    // Padding
    if let Some(p) = styles.get("pad").or_else(|| styles.get("padding")) {
        ls.pad = parse_edges(p);
    }
    // Individual padding overrides
    if let Some(v) = styles.get("padding-top").and_then(|v| parse_px(v)) { ls.pad.top = v; }
    if let Some(v) = styles.get("padding-right").and_then(|v| parse_px(v)) { ls.pad.right = v; }
    if let Some(v) = styles.get("padding-bottom").and_then(|v| parse_px(v)) { ls.pad.bottom = v; }
    if let Some(v) = styles.get("padding-left").and_then(|v| parse_px(v)) { ls.pad.left = v; }

    // Align (cross-axis)
    if let Some(a) = styles.get("align").or_else(|| styles.get("align-items")) {
        ls.align = match a.as_str() {
            "start" | "flex-start" => Align::Start,
            "center" => Align::Center,
            "end" | "flex-end" => Align::End,
            "stretch" => Align::Stretch,
            _ => Align::Stretch,
        };
    }

    // Justify (main-axis)
    if let Some(j) = styles.get("justify").or_else(|| styles.get("justify-content")) {
        ls.justify = match j.as_str() {
            "start" | "flex-start" => Justify::Start,
            "center" => Justify::Center,
            "end" | "flex-end" => Justify::End,
            "space-between" => Justify::SpaceBetween,
            _ => Justify::Start,
        };
    }

    // Width
    if let Some(w) = styles.get("width") {
        ls.width = parse_size_policy(w);
    }

    // Height
    if let Some(h) = styles.get("height") {
        ls.height = parse_size_policy(h);
    }

    // Nectar-native "size" shorthand: sets both axes
    if let Some(s) = styles.get("size") {
        let policy = parse_size_policy(s);
        ls.width = policy;
        ls.height = policy;
    }

    // Min/max constraints
    ls.min_width = styles.get("min-width").and_then(|v| parse_px(v));
    ls.max_width = styles.get("max-width").and_then(|v| parse_px(v));
    ls.min_height = styles.get("min-height").and_then(|v| parse_px(v));
    ls.max_height = styles.get("max-height").and_then(|v| parse_px(v));

    // Scroll
    if let Some(s) = styles.get("scroll") {
        ls.scroll = s == "true" || s == "vertical" || s == "horizontal" || s == "both";
    } else if let Some(o) = styles.get("overflow") {
        ls.scroll = o == "auto" || o == "scroll";
    }

    // Wrap
    if let Some(w) = styles.get("wrap") {
        ls.wrap = w == "true";
    } else if let Some(fw) = styles.get("flex-wrap") {
        ls.wrap = fw == "wrap";
    }

    // Text nodes: always hug
    if el.tag == "#text" {
        ls.width = SizePolicy::Hug;
        ls.height = SizePolicy::Hug;
    }

    ls
}

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
        tree.get_mut(1).unwrap().styles.insert("direction".into(), "vertical".into());
        tree.get_mut(1).unwrap().styles.insert("width".into(), "1400px".into());
        tree.get_mut(1).unwrap().styles.insert("height".into(), "hug".into());
        tree.get_mut(1).unwrap().styles.insert("gap".into(), "12".into());
        tree.get_mut(1).unwrap().styles.insert("padding".into(), "20".into());

        // Header: vertical, hug height
        let header = tree.create("div");
        tree.append_child(1, header);
        tree.get_mut(header).unwrap().styles.insert("direction".into(), "vertical".into());
        tree.get_mut(header).unwrap().styles.insert("height".into(), "hug".into());
        tree.get_mut(header).unwrap().styles.insert("padding".into(), "16".into());

        // Title text in header
        let title = tree.create("div");
        tree.append_child(header, title);
        tree.get_mut(title).unwrap().text = Some("E-Commerce Product Grid".into());
        tree.get_mut(title).unwrap().styles.insert("font-size".into(), "24px".into());
        tree.get_mut(title).unwrap().styles.insert("height".into(), "hug".into());

        // Pills row: horizontal, hug, wrap
        let pills = tree.create("div");
        tree.append_child(1, pills);
        tree.get_mut(pills).unwrap().styles.insert("direction".into(), "horizontal".into());
        tree.get_mut(pills).unwrap().styles.insert("height".into(), "hug".into());
        tree.get_mut(pills).unwrap().styles.insert("gap".into(), "10".into());
        tree.get_mut(pills).unwrap().styles.insert("wrap".into(), "true".into());

        // 6 pill children
        for label in &["All", "Electronics", "Clothing", "Home", "Sports", "Books"] {
            let pill = tree.create("div");
            tree.append_child(pills, pill);
            tree.get_mut(pill).unwrap().text = Some(label.to_string());
            tree.get_mut(pill).unwrap().styles.insert("width".into(), "hug".into());
            tree.get_mut(pill).unwrap().styles.insert("height".into(), "hug".into());
            tree.get_mut(pill).unwrap().styles.insert("padding".into(), "8".into());
        }

        // Product grid: horizontal, wrap, hug height
        let grid = tree.create("div");
        tree.append_child(1, grid);
        tree.get_mut(grid).unwrap().styles.insert("direction".into(), "horizontal".into());
        tree.get_mut(grid).unwrap().styles.insert("height".into(), "hug".into());
        tree.get_mut(grid).unwrap().styles.insert("gap".into(), "16".into());
        tree.get_mut(grid).unwrap().styles.insert("wrap".into(), "true".into());

        // 10 product cards
        for _ in 0..10 {
            let card = tree.create("div");
            tree.append_child(grid, card);
            tree.get_mut(card).unwrap().styles.insert("width".into(), "260px".into());
            tree.get_mut(card).unwrap().styles.insert("height".into(), "340px".into());
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
        tree.get_mut(1).unwrap().styles.insert("direction".into(), "vertical".into());
        tree.get_mut(1).unwrap().styles.insert("height".into(), "hug".into());
        tree.get_mut(1).unwrap().styles.insert("width".into(), "800px".into());

        let child = tree.create("div");
        tree.append_child(1, child);
        tree.get_mut(child).unwrap().text = Some("Hello".into());
        tree.get_mut(child).unwrap().styles.insert("height".into(), "hug".into());
        tree.get_mut(child).unwrap().styles.insert("font-size".into(), "16px".into());

        layout_tree(&mut tree, 800.0, 999999.0);

        let r = tree.get(1).unwrap();
        let c = tree.get(child).unwrap();
        println!("root: w={} h={}", r.layout.width, r.layout.height);
        println!("child: w={} h={}", c.layout.width, c.layout.height);
        assert!(r.layout.height < 100.0, "root should hug child, got h={}", r.layout.height);
        assert!(c.layout.height < 50.0, "child should hug text, got h={}", c.layout.height);
    }
}
