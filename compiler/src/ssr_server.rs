use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Server-Side Rendering server powered by wasmtime.
///
/// Runs the compiled `.wasm` binary on the server with stub imports that
/// capture rendered HTML instead of manipulating a real DOM. The same WASM
/// that runs in the browser runs here — only the import implementations differ.
///
/// Architecture:
///   Browser request → axum handler → wasmtime runs app.wasm →
///   DOM stubs build an element table → serialize to HTML → respond
///
/// The browser receives complete HTML immediately (fast first paint),
/// then loads the same .wasm + core.js to hydrate for interactivity.

/// A server-side element — flat table entry, not a tree node.
/// The WASM assigns integer IDs; we track tag, attrs, children, text.
#[derive(Clone)]
struct SsrElement {
    tag: String,
    attrs: Vec<(String, String)>,
    styles: Vec<(String, String)>,
    children: Vec<i32>,
    text: Option<String>,
    inner_html: Option<String>,
    is_text_node: bool,
}

impl SsrElement {
    fn new(tag: &str) -> Self {
        Self {
            tag: tag.to_string(),
            attrs: Vec::new(),
            styles: Vec::new(),
            children: Vec::new(),
            text: None,
            inner_html: None,
            is_text_node: false,
        }
    }

    fn text_node(content: &str) -> Self {
        Self {
            tag: String::new(),
            attrs: Vec::new(),
            styles: Vec::new(),
            children: Vec::new(),
            text: Some(content.to_string()),
            inner_html: None,
            is_text_node: true,
        }
    }
}

/// Server-side state passed to wasmtime host functions.
pub struct ServerState {
    /// The current request path (injected per-request for routing)
    pub request_path: String,
    /// Element table: ID → element. IDs are assigned by createElement/createTextNode.
    elements: HashMap<i32, SsrElement>,
    /// Next element ID to assign
    next_id: i32,
    /// The root element ID (the #app container)
    root_id: i32,
    /// Injected CSS blocks
    style_blocks: Vec<String>,
}

impl Default for ServerState {
    fn default() -> Self {
        let mut elements = HashMap::new();
        // Pre-create element 1 as the root (#app div)
        elements.insert(1, SsrElement::new("div"));
        Self {
            request_path: String::new(),
            elements,
            next_id: 2,
            root_id: 1,
            style_blocks: Vec::new(),
        }
    }
}

impl ServerState {
    pub fn new(request_path: String) -> Self {
        Self {
            request_path,
            ..Default::default()
        }
    }

    fn alloc_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Serialize the element tree rooted at `root_id` to an HTML string.
    pub fn serialize_html(&self) -> String {
        let mut out = String::new();
        // Inject styles first
        for css in &self.style_blocks {
            out.push_str("<style>");
            out.push_str(css);
            out.push_str("</style>");
        }
        self.serialize_element(self.root_id, &mut out);
        out
    }

    fn serialize_element(&self, id: i32, out: &mut String) {
        let el = match self.elements.get(&id) {
            Some(el) => el,
            None => return,
        };

        if el.is_text_node {
            if let Some(ref t) = el.text {
                out.push_str(&html_escape(t));
            }
            return;
        }

        // Skip the root wrapper div — just emit children
        if id == self.root_id {
            for &child_id in &el.children {
                self.serialize_element(child_id, out);
            }
            return;
        }

        out.push('<');
        out.push_str(&el.tag);

        for (k, v) in &el.attrs {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            out.push_str(&html_escape_attr(v));
            out.push('"');
        }

        if !el.styles.is_empty() {
            out.push_str(" style=\"");
            for (i, (prop, val)) in el.styles.iter().enumerate() {
                if i > 0 { out.push_str("; "); }
                out.push_str(prop);
                out.push_str(": ");
                out.push_str(val);
            }
            out.push('"');
        }

        // Void elements
        if is_void_element(&el.tag) {
            out.push_str(" />");
            return;
        }

        out.push('>');

        if let Some(ref html) = el.inner_html {
            out.push_str(html);
        } else {
            if let Some(ref t) = el.text {
                out.push_str(&html_escape(t));
            }
            for &child_id in &el.children {
                self.serialize_element(child_id, out);
            }
        }

        out.push_str("</");
        out.push_str(&el.tag);
        out.push('>');
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
}

fn html_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('"', "&quot;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
}

fn is_void_element(tag: &str) -> bool {
    matches!(tag, "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta" | "source" | "track" | "wbr")
}

/// Configuration for the SSR server.
pub struct SsrServerConfig {
    /// Path to the compiled .wasm file
    pub wasm_path: PathBuf,
    /// Port to listen on
    pub port: u16,
    /// Directory for static assets (core.js, images, etc.)
    pub static_dir: Option<PathBuf>,
    /// Base URL for API requests during SSR
    pub api_base_url: Option<String>,
    /// Bearer token for authenticated API endpoints
    pub api_token: Option<String>,
}

/// Shared state for the axum server.
pub struct SsrServer {
    /// Raw WASM bytes (loaded once, instantiated per-request)
    pub wasm_bytes: Vec<u8>,
    /// Static assets directory
    pub static_dir: Option<PathBuf>,
    /// API configuration
    pub api_base_url: Option<String>,
    pub api_token: Option<String>,
}

/// Start the SSR server.
///
/// This is the entry point called from `nectar serve`. It:
/// 1. Loads the compiled WASM binary
/// 2. Sets up the axum router with SSR handler + static file serving
/// 3. Listens for HTTP requests
/// 4. For each request, creates a fresh wasmtime instance with server-side stubs
/// 5. Runs the WASM module, captures rendered HTML, sends response
pub fn serve(config: SsrServerConfig) -> anyhow::Result<()> {
    let wasm_bytes = match std::fs::read(&config.wasm_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            eprintln!("nectar serve: no WASM file at {} — starting in standby mode", config.wasm_path.display());
            Vec::new()
        }
    };

    let server = Arc::new(SsrServer {
        wasm_bytes,
        static_dir: config.static_dir.clone(),
        api_base_url: config.api_base_url,
        api_token: config.api_token,
    });

    // Build the tokio runtime and run the async server
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| anyhow::anyhow!("failed to create tokio runtime: {}", e))?;

    rt.block_on(async move {
        run_server(server, config.port, config.static_dir).await
    })
}

/// Serve a static HTML page from the static directory.
/// Looks for `{name}.html` in the static dir for routes like `/{name}`.
async fn static_page_handler(
    axum::extract::State(state): axum::extract::State<(Arc<SsrServer>, PathBuf)>,
    request: axum::extract::Request,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let path = request.uri().path();
    let name = path.trim_start_matches('/');
    let file_path = state.1.join(format!("{}.html", name));

    if let Ok(contents) = tokio::fs::read_to_string(&file_path).await {
        axum::response::Html(contents).into_response()
    } else {
        // Fall through to SSR
        match render_with_wasmtime(&state.0.wasm_bytes, path) {
            Ok(ssr_state) => {
                let ssr_content = ssr_state.serialize_html();
                let html = build_html_shell(&ssr_content, path);
                axum::response::Html(html).into_response()
            }
            Err(e) => {
                eprintln!("SSR render error for {}: {}", path, e);
                axum::response::Html(build_error_page(path, &format!("{}", e))).into_response()
            }
        }
    }
}

/// Simple in-memory rate limiter: global request counter with a per-second cap.
/// In production, per-IP tracking would use a concurrent HashMap; this provides
/// a baseline safeguard without external dependencies.
struct RateLimiter {
    /// Number of requests in the current window
    count: AtomicU64,
    /// Maximum requests per window
    max_per_window: u64,
}

impl RateLimiter {
    fn new(max_per_window: u64) -> Self {
        Self {
            count: AtomicU64::new(0),
            max_per_window,
        }
    }

    /// Returns true if the request is allowed.
    fn allow(&self) -> bool {
        let current = self.count.fetch_add(1, Ordering::Relaxed);
        current < self.max_per_window
    }

    /// Reset the counter (called periodically).
    fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
    }
}

/// Shared state for the API proxy handler.
struct ApiProxyState {
    http_client: reqwest::Client,
    rate_limiter: Arc<RateLimiter>,
}

/// Resolve provider env vars from a provider name.
/// `/api/payment` → `NECTAR_PAYMENT_URL` + `NECTAR_PAYMENT_KEY`
fn resolve_provider(provider: &str) -> (String, String) {
    let upper = provider.to_uppercase();
    let base_url = std::env::var(format!("NECTAR_{}_URL", upper)).unwrap_or_default();
    let api_key = std::env::var(format!("NECTAR_{}_KEY", upper)).unwrap_or_default();
    (base_url, api_key)
}

/// Validate that a byte slice is valid JSON (basic structural check).
fn is_valid_json(data: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(data).is_ok()
}

/// Handle `/api/{provider}` requests by proxying to the configured provider URL.
///
/// The provider is determined by the path segment after `/api/`. Environment
/// variables `NECTAR_{PROVIDER}_URL` and `NECTAR_{PROVIDER}_KEY` configure the
/// target endpoint and authorization token respectively.
///
/// Security: API keys never leave the server. Request bodies are validated as
/// JSON before forwarding. A global rate limiter prevents abuse.
async fn api_proxy_handler(
    axum::extract::State(state): axum::extract::State<Arc<ApiProxyState>>,
    axum::extract::Path(provider): axum::extract::Path<String>,
    request: axum::extract::Request,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let start = Instant::now();

    // Rate limit check
    if !state.rate_limiter.allow() {
        eprintln!("api_proxy: rate limit exceeded for provider={}", provider);
        return (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded").into_response();
    }

    // Resolve provider config from environment
    let (base_url, api_key) = resolve_provider(&provider);

    if base_url.is_empty() {
        eprintln!("api_proxy: provider '{}' not configured (no NECTAR_{}_URL)", provider, provider.to_uppercase());
        return (StatusCode::NOT_FOUND, "provider not configured").into_response();
    }

    // Extract the HTTP method from the inbound request
    let method = request.method().clone();

    // Read the request body (max 1 MB)
    let body_bytes = match axum::body::to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
        }
    };

    // Validate JSON if there is a body
    if !body_bytes.is_empty() && !is_valid_json(&body_bytes) {
        return (StatusCode::BAD_REQUEST, "invalid JSON body").into_response();
    }

    // Build the outbound request
    let mut outbound = state.http_client
        .request(method.clone(), &base_url)
        .header("Content-Type", "application/json");

    if !api_key.is_empty() {
        outbound = outbound.header("Authorization", format!("Bearer {}", api_key));
    }

    if !body_bytes.is_empty() {
        outbound = outbound.body(body_bytes.to_vec());
    }

    // Send the request to the provider
    let result = outbound.send().await;

    let elapsed = start.elapsed();

    match result {
        Ok(resp) => {
            let status = resp.status();
            eprintln!(
                "api_proxy: provider={} method={} status={} latency={}ms",
                provider, method, status.as_u16(), elapsed.as_millis()
            );

            let resp_status = StatusCode::from_u16(status.as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let resp_body = resp.text().await.unwrap_or_default();

            (
                resp_status,
                [("content-type", "application/json")],
                resp_body,
            ).into_response()
        }
        Err(e) => {
            eprintln!(
                "api_proxy: provider={} method={} error={} latency={}ms",
                provider, method, e, elapsed.as_millis()
            );
            (StatusCode::BAD_GATEWAY, format!("upstream request failed: {}", e)).into_response()
        }
    }
}

/// Async server loop.
async fn run_server(
    server: Arc<SsrServer>,
    port: u16,
    static_dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    use axum::{Router, routing::{get, any}};
    use tower_http::services::ServeDir;

    // Health check endpoint (Cloud Run uses this)
    let health = Router::new()
        .route("/_health", get(|| async { "ok" }));

    // Set up the API proxy with rate limiting
    let rate_limiter = Arc::new(RateLimiter::new(1000));
    let api_state = Arc::new(ApiProxyState {
        http_client: reqwest::Client::new(),
        rate_limiter: rate_limiter.clone(),
    });

    // Spawn a background task to reset the rate limiter every 60 seconds
    let rl = rate_limiter.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            rl.reset();
        }
    });

    // API proxy route: /api/{provider} accepts any HTTP method
    let api_router = Router::new()
        .route("/api/{provider}", any(api_proxy_handler))
        .with_state(api_state);

    // Build the SSR-only router first (needs state)
    let ssr_router: Router<Arc<SsrServer>> = Router::new()
        .fallback(get(ssr_handler));
    let ssr_router: Router = health
        .merge(api_router)
        .merge(ssr_router.with_state(server.clone()));

    // If we have a static dir, check for static HTML pages and serve them
    // at clean URLs (e.g., /examples serves examples.html).
    let app: Router = if let Some(ref dir) = static_dir {
        // Scan for .html files in the static dir and add routes for them
        let mut page_router = Router::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "html") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if stem != "index" {
                            let route = format!("/{}", stem);
                            let state = (server.clone(), dir.clone());
                            page_router = page_router.route(
                                &route,
                                get(static_page_handler).with_state(state),
                            );
                        }
                    }
                }
            }
        }

        let serve_dir = ServeDir::new(dir).fallback(ssr_router);
        page_router.fallback_service(serve_dir)
    } else {
        ssr_router
    };

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    eprintln!("nectar serve: SSR server running at http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(addr).await
        .map_err(|e| anyhow::anyhow!("failed to bind to port {}: {}", port, e))?;

    axum::serve(listener, app).await
        .map_err(|e| anyhow::anyhow!("server error: {}", e))
}

/// Handle an incoming HTTP request by rendering the page with wasmtime.
async fn ssr_handler(
    axum::extract::State(server): axum::extract::State<Arc<SsrServer>>,
    request: axum::extract::Request,
) -> axum::response::Html<String> {
    let path = request.uri().path().to_string();

    if server.wasm_bytes.is_empty() {
        return axum::response::Html(
            "<html><body><h1>Nectar Edge</h1><p>Runtime ready. No application deployed.</p></body></html>".to_string()
        );
    }

    match render_with_wasmtime(&server.wasm_bytes, &path) {
        Ok(state) => {
            let ssr_content = state.serialize_html();
            let html = build_html_shell(&ssr_content, &path);
            axum::response::Html(html)
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            eprintln!("SSR render error for {}: {}", path, err_msg);
            axum::response::Html(build_error_page(&path, &err_msg))
        }
    }
}

/// Render a page by running the WASM module in wasmtime with server-side stubs.
///
/// Creates a fresh wasmtime Store per request (isolation), instantiates the
/// module with stub imports, calls the appropriate mount/init function, and
/// returns the captured HTML from ServerState.
fn render_with_wasmtime(wasm_bytes: &[u8], request_path: &str) -> anyhow::Result<ServerState> {
    use wasmtime::*;

    let engine = Engine::default();
    let module = Module::new(&engine, wasm_bytes)
        .map_err(|e| anyhow::anyhow!("failed to compile WASM module: {}", e))?;

    let state = ServerState::new(request_path.to_string());

    let mut linker = Linker::new(&engine);

    // ── Provide linear memory ──────────────────────────────────────────
    let memory_type = MemoryType::new(256, None);
    let mut store = Store::new(&engine, state);

    let memory = Memory::new(&mut store, memory_type)
        .map_err(|e| anyhow::anyhow!("failed to create memory: {}", e))?;
    linker.define(&store, "env", "memory", memory)?;

    // ── DOM stubs that build the SSR element table ─────────────────

    // Helper: read a string from WASM linear memory
    fn read_wasm_str(memory: Memory, caller: &Caller<'_, ServerState>, ptr: i32, len: i32) -> String {
        let data = memory.data(caller);
        let start = ptr as usize;
        let end = start + len as usize;
        if end <= data.len() {
            String::from_utf8_lossy(&data[start..end]).to_string()
        } else {
            String::new()
        }
    }

    // createElement(tag_ptr, tag_len) -> element_id
    let mem_ce = memory;
    linker.func_wrap("dom", "createElement", move |mut caller: Caller<'_, ServerState>, ptr: i32, len: i32| -> i32 {
        let tag = read_wasm_str(mem_ce, &caller, ptr, len);
        let id = caller.data_mut().alloc_id();
        caller.data_mut().elements.insert(id, SsrElement::new(&tag));
        id
    })?;

    // createTextNode(text_ptr, text_len) -> element_id
    let mem_tn = memory;
    linker.func_wrap("dom", "createTextNode", move |mut caller: Caller<'_, ServerState>, ptr: i32, len: i32| -> i32 {
        let text = read_wasm_str(mem_tn, &caller, ptr, len);
        let id = caller.data_mut().alloc_id();
        caller.data_mut().elements.insert(id, SsrElement::text_node(&text));
        id
    })?;

    // setText(el_id, text_ptr, text_len)
    let mem_st = memory;
    linker.func_wrap("dom", "setText", move |mut caller: Caller<'_, ServerState>, el: i32, ptr: i32, len: i32| {
        let text = read_wasm_str(mem_st, &caller, ptr, len);
        if let Some(elem) = caller.data_mut().elements.get_mut(&el) {
            elem.text = Some(text);
        }
    })?;

    // appendChild(parent_id, child_id)
    linker.func_wrap("dom", "appendChild", |mut caller: Caller<'_, ServerState>, parent: i32, child: i32| {
        if let Some(p) = caller.data_mut().elements.get_mut(&parent) {
            p.children.push(child);
        }
    })?;

    // setAttribute(el_id, key_ptr, key_len, val_ptr, val_len)
    let mem_sa = memory;
    linker.func_wrap("dom", "setAttribute", move |mut caller: Caller<'_, ServerState>, el: i32, k_ptr: i32, k_len: i32, v_ptr: i32, v_len: i32| {
        let key = read_wasm_str(mem_sa, &caller, k_ptr, k_len);
        let val = read_wasm_str(mem_sa, &caller, v_ptr, v_len);
        if let Some(elem) = caller.data_mut().elements.get_mut(&el) {
            elem.attrs.push((key, val));
        }
    })?;

    // setStyle(el_id, prop_ptr, prop_len, val_ptr, val_len)
    let mem_ss = memory;
    linker.func_wrap("dom", "setStyle", move |mut caller: Caller<'_, ServerState>, el: i32, p_ptr: i32, p_len: i32, v_ptr: i32, v_len: i32| {
        let prop = read_wasm_str(mem_ss, &caller, p_ptr, p_len);
        let val = read_wasm_str(mem_ss, &caller, v_ptr, v_len);
        if let Some(elem) = caller.data_mut().elements.get_mut(&el) {
            elem.styles.push((prop, val));
        }
    })?;

    // setInnerHTML(el_id, html_ptr, html_len)
    let mem_ih = memory;
    linker.func_wrap("dom", "setInnerHTML", move |mut caller: Caller<'_, ServerState>, el: i32, ptr: i32, len: i32| {
        let html = read_wasm_str(mem_ih, &caller, ptr, len);
        if let Some(elem) = caller.data_mut().elements.get_mut(&el) {
            elem.inner_html = Some(html);
        }
    })?;

    // mount(container_id, html_ptr, html_len) — innerHTML-based mount
    let mem_m = memory;
    linker.func_wrap("dom", "mount", move |mut caller: Caller<'_, ServerState>, container: i32, ptr: i32, len: i32| {
        let html = read_wasm_str(mem_m, &caller, ptr, len);
        if let Some(elem) = caller.data_mut().elements.get_mut(&container) {
            elem.inner_html = Some(html);
        } else {
            // Mount to root if container doesn't exist
            let root_id = caller.data().root_id;
            if let Some(elem) = caller.data_mut().elements.get_mut(&root_id) {
                elem.inner_html = Some(html);
            }
        }
    })?;

    // getRoot() -> element_id (the #app container)
    linker.func_wrap("dom", "getRoot", |caller: Caller<'_, ServerState>| -> i32 {
        caller.data().root_id
    })?;

    // getBody() -> element_id
    linker.func_wrap("dom", "getBody", |caller: Caller<'_, ServerState>| -> i32 {
        caller.data().root_id
    })?;

    // getHead() -> element_id (just return root; styles go to style_blocks)
    linker.func_wrap("dom", "getHead", |caller: Caller<'_, ServerState>| -> i32 {
        caller.data().root_id
    })?;

    // getDocumentElement() -> element_id
    linker.func_wrap("dom", "getDocumentElement", |caller: Caller<'_, ServerState>| -> i32 {
        caller.data().root_id
    })?;

    // getElementById(id_ptr, id_len) -> element_id (return root as placeholder)
    linker.func_wrap("dom", "getElementById", |caller: Caller<'_, ServerState>, _ptr: i32, _len: i32| -> i32 {
        caller.data().root_id
    })?;

    // querySelector(sel_ptr, sel_len) -> element_id
    linker.func_wrap("dom", "querySelector", |caller: Caller<'_, ServerState>, _ptr: i32, _len: i32| -> i32 {
        caller.data().root_id
    })?;

    // injectStyles(name_ptr, name_len, css_ptr, css_len) -> style_id
    let mem_is = memory;
    linker.func_wrap("dom", "injectStyles", move |mut caller: Caller<'_, ServerState>, _n_ptr: i32, _n_len: i32, c_ptr: i32, c_len: i32| -> i32 {
        let css = read_wasm_str(mem_is, &caller, c_ptr, c_len);
        caller.data_mut().style_blocks.push(css);
        0
    })?;

    // addEventListener — no-op on server
    linker.func_wrap("dom", "addEventListener", |_caller: Caller<'_, ServerState>, _el: i32, _ev_ptr: i32, _ev_len: i32, _cb: i32| {})?;

    // removeEventListener — no-op
    linker.func_wrap("dom", "removeEventListener", |_caller: Caller<'_, ServerState>, _el: i32, _ev_ptr: i32, _ev_len: i32, _cb: i32| {})?;

    // removeChild — no-op (initial render only)
    linker.func_wrap("dom", "removeChild", |_caller: Caller<'_, ServerState>, _parent: i32, _child: i32| {})?;

    // webapi::getLocationPathname — returns server request path
    let mem_lp = memory;
    linker.func_wrap("webapi", "getLocationPathname", move |mut caller: Caller<'_, ServerState>| -> i32 {
        let path = caller.data().request_path.clone();
        let bytes = path.as_bytes();
        let str_offset = 128u32;
        let data = mem_lp.data_mut(&mut caller);
        let end = str_offset as usize + bytes.len();
        if end <= data.len() {
            data[str_offset as usize..end].copy_from_slice(bytes);
        }
        str_offset as i32
    })?;

    // ── Auto-stub all remaining imports ──────────────────────────────
    // Dynamically create no-op stubs for every import not already defined.
    let defined: Vec<(String, String)> = vec![
        ("env".into(), "memory".into()),
        ("dom".into(), "createElement".into()),
        ("dom".into(), "createTextNode".into()),
        ("dom".into(), "setText".into()),
        ("dom".into(), "appendChild".into()),
        ("dom".into(), "setAttribute".into()),
        ("dom".into(), "setStyle".into()),
        ("dom".into(), "setInnerHTML".into()),
        ("dom".into(), "mount".into()),
        ("dom".into(), "getRoot".into()),
        ("dom".into(), "getBody".into()),
        ("dom".into(), "getHead".into()),
        ("dom".into(), "getDocumentElement".into()),
        ("dom".into(), "getElementById".into()),
        ("dom".into(), "querySelector".into()),
        ("dom".into(), "injectStyles".into()),
        ("dom".into(), "addEventListener".into()),
        ("dom".into(), "removeEventListener".into()),
        ("dom".into(), "removeChild".into()),
        ("webapi".into(), "getLocationPathname".into()),
    ];

    for import in module.imports() {
        let module_name = import.module();
        let field_name = import.name();

        if defined.iter().any(|(m, f)| m == module_name && f == field_name) {
            continue;
        }

        if let ExternType::Func(func_type) = import.ty() {
            let results: Vec<ValType> = func_type.results().collect();
            let func = Func::new(&mut store, func_type.clone(), move |_caller, _params, out_results| {
                for (i, ty) in results.iter().enumerate() {
                    out_results[i] = match ty {
                        ValType::I32 => Val::I32(0),
                        ValType::I64 => Val::I64(0),
                        ValType::F32 => Val::F32(0.0_f32.to_bits()),
                        ValType::F64 => Val::F64(0.0_f64.to_bits()),
                        _ => Val::I32(0),
                    };
                }
                Ok(())
            });
            linker.define(&store, module_name, field_name, func)?;
        }
    }

    // ── Instantiate and run ────────────────────────────────────────────
    let instance = linker.instantiate(&mut store, &module)
        .map_err(|e| anyhow::anyhow!("WASM instantiation failed: {}", e))?;

    // ── Entry point resolution ─────────────────────────────────────────
    //
    // For router-based apps, `SiteRouter_init` registers routes but doesn't
    // mount pages (route matching normally happens in the JS runtime). For SSR
    // we need to call the correct *Page_mount directly based on the request path.
    //
    // Strategy:
    //   1. Call SiteRouter_init if present (sets up global state)
    //   2. Map request path to a page mount function:
    //      "/" → HomePage_mount, "/examples" → ExamplesPage_mount, etc.
    //   3. Fall back to App_mount / any *_mount
    let mut called = false;

    // Build the page mount name from the request path.
    // "/examples" → "ExamplesPage_mount", "/" → "HomePage_mount", "/docs" → "DocsPage_mount"
    // NOTE: We skip SiteRouter_init because it would also mount a page,
    // causing duplicate content. We call the page mount function directly.
    let page_name = if request_path == "/" {
        "HomePage".to_string()
    } else {
        let slug = request_path.trim_start_matches('/').split('/').next().unwrap_or("");
        // Convert "examples" → "Examples", "docs" → "Docs"
        let mut chars = slug.chars();
        match chars.next() {
            Some(c) => format!("{}{}Page", c.to_uppercase(), chars.as_str()),
            None => "HomePage".to_string(),
        }
    };

    // Try the path-derived page mount
    let mount_name = format!("{}_mount", page_name);
    if let Ok(func) = instance.get_typed_func::<(i32,), ()>(&mut store, &mount_name) {
        let root_id = store.data().root_id;
        func.call(&mut store, (root_id,))
            .map_err(|e| anyhow::anyhow!("{} failed: {}", mount_name, e))?;
        called = true;
    }

    // Fall back to route mount trampolines (__route_mount_N)
    if !called {
        // Collect route mount exports and try them
        let export_names: Vec<String> = module.exports()
            .filter(|e| e.name().starts_with("__route_mount_"))
            .map(|e| e.name().to_string())
            .collect();
        for name in &export_names {
            if let Ok(func) = instance.get_typed_func::<(i32,), ()>(&mut store, name) {
                let root_id = store.data().root_id;
                func.call(&mut store, (root_id,)).ok();
                called = true;
                break;
            }
        }
    }

    if !called {
        // Try common mount functions with (root: i32)
        for name in &["App_mount", "HomePage_mount"] {
            if let Ok(func) = instance.get_typed_func::<(i32,), ()>(&mut store, name) {
                let root_id = store.data().root_id;
                func.call(&mut store, (root_id,))
                    .map_err(|e| anyhow::anyhow!("{} failed: {}", name, e))?;
                called = true;
                break;
            }
        }
    }

    if !called {
        // Scan all exports for any *_init (0 params) or *_mount (1 param)
        let export_names: Vec<String> = module.exports()
            .map(|e| e.name().to_string())
            .collect();
        for name in &export_names {
            if name.ends_with("_init") {
                if let Ok(func) = instance.get_typed_func::<(), ()>(&mut store, name) {
                    func.call(&mut store, ()).ok();
                    called = true;
                    break;
                }
            } else if name.ends_with("_mount") {
                if let Ok(func) = instance.get_typed_func::<(i32,), ()>(&mut store, name) {
                    func.call(&mut store, (1,)).ok();
                    called = true;
                    break;
                }
            }
        }
    }

    if !called {
        return Err(anyhow::anyhow!("no mount/init function found in WASM exports"));
    }

    Ok(store.into_data())
}

/// SEO metadata for HTML injection (mirrors main.rs PageMeta).
pub struct SsrPageMeta {
    pub title: Option<String>,
    pub description: Option<String>,
    pub canonical: Option<String>,
    pub og_image: Option<String>,
    pub structured_data_json: Vec<String>,
}

/// Generate HTML meta tags for SSR <head> injection.
fn generate_ssr_meta_html(meta: &SsrPageMeta) -> String {
    let mut out = String::new();

    if let Some(ref title) = meta.title {
        out.push_str(&format!("    <title>{}</title>\n", ssr_html_escape(title)));
        out.push_str(&format!("    <meta property=\"og:title\" content=\"{}\">\n", ssr_html_escape(title)));
    }
    if let Some(ref desc) = meta.description {
        out.push_str(&format!("    <meta name=\"description\" content=\"{}\">\n", ssr_html_escape(desc)));
        out.push_str(&format!("    <meta property=\"og:description\" content=\"{}\">\n", ssr_html_escape(desc)));
    }
    if let Some(ref canonical) = meta.canonical {
        out.push_str(&format!("    <link rel=\"canonical\" href=\"{}\">\n", ssr_html_escape(canonical)));
        out.push_str(&format!("    <meta property=\"og:url\" content=\"{}\">\n", ssr_html_escape(canonical)));
    }
    if let Some(ref og_img) = meta.og_image {
        out.push_str(&format!("    <meta property=\"og:image\" content=\"{}\">\n", ssr_html_escape(og_img)));
    }
    out.push_str("    <meta property=\"og:type\" content=\"website\">\n");

    for json in &meta.structured_data_json {
        out.push_str(&format!("    <script type=\"application/ld+json\">{}</script>\n", json));
    }

    out
}

fn ssr_html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('"', "&quot;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
}

/// Build the complete HTML document wrapping SSR-rendered content.
fn build_html_shell(ssr_html: &str, path: &str) -> String {
    build_html_shell_with_meta(ssr_html, path, None)
}

/// Build HTML shell with optional SEO meta injection.
fn build_html_shell_with_meta(ssr_html: &str, path: &str, meta: Option<&SsrPageMeta>) -> String {
    // Derive the page mount function name from the path (same logic as SSR routing)
    let page_name = if path == "/" {
        "HomePage".to_string()
    } else {
        let slug = path.trim_start_matches('/').split('/').next().unwrap_or("");
        let mut chars = slug.chars();
        match chars.next() {
            Some(c) => format!("{}{}Page", c.to_uppercase(), chars.as_str()),
            None => "HomePage".to_string(),
        }
    };
    let mount_fn = format!("{}_mount", page_name);

    let meta_html = meta.map(|m| generate_ssr_meta_html(m)).unwrap_or_else(|| {
        "    <title>Nectar App</title>\n".to_string()
    });

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
{meta_html}    <link rel="icon" href="data:,">
</head>
<body>
    <div id="app">{ssr}</div>
    <script type="module">
        import {{ instantiate }} from './core.js';
        const instance = await instantiate('./app.wasm');
        // Client-side boot: clear SSR content and mount the WASM app
        const app = document.getElementById('app');
        app.innerHTML = '';
        const mount = instance.exports['{mount_fn}'] || instance.exports.SiteRouter_init;
        if (mount) {{
            // Page mounts take a root element ID (1 = #app in the runtime)
            try {{ mount(1); }} catch(e) {{ try {{ mount(); }} catch(e2) {{}} }}
        }}
    </script>
</body>
</html>"#, meta_html = meta_html, ssr = ssr_html, mount_fn = mount_fn)
}

/// Build an error page for SSR failures.
fn build_error_page(path: &str, error: &str) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Server Error</title>
    <style>body {{ font-family: system-ui; padding: 2rem; }} pre {{ background: #f5f5f5; padding: 1rem; overflow-x: auto; }}</style>
</head>
<body>
    <h1>SSR Error</h1>
    <p>Failed to render <code>{}</code></p>
    <pre>{}</pre>
    <script type="module">
        import {{ instantiate }} from './core.js';
        const instance = await instantiate('./app.wasm');
        if (instance.exports.SiteRouter_init) {{
            instance.exports.SiteRouter_init(0, 0);
        }}
    </script>
</body>
</html>"#, path, error)
}

// ==========================================================================
// Tests
// ==========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_state_default() {
        let state = ServerState::default();
        assert!(state.request_path.is_empty());
        assert!(state.elements.contains_key(&1)); // root element exists
        assert_eq!(state.next_id, 2);
    }

    #[test]
    fn test_server_state_new() {
        let state = ServerState::new("/products".to_string());
        assert_eq!(state.request_path, "/products");
        assert_eq!(state.root_id, 1);
    }

    #[test]
    fn test_serialize_empty() {
        let state = ServerState::default();
        let html = state.serialize_html();
        assert_eq!(html, ""); // root has no children
    }

    #[test]
    fn test_serialize_elements() {
        let mut state = ServerState::default();
        let div_id = state.alloc_id();
        state.elements.insert(div_id, SsrElement::new("div"));
        state.elements.get_mut(&div_id).unwrap().attrs.push(("class".into(), "hero".into()));

        let text_id = state.alloc_id();
        state.elements.insert(text_id, SsrElement::text_node("Hello Nectar"));
        state.elements.get_mut(&div_id).unwrap().children.push(text_id);

        state.elements.get_mut(&state.root_id).unwrap().children.push(div_id);

        let html = state.serialize_html();
        assert!(html.contains("<div class=\"hero\">"));
        assert!(html.contains("Hello Nectar"));
        assert!(html.contains("</div>"));
    }

    #[test]
    fn test_serialize_styles() {
        let mut state = ServerState::default();
        state.style_blocks.push("body { color: red; }".into());
        let html = state.serialize_html();
        assert!(html.contains("<style>body { color: red; }</style>"));
    }

    #[test]
    fn test_serialize_void_elements() {
        let mut state = ServerState::default();
        let img_id = state.alloc_id();
        state.elements.insert(img_id, SsrElement::new("img"));
        state.elements.get_mut(&img_id).unwrap().attrs.push(("src".into(), "logo.png".into()));
        state.elements.get_mut(&state.root_id).unwrap().children.push(img_id);
        let html = state.serialize_html();
        assert!(html.contains("<img src=\"logo.png\" />"));
        assert!(!html.contains("</img>"));
    }

    #[test]
    fn test_serialize_inline_styles() {
        let mut state = ServerState::default();
        let div_id = state.alloc_id();
        state.elements.insert(div_id, SsrElement::new("div"));
        state.elements.get_mut(&div_id).unwrap().styles.push(("color".into(), "red".into()));
        state.elements.get_mut(&div_id).unwrap().styles.push(("font-size".into(), "16px".into()));
        state.elements.get_mut(&state.root_id).unwrap().children.push(div_id);
        let html = state.serialize_html();
        assert!(html.contains("style=\"color: red; font-size: 16px\""));
    }

    #[test]
    fn test_serialize_inner_html() {
        let mut state = ServerState::default();
        let div_id = state.alloc_id();
        let mut el = SsrElement::new("div");
        el.inner_html = Some("<b>bold</b>".into());
        state.elements.insert(div_id, el);
        state.elements.get_mut(&state.root_id).unwrap().children.push(div_id);
        let html = state.serialize_html();
        assert!(html.contains("<div><b>bold</b></div>"));
    }

    #[test]
    fn test_html_shell_contains_ssr_content() {
        let html = build_html_shell("<h1>Hello</h1>", "/");
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("id=\"app\""));
        assert!(html.contains("core.js"));
        assert!(html.contains("app.wasm"));
    }

    #[test]
    fn test_html_shell_has_hydration_script() {
        let html = build_html_shell("", "/");
        assert!(html.contains("instantiate"));
        assert!(html.contains("HomePage_mount"));
    }

    #[test]
    fn test_html_shell_has_doctype() {
        let html = build_html_shell("", "/");
        assert!(html.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn test_error_page_contains_path() {
        let html = build_error_page("/products/123", "module not found");
        assert!(html.contains("/products/123"));
        assert!(html.contains("module not found"));
    }

    #[test]
    fn test_error_page_has_fallback_script() {
        let html = build_error_page("/", "error");
        assert!(html.contains("core.js"));
        assert!(html.contains("SiteRouter_init"));
    }

    #[test]
    fn test_ssr_config_fields() {
        let config = SsrServerConfig {
            wasm_path: PathBuf::from("app.wasm"),
            port: 3000,
            static_dir: Some(PathBuf::from("./static")),
            api_base_url: Some("https://staging.example.com".to_string()),
            api_token: Some("token123".to_string()),
        };
        assert_eq!(config.port, 3000);
        assert_eq!(config.wasm_path, PathBuf::from("app.wasm"));
        assert!(config.static_dir.is_some());
    }

    #[test]
    fn test_html_shell_escaping() {
        let html = build_html_shell("<div class=\"test\">Content &amp; more</div>", "/test");
        assert!(html.contains("<div class=\"test\">Content &amp; more</div>"));
    }

    #[test]
    fn test_error_page_structure() {
        let html = build_error_page("/broken", "timeout");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("SSR Error"));
        assert!(html.contains("<pre>timeout</pre>"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>alert('xss')</script>"), "&lt;script&gt;alert('xss')&lt;/script&gt;");
        assert_eq!(html_escape_attr("a\"b"), "a&quot;b");
    }

    // ── API proxy tests ──────────────────────────────────────────────

    #[test]
    fn test_resolve_provider_env_vars() {
        // With no env vars set, both should be empty
        let (url, key) = resolve_provider("nonexistent_test_provider_xyz");
        assert!(url.is_empty());
        assert!(key.is_empty());
    }

    #[test]
    fn test_resolve_provider_uppercases_name() {
        unsafe {
            std::env::set_var("NECTAR_TESTPROV_URL", "https://test.example.com");
            std::env::set_var("NECTAR_TESTPROV_KEY", "sk_test_123");
        }
        let (url, key) = resolve_provider("testprov");
        assert_eq!(url, "https://test.example.com");
        assert_eq!(key, "sk_test_123");
        unsafe {
            std::env::remove_var("NECTAR_TESTPROV_URL");
            std::env::remove_var("NECTAR_TESTPROV_KEY");
        }
    }

    #[test]
    fn test_resolve_provider_mixed_case() {
        unsafe {
            std::env::set_var("NECTAR_PAYMENT_URL", "https://api.moov.io");
            std::env::set_var("NECTAR_PAYMENT_KEY", "moov_key");
        }
        let (url, key) = resolve_provider("payment");
        assert_eq!(url, "https://api.moov.io");
        assert_eq!(key, "moov_key");
        // Also works with Payment (uppercased to PAYMENT)
        let (url2, key2) = resolve_provider("Payment");
        // "Payment".to_uppercase() = "PAYMENT" so it should match
        assert_eq!(url2, "https://api.moov.io");
        assert_eq!(key2, "moov_key");
        unsafe {
            std::env::remove_var("NECTAR_PAYMENT_URL");
            std::env::remove_var("NECTAR_PAYMENT_KEY");
        }
    }

    #[test]
    fn test_is_valid_json() {
        assert!(is_valid_json(b"{}"));
        assert!(is_valid_json(b"{\"amount\": 100}"));
        assert!(is_valid_json(b"[1, 2, 3]"));
        assert!(is_valid_json(b"\"hello\""));
        assert!(is_valid_json(b"42"));
        assert!(is_valid_json(b"null"));
        assert!(!is_valid_json(b"{invalid}"));
        assert!(!is_valid_json(b""));
        assert!(!is_valid_json(b"{\"key\": }"));
    }

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.allow());
        }
        // 6th request should be denied
        assert!(!limiter.allow());
    }

    #[test]
    fn test_rate_limiter_reset() {
        let limiter = RateLimiter::new(2);
        assert!(limiter.allow());
        assert!(limiter.allow());
        assert!(!limiter.allow());
        limiter.reset();
        assert!(limiter.allow());
        assert!(limiter.allow());
        assert!(!limiter.allow());
    }

    #[test]
    fn test_api_proxy_returns_404_no_env_vars() {
        // Integration-style test: verify that the handler returns 404
        // when the provider is not configured (no env vars set).
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use axum::http::{Request, Method};
            use axum::body::Body;
            use axum::response::IntoResponse;

            let state = Arc::new(ApiProxyState {
                http_client: reqwest::Client::new(),
                rate_limiter: Arc::new(RateLimiter::new(100)),
            });

            // Ensure no env vars for this provider
            unsafe {
                std::env::remove_var("NECTAR_XYZNOTREAL_URL");
                std::env::remove_var("NECTAR_XYZNOTREAL_KEY");
            }

            let request = Request::builder()
                .method(Method::POST)
                .uri("/api/xyznotreal")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap();

            let response = api_proxy_handler(
                axum::extract::State(state),
                axum::extract::Path("xyznotreal".to_string()),
                request,
            ).await;

            assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
        });
    }

    #[test]
    fn test_api_proxy_rejects_invalid_json() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use axum::http::{Request, Method};
            use axum::body::Body;

            let state = Arc::new(ApiProxyState {
                http_client: reqwest::Client::new(),
                rate_limiter: Arc::new(RateLimiter::new(100)),
            });

            // Set a provider URL so we get past the 404 check
            unsafe {
                std::env::set_var("NECTAR_JSONTEST_URL", "https://httpbin.org/post");
                std::env::set_var("NECTAR_JSONTEST_KEY", "testkey");
            }

            let request = Request::builder()
                .method(Method::POST)
                .uri("/api/jsontest")
                .header("content-type", "application/json")
                .body(Body::from("{this is not valid json}"))
                .unwrap();

            let response = api_proxy_handler(
                axum::extract::State(state),
                axum::extract::Path("jsontest".to_string()),
                request,
            ).await;

            assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

            unsafe {
                std::env::remove_var("NECTAR_JSONTEST_URL");
                std::env::remove_var("NECTAR_JSONTEST_KEY");
            }
        });
    }

    #[test]
    fn test_api_proxy_rate_limit() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use axum::http::{Request, Method};
            use axum::body::Body;

            let state = Arc::new(ApiProxyState {
                http_client: reqwest::Client::new(),
                rate_limiter: Arc::new(RateLimiter::new(1)), // Only 1 request allowed
            });

            unsafe { std::env::remove_var("NECTAR_RATELIMITPROV_URL"); }

            // First request uses the one allowed slot (will get 404 since no URL)
            let req1 = Request::builder()
                .method(Method::GET)
                .uri("/api/ratelimitprov")
                .body(Body::empty())
                .unwrap();
            let resp1 = api_proxy_handler(
                axum::extract::State(state.clone()),
                axum::extract::Path("ratelimitprov".to_string()),
                req1,
            ).await;
            // First is allowed (gets 404 because no env var, but not 429)
            assert_ne!(resp1.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);

            // Second request should be rate-limited
            let req2 = Request::builder()
                .method(Method::GET)
                .uri("/api/ratelimitprov")
                .body(Body::empty())
                .unwrap();
            let resp2 = api_proxy_handler(
                axum::extract::State(state.clone()),
                axum::extract::Path("ratelimitprov".to_string()),
                req2,
            ).await;
            assert_eq!(resp2.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
        });
    }

    #[test]
    fn test_api_proxy_empty_body_allowed() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use axum::http::{Request, Method};
            use axum::body::Body;

            let state = Arc::new(ApiProxyState {
                http_client: reqwest::Client::new(),
                rate_limiter: Arc::new(RateLimiter::new(100)),
            });

            // Provider not configured — we just verify empty body doesn't
            // trigger the "invalid JSON" rejection
            unsafe { std::env::remove_var("NECTAR_EMPTYTEST_URL"); }

            let request = Request::builder()
                .method(Method::GET)
                .uri("/api/emptytest")
                .body(Body::empty())
                .unwrap();

            let response = api_proxy_handler(
                axum::extract::State(state),
                axum::extract::Path("emptytest".to_string()),
                request,
            ).await;

            // Should be 404 (not configured), not 400 (bad JSON)
            assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
        });
    }

    // --- SSR meta injection tests ---

    #[test]
    fn test_build_html_shell_with_meta_injects_title() {
        let meta = SsrPageMeta {
            title: Some("My Store".to_string()),
            description: Some("Best products".to_string()),
            canonical: Some("https://example.com".to_string()),
            og_image: Some("https://example.com/og.png".to_string()),
            structured_data_json: vec![
                r#"{"@context":"https://schema.org","@type":"Product","name":"Widget"}"#.to_string(),
            ],
        };
        let html = build_html_shell_with_meta("<p>content</p>", "/", Some(&meta));
        assert!(html.contains("<title>My Store</title>"));
        assert!(html.contains("<meta name=\"description\" content=\"Best products\">"));
        assert!(html.contains("<meta property=\"og:title\" content=\"My Store\">"));
        assert!(html.contains("<meta property=\"og:image\" content=\"https://example.com/og.png\">"));
        assert!(html.contains("<link rel=\"canonical\" href=\"https://example.com\">"));
        assert!(html.contains("application/ld+json"));
        assert!(html.contains("\"@type\":\"Product\""));
        assert!(html.contains("core.js"));
        assert!(html.contains("<p>content</p>"));
    }

    #[test]
    fn test_build_html_shell_without_meta_uses_default_title() {
        let html = build_html_shell("<p>hello</p>", "/about");
        assert!(html.contains("<title>Nectar App</title>"));
        assert!(html.contains("<p>hello</p>"));
        assert!(html.contains("core.js"));
    }

    #[test]
    fn test_ssr_html_escape() {
        assert_eq!(ssr_html_escape("a&b"), "a&amp;b");
        assert_eq!(ssr_html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(ssr_html_escape("say \"hi\""), "say &quot;hi&quot;");
    }

    #[test]
    fn test_generate_ssr_meta_html_og_type() {
        let meta = SsrPageMeta {
            title: None,
            description: None,
            canonical: None,
            og_image: None,
            structured_data_json: vec![],
        };
        let html = generate_ssr_meta_html(&meta);
        assert!(html.contains("og:type"));
        assert!(html.contains("website"));
    }
}
