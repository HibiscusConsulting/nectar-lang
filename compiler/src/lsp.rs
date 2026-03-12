//! Language Server Protocol (LSP) implementation for Nectar.
//!
//! Communicates over stdin/stdout using JSON-RPC as specified by the LSP spec.
//! Provides completions, hover info, go-to-definition, and diagnostics by
//! reusing the existing lexer, parser, type checker, and borrow checker.

use std::collections::HashMap;
use std::io::{self, BufRead, Read as _, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ast::{Program, Item};
use crate::borrow_checker;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::token::Span;
use crate::type_checker;

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcNotification {
    jsonrpc: String,
    method: String,
    params: Value,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

// ---------------------------------------------------------------------------
// LSP message types (minimal subset)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct TextDocumentIdentifier {
    uri: String,
}

#[derive(Deserialize)]
struct TextDocumentItem {
    uri: String,
    #[serde(rename = "languageId")]
    #[allow(dead_code)]
    language_id: String,
    #[allow(dead_code)]
    version: i64,
    text: String,
}

#[derive(Deserialize)]
struct DidOpenParams {
    #[serde(rename = "textDocument")]
    text_document: TextDocumentItem,
}

#[derive(Deserialize)]
struct DidChangeParams {
    #[serde(rename = "textDocument")]
    text_document: VersionedTextDocumentIdentifier,
    #[serde(rename = "contentChanges")]
    content_changes: Vec<ContentChange>,
}

#[derive(Deserialize)]
struct VersionedTextDocumentIdentifier {
    uri: String,
    #[allow(dead_code)]
    version: Option<i64>,
}

#[derive(Deserialize)]
struct ContentChange {
    text: String,
}

#[derive(Deserialize)]
struct CompletionParams {
    #[serde(rename = "textDocument")]
    text_document: TextDocumentIdentifier,
    position: Position,
}

#[derive(Deserialize)]
struct HoverParams {
    #[serde(rename = "textDocument")]
    text_document: TextDocumentIdentifier,
    position: Position,
}

#[derive(Deserialize)]
struct DefinitionParams {
    #[serde(rename = "textDocument")]
    text_document: TextDocumentIdentifier,
    position: Position,
}

#[derive(Deserialize, Serialize, Clone)]
struct Position {
    line: u32,
    character: u32,
}

#[derive(Serialize, Clone)]
struct Range {
    start: Position,
    end: Position,
}

#[derive(Serialize)]
struct Diagnostic {
    range: Range,
    severity: u32,
    source: String,
    message: String,
}

#[derive(Serialize)]
struct CompletionItem {
    label: String,
    kind: u32,
    detail: Option<String>,
}

#[derive(Serialize)]
struct Location {
    uri: String,
    range: Range,
}

// ---------------------------------------------------------------------------
// Document state
// ---------------------------------------------------------------------------

struct DocumentState {
    text: String,
    program: Option<Program>,
}

// ---------------------------------------------------------------------------
// LSP Server
// ---------------------------------------------------------------------------

/// The Nectar Language Server. Call `run()` to start the stdin/stdout loop.
pub struct LspServer {
    documents: HashMap<String, DocumentState>,
}

impl LspServer {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    /// Run the LSP server, reading JSON-RPC messages from stdin and writing
    /// responses to stdout.
    pub fn run(&mut self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut reader = stdin.lock();

        loop {
            // Read Content-Length header
            let mut header = String::new();
            loop {
                header.clear();
                let bytes_read = reader.read_line(&mut header)?;
                if bytes_read == 0 {
                    return Ok(()); // EOF
                }
                let trimmed = header.trim();
                if trimmed.is_empty() {
                    break;
                }
            }

            // Parse Content-Length from accumulated headers
            // We need to re-read lines until we find Content-Length
            let content_length = self.read_content_length(&mut reader)?;

            // Read the body
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body)?;

            let body_str = String::from_utf8_lossy(&body);

            if let Ok(request) = serde_json::from_str::<JsonRpcRequest>(&body_str) {
                self.handle_message(request)?;
            }
        }
    }

    fn read_content_length(&self, reader: &mut impl BufRead) -> io::Result<usize> {
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = trimmed.strip_prefix("Content-Length: ") {
                length = value.parse().unwrap_or(0);
            }
        }
        Ok(length)
    }

    fn handle_message(&mut self, request: JsonRpcRequest) -> io::Result<()> {
        match request.method.as_str() {
            "initialize" => {
                let result = serde_json::json!({
                    "capabilities": {
                        "textDocumentSync": 1, // Full sync
                        "completionProvider": {
                            "triggerCharacters": [".", ":"]
                        },
                        "hoverProvider": true,
                        "definitionProvider": true
                    },
                    "serverInfo": {
                        "name": "nectar-lsp",
                        "version": "0.1.0"
                    }
                });
                self.send_response(request.id, Some(result), None)?;
            }
            "initialized" => {
                // Client acknowledges initialization — nothing to do.
            }
            "shutdown" => {
                self.send_response(request.id, Some(Value::Null), None)?;
            }
            "exit" => {
                std::process::exit(0);
            }
            "textDocument/didOpen" => {
                if let Some(params) = request.params {
                    if let Ok(p) = serde_json::from_value::<DidOpenParams>(params) {
                        self.on_did_open(p)?;
                    }
                }
            }
            "textDocument/didChange" => {
                if let Some(params) = request.params {
                    if let Ok(p) = serde_json::from_value::<DidChangeParams>(params) {
                        self.on_did_change(p)?;
                    }
                }
            }
            "textDocument/completion" => {
                if let Some(params) = request.params {
                    if let Ok(p) = serde_json::from_value::<CompletionParams>(params) {
                        let items = self.on_completion(p);
                        self.send_response(request.id, Some(serde_json::to_value(items).unwrap()), None)?;
                    }
                }
            }
            "textDocument/hover" => {
                if let Some(params) = request.params {
                    if let Ok(p) = serde_json::from_value::<HoverParams>(params) {
                        let result = self.on_hover(p);
                        self.send_response(request.id, result, None)?;
                    }
                }
            }
            "textDocument/definition" => {
                if let Some(params) = request.params {
                    if let Ok(p) = serde_json::from_value::<DefinitionParams>(params) {
                        let result = self.on_definition(p);
                        self.send_response(request.id, result, None)?;
                    }
                }
            }
            _ => {
                // Unknown method — ignore notifications, send error for requests.
                if request.id.is_some() {
                    self.send_response(
                        request.id,
                        None,
                        Some(JsonRpcError {
                            code: -32601,
                            message: format!("Method not found: {}", request.method),
                        }),
                    )?;
                }
            }
        }
        Ok(())
    }

    // -- Document events ----------------------------------------------------

    fn on_did_open(&mut self, params: DidOpenParams) -> io::Result<()> {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();
        let program = self.parse_document(&text);
        let diagnostics = self.collect_diagnostics(&text, &program);

        self.documents.insert(
            uri.clone(),
            DocumentState { text, program },
        );

        self.publish_diagnostics(&uri, diagnostics)
    }

    fn on_did_change(&mut self, params: DidChangeParams) -> io::Result<()> {
        let uri = params.text_document.uri.clone();

        // Full document sync — use the last content change.
        if let Some(change) = params.content_changes.into_iter().last() {
            let text = change.text;
            let program = self.parse_document(&text);
            let diagnostics = self.collect_diagnostics(&text, &program);

            self.documents.insert(
                uri.clone(),
                DocumentState { text, program },
            );

            self.publish_diagnostics(&uri, diagnostics)?;
        }

        Ok(())
    }

    // -- Completion ---------------------------------------------------------

    fn on_completion(&self, params: CompletionParams) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        // Keyword completions
        let keywords = [
            "let", "mut", "fn", "component", "render", "struct", "enum",
            "impl", "trait", "if", "else", "match", "for", "in", "while",
            "return", "own", "ref", "self", "Self", "pub", "use", "mod",
            "true", "false", "signal", "store", "action", "effect",
            "computed", "async", "await", "fetch", "derive", "spawn",
            "channel", "select", "parallel", "stream", "lazy", "suspend",
            "yield", "agent", "prompt", "tool", "route", "link",
            "navigate", "router", "fallback", "guard", "style",
            "i32", "i64", "f32", "f64", "u32", "u64", "bool", "String",
        ];
        for kw in &keywords {
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: 14, // Keyword
                detail: Some("keyword".to_string()),
            });
        }

        // Component/function/struct name completions from the current document.
        let uri = &params.text_document.uri;
        if let Some(doc) = self.documents.get(uri) {
            if let Some(program) = &doc.program {
                for item in &program.items {
                    match item {
                        Item::Component(c) => {
                            items.push(CompletionItem {
                                label: c.name.clone(),
                                kind: 7, // Class
                                detail: Some("component".to_string()),
                            });
                        }
                        Item::Function(f) => {
                            items.push(CompletionItem {
                                label: f.name.clone(),
                                kind: 3, // Function
                                detail: Some("function".to_string()),
                            });
                        }
                        Item::Struct(s) => {
                            items.push(CompletionItem {
                                label: s.name.clone(),
                                kind: 22, // Struct
                                detail: Some("struct".to_string()),
                            });
                        }
                        Item::Enum(e) => {
                            items.push(CompletionItem {
                                label: e.name.clone(),
                                kind: 13, // Enum
                                detail: Some("enum".to_string()),
                            });
                        }
                        Item::Store(s) => {
                            items.push(CompletionItem {
                                label: s.name.clone(),
                                kind: 7, // Class
                                detail: Some("store".to_string()),
                            });
                        }
                        _ => {}
                    }
                }
            }
        }

        // Field/method completions based on type info.
        // We look for a "." trigger at the cursor position and try to resolve
        // the type of the expression before the dot.
        let _position = &params.position;
        if let Some(doc) = self.documents.get(uri) {
            if let Some(program) = &doc.program {
                // Extract struct field completions for any struct in scope
                for item in &program.items {
                    if let Item::Struct(s) = item {
                        for field in &s.fields {
                            items.push(CompletionItem {
                                label: field.name.clone(),
                                kind: 5, // Field
                                detail: Some(format!("{}.{}", s.name, field.name)),
                            });
                        }
                    }
                    if let Item::Impl(imp) = item {
                        for method in &imp.methods {
                            items.push(CompletionItem {
                                label: method.name.clone(),
                                kind: 2, // Method
                                detail: Some(format!("{}::{}", imp.target, method.name)),
                            });
                        }
                    }
                }
            }
        }

        items
    }

    // -- Hover --------------------------------------------------------------

    fn on_hover(&self, params: HoverParams) -> Option<Value> {
        let uri = &params.text_document.uri;
        let doc = self.documents.get(uri)?;
        let program = doc.program.as_ref()?;

        let line = params.position.line + 1; // LSP is 0-based, our spans are 1-based
        let col = params.position.character + 1;

        // Try to find a function/component/struct at this position via type info.
        for item in &program.items {
            match item {
                Item::Function(f) => {
                    if self.span_contains(&f.span, line, col) {
                        let ret = f
                            .return_type
                            .as_ref()
                            .map(|t| format!(" -> {:?}", t))
                            .unwrap_or_default();
                        let params: Vec<String> = f
                            .params
                            .iter()
                            .map(|p| format!("{}: {:?}", p.name, p.ty))
                            .collect();
                        let sig = format!("fn {}({}){}", f.name, params.join(", "), ret);
                        return Some(serde_json::json!({
                            "contents": { "kind": "markdown", "value": format!("```nectar\n{}\n```", sig) }
                        }));
                    }
                }
                Item::Component(c) => {
                    if self.span_contains(&c.span, line, col) {
                        let props: Vec<String> = c
                            .props
                            .iter()
                            .map(|p| format!("{}: {:?}", p.name, p.ty))
                            .collect();
                        let sig = format!("component {}({})", c.name, props.join(", "));
                        return Some(serde_json::json!({
                            "contents": { "kind": "markdown", "value": format!("```nectar\n{}\n```", sig) }
                        }));
                    }
                }
                Item::Struct(s) => {
                    if self.span_contains(&s.span, line, col) {
                        let fields: Vec<String> = s
                            .fields
                            .iter()
                            .map(|f| format!("  {}: {:?}", f.name, f.ty))
                            .collect();
                        let sig = format!("struct {} {{\n{}\n}}", s.name, fields.join(",\n"));
                        return Some(serde_json::json!({
                            "contents": { "kind": "markdown", "value": format!("```nectar\n{}\n```", sig) }
                        }));
                    }
                }
                _ => {}
            }
        }

        // If we have type checker results, try to look up the identifier under
        // the cursor and show its inferred type.
        if let Ok(typed) = type_checker::infer_program(program) {
            let identifier = self.identifier_at_position(&doc.text, line, col);
            if let Some(ident) = identifier {
                if let Some(ty) = typed.bindings.get(&ident) {
                    return Some(serde_json::json!({
                        "contents": {
                            "kind": "markdown",
                            "value": format!("```nectar\n{}: {}\n```", ident, ty)
                        }
                    }));
                }
            }
        }

        None
    }

    // -- Go to definition ---------------------------------------------------

    fn on_definition(&self, params: DefinitionParams) -> Option<Value> {
        let uri = &params.text_document.uri;
        let doc = self.documents.get(uri)?;
        let program = doc.program.as_ref()?;

        let line = params.position.line + 1;
        let col = params.position.character + 1;

        let identifier = self.identifier_at_position(&doc.text, line, col)?;

        // Search for the definition of this identifier.
        for item in &program.items {
            match item {
                Item::Function(f) if f.name == identifier => {
                    return Some(serde_json::to_value(Location {
                        uri: uri.clone(),
                        range: self.span_to_range(&f.span),
                    }).ok()?);
                }
                Item::Component(c) if c.name == identifier => {
                    return Some(serde_json::to_value(Location {
                        uri: uri.clone(),
                        range: self.span_to_range(&c.span),
                    }).ok()?);
                }
                Item::Struct(s) if s.name == identifier => {
                    return Some(serde_json::to_value(Location {
                        uri: uri.clone(),
                        range: self.span_to_range(&s.span),
                    }).ok()?);
                }
                Item::Enum(e) if e.name == identifier => {
                    return Some(serde_json::to_value(Location {
                        uri: uri.clone(),
                        range: self.span_to_range(&e.span),
                    }).ok()?);
                }
                Item::Store(s) if s.name == identifier => {
                    return Some(serde_json::to_value(Location {
                        uri: uri.clone(),
                        range: self.span_to_range(&s.span),
                    }).ok()?);
                }
                Item::Impl(imp) => {
                    for method in &imp.methods {
                        if method.name == identifier {
                            return Some(serde_json::to_value(Location {
                                uri: uri.clone(),
                                range: self.span_to_range(&method.span),
                            }).ok()?);
                        }
                    }
                }
                _ => {}
            }
        }

        None
    }

    // -- Diagnostics --------------------------------------------------------

    fn collect_diagnostics(&self, source: &str, program: &Option<Program>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // If parsing failed (program is None), we re-lex + parse to capture errors.
        if program.is_none() {
            let mut lexer = Lexer::new(source);
            match lexer.tokenize() {
                Err(e) => {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position { line: e.line.saturating_sub(1), character: e.col.saturating_sub(1) },
                            end: Position { line: e.line.saturating_sub(1), character: e.col },
                        },
                        severity: 1, // Error
                        source: "nectar-lexer".to_string(),
                        message: e.message.clone(),
                    });
                }
                Ok(tokens) => {
                    let mut parser = Parser::new(tokens);
                    let (_prog, parse_errors) = parser.parse_program_recovering();
                    for e in &parse_errors {
                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position {
                                    line: e.span.line.saturating_sub(1),
                                    character: e.span.col.saturating_sub(1),
                                },
                                end: Position {
                                    line: e.span.line.saturating_sub(1),
                                    character: e.span.col,
                                },
                            },
                            severity: 1,
                            source: "nectar-parser".to_string(),
                            message: e.message.clone(),
                        });
                    }
                }
            }
            return diagnostics;
        }

        let program = program.as_ref().unwrap();

        // Borrow checker errors
        if let Err(errors) = borrow_checker::check(program) {
            for err in &errors {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: err.span.line.saturating_sub(1),
                            character: err.span.col.saturating_sub(1),
                        },
                        end: Position {
                            line: err.span.line.saturating_sub(1),
                            character: err.span.col,
                        },
                    },
                    severity: 1,
                    source: "nectar-borrow".to_string(),
                    message: err.message.clone(),
                });
            }
        }

        // Type checker errors
        if let Err(errors) = type_checker::infer_program(program) {
            for err in &errors {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position { line: 0, character: 0 },
                        end: Position { line: 0, character: 1 },
                    },
                    severity: 1,
                    source: "nectar-types".to_string(),
                    message: format!("{}", err),
                });
            }
        }

        diagnostics
    }

    fn publish_diagnostics(&self, uri: &str, diagnostics: Vec<Diagnostic>) -> io::Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "textDocument/publishDiagnostics".to_string(),
            params: serde_json::json!({
                "uri": uri,
                "diagnostics": diagnostics.iter().map(|d| serde_json::json!({
                    "range": {
                        "start": { "line": d.range.start.line, "character": d.range.start.character },
                        "end": { "line": d.range.end.line, "character": d.range.end.character },
                    },
                    "severity": d.severity,
                    "source": d.source,
                    "message": d.message,
                })).collect::<Vec<_>>(),
            }),
        };
        let body = serde_json::to_string(&notification).unwrap();
        self.write_message(&body)
    }

    // -- Helpers ------------------------------------------------------------

    fn parse_document(&self, source: &str) -> Option<Program> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().ok()?;
        let mut parser = Parser::new(tokens);
        let (program, errors) = parser.parse_program_recovering();
        if errors.is_empty() {
            Some(program)
        } else {
            // Return the program even with errors for partial analysis
            Some(program)
        }
    }

    fn span_contains(&self, span: &Span, line: u32, _col: u32) -> bool {
        span.line == line
    }

    fn span_to_range(&self, span: &Span) -> Range {
        Range {
            start: Position {
                line: span.line.saturating_sub(1),
                character: span.col.saturating_sub(1),
            },
            end: Position {
                line: span.line.saturating_sub(1),
                character: span.col + 10, // approximate end
            },
        }
    }

    /// Extract the identifier (word) at the given 1-based line and column.
    fn identifier_at_position(&self, text: &str, line: u32, col: u32) -> Option<String> {
        let target_line = text.lines().nth((line - 1) as usize)?;
        let col_idx = (col - 1) as usize;
        if col_idx >= target_line.len() {
            return None;
        }

        // Walk backward and forward to find word boundaries.
        let bytes = target_line.as_bytes();
        let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

        if !is_ident(bytes[col_idx]) {
            return None;
        }

        let mut start = col_idx;
        while start > 0 && is_ident(bytes[start - 1]) {
            start -= 1;
        }
        let mut end = col_idx;
        while end < bytes.len() && is_ident(bytes[end]) {
            end += 1;
        }

        Some(target_line[start..end].to_string())
    }

    fn send_response(
        &self,
        id: Option<Value>,
        result: Option<Value>,
        error: Option<JsonRpcError>,
    ) -> io::Result<()> {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: id.unwrap_or(Value::Null),
            result,
            error,
        };
        let body = serde_json::to_string(&response).unwrap();
        self.write_message(&body)
    }

    fn write_message(&self, body: &str) -> io::Result<()> {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        write!(out, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
        out.flush()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identifier_at_position() {
        let server = LspServer::new();
        let text = "let counter = 42;\nfn main() {}";

        assert_eq!(
            server.identifier_at_position(text, 1, 5),
            Some("counter".to_string())
        );
        assert_eq!(
            server.identifier_at_position(text, 2, 4),
            Some("main".to_string())
        );
        // On whitespace
        assert_eq!(server.identifier_at_position(text, 1, 4), None);
    }

    #[test]
    fn test_parse_document_valid() {
        let server = LspServer::new();
        let source = "fn main() -> i32 { return 42; }";
        let program = server.parse_document(source);
        assert!(program.is_some());
    }

    #[test]
    fn test_completion_includes_keywords() {
        let server = LspServer::new();
        let params = CompletionParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///test.nectar".to_string(),
            },
            position: Position { line: 0, character: 0 },
        };
        let items = server.on_completion(params);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"fn"));
        assert!(labels.contains(&"component"));
        assert!(labels.contains(&"signal"));
        assert!(labels.contains(&"struct"));
    }
}
