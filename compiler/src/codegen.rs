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

        // Import string runtime — format string support
        self.line("(import \"string\" \"concat\" (func $string_concat (param i32 i32 i32 i32) (result i32 i32)))");
        self.line("(import \"string\" \"fromI32\" (func $string_fromI32 (param i32) (result i32 i32)))");
        self.line("(import \"string\" \"fromF64\" (func $string_fromF64 (param f64) (result i32 i32)))");
        self.line("(import \"string\" \"fromBool\" (func $string_fromBool (param i32) (result i32 i32)))");
        // $to_string: generic value-to-string (i32 -> ptr, len). Runtime dispatches by type tag.
        self.line("(import \"string\" \"toString\" (func $to_string (param i32) (result i32 i32)))");

        // Import DOM manipulation functions from JS runtime
        self.line("(import \"dom\" \"createElement\" (func $dom_createElement (param i32 i32) (result i32)))");
        self.line("(import \"dom\" \"setText\" (func $dom_setText (param i32 i32 i32)))");
        self.line("(import \"dom\" \"appendChild\" (func $dom_appendChild (param i32 i32)))");
        self.line("(import \"dom\" \"addEventListener\" (func $dom_addEventListener (param i32 i32 i32 i32)))");
        self.line("(import \"dom\" \"setAttribute\" (func $dom_setAttribute (param i32 i32 i32 i32)))");
        self.line("(import \"dom\" \"setProperty\" (func $dom_setProperty (param i32 i32 i32 i32 i32)))");
        self.line("(import \"dom\" \"getProperty\" (func $dom_getProperty (param i32 i32 i32) (result i32 i32)))");

        // Import signal runtime
        self.line("(import \"signal\" \"create\" (func $signal_create (param i32) (result i32)))");
        self.line("(import \"signal\" \"get\" (func $signal_get (param i32) (result i32)))");
        self.line("(import \"signal\" \"set\" (func $signal_set (param i32 i32)))");
        self.line("(import \"signal\" \"subscribe\" (func $signal_subscribe (param i32 i32)))");
        self.line("(import \"signal\" \"createEffect\" (func $signal_createEffect (param i32)))");
        self.line("(import \"signal\" \"createMemo\" (func $signal_createMemo (param i32) (result i32)))");
        self.line("(import \"signal\" \"batch\" (func $signal_batch (param i32)))");

        // Import HTTP/fetch runtime
        self.line("(import \"http\" \"fetch\" (func $http_fetch (param i32 i32 i32 i32) (result i32)))");
        self.line("(import \"http\" \"fetchGetBody\" (func $http_fetchGetBody (param i32) (result i32 i32)))");
        self.line("(import \"http\" \"fetchGetStatus\" (func $http_fetchGetStatus (param i32) (result i32)))");

        // Import worker/concurrency runtime
        self.line("(import \"worker\" \"spawn\" (func $worker_spawn (param i32) (result i32)))");
        self.line("(import \"worker\" \"channelCreate\" (func $worker_channelCreate (result i32)))");
        self.line("(import \"worker\" \"channelSend\" (func $worker_channelSend (param i32 i32 i32)))");
        self.line("(import \"worker\" \"channelRecv\" (func $worker_channelRecv (param i32 i32)))");
        self.line("(import \"worker\" \"parallel\" (func $worker_parallel (param i32 i32 i32)))");

        // Import AI runtime — LLM interaction primitives
        self.line("");
        self.line(";; AI runtime imports");
        self.line("(import \"ai\" \"chatStream\" (func $ai_chatStream (param i32 i32 i32 i32 i32 i32 i32 i32 i32)))");
        self.line("(import \"ai\" \"chatComplete\" (func $ai_chatComplete (param i32 i32 i32 i32 i32)))");
        self.line("(import \"ai\" \"registerTool\" (func $ai_registerTool (param i32 i32 i32 i32 i32 i32 i32)))");
        self.line("(import \"ai\" \"embed\" (func $ai_embed (param i32 i32 i32)))");
        self.line("(import \"ai\" \"parseStructured\" (func $ai_parseStructured (param i32 i32 i32 i32) (result i32)))");

        // Import streaming runtime — for streaming fetch, SSE, WebSocket
        self.line("");
        self.line(";; Streaming runtime imports");
        self.line("(import \"streaming\" \"streamFetch\" (func $streaming_streamFetch (param i32 i32 i32)))");
        self.line("(import \"streaming\" \"sseConnect\" (func $streaming_sseConnect (param i32 i32 i32)))");
        self.line("(import \"streaming\" \"wsConnect\" (func $streaming_wsConnect (param i32 i32 i32)))");
        self.line("(import \"streaming\" \"wsSend\" (func $streaming_wsSend (param i32 i32 i32)))");
        self.line("(import \"streaming\" \"wsClose\" (func $streaming_wsClose (param i32)))");
        self.line("(import \"streaming\" \"yield\" (func $streaming_yield (param i32 i32)))");

        // Import media runtime — lazy images, decode, preload, progressive loading
        self.line("");
        self.line(";; Media runtime imports");
        self.line("(import \"media\" \"lazyImage\" (func $media_lazyImage (param i32 i32 i32 i32 i32)))");
        self.line("(import \"media\" \"decodeImage\" (func $media_decodeImage (param i32 i32 i32)))");
        self.line("(import \"media\" \"preload\" (func $media_preload (param i32 i32 i32 i32)))");
        self.line("(import \"media\" \"progressiveImage\" (func $media_progressiveImage (param i32 i32 i32 i32 i32)))");

        // Import lazy component mounting support
        self.line("(import \"dom\" \"lazyMount\" (func $dom_lazyMount (param i32 i32 i32 i32)))");

        // Import router runtime
        self.line("");
        self.line(";; Router runtime imports");
        self.line("(import \"router\" \"init\" (func $router_init (param i32 i32)))");
        self.line("(import \"router\" \"navigate\" (func $router_navigate (param i32 i32)))");
        self.line("(import \"router\" \"currentPath\" (func $router_currentPath (result i32 i32)))");
        self.line("(import \"router\" \"getParam\" (func $router_getParam (param i32 i32) (result i32 i32)))");
        self.line("(import \"router\" \"registerRoute\" (func $router_registerRoute (param i32 i32 i32)))");

        // Import style runtime
        self.line("");
        self.line(";; Scoped style runtime imports");
        self.line("(import \"style\" \"injectStyles\" (func $style_injectStyles (param i32 i32 i32 i32) (result i32)))");
        self.line("(import \"style\" \"applyScope\" (func $style_applyScope (param i32 i32 i32)))");

        // Import animation runtime
        self.line("");
        self.line(";; Animation runtime imports");
        self.line("(import \"animation\" \"registerTransition\" (func $animation_registerTransition (param i32 i32 i32 i32 i32 i32 i32)))");
        self.line("(import \"animation\" \"registerKeyframes\" (func $animation_registerKeyframes (param i32 i32 i32 i32)))");
        self.line("(import \"animation\" \"play\" (func $animation_play (param i32 i32 i32 i32 i32)))");
        self.line("(import \"animation\" \"pause\" (func $animation_pause (param i32)))");
        self.line("(import \"animation\" \"cancel\" (func $animation_cancel (param i32)))");
        self.line("(import \"animation\" \"onFinish\" (func $animation_onFinish (param i32 i32)))");

        // Import accessibility (a11y) runtime
        self.line("");
        self.line(";; Accessibility (a11y) runtime imports");
        self.line("(import \"a11y\" \"setAriaAttribute\" (func $a11y_setAriaAttribute (param i32 i32 i32 i32 i32)))");
        self.line("(import \"a11y\" \"setRole\" (func $a11y_setRole (param i32 i32 i32)))");
        self.line("(import \"a11y\" \"manageFocus\" (func $a11y_manageFocus (param i32)))");
        self.line("(import \"a11y\" \"announceToScreenReader\" (func $a11y_announceToScreenReader (param i32 i32 i32)))");
        self.line("(import \"a11y\" \"trapFocus\" (func $a11y_trapFocus (param i32)))");
        self.line("(import \"a11y\" \"releaseFocusTrap\" (func $a11y_releaseFocusTrap))");

        // Import test runtime
        self.line("");
        self.line(";; Test runtime imports");
        self.line("(import \"test\" \"pass\" (func $test_pass (param i32 i32)))");
        self.line("(import \"test\" \"fail\" (func $test_fail (param i32 i32 i32 i32)))");
        self.line("(import \"test\" \"summary\" (func $test_summary (param i32 i32)))");

        // Import Web API runtime — localStorage, clipboard, timers, URL, console, misc
        self.line("");
        self.line(";; Web API runtime imports — storage");
        self.line("(import \"webapi\" \"localStorageGet\" (func $webapi_localStorageGet (param i32 i32) (result i32 i32)))");
        self.line("(import \"webapi\" \"localStorageSet\" (func $webapi_localStorageSet (param i32 i32 i32 i32)))");
        self.line("(import \"webapi\" \"localStorageRemove\" (func $webapi_localStorageRemove (param i32 i32)))");
        self.line("(import \"webapi\" \"sessionStorageGet\" (func $webapi_sessionStorageGet (param i32 i32) (result i32 i32)))");
        self.line("(import \"webapi\" \"sessionStorageSet\" (func $webapi_sessionStorageSet (param i32 i32 i32 i32)))");
        self.line("");
        self.line(";; Web API runtime imports — clipboard");
        self.line("(import \"webapi\" \"clipboardWrite\" (func $webapi_clipboardWrite (param i32 i32)))");
        self.line("(import \"webapi\" \"clipboardRead\" (func $webapi_clipboardRead (param i32)))");
        self.line("");
        self.line(";; Web API runtime imports — timers");
        self.line("(import \"webapi\" \"setTimeout\" (func $webapi_setTimeout (param i32 i32) (result i32)))");
        self.line("(import \"webapi\" \"setInterval\" (func $webapi_setInterval (param i32 i32) (result i32)))");
        self.line("(import \"webapi\" \"clearTimer\" (func $webapi_clearTimer (param i32)))");
        self.line("");
        self.line(";; Web API runtime imports — URL/history");
        self.line("(import \"webapi\" \"getLocationHref\" (func $webapi_getLocationHref (result i32 i32)))");
        self.line("(import \"webapi\" \"getLocationSearch\" (func $webapi_getLocationSearch (result i32 i32)))");
        self.line("(import \"webapi\" \"getLocationHash\" (func $webapi_getLocationHash (result i32 i32)))");
        self.line("(import \"webapi\" \"pushState\" (func $webapi_pushState (param i32 i32)))");
        self.line("(import \"webapi\" \"replaceState\" (func $webapi_replaceState (param i32 i32)))");
        self.line("");
        self.line(";; Web API runtime imports — console");
        self.line("(import \"webapi\" \"consoleLog\" (func $webapi_consoleLog (param i32 i32)))");
        self.line("(import \"webapi\" \"consoleWarn\" (func $webapi_consoleWarn (param i32 i32)))");
        self.line("(import \"webapi\" \"consoleError\" (func $webapi_consoleError (param i32 i32)))");
        self.line("");
        self.line(";; Web API runtime imports — misc");
        self.line("(import \"webapi\" \"randomFloat\" (func $webapi_randomFloat (result f64)))");
        self.line("(import \"webapi\" \"now\" (func $webapi_now (result f64)))");
        self.line("(import \"webapi\" \"requestAnimationFrame\" (func $webapi_requestAnimationFrame (param i32) (result i32)))");

        // Allocator (bump allocator for now)
        self.line("");
        self.line("(global $heap_ptr (mut i32) (i32.const 1024))");
        self.line("");
        self.emit_alloc_function();

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
            _ => {
                self.line(&format!(";; TODO: codegen for {:?}", std::mem::discriminant(item)));
            }
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

        // Generate the init/mount function
        self.emit(&format!("(func ${comp_name}_mount (export \"{comp_name}_mount\") (param $root i32)"));
        self.indent += 1;

        // Each state field becomes a signal (returns signal ID)
        for state in &comp.state {
            self.line(&format!("(local ${} i32) ;; signal ID for {}", state.name, state.name));
        }

        // Initialize signals via runtime
        for state in &comp.state {
            self.generate_expr(&state.initializer);
            self.line("call $signal_create");
            self.line(&format!("local.set ${}", state.name));
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
            Stmt::Let { name, value, .. } => {
                self.generate_expr(value);
                self.line(&format!("local.set ${}", name));
            }
            Stmt::Signal { name, value, .. } => {
                // Signals compile to a memory slot + getter/setter
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
            Expr::Fetch { url, options } => {
                self.line(";; fetch — HTTP request");
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
            }
            Expr::Spawn { body } => {
                self.line(";; spawn — launch task on Web Worker");
                // The body expression should resolve to a function index
                self.generate_expr(body);
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
            Expr::Parallel { exprs } => {
                self.line(";; parallel — run expressions concurrently");
                // Store function indices in linear memory for the runtime
                let count = exprs.len() as u32;
                let array_label = self.next_label();
                self.line(&format!("(local $parallel_arr_{} i32)", array_label));
                self.line(&format!("i32.const {}", count * 4));
                self.line("call $alloc");
                self.line(&format!("local.set $parallel_arr_{}", array_label));
                for (i, expr) in exprs.iter().enumerate() {
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
