use crate::ast::*;

/// Built-in Nectar skeleton CSS — always inlined for instant loading feedback.
const ARC_SKELETON_CSS: &str = "\
.nectar-skeleton { background: linear-gradient(90deg, #1a1a2e 25%, #16213e 50%, #1a1a2e 75%); background-size: 200% 100%; animation: nectar-shimmer 1.5s infinite; border-radius: 4px; }\n\
@keyframes nectar-shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }\n\
.nectar-skeleton-text { height: 1em; margin: 0.5em 0; }\n\
.nectar-skeleton-avatar { width: 48px; height: 48px; border-radius: 50%; }\n\
.nectar-skeleton-rect { height: 100px; }";

/// Built-in Nectar base reset CSS — minimal reset for consistent rendering.
const ARC_BASE_RESET_CSS: &str = "\
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }\n\
[data-nectar-hydrate] { opacity: 1; transition: opacity 0.15s ease-in; }\n\
.nectar-loading { opacity: 0.6; pointer-events: none; }";

/// Result of critical CSS extraction from the program AST.
pub struct CriticalCssResult {
    /// CSS that should be inlined in `<head>` — styles from non-lazy components
    pub critical_css: String,
    /// CSS that should be loaded asynchronously — styles from lazy components
    pub deferred_css: String,
    /// Skeleton/base CSS that is always inlined regardless of component analysis
    pub skeleton_css: String,
}

/// Extracts and separates critical vs. deferred CSS from an Nectar program.
///
/// Critical CSS includes:
/// - Styles from non-lazy components (above the fold)
/// - Styles from the first route's component in any router
/// - Built-in skeleton and base reset styles
///
/// Deferred CSS includes:
/// - Styles from `lazy component` definitions
/// - Styles from components that are not the first route target
pub struct CriticalCssExtractor {
    /// Names of components referenced by the first route in each router
    first_route_components: Vec<String>,
    /// Names of all lazy components
    lazy_component_names: Vec<String>,
}

impl CriticalCssExtractor {
    pub fn new() -> Self {
        Self {
            first_route_components: Vec::new(),
            lazy_component_names: Vec::new(),
        }
    }

    /// Extract critical and deferred CSS from the entire program.
    pub fn extract(program: &Program) -> CriticalCssResult {
        let mut extractor = Self::new();

        // First pass: identify lazy components and first-route components
        extractor.analyze_program(program);

        // Second pass: collect and classify styles
        let mut critical_css = String::new();
        let mut deferred_css = String::new();

        for item in &program.items {
            match item {
                Item::Component(comp) => {
                    let css = Self::compile_styles(&comp.name, &comp.styles);
                    if !css.is_empty() {
                        // Non-lazy components are always critical
                        critical_css.push_str(&css);
                        critical_css.push('\n');
                    }
                }
                Item::LazyComponent(lazy) => {
                    let css = Self::compile_styles(&lazy.component.name, &lazy.component.styles);
                    if !css.is_empty() {
                        // Lazy components are deferred unless they are the
                        // first route target
                        if extractor.first_route_components.contains(&lazy.component.name) {
                            critical_css.push_str(&css);
                            critical_css.push('\n');
                        } else {
                            deferred_css.push_str(&css);
                            deferred_css.push('\n');
                        }
                    }
                }
                _ => {}
            }
        }

        let skeleton_css = format!("{}\n{}", ARC_BASE_RESET_CSS, ARC_SKELETON_CSS);

        CriticalCssResult {
            critical_css,
            deferred_css,
            skeleton_css,
        }
    }

    /// Analyze the program to determine which components are lazy and which
    /// are first-route targets.
    fn analyze_program(&mut self, program: &Program) {
        for item in &program.items {
            match item {
                Item::LazyComponent(lazy) => {
                    self.lazy_component_names.push(lazy.component.name.clone());
                }
                Item::Router(router) => {
                    // The first route in each router is considered above-the-fold
                    if let Some(first_route) = router.routes.first() {
                        self.first_route_components.push(first_route.component.clone());
                    }
                }
                _ => {}
            }
        }
    }

    /// Compile a component's StyleBlock list into a scoped CSS string.
    /// Uses the same hashing algorithm as the runtime's `hashString`.
    fn compile_styles(comp_name: &str, styles: &[StyleBlock]) -> String {
        if styles.is_empty() {
            return String::new();
        }

        let scope_id = format!("nectar-{}", hash_string(comp_name));
        let mut css = String::new();

        for block in styles {
            // Scope each selector with the component's data attribute
            let scoped_selectors: Vec<String> = block
                .selector
                .split(',')
                .map(|s| format!("[data-{}] {}", scope_id, s.trim()))
                .collect();

            css.push_str(&scoped_selectors.join(", "));
            css.push_str(" { ");
            for (prop, val) in &block.properties {
                css.push_str(prop);
                css.push_str(": ");
                css.push_str(val);
                css.push_str("; ");
            }
            css.push_str("}\n");
        }

        css
    }
}

/// Mirrors the runtime's `hashString` function (djb2 variant) so that
/// scoped selectors in the critical CSS match those generated at runtime.
///
/// ```
/// hash = 5381
/// for each char c: hash = ((hash << 5) + hash + c) & 0x7FFFFFFF
/// return base36(hash)
/// ```
fn hash_string(s: &str) -> String {
    let mut hash: u64 = 5381;
    for c in s.chars() {
        hash = ((hash << 5).wrapping_add(hash).wrapping_add(c as u64)) & 0x7FFFFFFF;
    }
    // Convert to base-36 string (matching JavaScript's Number.toString(36))
    to_base36(hash)
}

fn to_base36(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let chars: Vec<char> = "0123456789abcdefghijklmnopqrstuvwxyz".chars().collect();
    let mut result = Vec::new();
    while n > 0 {
        result.push(chars[(n % 36) as usize]);
        n /= 36;
    }
    result.reverse();
    result.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0)
    }

    #[test]
    fn test_hash_string_deterministic() {
        let h1 = hash_string("MyComponent");
        let h2 = hash_string("MyComponent");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_string_different_inputs() {
        let h1 = hash_string("Header");
        let h2 = hash_string("Footer");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_compile_styles_empty() {
        let result = CriticalCssExtractor::compile_styles("Test", &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_compile_styles_scopes_selector() {
        let styles = vec![StyleBlock {
            selector: ".card".to_string(),
            properties: vec![("padding".to_string(), "16px".to_string())],
            span: dummy_span(),
        }];
        let result = CriticalCssExtractor::compile_styles("Card", &styles);
        let scope_id = format!("nectar-{}", hash_string("Card"));
        assert!(result.contains(&format!("[data-{}] .card", scope_id)));
        assert!(result.contains("padding: 16px;"));
    }

    #[test]
    fn test_extract_separates_critical_and_deferred() {
        let program = Program {
            items: vec![
                Item::Component(Component {
                    name: "Header".to_string(),
                    type_params: vec![],
                    props: vec![],
                    state: vec![],
                    methods: vec![],
                    styles: vec![StyleBlock {
                        selector: ".header".to_string(),
                        properties: vec![("color".to_string(), "red".to_string())],
                        span: dummy_span(),
                    }],
                    transitions: vec![],
                    trait_bounds: vec![],
                    render: RenderBlock {
                        body: TemplateNode::Fragment(vec![]),
                        span: dummy_span(),
                    },
                    permissions: None,
                    gestures: vec![],
                    skeleton: None,
                    error_boundary: None,
                    chunk: None,
                    on_destroy: None,
                    a11y: None,
                    shortcuts: vec![],
                    span: dummy_span(),
                }),
                Item::LazyComponent(LazyComponentDef {
                    component: Component {
                        name: "HeavyChart".to_string(),
                        type_params: vec![],
                        props: vec![],
                        state: vec![],
                        methods: vec![],
                        styles: vec![StyleBlock {
                            selector: ".chart".to_string(),
                            properties: vec![("width".to_string(), "100%".to_string())],
                            span: dummy_span(),
                        }],
                        transitions: vec![],
                        trait_bounds: vec![],
                        render: RenderBlock {
                            body: TemplateNode::Fragment(vec![]),
                            span: dummy_span(),
                        },
                        permissions: None,
                        gestures: vec![],
                        skeleton: None,
                        error_boundary: None,
                        chunk: None,
                        on_destroy: None,
                        a11y: None,
                        shortcuts: vec![],
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                }),
            ],
        };

        let result = CriticalCssExtractor::extract(&program);

        // Header (non-lazy) should be in critical CSS
        assert!(result.critical_css.contains(".header"));
        // HeavyChart (lazy, not first route) should be in deferred CSS
        assert!(result.deferred_css.contains(".chart"));
        // Skeleton CSS should always be present
        assert!(result.skeleton_css.contains(".nectar-skeleton"));
        assert!(result.skeleton_css.contains("nectar-shimmer"));
    }

    // --- Edge case: to_base36 with zero ---

    #[test]
    fn test_to_base36_zero() {
        assert_eq!(to_base36(0), "0");
    }

    #[test]
    fn test_to_base36_small() {
        assert_eq!(to_base36(10), "a");
        assert_eq!(to_base36(35), "z");
        assert_eq!(to_base36(36), "10");
    }

    // --- Empty program ---

    #[test]
    fn test_extract_empty_program() {
        let program = Program { items: vec![] };
        let result = CriticalCssExtractor::extract(&program);
        assert!(result.critical_css.is_empty());
        assert!(result.deferred_css.is_empty());
        assert!(!result.skeleton_css.is_empty()); // skeleton always present
    }

    // --- Multiple selectors (comma-separated) ---

    #[test]
    fn test_compile_styles_multiple_selectors() {
        let styles = vec![StyleBlock {
            selector: ".a, .b".to_string(),
            properties: vec![("color".to_string(), "red".to_string())],
            span: dummy_span(),
        }];
        let result = CriticalCssExtractor::compile_styles("Multi", &styles);
        let scope_id = format!("nectar-{}", hash_string("Multi"));
        assert!(result.contains(&format!("[data-{}] .a", scope_id)));
        assert!(result.contains(&format!("[data-{}] .b", scope_id)));
    }

    // --- Multiple style blocks ---

    #[test]
    fn test_compile_styles_multiple_blocks() {
        let styles = vec![
            StyleBlock {
                selector: ".x".to_string(),
                properties: vec![("margin".to_string(), "0".to_string())],
                span: dummy_span(),
            },
            StyleBlock {
                selector: ".y".to_string(),
                properties: vec![("padding".to_string(), "10px".to_string())],
                span: dummy_span(),
            },
        ];
        let result = CriticalCssExtractor::compile_styles("Double", &styles);
        assert!(result.contains("margin: 0;"));
        assert!(result.contains("padding: 10px;"));
    }

    // --- Lazy component that is first route target goes to critical ---

    #[test]
    fn test_lazy_first_route_goes_critical() {
        let program = Program {
            items: vec![
                Item::Router(RouterDef {
                    name: "AppRouter".to_string(),
                    routes: vec![RouteDef {
                        path: "/".to_string(),
                        params: vec![],
                        component: "Home".to_string(),
                        guard: None,
                        transition: None,
                        span: dummy_span(),
                    }],
                    fallback: None,
                    layout: None,
                    transition: None,
                    span: dummy_span(),
                }),
                Item::LazyComponent(LazyComponentDef {
                    component: Component {
                        name: "Home".to_string(),
                        type_params: vec![],
                        props: vec![],
                        state: vec![],
                        methods: vec![],
                        styles: vec![StyleBlock {
                            selector: ".home".to_string(),
                            properties: vec![("display".to_string(), "block".to_string())],
                            span: dummy_span(),
                        }],
                        transitions: vec![],
                        trait_bounds: vec![],
                        render: RenderBlock {
                            body: TemplateNode::Fragment(vec![]),
                            span: dummy_span(),
                        },
                        permissions: None,
                        gestures: vec![],
                        skeleton: None,
                        error_boundary: None,
                        chunk: None,
                        on_destroy: None,
                        a11y: None,
                        shortcuts: vec![],
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                }),
            ],
        };
        let result = CriticalCssExtractor::extract(&program);
        assert!(result.critical_css.contains(".home"), "first-route lazy component should be critical");
        assert!(!result.deferred_css.contains(".home"));
    }

    // --- Component without styles has empty CSS ---

    #[test]
    fn test_component_without_styles() {
        let program = Program {
            items: vec![Item::Component(Component {
                name: "Empty".to_string(),
                type_params: vec![],
                props: vec![],
                state: vec![],
                methods: vec![],
                styles: vec![],
                transitions: vec![],
                trait_bounds: vec![],
                render: RenderBlock {
                    body: TemplateNode::Fragment(vec![]),
                    span: dummy_span(),
                },
                permissions: None,
                gestures: vec![],
                skeleton: None,
                error_boundary: None,
                chunk: None,
                on_destroy: None,
                a11y: None,
                shortcuts: vec![],
                span: dummy_span(),
            })],
        };
        let result = CriticalCssExtractor::extract(&program);
        assert!(result.critical_css.is_empty());
    }

    // --- Skeleton CSS always includes base reset ---

    #[test]
    fn test_skeleton_includes_base_reset() {
        let program = Program { items: vec![] };
        let result = CriticalCssExtractor::extract(&program);
        assert!(result.skeleton_css.contains("box-sizing: border-box"));
        assert!(result.skeleton_css.contains("data-nectar-hydrate"));
        assert!(result.skeleton_css.contains("nectar-loading"));
    }

    // --- Hash string empty input ---

    #[test]
    fn test_hash_string_empty() {
        let h = hash_string("");
        assert!(!h.is_empty());
        // djb2 with no chars: hash stays 5381
        assert_eq!(h, to_base36(5381));
    }

    // --- Multiple properties in a style block ---

    #[test]
    fn test_compile_styles_multiple_properties() {
        let styles = vec![StyleBlock {
            selector: ".btn".to_string(),
            properties: vec![
                ("color".to_string(), "blue".to_string()),
                ("padding".to_string(), "8px".to_string()),
                ("border".to_string(), "none".to_string()),
            ],
            span: dummy_span(),
        }];
        let result = CriticalCssExtractor::compile_styles("Button", &styles);
        assert!(result.contains("color: blue;"));
        assert!(result.contains("padding: 8px;"));
        assert!(result.contains("border: none;"));
    }

    // --- CriticalCssExtractor::new ---

    #[test]
    fn test_extractor_new() {
        let ext = CriticalCssExtractor::new();
        assert!(ext.first_route_components.is_empty());
        assert!(ext.lazy_component_names.is_empty());
    }
}
