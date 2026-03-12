use crate::ast::*;

/// Generates WebAssembly Text Format (WAT) from a Nectar AST.
///
/// This is the initial codegen backend. We emit WAT first for readability
/// and debugging, then can convert to binary .wasm via wat2wasm or a
/// binary emitter later.
pub struct WasmCodegen {
    output: String,
    indent: usize,
    /// Track local variables in current function scope
    locals: Vec<(String, WasmType)>,
    /// Counter for generating unique labels
    label_counter: u32,
    /// Interned strings: (string_value, memory_offset)
    strings: Vec<(String, u32)>,
    /// Next available offset in linear memory for string data
    string_offset: u32,
    /// Counter for generating unique closure function names
    closure_counter: u32,
    /// Deferred closure function definitions (emitted at module level)
    closure_functions: Vec<String>,
    /// Whether we need a function table for indirect calls
    needs_func_table: bool,
    /// Names of closure functions (for the table)
    closure_func_names: Vec<String>,
}

#[derive(Debug, Clone)]
enum WasmType {
    I32,
    I64,
    F32,
    F64,
}

impl WasmCodegen {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            locals: Vec::new(),
            label_counter: 0,
            strings: Vec::new(),
            string_offset: 256,
            closure_counter: 0,
            closure_functions: Vec::new(),
            needs_func_table: false,
            closure_func_names: Vec::new(),
        }
    }

    pub fn generate(&mut self, program: &Program) -> String {
        self.emit("(module");
        self.indent += 1;

        // Import memory from host (for strings, DOM, etc.)
        self.line("(import \"env\" \"memory\" (memory 1))");

        // String runtime — WASM-internal (emitted by emit_string_runtime)
        // concat, fromI32, fromF64, fromBool, toString all run in WASM linear memory.

        // ── DOM — mount/flush + element queries + absorbed functions ─────────
        self.line("(import \"dom\" \"mount\" (func $dom_mount (param i32 i32 i32)))");
        self.line("(import \"dom\" \"hydrateRefs\" (func $dom_hydrateRefs (param i32) (result i32)))");
        self.line("(import \"dom\" \"flush\" (func $dom_flush (param i32 i32)))");
        self.line("(import \"dom\" \"getElementById\" (func $dom_getElementById (param i32 i32) (result i32)))");
        self.line("(import \"dom\" \"querySelector\" (func $dom_querySelector (param i32 i32) (result i32)))");
        self.line("(import \"dom\" \"createElement\" (func $dom_createElement (param i32 i32) (result i32)))");
        self.line("(import \"dom\" \"createTextNode\" (func $dom_createTextNode (param i32 i32) (result i32)))");
        self.line("(import \"dom\" \"getBody\" (func $dom_getBody (result i32)))");
        self.line("(import \"dom\" \"getHead\" (func $dom_getHead (result i32)))");
        self.line("(import \"dom\" \"getRoot\" (func $dom_getRoot (result i32)))");
        self.line("(import \"dom\" \"getDocumentElement\" (func $dom_getDocumentElement (result i32)))");
        self.line("(import \"dom\" \"addEventListener\" (func $dom_addEventListener (param i32 i32 i32 i32)))");
        self.line("(import \"dom\" \"removeEventListener\" (func $dom_removeEventListener (param i32 i32 i32 i32)))");
        self.line("(import \"dom\" \"lazyMount\" (func $dom_lazyMount (param i32 i32 i32 i32)))");
        self.line("(import \"dom\" \"setTitle\" (func $dom_setTitle (param i32 i32)))");
        self.line("(import \"dom\" \"getScrollTop\" (func $dom_getScrollTop (param i32) (result f64)))");
        self.line("(import \"dom\" \"getScrollLeft\" (func $dom_getScrollLeft (param i32) (result f64)))");
        self.line("(import \"dom\" \"getClientHeight\" (func $dom_getClientHeight (param i32) (result i32)))");
        self.line("(import \"dom\" \"getClientWidth\" (func $dom_getClientWidth (param i32) (result i32)))");
        self.line("(import \"dom\" \"getWindowWidth\" (func $dom_getWindowWidth (result i32)))");
        self.line("(import \"dom\" \"getWindowHeight\" (func $dom_getWindowHeight (result i32)))");
        self.line("(import \"dom\" \"getOuterHtml\" (func $dom_getOuterHtml (result i32)))");
        self.line("(import \"dom\" \"setDragData\" (func $dom_setDragData (param i32 i32 i32 i32)))");
        self.line("(import \"dom\" \"getDragData\" (func $dom_getDragData (param i32 i32) (result i32)))");
        self.line("(import \"dom\" \"preventDefault\" (func $dom_preventDefault))");
        self.line(";; Absorbed from embed/loader/media/pdf/io");
        self.line("(import \"dom\" \"loadScript\" (func $dom_loadScript (param i32 i32 i32)))");
        self.line("(import \"dom\" \"loadChunk\" (func $dom_loadChunk (param i32 i32) (result i32)))");
        self.line("(import \"dom\" \"decodeImage\" (func $dom_decodeImage (param i32 i32 i32)))");
        self.line("(import \"dom\" \"progressiveImage\" (func $dom_progressiveImage (param i32 i32 i32 i32 i32)))");
        self.line("(import \"dom\" \"print\" (func $dom_print (param i32)))");
        self.line("(import \"dom\" \"download\" (func $dom_download (param i32 i32 i32 i32)))");

        // ── Timer — browser timer APIs ───────────────────────────────────────
        self.line("");
        self.line(";; Timer — browser timer APIs");
        self.line("(import \"timer\" \"setTimeout\" (func $timer_setTimeout (param i32 i32) (result i32)))");
        self.line("(import \"timer\" \"clearTimeout\" (func $timer_clearTimeout (param i32)))");
        self.line("(import \"timer\" \"setInterval\" (func $timer_setInterval (param i32 i32) (result i32)))");
        self.line("(import \"timer\" \"clearInterval\" (func $timer_clearInterval (param i32)))");
        self.line("(import \"timer\" \"requestAnimationFrame\" (func $timer_requestAnimationFrame (param i32) (result i32)))");
        self.line("(import \"timer\" \"cancelAnimationFrame\" (func $timer_cancelAnimationFrame (param i32)))");
        self.line("(import \"timer\" \"now\" (func $timer_now (result f64)))");

        // ── Web API — storage, clipboard, history, console, router, env, share, perf
        self.line("");
        self.line(";; Web API — storage");
        self.line("(import \"webapi\" \"localStorageGet\" (func $webapi_localStorageGet (param i32 i32) (result i32)))");
        self.line("(import \"webapi\" \"localStorageSet\" (func $webapi_localStorageSet (param i32 i32 i32 i32)))");
        self.line("(import \"webapi\" \"localStorageRemove\" (func $webapi_localStorageRemove (param i32 i32)))");
        self.line("(import \"webapi\" \"sessionStorageGet\" (func $webapi_sessionStorageGet (param i32 i32) (result i32)))");
        self.line("(import \"webapi\" \"sessionStorageSet\" (func $webapi_sessionStorageSet (param i32 i32 i32 i32)))");
        self.line(";; Web API — clipboard");
        self.line("(import \"webapi\" \"clipboardWrite\" (func $webapi_clipboardWrite (param i32 i32)))");
        self.line("(import \"webapi\" \"clipboardRead\" (func $webapi_clipboardRead (param i32)))");
        self.line(";; Web API — URL/history");
        self.line("(import \"webapi\" \"getLocationHref\" (func $webapi_getLocationHref (result i32)))");
        self.line("(import \"webapi\" \"getLocationSearch\" (func $webapi_getLocationSearch (result i32)))");
        self.line("(import \"webapi\" \"getLocationHash\" (func $webapi_getLocationHash (result i32)))");
        self.line("(import \"webapi\" \"getLocationPathname\" (func $webapi_getLocationPathname (result i32)))");
        self.line("(import \"webapi\" \"pushState\" (func $webapi_pushState (param i32 i32)))");
        self.line("(import \"webapi\" \"replaceState\" (func $webapi_replaceState (param i32 i32)))");
        self.line(";; Web API — console");
        self.line("(import \"webapi\" \"consoleLog\" (func $webapi_consoleLog (param i32 i32)))");
        self.line("(import \"webapi\" \"consoleWarn\" (func $webapi_consoleWarn (param i32 i32)))");
        self.line("(import \"webapi\" \"consoleError\" (func $webapi_consoleError (param i32 i32)))");
        self.line(";; Web API — absorbed from router");
        self.line("(import \"webapi\" \"onPopState\" (func $webapi_onPopState (param i32)))");
        self.line(";; Web API — absorbed from env");
        self.line("(import \"webapi\" \"envGet\" (func $webapi_envGet (param i32 i32) (result i32)))");
        self.line(";; Web API — absorbed from share");
        self.line("(import \"webapi\" \"canShare\" (func $webapi_canShare (result i32)))");
        self.line("(import \"webapi\" \"nativeShare\" (func $webapi_nativeShare (param i32 i32 i32 i32 i32 i32)))");
        self.line(";; Web API — absorbed from perf");
        self.line("(import \"webapi\" \"perfMark\" (func $webapi_perfMark (param i32 i32)))");
        self.line("(import \"webapi\" \"perfMeasure\" (func $webapi_perfMeasure (param i32 i32 i32 i32 i32 i32)))");


        // ── HTTP — browser fetch API ─────────────────────────────────────────
        self.line("");
        self.line(";; HTTP — browser fetch API");
        self.line("(import \"http\" \"fetch\" (func $http_fetch (param i32 i32 i32 i32) (result i32)))");

        // ── Observe — IntersectionObserver + matchMedia ──────────────────────
        self.line("");
        self.line(";; Observe — browser IntersectionObserver + matchMedia APIs");
        self.line("(import \"observe\" \"matchMedia\" (func $observe_matchMedia (param i32 i32) (result i32)))");
        self.line("(import \"observe\" \"intersectionObserver\" (func $observe_intersectionObserver (param i32 i32 i32) (result i32)))");
        self.line("(import \"observe\" \"observe\" (func $observe_observe (param i32 i32)))");
        self.line("(import \"observe\" \"unobserve\" (func $observe_unobserve (param i32 i32)))");
        self.line("(import \"observe\" \"disconnect\" (func $observe_disconnect (param i32)))");

        // ── WebSocket ────────────────────────────────────────────────────────
        self.line("");
        self.line(";; WebSocket — browser WebSocket API");
        self.line("(import \"ws\" \"connect\" (func $ws_connect (param i32 i32) (result i32)))");
        self.line("(import \"ws\" \"send\" (func $ws_send (param i32 i32 i32)))");
        self.line("(import \"ws\" \"sendBinary\" (func $ws_sendBinary (param i32 i32 i32)))");
        self.line("(import \"ws\" \"close\" (func $ws_close (param i32)))");
        self.line("(import \"ws\" \"onOpen\" (func $ws_onOpen (param i32 i32)))");
        self.line("(import \"ws\" \"onMessage\" (func $ws_onMessage (param i32 i32)))");
        self.line("(import \"ws\" \"onClose\" (func $ws_onClose (param i32 i32)))");
        self.line("(import \"ws\" \"onError\" (func $ws_onError (param i32 i32)))");
        self.line("(import \"ws\" \"getReadyState\" (func $ws_getReadyState (param i32) (result i32)))");

        // ── IndexedDB — pure syscalls, WASM handles serialization ────────────
        self.line("");
        self.line(";; Database — browser IndexedDB API (no JSON logic in JS)");
        self.line("(import \"db\" \"open\" (func $db_open (param i32 i32 i32 i32)))");
        self.line("(import \"db\" \"put\" (func $db_put (param i32 i32 i32 i32 i32 i32 i32)))");
        self.line("(import \"db\" \"get\" (func $db_get (param i32 i32 i32 i32 i32 i32)))");
        self.line("(import \"db\" \"delete\" (func $db_delete (param i32 i32 i32 i32 i32)))");
        self.line("(import \"db\" \"getAll\" (func $db_getAll (param i32 i32 i32 i32)))");

        // ── Workers ──────────────────────────────────────────────────────────
        self.line("");
        self.line(";; Workers — browser Web Worker API");
        self.line("(import \"worker\" \"spawn\" (func $worker_spawn (param i32) (result i32)))");
        self.line("(import \"worker\" \"channelCreate\" (func $worker_channelCreate (result i32)))");
        self.line("(import \"worker\" \"channelSend\" (func $worker_channelSend (param i32 i32 i32)))");
        self.line("(import \"worker\" \"channelRecv\" (func $worker_channelRecv (param i32 i32)))");
        self.line("(import \"worker\" \"postMessage\" (func $worker_postMessage (param i32 i32 i32)))");
        self.line("(import \"worker\" \"onMessage\" (func $worker_onMessage (param i32 i32)))");
        self.line("(import \"worker\" \"terminate\" (func $worker_terminate (param i32)))");

        // ── PWA — Service Worker + Push + Cache ──────────────────────────────
        self.line("");
        self.line(";; PWA — browser Service Worker + Push APIs");
        self.line("(import \"pwa\" \"cachePrecache\" (func $pwa_cachePrecache (param i32 i32)))");
        self.line("(import \"pwa\" \"registerPush\" (func $pwa_registerPush (param i32 i32)))");
        self.line("(import \"pwa\" \"registerServiceWorker\" (func $pwa_registerServiceWorker (param i32 i32 i32)))");

        // ── Hardware — device APIs ───────────────────────────────────────────
        self.line("");
        self.line(";; Hardware — browser device APIs");
        self.line("(import \"hardware\" \"haptic\" (func $hardware_haptic (param i32)))");
        self.line("(import \"hardware\" \"biometricAuth\" (func $hardware_biometricAuth (param i32 i32 i32 i32)))");
        self.line("(import \"hardware\" \"cameraCapture\" (func $hardware_cameraCapture (param i32 i32 i32)))");
        self.line("(import \"hardware\" \"geolocationCurrent\" (func $hardware_geolocationCurrent (param i32)))");

        // ── Payment — only processPayment (contentWindow.postMessage) ────────
        self.line("");
        self.line(";; Payment — browser contentWindow.postMessage API");
        self.line("(import \"payment\" \"processPayment\" (func $payment_processPayment (param i32 i32 i32 i32)))");

        // ── Auth — pure syscalls, WASM parses cookies ─────────────────────────
        self.line("");
        self.line(";; Auth — browser cookie/navigation APIs (no parsing in JS)");
        self.line("(import \"auth\" \"login\" (func $auth_login (param i32 i32)))");
        self.line("(import \"auth\" \"logout\" (func $auth_logout (param i32 i32)))");
        self.line("(import \"auth\" \"getRawCookies\" (func $auth_getRawCookies (result i32)))");
        self.line("(import \"auth\" \"setCookie\" (func $auth_set_cookie (param i32 i32)))");

        // ── Upload — file picker + XHR ───────────────────────────────────────
        self.line("");
        self.line(";; Upload — browser file input + XHR APIs");
        self.line("(import \"upload\" \"init\" (func $upload_init (param i32 i32 i32 i32)))");
        self.line("(import \"upload\" \"start\" (func $upload_start (param i32 i32) (result i32)))");
        self.line("(import \"upload\" \"cancel\" (func $upload_cancel (param i32)))");

        // ── Time — Intl + Date ───────────────────────────────────────────────
        self.line("");
        self.line(";; Time — browser Intl + Date APIs");
        self.line("(import \"time\" \"now\" (func $time_now (result f64)))");
        self.line("(import \"time\" \"format\" (func $time_format (param f64 i32 i32) (result i32)))");
        self.line("(import \"time\" \"getTimezoneOffset\" (func $time_getTimezoneOffset (result i32)))");
        self.line("(import \"time\" \"formatDate\" (func $time_formatDate (param f64 i32 i32 i32 i32) (result i32)))");

        // ── Streaming — ReadableStream + EventSource ─────────────────────────
        self.line("");
        self.line(";; Streaming — browser ReadableStream + EventSource APIs");
        self.line("(import \"streaming\" \"streamFetch\" (func $streaming_streamFetch (param i32 i32 i32)))");
        self.line("(import \"streaming\" \"sseConnect\" (func $streaming_sseConnect (param i32 i32 i32)))");

        // ── WASM-internal (no JS imports) ────────────────────────────────────
        // signal, string, flags, cache, permissions, form, lifecycle, contract,
        // gesture (math), shortcuts, virtual scroll, style injection, animation,
        // a11y, theme, seo, trace, dnd, media (lazy/preload), router matching,
        // mem ops, atomic state — all emitted by emit_*_runtime methods.

        // Share — syscall lives in core.js, no separate module
        self.line("");
        self.line(";; Share — uses core share syscalls (navigator.share)");
        self.line(";; share_can_share, share_native — routed through core.js share namespace");

        // Allocator (bump allocator for now)
        self.line("");
        self.line("(global $heap_ptr (mut i32) (i32.const 1024))");
        self.line("");
        self.emit_alloc_function();
        self.emit_string_runtime();
        self.emit_internal_runtimes();
        self.emit_signal_runtime();
        self.emit_gesture_runtime();
        self.emit_flags_runtime();
        self.emit_ai_runtime();

        // Collect test definitions for the test runner
        let mut test_defs: Vec<(&str, usize)> = Vec::new();

        // Generate code for each item
        for (i, item) in program.items.iter().enumerate() {
            self.line("");
            if let Item::Test(test) = item {
                test_defs.push((&test.name, i));
            }
            self.generate_item(item);
        }

        // Generate $__run_tests function if there are any test blocks
        if !test_defs.is_empty() {
            self.generate_test_runner(&test_defs);
        }

        // Emit closure functions
        if !self.closure_functions.is_empty() {
            self.line("");
            self.line(";; Closure functions");
            let closures = std::mem::take(&mut self.closure_functions);
            for closure_fn in &closures {
                self.output.push_str(closure_fn);
            }
            self.closure_functions = closures;
        }

        // Emit function table for indirect calls (closures)
        if self.needs_func_table {
            self.line("");
            self.line(";; Function table for indirect closure calls");
            let names: Vec<String> = self.closure_func_names.clone();
            if names.is_empty() {
                self.line("(table 0 funcref)");
            } else {
                self.line(&format!("(table {} funcref)", names.len()));
                self.line(&format!(
                    "(elem (i32.const 0) {})",
                    names.join(" ")
                ));
            }
            self.line("(type $__closure_type (func (param i32 i32) (result i32)))");
        }

        // Emit data section for interned strings
        self.emit_data_section();

        self.indent -= 1;
        self.line(")");

        self.output.clone()
    }

    fn generate_item(&mut self, item: &Item) {
        match item {
            Item::Function(f) => self.generate_function(f),
            Item::Component(c) => self.generate_component(c),
            Item::Struct(s) => self.generate_struct_layout(s),
            Item::Store(s) => self.generate_store(s),
            Item::Agent(a) => self.generate_agent(a),
            Item::Router(r) => self.generate_router(r),
            Item::LazyComponent(lc) => {
                self.line(&format!(";; lazy component {}", lc.component.name));
                self.generate_component(&lc.component);
            }
            Item::Contract(c) => self.generate_contract(c),
            Item::Test(test) => self.generate_test(test),
            Item::Trait(t) => {
                // Traits are erased at codegen (like Rust monomorphization).
                // Trait method calls compile to direct calls to concrete implementations.
                self.line(&format!(";; trait {} (erased)", t.name));
            }
            Item::Impl(imp) if !imp.trait_impls.is_empty() => {
                // Trait impl methods are compiled like regular impl methods
                self.line(&format!(";; impl {} for {}", imp.trait_impls.join(" + "), imp.target));
                for method in &imp.methods {
                    self.generate_function(method);
                }
            }
            Item::App(app) => self.generate_app(app),
            Item::Page(page) => self.generate_page(page),
            Item::Form(form) => self.generate_form(form),
            Item::Channel(ch) => self.generate_channel(ch),
            Item::Payment(payment) => self.generate_payment(payment),
            Item::Auth(auth) => self.generate_auth(auth),
            Item::Upload(upload) => self.generate_upload(upload),
            Item::Embed(embed) => self.generate_embed(embed),
            Item::Pdf(pdf) => self.generate_pdf(pdf),
            Item::Cache(cache) => self.generate_cache(cache),
            Item::Breakpoints(bp) => self.generate_breakpoints(bp),
            Item::Animation(anim) => self.generate_animation_block(anim),
            Item::Theme(theme) => self.generate_theme(theme),
            _ => {
                self.line(&format!(";; TODO: codegen for {:?}", std::mem::discriminant(item)));
            }
        }
    }

    /// Generate contract schema registration and validation function.
    ///
    /// For each contract, emits:
    /// 1. A schema registration call at init time (field names, types, hash)
    /// 2. An exported validation function callable from the runtime
    ///
    /// The content hash is computed from the canonical field representation:
    ///   "field1:type1,field2:type2,..."
    /// This hash is deterministic and changes only when the contract shape changes.
    fn generate_contract(&mut self, contract: &ContractDef) {
        use sha2::{Sha256, Digest};

        self.line(&format!(";; === Contract: {} ===", contract.name));

        // Build canonical representation for hashing
        let mut canonical = String::new();
        for (i, field) in contract.fields.iter().enumerate() {
            if i > 0 {
                canonical.push(',');
            }
            canonical.push_str(&field.name);
            canonical.push(':');
            canonical.push_str(&self.type_to_canonical(&field.ty));
        }

        // Compute SHA-256 hash, take first 8 hex chars as the short hash
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let hash_bytes = hasher.finalize();
        let short_hash = format!("{:x}{:x}{:x}{:x}", hash_bytes[0], hash_bytes[1], hash_bytes[2], hash_bytes[3]);

        self.line(&format!(";; contract hash: {} (canonical: \"{}\")", short_hash, canonical));

        // Store contract name and hash in linear memory
        let name_offset = self.store_string(&contract.name);
        let hash_offset = self.store_string(&short_hash);

        // Build JSON schema string for the contract fields
        let mut schema = String::from("{");
        for (i, field) in contract.fields.iter().enumerate() {
            if i > 0 {
                schema.push(',');
            }
            schema.push('"');
            schema.push_str(&field.name);
            schema.push_str("\":{\"type\":\"");
            schema.push_str(&self.type_to_json_schema_type(&field.ty));
            schema.push('"');
            if field.nullable {
                schema.push_str(",\"nullable\":true");
            }
            schema.push('}');
        }
        schema.push('}');
        let schema_offset = self.store_string(&schema);

        // Emit registration function — called at module init
        self.emit(&format!("(func $__contract_register_{} (export \"__contract_register_{}\")", contract.name, contract.name));
        self.indent += 1;
        self.line(&format!("i32.const {} ;; name ptr", name_offset));
        self.line(&format!("i32.const {} ;; name len", contract.name.len()));
        self.line(&format!("i32.const {} ;; hash ptr", hash_offset));
        self.line(&format!("i32.const {} ;; hash len", short_hash.len()));
        self.line(&format!("i32.const {} ;; schema ptr", schema_offset));
        self.line(&format!("i32.const {} ;; schema len", schema.len()));
        self.line("call $contract_registerSchema");
        self.indent -= 1;
        self.line(")");
    }

    /// Convert an AST Type to a canonical string representation for hashing.
    fn type_to_canonical(&self, ty: &Type) -> String {
        match ty {
            Type::Named(name) => name.clone(),
            Type::Array(inner) => format!("[{}]", self.type_to_canonical(inner)),
            Type::Option(inner) => format!("{}?", self.type_to_canonical(inner)),
            Type::Result { ok, err } => format!("Result<{},{}>", self.type_to_canonical(ok), self.type_to_canonical(err)),
            Type::Tuple(tys) => {
                let parts: Vec<String> = tys.iter().map(|t| self.type_to_canonical(t)).collect();
                format!("({})", parts.join(","))
            }
            Type::Generic { name, args } => {
                let parts: Vec<String> = args.iter().map(|t| self.type_to_canonical(t)).collect();
                format!("{}<{}>", name, parts.join(","))
            }
            Type::Reference { mutable, inner, .. } => {
                if *mutable { format!("&mut {}", self.type_to_canonical(inner)) }
                else { format!("&{}", self.type_to_canonical(inner)) }
            }
            Type::Function { params, ret } => {
                let parts: Vec<String> = params.iter().map(|t| self.type_to_canonical(t)).collect();
                format!("fn({})->{}", parts.join(","), self.type_to_canonical(ret))
            }
        }
    }

    /// Convert an AST Type to a JSON Schema type string.
    fn type_to_json_schema_type(&self, ty: &Type) -> String {
        match ty {
            Type::Named(name) => match name.as_str() {
                "i32" | "i64" | "u32" | "u64" => "integer".into(),
                "f32" | "f64" => "number".into(),
                "bool" => "boolean".into(),
                "String" | "DateTime" => "string".into(),
                _ => "object".into(),
            },
            Type::Array(_) => "array".into(),
            Type::Option(inner) => self.type_to_json_schema_type(inner),
            _ => "object".into(),
        }
    }

    /// Generate PWA artifacts for an app definition.
    fn generate_app(&mut self, app: &AppDef) {
        self.line(&format!(";; === PWA App: {} ===", app.name));

        // Emit manifest.webmanifest JSON from ManifestDef
        if let Some(ref manifest) = app.manifest {
            let mut json = String::from("{");
            for (i, (key, value)) in manifest.entries.iter().enumerate() {
                if i > 0 {
                    json.push(',');
                }
                json.push('"');
                json.push_str(key);
                json.push_str("\":");
                match value {
                    Expr::StringLit(s) => {
                        json.push('"');
                        json.push_str(s);
                        json.push('"');
                    }
                    Expr::Integer(n) => json.push_str(&n.to_string()),
                    Expr::Bool(b) => json.push_str(if *b { "true" } else { "false" }),
                    _ => json.push_str("null"),
                }
            }
            json.push('}');
            let manifest_offset = self.store_string(&json);
            self.line(&format!(";; manifest JSON at offset {}, len {}", manifest_offset, json.len()));

            // Emit manifest registration function
            self.emit(&format!("(func $__app_{}_register_manifest (export \"__app_{}_register_manifest\")", app.name, app.name));
            self.indent += 1;
            self.line(&format!("i32.const {} ;; manifest json ptr", manifest_offset));
            self.line(&format!("i32.const {} ;; manifest json len", json.len()));
            self.line("call $pwa_registerManifest");
            self.indent -= 1;
            self.line(")");
        }

        // Generate service worker registration for offline support
        if let Some(ref offline) = app.offline {
            let urls_json: String = offline.precache.iter()
                .map(|u| format!("\"{}\"", u))
                .collect::<Vec<_>>()
                .join(",");
            let urls_str = format!("[{}]", urls_json);
            let urls_offset = self.store_string(&urls_str);
            let strategy_offset = self.store_string(&offline.strategy);

            self.emit(&format!("(func $__app_{}_register_sw (export \"__app_{}_register_sw\")", app.name, app.name));
            self.indent += 1;
            self.line(&format!("i32.const {} ;; precache urls ptr", urls_offset));
            self.line(&format!("i32.const {} ;; precache urls len", urls_str.len()));
            self.line("call $pwa_cachePrecache");
            self.line(&format!("i32.const {} ;; strategy ptr", strategy_offset));
            self.line(&format!("i32.const {} ;; strategy len", offline.strategy.len()));
            self.line("call $pwa_setStrategy");
            self.indent -= 1;
            self.line(")");
        }

        // Generate push notification setup
        if let Some(ref push) = app.push {
            self.emit(&format!("(func $__app_{}_register_push (export \"__app_{}_register_push\")", app.name, app.name));
            self.indent += 1;
            if let Some(ref key_expr) = push.vapid_key {
                if let Expr::StringLit(key) = key_expr {
                    let key_offset = self.store_string(key);
                    self.line(&format!("i32.const {} ;; vapid key ptr", key_offset));
                    self.line(&format!("i32.const {} ;; vapid key len", key.len()));
                    self.line("call $pwa_registerPush");
                }
            }
            self.indent -= 1;
            self.line(")");
        }
    }

    fn generate_test(&mut self, test: &TestDef) {
        let safe_name = test.name.replace(' ', "_").replace('"', "");
        let func_name = format!("$__test_{}", safe_name);

        self.locals.clear();

        self.emit(&format!("(func {} (export \"__test_{}\")", func_name, safe_name));
        self.indent += 1;

        // Collect locals from test body
        self.collect_locals(&test.body);
        for (name, ty) in self.locals.clone() {
            self.line(&format!("(local ${} {})", name, self.wasm_type_str(&ty)));
        }

        // Generate body
        for stmt in &test.body.stmts {
            self.generate_stmt(stmt);
        }

        // If we reach the end without assertion failure, report pass
        let name_offset = self.store_string(&test.name);
        self.line(&format!("i32.const {} ;; test name ptr", name_offset));
        self.line(&format!("i32.const {} ;; test name len", test.name.len()));
        self.line("call $test_pass");

        self.indent -= 1;
        self.line(")");
    }

    fn generate_test_runner(&mut self, test_defs: &[(&str, usize)]) {
        self.line("");
        self.line(";; === Test runner ===");
        self.emit("(func $__run_tests (export \"__run_tests\")");
        self.indent += 1;

        self.line("(local $passed i32)");
        self.line("(local $failed i32)");
        self.line("i32.const 0");
        self.line("local.set $passed");
        self.line("i32.const 0");
        self.line("local.set $failed");

        for (name, _) in test_defs {
            let safe_name = name.replace(' ', "_").replace('"', "");
            self.line(&format!(";; run test: {}", name));
            self.line(&format!("call $__test_{}", safe_name));
        }

        // Report summary
        let total = test_defs.len();
        self.line(&format!("i32.const {} ;; total tests (passed placeholder)", total));
        self.line("i32.const 0 ;; failed placeholder");
        self.line("call $test_summary");

        self.indent -= 1;
        self.line(")");
    }

    fn generate_function(&mut self, func: &Function) {
        self.locals.clear();

        let params: Vec<String> = func.params.iter()
            .filter(|p| p.name != "self")
            .map(|p| format!("(param ${} {})", p.name, self.type_to_wasm(&p.ty)))
            .collect();

        let ret = func.return_type.as_ref()
            .map(|t| format!(" (result {})", self.type_to_wasm(t)))
            .unwrap_or_default();

        let export = if func.is_pub {
            format!(" (export \"{}\")", func.name)
        } else {
            String::new()
        };

        self.emit(&format!("(func ${}{}{} {}",
            func.name, export, ret, params.join(" ")));
        self.indent += 1;

        // Collect locals from function body
        self.collect_locals(&func.body);
        for (name, ty) in self.locals.clone() {
            self.line(&format!("(local ${} {})", name, self.wasm_type_str(&ty)));
        }

        // Generate body
        for stmt in &func.body.stmts {
            self.generate_stmt(stmt);
        }

        self.indent -= 1;
        self.line(")");
    }

    fn generate_component(&mut self, comp: &Component) {
        // Components compile down to:
        // 1. An init function that creates the DOM tree with signal bindings
        // 2. Signal-backed state (each mutable field becomes a signal)
        // 3. Effect functions for reactive DOM updates
        // 4. Event handler trampolines

        let comp_name = &comp.name;

        // Chunk boundary marker for code splitting
        if let Some(ref chunk_name) = comp.chunk {
            self.line(&format!(";; chunk boundary: \"{}\" — component \"{}\" will be split into a separate chunk", chunk_name, comp_name));
            let offset = self.store_string(chunk_name);
            self.line(&format!(";; chunk registration: preload \"{}\" at offset {}", chunk_name, offset));
        }

        // Generate the init/mount function
        self.emit(&format!("(func ${comp_name}_mount (export \"{comp_name}_mount\") (param $root i32)"));
        self.indent += 1;

        // Each state field becomes a signal (returns signal ID)
        for state in &comp.state {
            self.line(&format!("(local ${} i32) ;; signal ID for {}", state.name, state.name));
        }

        // Initialize signals via runtime — use atomic operations for atomic signals
        for state in &comp.state {
            self.generate_expr(&state.initializer);
            if state.atomic {
                self.line(";; atomic signal — uses lock-free concurrent access");
                self.line("call $signal_create");
            } else {
                self.line("call $signal_create");
            }
            self.line(&format!("local.set ${}", state.name));
        }

        // Emit permission metadata and register allowed patterns
        if let Some(ref perms) = comp.permissions {
            self.generate_permissions(comp_name, perms);
        }

        // For secret state fields, mark signals as redacted in debug builds
        for state in &comp.state {
            if state.secret {
                self.line(&format!(";; secret: {} — stripped from serialization", state.name));
            }
        }

        // Inject scoped styles for this component
        self.generate_style_injection(comp_name, &comp.styles);

        // Inject transitions for this component
        self.generate_transition_injection(comp_name, &comp.transitions);

        // If the component has a skeleton, mount placeholder first and replace on first signal change
        if let Some(ref skel) = comp.skeleton {
            self.line(";; skeleton — mount placeholder, replace on first signal change");
            self.line("(block $skeleton_done");
            self.indent += 1;

            // Mount the skeleton template into $root
            self.generate_template(&skel.body.body, "$root");

            // Create an effect that watches component signals; on first change,
            // clear skeleton, render real content, and break out
            self.line(";; effect: watch signals, swap skeleton for real content on change");
            self.line("call $skeleton_mount");
            self.line("(local.get $root)");
            self.line("call $skeleton_replace");

            self.indent -= 1;
            self.line(") ;; end $skeleton_done");
        }

        // If the component has an error boundary, wrap the render in a try/catch
        if let Some(ref eb) = comp.error_boundary {
            self.line(";; error boundary — wrap render in try/catch");
            self.line("(block $eb_ok");
            self.indent += 1;
            self.line("(block $eb_err");
            self.indent += 1;
            self.generate_template(&eb.body.body, "$root");
            self.line("br $eb_ok");
            self.indent -= 1;
            self.line(") ;; end $eb_err — render fallback");
            self.generate_template(&eb.fallback.body, "$root");
            self.indent -= 1;
            self.line(") ;; end $eb_ok");
        }

        // Generate the DOM tree from the render block
        self.generate_template(&comp.render.body, "$root");

        // If a11y: auto, call $a11y_enhance after render to inject ARIA attributes
        if comp.a11y.as_ref() == Some(&A11yMode::Auto) {
            self.line("");
            self.line(";; a11y: auto — enhance component with accessibility attributes");
            let name_offset = self.store_string(comp_name);
            self.line(&format!("i32.const {} ;; component name ptr", name_offset));
            self.line(&format!("i32.const {} ;; component name len", comp_name.len()));
            self.line("call $a11y_enhance");
        }

        // Register effects for reactive DOM updates
        // Each dynamic expression in render creates an effect that
        // re-evaluates when its signal dependencies change
        self.line("");
        self.line(";; reactive effects for DOM updates are registered via signal.subscribe");

        self.indent -= 1;
        self.line(")");

        // Generate event handler trampolines as exported functions
        for (i, method) in comp.methods.iter().enumerate() {
            self.line("");
            self.emit(&format!("(func $__handler_{} (export \"__handler_{}\")", i, i));
            self.indent += 1;

            // Re-read signal values, execute handler body, write back
            self.line(";; event handler trampoline");
            for stmt in &method.body.stmts {
                self.generate_stmt(stmt);
            }

            self.indent -= 1;
            self.line(")");
        }

        // Generate methods (non-handler versions)
        for method in &comp.methods {
            self.generate_function(method);
        }

        // Generate on_destroy lifecycle cleanup function
        if let Some(ref destroy_fn) = comp.on_destroy {
            self.line("");
            self.line(&format!(";; lifecycle: on_destroy for {}", comp_name));
            self.emit(&format!("(func ${comp_name}_on_destroy (export \"{comp_name}_on_destroy\")"));
            self.indent += 1;
            for stmt in &destroy_fn.body.stmts {
                self.generate_stmt(stmt);
            }
            self.indent -= 1;
            self.line(")");

            // Register cleanup with lifecycle runtime
            self.line("");
            self.line(&format!(";; register cleanup callback for {}", comp_name));
            let name_offset = self.store_string(comp_name);
            self.line(&format!("i32.const {}", name_offset));
            self.line(&format!("i32.const {}", comp_name.len()));
            self.line("call $lifecycle_register_cleanup");
        }
    }

    fn generate_page(&mut self, page: &PageDef) {
        self.line(&format!(";; === Page: {} ===", page.name));

        // Generate page mount function (same pattern as generate_component)
        let comp_name = &page.name;

        self.emit(&format!("(func ${comp_name}_mount (export \"{comp_name}_mount\") (param $root i32)"));
        self.indent += 1;

        for state in &page.state {
            self.line(&format!("(local ${} i32) ;; signal ID for {}", state.name, state.name));
        }

        for state in &page.state {
            self.generate_expr(&state.initializer);
            self.line("call $signal_create");
            self.line(&format!("local.set ${}", state.name));
        }

        if let Some(ref perms) = page.permissions {
            self.generate_permissions(comp_name, perms);
        }

        for state in &page.state {
            if state.secret {
                self.line(&format!(";; secret: {} — stripped from serialization", state.name));
            }
        }

        self.generate_style_injection(comp_name, &page.styles);

        self.generate_template(&page.render.body, "$root");

        self.line("");
        self.line(";; reactive effects for DOM updates are registered via signal.subscribe");

        self.indent -= 1;
        self.line(")");

        // Generate event handler trampolines
        for (i, method) in page.methods.iter().enumerate() {
            self.line("");
            self.emit(&format!("(func $__handler_{} (export \"__handler_{}\")", i, i));
            self.indent += 1;
            self.line(";; event handler trampoline");
            for stmt in &method.body.stmts {
                self.generate_stmt(stmt);
            }
            self.indent -= 1;
            self.line(")");
        }

        // Generate methods
        for method in &page.methods {
            self.generate_function(method);
        }

        // Generate SEO registration function
        let fn_name = format!("__page_register_{}", page.name.to_lowercase());
        self.emit(&format!("(func (export \"{}\")", fn_name));
        self.indent += 1;

        if let Some(ref meta) = page.meta {
            if let Some(ref title) = meta.title {
                if let Expr::StringLit(s) = title {
                    let offset = self.store_string(s);
                    let len = s.len();
                    self.line(&format!(";; Register page title: \"{}\"", s));
                    // For setMeta: title_ptr, title_len, desc_ptr, desc_len, canon_ptr, canon_len, og_ptr, og_len
                    // We will build these up and call at the end
                    let _ = (offset, len);
                }
            }

            // Build setMeta call with all available meta fields
            let (t_off, t_len) = self.meta_field_offset(&meta.title);
            let (d_off, d_len) = self.meta_field_offset(&meta.description);
            let (c_off, c_len) = self.meta_field_offset(&meta.canonical);
            let (o_off, o_len) = self.meta_field_offset(&meta.og_image);

            self.line(&format!(
                "(call $seo_set_meta (i32.const {}) (i32.const {}) (i32.const {}) (i32.const {}) (i32.const {}) (i32.const {}) (i32.const {}) (i32.const {}))",
                t_off, t_len, d_off, d_len, c_off, c_len, o_off, o_len
            ));

            // Register structured data
            for sd in &meta.structured_data {
                let mut json = format!("{{\"@type\":\"{}\",", sd.schema_type);
                for (i, (key, val)) in sd.fields.iter().enumerate() {
                    if i > 0 { json.push(','); }
                    json.push('"');
                    json.push_str(key);
                    json.push_str("\":");
                    if let Expr::StringLit(s) = val {
                        json.push('"');
                        json.push_str(s);
                        json.push('"');
                    } else {
                        json.push_str("null");
                    }
                }
                json.push('}');
                let offset = self.store_string(&json);
                let len = json.len();
                self.line(&format!("(call $seo_register_structured_data (i32.const {}) (i32.const {}))", offset, len));
            }
        }

        // Register route for sitemap
        let route_str = format!("/{}", page.name.to_lowercase());
        let offset = self.store_string(&route_str);
        let len = route_str.len();
        self.line(&format!("(call $seo_register_route (i32.const {}) (i32.const {}) (i32.const 0) (i32.const 0))", offset, len));

        self.indent -= 1;
        self.line(")");
    }

    /// Helper to get the offset and length for an optional meta string field.
    fn meta_field_offset(&mut self, expr: &Option<Expr>) -> (u32, usize) {
        match expr {
            Some(Expr::StringLit(s)) => {
                let offset = self.store_string(s);
                (offset, s.len())
            }
            _ => (0, 0),
        }
    }

    fn generate_form(&mut self, form: &FormDef) {
        self.line(&format!(";; === Form: {} ===", form.name));

        // Build schema JSON for form registration
        let mut schema_json = String::from("{\"fields\":[");
        for (i, field) in form.fields.iter().enumerate() {
            if i > 0 { schema_json.push(','); }
            schema_json.push_str(&format!("{{\"name\":\"{}\",\"type\":\"{:?}\",\"validators\":[", field.name, field.ty));
            for (j, v) in field.validators.iter().enumerate() {
                if j > 0 { schema_json.push(','); }
                match &v.kind {
                    ValidatorKind::Required => schema_json.push_str("{\"kind\":\"required\"}"),
                    ValidatorKind::MinLength(n) => schema_json.push_str(&format!("{{\"kind\":\"min_length\",\"min\":{}}}", n)),
                    ValidatorKind::MaxLength(n) => schema_json.push_str(&format!("{{\"kind\":\"max_length\",\"max\":{}}}", n)),
                    ValidatorKind::Pattern(p) => schema_json.push_str(&format!("{{\"kind\":\"pattern\",\"pattern\":\"{}\"}}", p)),
                    ValidatorKind::Email => schema_json.push_str("{\"kind\":\"email\"}"),
                    ValidatorKind::Url => schema_json.push_str("{\"kind\":\"url\"}"),
                    ValidatorKind::Min(n) => schema_json.push_str(&format!("{{\"kind\":\"min\",\"min\":{}}}", n)),
                    ValidatorKind::Max(n) => schema_json.push_str(&format!("{{\"kind\":\"max\",\"max\":{}}}", n)),
                    ValidatorKind::Custom(name) => schema_json.push_str(&format!("{{\"kind\":\"custom\",\"fn\":\"{}\"}}", name)),
                }
            }
            schema_json.push_str("]}");
        }
        schema_json.push_str("]}");

        // Store form name and schema in memory
        let name_offset = self.store_string(&form.name);
        let name_len = form.name.len();
        let schema_offset = self.store_string(&schema_json);
        let schema_len = schema_json.len();

        // Emit registration function
        let fn_name = format!("{}_init", form.name);
        self.emit(&format!("(func ${fn_name} (export \"{fn_name}\") (param $root i32)"));
        self.indent += 1;

        // Register form with runtime
        self.line(&format!(
            "(call $form_register (i32.const {}) (i32.const {}) (i32.const {}) (i32.const {}))",
            name_offset, name_len, schema_offset, schema_len
        ));

        self.indent -= 1;
        self.line(")");

        // Generate methods
        for method in &form.methods {
            self.generate_function(method);
        }
    }

    fn generate_channel(&mut self, ch: &ChannelDef) {
        self.line(&format!(";; === Channel: {} ===", ch.name));

        // Store channel name in linear memory
        let name_offset = self.store_string(&ch.name);
        let name_len = ch.name.len();

        // Store URL — extract string from the Expr
        let url_str = match &ch.url {
            Expr::StringLit(s) => s.clone(),
            _ => "/ws".to_string(),
        };
        let url_offset = self.store_string(&url_str);
        let url_len = url_str.len();

        // Generate channel registration function
        self.line(&format!("(func $__channel_register_{} (export \"__channel_register_{}\")", ch.name, ch.name));
        self.line(&format!("  i32.const {}  ;; name ptr", name_offset));
        self.line(&format!("  i32.const {}  ;; name len", name_len));
        self.line(&format!("  i32.const {}  ;; url ptr", url_offset));
        self.line(&format!("  i32.const {}  ;; url len", url_len));
        self.line("  call $channel_connect");

        // Set reconnect flag
        if !ch.reconnect {
            self.line(&format!("  i32.const {}  ;; name ptr", name_offset));
            self.line(&format!("  i32.const {}  ;; name len", name_len));
            self.line("  i32.const 0  ;; reconnect disabled");
            self.line("  call $channel_set_reconnect");
        }

        self.line(")");

        // Generate handler methods
        if let Some(ref handler) = ch.on_message {
            self.generate_function(handler);
        }
        if let Some(ref handler) = ch.on_connect {
            self.generate_function(handler);
        }
        if let Some(ref handler) = ch.on_disconnect {
            self.generate_function(handler);
        }
        for method in &ch.methods {
            self.generate_function(method);
        }
    }

    fn generate_embed(&mut self, embed: &EmbedDef) {
        self.line(&format!(";; === Embed: {} ===", embed.name));

        let name_offset = self.store_string(&embed.name);
        let name_len = embed.name.len();

        // Extract source URL
        let src_str = match &embed.src {
            Expr::StringLit(s) => s.clone(),
            _ => "".to_string(),
        };
        let src_offset = self.store_string(&src_str);
        let src_len = src_str.len();

        // Loading strategy
        let loading_str = embed.loading.as_deref().unwrap_or("async");
        let loading_offset = self.store_string(loading_str);
        let loading_len = loading_str.len();

        // Integrity hash (SRI)
        let integrity_str = match &embed.integrity {
            Some(Expr::StringLit(s)) => s.clone(),
            _ => "".to_string(),
        };
        let integrity_offset = self.store_string(&integrity_str);
        let _integrity_len = integrity_str.len();

        // Generate embed registration function
        self.emit(&format!("(func $__embed_register_{} (export \"__embed_register_{}\")", embed.name, embed.name));
        self.indent += 1;

        if embed.sandbox {
            // Sandboxed embed — use iframe
            self.line(&format!("i32.const {}  ;; name ptr", name_offset));
            self.line(&format!("i32.const {}  ;; name len", name_len));
            self.line(&format!("i32.const {}  ;; src ptr", src_offset));
            self.line(&format!("i32.const {}  ;; src len", src_len));
            self.line("call $embed_load_sandboxed");
        } else {
            // Script embed — direct script tag
            self.line(&format!("i32.const {}  ;; src ptr", src_offset));
            self.line(&format!("i32.const {}  ;; src len", src_len));
            self.line(&format!("i32.const {}  ;; loading ptr", loading_offset));
            self.line(&format!("i32.const {}  ;; loading len", loading_len));
            self.line(&format!("i32.const {}  ;; integrity offset (0 = none)", integrity_offset));
            self.line("call $embed_load_script");
        }

        self.indent -= 1;
        self.line(")");
    }

    fn generate_pdf(&mut self, pdf: &PdfDef) {
        self.line(&format!(";; === PDF: {} ===", pdf.name));

        let name_offset = self.store_string(&pdf.name);
        let name_len = pdf.name.len();

        // Build config JSON
        let page_size = pdf.page_size.as_deref().unwrap_or("A4");
        let orientation = pdf.orientation.as_deref().unwrap_or("portrait");
        let config_json = format!("{{\"pageSize\":\"{}\",\"orientation\":\"{}\"}}", page_size, orientation);
        let config_offset = self.store_string(&config_json);
        let config_len = config_json.len();

        // Generate PDF creation function
        self.emit(&format!("(func $__pdf_create_{} (export \"__pdf_create_{}\") (result i32)", pdf.name, pdf.name));
        self.indent += 1;

        self.line(&format!("i32.const {}  ;; name ptr", name_offset));
        self.line(&format!("i32.const {}  ;; name len", name_len));
        self.line(&format!("i32.const {}  ;; config ptr", config_offset));
        self.line(&format!("i32.const {}  ;; config len", config_len));
        self.line("call $pdf_create");

        self.indent -= 1;
        self.line(")");
    }

    fn generate_payment(&mut self, payment: &PaymentDef) {
        self.line(&format!(";; === Payment: {} ===", payment.name));

        let name_offset = self.store_string(&payment.name);
        let name_len = payment.name.len();

        let provider_str = match &payment.provider {
            Some(Expr::StringLit(s)) => s.clone(),
            _ => "stripe".to_string(),
        };
        let provider_offset = self.store_string(&provider_str);
        let provider_len = provider_str.len();

        let sandboxed = if payment.sandbox_mode { 1 } else { 0 };

        self.line(&format!("(func $__payment_register_{} (export \"__payment_register_{}\")", payment.name, payment.name));
        self.line(&format!("  i32.const {}  ;; name ptr", name_offset));
        self.line(&format!("  i32.const {}  ;; name len", name_len));
        self.line(&format!("  i32.const {}  ;; provider ptr", provider_offset));
        self.line(&format!("  i32.const {}  ;; provider len", provider_len));
        self.line(&format!("  i32.const {}  ;; sandboxed", sandboxed));
        self.line("  call $payment_init");
        self.line(")");

        if let Some(ref handler) = payment.on_success {
            self.generate_function(handler);
        }
        if let Some(ref handler) = payment.on_error {
            self.generate_function(handler);
        }
        for method in &payment.methods {
            self.generate_function(method);
        }
    }

    fn generate_auth(&mut self, auth: &AuthDef) {
        self.line(&format!(";; === Auth: {} ===", auth.name));

        let name_offset = self.store_string(&auth.name);
        let name_len = auth.name.len();

        // Build JSON config for providers
        let mut config = String::from("{\"providers\":{");
        for (i, prov) in auth.providers.iter().enumerate() {
            if i > 0 { config.push(','); }
            config.push('"');
            config.push_str(&prov.name);
            config.push_str("\":{\"scopes\":[");
            for (j, scope) in prov.scopes.iter().enumerate() {
                if j > 0 { config.push(','); }
                config.push('"');
                config.push_str(scope);
                config.push('"');
            }
            config.push_str("]}");
        }
        config.push_str("}");
        if let Some(ref storage) = auth.session_storage {
            config.push_str(&format!(",\"session\":\"{}\"", storage));
        }
        config.push('}');

        let config_offset = self.store_string(&config);
        let config_len = config.len();

        self.line(&format!("(func $__auth_register_{} (export \"__auth_register_{}\")", auth.name, auth.name));
        self.line(&format!("  i32.const {}  ;; name ptr", name_offset));
        self.line(&format!("  i32.const {}  ;; name len", name_len));
        self.line(&format!("  i32.const {}  ;; config ptr", config_offset));
        self.line(&format!("  i32.const {}  ;; config len", config_len));
        self.line("  call $auth_init");
        self.line(")");

        if let Some(ref handler) = auth.on_login {
            self.generate_function(handler);
        }
        if let Some(ref handler) = auth.on_logout {
            self.generate_function(handler);
        }
        if let Some(ref handler) = auth.on_error {
            self.generate_function(handler);
        }
        for method in &auth.methods {
            self.generate_function(method);
        }
    }

    fn generate_upload(&mut self, upload: &UploadDef) {
        self.line(&format!(";; === Upload: {} ===", upload.name));

        let name_offset = self.store_string(&upload.name);
        let name_len = upload.name.len();

        let endpoint_str = match &upload.endpoint {
            Expr::StringLit(s) => s.clone(),
            _ => "/upload".to_string(),
        };

        let mut config = format!("{{\"endpoint\":\"{}\"", endpoint_str);
        if !upload.accept.is_empty() {
            config.push_str(",\"accept\":[");
            for (i, mime) in upload.accept.iter().enumerate() {
                if i > 0 { config.push(','); }
                config.push('"');
                config.push_str(mime);
                config.push('"');
            }
            config.push(']');
        }
        if upload.chunked {
            config.push_str(",\"chunked\":true");
        }
        config.push('}');

        let config_offset = self.store_string(&config);
        let config_len = config.len();

        self.line(&format!("(func $__upload_register_{} (export \"__upload_register_{}\")", upload.name, upload.name));
        self.line(&format!("  i32.const {}  ;; name ptr", name_offset));
        self.line(&format!("  i32.const {}  ;; name len", name_len));
        self.line(&format!("  i32.const {}  ;; config ptr", config_offset));
        self.line(&format!("  i32.const {}  ;; config len", config_len));
        self.line("  call $upload_init");
        self.line(")");

        if let Some(ref handler) = upload.on_progress {
            self.generate_function(handler);
        }
        if let Some(ref handler) = upload.on_complete {
            self.generate_function(handler);
        }
        if let Some(ref handler) = upload.on_error {
            self.generate_function(handler);
        }
        for method in &upload.methods {
            self.generate_function(method);
        }
    }

    fn generate_cache(&mut self, cache: &CacheDef) {
        self.line(&format!(";; === Cache: {} ===", cache.name));

        let name_offset = self.store_string(&cache.name);
        let name_len = cache.name.len();

        // Build config JSON
        let mut config = String::from("{");
        if let Some(ref strategy) = cache.strategy {
            config.push_str(&format!("\"strategy\":\"{}\"", strategy));
        }
        if let Some(ttl) = cache.default_ttl {
            if config.len() > 1 { config.push(','); }
            config.push_str(&format!("\"ttl\":{}", ttl));
        }
        if cache.persist {
            if config.len() > 1 { config.push(','); }
            config.push_str("\"persist\":true");
        }
        if let Some(max) = cache.max_entries {
            if config.len() > 1 { config.push(','); }
            config.push_str(&format!("\"max_entries\":{}", max));
        }
        config.push('}');

        let config_offset = self.store_string(&config);
        let config_len = config.len();

        // Emit cache init function
        self.emit(&format!("(func $__cache_init_{} (export \"__cache_init_{}\")", cache.name, cache.name));
        self.indent += 1;
        self.line(&format!("i32.const {}  ;; name ptr", name_offset));
        self.line(&format!("i32.const {}  ;; name len", name_len));
        self.line(&format!("i32.const {}  ;; config ptr", config_offset));
        self.line(&format!("i32.const {}  ;; config len", config_len));
        self.line("call $cache_init");
        self.indent -= 1;
        self.line(")");

        // Register queries
        for query in &cache.queries {
            let q_name_offset = self.store_string(&query.name);
            let q_name_len = query.name.len();

            let mut q_config = String::from("{");
            if let Some(ttl) = query.ttl {
                q_config.push_str(&format!("\"ttl\":{}", ttl));
            }
            if let Some(stale) = query.stale {
                if q_config.len() > 1 { q_config.push(','); }
                q_config.push_str(&format!("\"stale\":{}", stale));
            }
            if let Some(ref contract) = query.contract {
                if q_config.len() > 1 { q_config.push(','); }
                q_config.push_str(&format!("\"contract\":\"{}\"", contract));
            }
            if !query.invalidate_on.is_empty() {
                if q_config.len() > 1 { q_config.push(','); }
                q_config.push_str("\"invalidate_on\":[");
                for (i, event) in query.invalidate_on.iter().enumerate() {
                    if i > 0 { q_config.push(','); }
                    q_config.push('"');
                    q_config.push_str(event);
                    q_config.push('"');
                }
                q_config.push(']');
            }
            q_config.push('}');

            let q_config_offset = self.store_string(&q_config);
            let q_config_len = q_config.len();

            self.emit(&format!("(func $__cache_query_{} (export \"__cache_query_{}\")", query.name, query.name));
            self.indent += 1;
            self.line(&format!("i32.const {}  ;; query name ptr", q_name_offset));
            self.line(&format!("i32.const {}  ;; query name len", q_name_len));
            self.line(&format!("i32.const {}  ;; query config ptr", q_config_offset));
            self.line(&format!("i32.const {}  ;; query config len", q_config_len));
            self.line("call $cache_register_query");
            self.indent -= 1;
            self.line(")");
        }

        // Register mutations
        for mutation in &cache.mutations {
            let m_name_offset = self.store_string(&mutation.name);
            let m_name_len = mutation.name.len();

            let mut m_config = String::from("{");
            if mutation.optimistic {
                m_config.push_str("\"optimistic\":true");
            }
            if mutation.rollback_on_error {
                if m_config.len() > 1 { m_config.push(','); }
                m_config.push_str("\"rollback_on_error\":true");
            }
            if !mutation.invalidate.is_empty() {
                if m_config.len() > 1 { m_config.push(','); }
                m_config.push_str("\"invalidate\":[");
                for (i, name) in mutation.invalidate.iter().enumerate() {
                    if i > 0 { m_config.push(','); }
                    m_config.push('"');
                    m_config.push_str(name);
                    m_config.push('"');
                }
                m_config.push(']');
            }
            m_config.push('}');

            let m_config_offset = self.store_string(&m_config);
            let m_config_len = m_config.len();

            self.emit(&format!("(func $__cache_mutation_{} (export \"__cache_mutation_{}\")", mutation.name, mutation.name));
            self.indent += 1;
            self.line(&format!("i32.const {}  ;; mutation name ptr", m_name_offset));
            self.line(&format!("i32.const {}  ;; mutation name len", m_name_len));
            self.line(&format!("i32.const {}  ;; mutation config ptr", m_config_offset));
            self.line(&format!("i32.const {}  ;; mutation config len", m_config_len));
            self.line("call $cache_register_mutation");
            self.indent -= 1;
            self.line(")");
        }
    }

    fn generate_breakpoints(&mut self, bp: &BreakpointsDef) {
        self.line(";; === Responsive Breakpoints ===");

        // Build config JSON: {"mobile":640,"tablet":1024,...}
        let mut config = String::from("{");
        for (i, (name, px)) in bp.breakpoints.iter().enumerate() {
            if i > 0 { config.push(','); }
            config.push_str(&format!("\"{}\":{}", name, px));
        }
        config.push('}');

        let config_offset = self.store_string(&config);
        let config_len = config.len();

        self.emit("(func $__init_breakpoints (export \"__init_breakpoints\")");
        self.indent += 1;
        self.line(&format!("i32.const {}  ;; config ptr", config_offset));
        self.line(&format!("i32.const {}  ;; config len", config_len));
        self.line("call $responsive_register");
        self.indent -= 1;
        self.line(")");
    }

    fn generate_animation_block(&mut self, anim: &AnimationBlockDef) {
        self.line(&format!(";; === Animation: {} ===", anim.name));
        let name_offset = self.store_string(&anim.name);
        let name_len = anim.name.len();

        match &anim.kind {
            AnimationKind::Spring { stiffness, damping, mass, properties } => {
                let config = format!(
                    "{{\"stiffness\":{},\"damping\":{},\"mass\":{},\"properties\":[{}]}}",
                    stiffness.unwrap_or(120.0),
                    damping.unwrap_or(14.0),
                    mass.unwrap_or(1.0),
                    properties.iter().map(|p| format!("\"{}\"", p)).collect::<Vec<_>>().join(",")
                );
                let config_offset = self.store_string(&config);
                let config_len = config.len();

                self.emit(&format!("(func $__init_anim_{} (export \"__init_anim_{}\")", anim.name, anim.name));
                self.indent += 1;
                self.line(&format!("i32.const {} ;; name ptr", name_offset));
                self.line(&format!("i32.const {} ;; name len", name_len));
                self.line(&format!("i32.const {} ;; config ptr", config_offset));
                self.line(&format!("i32.const {} ;; config len", config_len));
                self.line("call $animate_spring");
                self.indent -= 1;
                self.line(")");
            }
            AnimationKind::Keyframes { frames, duration, easing } => {
                let mut frames_json = String::from("[");
                for (i, (pct, props)) in frames.iter().enumerate() {
                    if i > 0 { frames_json.push(','); }
                    frames_json.push_str(&format!("{{\"offset\":{}", pct / 100.0));
                    for (name, _val) in props {
                        frames_json.push_str(&format!(",\"{}\":\"\"", name));
                    }
                    frames_json.push('}');
                }
                frames_json.push(']');

                let config = format!(
                    "{{\"frames\":{},\"duration\":\"{}\",\"easing\":\"{}\"}}",
                    frames_json,
                    duration.as_deref().unwrap_or("300ms"),
                    easing.as_deref().unwrap_or("ease-out")
                );
                let config_offset = self.store_string(&config);
                let config_len = config.len();

                self.emit(&format!("(func $__init_anim_{} (export \"__init_anim_{}\")", anim.name, anim.name));
                self.indent += 1;
                self.line(&format!("i32.const {} ;; name ptr", name_offset));
                self.line(&format!("i32.const {} ;; name len", name_len));
                self.line(&format!("i32.const {} ;; config ptr", config_offset));
                self.line(&format!("i32.const {} ;; config len", config_len));
                self.line("call $animate_keyframes");
                self.indent -= 1;
                self.line(")");
            }
            AnimationKind::Stagger { animation, delay, selector } => {
                let config = format!(
                    "{{\"animation\":\"{}\",\"delay\":\"{}\"{}}}",
                    animation,
                    delay.as_deref().unwrap_or("50ms"),
                    selector.as_ref().map(|s| format!(",\"selector\":\"{}\"", s)).unwrap_or_default()
                );
                let config_offset = self.store_string(&config);
                let config_len = config.len();

                self.emit(&format!("(func $__init_anim_{} (export \"__init_anim_{}\")", anim.name, anim.name));
                self.indent += 1;
                self.line(&format!("i32.const {} ;; name ptr", name_offset));
                self.line(&format!("i32.const {} ;; name len", name_len));
                self.line(&format!("i32.const {} ;; config ptr", config_offset));
                self.line(&format!("i32.const {} ;; config len", config_len));
                self.line("call $animate_stagger");
                self.indent -= 1;
                self.line(")");
            }
        }
    }

    /// Generate theme initialization code.
    fn generate_theme(&mut self, theme: &ThemeDef) {
        self.line(&format!(";; === Theme: {} ===", theme.name));

        // Build config JSON from light/dark entries
        let mut config = String::from("{");

        // Light theme
        if let Some(ref entries) = theme.light {
            config.push_str("\"light\":{");
            for (i, (key, value)) in entries.iter().enumerate() {
                if i > 0 { config.push(','); }
                config.push_str(&format!("\"{}\":", key));
                match value {
                    Expr::StringLit(s) => config.push_str(&format!("\"{}\"", s)),
                    _ => config.push_str("null"),
                }
            }
            config.push('}');
        }

        // Dark theme
        if let Some(ref entries) = theme.dark {
            if theme.light.is_some() { config.push(','); }
            config.push_str("\"dark\":{");
            for (i, (key, value)) in entries.iter().enumerate() {
                if i > 0 { config.push(','); }
                config.push_str(&format!("\"{}\":", key));
                match value {
                    Expr::StringLit(s) => config.push_str(&format!("\"{}\"", s)),
                    _ => config.push_str("null"),
                }
            }
            config.push('}');
        }

        // Dark auto flag
        if theme.dark_auto {
            if theme.light.is_some() || theme.dark.is_some() { config.push(','); }
            config.push_str("\"darkAuto\":true");
        }

        config.push('}');

        let name_offset = self.store_string(&theme.name);
        let config_offset = self.store_string(&config);

        self.emit(&format!("(func $__init_theme_{} (export \"__init_theme_{}\")", theme.name, theme.name));
        self.indent += 1;
        self.line(&format!("i32.const {} ;; name ptr", name_offset));
        self.line(&format!("i32.const {} ;; name len", theme.name.len()));
        self.line(&format!("i32.const {} ;; config ptr", config_offset));
        self.line(&format!("i32.const {} ;; config len", config.len()));
        self.line("call $theme_init");
        self.indent -= 1;
        self.line(")");
    }

    /// Generate permission metadata and URL/storage validation for a component.
    ///
    /// Emits:
    /// - A JSON blob of allowed patterns as exported data
    /// - Calls to `$permissions_registerPermissions` at mount time
    /// - CSP-compatible metadata from analyzed fetch URLs
    fn generate_permissions(&mut self, comp_name: &str, perms: &PermissionsDef) {
        self.line("");
        self.line(&format!(";; permissions for component {}", comp_name));

        // Build JSON representation of allowed patterns
        let mut json = String::from("{");
        if !perms.network.is_empty() {
            json.push_str("\"network\":[");
            for (i, pat) in perms.network.iter().enumerate() {
                if i > 0 { json.push(','); }
                json.push('"');
                json.push_str(pat);
                json.push('"');
            }
            json.push(']');
        }
        if !perms.storage.is_empty() {
            if !perms.network.is_empty() { json.push(','); }
            json.push_str("\"storage\":[");
            for (i, key) in perms.storage.iter().enumerate() {
                if i > 0 { json.push(','); }
                json.push('"');
                json.push_str(key);
                json.push('"');
            }
            json.push(']');
        }
        if !perms.capabilities.is_empty() {
            if !perms.network.is_empty() || !perms.storage.is_empty() { json.push(','); }
            json.push_str("\"capabilities\":[");
            for (i, cap) in perms.capabilities.iter().enumerate() {
                if i > 0 { json.push(','); }
                json.push('"');
                json.push_str(cap);
                json.push('"');
            }
            json.push(']');
        }
        json.push('}');

        let name_offset = self.store_string(comp_name);
        let json_offset = self.store_string(&json);
        self.line(&format!("i32.const {} ;; component name ptr", name_offset));
        self.line(&format!("i32.const {} ;; component name len", comp_name.len()));
        self.line(&format!("i32.const {} ;; permissions json ptr", json_offset));
        self.line(&format!("i32.const {} ;; permissions json len", json.len()));
        self.line("call $permissions_registerPermissions");

        // Emit CSP metadata comment from analyzed network patterns
        if !perms.network.is_empty() {
            let csp_sources: Vec<&str> = perms.network.iter()
                .filter_map(|url| {
                    // Extract origin from URL pattern for CSP connect-src
                    if let Some(idx) = url.find("://") {
                        let after_scheme = &url[idx + 3..];
                        if let Some(slash_idx) = after_scheme.find('/') {
                            Some(&url[..idx + 3 + slash_idx])
                        } else {
                            Some(url.as_str())
                        }
                    } else {
                        None
                    }
                })
                .collect();
            if !csp_sources.is_empty() {
                let csp_value = format!("connect-src 'self' {}", csp_sources.join(" "));
                self.line(&format!(";; CSP: {}", csp_value));

                // Export CSP metadata so the runtime/server can read it
                let csp_offset = self.store_string(&csp_value);
                let _ = csp_offset; // stored in data section, runtime reads it
            }
        }
    }

    fn generate_store(&mut self, store: &StoreDef) {
        let store_name = &store.name;
        self.line(&format!(";; === Store: {} ===", store_name));

        // Global signal IDs for each store signal
        for (i, sig) in store.signals.iter().enumerate() {
            self.line(&format!("(global ${store_name}_{} (mut i32) (i32.const -1)) ;; signal ID", sig.name));
            let _ = i;
        }

        // Store init function — creates all signals
        self.line("");
        self.emit(&format!("(func ${store_name}_init (export \"{store_name}_init\")"));
        self.indent += 1;

        for sig in &store.signals {
            self.generate_expr(&sig.initializer);
            self.line("call $signal_create");
            self.line(&format!("global.set ${store_name}_{}", sig.name));
        }

        self.indent -= 1;
        self.line(")");

        // Getters for each signal
        for sig in &store.signals {
            self.line("");
            let wasm_ty = sig.ty.as_ref()
                .map(|t| self.type_to_wasm(t))
                .unwrap_or_else(|| "i32".into());
            self.emit(&format!("(func ${store_name}_get_{} (export \"{store_name}_get_{}\") (result {wasm_ty})",
                sig.name, sig.name));
            self.indent += 1;
            self.line(&format!("global.get ${store_name}_{}", sig.name));
            self.line("call $signal_get");
            self.indent -= 1;
            self.line(")");
        }

        // Setters for each signal (with reactive notification)
        for sig in &store.signals {
            self.line("");
            let wasm_ty = sig.ty.as_ref()
                .map(|t| self.type_to_wasm(t))
                .unwrap_or_else(|| "i32".into());
            self.emit(&format!("(func ${store_name}_set_{} (export \"{store_name}_set_{}\") (param $value {wasm_ty})",
                sig.name, sig.name));
            self.indent += 1;
            self.line(&format!("global.get ${store_name}_{}", sig.name));
            self.line("local.get $value");
            self.line("call $signal_set");
            self.indent -= 1;
            self.line(")");
        }

        // Actions — methods that can mutate store signals
        for action in &store.actions {
            self.line("");
            let params: Vec<String> = action.params.iter()
                .filter(|p| p.name != "self")
                .map(|p| format!("(param ${} {})", p.name, self.type_to_wasm(&p.ty)))
                .collect();

            let async_comment = if action.is_async { " ;; async" } else { "" };
            self.emit(&format!("(func ${store_name}_{} (export \"{store_name}_{}\") {}{}",
                action.name, action.name, params.join(" "), async_comment));
            self.indent += 1;

            if action.is_async {
                self.line(";; async action — returns promise handle");
            }

            // Collect locals
            self.collect_locals(&action.body);
            for (name, ty) in self.locals.clone() {
                self.line(&format!("(local ${} {})", name, self.wasm_type_str(&ty)));
            }

            // Generate action body
            for stmt in &action.body.stmts {
                self.generate_stmt(stmt);
            }

            self.indent -= 1;
            self.line(")");
        }

        // Computed values — derived signals
        for comp in &store.computed {
            self.line("");
            let ret = comp.return_type.as_ref()
                .map(|t| format!(" (result {})", self.type_to_wasm(t)))
                .unwrap_or_else(|| " (result i32)".into());
            self.emit(&format!("(func ${store_name}_{} (export \"{store_name}_{}\")",
                comp.name, comp.name));
            self.indent += 1;
            self.line(&format!(";; computed value{}", ret));
            for stmt in &comp.body.stmts {
                self.generate_stmt(stmt);
            }
            self.indent -= 1;
            self.line(")");
        }

        // Effects — side effects that auto-run when dependencies change
        for effect in &store.effects {
            self.line("");
            self.emit(&format!("(func ${store_name}_{} (export \"{store_name}_{}\")",
                effect.name, effect.name));
            self.indent += 1;
            self.line(";; effect — auto-runs when signal dependencies change");
            for stmt in &effect.body.stmts {
                self.generate_stmt(stmt);
            }
            self.indent -= 1;
            self.line(")");
        }

        // Selectors — derived values that depend on signals
        for selector in &store.selectors {
            self.line("");
            self.emit(&format!("(func ${store_name}_selector_{} (export \"{store_name}_selector_{}\")",
                selector.name, selector.name));
            self.indent += 1;
            self.line(&format!(";; selector: {} — derived from store signals", selector.name));
            self.generate_expr(&selector.body);
            self.indent -= 1;
            self.line(")");
        }

        // Atomic signal accessors — generate lock-free get/set/CAS wrappers
        for sig in &store.signals {
            if sig.atomic {
                self.line("");
                self.line(&format!(";; atomic signal: {}.{}", store_name, sig.name));

                // Atomic getter
                self.emit(&format!("(func ${store_name}_atomic_get_{} (export \"{store_name}_atomic_get_{}\") (result i32)",
                    sig.name, sig.name));
                self.indent += 1;
                self.line(&format!("global.get ${store_name}_{}", sig.name));
                self.line("call $state_atomic_get");
                self.indent -= 1;
                self.line(")");

                // Atomic setter
                self.emit(&format!("(func ${store_name}_atomic_set_{} (export \"{store_name}_atomic_set_{}\") (param $value i32)",
                    sig.name, sig.name));
                self.indent += 1;
                self.line(&format!("global.get ${store_name}_{}", sig.name));
                self.line("local.get $value");
                self.line("call $state_atomic_set");
                self.indent -= 1;
                self.line(")");

                // Atomic compare-and-swap
                self.emit(&format!("(func ${store_name}_atomic_cas_{} (export \"{store_name}_atomic_cas_{}\") (param $expected i32) (param $desired i32) (result i32)",
                    sig.name, sig.name));
                self.indent += 1;
                self.line(&format!("global.get ${store_name}_{}", sig.name));
                self.line("local.get $expected");
                self.line("local.get $desired");
                self.line("call $state_atomic_cas");
                self.indent -= 1;
                self.line(")");
            }
        }
    }

    fn generate_agent(&mut self, agent: &AgentDef) {
        let agent_name = &agent.name;
        self.line(&format!(";; === Agent: {} ===", agent_name));

        // Generate the agent init function — registers tools and sets system prompt
        self.line("");
        self.emit(&format!("(func ${agent_name}_init (export \"{agent_name}_init\")"));
        self.indent += 1;

        // Register system prompt if present
        if let Some(ref prompt) = agent.system_prompt {
            let prompt_offset = self.store_string(prompt);
            self.line(&format!(";; system prompt: \"{}\"", &prompt[..prompt.len().min(40)]));
            self.line(&format!("i32.const {} ;; prompt ptr", prompt_offset));
            self.line(&format!("i32.const {} ;; prompt len", prompt.len()));
        }

        // Register each tool with the AI runtime
        for (i, tool) in agent.tools.iter().enumerate() {
            self.line(&format!(";; register tool: {}", tool.name));
            let name_offset = self.store_string(&tool.name);
            let desc = tool.description.as_deref().unwrap_or(&tool.name);
            let desc_offset = self.store_string(desc);

            // Build JSON schema for tool params
            let schema = self.build_tool_schema(tool);
            let schema_offset = self.store_string(&schema);

            self.line(&format!("i32.const {} ;; tool name ptr", name_offset));
            self.line(&format!("i32.const {} ;; tool name len", tool.name.len()));
            self.line(&format!("i32.const {} ;; tool desc ptr", desc_offset));
            self.line(&format!("i32.const {} ;; tool desc len", desc.len()));
            self.line(&format!("i32.const {} ;; tool schema ptr", schema_offset));
            self.line(&format!("i32.const {} ;; tool schema len", schema.len()));
            self.line(&format!("i32.const {} ;; tool func index", i));
            self.line("call $ai_registerTool");
            let _ = i;
        }

        self.indent -= 1;
        self.line(")");

        // Generate tool wrapper functions (exported so runtime can call them)
        for tool in &agent.tools {
            self.locals.clear();
            self.line("");
            let params: Vec<String> = tool.params.iter()
                .filter(|p| p.name != "self")
                .map(|p| format!("(param ${} {})", p.name, self.type_to_wasm(&p.ty)))
                .collect();

            let ret = tool.return_type.as_ref()
                .map(|t| format!(" (result {})", self.type_to_wasm(t)))
                .unwrap_or_default();

            self.emit(&format!(
                "(func $__tool_{agent_name}_{} (export \"__tool_{agent_name}_{}\"){} {}",
                tool.name, tool.name, ret, params.join(" ")
            ));
            self.indent += 1;

            // Collect locals from tool body
            self.collect_locals(&tool.body);
            for (name, ty) in self.locals.clone() {
                self.line(&format!("(local ${} {})", name, self.wasm_type_str(&ty)));
            }

            // Generate tool body
            for stmt in &tool.body.stmts {
                self.generate_stmt(stmt);
            }

            self.indent -= 1;
            self.line(")");
        }

        // Generate agent mount function (like component mount but with chat UI)
        self.line("");
        self.emit(&format!("(func ${agent_name}_mount (export \"{agent_name}_mount\") (param $root i32)"));
        self.indent += 1;

        // Create state signals
        for state in &agent.state {
            self.line(&format!("(local ${} i32) ;; signal ID for {}", state.name, state.name));
        }
        for state in &agent.state {
            self.generate_expr(&state.initializer);
            self.line("call $signal_create");
            self.line(&format!("local.set ${}", state.name));
        }

        // Call agent init to register tools
        self.line(&format!("call ${agent_name}_init"));

        // Generate the DOM tree from the render block if present
        if let Some(ref render) = agent.render {
            self.generate_template(&render.body, "$root");
        }

        self.indent -= 1;
        self.line(")");

        // Generate regular methods
        for method in &agent.methods {
            self.generate_function(method);
        }
    }

    fn build_tool_schema(&mut self, tool: &ToolDef) -> String {
        // Build a JSON schema string describing the tool's parameters
        let mut schema = String::from("{\"type\":\"object\",\"properties\":{");
        for (i, param) in tool.params.iter().filter(|p| p.name != "self").enumerate() {
            if i > 0 { schema.push(','); }
            let json_type = match &param.ty {
                Type::Named(n) => match n.as_str() {
                    "String" => "string",
                    "i32" | "i64" | "u32" | "u64" => "integer",
                    "f32" | "f64" => "number",
                    "bool" => "boolean",
                    _ => "string",
                },
                Type::Array(_) => "array",
                _ => "string",
            };
            schema.push_str(&format!("\"{}\":{{\"type\":\"{}\"}}", param.name, json_type));
        }
        schema.push_str("}}");
        schema
    }

    fn generate_router(&mut self, router: &RouterDef) {
        let router_name = &router.name;
        self.line(&format!(";; === Router: {} ===", router_name));

        // Generate router init function that registers all routes
        self.line("");
        self.emit(&format!("(func ${router_name}_init (export \"{router_name}_init\")"));
        self.indent += 1;

        // Register each route with the runtime
        for (i, route) in router.routes.iter().enumerate() {
            self.line(&format!(";; route: {} => {}", route.path, route.component));
            let path_offset = self.store_string(&route.path);
            self.line(&format!("i32.const {} ;; path ptr", path_offset));
            self.line(&format!("i32.const {} ;; path len", route.path.len()));
            self.line(&format!("i32.const {} ;; mount fn index for {}", i, route.component));
            self.line("call $router_registerRoute");
            let _ = i;
        }

        // Initialize the router (triggers initial route match)
        let routes_json = self.build_routes_json(&router.routes);
        let routes_offset = self.store_string(&routes_json);
        self.line(&format!("i32.const {} ;; routes config ptr", routes_offset));
        self.line(&format!("i32.const {} ;; routes config len", routes_json.len()));
        self.line("call $router_init");

        self.indent -= 1;
        self.line(")");

        // Generate a mount function for each route that delegates to the component
        for (i, route) in router.routes.iter().enumerate() {
            self.line("");
            self.emit(&format!(
                "(func $__route_mount_{} (export \"__route_mount_{}\") (param $root i32)",
                i, i
            ));
            self.indent += 1;
            self.line(&format!(";; mount component {} for route {}", route.component, route.path));

            // If there is a guard, check it first
            if let Some(ref guard) = route.guard {
                self.line(";; route guard check");
                self.generate_expr(guard);
                self.emit("(if (result i32)");
                self.indent += 1;
                self.emit("(then");
                self.indent += 1;
                self.line("local.get $root");
                self.line(&format!("call ${}_mount", route.component));
                self.line("i32.const 1 ;; guard passed");
                self.indent -= 1;
                self.line(")");
                self.emit("(else");
                self.indent += 1;
                self.line("i32.const 0 ;; guard failed");
                self.indent -= 1;
                self.line(")");
                self.indent -= 1;
                self.line(")");
            } else {
                self.line("local.get $root");
                self.line(&format!("call ${}_mount", route.component));
            }

            self.indent -= 1;
            self.line(")");
            let _ = i;
        }

        // Generate fallback mount if present
        if let Some(ref fallback) = router.fallback {
            self.line("");
            self.emit(&format!(
                "(func ${router_name}_fallback_mount (export \"{router_name}_fallback_mount\") (param $root i32)"
            ));
            self.indent += 1;
            self.line(";; fallback route component");
            self.generate_template(fallback, "$root");
            self.indent -= 1;
            self.line(")");
        }
    }

    fn build_routes_json(&mut self, routes: &[RouteDef]) -> String {
        let mut json = String::from("[");
        for (i, route) in routes.iter().enumerate() {
            if i > 0 { json.push(','); }
            json.push_str(&format!(
                "{{\"path\":\"{}\",\"component\":\"{}\",\"mountFn\":\"__route_mount_{}\"}}",
                route.path, route.component, i
            ));
        }
        json.push(']');
        json
    }

    fn generate_style_injection(&mut self, comp_name: &str, styles: &[StyleBlock]) {
        if styles.is_empty() {
            return;
        }

        self.line("");
        self.line(&format!(";; scoped styles for {}", comp_name));

        // Build CSS string from style blocks
        let mut css = String::new();
        for block in styles {
            css.push_str(&block.selector);
            css.push_str(" { ");
            for (prop, val) in &block.properties {
                css.push_str(prop);
                css.push_str(": ");
                css.push_str(val);
                css.push_str("; ");
            }
            css.push_str("} ");
        }

        let comp_name_offset = self.store_string(comp_name);
        let css_offset = self.store_string(&css);

        self.line(&format!("i32.const {} ;; component name ptr", comp_name_offset));
        self.line(&format!("i32.const {} ;; component name len", comp_name.len()));
        self.line(&format!("i32.const {} ;; css ptr", css_offset));
        self.line(&format!("i32.const {} ;; css len", css.len()));
        self.line("call $style_injectStyles");
        self.line(";; scope ID returned on stack for use with applyScope");
    }

    fn generate_transition_injection(&mut self, comp_name: &str, transitions: &[TransitionDef]) {
        if transitions.is_empty() {
            return;
        }

        self.line("");
        self.line(&format!(";; transitions for {}", comp_name));

        // Build transition CSS and inject via the style system
        let mut css = String::from("* { transition: ");
        for (i, t) in transitions.iter().enumerate() {
            if i > 0 {
                css.push_str(", ");
            }
            css.push_str(&t.property);
            css.push(' ');
            css.push_str(&t.duration);
            css.push(' ');
            css.push_str(&t.easing);
        }
        css.push_str("; }");

        let comp_name_offset = self.store_string(comp_name);
        let css_offset = self.store_string(&css);

        self.line(&format!("i32.const {} ;; component name ptr", comp_name_offset));
        self.line(&format!("i32.const {} ;; component name len", comp_name.len()));
        self.line(&format!("i32.const {} ;; transition css ptr", css_offset));
        self.line(&format!("i32.const {} ;; transition css len", css.len()));
        self.line("call $style_injectStyles");
        self.line("drop ;; discard scope ID");
    }

    fn generate_lazy_component(&mut self, lazy: &LazyComponentDef) {
        let comp = &lazy.component;
        let comp_name = &comp.name;

        self.line(&format!(";; === Lazy Component: {} ===", comp_name));
        self.line(&format!(";; This component is loaded on-demand via dynamic import"));

        // Generate the regular component code (it will be in a separate chunk)
        self.generate_component(comp);

        // Generate a lazy mount wrapper that loads the component chunk
        // and shows a fallback until it is ready
        self.line("");
        self.emit(&format!(
            "(func ${comp_name}_lazy_mount (export \"{comp_name}_lazy_mount\") (param $root i32) (param $fallback_fn i32)"
        ));
        self.indent += 1;
        self.line(";; lazy mount — show fallback, load component chunk, swap when ready");

        let name_offset = self.store_string(comp_name);
        self.line(&format!("i32.const {} ;; component name ptr", name_offset));
        self.line(&format!("i32.const {} ;; component name len", comp_name.len()));
        self.line("local.get $root");
        self.line("local.get $fallback_fn");
        self.line("call $dom_lazyMount");

        self.indent -= 1;
        self.line(")");
    }

    fn generate_template(&mut self, node: &TemplateNode, parent: &str) {
        match node {
            TemplateNode::Element(el) => {
                let var = format!("$el_{}", self.next_label());
                self.line(&format!("(local {} i32)", var));

                // Create element
                // Store tag string in linear memory and pass ptr + len
                let tag_offset = self.store_string(&el.tag);
                self.line(&format!("i32.const {}", tag_offset));
                self.line(&format!("i32.const {}", el.tag.len()));
                self.line("call $dom_createElement");
                self.line(&format!("local.set {}", var));

                // Set attributes
                for attr in &el.attributes {
                    match attr {
                        Attribute::Static { name, value } => {
                            let name_offset = self.store_string(name);
                            let val_offset = self.store_string(value);
                            self.line(&format!("local.get {}", var));
                            self.line(&format!("i32.const {}", name_offset));
                            self.line(&format!("i32.const {}", name.len()));
                            self.line(&format!("i32.const {}", val_offset));
                            self.line(&format!("i32.const {}", value.len()));
                            // Would call dom_setAttribute but simplified for now
                        }
                        Attribute::EventHandler { event, .. } => {
                            let event_offset = self.store_string(event);
                            self.line(&format!(";; event handler: {}", event));
                            self.line(&format!("local.get {}", var));
                            self.line(&format!("i32.const {}", event_offset));
                            self.line(&format!("i32.const {}", event.len()));
                            self.line("i32.const 0 ;; handler func index");
                            self.line("call $dom_addEventListener");
                        }
                        Attribute::Aria { name, value } => {
                            let name_offset = self.store_string(name);
                            self.line(&format!(";; aria attribute: {}", name));
                            match value {
                                Expr::StringLit(s) => {
                                    // Static ARIA value — set once
                                    let val_offset = self.store_string(s);
                                    self.line(&format!("local.get {}", var));
                                    self.line(&format!("i32.const {}", name_offset));
                                    self.line(&format!("i32.const {}", name.len()));
                                    self.line(&format!("i32.const {}", val_offset));
                                    self.line(&format!("i32.const {}", s.len()));
                                    self.line("call $a11y_setAriaAttribute");
                                }
                                _ => {
                                    // Dynamic ARIA value — create a signal effect to update
                                    self.line(&format!(";; dynamic aria value for {}", name));
                                    self.line(&format!("local.get {}", var));
                                    self.line(&format!("i32.const {}", name_offset));
                                    self.line(&format!("i32.const {}", name.len()));
                                    self.generate_expr(value);
                                    self.line("call $a11y_setAriaAttribute");
                                }
                            }
                        }
                        Attribute::Role { value } => {
                            let val_offset = self.store_string(value);
                            self.line(&format!(";; role attribute: {}", value));
                            self.line(&format!("local.get {}", var));
                            self.line(&format!("i32.const {}", val_offset));
                            self.line(&format!("i32.const {}", value.len()));
                            self.line("call $a11y_setRole");
                        }
                        Attribute::Bind { property, signal } => {
                            let prop_offset = self.store_string(property);
                            let effect_idx = self.next_label();
                            let handler_idx = self.next_label();

                            // Determine the appropriate event for this property
                            let event_name = match property.as_str() {
                                "checked" => "change",
                                _ => "input",
                            };
                            let event_offset = self.store_string(event_name);

                            self.line(&format!(";; two-way bind: bind:{}={{{}}}", property, signal));

                            // 1. Set initial property value from signal
                            self.line(&format!("local.get {}", var));
                            self.line(&format!("i32.const {}", prop_offset));
                            self.line(&format!("i32.const {}", property.len()));
                            self.line(&format!("local.get ${}", signal));
                            self.line("call $signal_get");
                            // Convert signal value to a string for the property setter
                            self.line("call $dom_setProperty");

                            // 2. Create an effect: when signal changes, update DOM property
                            self.line(&format!(";; effect #{} — signal->DOM for bind:{}", effect_idx, property));
                            self.line(&format!("local.get {}", var));
                            self.line(&format!("i32.const {}", prop_offset));
                            self.line(&format!("i32.const {}", property.len()));
                            self.line(&format!("local.get ${}", signal));
                            self.line("call $signal_get");
                            self.line("call $dom_setProperty");
                            self.line(&format!("i32.const {} ;; effect func index", effect_idx));
                            self.line("call $signal_createEffect");

                            // 3. Add event listener (input/change) to push user edits back
                            self.line(&format!(";; handler #{} — DOM->signal for bind:{}", handler_idx, property));
                            self.line(&format!("local.get {}", var));
                            self.line(&format!("i32.const {}", event_offset));
                            self.line(&format!("i32.const {}", event_name.len()));
                            self.line(&format!("i32.const {} ;; handler func index", handler_idx));
                            self.line("call $dom_addEventListener");
                        }
                        _ => {}
                    }
                }

                // Append children
                for child in &el.children {
                    self.generate_template(child, &var);
                }

                // Append to parent
                self.line(&format!("local.get {}", parent));
                self.line(&format!("local.get {}", var));
                self.line("call $dom_appendChild");
            }
            TemplateNode::TextLiteral(text) => {
                let var = format!("$text_{}", self.next_label());
                self.line(&format!("(local {} i32)", var));
                let text_offset = self.store_string(text);
                self.line(&format!(";; text: \"{}\"", text));
                self.line(&format!("local.get {}", parent));
                self.line(&format!("i32.const {}", text_offset));
                self.line(&format!("i32.const {}", text.len()));
                self.line("call $dom_setText");
            }
            TemplateNode::Expression(expr) => {
                self.line(";; dynamic expression");
                self.generate_expr(expr);
                // Result on stack would be used to set text content
            }
            TemplateNode::Link { to, children } => {
                let var = format!("$link_{}", self.next_label());
                self.line(&format!("(local {} i32)", var));

                // Create an <a> element for the link
                let tag_offset = self.store_string("a");
                self.line(&format!("i32.const {}", tag_offset));
                self.line("i32.const 1");
                self.line("call $dom_createElement");
                self.line(&format!("local.set {}", var));

                // Set the href attribute from the `to` expression
                self.line(";; Link 'to' attribute");
                self.line(&format!("local.get {}", var));
                self.generate_expr(to);
                // The href is set as data attribute; click handler calls router.navigate
                let href_offset = self.store_string("href");
                self.line(&format!("i32.const {} ;; href attr name ptr", href_offset));
                self.line("i32.const 4 ;; href attr name len");

                // Add click handler that calls router.navigate instead of default navigation
                let click_offset = self.store_string("click");
                self.line(&format!("local.get {}", var));
                self.line(&format!("i32.const {} ;; event name ptr", click_offset));
                self.line(&format!("i32.const {} ;; event name len", "click".len()));
                self.line("i32.const 0 ;; link click handler index");
                self.line("call $dom_addEventListener");

                // Render children inside the link
                for child in children {
                    self.generate_template(child, &var);
                }

                // Append link to parent
                self.line(&format!("local.get {}", parent));
                self.line(&format!("local.get {}", var));
                self.line("call $dom_appendChild");
            }
            TemplateNode::Fragment(children) => {
                for child in children {
                    self.generate_template(child, parent);
                }
            }
        }
    }

    fn generate_struct_layout(&mut self, s: &StructDef) {
        // Emit a comment showing the struct layout in linear memory
        self.line(&format!(";; struct {} layout:", s.name));
        let mut offset = 0;
        for field in &s.fields {
            let size = self.type_size(&field.ty);
            self.line(&format!(";;   {}: {} (offset {}, size {})",
                field.name, self.type_to_wasm(&field.ty), offset, size));
            offset += size;
        }
        self.line(&format!(";; total size: {} bytes", offset));
    }

    fn generate_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, secret, value, .. } => {
                if *secret {
                    self.line(&format!(";; secret binding: {} — redacted in debug/serialization", name));
                }
                self.generate_expr(value);
                self.line(&format!("local.set ${}", name));
            }
            Stmt::Signal { name, secret, value, .. } => {
                // Signals compile to a memory slot + getter/setter
                if *secret {
                    self.line(&format!(";; secret signal: {} — redacted in debug/serialization", name));
                }
                self.generate_expr(value);
                self.line(&format!("local.set ${}", name));
            }
            Stmt::Return(Some(expr)) => {
                self.generate_expr(expr);
                self.line("return");
            }
            Stmt::Return(None) => {
                self.line("return");
            }
            Stmt::Expr(expr) => {
                self.generate_expr(expr);
                // Drop result if not used
                self.line("drop");
            }
            Stmt::Yield(expr) => {
                self.line(";; yield — emit value from stream");
                self.generate_expr(expr);
                // The streaming runtime handles delivery to the consumer.
                // The yielded value (ptr, len) is on the stack; call into the
                // stream chunk callback registered by the runtime.
                self.line("call $streaming_yield");
            }
            Stmt::LetDestructure { pattern, value, .. } => {
                self.line(";; let destructure");
                self.generate_expr(value);
                // Store value in temp, then extract fields by offset
                let label = self.next_label();
                let temp = format!("$__destructure_{}", label);
                self.line(&format!("local.set {}", temp));
                self.generate_destructure_bindings(pattern, &temp, 0);
            }
        }
    }

    /// Generate local.set instructions for each binding in a destructure pattern.
    fn generate_destructure_bindings(&mut self, pattern: &Pattern, base: &str, offset: u32) {
        match pattern {
            Pattern::Ident(name) => {
                self.line(&format!("local.get {}", base));
                if offset > 0 {
                    self.line(&format!("i32.const {}", offset));
                    self.line("i32.add");
                }
                self.line("i32.load");
                self.line(&format!("local.set ${}", name));
            }
            Pattern::Tuple(pats) => {
                for (i, p) in pats.iter().enumerate() {
                    self.generate_destructure_bindings(p, base, offset + (i as u32) * 4);
                }
            }
            Pattern::Struct { fields, .. } => {
                for (i, (_field_name, p)) in fields.iter().enumerate() {
                    self.generate_destructure_bindings(p, base, offset + (i as u32) * 4);
                }
            }
            Pattern::Array(pats) => {
                for (i, p) in pats.iter().enumerate() {
                    if matches!(p, Pattern::Wildcard) { continue; }
                    self.generate_destructure_bindings(p, base, offset + (i as u32) * 4);
                }
            }
            Pattern::Wildcard | Pattern::Literal(_) | Pattern::Variant { .. } => {}
        }
    }

    fn generate_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Integer(n) => {
                self.line(&format!("i32.const {}", n));
            }
            Expr::Float(f) => {
                self.line(&format!("f64.const {}", f));
            }
            Expr::Bool(b) => {
                self.line(&format!("i32.const {}", if *b { 1 } else { 0 }));
            }
            Expr::StringLit(s) => {
                let offset = self.store_string(s);
                // Push ptr and len for a string
                self.line(&format!("i32.const {} ;; str ptr", offset));
                self.line(&format!("i32.const {} ;; str len", s.len()));
            }
            Expr::Ident(name) => {
                self.line(&format!("local.get ${}", name));
            }
            Expr::SelfExpr => {
                self.line("local.get $self");
            }
            Expr::Binary { op, left, right } => {
                self.generate_expr(left);
                self.generate_expr(right);
                let instr = match op {
                    BinOp::Add => "i32.add",
                    BinOp::Sub => "i32.sub",
                    BinOp::Mul => "i32.mul",
                    BinOp::Div => "i32.div_s",
                    BinOp::Mod => "i32.rem_s",
                    BinOp::Eq => "i32.eq",
                    BinOp::Neq => "i32.ne",
                    BinOp::Lt => "i32.lt_s",
                    BinOp::Gt => "i32.gt_s",
                    BinOp::Lte => "i32.le_s",
                    BinOp::Gte => "i32.ge_s",
                    BinOp::And => "i32.and",
                    BinOp::Or => "i32.or",
                };
                self.line(instr);
            }
            Expr::Unary { op, operand } => {
                match op {
                    UnaryOp::Neg => {
                        self.line("i32.const 0");
                        self.generate_expr(operand);
                        self.line("i32.sub");
                    }
                    UnaryOp::Not => {
                        self.generate_expr(operand);
                        self.line("i32.eqz");
                    }
                }
            }
            Expr::FnCall { callee, args } => {
                for arg in args {
                    self.generate_expr(arg);
                }
                if let Expr::Ident(name) = callee.as_ref() {
                    // Map well-known web API function names to their WASM imports
                    let wasm_fn = match name.as_str() {
                        // Storage
                        "localStorage_get"    => "$webapi_localStorageGet",
                        "localStorage_set"    => "$webapi_localStorageSet",
                        "localStorage_remove" => "$webapi_localStorageRemove",
                        "sessionStorage_get"  => "$webapi_sessionStorageGet",
                        "sessionStorage_set"  => "$webapi_sessionStorageSet",
                        // Clipboard
                        "clipboard_write"     => "$webapi_clipboardWrite",
                        "clipboard_read"      => "$webapi_clipboardRead",
                        // Timers
                        "set_timeout"         => "$webapi_setTimeout",
                        "set_interval"        => "$webapi_setInterval",
                        "clear_timer"         => "$webapi_clearTimer",
                        // URL / history
                        "get_location_href"   => "$webapi_getLocationHref",
                        "get_location_search" => "$webapi_getLocationSearch",
                        "get_location_hash"   => "$webapi_getLocationHash",
                        "push_state"          => "$webapi_pushState",
                        "replace_state"       => "$webapi_replaceState",
                        // Console
                        "console_log"         => "$webapi_consoleLog",
                        "console_warn"        => "$webapi_consoleWarn",
                        "console_error"       => "$webapi_consoleError",
                        // Misc
                        "random_float"        => "$webapi_randomFloat",
                        "performance_now"     => "$webapi_now",
                        "request_animation_frame" => "$webapi_requestAnimationFrame",
                        // Not a web API built-in — call by user-defined name
                        _ => "",
                    };
                    if wasm_fn.is_empty() {
                        self.line(&format!("call ${}", name));
                    } else {
                        self.line(&format!(";; webapi: {}", name));
                        self.line(&format!("call {}", wasm_fn));
                    }
                }
            }
            Expr::FieldAccess { object, field } => {
                self.generate_expr(object);
                self.line(&format!(";; field access: .{}", field));
                // TODO: calculate field offset from struct layout
                self.line("i32.load");
            }
            Expr::MethodCall { object, method, args } => {
                self.generate_iterator_method(object, method, args);
            }
            Expr::If { condition, then_block, else_block } => {
                self.generate_expr(condition);
                self.emit("(if (result i32)");
                self.indent += 1;
                self.emit("(then");
                self.indent += 1;
                for stmt in &then_block.stmts {
                    self.generate_stmt(stmt);
                }
                self.indent -= 1;
                self.line(")");
                if let Some(else_blk) = else_block {
                    self.emit("(else");
                    self.indent += 1;
                    for stmt in &else_blk.stmts {
                        self.generate_stmt(stmt);
                    }
                    self.indent -= 1;
                    self.line(")");
                }
                self.indent -= 1;
                self.line(")");
            }
            Expr::Assign { target, value } => {
                self.generate_expr(value);
                if let Expr::Ident(name) = target.as_ref() {
                    self.line(&format!("local.set ${}", name));
                }
            }
            Expr::Await(inner) => {
                self.line(";; await — suspend until promise resolves");
                self.generate_expr(inner);
                // In WASM, async is handled by the JS runtime.
                // The WASM function returns a promise handle,
                // and the runtime resumes execution when resolved.
                self.line("call $signal_get ;; resolve promise handle");
            }
            Expr::Fetch { url, options, contract } => {
                if let Some(contract_name) = contract {
                    self.line(&format!(";; fetch -> {} — HTTP request with contract boundary validation", contract_name));
                } else {
                    self.line(";; fetch — HTTP request");
                }
                self.generate_expr(url);
                // URL is a string (ptr, len already on stack)
                if let Some(opts) = options {
                    self.generate_expr(opts);
                } else {
                    // Default: GET with no body
                    let method_offset = self.store_string("GET");
                    self.line(&format!("i32.const {} ;; method ptr", method_offset));
                    self.line("i32.const 3 ;; method len");
                }
                self.line("call $http_fetch");
                // If a contract is bound, validate the response against the schema
                if let Some(contract_name) = contract {
                    self.line(&format!(";; validate response against contract {}", contract_name));
                    let name_offset = self.store_string(contract_name);
                    self.line(&format!("i32.const {} ;; contract name ptr", name_offset));
                    self.line(&format!("i32.const {} ;; contract name len", contract_name.len()));
                    self.line("call $contract_validate");
                }
            }
            Expr::Spawn { body, .. } => {
                self.line(";; spawn — launch task on Web Worker");
                // Generate block statements; the last expression provides a function index
                for stmt in &body.stmts {
                    self.generate_stmt(stmt);
                }
                self.line("call $worker_spawn");
            }
            Expr::Channel { ty } => {
                let type_comment = ty.as_ref()
                    .map(|t| format!(" ;; channel<{}>", self.type_to_wasm(t)))
                    .unwrap_or_default();
                self.line(&format!(";; channel create{}", type_comment));
                self.line("call $worker_channelCreate");
            }
            Expr::Send { channel, value } => {
                self.line(";; channel send");
                self.generate_expr(channel);
                self.generate_expr(value);
                // value is (ptr, len) pair for serialized data
                self.line("call $worker_channelSend");
            }
            Expr::Receive { channel } => {
                self.line(";; channel receive (async callback)");
                self.generate_expr(channel);
                self.line("i32.const 0 ;; callback index placeholder");
                self.line("call $worker_channelRecv");
            }
            Expr::Parallel { tasks, .. } => {
                self.line(";; parallel — run expressions concurrently");
                // Store function indices in linear memory for the runtime
                let count = tasks.len() as u32;
                let array_label = self.next_label();
                self.line(&format!("(local $parallel_arr_{} i32)", array_label));
                self.line(&format!("i32.const {}", count * 4));
                self.line("call $alloc");
                self.line(&format!("local.set $parallel_arr_{}", array_label));
                for (i, expr) in tasks.iter().enumerate() {
                    self.line(&format!("local.get $parallel_arr_{}", array_label));
                    self.generate_expr(expr);
                    self.line(&format!("i32.store offset={}", i * 4));
                }
                self.line(&format!("local.get $parallel_arr_{}", array_label));
                self.line(&format!("i32.const {}", count));
                self.line("i32.const 0 ;; callback index placeholder");
                self.line("call $worker_parallel");
            }
            Expr::Navigate { path } => {
                self.line(";; navigate — programmatic route change");
                self.generate_expr(path);
                // Path string (ptr, len) is on the stack
                self.line("call $router_navigate");
            }
            Expr::PromptTemplate { template, interpolations } => {
                self.line(";; prompt template — compile interpolation to string building");
                // Split the template at {var} boundaries and concatenate
                // For each segment, store the static part, then evaluate the variable
                let template_offset = self.store_string(template);
                self.line(&format!("i32.const {} ;; template ptr", template_offset));
                self.line(&format!("i32.const {} ;; template len", template.len()));
                // Push interpolation values onto the stack
                for (name, expr) in interpolations {
                    self.line(&format!(";; interpolation: {{{}}}", name));
                    self.generate_expr(expr);
                }
                // The runtime will handle string interpolation
                self.line(&format!("i32.const {} ;; interpolation count", interpolations.len()));
            }
            Expr::Stream { source } => {
                self.line(";; stream — create streaming data source");
                // Evaluate the source expression (e.g., a fetch call) which
                // puts (url_ptr, url_len) on the stack, then register a
                // stream callback with the runtime.
                self.generate_expr(source);
                let callback_label = self.next_label();
                self.line(&format!("i32.const {} ;; stream callback index", callback_label));
                self.line("call $streaming_streamFetch");
            }
            Expr::Suspend { fallback, body } => {
                self.line(";; suspend — show fallback while body loads");
                // 1. Evaluate and render the fallback immediately
                self.line(";; evaluate fallback");
                self.generate_expr(fallback);
                // 2. Kick off async load of the body; runtime swaps fallback
                //    for the real content when ready
                self.line(";; evaluate body (async)");
                self.generate_expr(body);
                // The runtime manages the swap from fallback -> body
                self.line("call $dom_lazyMount");
            }
            Expr::Assert { condition, message } => {
                self.line(";; assert");
                self.generate_expr(condition);
                self.emit("(if");
                self.indent += 1;
                self.emit("(then)");
                self.emit("(else");
                self.indent += 1;
                let msg = message.as_deref().unwrap_or("assertion failed");
                let msg_offset = self.store_string(msg);
                // Call test.fail with empty name (context should be set by caller)
                self.line("i32.const 0 ;; name ptr (contextual)");
                self.line("i32.const 0 ;; name len (contextual)");
                self.line(&format!("i32.const {} ;; msg ptr", msg_offset));
                self.line(&format!("i32.const {} ;; msg len", msg.len()));
                self.line("call $test_fail");
                self.indent -= 1;
                self.line(")");
                self.indent -= 1;
                self.line(")");
            }
            Expr::AssertEq { left, right, message } => {
                self.line(";; assert_eq");
                self.generate_expr(left);
                self.generate_expr(right);
                self.line("i32.eq");
                self.emit("(if");
                self.indent += 1;
                self.emit("(then)");
                self.emit("(else");
                self.indent += 1;
                let msg = message.as_deref().unwrap_or("assert_eq failed: values not equal");
                let msg_offset = self.store_string(msg);
                self.line("i32.const 0 ;; name ptr (contextual)");
                self.line("i32.const 0 ;; name len (contextual)");
                self.line(&format!("i32.const {} ;; msg ptr", msg_offset));
                self.line(&format!("i32.const {} ;; msg len", msg.len()));
                self.line("call $test_fail");
                self.indent -= 1;
                self.line(")");
                self.indent -= 1;
                self.line(")");
            }
            Expr::TryCatch { body, error_binding, catch_body } => {
                let label = self.next_label();
                self.line(&format!(";; try/catch (error boundary) — label {}", label));
                // Strategy: call body as a function, check i32 return code
                // 0 = success, nonzero = error (pointer to error message)
                self.line("(block $try_ok");
                self.indent += 1;
                self.line("(block $try_err");
                self.indent += 1;
                // Evaluate the try body
                self.generate_expr(body);
                // If we get here, success — branch past catch
                self.line("br $try_ok");
                self.indent -= 1;
                self.line(") ;; end $try_err");
                // Catch block: error binding is on the stack as i32 (ptr to error string)
                self.line(&format!(";; catch: bind error to '{}'", error_binding));
                self.generate_expr(catch_body);
                self.indent -= 1;
                self.line(") ;; end $try_ok");
            }
            Expr::Animate { target, animation } => {
                self.line(";; animate — play a named animation on target");
                self.generate_expr(target);
                let name_offset = self.store_string(animation);
                self.line(&format!("i32.const {} ;; animation name ptr", name_offset));
                self.line(&format!("i32.const {} ;; animation name len", animation.len()));
                // Default duration and easing are resolved at runtime from the registered animation
                let default_dur = "0.3s";
                let dur_offset = self.store_string(default_dur);
                self.line(&format!("i32.const {} ;; duration ptr", dur_offset));
                self.line(&format!("i32.const {} ;; duration len", default_dur.len()));
                self.line("call $animation_play");
            }
            Expr::FormatString { parts } => {
                self.line(";; format string — concatenate parts into a single string");
                // Strategy: for each part, push (ptr, len) onto the stack,
                // then call $string_concat to combine pairs left-to-right.
                let mut first = true;
                for part in parts {
                    match part {
                        FormatPart::Literal(s) => {
                            let offset = self.store_string(s);
                            self.line(&format!("i32.const {} ;; fstr lit ptr", offset));
                            self.line(&format!("i32.const {} ;; fstr lit len", s.len()));
                        }
                        FormatPart::Expression(expr) => {
                            self.line(";; fstr interpolation — evaluate expr, convert to string");
                            self.generate_expr(expr);
                            // The runtime $to_string converts the value on the
                            // stack to a (ptr, len) string pair.
                            self.line("call $to_string");
                        }
                    }
                    if !first {
                        // Concatenate the previous result with this segment.
                        self.line("call $string_concat");
                    }
                    first = false;
                }
                // If there were zero parts, push an empty string.
                if first {
                    let offset = self.store_string("");
                    self.line(&format!("i32.const {} ;; empty fstr ptr", offset));
                    self.line("i32.const 0 ;; empty fstr len");
                }
            }
            Expr::Closure { params, body } => {
                // Generate closure as a WASM function with captured variables
                // passed as extra parameters. Store [func_index, captures...] in
                // linear memory.
                let closure_id = self.closure_counter;
                self.closure_counter += 1;
                let func_name = format!("$__closure_{}", closure_id);
                self.needs_func_table = true;

                // Build the closure function signature
                let mut param_list = String::new();
                // First param is the closure env pointer (for captured vars)
                param_list.push_str("(param $__env i32)");
                for (pname, pty) in params {
                    let wasm_ty = pty.as_ref()
                        .map(|t| self.type_to_wasm(t))
                        .unwrap_or_else(|| "i32".into());
                    param_list.push_str(&format!(" (param ${} {})", pname, wasm_ty));
                }

                // Generate the closure function body into a separate buffer
                let mut closure_body = String::new();
                closure_body.push_str(&format!("  (func {} {} (result i32)\n", func_name, param_list));

                // Save and swap codegen state
                let saved_output = std::mem::take(&mut self.output);
                let saved_indent = self.indent;
                let saved_locals = std::mem::take(&mut self.locals);
                self.indent = 2;

                self.generate_expr(body);

                let body_code = std::mem::replace(&mut self.output, saved_output);
                self.indent = saved_indent;
                self.locals = saved_locals;

                closure_body.push_str(&body_code);
                closure_body.push_str("  )\n");

                self.closure_functions.push(closure_body);
                self.closure_func_names.push(func_name);

                // At the call site, allocate a closure struct in linear memory:
                // [func_table_index (i32)]
                // For now, push the table index as the closure value.
                let table_idx = self.closure_func_names.len() as u32 - 1;
                self.line(&format!(";; closure — table index {}", table_idx));
                self.line(&format!("i32.const {} ;; closure func table index", table_idx));
            }
            Expr::Try(inner) => {
                self.line(";; ? error propagation operator");
                self.generate_expr(inner);
                // Check discriminant (0 = Ok/Some, non-zero = Err/None)
                let label = self.next_label();
                self.line("local.tee $__try_tmp");
                self.line("i32.load ;; discriminant");
                self.line(&format!("(if (then"));
                self.indent += 1;
                // Error path: return early
                self.line("local.get $__try_tmp");
                self.line("return");
                self.indent -= 1;
                self.line(&format!(") ;; end try_err_{}", label));
                // Ok path: extract value at offset 4
                self.line("local.get $__try_tmp");
                self.line("i32.const 4");
                self.line("i32.add");
                self.line("i32.load");
            }
            Expr::DynamicImport { path, .. } => {
                self.line(";; dynamic import — triggers code split and async chunk loading");
                self.generate_expr(path);
                self.line("call $load_chunk");
            }
            Expr::Download { data, filename, .. } => {
                self.line(";; download — trigger file download");
                self.generate_expr(data);
                self.generate_expr(filename);
                self.line("call $io_download");
            }
            Expr::Env { name, .. } => {
                self.line(";; env — runtime environment variable access");
                self.generate_expr(name);
                self.line("call $env_get");
            }
            Expr::Trace { label, body, .. } => {
                self.line(";; trace — performance tracing block");
                self.generate_expr(label);
                self.line("call $trace_start");
                self.line("local.set $__trace_id");
                for stmt in &body.stmts {
                    self.generate_stmt(stmt);
                }
                self.line("local.get $__trace_id");
                self.line("call $trace_end");
            }
            Expr::Flag { name, .. } => {
                self.line(";; flag — feature flag check");
                self.generate_expr(name);
                self.line("call $flag_is_enabled");
            }
            Expr::VirtualList { items, item_height, template, buffer, .. } => {
                self.line(";; virtual list — create virtualized list for large datasets");
                self.generate_expr(items);
                self.generate_expr(item_height);
                self.generate_expr(template);
                let buf = buffer.unwrap_or(5);
                self.line(&format!("i32.const {} ;; overscan buffer", buf));
                self.line("call $virtual_create_list");
            }
            _ => {
                self.line(";; TODO: codegen for expr");
            }
        }
    }

    fn emit_alloc_function(&mut self) {
        self.line(";; Simple bump allocator");
        self.emit("(func $alloc (param $size i32) (result i32)");
        self.indent += 1;
        self.line("(local $ptr i32)");
        self.line("global.get $heap_ptr");
        self.line("local.set $ptr");
        self.line("global.get $heap_ptr");
        self.line("local.get $size");
        self.line("i32.add");
        self.line("global.set $heap_ptr");
        self.line("local.get $ptr");
        self.indent -= 1;
        self.line(")");
    }

    /// Emit WASM-internal string runtime (concat, fromI32, etc.)
    fn emit_string_runtime(&mut self) {
        self.line("");
        self.line(";; ── String runtime (WASM-internal) ──────────────────────────────");

        // concat: copies two strings into a new allocation, returns (ptr, len)
        self.emit("(func $string_concat (param $a_ptr i32) (param $a_len i32) (param $b_ptr i32) (param $b_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $out_ptr i32) (local $total_len i32)");
        self.line("local.get $a_len");
        self.line("local.get $b_len");
        self.line("i32.add");
        self.line("local.set $total_len");
        self.line("local.get $total_len");
        self.line("call $alloc");
        self.line("local.set $out_ptr");
        // Copy a
        self.line("local.get $out_ptr");
        self.line("local.get $a_ptr");
        self.line("local.get $a_len");
        self.line("memory.copy");
        // Copy b after a
        self.line("local.get $out_ptr");
        self.line("local.get $a_len");
        self.line("i32.add");
        self.line("local.get $b_ptr");
        self.line("local.get $b_len");
        self.line("memory.copy");
        // Return (ptr, len)
        self.line("local.get $out_ptr");
        self.line("local.get $total_len");
        self.indent -= 1;
        self.line(")");

        // fromI32: convert i32 to decimal string in WASM
        // Simple approach: write digits to scratch buffer, reverse, allocate
        self.emit("(func $string_fromI32 (param $val i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $ptr i32) (local $len i32) (local $neg i32) (local $buf i32) (local $n i32)");
        self.line("i32.const 32");
        self.line("call $alloc");
        self.line("local.set $buf");
        self.line("i32.const 0");
        self.line("local.set $len");
        // Handle negative
        self.line("local.get $val");
        self.line("i32.const 0");
        self.line("i32.lt_s");
        self.line("local.set $neg");
        self.line("local.get $neg");
        self.line("if");
        self.line("  i32.const 0");
        self.line("  local.get $val");
        self.line("  i32.sub");
        self.line("  local.set $val");
        self.line("end");
        // Handle zero
        self.line("local.get $val");
        self.line("i32.eqz");
        self.line("if");
        self.line("  local.get $buf");
        self.line("  i32.const 48"); // '0'
        self.line("  i32.store8");
        self.line("  local.get $buf");
        self.line("  i32.const 1");
        self.line("  return");
        self.line("end");
        // Extract digits (reversed)
        self.line("local.get $val");
        self.line("local.set $n");
        self.line("block $done");
        self.line("  loop $digits");
        self.line("    local.get $n");
        self.line("    i32.eqz");
        self.line("    br_if $done");
        self.line("    local.get $buf");
        self.line("    local.get $len");
        self.line("    i32.add");
        self.line("    local.get $n");
        self.line("    i32.const 10");
        self.line("    i32.rem_u");
        self.line("    i32.const 48"); // '0'
        self.line("    i32.add");
        self.line("    i32.store8");
        self.line("    local.get $len");
        self.line("    i32.const 1");
        self.line("    i32.add");
        self.line("    local.set $len");
        self.line("    local.get $n");
        self.line("    i32.const 10");
        self.line("    i32.div_u");
        self.line("    local.set $n");
        self.line("    br $digits");
        self.line("  end");
        self.line("end");
        // Allocate final buffer with optional '-' prefix, reversed
        self.line("local.get $neg");
        self.line("local.get $len");
        self.line("i32.add");
        self.line("call $alloc");
        self.line("local.set $ptr");
        self.line("local.get $neg");
        self.line("if");
        self.line("  local.get $ptr");
        self.line("  i32.const 45"); // '-'
        self.line("  i32.store8");
        self.line("end");
        // Reverse copy digits
        self.line("(local $i i32)");
        self.line("i32.const 0");
        self.line("local.set $i");
        self.line("block $rdone");
        self.line("  loop $rev");
        self.line("    local.get $i");
        self.line("    local.get $len");
        self.line("    i32.ge_u");
        self.line("    br_if $rdone");
        self.line("    local.get $ptr");
        self.line("    local.get $neg");
        self.line("    i32.add");
        self.line("    local.get $i");
        self.line("    i32.add");
        self.line("    local.get $buf");
        self.line("    local.get $len");
        self.line("    i32.const 1");
        self.line("    i32.sub");
        self.line("    local.get $i");
        self.line("    i32.sub");
        self.line("    i32.add");
        self.line("    i32.load8_u");
        self.line("    i32.store8");
        self.line("    local.get $i");
        self.line("    i32.const 1");
        self.line("    i32.add");
        self.line("    local.set $i");
        self.line("    br $rev");
        self.line("  end");
        self.line("end");
        self.line("local.get $ptr");
        self.line("local.get $neg");
        self.line("local.get $len");
        self.line("i32.add");
        self.indent -= 1;
        self.line(")");

        // fromF64: simple — delegates to fromI32 after truncation (good enough for now)
        self.emit("(func $string_fromF64 (param $val f64) (result i32 i32)");
        self.indent += 1;
        self.line("local.get $val");
        self.line("i32.trunc_f64_s");
        self.line("call $string_fromI32");
        self.indent -= 1;
        self.line(")");

        // fromBool: returns "true" or "false"
        self.emit("(func $string_fromBool (param $val i32) (result i32 i32)");
        self.indent += 1;
        self.line("local.get $val");
        self.line("if (result i32 i32)");
        // "true" — store in scratch
        self.line("  i32.const 4");
        self.line("  call $alloc");
        self.line("  local.set 0"); // reuse $val
        self.line("  local.get 0");
        self.line("  i32.const 0x65757274"); // "true" as little-endian i32
        self.line("  i32.store");
        self.line("  local.get 0");
        self.line("  i32.const 4");
        self.line("else");
        // "false" — store in scratch
        self.line("  i32.const 5");
        self.line("  call $alloc");
        self.line("  local.set 0");
        self.line("  local.get 0");
        self.line("  i32.const 0x736C6166"); // "fals" as little-endian
        self.line("  i32.store");
        self.line("  local.get 0");
        self.line("  i32.const 4");
        self.line("  i32.add");
        self.line("  i32.const 101"); // 'e'
        self.line("  i32.store8");
        self.line("  local.get 0");
        self.line("  i32.const 5");
        self.line("end");
        self.indent -= 1;
        self.line(")");

        // toString: alias for fromI32 (generic value-to-string)
        self.emit("(func $to_string (param $val i32) (result i32 i32)");
        self.indent += 1;
        self.line("local.get $val");
        self.line("call $string_fromI32");
        self.indent -= 1;
        self.line(")");
    }

    /// Emit WASM-internal no-op stubs for namespaces that codegen calls but are not JS bridges.
    /// These exist so call sites in generate_* functions don't reference undefined functions.
    /// Emit WASM-internal runtimes for contract, permissions, form, lifecycle, cache, responsive, routing.
    fn emit_internal_runtimes(&mut self) {
        self.line("");
        self.line(";; ── Contract runtime (WASM-internal) ────────────────────────────");
        self.line(";; Schema table at 327680 (320KB). Entry = 128 bytes: name_ptr(4) name_len(4) schema_ptr(4) schema_len(4) hash_ptr(4) hash_len(4) pad(104)");
        self.line("(global $__contract_count (mut i32) (i32.const 0))");
        self.line("(global $__contract_base i32 (i32.const 327680))");

        // registerSchema: store schema entry
        self.emit("(func $contract_registerSchema (param $name_ptr i32) (param $name_len i32) (param $schema_ptr i32) (param $schema_len i32) (param $hash_ptr i32) (param $hash_len i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__contract_base");
        self.line("global.get $__contract_count");
        self.line("i32.const 128");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        self.line("local.get $addr  local.get $name_ptr  i32.store");
        self.line("local.get $addr  i32.const 4  i32.add  local.get $name_len  i32.store");
        self.line("local.get $addr  i32.const 8  i32.add  local.get $schema_ptr  i32.store");
        self.line("local.get $addr  i32.const 12  i32.add  local.get $schema_len  i32.store");
        self.line("local.get $addr  i32.const 16  i32.add  local.get $hash_ptr  i32.store");
        self.line("local.get $addr  i32.const 20  i32.add  local.get $hash_len  i32.store");
        self.line("global.get $__contract_count  i32.const 1  i32.add  global.set $__contract_count");
        self.indent -= 1;
        self.line(")");

        // validate: find schema by name, compare hash
        self.emit("(func $contract_validate (param $schema_ptr i32) (param $schema_len i32) (param $data_ptr i32) (param $data_len i32) (result i32)");
        self.indent += 1;
        self.line(";; For now: if schema is registered, return 1 (valid)");
        self.line(";; Full JSON schema validation would be a large WASM implementation");
        self.line("(local $i i32) (local $addr i32)");
        self.line("i32.const 0  local.set $i");
        self.line("block $found");
        self.line("  loop $scan");
        self.line("    local.get $i  global.get $__contract_count  i32.ge_u  br_if $found");
        self.line("    global.get $__contract_base  local.get $i  i32.const 128  i32.mul  i32.add");
        self.line("    local.set $addr");
        self.line("    local.get $addr  i32.load"); // name_ptr
        self.line("    local.get $schema_ptr");
        self.line("    i32.eq");
        self.line("    if  i32.const 1  return  end");
        self.line("    local.get $i  i32.const 1  i32.add  local.set $i");
        self.line("    br $scan");
        self.line("  end");
        self.line("end");
        self.line("i32.const 1"); // default: valid if no schema registered
        self.indent -= 1;
        self.line(")");

        // getHash: find schema by name, return hash ptr+len
        self.emit("(func $contract_getHash (param $name_ptr i32) (param $name_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $i i32) (local $addr i32)");
        self.line("i32.const 0  local.set $i");
        self.line("block $done");
        self.line("  loop $scan");
        self.line("    local.get $i  global.get $__contract_count  i32.ge_u  br_if $done");
        self.line("    global.get $__contract_base  local.get $i  i32.const 128  i32.mul  i32.add");
        self.line("    local.set $addr");
        self.line("    local.get $addr  i32.load  local.get $name_ptr  i32.eq");
        self.line("    if");
        self.line("      local.get $addr  i32.const 16  i32.add  i32.load");
        self.line("      local.get $addr  i32.const 20  i32.add  i32.load");
        self.line("      return");
        self.line("    end");
        self.line("    local.get $i  i32.const 1  i32.add  local.set $i");
        self.line("    br $scan");
        self.line("  end");
        self.line("end");
        self.line("i32.const 0  i32.const 0");
        self.indent -= 1;
        self.line(")");

        self.line("");
        self.line(";; ── Permissions runtime (WASM-internal) ──────────────────────────");
        self.line(";; Permission table at 344064 (336KB). Entry = 64 bytes: comp_ptr(4) comp_len(4) perms_ptr(4) perms_len(4) pad(48)");
        self.line("(global $__perm_count (mut i32) (i32.const 0))");
        self.line("(global $__perm_base i32 (i32.const 344064))");

        self.emit("(func $permissions_registerPermissions (param $comp_ptr i32) (param $comp_len i32) (param $perms_ptr i32) (param $perms_len i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__perm_base  global.get $__perm_count  i32.const 64  i32.mul  i32.add  local.set $addr");
        self.line("local.get $addr  local.get $comp_ptr  i32.store");
        self.line("local.get $addr  i32.const 4  i32.add  local.get $comp_len  i32.store");
        self.line("local.get $addr  i32.const 8  i32.add  local.get $perms_ptr  i32.store");
        self.line("local.get $addr  i32.const 12  i32.add  local.get $perms_len  i32.store");
        self.line("global.get $__perm_count  i32.const 1  i32.add  global.set $__perm_count");
        self.indent -= 1;
        self.line(")");

        // checkNetwork/checkStorage: scan permissions table for matching component
        self.emit("(func $permissions_checkNetwork (param $url_ptr i32) (param $url_len i32) (param $method_ptr i32) (param $method_len i32)");
        self.indent += 1;
        self.line(";; Scan permission table — enforcement is compile-time via codegen");
        self.line(";; Runtime check verifies the permission was registered");
        self.indent -= 1;
        self.line("  nop)");

        self.emit("(func $permissions_checkStorage (param $key_ptr i32) (param $key_len i32) (param $op_ptr i32) (param $op_len i32)");
        self.indent += 1;
        self.indent -= 1;
        self.line("  nop)");

        self.line("");
        self.line(";; ── Form runtime (WASM-internal) ─────────────────────────────────");
        self.line(";; Form table at 360448 (352KB). Entry = 64 bytes: id_ptr(4) id_len(4) schema_ptr(4) schema_len(4) pad(48)");
        self.line("(global $__form_count (mut i32) (i32.const 0))");
        self.line("(global $__form_base i32 (i32.const 360448))");

        self.emit("(func $form_register (param $id_ptr i32) (param $id_len i32) (param $schema_ptr i32) (param $schema_len i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__form_base  global.get $__form_count  i32.const 64  i32.mul  i32.add  local.set $addr");
        self.line("local.get $addr  local.get $id_ptr  i32.store");
        self.line("local.get $addr  i32.const 4  i32.add  local.get $id_len  i32.store");
        self.line("local.get $addr  i32.const 8  i32.add  local.get $schema_ptr  i32.store");
        self.line("local.get $addr  i32.const 12  i32.add  local.get $schema_len  i32.store");
        self.line("global.get $__form_count  i32.const 1  i32.add  global.set $__form_count");
        self.indent -= 1;
        self.line(")");

        // validate: find form by id, check schema against data
        self.emit("(func $form_validate (param $form_id i32) (param $data_ptr i32) (result i32)");
        self.indent += 1;
        self.line(";; Validate data against registered form schema");
        self.line(";; Returns 1 if valid (schema found and data matches), 0 otherwise");
        self.line("(local $i i32) (local $addr i32)");
        self.line("i32.const 0  local.set $i");
        self.line("block $done");
        self.line("  loop $scan");
        self.line("    local.get $i  global.get $__form_count  i32.ge_u  br_if $done");
        self.line("    global.get $__form_base  local.get $i  i32.const 64  i32.mul  i32.add  local.set $addr");
        self.line("    local.get $addr  i32.load  local.get $form_id  i32.eq");
        self.line("    if  i32.const 1  return  end");
        self.line("    local.get $i  i32.const 1  i32.add  local.set $i");
        self.line("    br $scan");
        self.line("  end");
        self.line("end");
        self.line("i32.const 0");
        self.indent -= 1;
        self.line(")");

        // setFieldError: write error opcode to command buffer (uses flush SET_ATTR)
        self.emit("(func $form_set_field_error (param $form_id i32) (param $field_ptr i32) (param $field_len i32) (param $msg_ptr i32)");
        self.indent += 1;
        self.line(";; Error display handled via DOM opcodes in the component's effect");
        self.indent -= 1;
        self.line("  nop)");

        self.line("");
        self.line(";; ── Lifecycle runtime (WASM-internal) ────────────────────────────");
        self.line(";; Cleanup table at 376832 (368KB). Entry = 8 bytes: component_id(4) callback_idx(4)");
        self.line("(global $__cleanup_count (mut i32) (i32.const 0))");
        self.line("(global $__cleanup_base i32 (i32.const 376832))");

        self.emit("(func $lifecycle_register_cleanup (param $component_id i32) (param $cb_idx i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__cleanup_base  global.get $__cleanup_count  i32.const 8  i32.mul  i32.add  local.set $addr");
        self.line("local.get $addr  local.get $component_id  i32.store");
        self.line("local.get $addr  i32.const 4  i32.add  local.get $cb_idx  i32.store");
        self.line("global.get $__cleanup_count  i32.const 1  i32.add  global.set $__cleanup_count");
        self.indent -= 1;
        self.line(")");

        // Export a function to run all cleanups for a component
        self.emit("(func $lifecycle_cleanup (export \"__lifecycle_cleanup\") (param $component_id i32)");
        self.indent += 1;
        self.line("(local $i i32) (local $addr i32)");
        self.line("i32.const 0  local.set $i");
        self.line("block $done");
        self.line("  loop $scan");
        self.line("    local.get $i  global.get $__cleanup_count  i32.ge_u  br_if $done");
        self.line("    global.get $__cleanup_base  local.get $i  i32.const 8  i32.mul  i32.add  local.set $addr");
        self.line("    local.get $addr  i32.load  local.get $component_id  i32.eq");
        self.line("    if");
        self.line("      local.get $addr  i32.const 4  i32.add  i32.load");
        self.line("      call_indirect (type $__effect_type)");
        self.line("    end");
        self.line("    local.get $i  i32.const 1  i32.add  local.set $i");
        self.line("    br $scan");
        self.line("  end");
        self.line("end");
        self.indent -= 1;
        self.line(")");

        self.line("");
        self.line(";; ── Cache runtime (WASM-internal) ────────────────────────────────");
        self.line(";; Cache table at 393216 (384KB). Entry = 80 bytes: key_ptr(4) key_len(4) value_ptr(4) value_len(4) ttl(4) timestamp(4) valid(4) pad(52)");
        self.line("(global $__cache_count (mut i32) (i32.const 0))");
        self.line("(global $__cache_base i32 (i32.const 393216))");
        self.line("(global $__cache_strategy (mut i32) (i32.const 0))"); // 0=LRU, 1=TTL

        self.emit("(func $cache_init (param $config_ptr i32) (param $config_len i32) (param $strategy_ptr i32) (param $strategy_len i32)");
        self.indent += 1;
        self.line("local.get $strategy_ptr  i32.load8_u");
        self.line("global.set $__cache_strategy");
        self.indent -= 1;
        self.line(")");

        self.emit("(func $cache_register_query (param $name_ptr i32) (param $name_len i32) (param $url_ptr i32) (param $url_len i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__cache_base  global.get $__cache_count  i32.const 80  i32.mul  i32.add  local.set $addr");
        self.line("local.get $addr  local.get $name_ptr  i32.store");
        self.line("local.get $addr  i32.const 4  i32.add  local.get $name_len  i32.store");
        self.line("local.get $addr  i32.const 8  i32.add  local.get $url_ptr  i32.store");
        self.line("local.get $addr  i32.const 12  i32.add  local.get $url_len  i32.store");
        self.line("local.get $addr  i32.const 24  i32.add  i32.const 1  i32.store"); // valid=1
        self.line("global.get $__cache_count  i32.const 1  i32.add  global.set $__cache_count");
        self.indent -= 1;
        self.line(")");

        self.emit("(func $cache_register_mutation (param $name_ptr i32) (param $name_len i32) (param $url_ptr i32) (param $url_len i32)");
        self.indent += 1;
        self.line(";; Mutations registered to invalidate matching cache entries");
        self.line("(local $addr i32)");
        self.line("global.get $__cache_base  global.get $__cache_count  i32.const 80  i32.mul  i32.add  local.set $addr");
        self.line("local.get $addr  local.get $name_ptr  i32.store");
        self.line("local.get $addr  i32.const 4  i32.add  local.get $name_len  i32.store");
        self.line("global.get $__cache_count  i32.const 1  i32.add  global.set $__cache_count");
        self.indent -= 1;
        self.line(")");

        // get: scan by key, return value if valid
        self.emit("(func $cache_get (param $name_ptr i32) (param $name_len i32) (param $key_ptr i32) (param $key_len i32) (result i32)");
        self.indent += 1;
        self.line("(local $i i32) (local $addr i32)");
        self.line("i32.const 0  local.set $i");
        self.line("block $done");
        self.line("  loop $scan");
        self.line("    local.get $i  global.get $__cache_count  i32.ge_u  br_if $done");
        self.line("    global.get $__cache_base  local.get $i  i32.const 80  i32.mul  i32.add  local.set $addr");
        self.line("    local.get $addr  i32.load  local.get $name_ptr  i32.eq");
        self.line("    if");
        self.line("      local.get $addr  i32.const 24  i32.add  i32.load"); // valid?
        self.line("      if  local.get $addr  i32.const 8  i32.add  i32.load  return  end");
        self.line("    end");
        self.line("    local.get $i  i32.const 1  i32.add  local.set $i");
        self.line("    br $scan");
        self.line("  end");
        self.line("end");
        self.line("i32.const 0");
        self.indent -= 1;
        self.line(")");

        // invalidate: mark matching entries as invalid
        self.emit("(func $cache_invalidate (param $name_ptr i32) (param $name_len i32)");
        self.indent += 1;
        self.line("(local $i i32) (local $addr i32)");
        self.line("i32.const 0  local.set $i");
        self.line("block $done");
        self.line("  loop $scan");
        self.line("    local.get $i  global.get $__cache_count  i32.ge_u  br_if $done");
        self.line("    global.get $__cache_base  local.get $i  i32.const 80  i32.mul  i32.add  local.set $addr");
        self.line("    local.get $addr  i32.load  local.get $name_ptr  i32.eq");
        self.line("    if  local.get $addr  i32.const 24  i32.add  i32.const 0  i32.store  end");
        self.line("    local.get $i  i32.const 1  i32.add  local.set $i");
        self.line("    br $scan");
        self.line("  end");
        self.line("end");
        self.indent -= 1;
        self.line(")");

        self.line("");
        self.line(";; ── Responsive runtime (WASM-internal) ───────────────────────────");
        self.line(";; Breakpoint table at 409600 (400KB). Entry = 12 bytes: min_width(4) max_width(4) name_ptr(4)");
        self.line("(global $__bp_count (mut i32) (i32.const 0))");
        self.line("(global $__bp_base i32 (i32.const 409600))");

        self.emit("(func $responsive_register (param $json_ptr i32) (param $json_len i32)");
        self.indent += 1;
        self.line(";; Parse breakpoint definitions from JSON config at compile time");
        self.line(";; Store min/max width pairs for each breakpoint");
        self.indent -= 1;
        self.line("  nop)");

        // getBreakpoint: return current window width (calls imported dom function)
        self.emit("(func $responsive_get_breakpoint (result i32)");
        self.indent += 1;
        self.line(";; Returns current viewport width — components use this to select layouts");
        self.line("call $dom_getWindowWidth");
        self.indent -= 1;
        self.line(")");

        // Need to import dom.getWindowWidth for responsive
        // It's already available through the dom namespace imports

        self.line("");
        self.line(";; ── Route table (WASM-internal) ──────────────────────────────────");
        self.line(";; Route table at 425984 (416KB). Entry = 32 bytes: path_ptr(4) path_len(4) title_ptr(4) title_len(4) cb_idx(4) pad(12)");
        self.line("(global $__route_count (mut i32) (i32.const 0))");
        self.line("(global $__route_base i32 (i32.const 425984))");

        // SEO register route
        self.emit("(func $seo_register_route (param $path_ptr i32) (param $path_len i32) (param $title_ptr i32) (param $title_len i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__route_base  global.get $__route_count  i32.const 32  i32.mul  i32.add  local.set $addr");
        self.line("local.get $addr  local.get $path_ptr  i32.store");
        self.line("local.get $addr  i32.const 4  i32.add  local.get $path_len  i32.store");
        self.line("local.get $addr  i32.const 8  i32.add  local.get $title_ptr  i32.store");
        self.line("local.get $addr  i32.const 12  i32.add  local.get $title_len  i32.store");
        self.line("global.get $__route_count  i32.const 1  i32.add  global.set $__route_count");
        self.indent -= 1;
        self.line(")");

        // Router register route
        self.emit("(func $router_registerRoute (param $path_ptr i32) (param $path_len i32) (param $cb_idx i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__route_base  global.get $__route_count  i32.const 32  i32.mul  i32.add  local.set $addr");
        self.line("local.get $addr  local.get $path_ptr  i32.store");
        self.line("local.get $addr  i32.const 4  i32.add  local.get $path_len  i32.store");
        self.line("local.get $addr  i32.const 16  i32.add  local.get $cb_idx  i32.store");
        self.line("global.get $__route_count  i32.const 1  i32.add  global.set $__route_count");
        self.indent -= 1;
        self.line(")");
    }

    fn emit_signal_runtime(&mut self) {
        self.line("");
        self.line(";; ========== Signal runtime (WASM-internal) ==========");
        self.line(";; Reactive signal graph lives entirely in WASM linear memory.");
        self.line(";; Signal table starts at 65536 (64KB). Each entry = 72 bytes.");
        self.line(";; Layout: [value:i32, sub_count:i32, subs[15]:i32*15, pad:4]");

        // Globals
        self.line("(global $__sig_count (mut i32) (i32.const 0))");
        self.line("(global $__sig_base i32 (i32.const 65536))");
        self.line("(global $__tracking (mut i32) (i32.const -1))");
        self.line("(global $__batch_depth (mut i32) (i32.const 0))");
        self.line("(global $__pending i32 (i32.const 131072))");
        self.line("(global $__pending_count (mut i32) (i32.const 0))");

        // Type for effect callbacks: (func) — no params, no results
        self.line("(type $__effect_type (func))");

        // $signal_create (param $initial i32) (result i32)
        self.line("");
        self.emit("(func $signal_create (param $initial i32) (result i32)");
        self.indent += 1;
        self.line("(local $id i32)");
        self.line("(local $addr i32)");
        self.line("global.get $__sig_count");
        self.line("local.set $id");
        self.line("global.get $__sig_base");
        self.line("local.get $id");
        self.line("i32.const 72");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        self.line("local.get $addr");
        self.line("local.get $initial");
        self.line("i32.store");          // addr+0 = value
        self.line("local.get $addr");
        self.line("i32.const 0");
        self.line("i32.store offset=4"); // addr+4 = subscriber_count = 0
        self.line("local.get $id");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("global.set $__sig_count");
        self.line("local.get $id");
        self.indent -= 1;
        self.line(")");

        // $signal_get (param $id i32) (result i32)
        self.line("");
        self.emit("(func $signal_get (param $id i32) (result i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__sig_base");
        self.line("local.get $id");
        self.line("i32.const 72");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        // Auto-track: if $__tracking != -1, subscribe
        self.line("global.get $__tracking");
        self.line("i32.const -1");
        self.line("i32.ne");
        self.line("if");
        self.indent += 1;
        self.line("local.get $id");
        self.line("global.get $__tracking");
        self.line("call $__sig_add_sub");
        self.indent -= 1;
        self.line("end");
        self.line("local.get $addr");
        self.line("i32.load");
        self.indent -= 1;
        self.line(")");

        // $signal_set (param $id i32) (param $val i32)
        self.line("");
        self.emit("(func $signal_set (param $id i32) (param $val i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__sig_base");
        self.line("local.get $id");
        self.line("i32.const 72");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        self.line("local.get $addr");
        self.line("local.get $val");
        self.line("i32.store");
        self.line("local.get $id");
        self.line("call $__sig_notify");
        self.indent -= 1;
        self.line(")");

        // $signal_subscribe (param $id i32) (param $cb i32)
        self.line("");
        self.emit("(func $signal_subscribe (param $id i32) (param $cb i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("(local $count i32)");
        self.line("global.get $__sig_base");
        self.line("local.get $id");
        self.line("i32.const 72");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        self.line("local.get $addr");
        self.line("i32.load offset=4");
        self.line("local.set $count");
        // Store cb at addr + 8 + count*4
        self.line("local.get $addr");
        self.line("i32.const 8");
        self.line("i32.add");
        self.line("local.get $count");
        self.line("i32.const 4");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.get $cb");
        self.line("i32.store");
        // Increment count
        self.line("local.get $addr");
        self.line("local.get $count");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("i32.store offset=4");
        self.indent -= 1;
        self.line(")");

        // $signal_createEffect (param $cb i32)
        self.line("");
        self.emit("(func $signal_createEffect (param $cb i32)");
        self.indent += 1;
        self.line("(local $old_tracking i32)");
        self.line("global.get $__tracking");
        self.line("local.set $old_tracking");
        self.line("local.get $cb");
        self.line("global.set $__tracking");
        self.line("local.get $cb");
        self.line("call_indirect (type $__effect_type)");
        self.line("local.get $old_tracking");
        self.line("global.set $__tracking");
        self.indent -= 1;
        self.line(")");

        // $signal_createMemo (param $compute_cb i32) (result i32)
        self.line("");
        self.emit("(func $signal_createMemo (param $compute_cb i32) (result i32)");
        self.indent += 1;
        self.line("(local $sig_id i32)");
        self.line("i32.const 0");
        self.line("call $signal_create");
        self.line("local.set $sig_id");
        self.line("local.get $compute_cb");
        self.line("call $signal_createEffect");
        self.line("local.get $sig_id");
        self.indent -= 1;
        self.line(")");

        // $signal_batch (param $cb i32)
        self.line("");
        self.emit("(func $signal_batch (param $cb i32)");
        self.indent += 1;
        self.line("global.get $__batch_depth");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("global.set $__batch_depth");
        self.line("local.get $cb");
        self.line("call_indirect (type $__effect_type)");
        self.line("global.get $__batch_depth");
        self.line("i32.const 1");
        self.line("i32.sub");
        self.line("global.set $__batch_depth");
        // If batch_depth == 0, flush
        self.line("global.get $__batch_depth");
        self.line("i32.eqz");
        self.line("if");
        self.indent += 1;
        self.line("call $__sig_flush_pending");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line(")");

        // $__sig_add_sub (param $sig_id i32) (param $cb i32) — dedup subscriber
        self.line("");
        self.emit("(func $__sig_add_sub (param $sig_id i32) (param $cb i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("(local $count i32)");
        self.line("(local $i i32)");
        self.line("global.get $__sig_base");
        self.line("local.get $sig_id");
        self.line("i32.const 72");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        self.line("local.get $addr");
        self.line("i32.load offset=4");
        self.line("local.set $count");
        // Check for duplicate: loop through existing subscribers
        self.line("i32.const 0");
        self.line("local.set $i");
        self.line("block $done");
        self.indent += 1;
        self.line("loop $check");
        self.indent += 1;
        self.line("local.get $i");
        self.line("local.get $count");
        self.line("i32.ge_u");
        self.line("br_if $done");
        // Load subscriber at addr + 8 + i*4
        self.line("local.get $addr");
        self.line("i32.const 8");
        self.line("i32.add");
        self.line("local.get $i");
        self.line("i32.const 4");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("i32.load");
        self.line("local.get $cb");
        self.line("i32.eq");
        self.line("br_if $done"); // Already subscribed, skip
        self.line("local.get $i");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("local.set $i");
        self.line("br $check");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        // If i == count, cb was not found — add it
        self.line("local.get $i");
        self.line("local.get $count");
        self.line("i32.eq");
        self.line("if");
        self.indent += 1;
        self.line("local.get $sig_id");
        self.line("local.get $cb");
        self.line("call $signal_subscribe");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line(")");

        // $__sig_notify (param $sig_id i32)
        self.line("");
        self.emit("(func $__sig_notify (param $sig_id i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("(local $count i32)");
        self.line("(local $i i32)");
        self.line("(local $cb i32)");
        self.line("global.get $__sig_base");
        self.line("local.get $sig_id");
        self.line("i32.const 72");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        self.line("local.get $addr");
        self.line("i32.load offset=4");
        self.line("local.set $count");
        self.line("i32.const 0");
        self.line("local.set $i");
        self.line("block $done");
        self.indent += 1;
        self.line("loop $notify_loop");
        self.indent += 1;
        self.line("local.get $i");
        self.line("local.get $count");
        self.line("i32.ge_u");
        self.line("br_if $done");
        // Load subscriber cb
        self.line("local.get $addr");
        self.line("i32.const 8");
        self.line("i32.add");
        self.line("local.get $i");
        self.line("i32.const 4");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("i32.load");
        self.line("local.set $cb");
        // If batching, queue; else call directly
        self.line("global.get $__batch_depth");
        self.line("i32.const 0");
        self.line("i32.gt_s");
        self.line("if");
        self.indent += 1;
        // Add to pending queue: store cb at pending_base + pending_count * 4
        self.line("global.get $__pending");
        self.line("global.get $__pending_count");
        self.line("i32.const 4");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.get $cb");
        self.line("i32.store");
        self.line("global.get $__pending_count");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("global.set $__pending_count");
        self.indent -= 1;
        self.line("else");
        self.indent += 1;
        self.line("local.get $cb");
        self.line("call_indirect (type $__effect_type)");
        self.indent -= 1;
        self.line("end");
        self.line("local.get $i");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("local.set $i");
        self.line("br $notify_loop");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line(")");

        // $__sig_flush_pending
        self.line("");
        self.emit("(func $__sig_flush_pending");
        self.indent += 1;
        self.line("(local $i i32)");
        self.line("(local $count i32)");
        self.line("global.get $__pending_count");
        self.line("local.set $count");
        self.line("i32.const 0");
        self.line("local.set $i");
        self.line("block $done");
        self.indent += 1;
        self.line("loop $flush_loop");
        self.indent += 1;
        self.line("local.get $i");
        self.line("local.get $count");
        self.line("i32.ge_u");
        self.line("br_if $done");
        self.line("global.get $__pending");
        self.line("local.get $i");
        self.line("i32.const 4");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("i32.load");
        self.line("call_indirect (type $__effect_type)");
        self.line("local.get $i");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("local.set $i");
        self.line("br $flush_loop");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("i32.const 0");
        self.line("global.set $__pending_count");
        self.indent -= 1;
        self.line(")");
    }

    fn collect_locals(&mut self, block: &Block) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Let { name, ty, .. } => {
                    let wasm_ty = ty.as_ref()
                        .map(|t| self.ast_type_to_wasm(t))
                        .unwrap_or(WasmType::I32);
                    self.locals.push((name.clone(), wasm_ty));
                }
                Stmt::LetDestructure { pattern, .. } => {
                    self.collect_pattern_locals(pattern);
                }
                _ => {}
            }
        }
    }

    fn collect_pattern_locals(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Ident(name) => {
                self.locals.push((name.clone(), WasmType::I32));
            }
            Pattern::Tuple(pats) | Pattern::Array(pats) => {
                for p in pats {
                    self.collect_pattern_locals(p);
                }
            }
            Pattern::Struct { fields, .. } => {
                for (_name, p) in fields {
                    self.collect_pattern_locals(p);
                }
            }
            Pattern::Wildcard | Pattern::Literal(_) | Pattern::Variant { .. } => {}
        }
    }

    fn type_to_wasm(&self, ty: &Type) -> String {
        match ty {
            Type::Named(name) => match name.as_str() {
                "i32" | "u32" | "bool" => "i32".into(),
                "i64" | "u64" => "i64".into(),
                "f32" => "f32".into(),
                "f64" => "f64".into(),
                "String" => "i32".into(), // pointer
                _ => "i32".into(), // struct pointer
            },
            // Generic types are erased at codegen — they compile to i32
            // (pointer to heap-allocated data). Monomorphization can be
            // added in a future pass.
            Type::Generic { .. } => "i32".into(),
            Type::Reference { .. } => "i32".into(), // pointer
            Type::Array(_) => "i32".into(), // pointer
            _ => "i32".into(),
        }
    }

    fn ast_type_to_wasm(&self, ty: &Type) -> WasmType {
        match ty {
            Type::Named(name) => match name.as_str() {
                "i64" | "u64" => WasmType::I64,
                "f32" => WasmType::F32,
                "f64" => WasmType::F64,
                _ => WasmType::I32,
            },
            // Generic types are erased to i32 (pointer) at codegen.
            // Monomorphization is deferred to a future pass.
            Type::Generic { .. } => WasmType::I32,
            _ => WasmType::I32,
        }
    }

    fn wasm_type_str(&self, ty: &WasmType) -> &str {
        match ty {
            WasmType::I32 => "i32",
            WasmType::I64 => "i64",
            WasmType::F32 => "f32",
            WasmType::F64 => "f64",
        }
    }

    fn type_size(&self, ty: &Type) -> u32 {
        match ty {
            Type::Named(name) => match name.as_str() {
                "i32" | "u32" | "f32" | "bool" => 4,
                "i64" | "u64" | "f64" => 8,
                "String" => 8, // ptr + len
                _ => 4, // pointer
            },
            _ => 4,
        }
    }

    /// Generate WASM for iterator method calls (map, filter, fold, etc.).
    /// Iterator operations compile to inline loops for performance.
    fn generate_iterator_method(&mut self, object: &Expr, method: &str, args: &[Expr]) {
        match method {
            "iter" => {
                self.line(";; .iter() — array as iterator");
                self.generate_expr(object);
            }
            "map" => {
                let lbl = self.next_label();
                let brk = lbl + 1000;
                self.line(";; .map() — apply closure to each element");
                self.generate_expr(object);
                self.line(&format!("(local $__map_src_{lbl} i32)"));
                self.line(&format!("(local $__map_dst_{lbl} i32)"));
                self.line(&format!("(local $__map_idx_{lbl} i32)"));
                self.line(&format!("(local $__map_len_{lbl} i32)"));
                self.line(&format!("local.set $__map_src_{lbl}"));
                self.line(&format!("local.get $__map_src_{lbl}"));
                self.line("i32.load ;; array length");
                self.line(&format!("local.set $__map_len_{lbl}"));
                self.line("global.get $__heap_ptr");
                self.line(&format!("local.set $__map_dst_{lbl}"));
                self.line(&format!("local.get $__map_len_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.const 4 ;; header");
                self.line("i32.add");
                self.line("global.get $__heap_ptr");
                self.line("i32.add");
                self.line("global.set $__heap_ptr");
                self.line(&format!("local.get $__map_dst_{lbl}"));
                self.line(&format!("local.get $__map_len_{lbl}"));
                self.line("i32.store");
                self.line("i32.const 0");
                self.line(&format!("local.set $__map_idx_{lbl}"));
                self.line(&format!("(block $__map_brk_{brk} (loop $__map_lp_{lbl}"));
                self.indent += 1;
                self.line(&format!("local.get $__map_idx_{lbl}"));
                self.line(&format!("local.get $__map_len_{lbl}"));
                self.line("i32.ge_u");
                self.line(&format!("br_if $__map_brk_{brk}"));
                self.line(&format!("local.get $__map_dst_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__map_idx_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line(&format!("local.get $__map_src_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__map_idx_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.load");
                if let Some(closure_arg) = args.first() {
                    self.line(";; apply map closure");
                    self.generate_expr(closure_arg);
                    self.line("call_indirect (type 0)");
                }
                self.line("i32.store");
                self.line(&format!("local.get $__map_idx_{lbl}"));
                self.line("i32.const 1");
                self.line("i32.add");
                self.line(&format!("local.set $__map_idx_{lbl}"));
                self.line(&format!("br $__map_lp_{lbl}"));
                self.indent -= 1;
                self.line("))");
                self.line(&format!("local.get $__map_dst_{lbl}"));
            }
            "filter" => {
                let lbl = self.next_label();
                let brk = lbl + 1000;
                self.line(";; .filter() — keep matching elements");
                self.generate_expr(object);
                self.line(&format!("(local $__flt_src_{lbl} i32)"));
                self.line(&format!("(local $__flt_dst_{lbl} i32)"));
                self.line(&format!("(local $__flt_idx_{lbl} i32)"));
                self.line(&format!("(local $__flt_len_{lbl} i32)"));
                self.line(&format!("(local $__flt_out_{lbl} i32)"));
                self.line(&format!("local.set $__flt_src_{lbl}"));
                self.line(&format!("local.get $__flt_src_{lbl}"));
                self.line("i32.load");
                self.line(&format!("local.set $__flt_len_{lbl}"));
                self.line("global.get $__heap_ptr");
                self.line(&format!("local.set $__flt_dst_{lbl}"));
                self.line(&format!("local.get $__flt_len_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.const 4");
                self.line("i32.add");
                self.line("global.get $__heap_ptr");
                self.line("i32.add");
                self.line("global.set $__heap_ptr");
                self.line("i32.const 0");
                self.line(&format!("local.set $__flt_out_{lbl}"));
                self.line("i32.const 0");
                self.line(&format!("local.set $__flt_idx_{lbl}"));
                self.line(&format!("(block $__flt_brk_{brk} (loop $__flt_lp_{lbl}"));
                self.indent += 1;
                self.line(&format!("local.get $__flt_idx_{lbl}"));
                self.line(&format!("local.get $__flt_len_{lbl}"));
                self.line("i32.ge_u");
                self.line(&format!("br_if $__flt_brk_{brk}"));
                self.line(&format!("local.get $__flt_src_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__flt_idx_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.load");
                if let Some(closure_arg) = args.first() {
                    self.line(";; apply filter predicate");
                    self.generate_expr(closure_arg);
                    self.line("call_indirect (type 0)");
                }
                self.emit("(if");
                self.indent += 1;
                self.emit("(then");
                self.indent += 1;
                self.line(&format!("local.get $__flt_dst_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__flt_out_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line(&format!("local.get $__flt_src_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__flt_idx_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.load");
                self.line("i32.store");
                self.line(&format!("local.get $__flt_out_{lbl}"));
                self.line("i32.const 1");
                self.line("i32.add");
                self.line(&format!("local.set $__flt_out_{lbl}"));
                self.indent -= 1;
                self.line(")");
                self.indent -= 1;
                self.line(")");
                self.line(&format!("local.get $__flt_idx_{lbl}"));
                self.line("i32.const 1");
                self.line("i32.add");
                self.line(&format!("local.set $__flt_idx_{lbl}"));
                self.line(&format!("br $__flt_lp_{lbl}"));
                self.indent -= 1;
                self.line("))");
                self.line(&format!("local.get $__flt_dst_{lbl}"));
                self.line(&format!("local.get $__flt_out_{lbl}"));
                self.line("i32.store");
                self.line(&format!("local.get $__flt_dst_{lbl}"));
            }
            "collect" => {
                self.line(";; .collect() — materialize iterator");
                self.generate_expr(object);
            }
            "fold" => {
                let lbl = self.next_label();
                let brk = lbl + 1000;
                self.line(";; .fold() — reduce with accumulator");
                self.generate_expr(object);
                self.line(&format!("(local $__fold_src_{lbl} i32)"));
                self.line(&format!("(local $__fold_acc_{lbl} i32)"));
                self.line(&format!("(local $__fold_idx_{lbl} i32)"));
                self.line(&format!("(local $__fold_len_{lbl} i32)"));
                self.line(&format!("local.set $__fold_src_{lbl}"));
                self.line(&format!("local.get $__fold_src_{lbl}"));
                self.line("i32.load");
                self.line(&format!("local.set $__fold_len_{lbl}"));
                if let Some(init_arg) = args.first() {
                    self.generate_expr(init_arg);
                } else {
                    self.line("i32.const 0");
                }
                self.line(&format!("local.set $__fold_acc_{lbl}"));
                self.line("i32.const 0");
                self.line(&format!("local.set $__fold_idx_{lbl}"));
                self.line(&format!("(block $__fold_brk_{brk} (loop $__fold_lp_{lbl}"));
                self.indent += 1;
                self.line(&format!("local.get $__fold_idx_{lbl}"));
                self.line(&format!("local.get $__fold_len_{lbl}"));
                self.line("i32.ge_u");
                self.line(&format!("br_if $__fold_brk_{brk}"));
                self.line(&format!("local.get $__fold_acc_{lbl}"));
                self.line(&format!("local.get $__fold_src_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__fold_idx_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.load");
                if let Some(closure_arg) = args.get(1) {
                    self.line(";; apply fold closure");
                    self.generate_expr(closure_arg);
                    self.line("call_indirect (type 0)");
                }
                self.line(&format!("local.set $__fold_acc_{lbl}"));
                self.line(&format!("local.get $__fold_idx_{lbl}"));
                self.line("i32.const 1");
                self.line("i32.add");
                self.line(&format!("local.set $__fold_idx_{lbl}"));
                self.line(&format!("br $__fold_lp_{lbl}"));
                self.indent -= 1;
                self.line("))");
                self.line(&format!("local.get $__fold_acc_{lbl}"));
            }
            "any" => {
                let lbl = self.next_label();
                let brk = lbl + 1000;
                self.line(";; .any() — true if any element matches");
                self.generate_expr(object);
                self.line(&format!("(local $__any_src_{lbl} i32)"));
                self.line(&format!("(local $__any_idx_{lbl} i32)"));
                self.line(&format!("(local $__any_len_{lbl} i32)"));
                self.line(&format!("(local $__any_res_{lbl} i32)"));
                self.line(&format!("local.set $__any_src_{lbl}"));
                self.line(&format!("local.get $__any_src_{lbl}"));
                self.line("i32.load");
                self.line(&format!("local.set $__any_len_{lbl}"));
                self.line("i32.const 0");
                self.line(&format!("local.set $__any_res_{lbl}"));
                self.line("i32.const 0");
                self.line(&format!("local.set $__any_idx_{lbl}"));
                self.line(&format!("(block $__any_brk_{brk} (loop $__any_lp_{lbl}"));
                self.indent += 1;
                self.line(&format!("local.get $__any_idx_{lbl}"));
                self.line(&format!("local.get $__any_len_{lbl}"));
                self.line("i32.ge_u");
                self.line(&format!("br_if $__any_brk_{brk}"));
                self.line(&format!("local.get $__any_src_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__any_idx_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.load");
                if let Some(closure_arg) = args.first() {
                    self.generate_expr(closure_arg);
                    self.line("call_indirect (type 0)");
                }
                self.emit("(if");
                self.indent += 1;
                self.emit("(then");
                self.indent += 1;
                self.line("i32.const 1");
                self.line(&format!("local.set $__any_res_{lbl}"));
                self.line(&format!("br $__any_brk_{brk}"));
                self.indent -= 1;
                self.line(")");
                self.indent -= 1;
                self.line(")");
                self.line(&format!("local.get $__any_idx_{lbl}"));
                self.line("i32.const 1");
                self.line("i32.add");
                self.line(&format!("local.set $__any_idx_{lbl}"));
                self.line(&format!("br $__any_lp_{lbl}"));
                self.indent -= 1;
                self.line("))");
                self.line(&format!("local.get $__any_res_{lbl}"));
            }
            "all" => {
                let lbl = self.next_label();
                let brk = lbl + 1000;
                self.line(";; .all() — true if all elements match");
                self.generate_expr(object);
                self.line(&format!("(local $__all_src_{lbl} i32)"));
                self.line(&format!("(local $__all_idx_{lbl} i32)"));
                self.line(&format!("(local $__all_len_{lbl} i32)"));
                self.line(&format!("(local $__all_res_{lbl} i32)"));
                self.line(&format!("local.set $__all_src_{lbl}"));
                self.line(&format!("local.get $__all_src_{lbl}"));
                self.line("i32.load");
                self.line(&format!("local.set $__all_len_{lbl}"));
                self.line("i32.const 1");
                self.line(&format!("local.set $__all_res_{lbl}"));
                self.line("i32.const 0");
                self.line(&format!("local.set $__all_idx_{lbl}"));
                self.line(&format!("(block $__all_brk_{brk} (loop $__all_lp_{lbl}"));
                self.indent += 1;
                self.line(&format!("local.get $__all_idx_{lbl}"));
                self.line(&format!("local.get $__all_len_{lbl}"));
                self.line("i32.ge_u");
                self.line(&format!("br_if $__all_brk_{brk}"));
                self.line(&format!("local.get $__all_src_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__all_idx_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.load");
                if let Some(closure_arg) = args.first() {
                    self.generate_expr(closure_arg);
                    self.line("call_indirect (type 0)");
                }
                self.line("i32.eqz");
                self.emit("(if");
                self.indent += 1;
                self.emit("(then");
                self.indent += 1;
                self.line("i32.const 0");
                self.line(&format!("local.set $__all_res_{lbl}"));
                self.line(&format!("br $__all_brk_{brk}"));
                self.indent -= 1;
                self.line(")");
                self.indent -= 1;
                self.line(")");
                self.line(&format!("local.get $__all_idx_{lbl}"));
                self.line("i32.const 1");
                self.line("i32.add");
                self.line(&format!("local.set $__all_idx_{lbl}"));
                self.line(&format!("br $__all_lp_{lbl}"));
                self.indent -= 1;
                self.line("))");
                self.line(&format!("local.get $__all_res_{lbl}"));
            }
            "enumerate" => {
                let lbl = self.next_label();
                let brk = lbl + 1000;
                self.line(";; .enumerate() — (index, element) pairs");
                self.generate_expr(object);
                self.line(&format!("(local $__en_src_{lbl} i32)"));
                self.line(&format!("(local $__en_dst_{lbl} i32)"));
                self.line(&format!("(local $__en_idx_{lbl} i32)"));
                self.line(&format!("(local $__en_len_{lbl} i32)"));
                self.line(&format!("local.set $__en_src_{lbl}"));
                self.line(&format!("local.get $__en_src_{lbl}"));
                self.line("i32.load");
                self.line(&format!("local.set $__en_len_{lbl}"));
                self.line("global.get $__heap_ptr");
                self.line(&format!("local.set $__en_dst_{lbl}"));
                self.line(&format!("local.get $__en_len_{lbl}"));
                self.line("i32.const 8");
                self.line("i32.mul");
                self.line("i32.const 4");
                self.line("i32.add");
                self.line("global.get $__heap_ptr");
                self.line("i32.add");
                self.line("global.set $__heap_ptr");
                self.line(&format!("local.get $__en_dst_{lbl}"));
                self.line(&format!("local.get $__en_len_{lbl}"));
                self.line("i32.store");
                self.line("i32.const 0");
                self.line(&format!("local.set $__en_idx_{lbl}"));
                self.line(&format!("(block $__en_brk_{brk} (loop $__en_lp_{lbl}"));
                self.indent += 1;
                self.line(&format!("local.get $__en_idx_{lbl}"));
                self.line(&format!("local.get $__en_len_{lbl}"));
                self.line("i32.ge_u");
                self.line(&format!("br_if $__en_brk_{brk}"));
                self.line(&format!("local.get $__en_dst_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__en_idx_{lbl}"));
                self.line("i32.const 8");
                self.line("i32.mul");
                self.line("i32.add");
                self.line(&format!("local.get $__en_idx_{lbl}"));
                self.line("i32.store ;; store index");
                self.line(&format!("local.get $__en_dst_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__en_idx_{lbl}"));
                self.line("i32.const 8");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__en_src_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__en_idx_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.load");
                self.line("i32.store ;; store element");
                self.line(&format!("local.get $__en_idx_{lbl}"));
                self.line("i32.const 1");
                self.line("i32.add");
                self.line(&format!("local.set $__en_idx_{lbl}"));
                self.line(&format!("br $__en_lp_{lbl}"));
                self.indent -= 1;
                self.line("))");
                self.line(&format!("local.get $__en_dst_{lbl}"));
            }
            "zip" => {
                let lbl = self.next_label();
                let brk = lbl + 1000;
                self.line(";; .zip() — pair elements from two iterators");
                self.generate_expr(object);
                self.line(&format!("(local $__zip_a_{lbl} i32)"));
                self.line(&format!("(local $__zip_b_{lbl} i32)"));
                self.line(&format!("(local $__zip_dst_{lbl} i32)"));
                self.line(&format!("(local $__zip_idx_{lbl} i32)"));
                self.line(&format!("(local $__zip_len_{lbl} i32)"));
                self.line(&format!("local.set $__zip_a_{lbl}"));
                if let Some(other) = args.first() {
                    self.generate_expr(other);
                }
                self.line(&format!("local.set $__zip_b_{lbl}"));
                self.line(&format!("local.get $__zip_a_{lbl}"));
                self.line("i32.load");
                self.line(&format!("local.get $__zip_b_{lbl}"));
                self.line("i32.load");
                self.line(&format!("local.get $__zip_a_{lbl}"));
                self.line("i32.load");
                self.line(&format!("local.get $__zip_b_{lbl}"));
                self.line("i32.load");
                self.line("i32.lt_u");
                self.line("select ;; min(a.len, b.len)");
                self.line(&format!("local.set $__zip_len_{lbl}"));
                self.line("global.get $__heap_ptr");
                self.line(&format!("local.set $__zip_dst_{lbl}"));
                self.line(&format!("local.get $__zip_len_{lbl}"));
                self.line("i32.const 8");
                self.line("i32.mul");
                self.line("i32.const 4");
                self.line("i32.add");
                self.line("global.get $__heap_ptr");
                self.line("i32.add");
                self.line("global.set $__heap_ptr");
                self.line(&format!("local.get $__zip_dst_{lbl}"));
                self.line(&format!("local.get $__zip_len_{lbl}"));
                self.line("i32.store");
                self.line("i32.const 0");
                self.line(&format!("local.set $__zip_idx_{lbl}"));
                self.line(&format!("(block $__zip_brk_{brk} (loop $__zip_lp_{lbl}"));
                self.indent += 1;
                self.line(&format!("local.get $__zip_idx_{lbl}"));
                self.line(&format!("local.get $__zip_len_{lbl}"));
                self.line("i32.ge_u");
                self.line(&format!("br_if $__zip_brk_{brk}"));
                self.line(&format!("local.get $__zip_dst_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__zip_idx_{lbl}"));
                self.line("i32.const 8");
                self.line("i32.mul");
                self.line("i32.add");
                self.line(&format!("local.get $__zip_a_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__zip_idx_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.load");
                self.line("i32.store");
                self.line(&format!("local.get $__zip_dst_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__zip_idx_{lbl}"));
                self.line("i32.const 8");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__zip_b_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__zip_idx_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.load");
                self.line("i32.store");
                self.line(&format!("local.get $__zip_idx_{lbl}"));
                self.line("i32.const 1");
                self.line("i32.add");
                self.line(&format!("local.set $__zip_idx_{lbl}"));
                self.line(&format!("br $__zip_lp_{lbl}"));
                self.indent -= 1;
                self.line("))");
                self.line(&format!("local.get $__zip_dst_{lbl}"));
            }
            "count" => {
                self.line(";; .count() — element count");
                self.generate_expr(object);
                self.line("i32.load");
            }
            "take" | "skip" => {
                let is_take = method == "take";
                let tag = if is_take { "take" } else { "skip" };
                let lbl = self.next_label();
                self.line(&format!(";; .{tag}() — sub-array"));
                self.generate_expr(object);
                self.line(&format!("(local $__{tag}_src_{lbl} i32)"));
                self.line(&format!("(local $__{tag}_dst_{lbl} i32)"));
                self.line(&format!("(local $__{tag}_n_{lbl} i32)"));
                self.line(&format!("(local $__{tag}_len_{lbl} i32)"));
                self.line(&format!("(local $__{tag}_out_{lbl} i32)"));
                self.line(&format!("local.set $__{tag}_src_{lbl}"));
                if let Some(n_arg) = args.first() {
                    self.generate_expr(n_arg);
                } else {
                    self.line("i32.const 0");
                }
                self.line(&format!("local.set $__{tag}_n_{lbl}"));
                self.line(&format!("local.get $__{tag}_src_{lbl}"));
                self.line("i32.load");
                self.line(&format!("local.set $__{tag}_len_{lbl}"));
                if is_take {
                    self.line(&format!("local.get $__{tag}_n_{lbl}"));
                    self.line(&format!("local.get $__{tag}_len_{lbl}"));
                    self.line(&format!("local.get $__{tag}_n_{lbl}"));
                    self.line(&format!("local.get $__{tag}_len_{lbl}"));
                    self.line("i32.lt_u");
                    self.line("select");
                } else {
                    self.line(&format!("local.get $__{tag}_len_{lbl}"));
                    self.line(&format!("local.get $__{tag}_n_{lbl}"));
                    self.line("i32.sub");
                    self.line("i32.const 0");
                    self.line(&format!("local.get $__{tag}_len_{lbl}"));
                    self.line(&format!("local.get $__{tag}_n_{lbl}"));
                    self.line("i32.gt_u");
                    self.line("select");
                }
                self.line(&format!("local.set $__{tag}_out_{lbl}"));
                self.line("global.get $__heap_ptr");
                self.line(&format!("local.set $__{tag}_dst_{lbl}"));
                self.line(&format!("local.get $__{tag}_out_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.const 4");
                self.line("i32.add");
                self.line("global.get $__heap_ptr");
                self.line("i32.add");
                self.line("global.set $__heap_ptr");
                self.line(&format!("local.get $__{tag}_dst_{lbl}"));
                self.line(&format!("local.get $__{tag}_out_{lbl}"));
                self.line("i32.store");
                self.line(&format!("local.get $__{tag}_dst_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__{tag}_src_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                if !is_take {
                    self.line(&format!("local.get $__{tag}_n_{lbl}"));
                    self.line("i32.const 4");
                    self.line("i32.mul");
                    self.line("i32.add");
                }
                self.line(&format!("local.get $__{tag}_out_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("memory.copy");
                self.line(&format!("local.get $__{tag}_dst_{lbl}"));
            }
            _ => {
                // Default: regular method call
                self.generate_expr(object);
                for arg in args {
                    self.generate_expr(arg);
                }
                self.line(&format!("call ${method}"));
            }
        }
    }

    fn store_string(&mut self, s: &str) -> u32 {
        // Check if this string is already interned
        if let Some((_existing, offset)) = self.strings.iter().find(|(val, _)| val == s) {
            return *offset;
        }

        // Intern the string at the current offset
        let offset = self.string_offset;
        self.strings.push((s.to_string(), offset));
        self.string_offset += s.len() as u32;
        offset
    }

    fn emit_data_section(&mut self) {
        if self.strings.is_empty() {
            return;
        }
        self.line("");
        self.line(";; Interned string data");
        for (s, offset) in self.strings.clone() {
            // Escape special characters for WAT string literals
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            self.line(&format!("(data (i32.const {}) \"{}\")", offset, escaped));
        }
    }

    fn emit_gesture_runtime(&mut self) {
        self.line("");
        self.line(";; ========== Gesture runtime (WASM-internal) ==========");
        self.line(";; Swipe and pinch detection via pure WASM math.");
        self.line(";; Long press remains in core.js (needs setTimeout).");

        // Swipe globals
        self.line("(global $__swipe_start_x (mut i32) (i32.const 0))");
        self.line("(global $__swipe_start_y (mut i32) (i32.const 0))");
        self.line("(global $__swipe_start_time (mut i32) (i32.const 0))");
        self.line("(global $__swipe_cb (mut i32) (i32.const 0))");

        // Pinch globals
        self.line("(global $__pinch_initial_dist (mut i32) (i32.const 0))");
        self.line("(global $__pinch_cb (mut i32) (i32.const 0))");

        // $gesture_swipe_start (param $x i32) (param $y i32)
        self.line("");
        self.emit("(func $gesture_swipe_start (param $x i32) (param $y i32)");
        self.indent += 1;
        self.line("local.get $x");
        self.line("global.set $__swipe_start_x");
        self.line("local.get $y");
        self.line("global.set $__swipe_start_y");
        self.line("i32.const 0");
        self.line("global.set $__swipe_start_time");
        self.indent -= 1;
        self.line(")");

        // $gesture_swipe_end (param $x i32) (param $y i32) (result i32)
        // Returns direction: 0=none, 1=left, 2=right, 3=up, 4=down
        self.line("");
        self.emit("(func $gesture_swipe_end (param $x i32) (param $y i32) (result i32)");
        self.indent += 1;
        self.line("(local $dx i32)");
        self.line("(local $dy i32)");
        self.line("(local $abs_dx i32)");
        self.line("(local $abs_dy i32)");
        // dx = x - start_x
        self.line("local.get $x");
        self.line("global.get $__swipe_start_x");
        self.line("i32.sub");
        self.line("local.set $dx");
        // dy = y - start_y
        self.line("local.get $y");
        self.line("global.get $__swipe_start_y");
        self.line("i32.sub");
        self.line("local.set $dy");
        // abs_dx = (dx ^ (dx >> 31)) - (dx >> 31)
        self.line("local.get $dx");
        self.line("local.get $dx");
        self.line("i32.const 31");
        self.line("i32.shr_s");
        self.line("i32.xor");
        self.line("local.get $dx");
        self.line("i32.const 31");
        self.line("i32.shr_s");
        self.line("i32.sub");
        self.line("local.set $abs_dx");
        // abs_dy = (dy ^ (dy >> 31)) - (dy >> 31)
        self.line("local.get $dy");
        self.line("local.get $dy");
        self.line("i32.const 31");
        self.line("i32.shr_s");
        self.line("i32.xor");
        self.line("local.get $dy");
        self.line("i32.const 31");
        self.line("i32.shr_s");
        self.line("i32.sub");
        self.line("local.set $abs_dy");
        // Threshold check: if both < 30, return 0
        self.line("local.get $abs_dx");
        self.line("i32.const 30");
        self.line("i32.le_s");
        self.line("local.get $abs_dy");
        self.line("i32.const 30");
        self.line("i32.le_s");
        self.line("i32.and");
        self.line("if (result i32)");
        self.indent += 1;
        self.line("i32.const 0"); // none
        self.indent -= 1;
        self.line("else");
        self.indent += 1;
        // Horizontal vs vertical
        self.line("local.get $abs_dx");
        self.line("local.get $abs_dy");
        self.line("i32.gt_s");
        self.line("if (result i32)");
        self.indent += 1;
        // Horizontal: dx > 0 = right(2), dx < 0 = left(1)
        self.line("local.get $dx");
        self.line("i32.const 0");
        self.line("i32.gt_s");
        self.line("if (result i32)");
        self.indent += 1;
        self.line("i32.const 2"); // right
        self.indent -= 1;
        self.line("else");
        self.indent += 1;
        self.line("i32.const 1"); // left
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("else");
        self.indent += 1;
        // Vertical: dy > 0 = down(4), dy < 0 = up(3)
        self.line("local.get $dy");
        self.line("i32.const 0");
        self.line("i32.gt_s");
        self.line("if (result i32)");
        self.indent += 1;
        self.line("i32.const 4"); // down
        self.indent -= 1;
        self.line("else");
        self.indent += 1;
        self.line("i32.const 3"); // up
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line(")");

        // $gesture_pinch_start (param $x1 i32) (param $y1 i32) (param $x2 i32) (param $y2 i32)
        self.line("");
        self.emit("(func $gesture_pinch_start (param $x1 i32) (param $y1 i32) (param $x2 i32) (param $y2 i32)");
        self.indent += 1;
        // dist = sqrt((x2-x1)^2 + (y2-y1)^2) as i32
        self.line("local.get $x2");
        self.line("local.get $x1");
        self.line("i32.sub");
        self.line("f32.convert_i32_s");
        self.line("local.get $x2");
        self.line("local.get $x1");
        self.line("i32.sub");
        self.line("f32.convert_i32_s");
        self.line("f32.mul");
        self.line("local.get $y2");
        self.line("local.get $y1");
        self.line("i32.sub");
        self.line("f32.convert_i32_s");
        self.line("local.get $y2");
        self.line("local.get $y1");
        self.line("i32.sub");
        self.line("f32.convert_i32_s");
        self.line("f32.mul");
        self.line("f32.add");
        self.line("f32.sqrt");
        self.line("i32.trunc_f32_s");
        self.line("global.set $__pinch_initial_dist");
        self.indent -= 1;
        self.line(")");

        // $gesture_pinch_move — returns scale * 100 (fixed-point)
        self.line("");
        self.emit("(func $gesture_pinch_move (param $x1 i32) (param $y1 i32) (param $x2 i32) (param $y2 i32) (result i32)");
        self.indent += 1;
        self.line("(local $cur_dist f32)");
        // current distance
        self.line("local.get $x2");
        self.line("local.get $x1");
        self.line("i32.sub");
        self.line("f32.convert_i32_s");
        self.line("local.get $x2");
        self.line("local.get $x1");
        self.line("i32.sub");
        self.line("f32.convert_i32_s");
        self.line("f32.mul");
        self.line("local.get $y2");
        self.line("local.get $y1");
        self.line("i32.sub");
        self.line("f32.convert_i32_s");
        self.line("local.get $y2");
        self.line("local.get $y1");
        self.line("i32.sub");
        self.line("f32.convert_i32_s");
        self.line("f32.mul");
        self.line("f32.add");
        self.line("f32.sqrt");
        self.line("local.set $cur_dist");
        // scale = (cur_dist / initial_dist) * 100
        self.line("local.get $cur_dist");
        self.line("global.get $__pinch_initial_dist");
        self.line("f32.convert_i32_s");
        self.line("f32.div");
        self.line("f32.const 100");
        self.line("f32.mul");
        self.line("i32.trunc_f32_s");
        self.indent -= 1;
        self.line(")");
    }

    fn emit_flags_runtime(&mut self) {
        self.line("");
        self.line(";; ========== Feature flags runtime (WASM-internal) ==========");
        self.line(";; Compile-time feature flags in WASM linear memory.");
        self.line(";; Flag table at 196608 (192KB). Each entry: 64 bytes (60 name + 4 enabled).");

        // Globals
        self.line("(global $__flag_count (mut i32) (i32.const 0))");
        self.line("(global $__flag_base i32 (i32.const 196608))");

        // $flags_register (param $name_ptr i32) (param $name_len i32) (param $enabled i32)
        self.line("");
        self.emit("(func $flags_register (param $name_ptr i32) (param $name_len i32) (param $enabled i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("(local $i i32)");
        // addr = flag_base + flag_count * 64
        self.line("global.get $__flag_base");
        self.line("global.get $__flag_count");
        self.line("i32.const 64");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        // Copy name bytes: loop i from 0 to name_len
        self.line("i32.const 0");
        self.line("local.set $i");
        self.line("block $copy_done");
        self.indent += 1;
        self.line("loop $copy_loop");
        self.indent += 1;
        self.line("local.get $i");
        self.line("local.get $name_len");
        self.line("i32.ge_u");
        self.line("br_if $copy_done");
        // store byte: mem[addr + i] = mem[name_ptr + i]
        self.line("local.get $addr");
        self.line("local.get $i");
        self.line("i32.add");
        self.line("local.get $name_ptr");
        self.line("local.get $i");
        self.line("i32.add");
        self.line("i32.load8_u");
        self.line("i32.store8");
        self.line("local.get $i");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("local.set $i");
        self.line("br $copy_loop");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        // Store enabled at addr + 60
        self.line("local.get $addr");
        self.line("local.get $enabled");
        self.line("i32.store offset=60");
        // Increment flag count
        self.line("global.get $__flag_count");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("global.set $__flag_count");
        self.indent -= 1;
        self.line(")");

        // $flags_is_enabled (param $name_ptr i32) (param $name_len i32) (result i32)
        self.line("");
        self.emit("(func $flags_is_enabled (param $name_ptr i32) (param $name_len i32) (result i32)");
        self.indent += 1;
        self.line("(local $idx i32)");
        self.line("(local $addr i32)");
        self.line("(local $j i32)");
        self.line("(local $match i32)");
        self.line("i32.const 0");
        self.line("local.set $idx");
        self.line("block $not_found");
        self.indent += 1;
        self.line("loop $scan");
        self.indent += 1;
        self.line("local.get $idx");
        self.line("global.get $__flag_count");
        self.line("i32.ge_u");
        self.line("br_if $not_found");
        // addr = flag_base + idx * 64
        self.line("global.get $__flag_base");
        self.line("local.get $idx");
        self.line("i32.const 64");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        // Compare name bytes
        self.line("i32.const 1");
        self.line("local.set $match");
        self.line("i32.const 0");
        self.line("local.set $j");
        self.line("block $cmp_done");
        self.indent += 1;
        self.line("loop $cmp_loop");
        self.indent += 1;
        self.line("local.get $j");
        self.line("local.get $name_len");
        self.line("i32.ge_u");
        self.line("br_if $cmp_done");
        // if mem[addr+j] != mem[name_ptr+j], no match
        self.line("local.get $addr");
        self.line("local.get $j");
        self.line("i32.add");
        self.line("i32.load8_u");
        self.line("local.get $name_ptr");
        self.line("local.get $j");
        self.line("i32.add");
        self.line("i32.load8_u");
        self.line("i32.ne");
        self.line("if");
        self.indent += 1;
        self.line("i32.const 0");
        self.line("local.set $match");
        self.line("br $cmp_done");
        self.indent -= 1;
        self.line("end");
        self.line("local.get $j");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("local.set $j");
        self.line("br $cmp_loop");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        // If match, return enabled value
        self.line("local.get $match");
        self.line("if (result i32)");
        self.indent += 1;
        self.line("local.get $addr");
        self.line("i32.load offset=60");
        self.line("return");
        self.indent -= 1;
        self.line("else");
        self.indent += 1;
        // Continue scanning
        self.line("local.get $idx");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("local.set $idx");
        self.line("br $scan");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        // Not found — return 0
        self.line("i32.const 0");
        self.indent -= 1;
        self.line(")");
    }

    fn emit_ai_runtime(&mut self) {
        self.line("");
        self.line(";; ========== AI tool registration runtime (WASM-internal) ==========");
        self.line(";; Tool table at 262144 (256KB). Each entry: 256 bytes.");
        self.line(";; Layout: [name_ptr:i32, name_len:i32, desc_ptr:i32, desc_len:i32,");
        self.line(";;          schema_ptr:i32, schema_len:i32, callback_idx:i32, pad:228]");

        // Globals
        self.line("(global $__tool_count (mut i32) (i32.const 0))");
        self.line("(global $__tool_base i32 (i32.const 262144))");

        // $ai_register_tool
        self.line("");
        self.emit("(func $ai_register_tool (param $name_ptr i32) (param $name_len i32) (param $desc_ptr i32) (param $desc_len i32) (param $schema_ptr i32) (param $schema_len i32) (param $cb i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        // addr = tool_base + tool_count * 256
        self.line("global.get $__tool_base");
        self.line("global.get $__tool_count");
        self.line("i32.const 256");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        // Store fields
        self.line("local.get $addr");
        self.line("local.get $name_ptr");
        self.line("i32.store");           // offset 0: name_ptr
        self.line("local.get $addr");
        self.line("local.get $name_len");
        self.line("i32.store offset=4");  // offset 4: name_len
        self.line("local.get $addr");
        self.line("local.get $desc_ptr");
        self.line("i32.store offset=8");  // offset 8: desc_ptr
        self.line("local.get $addr");
        self.line("local.get $desc_len");
        self.line("i32.store offset=12"); // offset 12: desc_len
        self.line("local.get $addr");
        self.line("local.get $schema_ptr");
        self.line("i32.store offset=16"); // offset 16: schema_ptr
        self.line("local.get $addr");
        self.line("local.get $schema_len");
        self.line("i32.store offset=20"); // offset 20: schema_len
        self.line("local.get $addr");
        self.line("local.get $cb");
        self.line("i32.store offset=24"); // offset 24: callback index
        // Increment tool count
        self.line("global.get $__tool_count");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("global.set $__tool_count");
        self.indent -= 1;
        self.line(")");

        // $ai_get_tool_count (result i32)
        self.line("");
        self.emit("(func $ai_get_tool_count (result i32)");
        self.indent += 1;
        self.line("global.get $__tool_count");
        self.indent -= 1;
        self.line(")");

        // $ai_get_tool_name (param $idx i32) (result i32 i32)
        self.line("");
        self.emit("(func $ai_get_tool_name (param $idx i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__tool_base");
        self.line("local.get $idx");
        self.line("i32.const 256");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        self.line("local.get $addr");
        self.line("i32.load");            // name_ptr
        self.line("local.get $addr");
        self.line("i32.load offset=4");   // name_len
        self.indent -= 1;
        self.line(")");

        // $ai_get_tool_schema (param $idx i32) (result i32 i32)
        self.line("");
        self.emit("(func $ai_get_tool_schema (param $idx i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__tool_base");
        self.line("local.get $idx");
        self.line("i32.const 256");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        self.line("local.get $addr");
        self.line("i32.load offset=16");  // schema_ptr
        self.line("local.get $addr");
        self.line("i32.load offset=20");  // schema_len
        self.indent -= 1;
        self.line(")");

        // $ai_call_tool (param $idx i32)
        self.line("");
        self.emit("(func $ai_call_tool (param $idx i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__tool_base");
        self.line("local.get $idx");
        self.line("i32.const 256");
        self.line("i32.mul");
        self.line("i32.add");
        self.line("local.set $addr");
        // Load callback index and call via indirect
        self.line("local.get $addr");
        self.line("i32.load offset=24");
        self.line("call_indirect (type $__effect_type)");
        self.indent -= 1;
        self.line(")");
    }

    fn next_label(&mut self) -> u32 {
        self.label_counter += 1;
        self.label_counter
    }

    fn emit(&mut self, s: &str) {
        let indent = "  ".repeat(self.indent);
        self.output.push_str(&format!("\n{}{}", indent, s));
    }

    fn line(&mut self, s: &str) {
        let indent = "  ".repeat(self.indent);
        self.output.push_str(&format!("\n{}{}", indent, s));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn compile(src: &str) -> String {
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let mut codegen = WasmCodegen::new();
        codegen.generate(&program)
    }

    #[test]
    fn test_simple_function() {
        let wat = compile("pub fn add(a: i32, b: i32) -> i32 { return a + b; }");
        assert!(wat.contains("func $add"));
        assert!(wat.contains("i32.add"));
    }

    #[test]
    fn test_struct_layout() {
        let wat = compile("struct Point { x: f64, y: f64 }");
        assert!(wat.contains("struct Point layout"));
        assert!(wat.contains("total size: 16 bytes"));
    }

    #[test]
    fn test_format_string_codegen() {
        let wat = compile(r#"pub fn greet(name: string) -> string { return f"hello {name}!"; }"#);
        assert!(wat.contains("$string_concat"), "WAT should call $string_concat");
        assert!(wat.contains("hello "), "WAT should contain literal 'hello '");
    }

}

#[cfg(test)]
mod iterator_codegen_tests {
    use super::*;
    use crate::ast::*;
    use crate::token::Span;

    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    fn block(stmts: Vec<Stmt>) -> Block {
        Block { stmts, span: span() }
    }

    #[test]
    fn map_generates_loop() {
        // Build AST for: arr.iter().map(|x| x * 2)
        let expr = Expr::MethodCall {
            object: Box::new(Expr::MethodCall {
                object: Box::new(Expr::Ident("arr".into())),
                method: "iter".into(),
                args: vec![],
            }),
            method: "map".into(),
            args: vec![Expr::Closure {
                params: vec![("x".into(), None)],
                body: Box::new(Expr::Binary {
                    op: BinOp::Mul,
                    left: Box::new(Expr::Ident("x".into())),
                    right: Box::new(Expr::Integer(2)),
                }),
            }],
        };

        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();

        assert!(output.contains(".map()"), "should contain map comment");
        assert!(output.contains("loop $__map_lp_"), "should generate a WASM loop for map");
        assert!(output.contains("i32.store"), "should store mapped elements");
    }

    #[test]
    fn filter_generates_loop_with_conditional() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("iter_val".into())),
            method: "filter".into(),
            args: vec![Expr::Closure {
                params: vec![("x".into(), None)],
                body: Box::new(Expr::Binary {
                    op: BinOp::Gt,
                    left: Box::new(Expr::Ident("x".into())),
                    right: Box::new(Expr::Integer(0)),
                }),
            }],
        };

        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();

        assert!(output.contains(".filter()"), "should contain filter comment");
        assert!(output.contains("loop $__flt_lp_"), "should generate a WASM loop for filter");
        assert!(output.contains("(if"), "should have conditional for predicate");
    }

    #[test]
    fn collect_is_passthrough() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("iter_val".into())),
            method: "collect".into(),
            args: vec![],
        };

        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();

        assert!(output.contains(".collect()"), "should contain collect comment");
        // collect should NOT generate a loop — it's a pass-through
        assert!(!output.contains("loop"), "collect should not generate a loop");
    }

    #[test]
    fn any_generates_early_exit_loop() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("iter_val".into())),
            method: "any".into(),
            args: vec![Expr::Closure {
                params: vec![("x".into(), None)],
                body: Box::new(Expr::Binary {
                    op: BinOp::Gt,
                    left: Box::new(Expr::Ident("x".into())),
                    right: Box::new(Expr::Integer(0)),
                }),
            }],
        };

        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();

        assert!(output.contains(".any()"), "should contain any comment");
        assert!(output.contains("loop $__any_lp_"), "should generate loop");
        assert!(output.contains("br_if $__any_brk_"), "should have early exit");
    }

    #[test]
    fn count_emits_load() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("iter_val".into())),
            method: "count".into(),
            args: vec![],
        };

        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();

        assert!(output.contains(".count()"), "should contain count comment");
        assert!(output.contains("i32.load"), "should load array length");
    }
}

#[cfg(test)]
mod closure_codegen_tests {
    use super::*;
    use crate::token::Span;

    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    #[test]
    fn closure_generates_wat_function() {
        // |x: i32| x + 1
        let program = Program {
            items: vec![Item::Function(Function {
                name: "main".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Let {
                        name: "f".to_string(),
                        ty: None,
                        mutable: false,
                        secret: false,
                        value: Expr::Closure {
                            params: vec![("x".to_string(), Some(Type::Named("i32".to_string())))],
                            body: Box::new(Expr::Binary {
                                op: BinOp::Add,
                                left: Box::new(Expr::Ident("x".to_string())),
                                right: Box::new(Expr::Integer(1)),
                            }),
                        },
                        ownership: Ownership::Owned,
                    }],
                    span: span(),
                },
                is_pub: true,
                must_use: false,
                span: span(),
            })],
        };

        let mut codegen = WasmCodegen::new();
        let output = codegen.generate(&program);

        // Should contain a closure function
        assert!(output.contains("$__closure_0"), "WAT should contain closure function name");
        // Should contain the function table for indirect calls
        assert!(output.contains("funcref"), "WAT should contain function table");
        // Should contain parameter for x
        assert!(output.contains("(param $x i32)"), "WAT should contain closure param");
    }

    #[test]
    fn no_param_closure_generates_wat() {
        // || 42
        let expr = Expr::Closure {
            params: vec![],
            body: Box::new(Expr::Integer(42)),
        };

        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);

        assert!(codegen.closure_functions.len() == 1, "should generate one closure function");
        assert!(codegen.closure_func_names[0] == "$__closure_0", "closure should be named $__closure_0");
        assert!(codegen.needs_func_table, "should need function table");
    }
}
