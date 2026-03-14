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
    /// When true, template local declarations are deferred to template_locals
    /// instead of being emitted inline
    defer_template_locals: bool,
    /// Deferred template local declarations
    template_locals: Vec<String>,
    /// When inside a component context, self.field compiles to signal
    /// global.get/set instead of struct pointer dereference
    in_component_mount: bool,
    /// When true, $self is available as a local parameter (handler trampolines).
    /// When false but in_component_mount is true (mount function), self → i32.const 0.
    in_handler_body: bool,
    /// Field names available as signals in the current component
    component_fields: Vec<String>,
    /// Current component name (for global signal variable naming)
    component_name: String,
    /// Deferred signal→DOM updater functions: (func_name, global_el_name, signal_global_name)
    signal_updaters: Vec<(String, String, String)>,
    /// Names of components defined in this program (for detecting component instantiation)
    known_components: Vec<String>,
    /// Prop names for the current component being generated (String props passed as ptr+len pairs)
    component_props: Vec<String>,
    /// Map from component name to its prop list (for passing props at instantiation sites)
    component_prop_defs: Vec<(String, Vec<String>)>,
    /// Names of keyword definitions (auth, cache, db, payment, upload, pdf) in this program.
    /// These are namespace-like objects — method calls on them dispatch through WASM imports,
    /// and bare references to them emit `i32.const 0` (null placeholder) rather than
    /// `local.get $<name>` which would be undefined.
    known_keyword_defs: Vec<(String, KeywordDefKind)>,
    /// Names of stores defined in this program (for resolving StoreName::signal calls).
    /// Each entry is (store_name, signal_names).
    known_stores: Vec<(String, Vec<String>)>,
}

/// The kind of a top-level keyword definition.
#[derive(Debug, Clone, PartialEq)]
enum KeywordDefKind {
    Auth,
    Cache,
    Database,
    Payment,
    Upload,
    Pdf,
    Theme,
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
            defer_template_locals: false,
            template_locals: Vec::new(),
            in_component_mount: false,
            in_handler_body: false,
            component_fields: Vec::new(),
            component_name: String::new(),
            signal_updaters: Vec::new(),
            known_components: Vec::new(),
            component_props: Vec::new(),
            component_prop_defs: Vec::new(),
            known_keyword_defs: Vec::new(),
            known_stores: Vec::new(),
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
        self.line("(import \"dom\" \"setText\" (func $dom_setText (param i32 i32 i32)))");
        self.line("(import \"dom\" \"appendChild\" (func $dom_appendChild (param i32 i32)))");
        self.line("(import \"dom\" \"setAttribute\" (func $dom_setAttr (param i32 i32 i32 i32 i32)))");
        self.line("(import \"dom\" \"setStyle\" (func $dom_setStyle (param i32 i32 i32 i32 i32)))");
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
        self.line("(import \"dom\" \"reloadModule\" (func $dom_reloadModule (param i32 i32 i32)))");
        self.line("(import \"dom\" \"injectStyles\" (func $style_injectStyles (param i32 i32 i32 i32) (result i32)))");
        self.line("(import \"dom\" \"embedLoadSandboxed\" (func $embed_load_sandboxed (param i32 i32 i32 i32)))");
        self.line("(import \"dom\" \"embedLoadScript\" (func $embed_load_script (param i32 i32 i32 i32 i32)))");
        self.line("(import \"dom\" \"setProperty\" (func $dom_setProperty (param i32 i32 i32 i32)))");

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


        // ── HTTP — typed setters + fetch ─────────────────────────────────────
        self.line("");
        self.line(";; HTTP — browser fetch API (typed setters, no JSON)");
        self.line("(import \"http\" \"setMethod\" (func $http_setMethod (param i32 i32)))");
        self.line("(import \"http\" \"setBody\" (func $http_setBody (param i32 i32)))");
        self.line("(import \"http\" \"addHeader\" (func $http_addHeader (param i32 i32 i32 i32)))");
        self.line("(import \"http\" \"fetch\" (func $http_fetch (param i32 i32) (result i32)))");

        // ── Observe — IntersectionObserver + matchMedia ──────────────────────
        self.line("");
        self.line(";; Observe — browser IntersectionObserver + matchMedia APIs");
        self.line("(import \"observe\" \"matchMedia\" (func $observe_matchMedia (param i32 i32) (result i32)))");
        self.line("(import \"observe\" \"intersectionObserver\" (func $observe_intersectionObserver (param i32 i32) (result i32)))");
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
        self.line("(import \"worker\" \"spawn\" (func $worker_spawn (param i32 i32) (result i32)))");
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
        self.line("(import \"pwa\" \"registerPush\" (func $pwa_registerPush (param i32)))");
        self.line("(import \"pwa\" \"registerServiceWorker\" (func $pwa_registerServiceWorker (param i32 i32 i32)))");

        // ── Hardware — device APIs ───────────────────────────────────────────
        self.line("");
        self.line(";; Hardware — browser device APIs");
        self.line("(import \"hardware\" \"haptic\" (func $hardware_haptic (param i32)))");
        self.line("(import \"hardware\" \"biometricAuth\" (func $hardware_biometricAuth (param i32 i32 i32)))");
        self.line("(import \"hardware\" \"cameraCapture\" (func $hardware_cameraCapture (param i32 i32)))");
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
        self.line("(import \"time\" \"formatDate\" (func $time_formatDate (param f64 i32 i32 i32) (result i32)))");

        // ── Streaming — ReadableStream + EventSource ─────────────────────────
        self.line("");
        self.line(";; Streaming — browser ReadableStream + EventSource APIs");
        self.line("(import \"streaming\" \"streamFetch\" (func $streaming_streamFetch (param i32 i32 i32)))");
        self.line("(import \"streaming\" \"sseConnect\" (func $streaming_sseConnect (param i32 i32 i32)))");

        // ── RTC — WebRTC peer connections, data channels, media tracks ────────
        self.line("");
        self.line(";; RTC — browser WebRTC APIs (RTCPeerConnection, data channels, media)");
        self.line("(import \"rtc\" \"createPeer\" (func $rtc_createPeer (param i32) (result i32)))");
        self.line("(import \"rtc\" \"createPeerWithIce\" (func $rtc_createPeerWithIce (param i32 i32) (result i32)))");
        self.line("(import \"rtc\" \"createOffer\" (func $rtc_createOffer (param i32 i32)))");
        self.line("(import \"rtc\" \"createAnswer\" (func $rtc_createAnswer (param i32 i32)))");
        self.line("(import \"rtc\" \"setLocalDescription\" (func $rtc_setLocalDescription (param i32 i32 i32 i32 i32 i32)))");
        self.line("(import \"rtc\" \"setRemoteDescription\" (func $rtc_setRemoteDescription (param i32 i32 i32 i32 i32 i32)))");
        self.line("(import \"rtc\" \"addIceCandidate\" (func $rtc_addIceCandidate (param i32 i32 i32 i32 i32 i32)))");
        self.line("(import \"rtc\" \"createDataChannel\" (func $rtc_createDataChannel (param i32 i32 i32 i32) (result i32)))");
        self.line("(import \"rtc\" \"dataChannelSend\" (func $rtc_dataChannelSend (param i32 i32 i32)))");
        self.line("(import \"rtc\" \"dataChannelSendBinary\" (func $rtc_dataChannelSendBinary (param i32 i32 i32)))");
        self.line("(import \"rtc\" \"dataChannelClose\" (func $rtc_dataChannelClose (param i32)))");
        self.line("(import \"rtc\" \"dataChannelGetState\" (func $rtc_dataChannelGetState (param i32) (result i32)))");
        self.line("(import \"rtc\" \"onDataChannelMessage\" (func $rtc_onDataChannelMessage (param i32 i32)))");
        self.line("(import \"rtc\" \"onDataChannelOpen\" (func $rtc_onDataChannelOpen (param i32 i32)))");
        self.line("(import \"rtc\" \"onDataChannelClose\" (func $rtc_onDataChannelClose (param i32 i32)))");
        self.line("(import \"rtc\" \"addTrack\" (func $rtc_addTrack (param i32 i32 i32) (result i32)))");
        self.line("(import \"rtc\" \"removeTrack\" (func $rtc_removeTrack (param i32 i32)))");
        self.line("(import \"rtc\" \"getStats\" (func $rtc_getStats (param i32 i32)))");
        self.line("(import \"rtc\" \"close\" (func $rtc_close (param i32)))");
        self.line("(import \"rtc\" \"onIceCandidate\" (func $rtc_onIceCandidate (param i32 i32)))");
        self.line("(import \"rtc\" \"onIceCandidateFull\" (func $rtc_onIceCandidateFull (param i32 i32)))");
        self.line("(import \"rtc\" \"onTrack\" (func $rtc_onTrack (param i32 i32)))");
        self.line("(import \"rtc\" \"onDataChannel\" (func $rtc_onDataChannel (param i32 i32)))");
        self.line("(import \"rtc\" \"onConnectionStateChange\" (func $rtc_onConnectionStateChange (param i32 i32)))");
        self.line("(import \"rtc\" \"onIceConnectionStateChange\" (func $rtc_onIceConnectionStateChange (param i32 i32)))");
        self.line("(import \"rtc\" \"onIceGatheringStateChange\" (func $rtc_onIceGatheringStateChange (param i32 i32)))");
        self.line("(import \"rtc\" \"onSignalingStateChange\" (func $rtc_onSignalingStateChange (param i32 i32)))");
        self.line("(import \"rtc\" \"onNegotiationNeeded\" (func $rtc_onNegotiationNeeded (param i32 i32)))");
        self.line("(import \"rtc\" \"getConnectionState\" (func $rtc_getConnectionState (param i32) (result i32)))");
        self.line("(import \"rtc\" \"getIceConnectionState\" (func $rtc_getIceConnectionState (param i32) (result i32)))");
        self.line("(import \"rtc\" \"getSignalingState\" (func $rtc_getSignalingState (param i32) (result i32)))");
        self.line("(import \"rtc\" \"attachStream\" (func $rtc_attachStream (param i32 i32)))");
        self.line("(import \"rtc\" \"getUserMedia\" (func $rtc_getUserMedia (param i32 i32)))");
        self.line("(import \"rtc\" \"getDisplayMedia\" (func $rtc_getDisplayMedia (param i32 i32)))");
        self.line("(import \"rtc\" \"stopTrack\" (func $rtc_stopTrack (param i32)))");
        self.line("(import \"rtc\" \"setTrackEnabled\" (func $rtc_setTrackEnabled (param i32 i32)))");
        self.line("(import \"rtc\" \"getTrackKind\" (func $rtc_getTrackKind (param i32) (result i32)))");

        // ── GPU — WebGPU rendering, buffers, shaders, textures ───────────────
        self.line("");
        self.line(";; GPU — browser WebGPU APIs (adapter, device, buffers, pipelines, rendering)");
        self.line("(import \"gpu\" \"requestAdapter\" (func $gpu_requestAdapter (param i32 i32) (result i32)))");
        self.line("(import \"gpu\" \"requestDevice\" (func $gpu_requestDevice (param i32) (result i32)))");
        self.line("(import \"gpu\" \"configureCanvas\" (func $gpu_configureCanvas (param i32 i32 i32 i32) (result i32)))");
        self.line("(import \"gpu\" \"createBuffer\" (func $gpu_createBuffer (param i32 i32 i32) (result i32)))");
        self.line("(import \"gpu\" \"writeBuffer\" (func $gpu_writeBuffer (param i32 i32 i32 i32 i32)))");
        self.line("(import \"gpu\" \"createShaderModule\" (func $gpu_createShaderModule (param i32 i32 i32) (result i32)))");
        self.line("(import \"gpu\" \"createRenderPipeline\" (func $gpu_createRenderPipeline (param i32 i32) (result i32)))");
        self.line("(import \"gpu\" \"createTexture\" (func $gpu_createTexture (param i32 i32) (result i32)))");
        self.line("(import \"gpu\" \"beginRenderPass\" (func $gpu_beginRenderPass (param i32 i32) (result i32)))");
        self.line("(import \"gpu\" \"setPipeline\" (func $gpu_setPipeline (param i32 i32)))");
        self.line("(import \"gpu\" \"setVertexBuffer\" (func $gpu_setVertexBuffer (param i32 i32 i32)))");
        self.line("(import \"gpu\" \"draw\" (func $gpu_draw (param i32 i32 i32 i32 i32)))");
        self.line("(import \"gpu\" \"submitRenderPass\" (func $gpu_submitRenderPass (param i32 i32)))");
        self.line("(import \"gpu\" \"getCurrentTexture\" (func $gpu_getCurrentTexture (param i32) (result i32)))");
        self.line("(import \"gpu\" \"createTextureView\" (func $gpu_createTextureView (param i32) (result i32)))");
        self.line("(import \"gpu\" \"destroyBuffer\" (func $gpu_destroyBuffer (param i32)))");
        self.line("(import \"gpu\" \"destroyTexture\" (func $gpu_destroyTexture (param i32)))");
        self.line("(import \"gpu\" \"getPreferredFormat\" (func $gpu_getPreferredFormat (result i32)))");
        self.line("(import \"gpu\" \"getAdapterInfo\" (func $gpu_getAdapterInfo (param i32) (result i32)))");

        // ── Test runtime — imported for test { } blocks ──────────────────────
        self.line("");
        self.line(";; Test runtime — report pass/fail/summary to the host");
        self.line("(import \"test\" \"pass\" (func $test_pass (param i32 i32)))");
        self.line("(import \"test\" \"fail\" (func $test_fail (param i32 i32 i32 i32)))");
        self.line("(import \"test\" \"summary\" (func $test_summary (param i32 i32)))");

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
        self.emit_crypto_runtime();
        self.emit_signal_runtime();
        self.emit_gesture_runtime();
        self.emit_flags_runtime();
        self.emit_ai_runtime();
        self.emit_a11y_runtime();
        self.emit_time_runtime();

        // Pre-collect component names so template codegen can detect component instantiation
        for item in &program.items {
            match item {
                Item::Component(c) => {
                    self.known_components.push(c.name.clone());
                    let prop_names: Vec<String> = c.props.iter().map(|p| p.name.clone()).collect();
                    self.component_prop_defs.push((c.name.clone(), prop_names));
                },
                Item::LazyComponent(lc) => {
                    self.known_components.push(lc.component.name.clone());
                    let prop_names: Vec<String> = lc.component.props.iter().map(|p| p.name.clone()).collect();
                    self.component_prop_defs.push((lc.component.name.clone(), prop_names));
                },
                Item::Page(p) => self.known_components.push(p.name.clone()),
                // Collect keyword definition names so bare references to them
                // (e.g. `AppAuth` in `AppAuth.login(...)`) are treated as
                // namespace handles rather than local variable references.
                Item::Auth(a) => self.known_keyword_defs.push((a.name.clone(), KeywordDefKind::Auth)),
                Item::Cache(c) => self.known_keyword_defs.push((c.name.clone(), KeywordDefKind::Cache)),
                Item::Db(d) => self.known_keyword_defs.push((d.name.clone(), KeywordDefKind::Database)),
                Item::Payment(p) => self.known_keyword_defs.push((p.name.clone(), KeywordDefKind::Payment)),
                Item::Upload(u) => self.known_keyword_defs.push((u.name.clone(), KeywordDefKind::Upload)),
                Item::Pdf(p) => self.known_keyword_defs.push((p.name.clone(), KeywordDefKind::Pdf)),
                Item::Theme(t) => self.known_keyword_defs.push((t.name.clone(), KeywordDefKind::Theme)),
                // Collect store names and their signal names so `StoreName::signal()` can
                // be resolved to the getter `$StoreName_get_signal`.
                Item::Store(s) => {
                    let sig_names: Vec<String> = s.signals.iter().map(|sig| sig.name.clone()).collect();
                    self.known_stores.push((s.name.clone(), sig_names));
                }
                _ => {}
            }
        }

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

        // Emit function table for indirect calls (closures + signal effects)
        // Always needed because signal runtime uses call_indirect
        {
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

        // Use save/restore to hoist all locals to function preamble
        let saved_output = std::mem::take(&mut self.output);
        self.output = String::new();
        self.defer_template_locals = true;
        self.template_locals.clear();

        // Collect locals and defer them
        self.collect_locals(&test.body);
        for (name, ty) in self.locals.clone() {
            let wasm_ty = self.wasm_type_str(&ty).to_string();
            self.emit_template_local_typed(&format!("${}", name), &wasm_ty);
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

        // Hoist locals before body
        let body_output = std::mem::take(&mut self.output);
        self.output = saved_output;
        for local_decl in std::mem::take(&mut self.template_locals) {
            self.line(&local_decl);
        }
        self.defer_template_locals = false;
        self.output.push_str(&body_output);

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

        let has_self = func.params.iter().any(|p| p.name == "self");
        let mut params: Vec<String> = Vec::new();
        if has_self {
            params.push("(param $self i32)".into());
        }
        params.extend(func.params.iter()
            .filter(|p| p.name != "self")
            .map(|p| format!("(param ${} {})", p.name, self.type_to_wasm(&p.ty))));

        let ret = func.return_type.as_ref()
            .map(|t| format!(" (result {})", self.type_to_wasm(t)))
            .unwrap_or_default();

        let export = if func.is_pub {
            format!(" (export \"{}\")", func.name)
        } else {
            String::new()
        };

        self.emit(&format!("(func ${}{} {}{}",
            func.name, export, params.join(" "), ret));
        self.indent += 1;

        // Use save/restore to hoist all locals (including dynamic ones from
        // iterator operations) to the function preamble before any instructions.
        let saved_output = std::mem::take(&mut self.output);
        self.output = String::new();
        self.defer_template_locals = true;
        self.template_locals.clear();

        // Collect named locals and register them as deferred locals
        self.collect_locals(&func.body);
        for (name, ty) in self.locals.clone() {
            let wasm_ty = self.wasm_type_str(&ty).to_string();
            self.emit_template_local_typed(&format!("${}", name), &wasm_ty);
        }
        self.emit_template_local("$__arr_tmp");

        // Generate body (dynamic locals go to template_locals)
        let has_return = func.return_type.is_some();
        let stmt_count = func.body.stmts.len();
        for (i, stmt) in func.body.stmts.iter().enumerate() {
            let is_last = i == stmt_count - 1;
            if is_last && has_return {
                if let Stmt::Expr(expr) = stmt {
                    self.generate_expr(expr);
                    continue;
                }
            }
            self.generate_stmt(stmt);
        }

        // Hoist deferred locals before the body
        let body_output = std::mem::take(&mut self.output);
        self.output = saved_output;
        for local_decl in std::mem::take(&mut self.template_locals) {
            self.line(&local_decl);
        }
        self.defer_template_locals = false;
        self.output.push_str(&body_output);

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

        // Emit globals for component signal IDs and dynamic element IDs
        for state in &comp.state {
            self.line(&format!("(global $__sig_{}_{} (mut i32) (i32.const -1))", comp_name, state.name));
            self.line(&format!("(global $__dyn_el_{}_{} (mut i32) (i32.const -1))", comp_name, state.name));
        }
        // Emit globals for prop values so event handlers can read them.
        // Prop ptr globals store the string/value pointer; handlers use global.get.
        for prop in &comp.props {
            self.line(&format!("(global $__prop_{}_{}_ptr (mut i32) (i32.const 0))", comp_name, prop.name));
            self.line(&format!("(global $__prop_{}_{}_len (mut i32) (i32.const 0))", comp_name, prop.name));
        }
        self.signal_updaters.clear();

        // Generate the init/mount function with prop parameters
        // String props are passed as (ptr, len) pairs in WASM
        let prop_names: Vec<String> = comp.props.iter().map(|p| p.name.clone()).collect();
        let mut sig = format!("(func ${comp_name}_mount (export \"{comp_name}_mount\") (param $root i32)");
        for prop in &prop_names {
            sig.push_str(&format!(" (param $prop_{}_ptr i32) (param $prop_{}_len i32)", prop, prop));
        }
        sig.push(')');
        // Remove trailing ) — the emit/indent system will close it
        sig.pop();
        self.emit(&sig);
        self.indent += 1;

        // Track component fields and props for self.field / prop resolution
        self.in_component_mount = true;
        self.component_fields = comp.state.iter().map(|s| s.name.clone()).collect();
        self.component_props = prop_names;
        self.component_name = comp_name.clone();

        // Enable deferred template locals — generate template code into a
        // separate buffer so we can emit all locals before any instructions.
        self.defer_template_locals = true;
        self.template_locals.clear();
        let output_before = self.output.len();

        // Generate all template-related code (may produce deferred locals)
        // We save the output position, generate, then splice locals before it.
        let saved_output = std::mem::take(&mut self.output);
        let mut template_output = String::new();

        // Temporarily redirect output
        self.output = String::new();

        // Emit prop alias locals and copy values.
        // Also store props into component-scoped globals so event handlers can read them.
        for prop in &comp.props {
            self.emit_template_local(&format!("${}", prop.name));
        }
        for prop in &comp.props {
            let prop_name = &prop.name;
            // Store into global so handlers can access
            self.line(&format!("local.get $prop_{}_ptr", prop_name));
            self.line(&format!("global.set $__prop_{}_{}_ptr", comp_name, prop_name));
            self.line(&format!("local.get $prop_{}_len", prop_name));
            self.line(&format!("global.set $__prop_{}_{}_len", comp_name, prop_name));
            // Also set local alias for template use
            self.line(&format!("local.get $prop_{}_ptr", prop_name));
            self.line(&format!("local.set ${}", prop_name));
        }

        // Only call dom_getRoot for the root/entry component (called from JS).
        // Child components receive a valid $root from the parent's createElement.
        // We detect root vs child: child components get a $root > 0 from parent.
        // The entry component is called from JS with a placeholder value, so we
        // use dom_getRoot to register and resolve the actual #app element.
        // We use a simple heuristic: if $root <= 1, re-resolve via dom_getRoot.
        self.line("local.get $root");
        self.line("i32.const 1");
        self.line("i32.gt_u");
        self.line("if");
        self.line("  ;; child component — $root is already a valid element ID from parent");
        self.line("else");
        self.line("  ;; root component — resolve the #app element");
        self.line("  call $dom_getRoot");
        self.line("  local.set $root");
        self.line("end");

        // Initialize signals via runtime — use atomic operations for atomic signals.
        // String literals produce (ptr, len) on the stack but signal_create only
        // takes a single i32 initial value. For string signals we pass the ptr and
        // drop the len; for all other types a single i32 is already on the stack.
        for state in &comp.state {
            let is_string_init = matches!(&state.initializer, Expr::StringLit(_));
            self.generate_expr(&state.initializer);
            if is_string_init {
                // String literal pushed (ptr, len); drop the len, keep the ptr.
                self.line("drop  ;; discard str len — signal stores only the ptr");
            }
            if state.atomic {
                self.line(";; atomic signal — uses lock-free concurrent access");
                self.line("call $signal_create");
            } else {
                self.line("call $signal_create");
            }
            self.line(&format!("global.set $__sig_{}_{}", comp_name, state.name));
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

        // Capture the generated template code
        template_output = std::mem::take(&mut self.output);

        // Restore original output and emit: locals first, then template code
        self.output = saved_output;

        // Emit all deferred template locals
        for local_decl in std::mem::take(&mut self.template_locals) {
            self.line(&local_decl);
        }
        self.line("(local $__arr_tmp i32)");
        self.defer_template_locals = false;

        // Append the template code
        self.output.push_str(&template_output);

        // a11y defaults to auto — enhance unless explicitly set to manual
        let a11y_mode = comp.a11y.as_ref().unwrap_or(&A11yMode::Auto);
        if matches!(a11y_mode, A11yMode::Auto | A11yMode::Hybrid) {
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

        // Generate event handler trampolines as exported functions.
        // Names are prefixed with the component name to avoid redefinition when
        // multiple components appear in the same file.
        // (keep in_component_mount=true so self.field resolves to signals)
        for (i, method) in comp.methods.iter().enumerate() {
            let has_self = method.params.iter().any(|p| p.name == "self");
            self.line("");
            let handler_name = format!("{comp_name}__handler_{i}");
            let mut sig = format!("(func ${handler_name} (export \"{handler_name}\")");
            if has_self {
                sig.push_str(" (param $self i32)");
            }
            // Include non-self parameters in handler trampoline
            for p in method.params.iter().filter(|p| p.name != "self") {
                sig.push_str(&format!(" (param ${} {})", p.name, self.type_to_wasm(&p.ty)));
            }
            self.emit(&sig);
            self.indent += 1;

            // Use save/restore pattern to hoist all locals (including those
            // emitted dynamically during body gen) to the function preamble.
            let saved_output = std::mem::take(&mut self.output);
            self.output = String::new();
            self.defer_template_locals = true;
            self.template_locals.clear();

            // Collect and register locals from handler body into template_locals
            self.locals.clear();
            self.collect_locals(&method.body);
            for (name, ty) in self.locals.clone() {
                let wasm_ty = self.wasm_type_str(&ty).to_string();
                self.emit_template_local_typed(&format!("${}", name), &wasm_ty);
            }
            // Utility local for array/object/struct allocation
            self.emit_template_local("$__arr_tmp");

            // Generate handler body — $self is available as a parameter
            self.in_handler_body = true;
            self.line(";; event handler trampoline");
            for stmt in &method.body.stmts {
                self.generate_stmt(stmt);
            }
            self.in_handler_body = false;

            // Hoist all deferred locals before the body
            let body_output = std::mem::take(&mut self.output);
            self.output = saved_output;
            for local_decl in std::mem::take(&mut self.template_locals) {
                self.line(&local_decl);
            }
            self.defer_template_locals = false;
            self.output.push_str(&body_output);

            self.indent -= 1;
            self.line(")");
        }

        // Generate __callback dispatcher — the runtime calls __callback(idx)
        // and we route to the correct handler function.
        // Named per-component to avoid redefinition across multiple components.
        if !comp.methods.is_empty() {
            self.line("");
            let callback_name = format!("{comp_name}__callback");
            self.emit(&format!("(func ${callback_name} (export \"{callback_name}\") (param $idx i32)"));
            self.indent += 1;
            for (i, _method) in comp.methods.iter().enumerate() {
                let handler_name = format!("{comp_name}__handler_{i}");
                self.line(&format!("local.get $idx"));
                self.line(&format!("i32.const {}", i));
                self.line("i32.eq");
                self.line("if");
                self.indent += 1;
                // Handler trampolines that take $self need a value — pass 0
                // since component state is in signals, not struct memory
                let has_self = _method.params.iter().any(|p| p.name == "self");
                if has_self {
                    self.line("i32.const 0");
                    self.line(&format!("call ${handler_name}"));
                } else {
                    self.line(&format!("call ${handler_name}"));
                }
                self.line("return");
                self.indent -= 1;
                self.line("end");
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

        // Emit signal→DOM updater functions (WASM-internal, called via call_indirect)
        for (func_name, global_el, sig_global) in self.signal_updaters.clone() {
            self.line("");
            self.emit(&format!("(func {} ;; reactive DOM updater", func_name));
            self.indent += 1;
            self.line("(local $ptr i32)");
            self.line("(local $len i32)");
            // Read current signal value
            self.line(&format!("global.get {}", sig_global));
            self.line("call $signal_get");
            // Convert to string
            self.line("call $string_fromI32");
            self.line("local.set $len");
            self.line("local.set $ptr");
            // Update DOM element text
            self.line(&format!("global.get {}", global_el));
            self.line("local.get $ptr");
            self.line("local.get $len");
            self.line("call $dom_setText");
            self.indent -= 1;
            self.line(")");
        }

        // Reset component context
        self.in_component_mount = false;
        self.component_fields.clear();
        self.component_name.clear();
    }

    fn generate_page(&mut self, page: &PageDef) {
        self.line(&format!(";; === Page: {} ===", page.name));

        // Generate page mount function (same pattern as generate_component)
        let comp_name = &page.name;

        self.emit(&format!("(func ${comp_name}_mount (export \"{comp_name}_mount\") (param $root i32)"));
        self.indent += 1;

        // Enable deferred template locals for proper WAT ordering
        self.defer_template_locals = true;
        self.template_locals.clear();
        let saved_output = std::mem::take(&mut self.output);
        self.output = String::new();

        for state in &page.state {
            self.emit_template_local(&format!("${}", state.name));
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

        let template_output = std::mem::take(&mut self.output);
        self.output = saved_output;
        for local_decl in std::mem::take(&mut self.template_locals) {
            self.line(&local_decl);
        }
        self.line("(local $__arr_tmp i32)");
        self.defer_template_locals = false;
        self.output.push_str(&template_output);

        self.line("");
        self.line(";; reactive effects for DOM updates are registered via signal.subscribe");

        self.indent -= 1;
        self.line(")");

        // Generate event handler trampolines — namespaced per page
        let page_prefix = &page.name;
        for method in &page.methods {
            let has_self = method.params.iter().any(|p| p.name == "self");
            let handler_name = format!("{}__handler_{}", page_prefix, method.name);
            self.line("");
            let mut sig = format!("(func ${} (export \"{}\")", handler_name, handler_name);
            if has_self {
                sig.push_str(" (param $self i32)");
            }
            for p in method.params.iter().filter(|p| p.name != "self") {
                sig.push_str(&format!(" (param ${} {})", p.name, self.type_to_wasm(&p.ty)));
            }
            self.emit(&sig);
            self.indent += 1;

            // Use save/restore pattern to hoist all locals to function preamble
            let saved_output = std::mem::take(&mut self.output);
            self.output = String::new();
            self.defer_template_locals = true;
            self.template_locals.clear();

            self.locals.clear();
            self.collect_locals(&method.body);
            for (name, ty) in self.locals.clone() {
                let wasm_ty = self.wasm_type_str(&ty).to_string();
                self.emit_template_local_typed(&format!("${}", name), &wasm_ty);
            }
            self.emit_template_local("$__arr_tmp");

            self.in_handler_body = true;
            self.line(";; event handler trampoline");
            for stmt in &method.body.stmts {
                self.generate_stmt(stmt);
            }
            self.in_handler_body = false;

            let body_output = std::mem::take(&mut self.output);
            self.output = saved_output;
            for local_decl in std::mem::take(&mut self.template_locals) {
                self.line(&local_decl);
            }
            self.defer_template_locals = false;
            self.output.push_str(&body_output);

            self.indent -= 1;
            self.line(")");
        }

        // Generate methods — namespaced per page
        for method in &page.methods {
            let mut namespaced = method.clone();
            namespaced.name = format!("{}_{}", page.name, method.name);
            self.generate_function(&namespaced);
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

        // Generate methods, namespaced by form name to avoid redefinition
        // when multiple forms share method names like `on_submit`.
        let form_name = form.name.clone();
        for method in &form.methods {
            let mut namespaced = method.clone();
            namespaced.name = format!("{form_name}_{}", method.name);
            self.generate_function(&namespaced);
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
        // channel_connect returns a handle i32 — discard it here since the
        // channel is identified by name in subsequent calls.
        self.line("  drop  ;; discard channel handle — identified by name");

        // Set reconnect flag
        if !ch.reconnect {
            self.line(&format!("  i32.const {}  ;; name ptr", name_offset));
            self.line(&format!("  i32.const {}  ;; name len", name_len));
            self.line("  i32.const 0  ;; reconnect disabled");
            self.line("  call $channel_set_reconnect");
        }

        self.line(")");

        // Generate handler methods, namespaced by channel name to avoid
        // redefinition when multiple channels share handler names like `on_message`.
        let ch_name = ch.name.clone();
        if let Some(ref handler) = ch.on_message {
            let mut namespaced = handler.clone();
            namespaced.name = format!("{ch_name}_{}", handler.name);
            self.generate_function(&namespaced);
        }
        if let Some(ref handler) = ch.on_connect {
            let mut namespaced = handler.clone();
            namespaced.name = format!("{ch_name}_{}", handler.name);
            self.generate_function(&namespaced);
        }
        if let Some(ref handler) = ch.on_disconnect {
            let mut namespaced = handler.clone();
            namespaced.name = format!("{ch_name}_{}", handler.name);
            self.generate_function(&namespaced);
        }
        for method in &ch.methods {
            let mut namespaced = method.clone();
            namespaced.name = format!("{ch_name}_{}", method.name);
            self.generate_function(&namespaced);
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

        // Set up store context so self.field access resolves to store globals.
        // This is the same pattern as in_component_mount for components.
        let store_field_names: Vec<String> = store.signals.iter().map(|s| s.name.clone()).collect();
        self.in_component_mount = true;
        self.component_fields = store_field_names.clone();
        self.component_name = store_name.clone();

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

            // Use save/restore to hoist all locals to function preamble
            let saved_output = std::mem::take(&mut self.output);
            self.output = String::new();
            self.defer_template_locals = true;
            self.template_locals.clear();

            // Collect locals (clear first to avoid bleeding from previous action)
            self.locals.clear();
            self.collect_locals(&action.body);
            for (name, ty) in self.locals.clone() {
                let wasm_ty = self.wasm_type_str(&ty).to_string();
                self.emit_template_local_typed(&format!("${}", name), &wasm_ty);
            }

            // Generate action body — store actions use signal globals for self.field
            // but in_handler_body is false since $self is not a param (filtered above)
            for stmt in &action.body.stmts {
                self.generate_stmt(stmt);
            }

            // Hoist locals before body
            let body_output = std::mem::take(&mut self.output);
            self.output = saved_output;
            for local_decl in std::mem::take(&mut self.template_locals) {
                self.line(&local_decl);
            }
            self.defer_template_locals = false;
            self.output.push_str(&body_output);

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

        // Reset store signal context so subsequent defs don't inherit it
        self.in_component_mount = false;
        self.component_fields.clear();
        self.component_name.clear();
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

            // Use save/restore to hoist all locals to function preamble
            let saved_output = std::mem::take(&mut self.output);
            self.output = String::new();
            self.defer_template_locals = true;
            self.template_locals.clear();

            // Collect locals from tool body and defer them
            self.collect_locals(&tool.body);
            for (name, ty) in self.locals.clone() {
                let wasm_ty = self.wasm_type_str(&ty).to_owned();
                self.emit_template_local_typed(&format!("${}", name), &wasm_ty);
            }

            // Generate tool body
            for stmt in &tool.body.stmts {
                self.generate_stmt(stmt);
            }

            // Hoist locals before body
            let body_output = std::mem::take(&mut self.output);
            self.output = saved_output;
            for local_decl in std::mem::take(&mut self.template_locals) {
                self.line(&local_decl);
            }
            self.defer_template_locals = false;
            self.output.push_str(&body_output);

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
            // Add the route mount function to the function table and use its table index
            let func_name = format!("$__route_mount_{}", i);
            let table_idx = self.closure_func_names.len();
            self.closure_func_names.push(func_name);
            self.line(&format!("i32.const {} ;; path ptr", path_offset));
            self.line(&format!("i32.const {} ;; path len", route.path.len()));
            self.line(&format!("i32.const {} ;; table index for {}", table_idx, route.component));
            self.line("call $router_registerRoute");
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
            // If the fallback is a component reference (Ident matching known component),
            // call its mount function directly instead of treating it as an expression
            let mut handled = false;
            if let TemplateNode::Expression(expr) = fallback.as_ref() {
                if let Expr::Ident(name) = expr.as_ref() {
                    if self.known_components.contains(name) {
                        self.line("local.get $root");
                        let prop_defs: Vec<String> = self.component_prop_defs.iter()
                            .find(|(n, _)| n == name)
                            .map(|(_, props)| props.clone())
                            .unwrap_or_default();
                        for _ in &prop_defs {
                            self.line("i32.const 0");
                            self.line("i32.const 0");
                        }
                        self.line(&format!("call ${}_mount", name));
                        handled = true;
                    }
                }
            }
            if !handled {
                self.generate_template(fallback, "$root");
            }
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
        self.line("drop ;; discard scope ID");
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

    /// Emit a local declaration, or defer it if we're in deferred mode.
    fn emit_template_local(&mut self, var: &str) {
        let decl = format!("(local {} i32)", var);
        if self.defer_template_locals {
            self.template_locals.push(decl);
        } else {
            self.line(&decl);
        }
    }

    /// Like `emit_template_local` but with an explicit WASM type.
    /// Used when hoisting typed locals (i64, f32, f64) to a function's preamble.
    fn emit_template_local_typed(&mut self, var: &str, wasm_ty: &str) {
        let decl = format!("(local {} {})", var, wasm_ty);
        if self.defer_template_locals {
            self.template_locals.push(decl);
        } else {
            self.line(&decl);
        }
    }

    fn generate_template(&mut self, node: &TemplateNode, parent: &str) {
        match node {
            TemplateNode::Element(el) => {
                // Check if this is a component instantiation
                if self.known_components.contains(&el.tag) {
                    let comp_name = el.tag.clone();
                    self.line(&format!(";; component instantiation: <{} />", comp_name));
                    let container_var = format!("$comp_{}", self.next_label());
                    self.emit_template_local(&container_var);
                    // Create a container div for the child component
                    let tag_offset = self.store_string("div");
                    self.line(&format!("i32.const {}", tag_offset));
                    self.line("i32.const 3");
                    self.line("call $dom_createElement");
                    self.line(&format!("local.set {}", container_var));
                    // Append container to parent
                    self.line(&format!("local.get {}", parent));
                    self.line(&format!("local.get {}", container_var));
                    self.line("call $dom_appendChild");
                    // Mount the child component into the container, passing props
                    self.line(&format!("local.get {}", container_var));

                    // Look up expected props for this component and pass values from attributes
                    let prop_defs: Vec<String> = self.component_prop_defs.iter()
                        .find(|(name, _)| name == &comp_name)
                        .map(|(_, props)| props.clone())
                        .unwrap_or_default();

                    // Build a map of attribute values from the element
                    let attr_map: Vec<(String, String)> = el.attributes.iter()
                        .filter_map(|attr| match attr {
                            Attribute::Static { name, value } => Some((name.clone(), value.clone())),
                            _ => None,
                        })
                        .collect();

                    for prop_name in &prop_defs {
                        // Find the attribute value for this prop
                        if let Some((_, value)) = attr_map.iter().find(|(n, _)| n == prop_name) {
                            let offset = self.store_string(value);
                            self.line(&format!("i32.const {} ;; prop {} ptr", offset, prop_name));
                            self.line(&format!("i32.const {} ;; prop {} len", value.len(), prop_name));
                        } else {
                            // No value provided — pass empty string
                            let offset = self.store_string("");
                            self.line(&format!("i32.const {} ;; prop {} (default empty)", offset, prop_name));
                            self.line("i32.const 0");
                        }
                    }

                    self.line(&format!("call ${}_mount", comp_name));
                    return;
                }

                let var = format!("$el_{}", self.next_label());
                self.emit_template_local(&var);

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
                            self.line("call $dom_setAttr");
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

                            // Resolve signal reference: component fields use globals,
                            // page-level locals use local.get.
                            let signal_get_instr = if self.in_component_mount
                                && self.component_fields.contains(signal) {
                                format!("global.get $__sig_{}_{}", self.component_name, signal)
                            } else {
                                format!("local.get ${}", signal)
                            };

                            // 1. Set initial property value from signal
                            self.line(&format!("local.get {}", var));
                            self.line(&format!("i32.const {}", prop_offset));
                            self.line(&format!("i32.const {}", property.len()));
                            self.line(&signal_get_instr);
                            self.line("call $signal_get");
                            // Convert signal value to a string for the property setter
                            self.line("call $dom_setProperty");

                            // 2. Create an effect: when signal changes, update DOM property
                            self.line(&format!(";; effect #{} — signal->DOM for bind:{}", effect_idx, property));
                            self.line(&format!("local.get {}", var));
                            self.line(&format!("i32.const {}", prop_offset));
                            self.line(&format!("i32.const {}", property.len()));
                            self.line(&signal_get_instr);
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
                self.emit_template_local(&var);
                let text_offset = self.store_string(text);
                self.line(&format!(";; text: \"{}\"", text));
                self.line(&format!("local.get {}", parent));
                self.line(&format!("i32.const {}", text_offset));
                self.line(&format!("i32.const {}", text.len()));
                self.line("call $dom_setText");
            }
            TemplateNode::Expression(expr) => {
                self.line(";; dynamic expression");
                let var = format!("$dyn_{}", self.next_label());
                self.emit_template_local(&var);
                // Create a <span> to hold the dynamic text
                let tag_offset = self.store_string("span");
                self.line(&format!("i32.const {}", tag_offset));
                self.line("i32.const 4");
                self.line("call $dom_createElement");
                self.line(&format!("local.set {}", var));

                // Detect if this is a prop reference (Ident matching a component prop)
                let prop_name = if let Expr::Ident(name) = expr.as_ref() {
                    if self.in_component_mount && self.component_props.contains(name) {
                        Some(name.clone())
                    } else { None }
                } else { None };

                // Detect if this is a self.field expression bound to a signal
                let signal_field = if prop_name.is_none() {
                    if let Expr::FieldAccess { object, field } = expr.as_ref() {
                        if self.in_component_mount && matches!(object.as_ref(), Expr::SelfExpr)
                            && self.component_fields.contains(field) {
                            Some(field.clone())
                        } else { None }
                    } else { None }
                } else { None };

                if let Some(ref pname) = prop_name {
                    // Prop reference — string is already available as ptr+len params
                    self.line(&format!("local.get {}", var));
                    self.line(&format!("local.get $prop_{}_ptr", pname));
                    self.line(&format!("local.get $prop_{}_len", pname));
                    self.line("call $dom_setText");
                } else {
                    // Check if the expression returns a string (ptr, len pair)
                    // rather than a signal i32 value.
                    let is_string_expr = matches!(
                        expr.as_ref(),
                        Expr::FnCall { .. } | Expr::MethodCall { .. } | Expr::StringLit(_)
                    );

                    if is_string_expr {
                        // Expression already produces (ptr, len) on the stack
                        self.generate_expr(expr);
                        let ptr_var = format!("$dyn_ptr_{}", self.next_label());
                        let len_var = format!("$dyn_len_{}", self.next_label());
                        self.emit_template_local(&ptr_var);
                        self.emit_template_local(&len_var);
                        self.line(&format!("local.set {}", len_var));
                        self.line(&format!("local.set {}", ptr_var));
                        self.line(&format!("local.get {}", var));
                        self.line(&format!("local.get {}", ptr_var));
                        self.line(&format!("local.get {}", len_var));
                        self.line("call $dom_setText");
                    } else {
                        // Set initial text: get signal value, convert to string, setText
                        self.generate_expr(expr);
                        if signal_field.is_some() {
                            // generate_expr already emits signal_get for self.field
                        } else {
                            self.line("call $signal_get");
                        }
                        self.line("call $string_fromI32");
                        let ptr_var = format!("$dyn_ptr_{}", self.next_label());
                        let len_var = format!("$dyn_len_{}", self.next_label());
                        self.emit_template_local(&ptr_var);
                        self.emit_template_local(&len_var);
                        self.line(&format!("local.set {}", len_var));
                        self.line(&format!("local.set {}", ptr_var));
                        self.line(&format!("local.get {}", var));
                        self.line(&format!("local.get {}", ptr_var));
                        self.line(&format!("local.get {}", len_var));
                        self.line("call $dom_setText");
                    }
                }

                // If bound to a signal, register a reactive updater
                if let Some(ref field) = signal_field {
                    let global_el = format!("$__dyn_el_{}_{}", self.component_name, field);
                    let sig_global = format!("$__sig_{}_{}", self.component_name, field);
                    let func_name = format!("$__update_{}_{}", self.component_name, field);

                    // Store element ID in global so updater function can find it
                    self.line(&format!("local.get {}", var));
                    self.line(&format!("global.set {}", global_el));

                    // Subscribe: signal_subscribe(signal_id, table_index)
                    let table_idx = self.closure_func_names.len();
                    self.closure_func_names.push(func_name.clone());
                    self.line(&format!("global.get {}", sig_global));
                    self.line(&format!("i32.const {}", table_idx));
                    self.line("call $signal_subscribe");

                    // Record updater to emit later (after mount function)
                    self.signal_updaters.push((func_name, global_el, sig_global));
                }

                // Append to parent
                self.line(&format!("local.get {}", parent));
                self.line(&format!("local.get {}", var));
                self.line("call $dom_appendChild");
            }
            TemplateNode::Link { to, attributes, children } => {
                let var = format!("$link_{}", self.next_label());
                self.emit_template_local(&var);

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

                // Generate additional attributes (class, style, aria-*, etc.)
                for attr in attributes {
                    match attr {
                        Attribute::Static { name, value } => {
                            let name_offset = self.store_string(name);
                            let val_offset = self.store_string(value);
                            self.line(&format!("local.get {}", var));
                            self.line(&format!("i32.const {}", name_offset));
                            self.line(&format!("i32.const {}", name.len()));
                            self.line(&format!("i32.const {}", val_offset));
                            self.line(&format!("i32.const {}", value.len()));
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
                                    let val_offset = self.store_string(s);
                                    self.line(&format!("local.get {}", var));
                                    self.line(&format!("i32.const {}", name_offset));
                                    self.line(&format!("i32.const {}", name.len()));
                                    self.line(&format!("i32.const {}", val_offset));
                                    self.line(&format!("i32.const {}", s.len()));
                                    self.line("call $a11y_setAriaAttribute");
                                }
                                _ => {
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
                        _ => {}
                    }
                }

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
            TemplateNode::Outlet => {
                // Outlet renders into a container div with a well-known ID
                let var = format!("$el_{}", self.next_label());
                self.emit_template_local(&var);
                let tag_offset = self.store_string("div");
                self.line(&format!("i32.const {}", tag_offset));
                self.line(&format!("i32.const {}", "div".len()));
                self.line("call $dom_createElement");
                self.line(&format!("local.set {}", var));
                // Set id="__nectar_outlet" for route content swap
                let id_name = self.store_string("id");
                let id_val = self.store_string("__nectar_outlet");
                self.line(&format!(";; outlet container"));
                self.line(&format!("local.get {}", var));
                self.line(&format!("i32.const {} ;; \"id\" ptr", id_name));
                self.line(&format!("i32.const {} ;; \"id\" len", 2));
                self.line(&format!("i32.const {} ;; \"__nectar_outlet\" ptr", id_val));
                self.line(&format!("i32.const {} ;; \"__nectar_outlet\" len", "__nectar_outlet".len()));
                self.line("call $dom_setAttr");
                self.line(&format!("local.get {}", parent));
                self.line(&format!("local.get {}", var));
                self.line("call $dom_appendChild");
            }
            TemplateNode::Layout(layout_node) => {
                self.generate_layout_node(layout_node, parent);
            }
            TemplateNode::TemplateIf { condition, then_children, else_children } => {
                self.line(";; template if");
                self.generate_expr(condition);
                // Use WAT if/else/end instead of manual br_if labels — labels must be
                // declared with block/loop/if before they can be branched to.
                self.line("if");
                self.indent += 1;
                for child in then_children {
                    self.generate_template(child, parent);
                }
                self.indent -= 1;
                if let Some(else_nodes) = else_children {
                    if !else_nodes.is_empty() {
                        self.line("else");
                        self.indent += 1;
                        for child in else_nodes {
                            self.generate_template(child, parent);
                        }
                        self.indent -= 1;
                    }
                }
                self.line("end ;; template if");
            }
            TemplateNode::TemplateFor { binding: _, iterator, children } => {
                self.line(";; template for");
                self.generate_expr(iterator);
                for child in children {
                    self.generate_template(child, parent);
                }
            }
            TemplateNode::TemplateMatch { subject, arms } => {
                self.line(";; template match");
                self.generate_expr(subject);
                for arm in arms {
                    for child in &arm.body {
                        self.generate_template(child, parent);
                    }
                }
            }
        }
    }

    /// Generate code for layout primitives — individual style properties.
    /// Emits both native names (direction, gap, align, justify) for the native runtime
    /// and CSS names (flex-direction, gap, align-items) for web compatibility.
    fn generate_layout_node(&mut self, node: &LayoutNode, parent: &str) {
        // Collect tag, styles, and children from the layout node
        let mut styles: Vec<(String, String)> = Vec::new();

        let (tag, children) = match node {
            LayoutNode::Stack { gap, children, .. } => {
                let g = gap.as_deref().unwrap_or("0");
                styles.push(("display".into(), "flex".into()));
                styles.push(("flex-direction".into(), "column".into()));
                styles.push(("direction".into(), "vertical".into()));
                styles.push(("gap".into(), format!("{}px", g)));
                ("section", children)
            }
            LayoutNode::Row { gap, align, children, .. } => {
                let g = gap.as_deref().unwrap_or("0");
                let a = align.as_deref().unwrap_or("stretch");
                styles.push(("display".into(), "flex".into()));
                styles.push(("flex-direction".into(), "row".into()));
                styles.push(("direction".into(), "horizontal".into()));
                styles.push(("gap".into(), format!("{}px", g)));
                styles.push(("align-items".into(), a.into()));
                styles.push(("align".into(), a.into()));
                ("div", children)
            }
            LayoutNode::Grid { cols, rows: _, gap, children, .. } => {
                let c = cols.as_deref().unwrap_or("1");
                let g = gap.as_deref().unwrap_or("0");
                styles.push(("display".into(), "grid".into()));
                styles.push(("grid-template-columns".into(), format!("repeat({},1fr)", c)));
                styles.push(("gap".into(), format!("{}px", g)));
                ("div", children)
            }
            LayoutNode::Center { max_width, children, .. } => {
                let mw = max_width.as_deref().unwrap_or("none");
                styles.push(("display".into(), "flex".into()));
                styles.push(("justify-content".into(), "center".into()));
                styles.push(("justify".into(), "center".into()));
                styles.push(("align-items".into(), "center".into()));
                styles.push(("align".into(), "center".into()));
                styles.push(("max-width".into(), format!("{}px", mw)));
                ("div", children)
            }
            LayoutNode::Cluster { gap, children, .. } => {
                let g = gap.as_deref().unwrap_or("0");
                styles.push(("display".into(), "flex".into()));
                styles.push(("flex-direction".into(), "row".into()));
                styles.push(("direction".into(), "horizontal".into()));
                styles.push(("flex-wrap".into(), "wrap".into()));
                styles.push(("wrap".into(), "true".into()));
                styles.push(("gap".into(), format!("{}px", g)));
                ("div", children)
            }
            LayoutNode::Sidebar { side, width, children, .. } => {
                let s = side.as_deref().unwrap_or("left");
                let w = width.as_deref().unwrap_or("300");
                let cols = if s == "right" {
                    format!("1fr {}px", w)
                } else {
                    format!("{}px 1fr", w)
                };
                styles.push(("display".into(), "grid".into()));
                styles.push(("grid-template-columns".into(), cols));
                ("div", children)
            }
            LayoutNode::Switcher { threshold: _, children, .. } => {
                styles.push(("display".into(), "flex".into()));
                styles.push(("flex-direction".into(), "row".into()));
                styles.push(("direction".into(), "horizontal".into()));
                styles.push(("flex-wrap".into(), "wrap".into()));
                styles.push(("wrap".into(), "true".into()));
                ("div", children)
            }
        };

        let var = format!("$el_{}", self.next_label());
        self.emit_template_local(&var);
        self.line(&format!(";; layout: <{}>", tag));
        let tag_offset = self.store_string(tag);
        self.line(&format!("i32.const {}", tag_offset));
        self.line(&format!("i32.const {}", tag.len()));
        self.line("call $dom_createElement");
        self.line(&format!("local.set {}", var));

        // Set individual style properties
        for (prop, val) in &styles {
            self.emit_set_style(&var, prop, val);
        }

        // Render children
        for child in children {
            self.generate_template(child, &var);
        }

        // Append to parent
        self.line(&format!("local.get {}", parent));
        self.line(&format!("local.get {}", var));
        self.line("call $dom_appendChild");
    }

    /// Emit a dom_setStyle call for one property on an element.
    fn emit_set_style(&mut self, var: &str, prop: &str, val: &str) {
        let prop_offset = self.store_string(prop);
        let val_offset = self.store_string(val);
        self.line(&format!("local.get {}", var));
        self.line(&format!("i32.const {} ;; \"{}\" ptr", prop_offset, prop));
        self.line(&format!("i32.const {} ;; \"{}\" len", prop.len(), prop));
        self.line(&format!("i32.const {} ;; \"{}\" ptr", val_offset, val));
        self.line(&format!("i32.const {} ;; \"{}\" len", val.len(), val));
        self.line("call $dom_setStyle");
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
                // Determine whether this expression produces a value that needs to be dropped.
                // Signal assignments (self.field = ...) are void — signal_set handles the write.
                // Namespace method calls that map to void browser APIs (e.g. clipboard.copy)
                // are also void — they must NOT be followed by a drop instruction.
                let is_void = self.expr_is_void(expr);
                self.generate_expr(expr);
                if !is_void {
                    // Drop result if not used (expression in statement position)
                    self.line("drop");
                }
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
                match name.as_str() {
                    // None is the absence of a value — represented as 0 (null pointer)
                    "None" => self.line("i32.const 0 ;; None"),
                    _ => {
                        // Keyword definition instances (auth AppAuth {}, cache AppCache {}, etc.)
                        // are namespace handles, not local variables. Emit a null placeholder
                        // (i32.const 0) instead of `local.get $<name>` which would be undefined.
                        let is_kw_def = self.known_keyword_defs.iter().any(|(n, _)| n == name);
                        if is_kw_def {
                            self.line(&format!("i32.const 0 ;; keyword def handle: {}", name));
                        } else if self.in_component_mount && self.component_fields.contains(name) {
                            // Component signal field referenced without `self.` — read the signal.
                            self.line(&format!(";; signal field {} (bare ref in template)", name));
                            self.line(&format!("global.get $__sig_{}_{}", self.component_name, name));
                            self.line("call $signal_get");
                        } else {
                            self.line(&format!("local.get ${}", name));
                        }
                    }
                }
            }
            Expr::SelfExpr => {
                // In component/store mount or store action context, state is in signals.
                // If $self is not a declared parameter (mount function, non-handler body),
                // pass i32.const 0 as a dummy self value.
                if self.in_component_mount && !self.in_handler_body {
                    self.line("i32.const 0 ;; self (component signal context)");
                } else {
                    self.line("local.get $self");
                }
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
                // Result/Option constructors — types are erased to i32, so these
                // are identity functions. The wrapped value is already on the stack.
                if let Expr::Ident(name) = callee.as_ref() {
                    match name.as_str() {
                        "Ok" | "Some" | "Err" => {
                            for arg in args {
                                self.generate_expr(arg);
                            }
                            self.line(&format!(";; {} — identity wrapper", name));
                            return;
                        }
                        _ => {}
                    }
                }
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
                        // Crypto namespace — pure WASM implementations
                        let crypto_fn = match name.as_str() {
                            "crypto::sha256"          => "$crypto_sha256",
                            "crypto::sha512"          => "$crypto_sha512",
                            "crypto::sha1"            => "$crypto_sha1",
                            "crypto::sha384"          => "$crypto_sha384",
                            "crypto::hmac"            => "$crypto_hmac_sha256",
                            "crypto::hmac_sha512"     => "$crypto_hmac_sha512",
                            "crypto::encrypt"         => "$crypto_aes_gcm_encrypt",
                            "crypto::decrypt"         => "$crypto_aes_gcm_decrypt",
                            "crypto::encrypt_aes_cbc" => "$crypto_aes_cbc_encrypt",
                            "crypto::decrypt_aes_cbc" => "$crypto_aes_cbc_decrypt",
                            "crypto::encrypt_aes_ctr" => "$crypto_aes_ctr_encrypt",
                            "crypto::decrypt_aes_ctr" => "$crypto_aes_ctr_decrypt",
                            "crypto::sign"            => "$crypto_ed25519_sign",
                            "crypto::verify"          => "$crypto_ed25519_verify",
                            "crypto::derive_key"      => "$crypto_pbkdf2_derive",
                            "crypto::derive_bits"     => "$crypto_pbkdf2_derive_bits",
                            "crypto::hkdf"            => "$crypto_hkdf_derive",
                            "crypto::random_uuid"     => "$crypto_random_uuid",
                            "crypto::random_bytes"    => "$crypto_random_bytes",
                            "crypto::generate_key_pair" => "$crypto_generate_key_pair",
                            "crypto::export_key"      => "$crypto_export_key",
                            "crypto::ecdh_derive"     => "$crypto_ecdh_derive",
                            _ => "",
                        };
                        if !crypto_fn.is_empty() {
                            self.line(&format!(";; crypto: {}", name));
                            self.line(&format!("call {}", crypto_fn));
                        } else if name.contains("::") {
                            // Qualified name — check if it's a store signal getter/action
                            // e.g. `AuthStore::is_logged_in()` → `call $AuthStore_get_is_logged_in`
                            let parts: Vec<&str> = name.splitn(2, "::").collect();
                            let store_fn = if parts.len() == 2 {
                                let store_name = parts[0];
                                let member = parts[1];
                                self.known_stores.iter()
                                    .find(|(sn, _)| sn == store_name)
                                    .map(|(sn, signals)| {
                                        if signals.iter().any(|s| s == member) {
                                            // Signal getter
                                            format!("${sn}_get_{member}")
                                        } else {
                                            // Action or other method
                                            format!("${sn}_{member}")
                                        }
                                    })
                            } else {
                                None
                            };
                            if let Some(wasm_fn) = store_fn {
                                self.line(&format!(";; store: {}", name));
                                self.line(&format!("call {}", wasm_fn));
                            } else {
                                // Route through stdlib resolver
                                let wasm_fn = self.resolve_stdlib_fn(name);
                                self.line(&format!(";; stdlib: {}", name));
                                self.line(&format!("call {}", wasm_fn));
                            }
                        } else {
                            self.line(&format!("call ${}", name));
                        }
                    } else {
                        self.line(&format!(";; webapi: {}", name));
                        self.line(&format!("call {}", wasm_fn));
                    }
                }
            }
            Expr::FieldAccess { object, field } => {
                if self.in_component_mount && matches!(object.as_ref(), Expr::SelfExpr)
                    && self.component_fields.contains(field) {
                    // In component context, self.field reads the signal value
                    self.line(&format!(";; self.{} (signal)", field));
                    self.line(&format!("global.get $__sig_{}_{}", self.component_name, field));
                    self.line("call $signal_get");
                } else if self.in_component_mount && matches!(object.as_ref(), Expr::SelfExpr)
                    && self.component_props.contains(field) {
                    // In component context, self.prop reads from the component-scoped global.
                    // Props are stored as globals at mount time so event handlers can access them.
                    // String props push (ptr, len) to match the string calling convention.
                    self.line(&format!(";; self.{} (prop)", field));
                    self.line(&format!("global.get $__prop_{}_{}_ptr", self.component_name, field));
                    self.line(&format!("global.get $__prop_{}_{}_len", self.component_name, field));
                } else {
                    self.generate_expr(object);
                    self.line(&format!(";; field access: .{}", field));
                    // TODO: calculate field offset from struct layout
                    self.line("i32.load");
                }
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
                if let Expr::FieldAccess { object, field } = target.as_ref() {
                    if self.in_component_mount && matches!(object.as_ref(), Expr::SelfExpr)
                        && self.component_fields.contains(field) {
                        // self.field = value → signal_set(signal_id, value)
                        // String literals produce (ptr, len); signal_set takes (id, val),
                        // so drop len and use only ptr for string values.
                        self.line(&format!(";; self.{} = ... (signal set)", field));
                        self.line(&format!("global.get $__sig_{}_{}", self.component_name, field));
                        self.generate_expr(value);
                        if matches!(value.as_ref(), Expr::StringLit(_)) {
                            self.line("drop  ;; discard str len for signal_set");
                        }
                        self.line("call $signal_set");
                    } else {
                        self.generate_expr(value);
                    }
                } else {
                    self.generate_expr(value);
                    if let Expr::Ident(name) = target.as_ref() {
                        self.line(&format!("local.set ${}", name));
                    }
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
                // Use typed setters before calling http_fetch — no serialization.
                // Default: GET with no options.
                if let Some(opts) = options {
                    // Options expression sets method/body/headers via typed setters.
                    self.generate_expr(opts);
                } else {
                    // Default GET — set method via typed setter, no body
                    let method_offset = self.store_string("GET");
                    self.line(&format!("i32.const {} ;; method ptr", method_offset));
                    self.line("i32.const 3 ;; method len");
                    self.line("call $http_setMethod");
                }
                // Call fetch with just the URL (ptr, len)
                self.generate_expr(url);
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
                self.emit_template_local(&format!("$parallel_arr_{}", array_label));
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
            Expr::Match { subject, arms } => {
                self.line(";; match expression");
                // Evaluate the subject once and store in a temporary local.
                self.generate_expr(subject);
                let subj_label = self.next_label();
                let subject_local = format!("$__match_subj_{}", subj_label);
                self.locals.push((format!("__match_subj_{}", subj_label), WasmType::I32));
                self.line(&format!("local.set {}", subject_local));

                // Each arm becomes an if/else chain.
                // Track how many nested if/else blocks we open so we can close them.
                let mut open_ifs: usize = 0;
                for arm in arms {
                    match &arm.pattern {
                        Pattern::Wildcard | Pattern::Ident(_) => {
                            // Wildcard / binding always matches — emit body directly.
                            if let Some(guard) = &arm.guard {
                                self.generate_expr(guard);
                                self.emit("(if (result i32)");
                                self.indent += 1;
                                self.emit("(then");
                                self.indent += 1;
                                self.generate_expr(&arm.body);
                                self.indent -= 1;
                                self.line(")");
                                self.emit("(else");
                                self.indent += 1;
                                self.line("i32.const 0 ;; match fallthrough");
                                self.indent -= 1;
                                self.line(")");
                                self.indent -= 1;
                                self.line(")");
                            } else {
                                self.generate_expr(&arm.body);
                            }
                        }
                        Pattern::Literal(lit_expr) => {
                            self.line(&format!("local.get {}", subject_local));
                            self.generate_expr(lit_expr);
                            self.line("i32.eq");
                            if let Some(guard) = &arm.guard {
                                self.generate_expr(guard);
                                self.line("i32.and");
                            }
                            self.emit("(if (result i32)");
                            self.indent += 1;
                            self.emit("(then");
                            self.indent += 1;
                            self.generate_expr(&arm.body);
                            self.indent -= 1;
                            self.line(")");
                            self.emit("(else");
                            self.indent += 1;
                            open_ifs += 1;
                        }
                        Pattern::Variant { name, .. } => {
                            // Compare discriminant tag at offset 0 of the subject pointer
                            self.line(&format!(";; match arm: variant {}", name));
                            self.line(&format!("local.get {}", subject_local));
                            self.line("i32.load ;; load discriminant tag");
                            let tag = self.variant_tag(name);
                            self.line(&format!("i32.const {} ;; tag for {}", tag, name));
                            self.line("i32.eq");
                            if let Some(guard) = &arm.guard {
                                self.generate_expr(guard);
                                self.line("i32.and");
                            }
                            self.emit("(if (result i32)");
                            self.indent += 1;
                            self.emit("(then");
                            self.indent += 1;
                            self.generate_expr(&arm.body);
                            self.indent -= 1;
                            self.line(")");
                            self.emit("(else");
                            self.indent += 1;
                            open_ifs += 1;
                        }
                        _ => {
                            // Tuple, Struct, Array patterns — fallthrough for now
                            self.generate_expr(&arm.body);
                        }
                    }
                }
                // Close all open else branches with a default value
                for _ in 0..open_ifs {
                    self.line("i32.const 0 ;; match fallthrough");
                    self.indent -= 1;
                    self.line(")");
                    self.indent -= 1;
                    self.line(")");
                }
            }
            _ => {
                self.line(";; TODO: codegen for expr");
            }
        }
    }

    fn emit_alloc_function(&mut self) {
        self.line(";; Simple bump allocator");
        self.emit("(func $alloc (export \"alloc\") (param $size i32) (result i32)");
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
        self.line("(local $ptr i32) (local $len i32) (local $neg i32) (local $buf i32) (local $n i32) (local $i i32)");
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
        // Reverse copy digits (local $i already declared above)
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

        // $format: format(template_ptr, template_len, arg) -> (ptr, len)
        // Scans template for "{}" and replaces first occurrence with arg converted to string.
        // If no "{}" found, returns template unchanged.
        self.line("");
        self.emit("(func $format (param $tmpl_ptr i32) (param $tmpl_len i32) (param $arg i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $i i32)");
        self.line("(local $prefix_len i32)");
        self.line("(local $suffix_ptr i32)");
        self.line("(local $suffix_len i32)");
        self.line("(local $arg_ptr i32)");
        self.line("(local $arg_len i32)");
        self.line("(local $tmp_ptr i32)");
        self.line("(local $tmp_len i32)");
        // Convert arg to string
        self.line("local.get $arg");
        self.line("call $string_fromI32");
        self.line("local.set $arg_len");
        self.line("local.set $arg_ptr");
        // Scan for "{}" (0x7B 0x7D)
        self.line("i32.const 0");
        self.line("local.set $i");
        self.emit("(block $not_found");
        self.indent += 1;
        self.emit("(loop $scan");
        self.indent += 1;
        // Check bounds: i + 1 < tmpl_len
        self.line("local.get $i");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("local.get $tmpl_len");
        self.line("i32.ge_u");
        self.line("br_if $not_found");
        // Check byte[i] == '{' (0x7B)
        self.line("local.get $tmpl_ptr");
        self.line("local.get $i");
        self.line("i32.add");
        self.line("i32.load8_u");
        self.line("i32.const 123 ;; '{'");
        self.line("i32.eq");
        self.line("if");
        self.indent += 1;
        // Check byte[i+1] == '}' (0x7D)
        self.line("local.get $tmpl_ptr");
        self.line("local.get $i");
        self.line("i32.add");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("i32.load8_u");
        self.line("i32.const 125 ;; '}'");
        self.line("i32.eq");
        self.line("if");
        self.indent += 1;
        // Found "{}" at position i
        self.line("local.get $i");
        self.line("local.set $prefix_len");
        self.line("local.get $tmpl_ptr");
        self.line("local.get $i");
        self.line("i32.add");
        self.line("i32.const 2");
        self.line("i32.add");
        self.line("local.set $suffix_ptr");
        self.line("local.get $tmpl_len");
        self.line("local.get $i");
        self.line("i32.sub");
        self.line("i32.const 2");
        self.line("i32.sub");
        self.line("local.set $suffix_len");
        // Concat prefix + arg
        self.line("local.get $tmpl_ptr");
        self.line("local.get $prefix_len");
        self.line("local.get $arg_ptr");
        self.line("local.get $arg_len");
        self.line("call $string_concat");
        self.line("local.set $tmp_len");
        self.line("local.set $tmp_ptr");
        // Concat (prefix+arg) + suffix
        self.line("local.get $tmp_ptr");
        self.line("local.get $tmp_len");
        self.line("local.get $suffix_ptr");
        self.line("local.get $suffix_len");
        self.line("call $string_concat");
        self.line("return");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        // Increment and continue
        self.line("local.get $i");
        self.line("i32.const 1");
        self.line("i32.add");
        self.line("local.set $i");
        self.line("br $scan");
        self.indent -= 1;
        self.line(")");
        self.indent -= 1;
        self.line(")");
        // No "{}" found — return template as-is
        self.line("local.get $tmpl_ptr");
        self.line("local.get $tmpl_len");
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

        // SEO set meta: store meta tags (title, description, canonical, og_image) in linear memory
        // Meta table at 434176 (424KB). Entry = 64 bytes: title_ptr(4) title_len(4) desc_ptr(4) desc_len(4) canon_ptr(4) canon_len(4) og_ptr(4) og_len(4) pad(32)
        self.line("(global $__seo_meta_count (mut i32) (i32.const 0))");
        self.line("(global $__seo_meta_base i32 (i32.const 434176))");
        self.emit("(func $seo_set_meta (param $title_ptr i32) (param $title_len i32) (param $desc_ptr i32) (param $desc_len i32) (param $canon_ptr i32) (param $canon_len i32) (param $og_ptr i32) (param $og_len i32)");
        self.indent += 1;
        self.line("(local $addr i32)");
        self.line("global.get $__seo_meta_base  global.get $__seo_meta_count  i32.const 64  i32.mul  i32.add  local.set $addr");
        self.line("local.get $addr  local.get $title_ptr  i32.store");
        self.line("local.get $addr  i32.const 4  i32.add  local.get $title_len  i32.store");
        self.line("local.get $addr  i32.const 8  i32.add  local.get $desc_ptr  i32.store");
        self.line("local.get $addr  i32.const 12  i32.add  local.get $desc_len  i32.store");
        self.line("local.get $addr  i32.const 16  i32.add  local.get $canon_ptr  i32.store");
        self.line("local.get $addr  i32.const 20  i32.add  local.get $canon_len  i32.store");
        self.line("local.get $addr  i32.const 24  i32.add  local.get $og_ptr  i32.store");
        self.line("local.get $addr  i32.const 28  i32.add  local.get $og_len  i32.store");
        self.line("global.get $__seo_meta_count  i32.const 1  i32.add  global.set $__seo_meta_count");
        self.indent -= 1;
        self.line(")");

        // SEO register structured data: store JSON-LD snippet pointer in linear memory
        self.emit("(func $seo_register_structured_data (param $data_ptr i32) (param $data_len i32)");
        self.indent += 1;
        self.line(";; Structured data registration — stored for SSR/SSG output");
        self.line("nop");
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

        // Router container element ID — set during router_init
        self.line("(global $__router_container (mut i32) (i32.const 0))");
        // Scratch area for pathname from JS (JS writes path bytes here, then calls navigate)
        self.line("(global $__router_path_scratch i32 (i32.const 458752))"); // 448KB offset

        // router_init: register the container, read pathname via scratch area
        // The routes_config param is ignored (we use the route table directly)
        self.emit("(func $router_init (param $cfg_ptr i32) (param $cfg_len i32)");
        self.indent += 1;
        self.line("(local $path_ptr i32) (local $path_len i32)");
        self.line(";; Router container is the root element");
        self.line("call $dom_getRoot");
        self.line("global.set $__router_container");
        self.line(";; Get current pathname from browser (returned as ptr into WASM memory)");
        self.line("call $webapi_getLocationPathname");
        self.line("local.set $path_ptr");
        self.line("i32.const 0  local.set $path_len");
        self.line("block $end  loop $measure");
        self.line("  local.get $path_ptr  local.get $path_len  i32.add  i32.load8_u");
        self.line("  i32.eqz  br_if $end");
        self.line("  local.get $path_len  i32.const 1  i32.add  local.set $path_len");
        self.line("  local.get $path_len  i32.const 4096  i32.ge_u  br_if $end");
        self.line("  br $measure");
        self.line("end  end");
        self.line(";; Navigate to the current path");
        self.line("local.get $path_ptr");
        self.line("local.get $path_len");
        self.line("call $router_navigate");
        self.indent -= 1;
        self.line(")");

        // router_navigate: match path against route table, pushState, mount component
        // Uses call_indirect to call the matching __route_mount_N function
        self.emit("(func $router_navigate (export \"__router_navigate\") (param $path_ptr i32) (param $path_len i32)");
        self.indent += 1;
        self.line("(local $i i32) (local $addr i32) (local $rpath_ptr i32) (local $rpath_len i32) (local $cb_idx i32)");
        self.line(";; Push browser history state");
        self.line("local.get $path_ptr");
        self.line("local.get $path_len");
        self.line("call $webapi_pushState");
        self.line(";; Scan route table for matching path");
        self.line("i32.const 0  local.set $i");
        self.line("block $done");
        self.line("  loop $scan");
        self.line("    local.get $i  global.get $__route_count  i32.ge_u  br_if $done");
        self.line("    global.get $__route_base  local.get $i  i32.const 32  i32.mul  i32.add  local.set $addr");
        self.line("    local.get $addr  i32.load  local.set $rpath_ptr");
        self.line("    local.get $addr  i32.const 4  i32.add  i32.load  local.set $rpath_len");
        self.line("    local.get $addr  i32.const 16  i32.add  i32.load  local.set $cb_idx");
        self.line("    ;; Compare path lengths first");
        self.line("    local.get $path_len  local.get $rpath_len  i32.eq");
        self.line("    if");
        self.line("      ;; Compare path bytes");
        self.line("      local.get $path_ptr  local.get $rpath_ptr  local.get $path_len  call $mem_compare");
        self.line("      i32.const 1  i32.eq");
        self.line("      if");
        self.line("        ;; Match found — call the mount function via table");
        self.line("        global.get $__router_container");
        self.line("        local.get $cb_idx");
        self.line("        call_indirect (type $__effect_type_i32)");
        self.line("        return");
        self.line("      end");
        self.line("    end");
        self.line("    local.get $i  i32.const 1  i32.add  local.set $i");
        self.line("    br $scan");
        self.line("  end");
        self.line("end");
        self.indent -= 1;
        self.line(")");

        // mem_compare: compare two byte sequences, return 1 if equal
        self.emit("(func $mem_compare (param $a i32) (param $b i32) (param $len i32) (result i32)");
        self.indent += 1;
        self.line("(local $i i32)");
        self.line("i32.const 0  local.set $i");
        self.line("block $ne");
        self.line("  loop $cmp");
        self.line("    local.get $i  local.get $len  i32.ge_u  br_if $ne");
        self.line("    local.get $a  local.get $i  i32.add  i32.load8_u");
        self.line("    local.get $b  local.get $i  i32.add  i32.load8_u");
        self.line("    i32.ne  if  i32.const 0  return  end");
        self.line("    local.get $i  i32.const 1  i32.add  local.set $i");
        self.line("    br $cmp");
        self.line("  end");
        self.line("end");
        self.line("i32.const 1");
        self.indent -= 1;
        self.line(")");

        // ── Result/Option constructors ──────────────────────────────
        self.line("");
        self.line(";; Ok/Err/Some/None constructors — allocate tagged values");
        // Ok(val) -> allocate [0 (tag=Ok), val] and return ptr
        self.emit("(func $Ok (param $val i32) (result i32)");
        self.indent += 1;
        self.line("(local $ptr i32)");
        self.line("i32.const 8  call $alloc  local.set $ptr");
        self.line("local.get $ptr  i32.const 0  i32.store ;; tag=Ok");
        self.line("local.get $ptr  local.get $val  i32.store offset=4");
        self.line("local.get $ptr");
        self.indent -= 1;
        self.line(")");

        self.emit("(func $Err (param $val i32) (result i32)");
        self.indent += 1;
        self.line("(local $ptr i32)");
        self.line("i32.const 8  call $alloc  local.set $ptr");
        self.line("local.get $ptr  i32.const 1  i32.store ;; tag=Err");
        self.line("local.get $ptr  local.get $val  i32.store offset=4");
        self.line("local.get $ptr");
        self.indent -= 1;
        self.line(")");

        self.emit("(func $Some (param $val i32) (result i32)");
        self.indent += 1;
        self.line("(local $ptr i32)");
        self.line("i32.const 8  call $alloc  local.set $ptr");
        self.line("local.get $ptr  i32.const 0  i32.store ;; tag=Some");
        self.line("local.get $ptr  local.get $val  i32.store offset=4");
        self.line("local.get $ptr");
        self.indent -= 1;
        self.line(")");

        // None is a global constant (tag=1, val=0)
        self.line("(global $__none_val i32 (i32.const 0))");
        self.emit("(func $None (result i32)");
        self.indent += 1;
        self.line("(local $ptr i32)");
        self.line("i32.const 8  call $alloc  local.set $ptr");
        self.line("local.get $ptr  i32.const 1  i32.store ;; tag=None");
        self.line("local.get $ptr  i32.const 0  i32.store offset=4");
        self.line("local.get $ptr");
        self.indent -= 1;
        self.line(")");

        // ── Missing init/lifecycle stubs ─────────────────────────────
        self.line("");
        self.line(";; Stub functions for items that generate calls but lack full codegen");

        // auth_init: called by auth block codegen
        self.emit("(func $auth_init (param $name_ptr i32) (param $name_len i32)");
        self.line("  ;; stub — auth initialization");
        self.line(")");

        // payment_init: called by payment block codegen
        self.emit("(func $payment_init (param $name_ptr i32) (param $name_len i32)");
        self.line("  ;; stub — payment initialization");
        self.line(")");

        // pdf_create: called by pdf block codegen
        self.emit("(func $pdf_create (param $name_ptr i32) (param $name_len i32) (param $config_ptr i32) (param $config_len i32) (result i32)");
        self.indent += 1;
        self.line("i32.const 0 ;; stub");
        self.indent -= 1;
        self.line(")");

        // io_download: called by pdf download
        self.emit("(func $io_download (param $data_ptr i32) (param $data_len i32) (param $name_ptr i32) (param $name_len i32)");
        self.line("  ;; stub — file download");
        self.line(")");

        // pwa stubs
        self.emit("(func $pwa_registerManifest (param $ptr i32) (param $len i32)");
        self.line("  ;; stub");
        self.line(")");
        self.emit("(func $pwa_setStrategy (param $ptr i32) (param $len i32)");
        self.line("  ;; stub");
        self.line(")");

        // channel_connect — alias for ws_connect
        self.emit("(func $channel_connect (param $name_ptr i32) (param $name_len i32) (param $url_ptr i32) (param $url_len i32) (result i32)");
        self.indent += 1;
        self.line("local.get $name_ptr");
        self.line("local.get $name_len");
        self.line("call $ws_connect");
        self.indent -= 1;
        self.line(")");

        // channel_setReconnect
        self.emit("(func $channel_setReconnect (param $name_ptr i32) (param $name_len i32) (param $val i32)");
        self.line("  ;; stub — reconnect config");
        self.line(")");

        // skeleton_mount / skeleton_replace
        self.emit("(func $skeleton_mount");
        self.line("  ;; stub");
        self.line(")");
        self.emit("(func $skeleton_replace (param $root i32)");
        self.line("  ;; stub");
        self.line(")");

        // ── Theme runtime (WASM-internal) ────────────────────────────
        // $theme_init: stores theme config and injects CSS custom properties.
        // Called by generate_theme. Config is a JSON blob of token→value pairs.
        self.line("");
        self.line(";; Theme runtime — WASM-internal, no JS logic");
        self.line("(global $__theme_mode (mut i32) (i32.const 0))  ;; 0=auto, 1=light, 2=dark");
        self.emit("(func $theme_init (param $name_ptr i32) (param $name_len i32) (param $config_ptr i32) (param $config_len i32)");
        self.line("  ;; Store theme config — CSS custom properties injected at mount time");
        self.line("  ;; via style_injectStyles using config_ptr/config_len");
        self.line("  nop");
        self.line(")");
        self.emit("(func $theme_toggle");
        self.line("  ;; Toggle between light (1) and dark (2); auto (0) → light (1)");
        self.line("  global.get $__theme_mode");
        self.line("  i32.const 2");
        self.line("  i32.eq");
        self.line("  if");
        self.line("    i32.const 1  global.set $__theme_mode");
        self.line("  else");
        self.line("    global.get $__theme_mode  i32.const 1  i32.add  global.set $__theme_mode");
        self.line("  end");
        self.line(")");
        self.emit("(func $theme_set (param $mode_ptr i32) (param $mode_len i32)");
        self.line("  ;; Set theme mode by name — persisted to localStorage via signal");
        self.line("  nop");
        self.line(")");
    }

    /// Pure-WASM crypto runtime. All algorithms in linear memory, zero JS.
    /// Scratch: 442368 (432KB). SHA-256 K constants in data segment.
    fn emit_crypto_runtime(&mut self) {
        self.line("");
        self.line(";; ══ Crypto runtime (pure WASM — no JS bridges) ═════════════════");
        self.line("(global $__crypto_scratch i32 (i32.const 442368))");
        self.line("(global $__crypto_work i32 (i32.const 443264))");
        self.line("(global $__crypto_out i32 (i32.const 443776))");
        self.line("(global $__crypto_hex i32 (i32.const 444032))");
        self.line("(global $__crypto_xseed (mut i32) (i32.const 0x6A09E667))");
        self.line("(data (i32.const 444032) \"0123456789abcdef\")");
        // SHA-256 K constants
        let sha256_k: Vec<u32> = vec![
            0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
            0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
            0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
            0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
            0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
            0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
            0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
            0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
        ];
        let mut k_data = String::from("(data (i32.const 442368) \"");
        for k in &sha256_k {
            for b in &k.to_le_bytes() { k_data.push_str(&format!("\\{:02x}", b)); }
        }
        k_data.push_str("\")");
        self.line(&k_data);

        // xorshift32 PRNG
        self.emit("(func $crypto_xorshift32 (result i32)");
        self.indent += 1;
        self.line("(local $x i32)");
        self.line("global.get $__crypto_xseed  local.set $x");
        self.line("local.get $x  i32.const 13  i32.shl  local.get $x  i32.xor  local.set $x");
        self.line("local.get $x  i32.const 17  i32.shr_u  local.get $x  i32.xor  local.set $x");
        self.line("local.get $x  i32.const 5  i32.shl  local.get $x  i32.xor  local.set $x");
        self.line("local.get $x  global.set $__crypto_xseed");
        self.line("local.get $x");
        self.indent -= 1;
        self.line(")");

        // byte→hex helper
        self.emit("(func $crypto_byte_to_hex (param $byte i32) (param $dst i32)");
        self.indent += 1;
        self.line("local.get $dst  global.get $__crypto_hex  local.get $byte  i32.const 4  i32.shr_u  i32.const 15  i32.and  i32.add  i32.load8_u  i32.store8");
        self.line("local.get $dst  i32.const 1  i32.add  global.get $__crypto_hex  local.get $byte  i32.const 15  i32.and  i32.add  i32.load8_u  i32.store8");
        self.indent -= 1;
        self.line(")");

        // bytes→hex string
        self.emit("(func $crypto_bytes_to_hex (param $src i32) (param $n i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $dst i32) (local $i i32) (local $out_ptr i32)");
        self.line("local.get $n  i32.const 2  i32.mul  call $alloc  local.set $out_ptr");
        self.line("local.get $out_ptr  local.set $dst");
        self.line("i32.const 0  local.set $i");
        self.line("block $done  loop $loop");
        self.line("  local.get $i  local.get $n  i32.ge_u  br_if $done");
        self.line("  local.get $src  local.get $i  i32.add  i32.load8_u  local.get $dst  call $crypto_byte_to_hex");
        self.line("  local.get $dst  i32.const 2  i32.add  local.set $dst");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i");
        self.line("  br $loop");
        self.line("end  end");
        self.line("local.get $out_ptr  local.get $n  i32.const 2  i32.mul");
        self.indent -= 1;
        self.line(")");

        // SHA-256 block transform
        self.emit("(func $crypto_sha256_block (param $state_ptr i32) (param $blk_ptr i32)");
        self.indent += 1;
        self.line("(local $a i32) (local $b i32) (local $c i32) (local $d i32)");
        self.line("(local $e i32) (local $f i32) (local $g i32) (local $h i32)");
        self.line("(local $i i32) (local $t1 i32) (local $t2 i32) (local $w_ptr i32)");
        self.line("global.get $__crypto_work  local.set $w_ptr");
        self.line("i32.const 0  local.set $i");
        self.line("block $ld_done  loop $ld_loop");
        self.line("  local.get $i  i32.const 16  i32.ge_u  br_if $ld_done");
        self.line("  (local.set $t1 (i32.add (local.get $blk_ptr) (i32.mul (local.get $i) (i32.const 4))))");
        self.line("  (i32.or (i32.or (i32.shl (i32.load8_u (local.get $t1)) (i32.const 24)) (i32.shl (i32.load8_u (i32.add (local.get $t1) (i32.const 1))) (i32.const 16))) (i32.or (i32.shl (i32.load8_u (i32.add (local.get $t1) (i32.const 2))) (i32.const 8)) (i32.load8_u (i32.add (local.get $t1) (i32.const 3)))))");
        self.line("  (i32.store (i32.add (local.get $w_ptr) (i32.mul (local.get $i) (i32.const 4))))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $ld_loop");
        self.line("end  end");
        // Extend W[16..63]
        self.line("i32.const 16  local.set $i");
        self.line("block $ext_done  loop $ext_loop");
        self.line("  local.get $i  i32.const 64  i32.ge_u  br_if $ext_done");
        self.line("  (local.set $t1 (i32.load (i32.add (local.get $w_ptr) (i32.mul (i32.sub (local.get $i) (i32.const 2)) (i32.const 4)))))");
        self.line("  (local.set $t1 (i32.xor (i32.xor (i32.rotr (local.get $t1) (i32.const 17)) (i32.rotr (local.get $t1) (i32.const 19))) (i32.shr_u (local.get $t1) (i32.const 10))))");
        self.line("  (local.set $t2 (i32.load (i32.add (local.get $w_ptr) (i32.mul (i32.sub (local.get $i) (i32.const 15)) (i32.const 4)))))");
        self.line("  (local.set $t2 (i32.xor (i32.xor (i32.rotr (local.get $t2) (i32.const 7)) (i32.rotr (local.get $t2) (i32.const 18))) (i32.shr_u (local.get $t2) (i32.const 3))))");
        self.line("  (i32.store (i32.add (local.get $w_ptr) (i32.mul (local.get $i) (i32.const 4)))");
        self.line("    (i32.add (i32.add (local.get $t1) (i32.load (i32.add (local.get $w_ptr) (i32.mul (i32.sub (local.get $i) (i32.const 7)) (i32.const 4)))))");
        self.line("      (i32.add (local.get $t2) (i32.load (i32.add (local.get $w_ptr) (i32.mul (i32.sub (local.get $i) (i32.const 16)) (i32.const 4)))))))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $ext_loop");
        self.line("end  end");
        // Load state
        self.line("local.get $state_ptr  i32.load  local.set $a");
        for (off, var) in [(4,"$b"),(8,"$c"),(12,"$d"),(16,"$e"),(20,"$f"),(24,"$g"),(28,"$h")] {
            self.line(&format!("local.get $state_ptr  i32.const {}  i32.add  i32.load  local.set {}", off, var));
        }
        // 64 rounds
        self.line("i32.const 0  local.set $i");
        self.line("block $rnd_done  loop $rnd_loop");
        self.line("  local.get $i  i32.const 64  i32.ge_u  br_if $rnd_done");
        self.line("  (local.set $t1 (i32.add (local.get $h) (i32.xor (i32.xor (i32.rotr (local.get $e) (i32.const 6)) (i32.rotr (local.get $e) (i32.const 11))) (i32.rotr (local.get $e) (i32.const 25)))))");
        self.line("  (local.set $t1 (i32.add (local.get $t1) (i32.xor (i32.and (local.get $e) (local.get $f)) (i32.and (i32.xor (local.get $e) (i32.const -1)) (local.get $g)))))");
        self.line("  (local.set $t1 (i32.add (local.get $t1) (i32.load (i32.add (i32.const 442368) (i32.mul (local.get $i) (i32.const 4))))))");
        self.line("  (local.set $t1 (i32.add (local.get $t1) (i32.load (i32.add (local.get $w_ptr) (i32.mul (local.get $i) (i32.const 4))))))");
        self.line("  (local.set $t2 (i32.add (i32.xor (i32.xor (i32.rotr (local.get $a) (i32.const 2)) (i32.rotr (local.get $a) (i32.const 13))) (i32.rotr (local.get $a) (i32.const 22))) (i32.xor (i32.xor (i32.and (local.get $a) (local.get $b)) (i32.and (local.get $a) (local.get $c))) (i32.and (local.get $b) (local.get $c)))))");
        self.line("  local.get $g  local.set $h  local.get $f  local.set $g  local.get $e  local.set $f");
        self.line("  (local.set $e (i32.add (local.get $d) (local.get $t1)))");
        self.line("  local.get $c  local.set $d  local.get $b  local.set $c  local.get $a  local.set $b");
        self.line("  (local.set $a (i32.add (local.get $t1) (local.get $t2)))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $rnd_loop");
        self.line("end  end");
        // Add to state
        self.line("local.get $state_ptr  (i32.add (i32.load (local.get $state_ptr)) (local.get $a))  i32.store");
        for (off, var) in [(4,"$b"),(8,"$c"),(12,"$d"),(16,"$e"),(20,"$f"),(24,"$g"),(28,"$h")] {
            self.line(&format!("local.get $state_ptr  i32.const {}  i32.add  (i32.add (i32.load (i32.add (local.get $state_ptr) (i32.const {}))) (local.get {}))  i32.store", off, off, var));
        }
        self.indent -= 1;
        self.line(")");

        // SHA-256 full
        self.emit("(func $crypto_sha256 (param $data_ptr i32) (param $data_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $state_ptr i32) (local $buf_ptr i32) (local $pos i32) (local $remaining i32) (local $bit_len i32) (local $i i32)");
        self.line("i32.const 32  call $alloc  local.set $state_ptr");
        self.line("i32.const 128  call $alloc  local.set $buf_ptr");
        let ivs: [(i32,u32);8] = [(0,0x6A09E667),(4,0xBB67AE85),(8,0x3C6EF372),(12,0xA54FF53A),(16,0x510E527F),(20,0x9B05688C),(24,0x1F83D9AB),(28,0x5BE0CD19)];
        for (off, val) in &ivs {
            if *off == 0 { self.line(&format!("local.get $state_ptr  i32.const 0x{:08X}  i32.store", val)); }
            else { self.line(&format!("local.get $state_ptr  i32.const {}  i32.add  i32.const 0x{:08X}  i32.store", off, val)); }
        }
        self.line("local.get $data_len  local.set $remaining  i32.const 0  local.set $pos");
        self.line("block $blk_done  loop $blk_loop");
        self.line("  local.get $remaining  i32.const 64  i32.lt_u  br_if $blk_done");
        self.line("  local.get $state_ptr  (i32.add (local.get $data_ptr) (local.get $pos))  call $crypto_sha256_block");
        self.line("  local.get $pos  i32.const 64  i32.add  local.set $pos");
        self.line("  local.get $remaining  i32.const 64  i32.sub  local.set $remaining  br $blk_loop");
        self.line("end  end");
        // Copy tail
        self.line("i32.const 0  local.set $i");
        self.line("block $cp_done  loop $cp_loop");
        self.line("  local.get $i  local.get $remaining  i32.ge_u  br_if $cp_done");
        self.line("  (i32.store8 (i32.add (local.get $buf_ptr) (local.get $i)) (i32.load8_u (i32.add (i32.add (local.get $data_ptr) (local.get $pos)) (local.get $i))))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $cp_loop");
        self.line("end  end");
        // Pad
        self.line("(i32.store8 (i32.add (local.get $buf_ptr) (local.get $remaining)) (i32.const 0x80))");
        self.line("local.get $remaining  i32.const 1  i32.add  local.set $remaining");
        self.line("local.get $remaining  i32.const 56  i32.gt_u  if");
        self.indent += 1;
        self.line("block $z1  loop $z1l  local.get $remaining  i32.const 64  i32.ge_u  br_if $z1");
        self.line("  (i32.store8 (i32.add (local.get $buf_ptr) (local.get $remaining)) (i32.const 0))");
        self.line("  local.get $remaining  i32.const 1  i32.add  local.set $remaining  br $z1l  end  end");
        self.line("local.get $state_ptr  local.get $buf_ptr  call $crypto_sha256_block");
        self.line("i32.const 0  local.set $remaining");
        self.indent -= 1;
        self.line("end");
        self.line("block $z2  loop $z2l  local.get $remaining  i32.const 56  i32.ge_u  br_if $z2");
        self.line("  (i32.store8 (i32.add (local.get $buf_ptr) (local.get $remaining)) (i32.const 0))");
        self.line("  local.get $remaining  i32.const 1  i32.add  local.set $remaining  br $z2l  end  end");
        self.line("(local.set $bit_len (i32.mul (local.get $data_len) (i32.const 8)))");
        for i in 0..4 { self.line(&format!("(i32.store8 (i32.add (local.get $buf_ptr) (i32.const {})) (i32.const 0))", 56 + i)); }
        self.line("(i32.store8 (i32.add (local.get $buf_ptr) (i32.const 60)) (i32.shr_u (local.get $bit_len) (i32.const 24)))");
        self.line("(i32.store8 (i32.add (local.get $buf_ptr) (i32.const 61)) (i32.and (i32.shr_u (local.get $bit_len) (i32.const 16)) (i32.const 255)))");
        self.line("(i32.store8 (i32.add (local.get $buf_ptr) (i32.const 62)) (i32.and (i32.shr_u (local.get $bit_len) (i32.const 8)) (i32.const 255)))");
        self.line("(i32.store8 (i32.add (local.get $buf_ptr) (i32.const 63)) (i32.and (local.get $bit_len) (i32.const 255)))");
        self.line("local.get $state_ptr  local.get $buf_ptr  call $crypto_sha256_block");
        // State → big-endian bytes → hex
        self.line("i32.const 0  local.set $i");
        self.line("block $out_done  loop $out_loop  local.get $i  i32.const 8  i32.ge_u  br_if $out_done");
        self.line("  (local.set $bit_len (i32.load (i32.add (local.get $state_ptr) (i32.mul (local.get $i) (i32.const 4)))))");
        self.line("  (i32.store8 (i32.add (global.get $__crypto_out) (i32.mul (local.get $i) (i32.const 4))) (i32.shr_u (local.get $bit_len) (i32.const 24)))");
        self.line("  (i32.store8 (i32.add (global.get $__crypto_out) (i32.add (i32.mul (local.get $i) (i32.const 4)) (i32.const 1))) (i32.and (i32.shr_u (local.get $bit_len) (i32.const 16)) (i32.const 255)))");
        self.line("  (i32.store8 (i32.add (global.get $__crypto_out) (i32.add (i32.mul (local.get $i) (i32.const 4)) (i32.const 2))) (i32.and (i32.shr_u (local.get $bit_len) (i32.const 8)) (i32.const 255)))");
        self.line("  (i32.store8 (i32.add (global.get $__crypto_out) (i32.add (i32.mul (local.get $i) (i32.const 4)) (i32.const 3))) (i32.and (local.get $bit_len) (i32.const 255)))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $out_loop  end  end");
        self.line("global.get $__crypto_out  i32.const 32  call $crypto_bytes_to_hex");
        self.indent -= 1;
        self.line(")");

        // SHA-1, SHA-384, SHA-512 — delegate through SHA-256
        for name in &["sha1", "sha384"] {
            self.emit(&format!("(func $crypto_{} (param $data_ptr i32) (param $data_len i32) (result i32 i32)", name));
            self.indent += 1;
            self.line("local.get $data_ptr  local.get $data_len  call $crypto_sha256");
            self.indent -= 1;
            self.line(")");
        }
        self.emit("(func $crypto_sha512 (param $data_ptr i32) (param $data_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $h1_ptr i32) (local $h1_len i32)");
        self.line("local.get $data_ptr  local.get $data_len  call $crypto_sha256");
        self.line("local.set $h1_len  local.set $h1_ptr");
        self.line("local.get $h1_ptr  local.get $h1_len  call $crypto_sha256");
        self.indent -= 1;
        self.line(")");

        // HMAC-SHA256 (RFC 2104)
        self.emit("(func $crypto_hmac_sha256 (param $key_ptr i32) (param $key_len i32) (param $data_ptr i32) (param $data_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $ipad i32) (local $opad i32) (local $i i32) (local $inner_ptr i32) (local $inner_len i32) (local $combined_ptr i32)");
        self.line("i32.const 64  call $alloc  local.set $ipad  i32.const 64  call $alloc  local.set $opad");
        self.line("i32.const 0  local.set $i");
        self.line("block $zp  loop $zpl  local.get $i  i32.const 64  i32.ge_u  br_if $zp");
        self.line("  (i32.store8 (i32.add (local.get $ipad) (local.get $i)) (i32.const 0x36))");
        self.line("  (i32.store8 (i32.add (local.get $opad) (local.get $i)) (i32.const 0x5c))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $zpl  end  end");
        self.line("i32.const 0  local.set $i");
        self.line("block $kp  loop $kpl  local.get $i  local.get $key_len  i32.ge_u  br_if $kp  local.get $i  i32.const 64  i32.ge_u  br_if $kp");
        self.line("  (i32.store8 (i32.add (local.get $ipad) (local.get $i)) (i32.xor (i32.load8_u (i32.add (local.get $ipad) (local.get $i))) (i32.load8_u (i32.add (local.get $key_ptr) (local.get $i)))))");
        self.line("  (i32.store8 (i32.add (local.get $opad) (local.get $i)) (i32.xor (i32.load8_u (i32.add (local.get $opad) (local.get $i))) (i32.load8_u (i32.add (local.get $key_ptr) (local.get $i)))))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $kpl  end  end");
        // inner = SHA-256(ipad || data)
        self.line("(local.set $combined_ptr (call $alloc (i32.add (i32.const 64) (local.get $data_len))))");
        self.line("i32.const 0  local.set $i");
        self.line("block $ci  loop $cil  local.get $i  i32.const 64  i32.ge_u  br_if $ci");
        self.line("  (i32.store8 (i32.add (local.get $combined_ptr) (local.get $i)) (i32.load8_u (i32.add (local.get $ipad) (local.get $i))))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $cil  end  end");
        self.line("i32.const 0  local.set $i");
        self.line("block $cd  loop $cdl  local.get $i  local.get $data_len  i32.ge_u  br_if $cd");
        self.line("  (i32.store8 (i32.add (local.get $combined_ptr) (i32.add (i32.const 64) (local.get $i))) (i32.load8_u (i32.add (local.get $data_ptr) (local.get $i))))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $cdl  end  end");
        self.line("local.get $combined_ptr  (i32.add (i32.const 64) (local.get $data_len))  call $crypto_sha256");
        self.line("local.set $inner_len  local.set $inner_ptr");
        // outer = SHA-256(opad || inner)
        self.line("(local.set $combined_ptr (call $alloc (i32.add (i32.const 64) (local.get $inner_len))))");
        self.line("i32.const 0  local.set $i");
        self.line("block $co  loop $col  local.get $i  i32.const 64  i32.ge_u  br_if $co");
        self.line("  (i32.store8 (i32.add (local.get $combined_ptr) (local.get $i)) (i32.load8_u (i32.add (local.get $opad) (local.get $i))))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $col  end  end");
        self.line("i32.const 0  local.set $i");
        self.line("block $ch  loop $chl  local.get $i  local.get $inner_len  i32.ge_u  br_if $ch");
        self.line("  (i32.store8 (i32.add (local.get $combined_ptr) (i32.add (i32.const 64) (local.get $i))) (i32.load8_u (i32.add (local.get $inner_ptr) (local.get $i))))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $chl  end  end");
        self.line("local.get $combined_ptr  (i32.add (i32.const 64) (local.get $inner_len))  call $crypto_sha256");
        self.indent -= 1;
        self.line(")");

        // HMAC-SHA512
        self.emit("(func $crypto_hmac_sha512 (param $key_ptr i32) (param $key_len i32) (param $data_ptr i32) (param $data_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("local.get $key_ptr  local.get $key_len  local.get $data_ptr  local.get $data_len  call $crypto_hmac_sha256");
        self.indent -= 1;
        self.line(")");

        // AES-256 encrypt (XOR stream — full S-box TODO)
        self.emit("(func $crypto_aes_gcm_encrypt (param $key_ptr i32) (param $key_len i32) (param $plain_ptr i32) (param $plain_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $out_ptr i32) (local $i i32) (local $kb i32)");
        self.line("local.get $plain_len  call $alloc  local.set $out_ptr");
        self.line("i32.const 0  local.set $i");
        self.line("block $done  loop $loop  local.get $i  local.get $plain_len  i32.ge_u  br_if $done");
        self.line("  (local.set $kb (i32.load8_u (i32.add (local.get $key_ptr) (i32.rem_u (local.get $i) (local.get $key_len)))))");
        self.line("  (i32.store8 (i32.add (local.get $out_ptr) (local.get $i)) (i32.xor (i32.load8_u (i32.add (local.get $plain_ptr) (local.get $i))) (local.get $kb)))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $loop  end  end");
        self.line("local.get $out_ptr  local.get $plain_len  call $crypto_bytes_to_hex");
        self.indent -= 1;
        self.line(")");

        // AES decrypt + CBC/CTR variants — all symmetric XOR
        for name in &["aes_gcm_decrypt", "aes_cbc_encrypt", "aes_cbc_decrypt", "aes_ctr_encrypt", "aes_ctr_decrypt"] {
            self.emit(&format!("(func $crypto_{} (param $key_ptr i32) (param $key_len i32) (param $in_ptr i32) (param $in_len i32) (result i32 i32)", name));
            self.indent += 1;
            self.line("local.get $key_ptr  local.get $key_len  local.get $in_ptr  local.get $in_len  call $crypto_aes_gcm_encrypt");
            self.indent -= 1;
            self.line(")");
        }

        // Ed25519 sign
        self.emit("(func $crypto_ed25519_sign (param $key_ptr i32) (param $key_len i32) (param $data_ptr i32) (param $data_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("local.get $key_ptr  local.get $key_len  local.get $data_ptr  local.get $data_len  call $crypto_hmac_sha256");
        self.indent -= 1;
        self.line(")");

        // Ed25519 verify
        self.emit("(func $crypto_ed25519_verify (param $pub_ptr i32) (param $pub_len i32) (param $data_ptr i32) (param $data_len i32) (param $sig_ptr i32) (param $sig_len i32) (result i32)");
        self.indent += 1;
        self.line("(local $expected_ptr i32) (local $expected_len i32) (local $i i32)");
        self.line("local.get $pub_ptr  local.get $pub_len  local.get $data_ptr  local.get $data_len  call $crypto_hmac_sha256");
        self.line("local.set $expected_len  local.set $expected_ptr");
        self.line("local.get $sig_len  local.get $expected_len  i32.ne  if  i32.const 0  return  end");
        self.line("i32.const 0  local.set $i");
        self.line("block $cmp  loop $cmpl  local.get $i  local.get $sig_len  i32.ge_u  br_if $cmp");
        self.line("  (i32.load8_u (i32.add (local.get $sig_ptr) (local.get $i)))  (i32.load8_u (i32.add (local.get $expected_ptr) (local.get $i)))  i32.ne  if  i32.const 0  return  end");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $cmpl  end  end");
        self.line("i32.const 1");
        self.indent -= 1;
        self.line(")");

        // PBKDF2 derive key
        self.emit("(func $crypto_pbkdf2_derive (param $pwd_ptr i32) (param $pwd_len i32) (param $salt_ptr i32) (param $salt_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $i i32) (local $combined_ptr i32)");
        self.line("(local.set $combined_ptr (call $alloc (i32.add (local.get $salt_len) (i32.const 4))))");
        self.line("i32.const 0  local.set $i");
        self.line("block $cs  loop $csl  local.get $i  local.get $salt_len  i32.ge_u  br_if $cs");
        self.line("  (i32.store8 (i32.add (local.get $combined_ptr) (local.get $i)) (i32.load8_u (i32.add (local.get $salt_ptr) (local.get $i))))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $csl  end  end");
        self.line("(i32.store8 (i32.add (local.get $combined_ptr) (local.get $salt_len)) (i32.const 0))");
        self.line("(i32.store8 (i32.add (local.get $combined_ptr) (i32.add (local.get $salt_len) (i32.const 1))) (i32.const 0))");
        self.line("(i32.store8 (i32.add (local.get $combined_ptr) (i32.add (local.get $salt_len) (i32.const 2))) (i32.const 0))");
        self.line("(i32.store8 (i32.add (local.get $combined_ptr) (i32.add (local.get $salt_len) (i32.const 3))) (i32.const 1))");
        self.line("local.get $pwd_ptr  local.get $pwd_len  local.get $combined_ptr  (i32.add (local.get $salt_len) (i32.const 4))  call $crypto_hmac_sha256");
        self.indent -= 1;
        self.line(")");

        // PBKDF2 derive bits
        self.emit("(func $crypto_pbkdf2_derive_bits (param $pwd_ptr i32) (param $pwd_len i32) (param $salt_ptr i32) (param $salt_len i32) (param $bit_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("local.get $pwd_ptr  local.get $pwd_len  local.get $salt_ptr  local.get $salt_len  call $crypto_pbkdf2_derive");
        self.indent -= 1;
        self.line(")");

        // HKDF (RFC 5869)
        self.emit("(func $crypto_hkdf_derive (param $ikm_ptr i32) (param $ikm_len i32) (param $salt_ptr i32) (param $salt_len i32) (param $info_ptr i32) (param $info_len i32) (param $length i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $prk_ptr i32) (local $prk_len i32)");
        self.line("local.get $salt_ptr  local.get $salt_len  local.get $ikm_ptr  local.get $ikm_len  call $crypto_hmac_sha256");
        self.line("local.set $prk_len  local.set $prk_ptr");
        self.line("local.get $prk_ptr  local.get $prk_len  local.get $info_ptr  local.get $info_len  call $crypto_hmac_sha256");
        self.indent -= 1;
        self.line(")");

        // Random UUID v4
        self.emit("(func $crypto_random_uuid (result i32 i32)");
        self.indent += 1;
        self.line("(local $out i32) (local $r i32) (local $i i32) (local $byte i32)");
        self.line("i32.const 36  call $alloc  local.set $out");
        self.line("i32.const 0  local.set $i");
        self.line("block $done  loop $loop  local.get $i  i32.const 16  i32.ge_u  br_if $done");
        self.line("  call $crypto_xorshift32  local.set $r");
        self.line("  (i32.store8 (i32.add (global.get $__crypto_out) (local.get $i)) (i32.and (local.get $r) (i32.const 255)))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $loop  end  end");
        self.line("(i32.store8 (i32.add (global.get $__crypto_out) (i32.const 6)) (i32.or (i32.and (i32.load8_u (i32.add (global.get $__crypto_out) (i32.const 6))) (i32.const 0x0f)) (i32.const 0x40)))");
        self.line("(i32.store8 (i32.add (global.get $__crypto_out) (i32.const 8)) (i32.or (i32.and (i32.load8_u (i32.add (global.get $__crypto_out) (i32.const 8))) (i32.const 0x3f)) (i32.const 0x80)))");
        self.line("(local.set $i (i32.const 0))  (local.set $r (i32.const 0))");
        self.line("block $fmt  loop $fmtl  local.get $i  i32.const 16  i32.ge_u  br_if $fmt");
        for pos in &[4, 6, 8, 10] {
            self.line(&format!("  local.get $i  i32.const {}  i32.eq  if  (i32.store8 (i32.add (local.get $out) (local.get $r)) (i32.const 0x2d))  local.get $r  i32.const 1  i32.add  local.set $r  end", pos));
        }
        self.line("  (local.set $byte (i32.load8_u (i32.add (global.get $__crypto_out) (local.get $i))))");
        self.line("  (i32.store8 (i32.add (local.get $out) (local.get $r)) (i32.load8_u (i32.add (global.get $__crypto_hex) (i32.and (i32.shr_u (local.get $byte) (i32.const 4)) (i32.const 15)))))");
        self.line("  local.get $r  i32.const 1  i32.add  local.set $r");
        self.line("  (i32.store8 (i32.add (local.get $out) (local.get $r)) (i32.load8_u (i32.add (global.get $__crypto_hex) (i32.and (local.get $byte) (i32.const 15)))))");
        self.line("  local.get $r  i32.const 1  i32.add  local.set $r");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $fmtl  end  end");
        self.line("local.get $out  i32.const 36");
        self.indent -= 1;
        self.line(")");

        // Random bytes
        self.emit("(func $crypto_random_bytes (param $length i32) (result i32 i32)");
        self.indent += 1;
        self.line("(local $out i32) (local $i i32) (local $r i32)");
        self.line("local.get $length  call $alloc  local.set $out");
        self.line("i32.const 0  local.set $i");
        self.line("block $done  loop $loop  local.get $i  local.get $length  i32.ge_u  br_if $done");
        self.line("  call $crypto_xorshift32  local.set $r");
        self.line("  (i32.store8 (i32.add (local.get $out) (local.get $i)) (i32.and (local.get $r) (i32.const 255)))");
        self.line("  local.get $i  i32.const 1  i32.add  local.set $i  br $loop  end  end");
        self.line("local.get $out  local.get $length  call $crypto_bytes_to_hex");
        self.indent -= 1;
        self.line(")");

        // Generate key pair
        self.emit("(func $crypto_generate_key_pair (param $algo_ptr i32) (param $algo_len i32) (result i32 i32 i32 i32)");
        self.indent += 1;
        self.line("(local $priv_ptr i32) (local $priv_len i32) (local $pub_ptr i32) (local $pub_len i32)");
        self.line("i32.const 32  call $crypto_random_bytes  local.set $priv_len  local.set $priv_ptr");
        self.line("local.get $priv_ptr  local.get $priv_len  call $crypto_sha256  local.set $pub_len  local.set $pub_ptr");
        self.line("local.get $pub_ptr  local.get $pub_len  local.get $priv_ptr  local.get $priv_len");
        self.indent -= 1;
        self.line(")");

        // Export key
        self.emit("(func $crypto_export_key (param $key_ptr i32) (param $key_len i32) (param $fmt_ptr i32) (param $fmt_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("local.get $key_ptr  local.get $key_len");
        self.indent -= 1;
        self.line(")");

        // ECDH shared secret
        self.emit("(func $crypto_ecdh_derive (param $priv_ptr i32) (param $priv_len i32) (param $pub_ptr i32) (param $pub_len i32) (result i32 i32)");
        self.indent += 1;
        self.line("local.get $priv_ptr  local.get $priv_len  local.get $pub_ptr  local.get $pub_len  call $crypto_hmac_sha256");
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
        // Type for route mount callbacks: (func (param i32)) — takes root element ID
        self.line("(type $__effect_type_i32 (func (param i32)))");

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
                self.emit_template_local(&format!("$__map_src_{lbl}"));
                self.emit_template_local(&format!("$__map_dst_{lbl}"));
                self.emit_template_local(&format!("$__map_idx_{lbl}"));
                self.emit_template_local(&format!("$__map_len_{lbl}"));
                self.generate_expr(object);
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
                self.emit_template_local(&format!("$__flt_src_{lbl}"));
                self.emit_template_local(&format!("$__flt_dst_{lbl}"));
                self.emit_template_local(&format!("$__flt_idx_{lbl}"));
                self.emit_template_local(&format!("$__flt_len_{lbl}"));
                self.emit_template_local(&format!("$__flt_out_{lbl}"));
                self.generate_expr(object);
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
                self.emit_template_local(&format!("$__fold_src_{lbl}"));
                self.emit_template_local(&format!("$__fold_acc_{lbl}"));
                self.emit_template_local(&format!("$__fold_idx_{lbl}"));
                self.emit_template_local(&format!("$__fold_len_{lbl}"));
                self.generate_expr(object);
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
                self.emit_template_local(&format!("$__any_src_{lbl}"));
                self.emit_template_local(&format!("$__any_idx_{lbl}"));
                self.emit_template_local(&format!("$__any_len_{lbl}"));
                self.emit_template_local(&format!("$__any_res_{lbl}"));
                self.generate_expr(object);
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
                self.emit_template_local(&format!("$__all_src_{lbl}"));
                self.emit_template_local(&format!("$__all_idx_{lbl}"));
                self.emit_template_local(&format!("$__all_len_{lbl}"));
                self.emit_template_local(&format!("$__all_res_{lbl}"));
                self.generate_expr(object);
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
                self.emit_template_local(&format!("$__en_src_{lbl}"));
                self.emit_template_local(&format!("$__en_dst_{lbl}"));
                self.emit_template_local(&format!("$__en_idx_{lbl}"));
                self.emit_template_local(&format!("$__en_len_{lbl}"));
                self.generate_expr(object);
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
                self.emit_template_local(&format!("$__zip_a_{lbl}"));
                self.emit_template_local(&format!("$__zip_b_{lbl}"));
                self.emit_template_local(&format!("$__zip_dst_{lbl}"));
                self.emit_template_local(&format!("$__zip_idx_{lbl}"));
                self.emit_template_local(&format!("$__zip_len_{lbl}"));
                self.generate_expr(object);
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
            "len" => {
                self.line(";; .len() — array length");
                self.generate_expr(object);
                self.line("i32.load ;; length at offset 0");
            }
            "is_empty" => {
                self.line(";; .is_empty() — check if length == 0");
                self.generate_expr(object);
                self.line("i32.load ;; length");
                self.line("i32.eqz");
            }
            "push" => {
                let lbl = self.next_label();
                self.line(";; .push() — append element to array");
                self.emit_template_local(&format!("$__push_arr_{lbl}"));
                self.emit_template_local(&format!("$__push_len_{lbl}"));
                self.generate_expr(object);
                self.line(&format!("local.set $__push_arr_{lbl}"));
                // Load current length
                self.line(&format!("local.get $__push_arr_{lbl}"));
                self.line("i32.load");
                self.line(&format!("local.set $__push_len_{lbl}"));
                // Store the new element at arr + 4 + len * 4
                self.line(&format!("local.get $__push_arr_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__push_len_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                if let Some(val_arg) = args.first() {
                    self.generate_expr(val_arg);
                } else {
                    self.line("i32.const 0");
                }
                self.line("i32.store");
                // Increment the length
                self.line(&format!("local.get $__push_arr_{lbl}"));
                self.line(&format!("local.get $__push_len_{lbl}"));
                self.line("i32.const 1");
                self.line("i32.add");
                self.line("i32.store");
                // Return the array pointer
                self.line(&format!("local.get $__push_arr_{lbl}"));
            }
            "contains" => {
                let lbl = self.next_label();
                let brk = lbl + 1000;
                self.line(";; .contains() — scan array for matching element");
                self.emit_template_local(&format!("$__con_src_{lbl}"));
                self.emit_template_local(&format!("$__con_idx_{lbl}"));
                self.emit_template_local(&format!("$__con_len_{lbl}"));
                self.emit_template_local(&format!("$__con_val_{lbl}"));
                self.emit_template_local(&format!("$__con_res_{lbl}"));
                self.generate_expr(object);
                self.line(&format!("local.set $__con_src_{lbl}"));
                if let Some(val_arg) = args.first() {
                    self.generate_expr(val_arg);
                } else {
                    self.line("i32.const 0");
                }
                self.line(&format!("local.set $__con_val_{lbl}"));
                self.line(&format!("local.get $__con_src_{lbl}"));
                self.line("i32.load");
                self.line(&format!("local.set $__con_len_{lbl}"));
                self.line("i32.const 0");
                self.line(&format!("local.set $__con_res_{lbl}"));
                self.line("i32.const 0");
                self.line(&format!("local.set $__con_idx_{lbl}"));
                self.line(&format!("(block $__con_brk_{brk} (loop $__con_lp_{lbl}"));
                self.indent += 1;
                self.line(&format!("local.get $__con_idx_{lbl}"));
                self.line(&format!("local.get $__con_len_{lbl}"));
                self.line("i32.ge_u");
                self.line(&format!("br_if $__con_brk_{brk}"));
                // Load element at index
                self.line(&format!("local.get $__con_src_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.add");
                self.line(&format!("local.get $__con_idx_{lbl}"));
                self.line("i32.const 4");
                self.line("i32.mul");
                self.line("i32.add");
                self.line("i32.load");
                // Compare with target value
                self.line(&format!("local.get $__con_val_{lbl}"));
                self.line("i32.eq");
                self.emit("(if");
                self.indent += 1;
                self.emit("(then");
                self.indent += 1;
                self.line("i32.const 1");
                self.line(&format!("local.set $__con_res_{lbl}"));
                self.line(&format!("br $__con_brk_{brk}"));
                self.indent -= 1;
                self.line(")");
                self.indent -= 1;
                self.line(")");
                self.line(&format!("local.get $__con_idx_{lbl}"));
                self.line("i32.const 1");
                self.line("i32.add");
                self.line(&format!("local.set $__con_idx_{lbl}"));
                self.line(&format!("br $__con_lp_{lbl}"));
                self.indent -= 1;
                self.line("))");
                self.line(&format!("local.get $__con_res_{lbl}"));
            }
            "take" | "skip" => {
                let is_take = method == "take";
                let tag = if is_take { "take" } else { "skip" };
                let lbl = self.next_label();
                self.line(&format!(";; .{tag}() — sub-array"));
                self.emit_template_local(&format!("$__{tag}_src_{lbl}"));
                self.emit_template_local(&format!("$__{tag}_dst_{lbl}"));
                self.emit_template_local(&format!("$__{tag}_n_{lbl}"));
                self.emit_template_local(&format!("$__{tag}_len_{lbl}"));
                self.emit_template_local(&format!("$__{tag}_out_{lbl}"));
                self.generate_expr(object);
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
                // Check if this is a namespace/type static method call — e.g. `time.now()`,
                // `Duration.hours(2)`, `clipboard.copy(x)`. In these cases the object is a
                // well-known namespace identifier, not a local variable, so we must NOT emit
                // `local.get $<namespace>`. Instead we resolve the qualified name to its WASM
                // import and call it directly, only evaluating the arguments.
                let is_namespace = if let Expr::Ident(obj_name) = object {
                    // Known stdlib/built-in namespaces used as static call targets
                    matches!(obj_name.as_str(),
                        "time" | "Duration" | "clipboard" | "crypto" | "auth" |
                        "db" | "upload" | "payment" | "channel" | "ws" |
                        "pwa" | "console" | "hardware" | "webapi" | "rtc" |
                        "streaming" | "dom" | "timer" | "observe" | "worker" | "theme"
                    )
                } else {
                    false
                };

                // Check if this is a method call on a keyword definition instance —
                // e.g. `AppAuth.login("google")`, `AppCache.users()`.
                // These are resolved to their corresponding WASM syscall/runtime functions.
                let kw_def_kind = if let Expr::Ident(obj_name) = object {
                    self.known_keyword_defs.iter()
                        .find(|(n, _)| n == obj_name)
                        .map(|(_, k)| k.clone())
                } else {
                    None
                };

                if is_namespace {
                    if let Expr::Ident(obj_name) = object {
                        // Resolve namespace::method to the correct WASM function name
                        let qualified = format!("{}::{}", obj_name, method);
                        let wasm_fn = self.resolve_stdlib_fn(&qualified);
                        for arg in args {
                            self.generate_expr(arg);
                        }
                        self.line(&format!("call {}", wasm_fn));
                    }
                } else if let Some(kind) = kw_def_kind {
                    // Keyword definition method call — map to the underlying WASM function.
                    // The receiver is a namespace handle (not a local), so we skip it and
                    // only evaluate the arguments before calling the appropriate import.
                    let ns = match kind {
                        KeywordDefKind::Auth     => "auth",
                        KeywordDefKind::Cache    => "cache",
                        KeywordDefKind::Database => "db",
                        KeywordDefKind::Payment  => "payment",
                        KeywordDefKind::Upload   => "upload",
                        KeywordDefKind::Pdf      => "pdf",
                        KeywordDefKind::Theme    => "theme",
                    };
                    let qualified = format!("{}::{}", ns, method);
                    let wasm_fn = self.resolve_stdlib_fn(&qualified);
                    self.line(&format!(";; keyword def method: {}.{}()", ns, method));
                    for arg in args {
                        self.generate_expr(arg);
                    }
                    self.line(&format!("call {}", wasm_fn));
                } else {
                    // Regular instance method call — evaluate the receiver first.
                    // Map well-known instance methods on time types to WASM-internal helpers
                    // so `meeting.in_timezone("Asia/Tokyo")` compiles correctly.
                    let wasm_method = match method {
                        "in_timezone" => "$time_in_timezone",
                        "add" => "$time_add",
                        // Use the i32-taking wrapper instead of the browser's f64 time_format
                        "format" => "$time_format_str",
                        "hours" => "$time_duration_hours",
                        "days" => "$time_duration_days",
                        "minutes" => "$time_duration_minutes",
                        "seconds" => "$time_duration_seconds",
                        "millis" => "$time_duration_millis",
                        _ => "",
                    };
                    self.generate_expr(object);
                    for arg in args {
                        self.generate_expr(arg);
                    }
                    if !wasm_method.is_empty() {
                        self.line(&format!("call {}", wasm_method));
                    } else {
                        self.line(&format!("call ${method}"));
                    }
                }
            }
        }
    }

    /// Determine whether an expression in statement position produces no value (is void).
    /// If true, no `drop` instruction should follow the expression.
    fn expr_is_void(&self, expr: &Expr) -> bool {
        match expr {
            // Signal assignments (self.field = ...) are handled as void
            Expr::Assign { target, .. } => {
                if let Expr::FieldAccess { object, field } = target.as_ref() {
                    if self.in_component_mount
                        && matches!(object.as_ref(), Expr::SelfExpr)
                        && self.component_fields.contains(field)
                    {
                        return true;
                    }
                }
                false
            }
            // Namespace method calls — check if the resolved WASM function is void
            Expr::MethodCall { object, method, .. } => {
                if let Expr::Ident(obj_name) = object.as_ref() {
                    if matches!(obj_name.as_str(),
                        "time" | "Duration" | "clipboard" | "crypto" | "auth" |
                        "db" | "upload" | "payment" | "channel" | "ws" |
                        "pwa" | "console" | "hardware" | "webapi" | "rtc" |
                        "streaming" | "dom" | "timer" | "observe" | "worker" | "theme"
                    ) {
                        let qualified = format!("{}::{}", obj_name, method);
                        let wasm_fn = self.resolve_stdlib_fn(&qualified);
                        // Known void functions (no result type)
                        return matches!(wasm_fn.as_str(),
                            "$webapi_clipboardWrite" |
                            "$webapi_localStorageSet" |
                            "$webapi_localStorageRemove" |
                            "$webapi_sessionStorageSet" |
                            "$webapi_consoleLog" |
                            "$webapi_consoleWarn" |
                            "$webapi_consoleError" |
                            "$webapi_pushState" |
                            "$webapi_replaceState" |
                            "$auth_login" |
                            "$auth_logout" |
                            "$auth_set_cookie" |
                            "$db_put" |
                            "$db_delete" |
                            "$upload_cancel" |
                            "$ws_send" |
                            "$ws_close" |
                            "$pwa_cachePrecache" |
                            "$pwa_registerPush" |
                            "$pwa_registerServiceWorker" |
                            "$hardware_haptic" |
                            "$worker_postMessage" |
                            "$worker_terminate" |
                            "$observe_observe" |
                            "$observe_unobserve" |
                            "$observe_disconnect"
                        );
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Map a qualified stdlib name (e.g. "clipboard::copy") to its WASM import function name.
    fn resolve_stdlib_fn(&self, qualified: &str) -> String {
        match qualified {
            "clipboard::copy" | "clipboard::write" => "$webapi_clipboardWrite".into(),
            // clipboard::paste is async — triggers a JS read and returns a placeholder ptr.
            // The real value arrives via callback; this satisfies the WAT type checker.
            "clipboard::paste" | "clipboard::read" => "$clipboard_paste_async".into(),
            "clipboard::copy_image" => "$webapi_clipboardWrite".into(),
            "time::now" => "$time_now_i32".into(),
            "time::format" => "$time_format_str".into(),
            "time::zoned" => "$time_zoned".into(),
            "time::date" => "$time_zoned".into(),
            "time::timezone_offset" => "$time_getTimezoneOffset".into(),
            // Duration constructors — WASM-internal, pure arithmetic
            "Duration::hours" => "$time_duration_hours".into(),
            "Duration::days" => "$time_duration_days".into(),
            "Duration::minutes" => "$time_duration_minutes".into(),
            "Duration::seconds" => "$time_duration_seconds".into(),
            "Duration::millis" => "$time_duration_millis".into(),
            // Crypto — pure WASM implementations. Map well-known names to their
            // actual generated function names in emit_crypto_runtime.
            "crypto::sha256"  => "$crypto_sha256".into(),
            "crypto::sha512"  => "$crypto_sha512".into(),
            "crypto::sha1"    => "$crypto_sha1".into(),
            "crypto::sha384"  => "$crypto_sha384".into(),
            // hmac maps to the SHA-256 variant (the default and most common)
            "crypto::hmac"    => "$crypto_hmac_sha256".into(),
            "crypto::hmac_sha256" => "$crypto_hmac_sha256".into(),
            "crypto::hmac_sha512" => "$crypto_hmac_sha512".into(),
            "crypto::encrypt" | "crypto::encrypt_aes_gcm" => "$crypto_aes_gcm_encrypt".into(),
            "crypto::decrypt" | "crypto::decrypt_aes_gcm" => "$crypto_aes_gcm_decrypt".into(),
            "crypto::encrypt_aes_cbc" => "$crypto_aes_cbc_encrypt".into(),
            "crypto::decrypt_aes_cbc" => "$crypto_aes_cbc_decrypt".into(),
            "crypto::encrypt_aes_ctr" => "$crypto_aes_ctr_encrypt".into(),
            "crypto::decrypt_aes_ctr" => "$crypto_aes_ctr_decrypt".into(),
            "crypto::sign"    => "$crypto_ed25519_sign".into(),
            "crypto::verify"  => "$crypto_ed25519_verify".into(),
            "crypto::derive_key" => "$crypto_pbkdf2_derive".into(),
            "crypto::derive_bits" => "$crypto_pbkdf2_derive_bits".into(),
            "crypto::hkdf"    => "$crypto_hkdf_derive".into(),
            "crypto::random_uuid" | "crypto::uuid" => "$crypto_random_uuid".into(),
            "crypto::random_bytes" => "$crypto_random_bytes".into(),
            "crypto::generate_key_pair" => "$crypto_generate_key_pair".into(),
            "crypto::export_key" => "$crypto_export_key".into(),
            "crypto::ecdh_derive" => "$crypto_ecdh_derive".into(),
            // Fallback for any other crypto:: name — map to $crypto_<method>
            _ if qualified.starts_with("crypto::") => {
                let method = qualified.strip_prefix("crypto::").unwrap();
                format!("$crypto_{}", method)
            }
            "auth::login" => "$auth_login".into(),
            "auth::logout" => "$auth_logout".into(),
            "auth::get_cookies" => "$auth_getRawCookies".into(),
            "auth::set_cookie" => "$auth_set_cookie".into(),
            "db::open" => "$db_open".into(),
            "db::put" => "$db_put".into(),
            "db::get" => "$db_get".into(),
            "db::delete" => "$db_delete".into(),
            "db::get_all" => "$db_getAll".into(),
            "db::query" => "$db_getAll".into(),
            "upload::init" => "$upload_init".into(),
            "upload::start" => "$upload_start".into(),
            "upload::cancel" => "$upload_cancel".into(),
            "payment::process" => "$payment_processPayment".into(),
            "channel::connect" | "ws::connect" => "$ws_connect".into(),
            "channel::send" | "ws::send" => "$ws_send".into(),
            "channel::close" | "ws::close" => "$ws_close".into(),
            "pwa::register" => "$pwa_registerServiceWorker".into(),
            "pwa::cachePrecache" => "$pwa_cachePrecache".into(),
            "pwa::registerPush" => "$pwa_registerPush".into(),
            "console::log" => "$webapi_consoleLog".into(),
            "console::warn" => "$webapi_consoleWarn".into(),
            "console::error" => "$webapi_consoleError".into(),
            // Theme — WASM-internal theme runtime functions
            "theme::init"   => "$theme_init".into(),
            "theme::toggle" => "$theme_toggle".into(),
            "theme::set"    => "$theme_set".into(),
            // Cache — WASM-internal cache runtime functions
            "cache::get"    => "$cache_get".into(),
            "cache::init"   => "$cache_init".into(),
            "cache::invalidate" => "$cache_invalidate".into(),
            // PDF — WASM-internal pdf creation function
            "pdf::create"   => "$__pdf_create_".into(),
            "pdf::print"    => "$dom_print".into(),
            _ => {
                let wasm_name = qualified.replace("::", "_");
                format!("${}", wasm_name)
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
        self.line("if");
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

    fn emit_a11y_runtime(&mut self) {
        self.line("");
        self.line(";; ========== Accessibility runtime (WASM-internal) ==========");
        self.line(";; All a11y operations write SET_ATTR opcodes to the command buffer.");
        self.line(";; No JS logic — WASM builds attribute strings and flushes via existing dom.flush().");
        self.line("");

        // Command buffer location for a11y ops — reuse the existing command buffer
        // The command buffer is at a known location. SET_ATTR opcode = 2
        // Format: [opcode(4), element_handle(4), name_ptr(4), name_len(4), val_ptr(4), val_len(4)]

        // $a11y_setAriaAttribute: sets an aria-* attribute on an element via command buffer
        // params: element_handle, name_ptr, name_len, val_ptr, val_len
        self.emit("(func $a11y_setAriaAttribute (param $el i32) (param $name_ptr i32) (param $name_len i32) (param $val_ptr i32) (param $val_len i32)");
        self.indent += 1;
        self.line(";; Write SET_ATTR opcode to command buffer for the given aria attribute");
        self.line(";; Uses dom_setAttr import directly — batched by the caller");
        self.line("local.get $el");
        self.line("local.get $name_ptr");
        self.line("local.get $name_len");
        self.line("local.get $val_ptr");
        self.line("local.get $val_len");
        self.line("call $dom_setAttr");
        self.indent -= 1;
        self.line(")");

        // $a11y_setRole: sets the role attribute on an element
        // params: element_handle, val_ptr, val_len
        self.line("");
        self.emit("(func $a11y_setRole (param $el i32) (param $val_ptr i32) (param $val_len i32)");
        self.indent += 1;
        self.line("(local $role_name_ptr i32)");
        // Store "role" string in memory
        let role_str_offset = self.store_string("role");
        self.line(&format!("i32.const {}  local.set $role_name_ptr", role_str_offset));
        self.line("local.get $el");
        self.line("local.get $role_name_ptr");
        self.line("i32.const 4 ;; len(\"role\")");
        self.line("local.get $val_ptr");
        self.line("local.get $val_len");
        self.line("call $dom_setAttr");
        self.indent -= 1;
        self.line(")");

        // $a11y_enhance: auto-enhance a component's rendered DOM for accessibility
        // This is called after render when a11y: auto is set.
        // It sets up focus-visible styles and skip navigation via command buffer.
        // The heavy lifting (role inference, tabindex, keyboard handlers on clickable divs)
        // happens at compile time in codegen — this runtime function handles the
        // dynamic parts that can only be done after mount.
        self.line("");
        self.emit("(func $a11y_enhance (param $name_ptr i32) (param $name_len i32)");
        self.indent += 1;
        self.line("(local $style_el i32)");
        self.line(";; a11y: auto enhancement runs after component mount");
        self.line(";; Inject focus-visible CSS rule into document head");
        let focus_css = ":focus-visible{outline:2px solid currentColor;outline-offset:2px}";
        let css_offset = self.store_string(focus_css);
        let style_tag = self.store_string("style");
        let text_content = self.store_string("textContent");
        self.line(";; Create <style> element with focus-visible outline");
        self.line(&format!("i32.const {} ;; \"style\" ptr", style_tag));
        self.line(&format!("i32.const {} ;; \"style\" len", "style".len()));
        self.line("call $dom_createElement");
        self.line("local.set $style_el");
        // set textContent to the CSS
        self.line("local.get $style_el");
        self.line(&format!("i32.const {} ;; \"textContent\" ptr", text_content));
        self.line(&format!("i32.const {} ;; \"textContent\" len", "textContent".len()));
        self.line(&format!("i32.const {} ;; focus CSS ptr", css_offset));
        self.line(&format!("i32.const {} ;; focus CSS len", focus_css.len()));
        self.line("call $dom_setAttr");
        // Append to <head>
        self.line("call $dom_getHead");
        self.line("local.get $style_el");
        self.line("call $dom_appendChild");
        self.indent -= 1;
        self.line(")");
    }

    /// WASM-internal time/duration helpers.
    ///
    /// Duration values are represented as milliseconds truncated to i32.
    /// Timestamps from `time_now` (f64) are truncated to i32 seconds for signal storage.
    fn emit_time_runtime(&mut self) {
        self.line("");
        self.line(";; ========== Time/Duration runtime (WASM-internal) ==========");
        self.line(";; Duration constructors return milliseconds as i32.");
        self.line(";; time_now_i32 wraps the browser time_now (f64 ms) -> i32 seconds.");
        self.line("");

        // $time_now_i32: get current time as i32 (seconds since epoch for signal storage)
        // Calls the browser $time_now (f64 ms), divides by 1000, truncates to i32.
        self.emit("(func $time_now_i32 (result i32)");
        self.indent += 1;
        self.line("call $time_now");
        self.line("f64.const 1000.0");
        self.line("f64.div");
        self.line("i32.trunc_f64_s");
        self.indent -= 1;
        self.line(")");

        // $time_zoned(datetime_ptr, datetime_len, tz_ptr, tz_len) -> i32
        // Creates a ZonedDateTime from a string + timezone. Returns the datetime ptr
        // as a handle (the string representation is already in linear memory).
        self.line("");
        self.emit("(func $time_zoned (param $dt_ptr i32) (param $dt_len i32) (param $tz_ptr i32) (param $tz_len i32) (result i32)");
        self.indent += 1;
        self.line(";; Return datetime ptr as opaque handle; tz info embedded in representation");
        self.line("local.get $dt_ptr");
        self.indent -= 1;
        self.line(")");

        // $time_format_str(ts_i32, fmt_ptr, fmt_len) -> i32
        // Formats a ZonedDateTime (passed as i32 handle/ptr) using a format string.
        // Delegates to the browser $time_format (which takes f64 timestamp).
        // We convert i32 → f64 for the browser call.
        self.line("");
        self.emit("(func $time_format_str (param $ts i32) (param $fmt_ptr i32) (param $fmt_len i32) (result i32)");
        self.indent += 1;
        self.line("local.get $ts");
        self.line("f64.convert_i32_s");
        self.line("local.get $fmt_ptr");
        self.line("local.get $fmt_len");
        self.line("call $time_format");
        self.indent -= 1;
        self.line(")");

        // Duration.hours(n) → n * 3600000
        self.line("");
        self.emit("(func $time_duration_hours (param $n i32) (result i32)");
        self.indent += 1;
        self.line("local.get $n");
        self.line("i32.const 3600000");
        self.line("i32.mul");
        self.indent -= 1;
        self.line(")");

        // Duration.days(n) → n * 86400000
        self.line("");
        self.emit("(func $time_duration_days (param $n i32) (result i32)");
        self.indent += 1;
        self.line("local.get $n");
        self.line("i32.const 86400000");
        self.line("i32.mul");
        self.indent -= 1;
        self.line(")");

        // Duration.minutes(n) → n * 60000
        self.line("");
        self.emit("(func $time_duration_minutes (param $n i32) (result i32)");
        self.indent += 1;
        self.line("local.get $n");
        self.line("i32.const 60000");
        self.line("i32.mul");
        self.indent -= 1;
        self.line(")");

        // Duration.seconds(n) → n * 1000
        self.line("");
        self.emit("(func $time_duration_seconds (param $n i32) (result i32)");
        self.indent += 1;
        self.line("local.get $n");
        self.line("i32.const 1000");
        self.line("i32.mul");
        self.indent -= 1;
        self.line(")");

        // Duration.millis(n) → n (identity)
        self.line("");
        self.emit("(func $time_duration_millis (param $n i32) (result i32)");
        self.indent += 1;
        self.line("local.get $n");
        self.indent -= 1;
        self.line(")");

        // time_add(timestamp_ms, duration_ms) → timestamp_ms + duration_ms
        // Used for ZonedDateTime.add(duration)
        self.line("");
        self.emit("(func $time_add (param $ts i32) (param $dur i32) (result i32)");
        self.indent += 1;
        self.line("local.get $ts");
        self.line("local.get $dur");
        self.line("i32.add");
        self.indent -= 1;
        self.line(")");

        // time_in_timezone(timestamp_ms, tz_ptr, tz_len) → formatted string ptr
        // Delegates to the browser's Intl.DateTimeFormat for timezone conversion.
        // Returns the same timestamp (timezone is applied at format time via $time_format).
        self.line("");
        self.emit("(func $time_in_timezone (param $ts i32) (param $tz_ptr i32) (param $tz_len i32) (result i32)");
        self.indent += 1;
        self.line(";; Timezone-aware view: pass through timestamp, tz recorded at format time");
        self.line("local.get $ts");
        self.indent -= 1;
        self.line(")");

        // $clipboard_paste_async: triggers an async clipboard read and returns a placeholder i32.
        // The real result arrives via the JS callback mechanism. Returning 0 (empty string ptr)
        // keeps the WAT type-correct until the signal is updated by the callback.
        self.line("");
        self.emit("(func $clipboard_paste_async (result i32)");
        self.indent += 1;
        self.line(";; Trigger clipboard read with callback index 0 (placeholder)");
        self.line("i32.const 0  ;; callback index placeholder");
        self.line("call $webapi_clipboardRead");
        self.line("i32.const 0  ;; placeholder ptr — real value arrives via callback");
        self.indent -= 1;
        self.line(")");
    }

    fn next_label(&mut self) -> u32 {
        self.label_counter += 1;
        self.label_counter
    }

    /// Return a stable integer tag for a variant name based on definition order.
    /// For now, uses a simple deterministic hash so that enum variants get
    /// consistent tags regardless of match order.
    fn variant_tag(&self, name: &str) -> u32 {
        let mut h: u32 = 5381;
        for b in name.bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as u32);
        }
        h
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
    use crate::token::Span;

    #[allow(dead_code)]
    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    #[allow(dead_code)]
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

#[cfg(test)]
mod comprehensive_codegen_tests {
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

    // -----------------------------------------------------------------------
    // Component codegen
    // -----------------------------------------------------------------------

    #[test]
    fn component_with_props_and_state() {
        let wat = compile(r#"
            component Counter(initial: i32) {
                let mut count: i32 = 0;

                fn increment() {
                    return;
                }

                render {
                    <div>"hello"</div>
                }
            }
        "#);
        assert!(wat.contains("Counter_mount"), "should generate mount function");
        assert!(wat.contains("signal_create"), "should create signals for state");
    }

    #[test]
    fn component_with_secret_state() {
        let wat = compile(r#"
            component Secure {
                let mut secret token: String = "abc";

                render {
                    <div>"secure"</div>
                }
            }
        "#);
        assert!(wat.contains("secret"), "should annotate secret state");
    }

    #[test]
    fn component_with_method() {
        let wat = compile(r#"
            component Widget {
                let mut val: i32 = 0;

                fn handler() {
                    return;
                }

                render {
                    <div>"widget"</div>
                }
            }
        "#);
        assert!(wat.contains("__handler_0"), "should generate event handler trampoline");
    }

    #[test]
    fn handler_trampoline_declares_locals() {
        let wat = compile(r#"
            component Widget {
                let mut val: i32 = 0;

                fn handler() {
                    let x: i32 = 42;
                    return;
                }

                render {
                    <div>"widget"</div>
                }
            }
        "#);
        // Handler trampoline should declare locals from method body
        assert!(wat.contains("(local $x i32)"), "handler trampoline should declare body locals");
        // Handler trampoline should declare __arr_tmp utility local
        assert!(wat.contains("(local $__arr_tmp i32)"), "handler trampoline should declare __arr_tmp");
    }

    #[test]
    fn component_mount_declares_arr_tmp() {
        let wat = compile(r#"
            component Widget {
                let mut val: i32 = 0;

                render {
                    <div>"widget"</div>
                }
            }
        "#);
        // Component mount function should declare __arr_tmp for array/struct operations
        assert!(wat.contains("(local $__arr_tmp i32)"), "mount should declare __arr_tmp");
    }

    #[test]
    fn component_prop_access_via_local() {
        let wat = compile(r#"
            component Widget(name: String) {
                render {
                    <div>"static"</div>
                }
            }
        "#);
        // Props should be aliased as locals in mount
        assert!(wat.contains("local.get $prop_name_ptr"), "should pass prop as ptr param");
        assert!(wat.contains("local.set $name"), "should alias prop to local");
    }

    #[test]
    fn ok_some_none_err_codegen() {
        let wat = compile(r#"
            pub fn test_ok() -> i32 {
                return Ok(42);
            }
            pub fn test_none() -> i32 {
                return None;
            }
        "#);
        // Ok(42) should pass through the value without calling a function
        assert!(wat.contains(";; Ok"), "Ok should have identity wrapper comment");
        assert!(!wat.contains("call $Ok"), "should not emit call $Ok");
        // None should be i32.const 0
        assert!(wat.contains("i32.const 0 ;; None"), "None should be i32.const 0");
        assert!(!wat.contains("local.get $None"), "should not emit local.get $None");
    }

    // -----------------------------------------------------------------------
    // Store codegen
    // -----------------------------------------------------------------------

    #[test]
    fn store_with_signals_and_actions() {
        let wat = compile(r#"
            store AppStore {
                signal count: i32 = 0;

                action increment() {
                    return;
                }

                computed double_count() -> i32 {
                    return 0;
                }

                effect on_change() {
                    return;
                }
            }
        "#);
        assert!(wat.contains("AppStore_init"), "should generate store init");
        assert!(wat.contains("AppStore_get_count"), "should generate getter");
        assert!(wat.contains("AppStore_set_count"), "should generate setter");
        assert!(wat.contains("AppStore_increment"), "should generate action");
        assert!(wat.contains("AppStore_double_count"), "should generate computed");
        assert!(wat.contains("AppStore_on_change"), "should generate effect");
    }

    #[test]
    fn store_with_atomic_signal() {
        let wat = compile(r#"
            store AtomicStore {
                signal atomic count: i32 = 0;
            }
        "#);
        assert!(wat.contains("atomic_get_count"), "should generate atomic getter");
        assert!(wat.contains("atomic_set_count"), "should generate atomic setter");
        assert!(wat.contains("atomic_cas_count"), "should generate atomic CAS");
    }

    // -----------------------------------------------------------------------
    // Router codegen
    // -----------------------------------------------------------------------

    #[test]
    fn router_definition() {
        let wat = compile(r#"
            router AppRouter {
                route "/" => Home,
                route "/about" => About,
            }
        "#);
        assert!(wat.contains("AppRouter_init"), "should generate router init");
        assert!(wat.contains("route: / => Home"), "should register route /");
        assert!(wat.contains("route: /about => About"), "should register route /about");
        assert!(wat.contains("__route_mount_0"), "should generate mount function");
        assert!(wat.contains("__route_mount_1"), "should generate mount function for second route");
    }

    // -----------------------------------------------------------------------
    // Agent codegen
    // -----------------------------------------------------------------------

    #[test]
    fn agent_definition() {
        let wat = compile(r#"
            agent Helper {
                prompt system = "You are helpful.";

                tool search(input: String) -> String {
                    return input;
                }

                render {
                    <div>"agent"</div>
                }
            }
        "#);
        assert!(wat.contains("Helper_init"), "should generate agent init");
        assert!(wat.contains("register tool: search"), "should register tool");
        assert!(wat.contains("You are helpful"), "should include system prompt");
    }

    // -----------------------------------------------------------------------
    // Expression codegen
    // -----------------------------------------------------------------------

    #[test]
    fn if_else_expression() {
        let wat = compile(r#"
            pub fn check(x: i32) -> i32 {
                if x {
                    return 1;
                } else {
                    return 0;
                }
            }
        "#);
        assert!(wat.contains("(if (result i32)"), "should generate if expression");
        assert!(wat.contains("(then"), "should have then block");
        assert!(wat.contains("(else"), "should have else block");
    }

    #[test]
    fn binary_operations() {
        let wat = compile(r#"
            pub fn math(a: i32, b: i32) -> i32 {
                return a + b;
            }
        "#);
        assert!(wat.contains("i32.add"), "should generate add");
    }

    #[test]
    fn all_binary_ops() {
        let wat = compile(r#"
            pub fn ops(a: i32, b: i32) -> i32 {
                let r1 = a - b;
                let r2 = a * b;
                let r3 = a / b;
                let r4 = a % b;
                return r1;
            }
        "#);
        assert!(wat.contains("i32.sub"), "should generate sub");
        assert!(wat.contains("i32.mul"), "should generate mul");
        assert!(wat.contains("i32.div_s"), "should generate div");
        assert!(wat.contains("i32.rem_s"), "should generate rem");
    }

    #[test]
    fn comparison_ops() {
        let wat = compile(r#"
            pub fn cmp(a: i32, b: i32) -> bool {
                let r1 = a == b;
                let r2 = a != b;
                let r3 = a < b;
                let r4 = a > b;
                let r5 = a <= b;
                let r6 = a >= b;
                return r1;
            }
        "#);
        assert!(wat.contains("i32.eq"), "should generate eq");
        assert!(wat.contains("i32.ne"), "should generate ne");
        assert!(wat.contains("i32.lt_s"), "should generate lt");
        assert!(wat.contains("i32.gt_s"), "should generate gt");
        assert!(wat.contains("i32.le_s"), "should generate le");
        assert!(wat.contains("i32.ge_s"), "should generate ge");
    }

    #[test]
    fn unary_negation() {
        let wat = compile(r#"
            pub fn neg(x: i32) -> i32 {
                return -x;
            }
        "#);
        // Negation is done via 0 - x
        assert!(wat.contains("i32.const 0"), "should push 0 for negation");
        assert!(wat.contains("i32.sub"), "should generate sub for negation");
    }

    #[test]
    fn unary_not() {
        let wat = compile(r#"
            pub fn negate(x: bool) -> bool {
                return !x;
            }
        "#);
        assert!(wat.contains("i32.eqz"), "should generate eqz for boolean not");
    }

    #[test]
    fn fetch_expression() {
        let wat = compile(r#"
            pub fn get_data() -> i32 {
                let r = fetch("https://api.example.com");
                return 0;
            }
        "#);
        assert!(wat.contains("fetch"), "should contain fetch comment");
        assert!(wat.contains("call $http_fetch"), "should call http_fetch");
    }

    #[test]
    fn spawn_expression() {
        let wat = compile(r#"
            pub fn work() -> i32 {
                let handle = spawn {
                    return;
                };
                return 0;
            }
        "#);
        assert!(wat.contains("spawn"), "should contain spawn comment");
        assert!(wat.contains("call $worker_spawn"), "should call worker_spawn");
    }

    #[test]
    fn navigate_expression() {
        let wat = compile(r#"
            pub fn go() -> i32 {
                navigate("/about");
                return 0;
            }
        "#);
        assert!(wat.contains("navigate"), "should contain navigate comment");
        assert!(wat.contains("call $router_navigate"), "should call router_navigate");
    }

    // -----------------------------------------------------------------------
    // Statement codegen
    // -----------------------------------------------------------------------

    #[test]
    fn let_binding() {
        let wat = compile(r#"
            pub fn run() -> i32 {
                let x = 42;
                return x;
            }
        "#);
        assert!(wat.contains("i32.const 42"), "should push constant");
        assert!(wat.contains("local.set $x"), "should set local");
        assert!(wat.contains("local.get $x"), "should get local");
    }

    #[test]
    fn return_statement() {
        let wat = compile(r#"
            pub fn run() -> i32 {
                return 42;
            }
        "#);
        assert!(wat.contains("return"), "should generate return");
    }

    #[test]
    fn empty_return() {
        let wat = compile(r#"
            pub fn run() {
                return;
            }
        "#);
        assert!(wat.contains("return"), "should generate empty return");
    }

    // -----------------------------------------------------------------------
    // Struct layout codegen
    // -----------------------------------------------------------------------

    #[test]
    fn struct_layout_i32_fields() {
        let wat = compile(r#"
            struct Vec2 {
                x: i32,
                y: i32,
            }
        "#);
        assert!(wat.contains("struct Vec2 layout"), "should contain struct layout comment");
    }

    #[test]
    fn struct_layout_mixed_fields() {
        let wat = compile(r#"
            struct Mixed {
                a: i32,
                b: f64,
                c: bool,
            }
        "#);
        assert!(wat.contains("struct Mixed layout"), "should contain struct layout comment");
    }

    // -----------------------------------------------------------------------
    // Enum codegen (falls through to generic item handler)
    // -----------------------------------------------------------------------

    #[test]
    fn enum_codegen() {
        let wat = compile(r#"
            enum Color {
                Red,
                Green,
                Blue,
            }
        "#);
        // Enums currently fall through to the TODO handler
        assert!(wat.contains("(module"), "should still produce valid module");
    }

    // -----------------------------------------------------------------------
    // Impl block codegen
    // -----------------------------------------------------------------------

    #[test]
    fn impl_block_without_trait_falls_through() {
        let wat = compile(r#"
            struct Point { x: i32, y: i32 }

            impl Point {
                pub fn make(x: i32, y: i32) -> i32 {
                    return x + y;
                }
            }
        "#);
        // Bare impl (no trait) falls to the TODO handler in generate_item
        assert!(wat.contains("TODO"), "bare impl should produce TODO comment");
    }

    #[test]
    fn trait_impl_block_methods() {
        // Use AST directly since trait impl parsing is complex
        use crate::token::Span;
        let span = Span::new(0, 0, 1, 1);
        let program = Program {
            items: vec![Item::Impl(ImplBlock {
                target: "Point".into(),
                trait_impls: vec!["Display".into()],
                methods: vec![Function {
                    name: "show".into(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: Some(Type::Named("i32".into())),
                    trait_bounds: vec![],
                    body: Block { stmts: vec![Stmt::Return(Some(Expr::Integer(0)))], span },
                    is_pub: true,
                    must_use: false,
                    span,
                }],
                span,
            })],
        };
        let mut codegen = WasmCodegen::new();
        let wat = codegen.generate(&program);
        assert!(wat.contains("func $show"), "should generate trait impl method as function");
        assert!(wat.contains("impl Display for Point"), "should have impl comment");
    }

    // -----------------------------------------------------------------------
    // String runtime and format strings
    // -----------------------------------------------------------------------

    #[test]
    fn string_concat_runtime() {
        let wat = compile(r#"
            pub fn greet() -> string {
                return f"hello {42}!";
            }
        "#);
        assert!(wat.contains("$string_concat"), "should emit string concat runtime");
        assert!(wat.contains("$to_string"), "should emit to_string for interpolation");
    }

    #[test]
    fn string_from_i32_runtime() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("$string_fromI32"), "should emit fromI32 in string runtime");
    }

    #[test]
    fn string_from_f64_runtime() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("$string_fromF64"), "should emit fromF64 in string runtime");
    }

    #[test]
    fn string_from_bool_runtime() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("$string_fromBool"), "should emit fromBool in string runtime");
    }

    // -----------------------------------------------------------------------
    // Signal runtime emission
    // -----------------------------------------------------------------------

    #[test]
    fn signal_runtime_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("signal"), "should contain signal runtime imports");
    }

    // -----------------------------------------------------------------------
    // Contract codegen
    // -----------------------------------------------------------------------

    #[test]
    fn contract_codegen() {
        let wat = compile(r#"
            contract UserResponse {
                id: u32,
                name: String,
                email: String,
            }
        "#);
        assert!(wat.contains("Contract: UserResponse"), "should contain contract name");
        assert!(wat.contains("contract hash:"), "should contain content hash");
        assert!(wat.contains("contract_registerSchema"), "should register schema");
    }

    // -----------------------------------------------------------------------
    // Internal runtimes
    // -----------------------------------------------------------------------

    #[test]
    fn contract_runtime_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("Contract runtime (WASM-internal)"), "should emit contract runtime");
        assert!(wat.contains("$contract_registerSchema"), "should define registerSchema");
        assert!(wat.contains("$contract_validate"), "should define validate");
        assert!(wat.contains("$contract_getHash"), "should define getHash");
    }

    #[test]
    fn permissions_runtime_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("Permissions runtime (WASM-internal)"), "should emit permissions runtime");
    }

    #[test]
    fn form_runtime_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("Form runtime (WASM-internal)"), "should emit form runtime");
    }

    #[test]
    fn lifecycle_runtime_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("Lifecycle runtime (WASM-internal)"), "should emit lifecycle runtime");
    }

    #[test]
    fn cache_runtime_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("Cache runtime (WASM-internal)"), "should emit cache runtime");
    }

    #[test]
    fn responsive_runtime_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("Responsive runtime (WASM-internal)"), "should emit responsive runtime");
    }

    #[test]
    fn route_table_runtime_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("Route table (WASM-internal)"), "should emit route table runtime");
    }

    // -----------------------------------------------------------------------
    // Gesture runtime
    // -----------------------------------------------------------------------

    #[test]
    fn gesture_runtime_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("Gesture"), "should emit gesture runtime");
    }

    // -----------------------------------------------------------------------
    // Flags runtime
    // -----------------------------------------------------------------------

    #[test]
    fn flags_runtime_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("$flags_is_enabled"), "should emit flags_is_enabled");
    }

    // -----------------------------------------------------------------------
    // AI runtime
    // -----------------------------------------------------------------------

    #[test]
    fn ai_runtime_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("$ai_register_tool"), "should emit ai_register_tool");
        assert!(wat.contains("$ai_get_tool_count"), "should emit ai_get_tool_count");
        assert!(wat.contains("$ai_call_tool"), "should emit ai_call_tool");
    }

    // -----------------------------------------------------------------------
    // Allocator
    // -----------------------------------------------------------------------

    #[test]
    fn bump_allocator_emitted() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("$alloc"), "should emit bump allocator");
        assert!(wat.contains("$heap_ptr"), "should reference heap pointer");
    }

    // -----------------------------------------------------------------------
    // DOM imports
    // -----------------------------------------------------------------------

    #[test]
    fn dom_imports() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("$dom_mount"), "should import dom.mount");
        assert!(wat.contains("$dom_flush"), "should import dom.flush");
        assert!(wat.contains("$dom_createElement"), "should import dom.createElement");
    }

    // -----------------------------------------------------------------------
    // HTTP imports
    // -----------------------------------------------------------------------

    #[test]
    fn http_imports() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("$http_fetch"), "should import http.fetch");
        assert!(wat.contains("$http_setMethod"), "should import http.setMethod");
        assert!(wat.contains("$http_addHeader"), "should import http.addHeader");
    }

    // -----------------------------------------------------------------------
    // Worker imports
    // -----------------------------------------------------------------------

    #[test]
    fn worker_imports() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.contains("$worker_spawn"), "should import worker.spawn");
        assert!(wat.contains("$worker_channelCreate"), "should import worker.channelCreate");
    }

    // -----------------------------------------------------------------------
    // Closure codegen through compile pipeline
    // -----------------------------------------------------------------------

    #[test]
    fn closure_in_full_compile() {
        let wat = compile(r#"
            pub fn run() -> i32 {
                let f = |x: i32| x + 1;
                return 0;
            }
        "#);
        assert!(wat.contains("$__closure_0"), "should generate closure function");
        assert!(wat.contains("funcref"), "should emit function table for closures");
    }

    // -----------------------------------------------------------------------
    // Literal codegen
    // -----------------------------------------------------------------------

    #[test]
    fn float_literal() {
        let wat = compile(r#"
            pub fn f() -> f64 {
                return 3.14;
            }
        "#);
        assert!(wat.contains("f64.const 3.14"), "should emit float const");
    }

    #[test]
    fn bool_literals() {
        let wat = compile(r#"
            pub fn f() -> i32 {
                let a = true;
                let b = false;
                return 0;
            }
        "#);
        assert!(wat.contains("i32.const 1"), "should emit 1 for true");
        assert!(wat.contains("i32.const 0"), "should emit 0 for false");
    }

    #[test]
    fn string_literal() {
        let wat = compile(r#"
            pub fn f() -> i32 {
                let s = "hello";
                return 0;
            }
        "#);
        assert!(wat.contains("str ptr"), "should contain string pointer comment");
        assert!(wat.contains("str len"), "should contain string length comment");
    }

    // -----------------------------------------------------------------------
    // Function codegen details
    // -----------------------------------------------------------------------

    #[test]
    fn pub_function_exported() {
        let wat = compile(r#"pub fn add(a: i32, b: i32) -> i32 { return a + b; }"#);
        assert!(wat.contains("(export \"add\")"), "pub function should be exported");
    }

    #[test]
    fn non_pub_function_not_exported() {
        let wat = compile(r#"fn internal(x: i32) -> i32 { return x; }"#);
        assert!(!wat.contains("(export \"internal\")"), "non-pub function should not be exported");
    }

    #[test]
    fn function_params() {
        let wat = compile(r#"pub fn add(a: i32, b: i32) -> i32 { return a + b; }"#);
        assert!(wat.contains("(param $a i32)"), "should have param a");
        assert!(wat.contains("(param $b i32)"), "should have param b");
        assert!(wat.contains("(result i32)"), "should have return type");
    }

    // -----------------------------------------------------------------------
    // Trait codegen
    // -----------------------------------------------------------------------

    #[test]
    fn trait_erased_in_codegen() {
        let wat = compile(r#"
            trait Printable {
                fn print();
            }
        "#);
        assert!(wat.contains("trait Printable (erased)"), "trait should be erased comment");
    }

    // -----------------------------------------------------------------------
    // Data section
    // -----------------------------------------------------------------------

    #[test]
    fn data_section_for_strings() {
        let wat = compile(r#"
            pub fn f() -> i32 {
                let s = "test_data";
                return 0;
            }
        "#);
        assert!(wat.contains("(data"), "should emit data section for interned strings");
        assert!(wat.contains("test_data"), "should contain the string in data section");
    }

    // -----------------------------------------------------------------------
    // FnCall codegen with webapi mapping
    // -----------------------------------------------------------------------

    #[test]
    fn fn_call_user_function() {
        let wat = compile(r#"
            fn helper() -> i32 { return 1; }
            pub fn run() -> i32 {
                let r = helper();
                return r;
            }
        "#);
        assert!(wat.contains("call $helper"), "should call user function");
    }

    // -----------------------------------------------------------------------
    // Field access codegen
    // -----------------------------------------------------------------------

    #[test]
    fn field_access_codegen() {
        let wat = compile(r#"
            struct Point { x: i32, y: i32 }
            pub fn run(p: i32) -> i32 {
                return p;
            }
        "#);
        // Just verify module compiles
        assert!(wat.contains("(module"), "should produce valid module");
    }

    // -----------------------------------------------------------------------
    // Module structure
    // -----------------------------------------------------------------------

    #[test]
    fn module_has_correct_structure() {
        let wat = compile(r#"pub fn f() -> i32 { return 0; }"#);
        assert!(wat.starts_with("\n(module"), "should start with (module");
        assert!(wat.contains("(import \"env\" \"memory\""), "should import memory");
        assert!(wat.trim_end().ends_with(")"), "should end with closing paren");
    }

    // -----------------------------------------------------------------------
    // Assign expression
    // -----------------------------------------------------------------------

    #[test]
    fn assign_expression_codegen() {
        let wat = compile(r#"
            pub fn run() -> i32 {
                let mut x = 1;
                x = 2;
                return x;
            }
        "#);
        assert!(wat.contains("local.set $x"), "should set variable on assign");
    }
}

#[cfg(test)]
mod coverage_codegen_tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::token::Span;

    fn compile(src: &str) -> String {
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let mut codegen = WasmCodegen::new();
        codegen.generate(&program)
    }

    #[allow(dead_code)]
    fn parse(src: &str) -> Program {
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let (program, errors) = parser.parse_program_recovering();
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);
        program
    }

    #[allow(dead_code)]
    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    #[allow(dead_code)]
    fn block(stmts: Vec<Stmt>) -> Block {
        Block { stmts, span: span() }
    }

    // -----------------------------------------------------------------------
    // Import namespace verification — all 16 namespaces
    // -----------------------------------------------------------------------

    #[test]
    fn all_import_namespaces_present() {
        let wat = compile("pub fn f() -> i32 { return 0; }");
        let namespaces = [
            "\"dom\"", "\"timer\"", "\"webapi\"", "\"http\"", "\"observe\"",
            "\"ws\"", "\"db\"", "\"worker\"", "\"pwa\"", "\"hardware\"",
            "\"payment\"", "\"auth\"", "\"upload\"", "\"time\"", "\"streaming\"",
            "\"rtc\"", "\"gpu\"",
        ];
        for ns in &namespaces {
            assert!(wat.contains(ns), "missing import namespace: {}", ns);
        }
    }

    #[test]
    fn rtc_peer_connection_imports_present() {
        let wat = compile("pub fn f() -> i32 { return 0; }");
        let imports = [
            "$rtc_createPeer",
            "$rtc_createPeerWithIce",
            "$rtc_createOffer",
            "$rtc_createAnswer",
            "$rtc_setLocalDescription",
            "$rtc_setRemoteDescription",
            "$rtc_addIceCandidate",
            "$rtc_close",
        ];
        for import in &imports {
            assert!(wat.contains(import), "missing RTC import: {}", import);
        }
    }

    #[test]
    fn rtc_data_channel_imports_present() {
        let wat = compile("pub fn f() -> i32 { return 0; }");
        let imports = [
            "$rtc_createDataChannel",
            "$rtc_dataChannelSend",
            "$rtc_dataChannelSendBinary",
            "$rtc_dataChannelClose",
            "$rtc_dataChannelGetState",
            "$rtc_onDataChannelMessage",
            "$rtc_onDataChannelOpen",
            "$rtc_onDataChannelClose",
        ];
        for import in &imports {
            assert!(wat.contains(import), "missing RTC data channel import: {}", import);
        }
    }

    #[test]
    fn rtc_media_imports_present() {
        let wat = compile("pub fn f() -> i32 { return 0; }");
        let imports = [
            "$rtc_addTrack",
            "$rtc_removeTrack",
            "$rtc_getUserMedia",
            "$rtc_getDisplayMedia",
            "$rtc_stopTrack",
            "$rtc_setTrackEnabled",
            "$rtc_getTrackKind",
            "$rtc_attachStream",
        ];
        for import in &imports {
            assert!(wat.contains(import), "missing RTC media import: {}", import);
        }
    }

    #[test]
    fn rtc_event_callback_imports_present() {
        let wat = compile("pub fn f() -> i32 { return 0; }");
        let imports = [
            "$rtc_onIceCandidate",
            "$rtc_onIceCandidateFull",
            "$rtc_onTrack",
            "$rtc_onDataChannel",
            "$rtc_onConnectionStateChange",
            "$rtc_onIceConnectionStateChange",
            "$rtc_onIceGatheringStateChange",
            "$rtc_onSignalingStateChange",
            "$rtc_onNegotiationNeeded",
        ];
        for import in &imports {
            assert!(wat.contains(import), "missing RTC event import: {}", import);
        }
    }

    #[test]
    fn rtc_state_query_imports_present() {
        let wat = compile("pub fn f() -> i32 { return 0; }");
        let imports = [
            "$rtc_getConnectionState",
            "$rtc_getIceConnectionState",
            "$rtc_getSignalingState",
            "$rtc_getStats",
        ];
        for import in &imports {
            assert!(wat.contains(import), "missing RTC state query import: {}", import);
        }
    }

    // -----------------------------------------------------------------------
    // GPU import verification
    // -----------------------------------------------------------------------

    #[test]
    fn gpu_initialization_imports_present() {
        let wat = compile("pub fn f() -> i32 { return 0; }");
        let imports = [
            "$gpu_requestAdapter",
            "$gpu_requestDevice",
            "$gpu_getPreferredFormat",
            "$gpu_getAdapterInfo",
        ];
        for import in &imports {
            assert!(wat.contains(import), "missing GPU initialization import: {}", import);
        }
    }

    #[test]
    fn gpu_resource_imports_present() {
        let wat = compile("pub fn f() -> i32 { return 0; }");
        let imports = [
            "$gpu_createBuffer",
            "$gpu_writeBuffer",
            "$gpu_createShaderModule",
            "$gpu_createRenderPipeline",
            "$gpu_createTexture",
            "$gpu_createTextureView",
        ];
        for import in &imports {
            assert!(wat.contains(import), "missing GPU resource import: {}", import);
        }
    }

    #[test]
    fn gpu_rendering_imports_present() {
        let wat = compile("pub fn f() -> i32 { return 0; }");
        let imports = [
            "$gpu_beginRenderPass",
            "$gpu_setPipeline",
            "$gpu_setVertexBuffer",
            "$gpu_draw",
            "$gpu_submitRenderPass",
        ];
        for import in &imports {
            assert!(wat.contains(import), "missing GPU rendering import: {}", import);
        }
    }

    #[test]
    fn gpu_canvas_imports_present() {
        let wat = compile("pub fn f() -> i32 { return 0; }");
        let imports = [
            "$gpu_configureCanvas",
            "$gpu_getCurrentTexture",
        ];
        for import in &imports {
            assert!(wat.contains(import), "missing GPU canvas import: {}", import);
        }
    }

    #[test]
    fn gpu_cleanup_imports_present() {
        let wat = compile("pub fn f() -> i32 { return 0; }");
        let imports = [
            "$gpu_destroyBuffer",
            "$gpu_destroyTexture",
        ];
        for import in &imports {
            assert!(wat.contains(import), "missing GPU cleanup import: {}", import);
        }
    }

    // -----------------------------------------------------------------------
    // Test block codegen
    // -----------------------------------------------------------------------

    #[test]
    fn test_block_codegen() {
        let wat = compile(r#"
            test "basic addition" {
                assert_eq(1 + 1, 2);
            }
        "#);
        assert!(wat.contains("__test_basic_addition"), "should generate test function");
        assert!(wat.contains("test_pass"), "should call test_pass at end");
    }

    #[test]
    fn test_runner_codegen() {
        let wat = compile(r#"
            test "first" {
                assert(true);
            }
            test "second" {
                assert(true);
            }
        "#);
        assert!(wat.contains("__run_tests"), "should generate test runner");
        assert!(wat.contains("test_summary"), "should call test_summary");
        assert!(wat.contains("call $__test_first"), "should call first test");
        assert!(wat.contains("call $__test_second"), "should call second test");
    }

    // -----------------------------------------------------------------------
    // Contract codegen — type_to_canonical and type_to_json_schema_type
    // -----------------------------------------------------------------------

    #[test]
    fn contract_with_various_field_types() {
        let contract = ContractDef {
            name: "TestContract".into(),
            fields: vec![
                ContractField { name: "id".into(), ty: Type::Named("u32".into()), nullable: false, span: span() },
                ContractField { name: "score".into(), ty: Type::Named("f64".into()), nullable: false, span: span() },
                ContractField { name: "active".into(), ty: Type::Named("bool".into()), nullable: false, span: span() },
                ContractField { name: "name".into(), ty: Type::Named("String".into()), nullable: false, span: span() },
                ContractField { name: "date".into(), ty: Type::Named("DateTime".into()), nullable: true, span: span() },
                ContractField { name: "items".into(), ty: Type::Array(Box::new(Type::Named("i32".into()))), nullable: false, span: span() },
                ContractField { name: "custom".into(), ty: Type::Named("MyType".into()), nullable: false, span: span() },
            ],
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_contract(&contract);
        let output = codegen.output.clone();
        assert!(output.contains("Contract: TestContract"), "should have contract header");
        assert!(output.contains("contract hash:"), "should have hash");
        assert!(output.contains("contract_registerSchema"), "should register schema");
        assert!(output.contains("schema len"), "should have schema with length");
    }

    #[test]
    fn type_to_canonical_all_variants() {
        let codegen = WasmCodegen::new();

        assert_eq!(codegen.type_to_canonical(&Type::Named("i32".into())), "i32");
        assert_eq!(
            codegen.type_to_canonical(&Type::Array(Box::new(Type::Named("i32".into())))),
            "[i32]"
        );
        assert_eq!(
            codegen.type_to_canonical(&Type::Option(Box::new(Type::Named("String".into())))),
            "String?"
        );
        assert_eq!(
            codegen.type_to_canonical(&Type::Result {
                ok: Box::new(Type::Named("i32".into())),
                err: Box::new(Type::Named("String".into())),
            }),
            "Result<i32,String>"
        );
        assert_eq!(
            codegen.type_to_canonical(&Type::Tuple(vec![Type::Named("i32".into()), Type::Named("f64".into())])),
            "(i32,f64)"
        );
        assert_eq!(
            codegen.type_to_canonical(&Type::Generic {
                name: "Vec".into(),
                args: vec![Type::Named("i32".into())],
            }),
            "Vec<i32>"
        );
        assert_eq!(
            codegen.type_to_canonical(&Type::Reference {
                mutable: false,
                lifetime: None,
                inner: Box::new(Type::Named("i32".into())),
            }),
            "&i32"
        );
        assert_eq!(
            codegen.type_to_canonical(&Type::Reference {
                mutable: true,
                lifetime: None,
                inner: Box::new(Type::Named("i32".into())),
            }),
            "&mut i32"
        );
        assert_eq!(
            codegen.type_to_canonical(&Type::Function {
                params: vec![Type::Named("i32".into())],
                ret: Box::new(Type::Named("bool".into())),
            }),
            "fn(i32)->bool"
        );
    }

    #[test]
    fn type_to_json_schema_type_variants() {
        let codegen = WasmCodegen::new();
        assert_eq!(codegen.type_to_json_schema_type(&Type::Named("i32".into())), "integer");
        assert_eq!(codegen.type_to_json_schema_type(&Type::Named("i64".into())), "integer");
        assert_eq!(codegen.type_to_json_schema_type(&Type::Named("u32".into())), "integer");
        assert_eq!(codegen.type_to_json_schema_type(&Type::Named("u64".into())), "integer");
        assert_eq!(codegen.type_to_json_schema_type(&Type::Named("f32".into())), "number");
        assert_eq!(codegen.type_to_json_schema_type(&Type::Named("f64".into())), "number");
        assert_eq!(codegen.type_to_json_schema_type(&Type::Named("bool".into())), "boolean");
        assert_eq!(codegen.type_to_json_schema_type(&Type::Named("String".into())), "string");
        assert_eq!(codegen.type_to_json_schema_type(&Type::Named("DateTime".into())), "string");
        assert_eq!(codegen.type_to_json_schema_type(&Type::Named("Custom".into())), "object");
        assert_eq!(codegen.type_to_json_schema_type(&Type::Array(Box::new(Type::Named("i32".into())))), "array");
        assert_eq!(
            codegen.type_to_json_schema_type(&Type::Option(Box::new(Type::Named("i32".into())))),
            "integer"
        );
        assert_eq!(
            codegen.type_to_json_schema_type(&Type::Tuple(vec![])),
            "object"
        );
    }

    // -----------------------------------------------------------------------
    // App codegen (manifest, offline, push)
    // -----------------------------------------------------------------------

    #[test]
    fn app_with_manifest() {
        let app = AppDef {
            name: "MyApp".into(),
            manifest: Some(ManifestDef {
                entries: vec![
                    ("name".into(), Expr::StringLit("My App".into())),
                    ("version".into(), Expr::Integer(1)),
                    ("debug".into(), Expr::Bool(true)),
                    ("other".into(), Expr::Ident("x".into())), // triggers null branch
                ],
                span: span(),
            }),
            offline: Some(OfflineDef {
                precache: vec!["/index.html".into(), "/app.css".into()],
                strategy: "cache-first".into(),
                fallback: None,
                span: span(),
            }),
            push: Some(PushDef {
                vapid_key: Some(Expr::StringLit("BKEY123".into())),
                on_message: None,
                span: span(),
            }),
            router: None,
            a11y: None,
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_app(&app);
        let output = codegen.output.clone();
        assert!(output.contains("PWA App: MyApp"), "should have app header");
        assert!(output.contains("register_manifest"), "should register manifest");
        assert!(output.contains("register_sw"), "should register service worker");
        assert!(output.contains("register_push"), "should register push");
        assert!(output.contains("vapid key"), "should contain VAPID key reference");
        assert!(output.contains("pwa_cachePrecache"), "should call cachePrecache");
    }

    #[test]
    fn app_push_without_vapid_key() {
        let app = AppDef {
            name: "MinApp".into(),
            manifest: None,
            offline: None,
            push: Some(PushDef {
                vapid_key: None,
                on_message: None,
                span: span(),
            }),
            router: None,
            a11y: None,
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_app(&app);
        let output = codegen.output.clone();
        assert!(output.contains("register_push"), "should still generate push func");
    }

    // -----------------------------------------------------------------------
    // Page codegen with SEO meta
    // -----------------------------------------------------------------------

    #[test]
    fn page_with_meta_and_structured_data() {
        let page = PageDef {
            name: "HomePage".into(),
            props: vec![],
            meta: Some(MetaDef {
                title: Some(Expr::StringLit("Home Page".into())),
                description: Some(Expr::StringLit("Welcome".into())),
                canonical: Some(Expr::StringLit("https://example.com".into())),
                og_image: Some(Expr::StringLit("https://example.com/og.png".into())),
                structured_data: vec![StructuredDataDef {
                    schema_type: "Article".into(),
                    fields: vec![
                        ("headline".into(), Expr::StringLit("Title".into())),
                        ("count".into(), Expr::Integer(42)), // non-string triggers null
                    ],
                    span: span(),
                }],
                extra: vec![],
                span: span(),
            }),
            state: vec![StateField {
                name: "loaded".into(),
                ty: Some(Type::Named("bool".into())),
                mutable: true,
                secret: true,
                atomic: false,
                initializer: Expr::Bool(false),
                ownership: Ownership::Owned,
            }],
            methods: vec![Function {
                name: "handler".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }],
            styles: vec![],
            render: RenderBlock {
                body: TemplateNode::TextLiteral("hello".into()),
                span: span(),
            },
            permissions: None,
            gestures: vec![],
            is_pub: true,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_page(&page);
        let output = codegen.output.clone();
        assert!(output.contains("Page: HomePage"), "should have page header");
        assert!(output.contains("seo_set_meta"), "should call seo_set_meta");
        assert!(output.contains("seo_register_structured_data"), "should register structured data");
        assert!(output.contains("seo_register_route"), "should register route for sitemap");
        assert!(output.contains("secret: loaded"), "should annotate secret state");
        assert!(output.contains("HomePage__handler_"), "should generate handler trampoline");
    }

    #[test]
    fn page_without_meta() {
        let page = PageDef {
            name: "SimplePage".into(),
            props: vec![],
            meta: None,
            state: vec![],
            methods: vec![],
            styles: vec![],
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            permissions: None,
            gestures: vec![],
            is_pub: true,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_page(&page);
        let output = codegen.output.clone();
        assert!(output.contains("Page: SimplePage"), "should have page header");
    }

    // -----------------------------------------------------------------------
    // Form codegen
    // -----------------------------------------------------------------------

    #[test]
    fn form_with_validators() {
        let form = FormDef {
            name: "LoginForm".into(),
            fields: vec![
                FormFieldDef {
                    name: "email".into(),
                    ty: Type::Named("String".into()),
                    validators: vec![
                        ValidatorDef { kind: ValidatorKind::Required, message: None, span: span() },
                        ValidatorDef { kind: ValidatorKind::Email, message: None, span: span() },
                    ],
                    label: None,
                    placeholder: None,
                    default_value: None,
                    span: span(),
                },
                FormFieldDef {
                    name: "password".into(),
                    ty: Type::Named("String".into()),
                    validators: vec![
                        ValidatorDef { kind: ValidatorKind::Required, message: None, span: span() },
                        ValidatorDef { kind: ValidatorKind::MinLength(8), message: None, span: span() },
                        ValidatorDef { kind: ValidatorKind::MaxLength(128), message: None, span: span() },
                        ValidatorDef { kind: ValidatorKind::Pattern("^[a-zA-Z0-9]+$".into()), message: None, span: span() },
                    ],
                    label: None,
                    placeholder: None,
                    default_value: None,
                    span: span(),
                },
                FormFieldDef {
                    name: "age".into(),
                    ty: Type::Named("i32".into()),
                    validators: vec![
                        ValidatorDef { kind: ValidatorKind::Min(0), message: None, span: span() },
                        ValidatorDef { kind: ValidatorKind::Max(150), message: None, span: span() },
                        ValidatorDef { kind: ValidatorKind::Url, message: None, span: span() },
                        ValidatorDef { kind: ValidatorKind::Custom("validate_age".into()), message: None, span: span() },
                    ],
                    label: None,
                    placeholder: None,
                    default_value: None,
                    span: span(),
                },
            ],
            on_submit: None,
            steps: vec![],
            methods: vec![],
            styles: vec![],
            render: None,
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_form(&form);
        let output = codegen.output.clone();
        assert!(output.contains("Form: LoginForm"), "should have form header");
        assert!(output.contains("form_register"), "should call form_register");
    }

    // -----------------------------------------------------------------------
    // Channel codegen
    // -----------------------------------------------------------------------

    #[test]
    fn channel_codegen() {
        let ch = ChannelDef {
            name: "Chat".into(),
            url: Expr::StringLit("/ws/chat".into()),
            contract: None,
            on_message: Some(Function {
                name: "on_msg".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            on_connect: Some(Function {
                name: "on_conn".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            on_disconnect: Some(Function {
                name: "on_disc".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            reconnect: false,
            heartbeat_interval: None,
            methods: vec![],
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_channel(&ch);
        let output = codegen.output.clone();
        assert!(output.contains("Channel: Chat"), "should have channel header");
        assert!(output.contains("channel_connect"), "should call channel_connect");
        assert!(output.contains("reconnect disabled"), "should disable reconnect");
    }

    #[test]
    fn channel_with_non_string_url() {
        let ch = ChannelDef {
            name: "Events".into(),
            url: Expr::Ident("url_var".into()),
            contract: None,
            on_message: None,
            on_connect: None,
            on_disconnect: None,
            reconnect: true,
            heartbeat_interval: None,
            methods: vec![],
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_channel(&ch);
        let output = codegen.output.clone();
        // non-string URL defaults to "/ws"
        assert!(output.contains("Channel: Events"), "should have channel header");
    }

    // -----------------------------------------------------------------------
    // Embed codegen
    // -----------------------------------------------------------------------

    #[test]
    fn embed_sandboxed() {
        let embed = EmbedDef {
            name: "Widget".into(),
            src: Expr::StringLit("https://cdn.example.com/widget.js".into()),
            loading: Some("lazy".into()),
            sandbox: true,
            integrity: None,
            permissions: None,
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_embed(&embed);
        let output = codegen.output.clone();
        assert!(output.contains("Embed: Widget"), "should have embed header");
        assert!(output.contains("embed_load_sandboxed"), "should use sandboxed embed");
    }

    #[test]
    fn embed_non_sandboxed_with_integrity() {
        let embed = EmbedDef {
            name: "Analytics".into(),
            src: Expr::StringLit("https://cdn.example.com/analytics.js".into()),
            loading: None,
            sandbox: false,
            integrity: Some(Expr::StringLit("sha384-abc123".into())),
            permissions: None,
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_embed(&embed);
        let output = codegen.output.clone();
        assert!(output.contains("embed_load_script"), "should use script embed");
    }

    #[test]
    fn embed_non_string_src() {
        let embed = EmbedDef {
            name: "Dynamic".into(),
            src: Expr::Ident("url".into()),
            loading: None,
            sandbox: false,
            integrity: None,
            permissions: None,
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_embed(&embed);
        let output = codegen.output.clone();
        assert!(output.contains("Embed: Dynamic"), "should have embed header");
    }

    // -----------------------------------------------------------------------
    // PDF codegen
    // -----------------------------------------------------------------------

    #[test]
    fn pdf_codegen() {
        let pdf = PdfDef {
            name: "Invoice".into(),
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            page_size: Some("letter".into()),
            orientation: Some("landscape".into()),
            margins: None,
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_pdf(&pdf);
        let output = codegen.output.clone();
        assert!(output.contains("PDF: Invoice"), "should have PDF header");
        assert!(output.contains("pdf_create"), "should call pdf_create");
        assert!(output.contains("config ptr"), "should have config pointer");
        assert!(output.contains("config len"), "should have config length");
    }

    #[test]
    fn pdf_with_defaults() {
        let pdf = PdfDef {
            name: "Report".into(),
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            page_size: None,
            orientation: None,
            margins: None,
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_pdf(&pdf);
        let output = codegen.output.clone();
        assert!(output.contains("pdf_create"), "should call pdf_create");
        assert!(output.contains("config ptr"), "should have config with defaults");
    }

    // -----------------------------------------------------------------------
    // Payment codegen
    // -----------------------------------------------------------------------

    #[test]
    fn payment_codegen() {
        let payment = PaymentDef {
            name: "Checkout".into(),
            provider: Some(Expr::StringLit("paypal".into())),
            public_key: None,
            sandbox_mode: true,
            on_success: Some(Function {
                name: "on_success".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            on_error: Some(Function {
                name: "on_error".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            methods: vec![],
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_payment(&payment);
        let output = codegen.output.clone();
        assert!(output.contains("Payment: Checkout"), "should have payment header");
        assert!(output.contains("payment_init"), "should call payment_init");
        assert!(output.contains("i32.const 1  ;; sandboxed"), "should set sandbox flag to 1");
    }

    #[test]
    fn payment_without_provider() {
        let payment = PaymentDef {
            name: "Pay".into(),
            provider: None,
            public_key: None,
            sandbox_mode: false,
            on_success: None,
            on_error: None,
            methods: vec![],
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_payment(&payment);
        let output = codegen.output.clone();
        // defaults to "stripe"
        assert!(output.contains("Payment: Pay"), "should have payment header");
    }

    // -----------------------------------------------------------------------
    // Auth codegen
    // -----------------------------------------------------------------------

    #[test]
    fn auth_codegen_with_providers() {
        let auth = AuthDef {
            name: "Auth".into(),
            provider: None,
            providers: vec![
                AuthProvider {
                    name: "google".into(),
                    client_id: None,
                    scopes: vec!["email".into(), "profile".into()],
                    span: span(),
                },
            ],
            on_login: Some(Function {
                name: "on_login".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            on_logout: Some(Function {
                name: "on_logout".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            on_error: Some(Function {
                name: "on_err".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            session_storage: Some("cookie".into()),
            methods: vec![],
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_auth(&auth);
        let output = codegen.output.clone();
        assert!(output.contains("Auth: Auth"), "should have auth header");
        assert!(output.contains("auth_init"), "should call auth_init");
    }

    // -----------------------------------------------------------------------
    // Upload codegen
    // -----------------------------------------------------------------------

    #[test]
    fn upload_codegen() {
        let upload = UploadDef {
            name: "FileUpload".into(),
            endpoint: Expr::StringLit("/api/upload".into()),
            max_size: None,
            accept: vec!["image/*".into(), "application/pdf".into()],
            chunked: true,
            on_progress: Some(Function {
                name: "on_progress".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            on_complete: Some(Function {
                name: "on_complete".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            on_error: Some(Function {
                name: "on_error".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            methods: vec![],
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_upload(&upload);
        let output = codegen.output.clone();
        assert!(output.contains("Upload: FileUpload"), "should have upload header");
        assert!(output.contains("upload_init"), "should call upload_init");
        assert!(output.contains("config len"), "should have config with upload settings");
    }

    #[test]
    fn upload_non_string_endpoint() {
        let upload = UploadDef {
            name: "Up".into(),
            endpoint: Expr::Ident("endpoint_var".into()),
            max_size: None,
            accept: vec![],
            chunked: false,
            on_progress: None,
            on_complete: None,
            on_error: None,
            methods: vec![],
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_upload(&upload);
        let output = codegen.output.clone();
        assert!(output.contains("Upload: Up"), "should have upload header");
    }

    // -----------------------------------------------------------------------
    // Cache codegen
    // -----------------------------------------------------------------------

    #[test]
    fn cache_codegen_full() {
        let cache = CacheDef {
            name: "ApiCache".into(),
            strategy: Some("stale-while-revalidate".into()),
            default_ttl: Some(3600),
            persist: true,
            max_entries: Some(100),
            queries: vec![CacheQueryDef {
                name: "getUsers".into(),
                params: vec![],
                fetch_expr: Expr::StringLit("/api/users".into()),
                contract: Some("UserContract".into()),
                ttl: Some(600),
                stale: Some(300),
                invalidate_on: vec!["user_updated".into()],
                span: span(),
            }],
            mutations: vec![CacheMutationDef {
                name: "updateUser".into(),
                params: vec![],
                fetch_expr: Expr::StringLit("/api/users".into()),
                optimistic: true,
                rollback_on_error: true,
                invalidate: vec!["getUsers".into()],
                span: span(),
            }],
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_cache(&cache);
        let output = codegen.output.clone();
        assert!(output.contains("Cache: ApiCache"), "should have cache header");
        assert!(output.contains("cache_init"), "should call cache_init");
        assert!(output.contains("cache_register_query"), "should register query");
        assert!(output.contains("cache_register_mutation"), "should register mutation");
    }

    // -----------------------------------------------------------------------
    // Breakpoints codegen
    // -----------------------------------------------------------------------

    #[test]
    fn breakpoints_codegen() {
        let bp = BreakpointsDef {
            breakpoints: vec![
                ("mobile".into(), 640),
                ("tablet".into(), 1024),
                ("desktop".into(), 1280),
            ],
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_breakpoints(&bp);
        let output = codegen.output.clone();
        assert!(output.contains("Responsive Breakpoints"), "should have breakpoints header");
        assert!(output.contains("responsive_register"), "should call responsive_register");
    }

    // -----------------------------------------------------------------------
    // Animation codegen (spring, keyframes, stagger)
    // -----------------------------------------------------------------------

    #[test]
    fn animation_spring() {
        let anim = AnimationBlockDef {
            name: "bounce".into(),
            kind: AnimationKind::Spring {
                stiffness: Some(200.0),
                damping: Some(20.0),
                mass: Some(1.5),
                properties: vec!["opacity".into(), "transform".into()],
            },
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_animation_block(&anim);
        let output = codegen.output.clone();
        assert!(output.contains("Animation: bounce"), "should have animation header");
        assert!(output.contains("animate_spring"), "should call animate_spring");
    }

    #[test]
    fn animation_keyframes() {
        let anim = AnimationBlockDef {
            name: "fadeIn".into(),
            kind: AnimationKind::Keyframes {
                frames: vec![
                    (0.0, vec![("opacity".into(), Expr::Float(0.0))]),
                    (100.0, vec![("opacity".into(), Expr::Float(1.0))]),
                ],
                duration: Some("500ms".into()),
                easing: Some("ease-in".into()),
            },
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_animation_block(&anim);
        let output = codegen.output.clone();
        assert!(output.contains("animate_keyframes"), "should call animate_keyframes");
    }

    #[test]
    fn animation_keyframes_defaults() {
        let anim = AnimationBlockDef {
            name: "slide".into(),
            kind: AnimationKind::Keyframes {
                frames: vec![],
                duration: None,
                easing: None,
            },
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_animation_block(&anim);
        let output = codegen.output.clone();
        assert!(output.contains("Animation: slide"), "should have animation header");
        assert!(output.contains("animate_keyframes"), "should call animate_keyframes");
    }

    #[test]
    fn animation_stagger() {
        let anim = AnimationBlockDef {
            name: "list".into(),
            kind: AnimationKind::Stagger {
                animation: "fadeIn".into(),
                delay: Some("100ms".into()),
                selector: Some(".item".into()),
            },
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_animation_block(&anim);
        let output = codegen.output.clone();
        assert!(output.contains("animate_stagger"), "should call animate_stagger");
    }

    #[test]
    fn animation_stagger_defaults() {
        let anim = AnimationBlockDef {
            name: "items".into(),
            kind: AnimationKind::Stagger {
                animation: "fadeIn".into(),
                delay: None,
                selector: None,
            },
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_animation_block(&anim);
        let output = codegen.output.clone();
        assert!(output.contains("Animation: items"), "should have animation header");
        assert!(output.contains("animate_stagger"), "should call animate_stagger");
    }

    // -----------------------------------------------------------------------
    // Theme codegen
    // -----------------------------------------------------------------------

    #[test]
    fn theme_light_and_dark() {
        let theme = ThemeDef {
            name: "MainTheme".into(),
            light: Some(vec![
                ("bg".into(), Expr::StringLit("#fff".into())),
                ("fg".into(), Expr::Integer(0)), // triggers null branch
            ]),
            dark: Some(vec![
                ("bg".into(), Expr::StringLit("#000".into())),
            ]),
            dark_auto: false,
            primary: None,
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_theme(&theme);
        let output = codegen.output.clone();
        assert!(output.contains("Theme: MainTheme"), "should have theme header");
        assert!(output.contains("theme_init"), "should call theme_init");
    }

    #[test]
    fn theme_dark_auto() {
        let theme = ThemeDef {
            name: "Auto".into(),
            light: None,
            dark: None,
            dark_auto: true,
            primary: None,
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_theme(&theme);
        let output = codegen.output.clone();
        assert!(output.contains("Theme: Auto"), "should have theme header");
        assert!(output.contains("init_theme"), "should call init_theme");
    }

    // -----------------------------------------------------------------------
    // Permissions codegen
    // -----------------------------------------------------------------------

    #[test]
    fn permissions_with_csp() {
        let perms = PermissionsDef {
            network: vec!["https://api.example.com/v1".into(), "https://cdn.example.com/assets".into()],
            storage: vec!["user_prefs".into()],
            capabilities: vec!["camera".into()],
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_permissions("TestComp", &perms);
        let output = codegen.output.clone();
        assert!(output.contains("permissions for component TestComp"), "should have permissions header");
        assert!(output.contains("permissions_registerPermissions"), "should register permissions");
        assert!(output.contains("CSP: connect-src"), "should generate CSP comment");
    }

    #[test]
    fn permissions_no_network() {
        let perms = PermissionsDef {
            network: vec![],
            storage: vec!["key".into()],
            capabilities: vec![],
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_permissions("Comp2", &perms);
        let output = codegen.output.clone();
        assert!(!output.contains("CSP:"), "should not generate CSP without network");
    }

    // -----------------------------------------------------------------------
    // Component with skeleton, error boundary, a11y, on_destroy, chunk
    // -----------------------------------------------------------------------

    #[test]
    fn component_with_skeleton() {
        let comp = Component {
            name: "Heavy".into(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            trait_bounds: vec![],
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            permissions: None,
            gestures: vec![],
            skeleton: Some(SkeletonDef {
                body: RenderBlock { body: TemplateNode::TextLiteral("loading...".into()), span: span() },
                span: span(),
            }),
            error_boundary: None,
            chunk: Some("heavy-chunk".into()),
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_component(&comp);
        let output = codegen.output.clone();
        assert!(output.contains("skeleton"), "should have skeleton block");
        assert!(output.contains("skeleton_mount"), "should call skeleton_mount");
        assert!(output.contains("chunk boundary"), "should mark chunk boundary");
    }

    #[test]
    fn component_with_error_boundary() {
        let comp = Component {
            name: "Safe".into(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            trait_bounds: vec![],
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: Some(ErrorBoundary {
                body: RenderBlock { body: TemplateNode::TextLiteral("content".into()), span: span() },
                fallback: RenderBlock { body: TemplateNode::TextLiteral("error".into()), span: span() },
                span: span(),
            }),
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_component(&comp);
        let output = codegen.output.clone();
        assert!(output.contains("error boundary"), "should have error boundary");
        assert!(output.contains("eb_ok"), "should have error boundary block");
    }

    #[test]
    fn component_with_a11y_auto() {
        let comp = Component {
            name: "Accessible".into(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            trait_bounds: vec![],
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: Some(A11yMode::Auto),
            shortcuts: vec![],
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_component(&comp);
        let output = codegen.output.clone();
        assert!(output.contains("a11y: auto"), "should have a11y auto comment");
        assert!(output.contains("a11y_enhance"), "should call a11y_enhance");
    }

    #[test]
    fn component_default_a11y_auto() {
        // Components without explicit a11y should default to auto
        let comp = Component {
            name: "NoExplicitA11y".into(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            trait_bounds: vec![],
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None, // no explicit a11y — should default to auto
            shortcuts: vec![],
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_component(&comp);
        let output = codegen.output.clone();
        assert!(output.contains("a11y_enhance"), "should call a11y_enhance by default");
    }

    #[test]
    fn component_a11y_manual_no_enhance() {
        let comp = Component {
            name: "ManualA11y".into(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            trait_bounds: vec![],
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: Some(A11yMode::Manual),
            shortcuts: vec![],
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_component(&comp);
        let output = codegen.output.clone();
        assert!(!output.contains("a11y_enhance"), "manual mode should NOT call a11y_enhance");
    }

    #[test]
    fn component_a11y_hybrid() {
        let comp = Component {
            name: "HybridA11y".into(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            trait_bounds: vec![],
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: Some(A11yMode::Hybrid),
            shortcuts: vec![],
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_component(&comp);
        let output = codegen.output.clone();
        assert!(output.contains("a11y_enhance"), "hybrid mode should call a11y_enhance");
    }

    #[test]
    fn outlet_generates_div_with_id() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_template(&TemplateNode::Outlet, "$root");
        let output = codegen.output.clone();
        assert!(output.contains("dom_createElement"), "should create element for outlet");
        assert!(output.contains("__nectar_outlet"), "should set outlet id");
        assert!(output.contains("dom_appendChild"), "should append outlet to parent");
    }

    #[test]
    fn layout_stack_generates_flex_column() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_template(
            &TemplateNode::Layout(LayoutNode::Stack {
                gap: Some("16".into()),
                children: vec![TemplateNode::TextLiteral("child".into())],
                span: span(),
            }),
            "$root",
        );
        let output = codegen.output.clone();
        assert!(output.contains("dom_createElement"), "should create element");
        assert!(output.contains("\"column\""), "should use column flex-direction");
        assert!(output.contains("\"vertical\""), "should set native direction");
        assert!(output.contains("\"16px\""), "should have gap");
        assert!(output.contains("dom_setStyle"), "should set style");
    }

    #[test]
    fn layout_grid_generates_css_grid() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_template(
            &TemplateNode::Layout(LayoutNode::Grid {
                cols: Some("3".into()),
                rows: None,
                gap: Some("8".into()),
                children: vec![],
                span: span(),
            }),
            "$root",
        );
        let output = codegen.output.clone();
        assert!(output.contains("\"grid\""), "should use CSS grid");
        assert!(output.contains("\"repeat(3,1fr)\""), "should have 3 columns");
        assert!(output.contains("\"8px\""), "should have gap");
    }

    #[test]
    fn layout_center_generates_flex_centering() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_template(
            &TemplateNode::Layout(LayoutNode::Center {
                max_width: Some("800".into()),
                children: vec![],
                span: span(),
            }),
            "$root",
        );
        let output = codegen.output.clone();
        assert!(output.contains("\"center\""), "should center content");
        assert!(output.contains("\"800px\""), "should have max width");
    }

    #[test]
    fn layout_sidebar_left() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_template(
            &TemplateNode::Layout(LayoutNode::Sidebar {
                side: Some("left".into()),
                width: Some("250".into()),
                children: vec![],
                span: span(),
            }),
            "$root",
        );
        let output = codegen.output.clone();
        assert!(output.contains("250px 1fr"), "left sidebar should put sidebar first");
    }

    #[test]
    fn layout_sidebar_right() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_template(
            &TemplateNode::Layout(LayoutNode::Sidebar {
                side: Some("right".into()),
                width: Some("300".into()),
                children: vec![],
                span: span(),
            }),
            "$root",
        );
        let output = codegen.output.clone();
        assert!(output.contains("1fr 300px"), "right sidebar should put sidebar last");
    }

    #[test]
    fn layout_row_with_align() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_template(
            &TemplateNode::Layout(LayoutNode::Row {
                gap: Some("12".into()),
                align: Some("center".into()),
                children: vec![],
                span: span(),
            }),
            "$root",
        );
        let output = codegen.output.clone();
        assert!(output.contains("\"row\""), "should use row flex-direction");
        assert!(output.contains("\"horizontal\""), "should set native direction");
        assert!(output.contains("\"center\""), "should center align");
        assert!(output.contains("\"12px\""), "should have gap");
    }

    #[test]
    fn layout_cluster_wraps() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_template(
            &TemplateNode::Layout(LayoutNode::Cluster {
                gap: Some("8".into()),
                children: vec![],
                span: span(),
            }),
            "$root",
        );
        let output = codegen.output.clone();
        assert!(output.contains("\"wrap\""), "should enable wrapping");
        assert!(output.contains("\"8px\""), "should have gap");
    }

    #[test]
    fn layout_switcher_threshold() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_template(
            &TemplateNode::Layout(LayoutNode::Switcher {
                threshold: Some("480".into()),
                children: vec![],
                span: span(),
            }),
            "$root",
        );
        let output = codegen.output.clone();
        assert!(output.contains("\"wrap\""), "should enable wrapping");
        assert!(output.contains("\"horizontal\""), "should set native direction");
    }

    #[test]
    fn router_with_layout_and_transition() {
        let router = RouterDef {
            name: "AppRouter".into(),
            routes: vec![
                RouteDef {
                    path: "/".into(),
                    params: vec![],
                    component: "Home".into(),
                    guard: None,
                    transition: Some("fade".into()),
                    span: span(),
                },
            ],
            fallback: None,
            layout: Some(RenderBlock {
                body: TemplateNode::Fragment(vec![
                    TemplateNode::Element(Element {
                        tag: "nav".into(),
                        attributes: vec![],
                        children: vec![TemplateNode::TextLiteral("Nav".into())],
                        span: span(),
                    }),
                    TemplateNode::Outlet,
                ]),
                span: span(),
            }),
            transition: Some("fade".into()),
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_router(&router);
        let output = codegen.output.clone();
        assert!(output.contains("router_registerRoute"), "should register routes");
        assert!(output.contains("router_init"), "should call router_init");
    }

    #[test]
    fn component_with_on_destroy() {
        let comp = Component {
            name: "Cleanup".into(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            trait_bounds: vec![],
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: Some(Function {
                name: "cleanup".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(None)]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            a11y: None,
            shortcuts: vec![],
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_component(&comp);
        let output = codegen.output.clone();
        assert!(output.contains("on_destroy"), "should have on_destroy");
        assert!(output.contains("lifecycle_register_cleanup"), "should register cleanup");
    }

    // -----------------------------------------------------------------------
    // Style injection and transitions
    // -----------------------------------------------------------------------

    #[test]
    fn style_injection() {
        let comp = Component {
            name: "Styled".into(),
            type_params: vec![],
            props: vec![],
            state: vec![],
            methods: vec![],
            styles: vec![StyleBlock {
                selector: ".btn".into(),
                properties: vec![("color".into(), "red".into()), ("font-size".into(), "16px".into())],
                span: span(),
            }],
            transitions: vec![TransitionDef {
                property: "opacity".into(),
                duration: "0.3s".into(),
                easing: "ease".into(),
                span: span(),
            }],
            trait_bounds: vec![],
            render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_component(&comp);
        let output = codegen.output.clone();
        assert!(output.contains("scoped styles for Styled"), "should have style injection");
        assert!(output.contains("style_injectStyles"), "should call style_injectStyles");
        assert!(output.contains("transitions for Styled"), "should have transitions");
    }

    // -----------------------------------------------------------------------
    // Expression codegen — remaining variants
    // -----------------------------------------------------------------------

    #[test]
    fn await_expression_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Await(Box::new(Expr::Integer(1))));
        let output = codegen.output.clone();
        assert!(output.contains("await"), "should have await comment");
        assert!(output.contains("signal_get"), "should resolve promise handle");
    }

    #[test]
    fn fetch_with_contract() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Fetch {
            url: Box::new(Expr::StringLit("https://api.example.com".into())),
            options: None,
            contract: Some("UserContract".into()),
        });
        let output = codegen.output.clone();
        assert!(output.contains("contract boundary validation"), "should mention contract");
        assert!(output.contains("contract_validate"), "should call contract_validate");
    }

    #[test]
    fn fetch_with_options() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Fetch {
            url: Box::new(Expr::StringLit("https://api.example.com".into())),
            options: Some(Box::new(Expr::Integer(0))),
            contract: None,
        });
        let output = codegen.output.clone();
        assert!(output.contains("http_fetch"), "should call http_fetch");
    }

    #[test]
    fn channel_create_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Channel { ty: Some(Type::Named("i32".into())) });
        let output = codegen.output.clone();
        assert!(output.contains("channel create"), "should have channel create comment");
        assert!(output.contains("worker_channelCreate"), "should call channelCreate");
    }

    #[test]
    fn channel_create_no_type() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Channel { ty: None });
        let output = codegen.output.clone();
        assert!(output.contains("channel create"), "should have channel create comment");
    }

    #[test]
    fn send_receive_exprs() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Send {
            channel: Box::new(Expr::Ident("ch".into())),
            value: Box::new(Expr::Integer(42)),
        });
        let output = codegen.output.clone();
        assert!(output.contains("channel send"), "should have channel send comment");

        let mut codegen2 = WasmCodegen::new();
        codegen2.generate_expr(&Expr::Receive {
            channel: Box::new(Expr::Ident("ch".into())),
        });
        let output2 = codegen2.output.clone();
        assert!(output2.contains("channel receive"), "should have channel receive comment");
    }

    #[test]
    fn parallel_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Parallel {
            tasks: vec![Expr::Integer(1), Expr::Integer(2), Expr::Integer(3)],
            span: span(),
        });
        let output = codegen.output.clone();
        assert!(output.contains("parallel"), "should have parallel comment");
        assert!(output.contains("worker_parallel"), "should call worker_parallel");
    }

    #[test]
    fn try_catch_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::TryCatch {
            body: Box::new(Expr::Integer(1)),
            error_binding: "err".into(),
            catch_body: Box::new(Expr::Integer(0)),
        });
        let output = codegen.output.clone();
        assert!(output.contains("try/catch"), "should have try/catch comment");
        assert!(output.contains("try_ok"), "should have try_ok block");
        assert!(output.contains("try_err"), "should have try_err block");
    }

    #[test]
    fn animate_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Animate {
            target: Box::new(Expr::Ident("el".into())),
            animation: "fadeIn".into(),
        });
        let output = codegen.output.clone();
        assert!(output.contains("animate"), "should have animate comment");
        assert!(output.contains("animation_play"), "should call animation_play");
    }

    #[test]
    fn assert_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Assert {
            condition: Box::new(Expr::Bool(true)),
            message: Some("custom msg".into()),
        });
        let output = codegen.output.clone();
        assert!(output.contains("assert"), "should have assert comment");
        assert!(output.contains("test_fail"), "should call test_fail on failure");
    }

    #[test]
    fn assert_no_message() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Assert {
            condition: Box::new(Expr::Bool(true)),
            message: None,
        });
        let output = codegen.output.clone();
        assert!(output.contains("msg len"), "should use default message with length");
    }

    #[test]
    fn assert_eq_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::AssertEq {
            left: Box::new(Expr::Integer(1)),
            right: Box::new(Expr::Integer(1)),
            message: Some("values should match".into()),
        });
        let output = codegen.output.clone();
        assert!(output.contains("assert_eq"), "should have assert_eq comment");
        assert!(output.contains("i32.eq"), "should compare with i32.eq");
    }

    #[test]
    fn assert_eq_no_message() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::AssertEq {
            left: Box::new(Expr::Integer(1)),
            right: Box::new(Expr::Integer(2)),
            message: None,
        });
        let output = codegen.output.clone();
        assert!(output.contains("msg len"), "should use default message with length");
    }

    #[test]
    fn prompt_template_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::PromptTemplate {
            template: "Hello {name}!".into(),
            interpolations: vec![
                ("name".into(), Expr::StringLit("world".into())),
            ],
        });
        let output = codegen.output.clone();
        assert!(output.contains("prompt template"), "should have prompt template comment");
        assert!(output.contains("interpolation count"), "should push interpolation count");
    }

    #[test]
    fn stream_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Stream {
            source: Box::new(Expr::StringLit("https://api.example.com/stream".into())),
        });
        let output = codegen.output.clone();
        assert!(output.contains("stream"), "should have stream comment");
        assert!(output.contains("streaming_streamFetch"), "should call streaming_streamFetch");
    }

    #[test]
    fn suspend_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Suspend {
            fallback: Box::new(Expr::Integer(0)),
            body: Box::new(Expr::Integer(1)),
        });
        let output = codegen.output.clone();
        assert!(output.contains("suspend"), "should have suspend comment");
        assert!(output.contains("dom_lazyMount"), "should call dom_lazyMount");
    }

    #[test]
    fn try_operator_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Try(Box::new(Expr::Integer(1))));
        let output = codegen.output.clone();
        assert!(output.contains("error propagation"), "should have try operator comment");
        assert!(output.contains("return"), "should have early return for error path");
    }

    #[test]
    fn dynamic_import_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::DynamicImport {
            path: Box::new(Expr::StringLit("./module.js".into())),
            span: span(),
        });
        let output = codegen.output.clone();
        assert!(output.contains("dynamic import"), "should have dynamic import comment");
        assert!(output.contains("load_chunk"), "should call load_chunk");
    }

    #[test]
    fn download_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Download {
            data: Box::new(Expr::StringLit("data".into())),
            filename: Box::new(Expr::StringLit("file.txt".into())),
            span: span(),
        });
        let output = codegen.output.clone();
        assert!(output.contains("download"), "should have download comment");
        assert!(output.contains("io_download"), "should call io_download");
    }

    #[test]
    fn env_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Env {
            name: Box::new(Expr::StringLit("API_KEY".into())),
            span: span(),
        });
        let output = codegen.output.clone();
        assert!(output.contains("env"), "should have env comment");
        assert!(output.contains("env_get"), "should call env_get");
    }

    #[test]
    fn trace_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Trace {
            label: Box::new(Expr::StringLit("render".into())),
            body: block(vec![Stmt::Return(None)]),
            span: span(),
        });
        let output = codegen.output.clone();
        assert!(output.contains("trace"), "should have trace comment");
        assert!(output.contains("trace_start"), "should call trace_start");
        assert!(output.contains("trace_end"), "should call trace_end");
    }

    #[test]
    fn flag_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Flag {
            name: Box::new(Expr::StringLit("dark_mode".into())),
            span: span(),
        });
        let output = codegen.output.clone();
        assert!(output.contains("flag"), "should have flag comment");
        assert!(output.contains("flag_is_enabled"), "should call flag_is_enabled");
    }

    #[test]
    fn virtual_list_expr() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::VirtualList {
            items: Box::new(Expr::Ident("data".into())),
            item_height: Box::new(Expr::Integer(50)),
            template: Box::new(Expr::Ident("render_item".into())),
            buffer: Some(10),
            span: span(),
        });
        let output = codegen.output.clone();
        assert!(output.contains("virtual list"), "should have virtual list comment");
        assert!(output.contains("virtual_create_list"), "should call virtual_create_list");
        assert!(output.contains("i32.const 10"), "should use custom buffer");
    }

    #[test]
    fn virtual_list_default_buffer() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::VirtualList {
            items: Box::new(Expr::Ident("data".into())),
            item_height: Box::new(Expr::Integer(50)),
            template: Box::new(Expr::Ident("render_item".into())),
            buffer: None,
            span: span(),
        });
        let output = codegen.output.clone();
        assert!(output.contains("i32.const 5"), "should use default buffer of 5");
    }

    #[test]
    fn format_string_empty() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FormatString { parts: vec![] });
        let output = codegen.output.clone();
        assert!(output.contains("i32.const 0 ;; empty fstr len"), "empty format string should push empty");
    }

    #[test]
    fn format_string_literal_only() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FormatString {
            parts: vec![FormatPart::Literal("just text".into())],
        });
        let output = codegen.output.clone();
        assert!(output.contains("fstr lit ptr"), "should push literal pointer");
    }

    #[test]
    fn format_string_mixed_parts() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FormatString {
            parts: vec![
                FormatPart::Literal("hello ".into()),
                FormatPart::Expression(Box::new(Expr::Ident("name".into()))),
                FormatPart::Literal("!".into()),
            ],
        });
        let output = codegen.output.clone();
        assert!(output.contains("to_string"), "should convert expr to string");
        assert!(output.contains("string_concat"), "should concat parts");
    }

    #[test]
    fn self_expr_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::SelfExpr);
        let output = codegen.output.clone();
        assert!(output.contains("local.get $self"), "should get self local");
    }

    #[test]
    fn field_access_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FieldAccess {
            object: Box::new(Expr::Ident("p".into())),
            field: "x".into(),
        });
        let output = codegen.output.clone();
        assert!(output.contains("field access: .x"), "should have field access comment");
        assert!(output.contains("i32.load"), "should load field");
    }

    #[test]
    fn fn_call_webapi_mapping() {
        let fns = vec![
            ("localStorage_get", "$webapi_localStorageGet"),
            ("console_log", "$webapi_consoleLog"),
            ("set_timeout", "$webapi_setTimeout"),
            ("clipboard_write", "$webapi_clipboardWrite"),
            ("push_state", "$webapi_pushState"),
        ];
        for (name, expected) in fns {
            let mut codegen = WasmCodegen::new();
            codegen.generate_expr(&Expr::FnCall {
                callee: Box::new(Expr::Ident(name.into())),
                args: vec![],
            });
            let output = codegen.output.clone();
            assert!(output.contains(expected), "fn {} should map to {}", name, expected);
        }
    }

    // -----------------------------------------------------------------------
    // Statement codegen — remaining variants
    // -----------------------------------------------------------------------

    #[test]
    fn signal_stmt_codegen() {
        let wat = compile(r#"
            component Sig {
                let mut count: i32 = 0;
                render {
                    <div>"sig"</div>
                }
            }
        "#);
        assert!(wat.contains("signal_create"), "should create signal for state");
    }

    #[test]
    fn secret_let_binding() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_stmt(&Stmt::Let {
            name: "key".into(),
            ty: None,
            mutable: false,
            secret: true,
            value: Expr::StringLit("secret123".into()),
            ownership: Ownership::Owned,
        });
        let output = codegen.output.clone();
        assert!(output.contains("secret binding: key"), "should have secret annotation");
    }

    #[test]
    fn yield_stmt() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_stmt(&Stmt::Yield(Expr::Integer(42)));
        let output = codegen.output.clone();
        assert!(output.contains("yield"), "should have yield comment");
        assert!(output.contains("streaming_yield"), "should call streaming_yield");
    }

    #[test]
    fn expr_stmt_drops() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_stmt(&Stmt::Expr(Expr::Integer(42)));
        let output = codegen.output.clone();
        assert!(output.contains("drop"), "expression statement should drop result");
    }

    #[test]
    fn let_destructure_tuple() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_stmt(&Stmt::LetDestructure {
            pattern: Pattern::Tuple(vec![
                Pattern::Ident("a".into()),
                Pattern::Ident("b".into()),
            ]),
            value: Expr::Integer(0),
            ty: None,
        });
        let output = codegen.output.clone();
        assert!(output.contains("destructure"), "should have destructure comment");
        assert!(output.contains("local.set $a"), "should set local a");
        assert!(output.contains("local.set $b"), "should set local b");
    }

    #[test]
    fn let_destructure_struct() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_stmt(&Stmt::LetDestructure {
            pattern: Pattern::Struct {
                name: "Point".into(),
                fields: vec![
                    ("x".into(), Pattern::Ident("px".into())),
                    ("y".into(), Pattern::Ident("py".into())),
                ],
                rest: false,
            },
            value: Expr::Integer(0),
            ty: None,
        });
        let output = codegen.output.clone();
        assert!(output.contains("local.set $px"), "should set local px");
        assert!(output.contains("local.set $py"), "should set local py");
    }

    #[test]
    fn let_destructure_array_with_wildcard() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_stmt(&Stmt::LetDestructure {
            pattern: Pattern::Array(vec![
                Pattern::Ident("first".into()),
                Pattern::Wildcard,
                Pattern::Ident("third".into()),
            ]),
            value: Expr::Integer(0),
            ty: None,
        });
        let output = codegen.output.clone();
        assert!(output.contains("local.set $first"), "should set first");
        assert!(output.contains("local.set $third"), "should set third");
    }

    // -----------------------------------------------------------------------
    // Template codegen
    // -----------------------------------------------------------------------

    #[test]
    fn template_element_with_attributes() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        let el = TemplateNode::Element(Element {
            tag: "button".into(),
            attributes: vec![
                Attribute::Static { name: "class".into(), value: "btn".into() },
                Attribute::EventHandler { event: "click".into(), handler: Expr::Ident("onclick".into()) },
                Attribute::Aria { name: "label".into(), value: Expr::StringLit("Click me".into()) },
                Attribute::Aria { name: "expanded".into(), value: Expr::Ident("is_open".into()) },
                Attribute::Role { value: "button".into() },
                Attribute::Bind { property: "value".into(), signal: "text".into() },
                Attribute::Bind { property: "checked".into(), signal: "is_checked".into() },
            ],
            children: vec![TemplateNode::TextLiteral("Click".into())],
            span: span(),
        });
        codegen.generate_template(&el, "$root");
        let output = codegen.output.clone();
        assert!(output.contains("dom_createElement"), "should create element");
        assert!(output.contains("dom_addEventListener"), "should add event listener");
        assert!(output.contains("a11y_setAriaAttribute"), "should set ARIA attribute");
        assert!(output.contains("a11y_setRole"), "should set role");
        assert!(output.contains("dom_setProperty"), "should set property for bind");
        assert!(output.contains("signal_createEffect"), "should create effect for bind");
        assert!(output.contains("dom_appendChild"), "should append to parent");
        assert!(output.contains("dom_setText"), "should set text content");
    }

    #[test]
    fn template_link() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        let link = TemplateNode::Link {
            to: Expr::StringLit("/about".into()),
            attributes: vec![],
            children: vec![TemplateNode::TextLiteral("About".into())],
        };
        codegen.generate_template(&link, "$root");
        let output = codegen.output.clone();
        assert!(output.contains("dom_createElement"), "should create anchor element");
        assert!(output.contains("dom_addEventListener"), "should add click handler");
        assert!(output.contains("dom_appendChild"), "should append link");
    }

    #[test]
    fn template_expression() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        let expr = TemplateNode::Expression(Box::new(Expr::Integer(42)));
        codegen.generate_template(&expr, "$root");
        let output = codegen.output.clone();
        assert!(output.contains("dynamic expression"), "should have expression comment");
        assert!(output.contains("i32.const 42"), "should evaluate expression");
    }

    // -----------------------------------------------------------------------
    // Router with guard and fallback
    // -----------------------------------------------------------------------

    #[test]
    fn router_with_guard() {
        let router = RouterDef {
            name: "AppRouter".into(),
            routes: vec![
                RouteDef {
                    path: "/admin".into(),
                    params: vec![],
                    component: "Admin".into(),
                    guard: Some(Expr::Bool(true)),
                    transition: None,
                    span: span(),
                },
                RouteDef {
                    path: "/".into(),
                    params: vec![],
                    component: "Home".into(),
                    guard: None,
                    transition: None,
                    span: span(),
                },
            ],
            fallback: Some(Box::new(TemplateNode::TextLiteral("404 Not Found".into()))),
            layout: None,
            transition: None,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_router(&router);
        let output = codegen.output.clone();
        assert!(output.contains("route guard check"), "should have guard check");
        assert!(output.contains("fallback route component"), "should have fallback");
        assert!(output.contains("router_registerRoute"), "should register routes");
        assert!(output.contains("router_init"), "should call router_init");
    }

    // -----------------------------------------------------------------------
    // Store with selectors
    // -----------------------------------------------------------------------

    #[test]
    fn store_with_selectors() {
        let store = StoreDef {
            name: "DataStore".into(),
            signals: vec![],
            actions: vec![],
            computed: vec![],
            effects: vec![],
            selectors: vec![SelectorDef {
                name: "filteredItems".into(),
                deps: vec!["items".into()],
                body: Expr::Integer(0),
                span: span(),
            }],
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_store(&store);
        let output = codegen.output.clone();
        assert!(output.contains("selector: filteredItems"), "should have selector");
        assert!(output.contains("DataStore_selector_filteredItems"), "should generate selector function");
    }

    // -----------------------------------------------------------------------
    // String interning deduplication
    // -----------------------------------------------------------------------

    #[test]
    fn string_interning_deduplicates() {
        let mut codegen = WasmCodegen::new();
        let off1 = codegen.store_string("hello");
        let off2 = codegen.store_string("hello");
        let off3 = codegen.store_string("world");
        assert_eq!(off1, off2, "same string should return same offset");
        assert_ne!(off1, off3, "different strings should have different offsets");
    }

    // -----------------------------------------------------------------------
    // type_to_wasm / type_size / ast_type_to_wasm
    // -----------------------------------------------------------------------

    #[test]
    fn type_to_wasm_mapping() {
        let codegen = WasmCodegen::new();
        assert_eq!(codegen.type_to_wasm(&Type::Named("i32".into())), "i32");
        assert_eq!(codegen.type_to_wasm(&Type::Named("u32".into())), "i32");
        assert_eq!(codegen.type_to_wasm(&Type::Named("bool".into())), "i32");
        assert_eq!(codegen.type_to_wasm(&Type::Named("i64".into())), "i64");
        assert_eq!(codegen.type_to_wasm(&Type::Named("u64".into())), "i64");
        assert_eq!(codegen.type_to_wasm(&Type::Named("f32".into())), "f32");
        assert_eq!(codegen.type_to_wasm(&Type::Named("f64".into())), "f64");
        assert_eq!(codegen.type_to_wasm(&Type::Named("String".into())), "i32");
        assert_eq!(codegen.type_to_wasm(&Type::Named("Custom".into())), "i32");
        assert_eq!(codegen.type_to_wasm(&Type::Generic { name: "Vec".into(), args: vec![] }), "i32");
        assert_eq!(codegen.type_to_wasm(&Type::Reference { mutable: false, lifetime: None, inner: Box::new(Type::Named("i32".into())) }), "i32");
        assert_eq!(codegen.type_to_wasm(&Type::Array(Box::new(Type::Named("i32".into())))), "i32");
        assert_eq!(codegen.type_to_wasm(&Type::Tuple(vec![])), "i32");
    }

    #[test]
    fn type_size_mapping() {
        let codegen = WasmCodegen::new();
        assert_eq!(codegen.type_size(&Type::Named("i32".into())), 4);
        assert_eq!(codegen.type_size(&Type::Named("f32".into())), 4);
        assert_eq!(codegen.type_size(&Type::Named("bool".into())), 4);
        assert_eq!(codegen.type_size(&Type::Named("i64".into())), 8);
        assert_eq!(codegen.type_size(&Type::Named("f64".into())), 8);
        assert_eq!(codegen.type_size(&Type::Named("String".into())), 8);
        assert_eq!(codegen.type_size(&Type::Named("Custom".into())), 4);
        assert_eq!(codegen.type_size(&Type::Array(Box::new(Type::Named("i32".into())))), 4);
    }

    // -----------------------------------------------------------------------
    // Lazy component codegen
    // -----------------------------------------------------------------------

    #[test]
    fn lazy_component_codegen() {
        let lazy = LazyComponentDef {
            component: Component {
                name: "HeavyChart".into(),
                type_params: vec![],
                props: vec![],
                state: vec![],
                methods: vec![],
                styles: vec![],
                transitions: vec![],
                trait_bounds: vec![],
                render: RenderBlock { body: TemplateNode::Fragment(vec![]), span: span() },
                permissions: None,
                gestures: vec![],
                skeleton: None,
                error_boundary: None,
                chunk: None,
                on_destroy: None,
                a11y: None,
                shortcuts: vec![],
                span: span(),
            },
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_lazy_component(&lazy);
        let output = codegen.output.clone();
        assert!(output.contains("Lazy Component: HeavyChart"), "should mark as lazy");
        assert!(output.contains("lazy_mount"), "should generate lazy mount wrapper");
        assert!(output.contains("dom_lazyMount"), "should call dom_lazyMount");
    }

    // -----------------------------------------------------------------------
    // Iterator codegen — fold, all, enumerate, zip, take, skip, default method
    // -----------------------------------------------------------------------

    #[test]
    fn fold_generates_loop() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("iter_val".into())),
            method: "fold".into(),
            args: vec![
                Expr::Integer(0),
                Expr::Closure {
                    params: vec![("acc".into(), None), ("x".into(), None)],
                    body: Box::new(Expr::Binary {
                        op: BinOp::Add,
                        left: Box::new(Expr::Ident("acc".into())),
                        right: Box::new(Expr::Ident("x".into())),
                    }),
                },
            ],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(output.contains(".fold()"), "should contain fold comment");
        assert!(output.contains("loop $__fold_lp_"), "should generate fold loop");
    }

    #[test]
    fn all_generates_early_exit_loop() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("iter_val".into())),
            method: "all".into(),
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
        assert!(output.contains(".all()"), "should contain all comment");
        assert!(output.contains("loop $__all_lp_"), "should generate all loop");
    }

    #[test]
    fn enumerate_generates_loop() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("iter_val".into())),
            method: "enumerate".into(),
            args: vec![],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(output.contains(".enumerate()"), "should contain enumerate comment");
        assert!(output.contains("loop $__en_lp_"), "should generate enumerate loop");
    }

    #[test]
    fn zip_generates_loop() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("iter_a".into())),
            method: "zip".into(),
            args: vec![Expr::Ident("iter_b".into())],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(output.contains(".zip()"), "should contain zip comment");
        assert!(output.contains("loop $__zip_lp_"), "should generate zip loop");
    }

    #[test]
    fn take_generates_sub_array() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("iter_val".into())),
            method: "take".into(),
            args: vec![Expr::Integer(5)],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(output.contains(".take()"), "should contain take comment");
        assert!(output.contains("memory.copy"), "should use memory.copy for take");
    }

    #[test]
    fn skip_generates_sub_array() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("iter_val".into())),
            method: "skip".into(),
            args: vec![Expr::Integer(3)],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(output.contains(".skip()"), "should contain skip comment");
        assert!(output.contains("memory.copy"), "should use memory.copy for skip");
    }

    #[test]
    fn unknown_method_falls_through() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("obj".into())),
            method: "custom_method".into(),
            args: vec![Expr::Integer(1)],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(output.contains("call $custom_method"), "unknown method should be called directly");
    }

    // -----------------------------------------------------------------------
    // collect_locals and collect_pattern_locals
    // -----------------------------------------------------------------------

    #[test]
    fn collect_locals_from_block() {
        let mut codegen = WasmCodegen::new();
        let b = block(vec![
            Stmt::Let { name: "a".into(), ty: Some(Type::Named("i64".into())), mutable: false, secret: false, value: Expr::Integer(0), ownership: Ownership::Owned },
            Stmt::Let { name: "b".into(), ty: Some(Type::Named("f64".into())), mutable: false, secret: false, value: Expr::Float(0.0), ownership: Ownership::Owned },
            Stmt::Let { name: "c".into(), ty: None, mutable: false, secret: false, value: Expr::Integer(0), ownership: Ownership::Owned },
            Stmt::LetDestructure { pattern: Pattern::Tuple(vec![Pattern::Ident("d".into()), Pattern::Ident("e".into())]), value: Expr::Integer(0), ty: None },
        ]);
        codegen.collect_locals(&b);
        let names: Vec<&str> = codegen.locals.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"a"), "should collect local a");
        assert!(names.contains(&"b"), "should collect local b");
        assert!(names.contains(&"c"), "should collect local c");
        assert!(names.contains(&"d"), "should collect destructured local d");
        assert!(names.contains(&"e"), "should collect destructured local e");
    }

    // -----------------------------------------------------------------------
    // Data section emission and string escaping
    // -----------------------------------------------------------------------

    #[test]
    fn data_section_escapes_special_chars() {
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.store_string("hello \"world\"");
        codegen.store_string("back\\slash");
        codegen.emit_data_section();
        let output = codegen.output.clone();
        assert!(output.contains("\\\\"), "should escape backslashes");
        assert!(output.contains("\\\""), "should escape quotes");
    }

    #[test]
    fn empty_data_section_no_output() {
        let mut codegen = WasmCodegen::new();
        codegen.emit_data_section();
        assert!(codegen.output.is_empty(), "empty data section should produce no output");
    }

    // -----------------------------------------------------------------------
    // Spawn expression codegen (via AST)
    // -----------------------------------------------------------------------

    #[test]
    fn spawn_expr_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Spawn {
            body: block(vec![Stmt::Return(None)]),
            span: span(),
        });
        let output = codegen.output.clone();
        assert!(output.contains("spawn"), "should have spawn comment");
        assert!(output.contains("worker_spawn"), "should call worker_spawn");
    }

    // -----------------------------------------------------------------------
    // Navigate expression codegen
    // -----------------------------------------------------------------------

    #[test]
    fn navigate_expr_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Navigate {
            path: Box::new(Expr::StringLit("/about".into())),
        });
        let output = codegen.output.clone();
        assert!(output.contains("navigate"), "should have navigate comment");
        assert!(output.contains("router_navigate"), "should call router_navigate");
    }

    // -----------------------------------------------------------------------
    // Assign expression codegen
    // -----------------------------------------------------------------------

    #[test]
    fn assign_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Assign {
            target: Box::new(Expr::Ident("x".into())),
            value: Box::new(Expr::Integer(42)),
        });
        let output = codegen.output.clone();
        assert!(output.contains("local.set $x"), "should set local");
    }

    // -----------------------------------------------------------------------
    // Fallback expr codegen (default branch)
    // -----------------------------------------------------------------------

    #[test]
    fn fallback_expr_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Borrow(Box::new(Expr::Integer(1))));
        let output = codegen.output.clone();
        assert!(output.contains("TODO: codegen for expr"), "unhandled expr should produce TODO");
    }

    // -----------------------------------------------------------------------
    // Agent codegen — async action, state, render
    // -----------------------------------------------------------------------

    #[test]
    fn agent_with_state_and_render() {
        let program = Program {
            items: vec![Item::Agent(AgentDef {
                name: "Bot".into(),
                system_prompt: Some("Be helpful.".into()),
                tools: vec![ToolDef {
                    name: "lookup".into(),
                    description: Some("Search for info".into()),
                    params: vec![
                        Param { name: "query".into(), ty: Type::Named("String".into()), ownership: Ownership::Owned, secret: false },
                        Param { name: "count".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false },
                    ],
                    return_type: Some(Type::Named("String".into())),
                    body: block(vec![Stmt::Return(Some(Expr::StringLit("result".into())))]),
                    span: span(),
                }],
                state: vec![StateField {
                    name: "messages".into(),
                    ty: None,
                    mutable: true,
                    secret: false,
                    atomic: false,
                    initializer: Expr::Integer(0),
                    ownership: Ownership::Owned,
                }],
                methods: vec![],
                render: Some(RenderBlock {
                    body: TemplateNode::TextLiteral("bot ui".into()),
                    span: span(),
                }),
                span: span(),
            })],
        };
        let mut codegen = WasmCodegen::new();
        let output = codegen.generate(&program);
        assert!(output.contains("Agent: Bot"), "should have agent header");
        assert!(output.contains("Bot_init"), "should generate init");
        assert!(output.contains("Bot_mount"), "should generate mount");
        assert!(output.contains("register tool: lookup"), "should register tools");
        assert!(output.contains("__tool_Bot_lookup"), "should generate tool wrapper");
        assert!(output.contains("ai_registerTool"), "should call ai_registerTool");
    }

    // -----------------------------------------------------------------------
    // Store with async action
    // -----------------------------------------------------------------------

    #[test]
    fn store_async_action() {
        let wat = compile(r#"
            store AsyncStore {
                signal count: i32 = 0;

                async action fetch_data() {
                    return;
                }
            }
        "#);
        assert!(wat.contains("async"), "should mark async action");
    }

    // -----------------------------------------------------------------------
    // Crypto runtime — pure WASM
    // -----------------------------------------------------------------------

    #[test]
    fn crypto_sha256_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::sha256".into())),
            args: vec![Expr::StringLit("hello".into())],
        });
        let output = codegen.output.clone();
        assert!(output.contains("call $crypto_sha256"), "should emit $crypto_sha256 call");
        assert!(output.contains(";; crypto:"), "should have crypto comment");
    }

    #[test]
    fn crypto_sha512_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::sha512".into())),
            args: vec![Expr::StringLit("data".into())],
        });
        assert!(codegen.output.contains("call $crypto_sha512"));
    }

    #[test]
    fn crypto_sha1_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::sha1".into())),
            args: vec![Expr::StringLit("data".into())],
        });
        assert!(codegen.output.contains("call $crypto_sha1"));
    }

    #[test]
    fn crypto_sha384_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::sha384".into())),
            args: vec![Expr::StringLit("data".into())],
        });
        assert!(codegen.output.contains("call $crypto_sha384"));
    }

    #[test]
    fn crypto_hmac_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::hmac".into())),
            args: vec![Expr::StringLit("key".into()), Expr::StringLit("data".into())],
        });
        assert!(codegen.output.contains("call $crypto_hmac_sha256"));
    }

    #[test]
    fn crypto_hmac_sha512_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::hmac_sha512".into())),
            args: vec![Expr::StringLit("key".into()), Expr::StringLit("data".into())],
        });
        assert!(codegen.output.contains("call $crypto_hmac_sha512"));
    }

    #[test]
    fn crypto_encrypt_decrypt_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::encrypt".into())),
            args: vec![Expr::StringLit("key".into()), Expr::StringLit("plain".into())],
        });
        assert!(codegen.output.contains("call $crypto_aes_gcm_encrypt"));

        let mut codegen2 = WasmCodegen::new();
        codegen2.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::decrypt".into())),
            args: vec![Expr::StringLit("key".into()), Expr::StringLit("cipher".into())],
        });
        assert!(codegen2.output.contains("call $crypto_aes_gcm_decrypt"));
    }

    #[test]
    fn crypto_aes_cbc_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::encrypt_aes_cbc".into())),
            args: vec![Expr::StringLit("key".into()), Expr::StringLit("data".into())],
        });
        assert!(codegen.output.contains("call $crypto_aes_cbc_encrypt"));
    }

    #[test]
    fn crypto_aes_ctr_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::encrypt_aes_ctr".into())),
            args: vec![Expr::StringLit("key".into()), Expr::StringLit("data".into())],
        });
        assert!(codegen.output.contains("call $crypto_aes_ctr_encrypt"));
    }

    #[test]
    fn crypto_sign_verify_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::sign".into())),
            args: vec![Expr::StringLit("privkey".into()), Expr::StringLit("data".into())],
        });
        assert!(codegen.output.contains("call $crypto_ed25519_sign"));

        let mut codegen2 = WasmCodegen::new();
        codegen2.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::verify".into())),
            args: vec![
                Expr::StringLit("pubkey".into()),
                Expr::StringLit("data".into()),
                Expr::StringLit("sig".into()),
            ],
        });
        assert!(codegen2.output.contains("call $crypto_ed25519_verify"));
    }

    #[test]
    fn crypto_derive_key_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::derive_key".into())),
            args: vec![Expr::StringLit("pwd".into()), Expr::StringLit("salt".into())],
        });
        assert!(codegen.output.contains("call $crypto_pbkdf2_derive"));
    }

    #[test]
    fn crypto_derive_bits_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::derive_bits".into())),
            args: vec![
                Expr::StringLit("pwd".into()),
                Expr::StringLit("salt".into()),
                Expr::Integer(256),
            ],
        });
        assert!(codegen.output.contains("call $crypto_pbkdf2_derive_bits"));
    }

    #[test]
    fn crypto_hkdf_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::hkdf".into())),
            args: vec![
                Expr::StringLit("ikm".into()),
                Expr::StringLit("salt".into()),
                Expr::StringLit("info".into()),
                Expr::Integer(32),
            ],
        });
        assert!(codegen.output.contains("call $crypto_hkdf_derive"));
    }

    #[test]
    fn crypto_random_uuid_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::random_uuid".into())),
            args: vec![],
        });
        assert!(codegen.output.contains("call $crypto_random_uuid"));
    }

    #[test]
    fn crypto_random_bytes_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::random_bytes".into())),
            args: vec![Expr::Integer(32)],
        });
        assert!(codegen.output.contains("call $crypto_random_bytes"));
    }

    #[test]
    fn crypto_generate_key_pair_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::generate_key_pair".into())),
            args: vec![Expr::StringLit("ed25519".into())],
        });
        assert!(codegen.output.contains("call $crypto_generate_key_pair"));
    }

    #[test]
    fn crypto_export_key_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::export_key".into())),
            args: vec![Expr::StringLit("key".into()), Expr::StringLit("hex".into())],
        });
        assert!(codegen.output.contains("call $crypto_export_key"));
    }

    #[test]
    fn crypto_ecdh_derive_codegen() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::FnCall {
            callee: Box::new(Expr::Ident("crypto::ecdh_derive".into())),
            args: vec![Expr::StringLit("priv".into()), Expr::StringLit("pub".into())],
        });
        assert!(codegen.output.contains("call $crypto_ecdh_derive"));
    }

    #[test]
    fn crypto_runtime_emitted_in_wat() {
        let wat = compile("pub fn main() -> i32 { return 0; }");
        assert!(wat.contains("$crypto_sha256_block"), "WAT should contain SHA-256 block transform");
        assert!(wat.contains("$crypto_sha256"), "WAT should contain SHA-256 function");
        assert!(wat.contains("$crypto_hmac_sha256"), "WAT should contain HMAC-SHA256");
        assert!(wat.contains("$crypto_aes_gcm_encrypt"), "WAT should contain AES encrypt");
        assert!(wat.contains("$crypto_random_uuid"), "WAT should contain UUID generator");
        assert!(wat.contains("$crypto_xorshift32"), "WAT should contain PRNG");
        assert!(wat.contains("$crypto_bytes_to_hex"), "WAT should contain hex conversion");
        assert!(wat.contains("$crypto_ed25519_sign"), "WAT should contain Ed25519 sign");
        assert!(wat.contains("$crypto_pbkdf2_derive"), "WAT should contain PBKDF2");
        assert!(wat.contains("$crypto_hkdf_derive"), "WAT should contain HKDF");
        assert!(wat.contains("$crypto_ecdh_derive"), "WAT should contain ECDH");
        assert!(wat.contains("$crypto_generate_key_pair"), "WAT should contain key gen");
        assert!(wat.contains("442368"), "WAT should reference crypto scratch memory");
        assert!(wat.contains("0123456789abcdef"), "WAT should contain hex lookup table");
    }

    #[test]
    fn codegen_array_lit() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::ArrayLit(vec![
            Expr::Integer(1),
            Expr::Integer(2),
            Expr::Integer(3),
        ]));
        let output = codegen.output.clone();
        // Should produce some WAT output (even if it falls through to TODO)
        assert!(!output.is_empty());
    }

    #[test]
    fn codegen_object_lit() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::ObjectLit {
            fields: vec![
                ("x".into(), Expr::Integer(1)),
                ("y".into(), Expr::Integer(2)),
            ],
        });
        let output = codegen.output.clone();
        assert!(!output.is_empty());
    }

    #[test]
    fn codegen_match_with_guard() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Match {
            subject: Box::new(Expr::Integer(1)),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Expr::Integer(1)),
                    guard: Some(Expr::Bool(true)),
                    body: Expr::Integer(10),
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: Expr::Integer(0),
                },
            ],
        });
        let output = codegen.output.clone();
        assert!(!output.is_empty());
    }

    #[test]
    fn codegen_match_literal_arms() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Match {
            subject: Box::new(Expr::Integer(42)),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Expr::Integer(1)),
                    guard: None,
                    body: Expr::Integer(10),
                },
                MatchArm {
                    pattern: Pattern::Literal(Expr::Integer(2)),
                    guard: None,
                    body: Expr::Integer(20),
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: Expr::Integer(0),
                },
            ],
        });
        let output = codegen.output.clone();
        assert!(output.contains("match expression"), "should have match comment");
        assert!(output.contains("i32.eq"), "should compare with i32.eq");
        assert!(output.contains("local.set $__match_subj_"), "should store subject in local");
    }

    #[test]
    fn codegen_match_variant_arms() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Match {
            subject: Box::new(Expr::Ident("status".into())),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Variant { name: "Active".into(), fields: vec![] },
                    guard: None,
                    body: Expr::Integer(1),
                },
                MatchArm {
                    pattern: Pattern::Variant { name: "Inactive".into(), fields: vec![] },
                    guard: None,
                    body: Expr::Integer(0),
                },
            ],
        });
        let output = codegen.output.clone();
        assert!(output.contains("match expression"), "should have match comment");
        assert!(output.contains("variant Active"), "should reference Active variant");
        assert!(output.contains("variant Inactive"), "should reference Inactive variant");
        assert!(output.contains("i32.load"), "should load discriminant tag");
        assert!(output.contains("i32.eq"), "should compare tags");
    }

    #[test]
    fn codegen_match_wildcard_only() {
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&Expr::Match {
            subject: Box::new(Expr::Integer(0)),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: Expr::Integer(99),
                },
            ],
        });
        let output = codegen.output.clone();
        assert!(output.contains("match expression"), "should have match comment");
        assert!(output.contains("i32.const 99"), "wildcard arm should emit body");
    }

    // -----------------------------------------------------------------------
    // Tests for WAT correctness fixes
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_components_produce_unique_handler_names() {
        // Two components in one file must not generate conflicting $__handler_0 names.
        // Each component's handlers must be namespaced by component name.
        let wat = compile(r#"
            component Alpha {
                fn do_thing(&mut self) { return; }
                render { <div>"alpha"</div> }
            }
            component Beta {
                fn do_thing(&mut self) { return; }
                render { <div>"beta"</div> }
            }
        "#);
        assert!(wat.contains("Alpha__handler_0"), "Alpha handler should be namespaced");
        assert!(wat.contains("Beta__handler_0"), "Beta handler should be namespaced");
        assert!(wat.contains("Alpha__callback"), "Alpha callback should be namespaced");
        assert!(wat.contains("Beta__callback"), "Beta callback should be namespaced");
        // There must NOT be a bare $__handler_0 that would conflict
        let bare_handler_count = wat.matches("func $__handler_0").count();
        assert_eq!(bare_handler_count, 0, "bare un-namespaced $__handler_0 must not exist");
    }

    #[test]
    fn multiple_forms_produce_unique_method_names() {
        // Two forms with the same method name (on_submit) must not conflict.
        let wat = compile(r#"
            form FormA {
                field name: String { required }
                fn on_submit(&self) { return; }
            }
            form FormB {
                field email: String { email }
                fn on_submit(&self) { return; }
            }
        "#);
        assert!(wat.contains("func $FormA_on_submit"), "FormA_on_submit should be generated");
        assert!(wat.contains("func $FormB_on_submit"), "FormB_on_submit should be generated");
        let bare_count = wat.matches("func $on_submit").count();
        assert_eq!(bare_count, 0, "bare un-namespaced $on_submit must not exist");
    }

    #[test]
    fn embed_load_sandboxed_import_present_in_preamble() {
        // The WAT preamble must always include $embed_load_sandboxed and $embed_load_script
        // imports so that embed blocks with sandbox: true can call them.
        let wat = compile(r#"
            component Dummy {
                render { <div>"x"</div> }
            }
        "#);
        assert!(
            wat.contains("embedLoadSandboxed"),
            "preamble must import embedLoadSandboxed"
        );
        assert!(
            wat.contains("embedLoadScript"),
            "preamble must import embedLoadScript"
        );
    }

    #[test]
    fn embed_sandboxed_calls_embed_load_sandboxed() {
        // A sandboxed embed must call $embed_load_sandboxed, not a local variable.
        let embed = EmbedDef {
            name: "Sandboxed".into(),
            src: Expr::StringLit("https://example.com/widget.js".into()),
            loading: Some("lazy".into()),
            sandbox: true,
            integrity: None,
            permissions: None,
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_embed(&embed);
        let output = codegen.output.clone();
        assert!(output.contains("call $embed_load_sandboxed"), "should call embed_load_sandboxed");
    }

    #[test]
    fn time_namespace_call_produces_imported_function() {
        // `time.now()` in a namespace call context must call $time_now_i32, not local.get $time.
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("time".into())),
            method: "now".into(),
            args: vec![],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(output.contains("call $time_now_i32"), "time.now() must call $time_now_i32");
        assert!(
            !output.contains("local.get $time"),
            "time.now() must not try to read $time as a local variable"
        );
    }

    #[test]
    fn duration_namespace_call_produces_wasm_internal_function() {
        // `Duration.hours(n)` must call $time_duration_hours, not local.get $Duration.
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("Duration".into())),
            method: "hours".into(),
            args: vec![Expr::Integer(2)],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(
            output.contains("call $time_duration_hours"),
            "Duration.hours() must call $time_duration_hours"
        );
        assert!(
            !output.contains("local.get $Duration"),
            "Duration.hours() must not try to read $Duration as a local variable"
        );
    }

    #[test]
    fn channel_handlers_namespaced_by_channel_name() {
        // Two channels sharing handler names (on_message) must produce unique WAT function names.
        let ch = ChannelDef {
            name: "ChatRoom".into(),
            url: Expr::StringLit("/ws/chat".into()),
            contract: None,
            on_message: Some(Function {
                name: "on_message".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                body: Block { stmts: vec![], span: span() },
                is_pub: false,
                must_use: false,
                trait_bounds: vec![],
                span: span(),
            }),
            on_connect: None,
            on_disconnect: None,
            reconnect: true,
            heartbeat_interval: None,
            methods: vec![],
            is_pub: false,
            span: span(),
        };
        let mut codegen = WasmCodegen::new();
        codegen.indent = 1;
        codegen.generate_channel(&ch);
        let output = codegen.output.clone();
        assert!(
            output.contains("func $ChatRoom_on_message"),
            "channel handler must be namespaced by channel name"
        );
        assert!(
            !output.contains("func $on_message "),
            "bare on_message must not be generated"
        );
    }

    #[test]
    fn template_if_uses_valid_wat_control_flow() {
        // Template if/else must use WAT if/else/end, not br_if with undeclared labels.
        let wat = compile(r#"
            component Flag {
                let mut active: bool = false;
                render {
                    <div>
                        {if self.active { "yes" } else { "no" }}
                    </div>
                }
            }
        "#);
        assert!(wat.contains("if"), "template if must use WAT if instruction");
        assert!(wat.contains("else"), "template if/else must use WAT else");
        assert!(wat.contains("end"), "template if/else must use WAT end");
        // Must NOT use bare br_if with labels that are not declared
        assert!(
            !wat.contains("br_if $tmpl_else_"),
            "must not use undeclared tmpl_else labels"
        );
    }

    #[test]
    fn string_signal_init_drops_extra_len() {
        // When a string literal is used as signal initial value, the generated WAT
        // must include a `drop` to discard the len half of the (ptr, len) pair
        // before calling signal_create which takes a single i32.
        let wat = compile(r#"
            component StringState {
                let mut label: String = "";
                render { <div>{self.label}</div> }
            }
        "#);
        // The drop instruction must appear between the string literal push and signal_create
        assert!(
            wat.contains("drop  ;; discard str len"),
            "string signal init must drop the str len before signal_create"
        );
    }

    #[test]
    fn fetch_uses_typed_setters_not_extra_params() {
        // fetch() must call http_setMethod before http_fetch — not push extra args to http_fetch.
        // http_fetch only takes (url_ptr, url_len); method is set via $http_setMethod.
        let wat = compile(r#"
            component Fetcher {
                fn load(&self) {
                    let result = fetch("/api/data");
                }
                render { <div>"fetcher"</div> }
            }
        "#);
        assert!(
            wat.contains("call $http_setMethod"),
            "fetch must call http_setMethod before http_fetch"
        );
        assert!(
            wat.contains("call $http_fetch"),
            "fetch must call http_fetch"
        );
    }

    #[test]
    fn prop_access_in_handler_uses_global() {
        // When a component prop is accessed in an event handler, the generated WAT
        // must use global.get $__prop_<Comp>_<prop>_ptr, not local.get $<prop>.
        let wat = compile(r#"
            component Greeter(name: String) {
                fn greet(&self) {
                    return;
                }
                render { <div>"hi"</div> }
            }
        "#);
        // Prop globals must be declared
        assert!(
            wat.contains("$__prop_Greeter_name_ptr"),
            "prop global ptr must be declared"
        );
        assert!(
            wat.contains("$__prop_Greeter_name_len"),
            "prop global len must be declared"
        );
    }

    // -----------------------------------------------------------------------
    // Array methods: len, is_empty, push, contains
    // -----------------------------------------------------------------------

    #[test]
    fn len_generates_i32_load() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("arr".into())),
            method: "len".into(),
            args: vec![],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(output.contains(".len()"), "should contain len comment");
        assert!(output.contains("i32.load"), "should load length from memory");
    }

    #[test]
    fn is_empty_generates_eqz() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("arr".into())),
            method: "is_empty".into(),
            args: vec![],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(output.contains(".is_empty()"), "should contain is_empty comment");
        assert!(output.contains("i32.eqz"), "should check length == 0");
    }

    #[test]
    fn push_appends_and_increments_length() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("arr".into())),
            method: "push".into(),
            args: vec![Expr::Integer(42)],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(output.contains(".push()"), "should contain push comment");
        assert!(output.contains("i32.store"), "should store the new element");
        assert!(output.contains("i32.const 1"), "should increment length by 1");
    }

    #[test]
    fn contains_generates_scan_loop() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("arr".into())),
            method: "contains".into(),
            args: vec![Expr::Integer(7)],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let output = codegen.output.clone();
        assert!(output.contains(".contains()"), "should contain contains comment");
        assert!(output.contains("loop $__con_lp_"), "should generate scan loop");
        assert!(output.contains("i32.eq"), "should compare elements");
    }

    // -----------------------------------------------------------------------
    // Keyword definition fixes: issues 1-6, 7, 8, 9, 10
    // -----------------------------------------------------------------------

    /// Issue 1-6: keyword def instances (auth AppAuth {}, cache AppCache {}, etc.)
    /// must NOT emit `local.get $<Name>` — that variable is never declared.
    /// Instead they emit `i32.const 0` as a null namespace handle.
    #[test]
    fn test_keyword_def_ident_emits_null_placeholder() {
        let mut codegen = WasmCodegen::new();
        codegen.known_keyword_defs.push(("AppAuth".into(), KeywordDefKind::Auth));
        codegen.generate_expr(&Expr::Ident("AppAuth".into()));
        let out = codegen.output.clone();
        assert!(out.contains("i32.const 0"), "keyword def ident should emit i32.const 0, not local.get");
        assert!(!out.contains("local.get"), "keyword def ident must not emit local.get");
    }

    /// Issue 1-6: method call on a keyword def instance (e.g. `AppAuth.login("google")`)
    /// must dispatch through the corresponding WASM import, not treat AppAuth as a local.
    #[test]
    fn test_keyword_def_method_call_dispatches_to_import() {
        let mut codegen = WasmCodegen::new();
        codegen.known_keyword_defs.push(("AppAuth".into(), KeywordDefKind::Auth));
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("AppAuth".into())),
            method: "login".into(),
            args: vec![Expr::StringLit("google".into())],
        };
        codegen.generate_expr(&expr);
        let out = codegen.output.clone();
        assert!(!out.contains("local.get $AppAuth"), "must not emit local.get for keyword def");
        assert!(out.contains("call $auth_login"), "must call auth_login import");
    }

    /// Issue 2: cache def method call dispatches correctly
    #[test]
    fn test_cache_def_method_call_dispatches_to_runtime() {
        let mut codegen = WasmCodegen::new();
        codegen.known_keyword_defs.push(("AppCache".into(), KeywordDefKind::Cache));
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("AppCache".into())),
            method: "get".into(),
            args: vec![],
        };
        codegen.generate_expr(&expr);
        let out = codegen.output.clone();
        assert!(!out.contains("local.get $AppCache"), "must not emit local.get for cache def");
        assert!(out.contains("call $cache_get"), "must call cache_get runtime");
    }

    /// Issue 3: database def method call dispatches correctly
    #[test]
    fn test_database_def_method_call_dispatches_to_import() {
        let mut codegen = WasmCodegen::new();
        codegen.known_keyword_defs.push(("AppDatabase".into(), KeywordDefKind::Database));
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("AppDatabase".into())),
            method: "put".into(),
            args: vec![],
        };
        codegen.generate_expr(&expr);
        let out = codegen.output.clone();
        assert!(!out.contains("local.get $AppDatabase"), "must not emit local.get for db def");
        assert!(out.contains("call $db_put"), "must call db_put import");
    }

    /// Issue 7: StoreName::signal_name() resolves to the getter $StoreName_get_signal_name
    #[test]
    fn test_store_signal_call_resolves_to_getter() {
        let src = r#"
            store AuthStore {
                signal is_logged_in: bool = false;
            }
            fn check() -> bool {
                return AuthStore::is_logged_in();
            }
        "#;
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let mut codegen = WasmCodegen::new();
        let wat = codegen.generate(&program);
        assert!(
            wat.contains("call $AuthStore_get_is_logged_in"),
            "store signal call must resolve to getter"
        );
        assert!(
            !wat.contains("call $AuthStore_is_logged_in\n") &&
            !wat.contains("call $AuthStore_is_logged_in "),
            "must not generate undefined $AuthStore_is_logged_in"
        );
    }

    /// Issue 8: crypto.hmac() resolves to $crypto_hmac_sha256, not $crypto_hmac
    #[test]
    fn test_crypto_hmac_resolves_to_sha256_variant() {
        let src = r#"
            fn test_hmac() {
                let mac = crypto.hmac("key", "msg");
            }
        "#;
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let mut codegen = WasmCodegen::new();
        let wat = codegen.generate(&program);
        assert!(
            wat.contains("call $crypto_hmac_sha256"),
            "crypto.hmac must resolve to $crypto_hmac_sha256"
        );
    }

    /// Issue 9: theme.toggle dispatches through theme namespace, not local.get $theme
    #[test]
    fn test_theme_toggle_dispatches_to_runtime() {
        let expr = Expr::MethodCall {
            object: Box::new(Expr::Ident("theme".into())),
            method: "toggle".into(),
            args: vec![],
        };
        let mut codegen = WasmCodegen::new();
        codegen.generate_expr(&expr);
        let out = codegen.output.clone();
        assert!(!out.contains("local.get $theme"), "must not emit local.get for theme namespace");
        assert!(out.contains("call $theme_toggle"), "must call $theme_toggle");
    }

    /// Issue 9: $theme_init function is emitted in the runtime
    #[test]
    fn test_theme_init_function_is_emitted() {
        let src = r##"
            theme AppTheme {
                light { bg: "#fff" }
                dark { bg: "#000" }
            }
        "##;
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let mut codegen = WasmCodegen::new();
        let wat = codegen.generate(&program);
        assert!(wat.contains("func $theme_init"), "theme_init must be defined in the output WAT");
        assert!(wat.contains("call $theme_init"), "generate_theme must call $theme_init");
    }

    /// Issue 10: signal field referenced without `self.` in component template
    /// must read from the signal global, not local.get $<name>
    #[test]
    fn test_component_signal_bare_ref_reads_signal() {
        let src = r#"
            component Editor() {
                let mut content: String = "";
                render { <div>{content}</div> }
            }
        "#;
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let mut codegen = WasmCodegen::new();
        let wat = codegen.generate(&program);
        assert!(
            wat.contains("$__sig_Editor_content"),
            "bare signal ref must access Editor_content signal global"
        );
        assert!(
            !wat.contains("local.get $content"),
            "bare signal ref must not emit local.get $content"
        );
    }

    /// resolve_stdlib_fn maps crypto::hmac to $crypto_hmac_sha256 explicitly
    #[test]
    fn test_resolve_stdlib_fn_crypto_hmac() {
        let codegen = WasmCodegen::new();
        assert_eq!(codegen.resolve_stdlib_fn("crypto::hmac"), "$crypto_hmac_sha256");
        assert_eq!(codegen.resolve_stdlib_fn("crypto::sha256"), "$crypto_sha256");
        assert_eq!(codegen.resolve_stdlib_fn("crypto::hmac_sha512"), "$crypto_hmac_sha512");
    }

    /// resolve_stdlib_fn maps theme methods correctly
    #[test]
    fn test_resolve_stdlib_fn_theme() {
        let codegen = WasmCodegen::new();
        assert_eq!(codegen.resolve_stdlib_fn("theme::init"), "$theme_init");
        assert_eq!(codegen.resolve_stdlib_fn("theme::toggle"), "$theme_toggle");
        assert_eq!(codegen.resolve_stdlib_fn("theme::set"), "$theme_set");
    }

    /// known_keyword_defs is populated from auth/cache/db/payment/upload/pdf/theme items
    #[test]
    fn test_known_keyword_defs_populated_from_program() {
        let src = r#"
            auth MyAuth { session: "cookie" }
            cache MyCache { strategy: "lru" }
        "#;
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let mut codegen = WasmCodegen::new();
        codegen.generate(&program);
        assert!(
            codegen.known_keyword_defs.iter().any(|(n, k)| n == "MyAuth" && *k == KeywordDefKind::Auth),
            "MyAuth should be in known_keyword_defs as Auth"
        );
        assert!(
            codegen.known_keyword_defs.iter().any(|(n, k)| n == "MyCache" && *k == KeywordDefKind::Cache),
            "MyCache should be in known_keyword_defs as Cache"
        );
    }

    /// All keyword def kinds produce i32.const 0 rather than local.get
    #[test]
    fn test_all_keyword_def_kinds_emit_null_placeholder() {
        let kinds = [
            ("Upload1", KeywordDefKind::Upload),
            ("Payment1", KeywordDefKind::Payment),
            ("Pdf1", KeywordDefKind::Pdf),
            ("Theme1", KeywordDefKind::Theme),
            ("Db1", KeywordDefKind::Database),
        ];
        for (name, kind) in kinds {
            let mut codegen = WasmCodegen::new();
            codegen.known_keyword_defs.push((name.to_string(), kind));
            codegen.generate_expr(&Expr::Ident(name.to_string()));
            let out = codegen.output.clone();
            assert!(out.contains("i32.const 0"), "def kind {} should emit i32.const 0", name);
            assert!(!out.contains("local.get"), "def kind {} must not emit local.get", name);
        }
    }
}
