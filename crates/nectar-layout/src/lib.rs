//! nectar-layout: Portable layout engine for Nectar
//!
//! Compiles to both native (for nectar-runtime) and WASM (for browser canvas mode).
//! Provides element tree, stack-based layout, and text measurement trait.
//!
//! Architecture:
//!   ElementTree (element pool) → LayoutStyle resolution → Two-pass layout → LayoutNode results
//!
//! The layout engine handles:
//!   - Vertical/Horizontal/Layer stacking
//!   - Fill/Hug/Fixed sizing
//!   - Gap, padding, alignment, justification
//!   - Text wrapping (via TextMeasurer trait)
//!   - Scroll containers
//!   - Hit testing
//!
//! Rendering is NOT included — the consumer (browser canvas, wgpu, etc.)
//! reads the computed LayoutNode positions and draws accordingly.

pub mod element;
pub mod layout;
pub mod measure;
pub mod wasm_api;
pub mod canvas_syscalls;
pub mod canvas_app;
