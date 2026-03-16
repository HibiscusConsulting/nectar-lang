//! Text measurement trait — implemented differently per platform.
//! Browser: Canvas 2D measureText via WASM import
//! Native: cosmic-text

/// Style information needed for text measurement.
#[derive(Debug, Clone)]
pub struct TextStyle {
    pub font_size: f32,
    pub font_weight: u16,    // 400 = normal, 700 = bold
    pub font_family: String, // e.g. "system-ui"
    pub italic: bool,
    pub line_height: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            font_weight: 400,
            font_family: "system-ui".into(),
            italic: false,
            line_height: 1.5,
        }
    }
}

/// Trait for measuring text dimensions.
/// Platform-specific implementations handle the actual measurement.
pub trait TextMeasurer {
    /// Measure text with given style, optionally constrained to max_width.
    /// Returns (width, height).
    fn measure(&mut self, text: &str, style: &TextStyle, max_width: Option<f32>) -> (f32, f32);
}

/// Fallback measurer that estimates based on character count.
/// Used when no platform-specific measurer is available.
pub struct EstimateMeasurer;

impl TextMeasurer for EstimateMeasurer {
    fn measure(&mut self, text: &str, style: &TextStyle, _max_width: Option<f32>) -> (f32, f32) {
        let char_width = style.font_size * 0.6;
        let width = text.len() as f32 * char_width;
        // line_height is already in pixels (e.g. 19.2 for font_size=16, not 1.2)
        let height = style.line_height;
        (width, height)
    }
}
