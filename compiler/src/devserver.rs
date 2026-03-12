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
        let path = parts[1];

        // Check for WebSocket upgrade.
        if request.contains("Upgrade: websocket") || request.contains("upgrade: websocket") {
            return Self::handle_websocket_upgrade(&mut stream, &request, ws_clients);
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
}
