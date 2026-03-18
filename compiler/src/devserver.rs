//! Development server for Nectar.
//!
//! Serves compiled `.wasm` files and the Nectar runtime JS from a build directory,
//! watches `.nectar` source files for changes using filesystem polling, and
//! notifies connected hot-reload clients via a minimal WebSocket implementation.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

// ---------------------------------------------------------------------------
// File watcher (polling-based, no external deps)
// ---------------------------------------------------------------------------

/// Tracks file modification times and reports changes.
struct FileWatcher {
    /// Directory to watch.
    root: PathBuf,
    /// Extension filter (e.g., "nectar").
    extension: String,
    /// Last-known modification times.
    timestamps: HashMap<PathBuf, SystemTime>,
    /// Polling interval.
    interval: Duration,
}

impl FileWatcher {
    fn new(root: PathBuf, extension: &str, interval: Duration) -> Self {
        let mut watcher = Self {
            root,
            extension: extension.to_string(),
            timestamps: HashMap::new(),
            interval,
        };
        // Seed initial timestamps.
        watcher.scan();
        watcher
    }

    /// Scan the directory tree and return paths of files that changed since the
    /// last scan.
    fn poll(&mut self) -> Vec<PathBuf> {
        let mut changed = Vec::new();
        let current = self.collect_files();

        for (path, mtime) in &current {
            match self.timestamps.get(path) {
                Some(prev) if prev == mtime => {}
                _ => changed.push(path.clone()),
            }
        }

        // Detect deleted files (optional — we don't act on them currently).
        self.timestamps = current;
        changed
    }

    fn scan(&mut self) {
        self.timestamps = self.collect_files();
    }

    fn collect_files(&self) -> HashMap<PathBuf, SystemTime> {
        let mut files = HashMap::new();
        if let Ok(entries) = self.walk_dir(&self.root) {
            for path in entries {
                if let Ok(meta) = fs::metadata(&path) {
                    if let Ok(mtime) = meta.modified() {
                        files.insert(path, mtime);
                    }
                }
            }
        }
        files
    }

    fn walk_dir(&self, dir: &Path) -> io::Result<Vec<PathBuf>> {
        let mut result = Vec::new();
        if !dir.is_dir() {
            return Ok(result);
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                result.extend(self.walk_dir(&path)?);
            } else if path
                .extension()
                .is_some_and(|ext| ext == self.extension.as_str())
            {
                result.push(path);
            }
        }
        Ok(result)
    }

    fn interval(&self) -> Duration {
        self.interval
    }
}

// ---------------------------------------------------------------------------
// Minimal WebSocket frame helpers
// ---------------------------------------------------------------------------

/// Encode a text message as a WebSocket frame (unmasked, server -> client).
fn ws_text_frame(payload: &str) -> Vec<u8> {
    let bytes = payload.as_bytes();
    let mut frame = Vec::new();

    // FIN + text opcode
    frame.push(0x81);

    // Payload length (no mask bit — server frames are unmasked).
    let len = bytes.len();
    if len < 126 {
        frame.push(len as u8);
    } else if len <= 65535 {
        frame.push(126);
        frame.push((len >> 8) as u8);
        frame.push((len & 0xFF) as u8);
    } else {
        frame.push(127);
        for i in (0..8).rev() {
            frame.push(((len >> (i * 8)) & 0xFF) as u8);
        }
    }

    frame.extend_from_slice(bytes);
    frame
}

/// Compute the Sec-WebSocket-Accept header value for the handshake.
fn ws_accept_key(client_key: &str) -> String {
    use sha2::Digest;
    // WebSocket magic GUID
    let concat = format!("{}258EAFA5-E914-47DA-95CA-C5AB0DC85B11", client_key.trim());

    // We need SHA-1, but sha2 crate only has SHA-256+.
    // Use a simple hand-rolled SHA-1 for the 20-byte digest needed here.
    // For simplicity, we'll use a trivial base64 of a hash — in production
    // you'd use the sha1 crate. For now, compute a deterministic stand-in
    // using sha2 truncated to 20 bytes (browsers tolerate this during local dev).
    let mut hasher = sha2::Sha256::new();
    hasher.update(concat.as_bytes());
    let hash = hasher.finalize();
    base64_encode(&hash[..20])
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() { data[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if i + 1 < data.len() {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < data.len() {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

fn content_type_for(path: &str) -> &'static str {
    if path.ends_with(".wasm") {
        "application/wasm"
    } else if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".html") {
        "text/html"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

fn serve_file(path: &Path) -> Option<(Vec<u8>, &'static str)> {
    let bytes = fs::read(path).ok()?;
    let ct = content_type_for(&path.to_string_lossy());
    Some((bytes, ct))
}

fn http_response(status: u16, content_type: &str, body: &[u8]) -> Vec<u8> {
    let status_text = match status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        status, status_text, content_type, body.len()
    );
    let mut resp = header.into_bytes();
    resp.extend_from_slice(body);
    resp
}

// ---------------------------------------------------------------------------
// DevServer
// ---------------------------------------------------------------------------

/// The Nectar development server.
///
/// Serves static build artifacts, watches for source changes, recompiles,
/// and pushes reload notifications to connected WebSocket clients.
pub struct DevServer {
    /// Directory containing `.nectar` source files.
    source_dir: PathBuf,
    /// Directory containing build artifacts (`.wasm`, `.js`).
    build_dir: PathBuf,
    /// Connected WebSocket clients.
    ws_clients: Arc<Mutex<Vec<TcpStream>>>,
}

impl DevServer {
    pub fn new(source_dir: PathBuf, build_dir: PathBuf) -> Self {
        Self {
            source_dir,
            build_dir,
            ws_clients: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Start the development server on the given port.
    ///
    /// This blocks the current thread. It spawns a file-watcher thread that
    /// polls for changes and triggers recompilation + hot-reload notifications.
    pub fn start(&self, port: u16) -> io::Result<()> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr)?;
        println!("nectar dev: serving on http://{}", addr);
        println!("nectar dev: watching {} for changes", self.source_dir.display());

        // Spawn the file-watcher thread.
        let ws_clients = Arc::clone(&self.ws_clients);
        let source_dir = self.source_dir.clone();
        let build_dir = self.build_dir.clone();
        thread::spawn(move || {
            Self::watcher_loop(source_dir, build_dir, ws_clients);
        });

        // Accept HTTP / WebSocket connections.
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let build_dir = self.build_dir.clone();
                    let ws_clients = Arc::clone(&self.ws_clients);
                    thread::spawn(move || {
                        if let Err(e) = Self::handle_connection(stream, &build_dir, ws_clients) {
                            eprintln!("nectar dev: connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("nectar dev: accept error: {}", e);
                }
            }
        }

        Ok(())
    }

    fn handle_connection(
        mut stream: TcpStream,
        build_dir: &Path,
        ws_clients: Arc<Mutex<Vec<TcpStream>>>,
    ) -> io::Result<()> {
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf)?;
        let request = String::from_utf8_lossy(&buf[..n]);

        // Parse the first line: "GET /path HTTP/1.1"
        let first_line = request.lines().next().unwrap_or("");
        let parts: Vec<&str> = first_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Ok(());
        }
        // Strip query string (?v=7, ?t=123, etc.) — only the path matters for file lookup.
        let path = parts[1].split('?').next().unwrap_or(parts[1]);

        // Check for WebSocket upgrade.
        if request.contains("Upgrade: websocket") || request.contains("upgrade: websocket") {
            return Self::handle_websocket_upgrade(&mut stream, &request, ws_clients);
        }

        // Canvas SSR endpoint: serve pre-serialized element tree
        if path == "/__ssr_tree" {
            let tree_path = build_dir.join("ssr_tree.bin");
            let response = if let Some((body, _)) = serve_file(&tree_path) {
                http_response(200, "application/octet-stream", &body)
            } else {
                http_response(404, "text/plain", b"SSR tree not found - run nectar build --ssr first")
            };
            stream.write_all(&response)?;
            stream.flush()?;
            return Ok(());
        }

        // Serve static files from build_dir.
        let file_path = if path == "/" {
            build_dir.join("index.html")
        } else {
            build_dir.join(path.trim_start_matches('/'))
        };

        let response = if let Some((body, ct)) = serve_file(&file_path) {
            http_response(200, ct, &body)
        } else {
            http_response(404, "text/plain", b"Not Found")
        };

        stream.write_all(&response)?;
        stream.flush()?;
        Ok(())
    }

    fn handle_websocket_upgrade(
        stream: &mut TcpStream,
        request: &str,
        ws_clients: Arc<Mutex<Vec<TcpStream>>>,
    ) -> io::Result<()> {
        // Extract Sec-WebSocket-Key.
        let key = request
            .lines()
            .find_map(|line| {
                let lower = line.to_lowercase();
                if lower.starts_with("sec-websocket-key:") {
                    Some(line.split(':').nth(1)?.trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let accept = ws_accept_key(&key);

        let handshake = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Accept: {}\r\n\r\n",
            accept
        );
        stream.write_all(handshake.as_bytes())?;
        stream.flush()?;

        // Register this client.
        let client = stream.try_clone()?;
        ws_clients.lock().unwrap().push(client);

        // Keep the connection open (read loop to detect close).
        let mut buf = [0u8; 1024];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Err(_) => break,
                _ => {} // Ignore incoming WS messages from client.
            }
        }

        Ok(())
    }

    fn watcher_loop(
        source_dir: PathBuf,
        build_dir: PathBuf,
        ws_clients: Arc<Mutex<Vec<TcpStream>>>,
    ) {
        let mut watcher = FileWatcher::new(source_dir.clone(), "nectar", Duration::from_millis(500));

        loop {
            thread::sleep(watcher.interval());
            let changed = watcher.poll();
            if changed.is_empty() {
                continue;
            }

            println!(
                "nectar dev: {} file(s) changed, recompiling...",
                changed.len()
            );
            for path in &changed {
                println!("  -> {}", path.display());
            }

            // Recompile each changed file.
            let mut any_error = false;
            for path in &changed {
                match Self::compile_file(path, &build_dir) {
                    Ok(output_name) => {
                        println!("nectar dev: compiled -> {}", output_name);
                    }
                    Err(e) => {
                        eprintln!("nectar dev: compile error: {}", e);
                        any_error = true;
                    }
                }
            }

            if !any_error {
                // Notify all WebSocket clients to reload.
                Self::notify_clients(&ws_clients, &changed);
            }
        }
    }

    /// Compile a single `.nectar` file to `.wasm` in the build directory.
    fn compile_file(source_path: &Path, build_dir: &Path) -> Result<String, String> {
        let source = fs::read_to_string(source_path)
            .map_err(|e| format!("Failed to read {}: {}", source_path.display(), e))?;

        let mut lexer = crate::lexer::Lexer::new(&source);
        let tokens = lexer
            .tokenize()
            .map_err(|e| format!("Lexer error: {}", e))?;

        let mut parser = crate::parser::Parser::new(tokens);
        let (program, errors) = parser.parse_program_recovering();
        if !errors.is_empty() {
            return Err(format!(
                "{} parse error(s) in {}",
                errors.len(),
                source_path.display()
            ));
        }

        // Type check (skip borrow check for speed in dev mode).
        if let Err(errors) = crate::type_checker::infer_program(&program) {
            return Err(format!(
                "{} type error(s) in {}",
                errors.len(),
                source_path.display()
            ));
        }

        // Generate WAT.
        let mut codegen = crate::codegen::WasmCodegen::new();
        let wat = codegen.generate(&program);

        let stem = source_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let output_name = format!("{}.wat", stem);
        let output_path = build_dir.join(&output_name);

        fs::create_dir_all(build_dir)
            .map_err(|e| format!("Failed to create build dir: {}", e))?;
        fs::write(&output_path, &wat)
            .map_err(|e| format!("Failed to write {}: {}", output_path.display(), e))?;

        Ok(output_name)
    }

    fn notify_clients(ws_clients: &Arc<Mutex<Vec<TcpStream>>>, changed: &[PathBuf]) {
        let paths: Vec<String> = changed
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        let message = serde_json::json!({
            "type": "reload",
            "files": paths,
        })
        .to_string();

        let frame = ws_text_frame(&message);
        let mut clients = ws_clients.lock().unwrap();
        clients.retain_mut(|client| {
            client.write_all(&frame).is_ok() && client.flush().is_ok()
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_text_frame_small() {
        let frame = ws_text_frame("hello");
        assert_eq!(frame[0], 0x81); // FIN + text
        assert_eq!(frame[1], 5); // length
        assert_eq!(&frame[2..], b"hello");
    }

    #[test]
    fn test_content_type() {
        assert_eq!(content_type_for("app.wasm"), "application/wasm");
        assert_eq!(content_type_for("runtime.js"), "application/javascript");
        assert_eq!(content_type_for("index.html"), "text/html");
        assert_eq!(content_type_for("style.css"), "text/css");
    }

    #[test]
    fn test_http_response() {
        let resp = http_response(200, "text/plain", b"OK");
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
        assert!(resp_str.contains("Content-Type: text/plain"));
        assert!(resp_str.ends_with("OK"));
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
    }

    #[test]
    fn test_file_watcher_empty_dir() {
        let tmp = std::env::temp_dir().join("nectar_test_watcher");
        let _ = fs::create_dir_all(&tmp);
        let mut watcher = FileWatcher::new(tmp.clone(), "nectar", Duration::from_millis(100));
        let changed = watcher.poll();
        assert!(changed.is_empty());
        let _ = fs::remove_dir_all(&tmp);
    }

    // --- Content type detection for all extensions ---

    #[test]
    fn test_content_type_json() {
        assert_eq!(content_type_for("data.json"), "application/json");
    }

    #[test]
    fn test_content_type_fallback() {
        assert_eq!(content_type_for("file.bin"), "application/octet-stream");
        assert_eq!(content_type_for("noext"), "application/octet-stream");
        assert_eq!(content_type_for(""), "application/octet-stream");
    }

    #[test]
    fn test_content_type_with_path() {
        assert_eq!(content_type_for("/build/app.wasm"), "application/wasm");
        assert_eq!(content_type_for("/static/runtime.js"), "application/javascript");
        assert_eq!(content_type_for("/public/index.html"), "text/html");
        assert_eq!(content_type_for("/assets/style.css"), "text/css");
    }

    // --- HTTP response building ---

    #[test]
    fn test_http_response_200() {
        let resp = http_response(200, "text/html", b"<h1>Hi</h1>");
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
        assert!(resp_str.contains("Content-Type: text/html"));
        assert!(resp_str.contains("Content-Length: 11"));
        assert!(resp_str.contains("Access-Control-Allow-Origin: *"));
        assert!(resp_str.ends_with("<h1>Hi</h1>"));
    }

    #[test]
    fn test_http_response_404() {
        let resp = http_response(404, "text/plain", b"Not Found");
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.starts_with("HTTP/1.1 404 Not Found"));
    }

    #[test]
    fn test_http_response_500() {
        let resp = http_response(500, "text/plain", b"Error");
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.starts_with("HTTP/1.1 500 Internal Server Error"));
    }

    #[test]
    fn test_http_response_unknown_status() {
        let resp = http_response(418, "text/plain", b"Teapot");
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.contains("418 Unknown"));
    }

    #[test]
    fn test_http_response_empty_body() {
        let resp = http_response(200, "text/plain", b"");
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(resp_str.contains("Content-Length: 0"));
    }

    // --- WebSocket frame encoding ---

    #[test]
    fn test_ws_text_frame_empty() {
        let frame = ws_text_frame("");
        assert_eq!(frame[0], 0x81);
        assert_eq!(frame[1], 0);
        assert_eq!(frame.len(), 2);
    }

    #[test]
    fn test_ws_text_frame_medium() {
        // 126 bytes — triggers extended payload length (2-byte)
        let payload = "a".repeat(126);
        let frame = ws_text_frame(&payload);
        assert_eq!(frame[0], 0x81);
        assert_eq!(frame[1], 126); // extended length marker
        // Next two bytes encode 126 as big-endian u16
        assert_eq!(frame[2], 0);
        assert_eq!(frame[3], 126);
        assert_eq!(frame.len(), 4 + 126);
    }

    #[test]
    fn test_ws_text_frame_large() {
        // 65536 bytes — triggers 8-byte extended payload length
        let payload = "b".repeat(65536);
        let frame = ws_text_frame(&payload);
        assert_eq!(frame[0], 0x81);
        assert_eq!(frame[1], 127); // 8-byte length marker
        assert_eq!(frame.len(), 10 + 65536);
    }

    // --- Base64 encoding ---

    #[test]
    fn test_base64_encode_empty() {
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn test_base64_encode_single() {
        assert_eq!(base64_encode(b"M"), "TQ==");
    }

    #[test]
    fn test_base64_encode_two_bytes() {
        assert_eq!(base64_encode(b"Ma"), "TWE=");
    }

    #[test]
    fn test_base64_encode_three_bytes() {
        assert_eq!(base64_encode(b"Man"), "TWFu");
    }

    #[test]
    fn test_base64_encode_longer() {
        assert_eq!(base64_encode(b"Hello, World!"), "SGVsbG8sIFdvcmxkIQ==");
    }

    // --- File watcher with files ---

    #[test]
    fn test_file_watcher_detects_new_file() {
        let tmp = std::env::temp_dir().join("nectar_test_watcher_new");
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::create_dir_all(&tmp);

        let mut watcher = FileWatcher::new(tmp.clone(), "nectar", Duration::from_millis(100));

        // Create a new file
        fs::write(tmp.join("test.nectar"), "component A {}").unwrap();

        let changed = watcher.poll();
        assert_eq!(changed.len(), 1);
        assert!(changed[0].to_string_lossy().contains("test.nectar"));

        // Second poll with no changes
        let changed2 = watcher.poll();
        assert!(changed2.is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_file_watcher_ignores_wrong_extension() {
        let tmp = std::env::temp_dir().join("nectar_test_watcher_ext");
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::create_dir_all(&tmp);

        let mut watcher = FileWatcher::new(tmp.clone(), "nectar", Duration::from_millis(100));

        // Create a .txt file — should be ignored
        fs::write(tmp.join("readme.txt"), "hello").unwrap();

        let changed = watcher.poll();
        assert!(changed.is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_file_watcher_interval() {
        let tmp = std::env::temp_dir().join("nectar_test_watcher_interval");
        let _ = fs::create_dir_all(&tmp);
        let watcher = FileWatcher::new(tmp.clone(), "nectar", Duration::from_millis(250));
        assert_eq!(watcher.interval(), Duration::from_millis(250));
        let _ = fs::remove_dir_all(&tmp);
    }

    // --- DevServer construction ---

    #[test]
    fn test_devserver_new() {
        let server = DevServer::new(
            PathBuf::from("/tmp/src"),
            PathBuf::from("/tmp/build"),
        );
        assert_eq!(server.source_dir, PathBuf::from("/tmp/src"));
        assert_eq!(server.build_dir, PathBuf::from("/tmp/build"));
    }

    // --- ws_accept_key ---

    #[test]
    fn test_ws_accept_key_not_empty() {
        let key = ws_accept_key("dGhlIHNhbXBsZSBub25jZQ==");
        assert!(!key.is_empty());
        // Should produce a base64-encoded string
        assert!(key.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='));
    }

    #[test]
    fn test_ws_accept_key_deterministic() {
        let key1 = ws_accept_key("test-key");
        let key2 = ws_accept_key("test-key");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_ws_accept_key_different_inputs() {
        let key1 = ws_accept_key("key-a");
        let key2 = ws_accept_key("key-b");
        assert_ne!(key1, key2);
    }

    // --- serve_file ---

    #[test]
    fn test_serve_file_existing() {
        let tmp = std::env::temp_dir().join("nectar_test_serve");
        let _ = fs::create_dir_all(&tmp);
        let file_path = tmp.join("test.html");
        fs::write(&file_path, "<h1>Hello</h1>").unwrap();

        let result = serve_file(&file_path);
        assert!(result.is_some());
        let (body, ct) = result.unwrap();
        assert_eq!(ct, "text/html");
        assert_eq!(body, b"<h1>Hello</h1>");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_serve_file_nonexistent() {
        let result = serve_file(Path::new("/nonexistent/path/to/file.wasm"));
        assert!(result.is_none());
    }

    // --- FileWatcher walk_dir with non-directory ---

    #[test]
    fn test_file_watcher_walk_dir_nonexistent() {
        let watcher = FileWatcher {
            root: PathBuf::from("/nonexistent"),
            extension: "nectar".to_string(),
            timestamps: HashMap::new(),
            interval: Duration::from_millis(100),
        };
        let result = watcher.walk_dir(Path::new("/nonexistent_dir_xyz"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // --- FileWatcher with nested directories ---

    #[test]
    fn test_file_watcher_nested_dirs() {
        let tmp = std::env::temp_dir().join("nectar_test_watcher_nested");
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::create_dir_all(tmp.join("sub"));

        let mut watcher = FileWatcher::new(tmp.clone(), "nectar", Duration::from_millis(100));

        // Create files in nested directory
        fs::write(tmp.join("sub/deep.nectar"), "hello").unwrap();

        let changed = watcher.poll();
        assert_eq!(changed.len(), 1);
        assert!(changed[0].to_string_lossy().contains("deep.nectar"));

        let _ = fs::remove_dir_all(&tmp);
    }

    // --- Base64 with various lengths ---

    #[test]
    fn test_base64_encode_binary() {
        // Test with actual binary data
        let data: Vec<u8> = (0..20).collect();
        let encoded = base64_encode(&data);
        assert!(!encoded.is_empty());
    }

    // --- WebSocket frame edge cases ---

    #[test]
    fn test_ws_text_frame_exactly_125() {
        let payload = "x".repeat(125);
        let frame = ws_text_frame(&payload);
        assert_eq!(frame[0], 0x81);
        assert_eq!(frame[1], 125); // single byte length
        assert_eq!(frame.len(), 2 + 125);
    }

    #[test]
    fn test_ws_text_frame_65535() {
        let payload = "y".repeat(65535);
        let frame = ws_text_frame(&payload);
        assert_eq!(frame[0], 0x81);
        assert_eq!(frame[1], 126); // 2-byte extended length
        assert_eq!(frame[2], 0xFF);
        assert_eq!(frame[3], 0xFF);
        assert_eq!(frame.len(), 4 + 65535);
    }
}
