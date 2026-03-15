use crate::ast::*;
use std::collections::HashMap;

// ── WebAssembly binary format constants ──────────────────────────────────────

const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D]; // \0asm
const WASM_VERSION: [u8; 4] = [0x01, 0x00, 0x00, 0x00]; // version 1

// Section IDs
const SECTION_TYPE: u8 = 1;
const SECTION_IMPORT: u8 = 2;
const SECTION_FUNCTION: u8 = 3;
#[allow(dead_code)]
const SECTION_MEMORY: u8 = 5;
const SECTION_GLOBAL: u8 = 6;
const SECTION_EXPORT: u8 = 7;
const SECTION_CODE: u8 = 10;
const SECTION_DATA: u8 = 11;

// Value types
const VALTYPE_I32: u8 = 0x7F;
const VALTYPE_I64: u8 = 0x7E;
const VALTYPE_F32: u8 = 0x7D;
const VALTYPE_F64: u8 = 0x7C;

// Type constructors
const TYPE_FUNC: u8 = 0x60;

// Import/export kinds
const KIND_FUNC: u8 = 0x00;
const KIND_MEMORY: u8 = 0x02;
#[allow(dead_code)]
const KIND_GLOBAL: u8 = 0x03;

// Global mutability
#[allow(dead_code)]
const GLOBAL_CONST: u8 = 0x00;
const GLOBAL_MUT: u8 = 0x01;

// Limits
const LIMITS_MIN_ONLY: u8 = 0x00;

// WASM opcodes
#[allow(dead_code)]
const OP_UNREACHABLE: u8 = 0x00;
const OP_NOP: u8 = 0x01;
const OP_BLOCK: u8 = 0x02;
#[allow(dead_code)]
const OP_LOOP: u8 = 0x03;
const OP_IF: u8 = 0x04;
const OP_ELSE: u8 = 0x05;
const OP_END: u8 = 0x0B;
const OP_BR: u8 = 0x0C;
const OP_BR_IF: u8 = 0x0D;
const OP_RETURN: u8 = 0x0F;
const OP_CALL: u8 = 0x10;
const OP_DROP: u8 = 0x1A;
const OP_LOCAL_GET: u8 = 0x20;
const OP_LOCAL_SET: u8 = 0x21;
#[allow(dead_code)]
const OP_LOCAL_TEE: u8 = 0x22;
const OP_GLOBAL_GET: u8 = 0x23;
const OP_GLOBAL_SET: u8 = 0x24;
const OP_I32_LOAD: u8 = 0x28;
#[allow(dead_code)]
const OP_I32_STORE: u8 = 0x36;
const OP_I32_CONST: u8 = 0x41;
#[allow(dead_code)]
const OP_I64_CONST: u8 = 0x42;
#[allow(dead_code)]
const OP_F32_CONST: u8 = 0x43;
const OP_F64_CONST: u8 = 0x44;
const OP_I32_EQZ: u8 = 0x45;
const OP_I32_EQ: u8 = 0x46;
const OP_I32_NE: u8 = 0x47;
const OP_I32_LT_S: u8 = 0x48;
const OP_I32_GT_S: u8 = 0x4A;
const OP_I32_LE_S: u8 = 0x4C;
const OP_I32_GE_S: u8 = 0x4E;
const OP_I32_ADD: u8 = 0x6A;
const OP_I32_SUB: u8 = 0x6B;
const OP_I32_MUL: u8 = 0x6C;
const OP_I32_DIV_S: u8 = 0x6D;
const OP_I32_REM_S: u8 = 0x6F;
const OP_I32_AND: u8 = 0x71;
const OP_I32_OR: u8 = 0x72;

// Block types
const BLOCKTYPE_VOID: u8 = 0x40;
const BLOCKTYPE_I32: u8 = VALTYPE_I32;

// ── LEB128 encoding ─────────────────────────────────────────────────────────

/// Encode an unsigned integer as LEB128.
fn encode_unsigned_leb128(mut value: u64) -> Vec<u8> {
    let mut result = Vec::new();
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        result.push(byte);
        if value == 0 {
            break;
        }
    }
    result
}

/// Encode a signed integer as LEB128.
fn encode_signed_leb128(mut value: i64) -> Vec<u8> {
    let mut result = Vec::new();
    let mut more = true;
    while more {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        // Sign bit of the byte is second-highest bit
        if (value == 0 && (byte & 0x40) == 0) || (value == -1 && (byte & 0x40) != 0) {
            more = false;
        } else {
            byte |= 0x80;
        }
        result.push(byte);
    }
    result
}

// ── WASM value type mapping ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WasmValType {
    I32,
    I64,
    F32,
    F64,
}

impl WasmValType {
    fn to_byte(self) -> u8 {
        match self {
            WasmValType::I32 => VALTYPE_I32,
            WasmValType::I64 => VALTYPE_I64,
            WasmValType::F32 => VALTYPE_F32,
            WasmValType::F64 => VALTYPE_F64,
        }
    }
}

/// A function type signature: params -> results.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FuncSig {
    params: Vec<WasmValType>,
    results: Vec<WasmValType>,
}

/// Tracks a function during compilation.
struct FuncEntry {
    /// Name used for internal lookup
    name: String,
    /// Index into the type section
    type_idx: u32,
    /// Whether this is an import (no body) or a defined function
    is_import: bool,
    /// Export name, if any
    export_name: Option<String>,
}

// ── String intern table ─────────────────────────────────────────────────────

struct StringIntern {
    /// Accumulated bytes for the data segment
    data: Vec<u8>,
    /// Map from string content to (offset, length)
    map: HashMap<String, (u32, u32)>,
    /// Base offset in linear memory where strings are placed
    base_offset: u32,
}

impl StringIntern {
    fn new(base_offset: u32) -> Self {
        Self {
            data: Vec::new(),
            map: HashMap::new(),
            base_offset,
        }
    }

    /// Intern a string, returning (memory_offset, byte_length).
    fn intern(&mut self, s: &str) -> (u32, u32) {
        if let Some(&entry) = self.map.get(s) {
            return entry;
        }
        let offset = self.base_offset + self.data.len() as u32;
        let len = s.len() as u32;
        self.data.extend_from_slice(s.as_bytes());
        self.map.insert(s.to_string(), (offset, len));
        (offset, len)
    }
}

// ── WasmBinaryEmitter ───────────────────────────────────────────────────────

/// Emits binary .wasm format directly from a Nectar AST, with no WAT text
/// intermediate. Implements the WebAssembly 1.0 binary spec.
pub struct WasmBinaryEmitter {
    /// De-duplicated function type signatures
    types: Vec<FuncSig>,
    /// All functions (imports first, then defined)
    functions: Vec<FuncEntry>,
    /// Number of imported functions (these come first in the function index space)
    num_imports: u32,
    /// String intern table for the data section
    strings: StringIntern,
    /// Name-to-function-index mapping
    func_index: HashMap<String, u32>,
    /// Index of the global heap_ptr
    heap_ptr_global_idx: u32,
}

impl WasmBinaryEmitter {
    pub fn new() -> Self {
        Self {
            types: Vec::new(),
            functions: Vec::new(),
            num_imports: 0,
            strings: StringIntern::new(1024), // strings start at offset 1024
            func_index: HashMap::new(),
            heap_ptr_global_idx: 0,
        }
    }

    /// Emit a complete .wasm binary from the given program.
    pub fn emit(&mut self, program: &Program) -> Vec<u8> {
        // Phase 1: register all imports and collect function signatures
        self.register_imports();
        self.register_program_functions(program);

        // Phase 2: build the binary
        let mut out = Vec::new();

        // Module header
        out.extend_from_slice(&WASM_MAGIC);
        out.extend_from_slice(&WASM_VERSION);

        // Type section
        out.extend_from_slice(&self.encode_type_section());

        // Import section
        out.extend_from_slice(&self.encode_import_section());

        // Function section (type indices for defined functions only)
        out.extend_from_slice(&self.encode_function_section());

        // Memory section — not emitted because we import memory from env

        // Global section
        out.extend_from_slice(&self.encode_global_section());

        // Export section
        out.extend_from_slice(&self.encode_export_section());

        // Code section
        out.extend_from_slice(&self.encode_code_section(program));

        // Data section
        out.extend_from_slice(&self.encode_data_section());

        out
    }

    // ── Phase 1: registration ────────────────────────────────────────────

    fn register_imports(&mut self) {
        // env.memory is an import but not a function — handled separately in
        // the import section encoder.

        // DOM imports matching the WAT codegen
        self.register_import_func(
            "dom", "createElement",
            FuncSig { params: vec![WasmValType::I32, WasmValType::I32], results: vec![WasmValType::I32] },
        );
        self.register_import_func(
            "dom", "setText",
            FuncSig { params: vec![WasmValType::I32, WasmValType::I32, WasmValType::I32], results: vec![] },
        );
        self.register_import_func(
            "dom", "appendChild",
            FuncSig { params: vec![WasmValType::I32, WasmValType::I32], results: vec![] },
        );
        self.register_import_func(
            "dom", "addEventListener",
            FuncSig {
                params: vec![WasmValType::I32, WasmValType::I32, WasmValType::I32, WasmValType::I32],
                results: vec![],
            },
        );
        self.register_import_func(
            "dom", "setAttribute",
            FuncSig {
                params: vec![WasmValType::I32, WasmValType::I32, WasmValType::I32, WasmValType::I32],
                results: vec![],
            },
        );

        // Test runtime imports
        self.register_import_func(
            "test", "test_pass",
            FuncSig { params: vec![WasmValType::I32, WasmValType::I32], results: vec![] },
        );
        self.register_import_func(
            "test", "test_fail",
            FuncSig {
                params: vec![WasmValType::I32, WasmValType::I32, WasmValType::I32, WasmValType::I32],
                results: vec![],
            },
        );
        self.register_import_func(
            "test", "test_summary",
            FuncSig { params: vec![WasmValType::I32, WasmValType::I32], results: vec![] },
        );

        self.num_imports = self.functions.len() as u32;

        // Built-in alloc function
        let alloc_sig = FuncSig {
            params: vec![WasmValType::I32],
            results: vec![WasmValType::I32],
        };
        let type_idx = self.intern_type(alloc_sig);
        let idx = self.functions.len() as u32;
        self.functions.push(FuncEntry {
            name: "alloc".into(),
            type_idx,
            is_import: false,
            export_name: None,
        });
        self.func_index.insert("alloc".into(), idx);
    }

    fn register_import_func(&mut self, _module: &str, name: &str, sig: FuncSig) {
        let type_idx = self.intern_type(sig);
        let idx = self.functions.len() as u32;
        self.functions.push(FuncEntry {
            name: name.into(),
            type_idx,
            is_import: true,
            export_name: None,
        });
        self.func_index.insert(name.into(), idx);
    }

    fn register_program_functions(&mut self, program: &Program) {
        let mut has_tests = false;
        for item in &program.items {
            match item {
                Item::Function(f) => {
                    self.register_function(f);
                }
                Item::Test(test) => {
                    has_tests = true;
                    let safe_name = test.name.replace(' ', "_").replace('"', "");
                    let func_name = format!("__test_{}", safe_name);
                    let sig = FuncSig { params: vec![], results: vec![] };
                    let type_idx = self.intern_type(sig);
                    let idx = self.functions.len() as u32;
                    self.functions.push(FuncEntry {
                        name: func_name.clone(),
                        type_idx,
                        is_import: false,
                        export_name: Some(func_name.clone()),
                    });
                    self.func_index.insert(func_name, idx);
                }
                _ => {}
            }
        }
        // Register __run_tests if there are test blocks
        if has_tests {
            let sig = FuncSig { params: vec![], results: vec![] };
            let type_idx = self.intern_type(sig);
            let idx = self.functions.len() as u32;
            self.functions.push(FuncEntry {
                name: "__run_tests".into(),
                type_idx,
                is_import: false,
                export_name: Some("__run_tests".into()),
            });
            self.func_index.insert("__run_tests".into(), idx);
        }
    }

    fn register_function(&mut self, func: &Function) {
        let params: Vec<WasmValType> = func.params.iter()
            .filter(|p| p.name != "self")
            .map(|p| ast_type_to_valtype(&p.ty))
            .collect();
        let results = func.return_type.as_ref()
            .map(|t| vec![ast_type_to_valtype(t)])
            .unwrap_or_default();
        let sig = FuncSig { params, results };
        let type_idx = self.intern_type(sig);
        let idx = self.functions.len() as u32;
        let export_name = if func.is_pub { Some(func.name.clone()) } else { None };
        self.functions.push(FuncEntry {
            name: func.name.clone(),
            type_idx,
            is_import: false,
            export_name,
        });
        self.func_index.insert(func.name.clone(), idx);
    }

    /// Deduplicate a function type, returning its index.
    fn intern_type(&mut self, sig: FuncSig) -> u32 {
        if let Some(pos) = self.types.iter().position(|s| s == &sig) {
            return pos as u32;
        }
        let idx = self.types.len() as u32;
        self.types.push(sig);
        idx
    }

    // ── Section encoders ─────────────────────────────────────────────────

    fn encode_type_section(&self) -> Vec<u8> {
        let mut content = Vec::new();
        // Number of types
        content.extend_from_slice(&encode_unsigned_leb128(self.types.len() as u64));
        for sig in &self.types {
            content.push(TYPE_FUNC);
            // Params vector
            content.extend_from_slice(&encode_unsigned_leb128(sig.params.len() as u64));
            for p in &sig.params {
                content.push(p.to_byte());
            }
            // Results vector
            content.extend_from_slice(&encode_unsigned_leb128(sig.results.len() as u64));
            for r in &sig.results {
                content.push(r.to_byte());
            }
        }
        encode_section(SECTION_TYPE, &content)
    }

    fn encode_import_section(&self) -> Vec<u8> {
        // Import entries: env.memory + the imported functions
        // Map import function names to their module names
        let import_module_map: HashMap<&str, &str> = [
            ("createElement", "dom"),
            ("setText", "dom"),
            ("appendChild", "dom"),
            ("addEventListener", "dom"),
            ("setAttribute", "dom"),
            ("test_pass", "test"),
            ("test_fail", "test"),
            ("test_summary", "test"),
        ].iter().copied().collect();

        let num_imports = 1 + self.num_imports as usize; // +1 for memory
        let mut content = Vec::new();
        content.extend_from_slice(&encode_unsigned_leb128(num_imports as u64));

        // env.memory
        encode_name(&mut content, "env");
        encode_name(&mut content, "memory");
        content.push(KIND_MEMORY);
        content.push(LIMITS_MIN_ONLY);
        content.extend_from_slice(&encode_unsigned_leb128(1)); // min 1 page

        // Function imports
        for func in self.functions.iter() {
            if !func.is_import {
                break;
            }
            let module = import_module_map.get(func.name.as_str()).unwrap_or(&"env");
            encode_name(&mut content, module);
            encode_name(&mut content, &func.name);
            content.push(KIND_FUNC);
            content.extend_from_slice(&encode_unsigned_leb128(func.type_idx as u64));
        }

        encode_section(SECTION_IMPORT, &content)
    }

    fn encode_function_section(&self) -> Vec<u8> {
        // Lists type indices for all defined (non-import) functions
        let defined: Vec<&FuncEntry> = self.functions.iter()
            .filter(|f| !f.is_import)
            .collect();

        let mut content = Vec::new();
        content.extend_from_slice(&encode_unsigned_leb128(defined.len() as u64));
        for f in &defined {
            content.extend_from_slice(&encode_unsigned_leb128(f.type_idx as u64));
        }
        encode_section(SECTION_FUNCTION, &content)
    }

    fn encode_global_section(&self) -> Vec<u8> {
        // heap_ptr: mutable i32, initialized to 1024
        let mut content = Vec::new();
        content.extend_from_slice(&encode_unsigned_leb128(1)); // 1 global
        content.push(VALTYPE_I32);
        content.push(GLOBAL_MUT);
        // Init expr: i32.const 1024, end
        content.push(OP_I32_CONST);
        content.extend_from_slice(&encode_signed_leb128(1024));
        content.push(OP_END);

        encode_section(SECTION_GLOBAL, &content)
    }

    fn encode_export_section(&self) -> Vec<u8> {
        let exports: Vec<&FuncEntry> = self.functions.iter()
            .filter(|f| f.export_name.is_some())
            .collect();

        let mut content = Vec::new();
        content.extend_from_slice(&encode_unsigned_leb128(exports.len() as u64));
        for f in &exports {
            let name = f.export_name.as_ref().unwrap();
            encode_name(&mut content, name);
            content.push(KIND_FUNC);
            // Find the absolute index of this function
            let idx = self.func_index[&f.name];
            content.extend_from_slice(&encode_unsigned_leb128(idx as u64));
        }
        encode_section(SECTION_EXPORT, &content)
    }

    fn encode_code_section(&mut self, program: &Program) -> Vec<u8> {
        // Collect test definitions for the run_tests body
        let test_names: Vec<String> = program.items.iter().filter_map(|item| {
            if let Item::Test(test) = item {
                Some(test.name.replace(' ', "_").replace('"', ""))
            } else {
                None
            }
        }).collect();

        // Collect bodies for all defined functions in order
        let defined_names: Vec<String> = self.functions.iter()
            .filter(|f| !f.is_import)
            .map(|f| f.name.clone())
            .collect();

        let mut bodies: Vec<Vec<u8>> = Vec::new();

        for name in &defined_names {
            if name == "alloc" {
                bodies.push(self.encode_alloc_body());
                continue;
            }
            if name == "__run_tests" {
                bodies.push(self.encode_run_tests_body(&test_names));
                continue;
            }
            // Check for test functions
            let test_def = program.items.iter().find_map(|item| {
                if let Item::Test(test) = item {
                    let safe = test.name.replace(' ', "_").replace('"', "");
                    if format!("__test_{}", safe) == *name {
                        return Some(test);
                    }
                }
                None
            });
            if let Some(test) = test_def {
                bodies.push(self.encode_test_body(test));
                continue;
            }
            // Find the AST function
            let func = program.items.iter().find_map(|item| {
                if let Item::Function(f) = item {
                    if &f.name == name { return Some(f); }
                }
                None
            });
            if let Some(f) = func {
                bodies.push(self.encode_function_body(f));
            }
        }

        let mut content = Vec::new();
        content.extend_from_slice(&encode_unsigned_leb128(bodies.len() as u64));
        for body in &bodies {
            // Each function body is length-prefixed
            content.extend_from_slice(&encode_unsigned_leb128(body.len() as u64));
            content.extend_from_slice(body);
        }
        encode_section(SECTION_CODE, &content)
    }

    fn encode_test_body(&mut self, test: &TestDef) -> Vec<u8> {
        let mut extra_locals: Vec<(String, WasmValType)> = Vec::new();
        collect_block_locals(&test.body, &mut extra_locals);

        let mut local_map: HashMap<String, u32> = HashMap::new();
        for (i, (name, _)) in extra_locals.iter().enumerate() {
            local_map.insert(name.clone(), i as u32);
        }

        let mut body = Vec::new();

        let local_groups = group_locals(&extra_locals);
        body.extend_from_slice(&encode_unsigned_leb128(local_groups.len() as u64));
        for (count, ty) in &local_groups {
            body.extend_from_slice(&encode_unsigned_leb128(*count as u64));
            body.push(ty.to_byte());
        }

        let mut ctx = CodegenCtx {
            local_map: &local_map,
            func_index: &self.func_index,
            strings: &mut self.strings,
            heap_ptr_global_idx: self.heap_ptr_global_idx,
        };
        for stmt in &test.body.stmts {
            encode_stmt(&mut body, stmt, &mut ctx);
        }

        // If we get here, the test passed -- call test_pass
        let (name_offset, name_len) = ctx.strings.intern(&test.name);
        body.push(OP_I32_CONST);
        body.extend_from_slice(&encode_signed_leb128(name_offset as i64));
        body.push(OP_I32_CONST);
        body.extend_from_slice(&encode_signed_leb128(name_len as i64));
        if let Some(&idx) = ctx.func_index.get("test_pass") {
            body.push(OP_CALL);
            body.extend_from_slice(&encode_unsigned_leb128(idx as u64));
        }

        body.push(OP_END);
        body
    }

    fn encode_run_tests_body(&mut self, test_names: &[String]) -> Vec<u8> {
        let mut body = Vec::new();

        // No locals needed
        body.extend_from_slice(&encode_unsigned_leb128(0u64));

        // Call each test function
        for name in test_names {
            let func_name = format!("__test_{}", name);
            if let Some(&idx) = self.func_index.get(&func_name) {
                body.push(OP_CALL);
                body.extend_from_slice(&encode_unsigned_leb128(idx as u64));
            }
        }

        // Call test_summary with total count
        let total = test_names.len() as i64;
        body.push(OP_I32_CONST);
        body.extend_from_slice(&encode_signed_leb128(total));
        body.push(OP_I32_CONST);
        body.extend_from_slice(&encode_signed_leb128(0)); // failed placeholder
        if let Some(&idx) = self.func_index.get("test_summary") {
            body.push(OP_CALL);
            body.extend_from_slice(&encode_unsigned_leb128(idx as u64));
        }

        body.push(OP_END);
        body
    }

    fn encode_data_section(&self) -> Vec<u8> {
        if self.strings.data.is_empty() {
            // Emit an empty data section
            let mut content = Vec::new();
            content.extend_from_slice(&encode_unsigned_leb128(0));
            return encode_section(SECTION_DATA, &content);
        }

        let mut content = Vec::new();
        // One data segment
        content.extend_from_slice(&encode_unsigned_leb128(1));

        // Memory index 0 (active segment)
        content.extend_from_slice(&encode_unsigned_leb128(0));

        // Offset expression: i32.const <base_offset>, end
        content.push(OP_I32_CONST);
        content.extend_from_slice(&encode_signed_leb128(self.strings.base_offset as i64));
        content.push(OP_END);

        // Data bytes
        content.extend_from_slice(&encode_unsigned_leb128(self.strings.data.len() as u64));
        content.extend_from_slice(&self.strings.data);

        encode_section(SECTION_DATA, &content)
    }

    // ── Function body encoding ───────────────────────────────────────────

    fn encode_alloc_body(&self) -> Vec<u8> {
        let mut body = Vec::new();

        // One local declaration group: 1 local of type i32 ($ptr)
        body.extend_from_slice(&encode_unsigned_leb128(1)); // 1 group
        body.extend_from_slice(&encode_unsigned_leb128(1)); // count 1
        body.push(VALTYPE_I32);

        // local 0 = $size (param), local 1 = $ptr
        let local_size: u32 = 0;
        let local_ptr: u32 = 1;

        // $ptr = heap_ptr
        body.push(OP_GLOBAL_GET);
        body.extend_from_slice(&encode_unsigned_leb128(self.heap_ptr_global_idx as u64));
        body.push(OP_LOCAL_SET);
        body.extend_from_slice(&encode_unsigned_leb128(local_ptr as u64));

        // heap_ptr = heap_ptr + $size
        body.push(OP_GLOBAL_GET);
        body.extend_from_slice(&encode_unsigned_leb128(self.heap_ptr_global_idx as u64));
        body.push(OP_LOCAL_GET);
        body.extend_from_slice(&encode_unsigned_leb128(local_size as u64));
        body.push(OP_I32_ADD);
        body.push(OP_GLOBAL_SET);
        body.extend_from_slice(&encode_unsigned_leb128(self.heap_ptr_global_idx as u64));

        // return $ptr
        body.push(OP_LOCAL_GET);
        body.extend_from_slice(&encode_unsigned_leb128(local_ptr as u64));

        body.push(OP_END);
        body
    }

    fn encode_function_body(&mut self, func: &Function) -> Vec<u8> {
        // Collect locals from the function body (params are implicit in the
        // WASM calling convention; only additional locals are declared here).
        let params: Vec<(&str, WasmValType)> = func.params.iter()
            .filter(|p| p.name != "self")
            .map(|p| (p.name.as_str(), ast_type_to_valtype(&p.ty)))
            .collect();

        let mut extra_locals: Vec<(String, WasmValType)> = Vec::new();
        collect_block_locals(&func.body, &mut extra_locals);

        // Build local-name-to-index map.  Params come first.
        let mut local_map: HashMap<String, u32> = HashMap::new();
        for (i, (name, _)) in params.iter().enumerate() {
            local_map.insert(name.to_string(), i as u32);
        }
        for (i, (name, _)) in extra_locals.iter().enumerate() {
            local_map.insert(name.clone(), (params.len() + i) as u32);
        }

        let mut body = Vec::new();

        // Local declarations: group consecutive identical types
        let local_groups = group_locals(&extra_locals);
        body.extend_from_slice(&encode_unsigned_leb128(local_groups.len() as u64));
        for (count, ty) in &local_groups {
            body.extend_from_slice(&encode_unsigned_leb128(*count as u64));
            body.push(ty.to_byte());
        }

        // Encode statements
        let mut ctx = CodegenCtx {
            local_map: &local_map,
            func_index: &self.func_index,
            strings: &mut self.strings,
            heap_ptr_global_idx: self.heap_ptr_global_idx,
        };
        for stmt in &func.body.stmts {
            encode_stmt(&mut body, stmt, &mut ctx);
        }

        body.push(OP_END);
        body
    }
}

// ── Codegen context passed during instruction emission ───────────────────────

struct CodegenCtx<'a> {
    local_map: &'a HashMap<String, u32>,
    func_index: &'a HashMap<String, u32>,
    strings: &'a mut StringIntern,
    #[allow(dead_code)]
    heap_ptr_global_idx: u32,
}

// ── Instruction encoding ─────────────────────────────────────────────────────

fn encode_stmt(out: &mut Vec<u8>, stmt: &Stmt, ctx: &mut CodegenCtx) {
    match stmt {
        Stmt::Let { name, value, .. } => {
            encode_expr(out, value, ctx);
            if let Some(&idx) = ctx.local_map.get(name.as_str()) {
                out.push(OP_LOCAL_SET);
                out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
            }
        }
        Stmt::Signal { name, value, .. } => {
            encode_expr(out, value, ctx);
            if let Some(&idx) = ctx.local_map.get(name.as_str()) {
                out.push(OP_LOCAL_SET);
                out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
            }
        }
        Stmt::Return(Some(expr)) => {
            encode_expr(out, expr, ctx);
            out.push(OP_RETURN);
        }
        Stmt::Return(None) => {
            out.push(OP_RETURN);
        }
        Stmt::Expr(expr) => {
            encode_expr(out, expr, ctx);
            out.push(OP_DROP);
        }
        Stmt::Yield(expr) => {
            encode_expr(out, expr, ctx);
            // Yield value is on stack; runtime handles stream delivery
        }
        Stmt::LetDestructure { pattern, value, .. } => {
            encode_expr(out, value, ctx);
            // Value is on the stack; store in temp local then extract fields
            encode_destructure_pattern(out, pattern, ctx);
        }
    }
}

fn encode_destructure_pattern(out: &mut Vec<u8>, pattern: &Pattern, ctx: &mut CodegenCtx) {
    match pattern {
        Pattern::Ident(name) => {
            if let Some(&idx) = ctx.local_map.get(name.as_str()) {
                out.push(OP_LOCAL_SET);
                out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
            }
        }
        Pattern::Tuple(pats) | Pattern::Array(pats) => {
            // Value ptr is on stack. For each element, load from offset.
            for (i, p) in pats.iter().enumerate() {
                if matches!(p, Pattern::Wildcard) { continue; }
                // Duplicate the base pointer (using local.tee would require a temp)
                out.push(OP_LOCAL_GET);
                out.extend_from_slice(&encode_unsigned_leb128(0)); // temp local 0
                out.push(OP_I32_CONST);
                out.extend_from_slice(&encode_signed_leb128((i as i64) * 4));
                out.push(OP_I32_ADD);
                out.push(OP_I32_LOAD);
                out.push(0x02); // alignment
                out.push(0x00); // offset
                encode_destructure_pattern(out, p, ctx);
            }
        }
        Pattern::Struct { fields, .. } => {
            for (i, (_name, p)) in fields.iter().enumerate() {
                out.push(OP_LOCAL_GET);
                out.extend_from_slice(&encode_unsigned_leb128(0));
                out.push(OP_I32_CONST);
                out.extend_from_slice(&encode_signed_leb128((i as i64) * 4));
                out.push(OP_I32_ADD);
                out.push(OP_I32_LOAD);
                out.push(0x02);
                out.push(0x00);
                encode_destructure_pattern(out, p, ctx);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) | Pattern::Variant { .. } => {}
    }
}

fn encode_expr(out: &mut Vec<u8>, expr: &Expr, ctx: &mut CodegenCtx) {
    match expr {
        Expr::Integer(n) => {
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(*n));
        }
        Expr::Float(f) => {
            out.push(OP_F64_CONST);
            out.extend_from_slice(&f.to_le_bytes());
        }
        Expr::Bool(b) => {
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(if *b { 1 } else { 0 }));
        }
        Expr::StringLit(s) => {
            let (offset, len) = ctx.strings.intern(s);
            // Push ptr
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(offset as i64));
            // Push len
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(len as i64));
        }
        Expr::Ident(name) => {
            if let Some(&idx) = ctx.local_map.get(name.as_str()) {
                out.push(OP_LOCAL_GET);
                out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
            }
        }
        Expr::SelfExpr => {
            if let Some(&idx) = ctx.local_map.get("self") {
                out.push(OP_LOCAL_GET);
                out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
            }
        }
        Expr::Binary { op, left, right } => {
            encode_expr(out, left, ctx);
            encode_expr(out, right, ctx);
            let opcode = match op {
                BinOp::Add => OP_I32_ADD,
                BinOp::Sub => OP_I32_SUB,
                BinOp::Mul => OP_I32_MUL,
                BinOp::Div => OP_I32_DIV_S,
                BinOp::Mod => OP_I32_REM_S,
                BinOp::Eq => OP_I32_EQ,
                BinOp::Neq => OP_I32_NE,
                BinOp::Lt => OP_I32_LT_S,
                BinOp::Gt => OP_I32_GT_S,
                BinOp::Lte => OP_I32_LE_S,
                BinOp::Gte => OP_I32_GE_S,
                BinOp::And => OP_I32_AND,
                BinOp::Or => OP_I32_OR,
            };
            out.push(opcode);
        }
        Expr::Unary { op, operand } => {
            match op {
                UnaryOp::Neg => {
                    out.push(OP_I32_CONST);
                    out.extend_from_slice(&encode_signed_leb128(0));
                    encode_expr(out, operand, ctx);
                    out.push(OP_I32_SUB);
                }
                UnaryOp::Not => {
                    encode_expr(out, operand, ctx);
                    out.push(OP_I32_EQZ);
                }
            }
        }
        Expr::FnCall { callee, args } => {
            for arg in args {
                encode_expr(out, arg, ctx);
            }
            if let Expr::Ident(name) = callee.as_ref() {
                if let Some(&idx) = ctx.func_index.get(name.as_str()) {
                    out.push(OP_CALL);
                    out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
                }
            }
        }
        Expr::MethodCall { object, method, args } => {
            encode_expr(out, object, ctx);
            for arg in args {
                encode_expr(out, arg, ctx);
            }
            if let Some(&idx) = ctx.func_index.get(method.as_str()) {
                out.push(OP_CALL);
                out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
            }
        }
        Expr::FieldAccess { object, .. } => {
            encode_expr(out, object, ctx);
            // Load i32 from the address on the stack (simplified: offset 0, align 2)
            out.push(OP_I32_LOAD);
            out.extend_from_slice(&encode_unsigned_leb128(2)); // alignment
            out.extend_from_slice(&encode_unsigned_leb128(0)); // offset
        }
        Expr::If { condition, then_block, else_block } => {
            encode_expr(out, condition, ctx);
            out.push(OP_IF);
            out.push(BLOCKTYPE_I32);

            for stmt in &then_block.stmts {
                encode_stmt(out, stmt, ctx);
            }

            if let Some(else_blk) = else_block {
                out.push(OP_ELSE);
                for stmt in &else_blk.stmts {
                    encode_stmt(out, stmt, ctx);
                }
            }

            out.push(OP_END);
        }
        Expr::Assign { target, value } => {
            encode_expr(out, value, ctx);
            if let Expr::Ident(name) = target.as_ref() {
                if let Some(&idx) = ctx.local_map.get(name.as_str()) {
                    out.push(OP_LOCAL_SET);
                    out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
                }
            }
        }
        Expr::Block(block) => {
            for stmt in &block.stmts {
                encode_stmt(out, stmt, ctx);
            }
        }
        Expr::Index { object, index } => {
            // Simplified: compute base + index * 4, then load
            encode_expr(out, object, ctx);
            encode_expr(out, index, ctx);
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(4));
            out.push(OP_I32_MUL);
            out.push(OP_I32_ADD);
            out.push(OP_I32_LOAD);
            out.extend_from_slice(&encode_unsigned_leb128(2)); // alignment
            out.extend_from_slice(&encode_unsigned_leb128(0)); // offset
        }
        // Ownership markers pass through
        Expr::Borrow(inner) | Expr::BorrowMut(inner) => {
            encode_expr(out, inner, ctx);
        }
        Expr::Assert { condition, message } => {
            // Evaluate condition; if false, call $test_fail
            encode_expr(out, condition, ctx);
            out.push(OP_IF);
            out.push(BLOCKTYPE_VOID);
            // then: nothing (pass)
            out.push(OP_ELSE);
            // else: call test.fail with message
            let msg = message.as_deref().unwrap_or("assertion failed");
            let (msg_offset, msg_len) = ctx.strings.intern(msg);
            // name ptr, name len (0,0 = contextual)
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(0));
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(0));
            // msg ptr, msg len
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(msg_offset as i64));
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(msg_len as i64));
            if let Some(&idx) = ctx.func_index.get("test_fail") {
                out.push(OP_CALL);
                out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
            }
            out.push(OP_END);
        }
        Expr::AssertEq { left, right, message } => {
            // Evaluate both sides, compare with i32.eq
            encode_expr(out, left, ctx);
            encode_expr(out, right, ctx);
            out.push(OP_I32_EQ);
            out.push(OP_IF);
            out.push(BLOCKTYPE_VOID);
            // then: nothing (pass)
            out.push(OP_ELSE);
            let msg = message.as_deref().unwrap_or("assert_eq failed: values not equal");
            let (msg_offset, msg_len) = ctx.strings.intern(msg);
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(0));
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(0));
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(msg_offset as i64));
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(msg_len as i64));
            if let Some(&idx) = ctx.func_index.get("test_fail") {
                out.push(OP_CALL);
                out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
            }
            out.push(OP_END);
        }
        Expr::TryCatch { body, error_binding: _, catch_body } => {
            // Implement as a block-based error code pattern:
            // block $ok { block $err { <body> br $ok } <catch_body> }
            // Try body
            out.push(OP_BLOCK);
            out.push(BLOCKTYPE_VOID);
            out.push(OP_BLOCK);
            out.push(BLOCKTYPE_VOID);
            encode_expr(out, body, ctx);
            out.push(OP_BR); // br $ok (index 1 = outer block)
            out.extend_from_slice(&encode_unsigned_leb128(1));
            out.push(OP_END); // end inner block ($err)
            // Catch body — reached if inner block falls through on error
            encode_expr(out, catch_body, ctx);
            out.push(OP_END); // end outer block ($ok)
        }
        Expr::FormatString { parts } => {
            // Encode format string: push each part as (ptr, len), then
            // call $string_concat to fold pairs left-to-right.
            // Assumes import index for string_concat/to_string are registered
            // in ctx.func_index.
            let mut first = true;
            for part in parts {
                match part {
                    FormatPart::Literal(s) => {
                        let (offset, len) = ctx.strings.intern(s);
                        out.push(OP_I32_CONST);
                        out.extend_from_slice(&encode_signed_leb128(offset as i64));
                        out.push(OP_I32_CONST);
                        out.extend_from_slice(&encode_signed_leb128(len as i64));
                    }
                    FormatPart::Expression(expr) => {
                        encode_expr(out, expr, ctx);
                        // Call $to_string to convert value to (ptr, len).
                        if let Some(&idx) = ctx.func_index.get("to_string") {
                            out.push(OP_CALL);
                            out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
                        }
                    }
                }
                if !first {
                    // Call $string_concat(ptr1, len1, ptr2, len2) -> (ptr, len)
                    if let Some(&idx) = ctx.func_index.get("string_concat") {
                        out.push(OP_CALL);
                        out.extend_from_slice(&encode_unsigned_leb128(idx as u64));
                    }
                }
                first = false;
            }
            // If there were zero parts, push empty string.
            if first {
                let (offset, len) = ctx.strings.intern("");
                out.push(OP_I32_CONST);
                out.extend_from_slice(&encode_signed_leb128(offset as i64));
                out.push(OP_I32_CONST);
                out.extend_from_slice(&encode_signed_leb128(len as i64));
            }
        }
        Expr::Try(inner) => {
            // ? operator: check discriminant, early return on error
            encode_expr(out, inner, ctx);
            // Block-based pattern: if discriminant != 0, return
            out.push(OP_BLOCK);
            out.push(BLOCKTYPE_VOID);
            out.push(OP_BLOCK);
            out.push(BLOCKTYPE_VOID);
            // Check discriminant (offset 0 of the Result/Option)
            out.push(OP_I32_LOAD);
            out.push(0x02); // alignment
            out.push(0x00); // offset
            out.push(OP_BR_IF);
            out.extend_from_slice(&encode_unsigned_leb128(0)); // branch to error block
            // Ok path: load value at offset 4
            out.push(OP_I32_CONST);
            out.extend_from_slice(&encode_signed_leb128(4));
            out.push(OP_I32_ADD);
            out.push(OP_I32_LOAD);
            out.push(0x02);
            out.push(0x00);
            out.push(OP_BR);
            out.extend_from_slice(&encode_unsigned_leb128(1)); // skip error block
            out.push(OP_END); // end inner block
            // Error path: return early
            out.push(OP_RETURN);
            out.push(OP_END); // end outer block
        }
        _ => {
            // Unsupported expressions emit nop for now
            out.push(OP_NOP);
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn collect_block_locals(block: &Block, out: &mut Vec<(String, WasmValType)>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let { name, ty, .. } => {
                let vt = ty.as_ref()
                    .map(|t| ast_type_to_valtype(t))
                    .unwrap_or(WasmValType::I32);
                out.push((name.clone(), vt));
            }
            Stmt::Signal { name, ty, .. } => {
                let vt = ty.as_ref()
                    .map(|t| ast_type_to_valtype(t))
                    .unwrap_or(WasmValType::I32);
                out.push((name.clone(), vt));
            }
            Stmt::LetDestructure { pattern, .. } => {
                collect_pattern_locals_wasm(pattern, out);
            }
            _ => {}
        }
    }
}

fn collect_pattern_locals_wasm(pattern: &Pattern, out: &mut Vec<(String, WasmValType)>) {
    match pattern {
        Pattern::Ident(name) => {
            out.push((name.clone(), WasmValType::I32));
        }
        Pattern::Tuple(pats) | Pattern::Array(pats) => {
            for p in pats {
                collect_pattern_locals_wasm(p, out);
            }
        }
        Pattern::Struct { fields, .. } => {
            for (_name, p) in fields {
                collect_pattern_locals_wasm(p, out);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) | Pattern::Variant { .. } => {}
    }
}

fn ast_type_to_valtype(ty: &Type) -> WasmValType {
    match ty {
        Type::Named(name) => match name.as_str() {
            "i64" | "u64" => WasmValType::I64,
            "f32" => WasmValType::F32,
            "f64" => WasmValType::F64,
            _ => WasmValType::I32,
        },
        // Generic types are erased to i32 (pointer to heap-allocated data).
        // Monomorphization can be added in a future compilation pass.
        Type::Generic { .. } => WasmValType::I32,
        _ => WasmValType::I32,
    }
}

/// Group consecutive locals of the same type: [(count, type), ...]
fn group_locals(locals: &[(String, WasmValType)]) -> Vec<(u32, WasmValType)> {
    let mut groups: Vec<(u32, WasmValType)> = Vec::new();
    for (_, ty) in locals {
        if let Some(last) = groups.last_mut() {
            if last.1 == *ty {
                last.0 += 1;
                continue;
            }
        }
        groups.push((1, *ty));
    }
    groups
}

/// Encode a UTF-8 name as a length-prefixed byte vector.
fn encode_name(out: &mut Vec<u8>, name: &str) {
    out.extend_from_slice(&encode_unsigned_leb128(name.len() as u64));
    out.extend_from_slice(name.as_bytes());
}

/// Wrap section content with its id byte and LEB128 size prefix.
fn encode_section(id: u8, content: &[u8]) -> Vec<u8> {
    let mut section = Vec::new();
    section.push(id);
    section.extend_from_slice(&encode_unsigned_leb128(content.len() as u64));
    section.extend_from_slice(content);
    section
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Span;

    fn empty_span() -> Span {
        Span { start: 0, end: 0, line: 1, col: 1 }
    }

    fn make_program(items: Vec<Item>) -> Program {
        Program { items }
    }

    fn make_add_function() -> Function {
        Function {
            name: "add".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![
                Param { name: "a".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false },
                Param { name: "b".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false },
            ],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: Block {
                stmts: vec![
                    Stmt::Return(Some(Expr::Binary {
                        op: BinOp::Add,
                        left: Box::new(Expr::Ident("a".into())),
                        right: Box::new(Expr::Ident("b".into())),
                    })),
                ],
                span: empty_span(),
            },
            is_pub: true,
            is_async: false,
            must_use: false,
            span: empty_span(),
        }
    }

    // ── LEB128 tests ─────────────────────────────────────────────────────

    #[test]
    fn test_unsigned_leb128_zero() {
        assert_eq!(encode_unsigned_leb128(0), vec![0x00]);
    }

    #[test]
    fn test_unsigned_leb128_small() {
        assert_eq!(encode_unsigned_leb128(1), vec![0x01]);
        assert_eq!(encode_unsigned_leb128(127), vec![0x7F]);
    }

    #[test]
    fn test_unsigned_leb128_multi_byte() {
        // 128 = 0x80 -> LEB128: [0x80, 0x01]
        assert_eq!(encode_unsigned_leb128(128), vec![0x80, 0x01]);
        // 624485 -> LEB128: [0xE5, 0x8E, 0x26]
        assert_eq!(encode_unsigned_leb128(624485), vec![0xE5, 0x8E, 0x26]);
    }

    #[test]
    fn test_signed_leb128_positive() {
        assert_eq!(encode_signed_leb128(0), vec![0x00]);
        assert_eq!(encode_signed_leb128(1), vec![0x01]);
        assert_eq!(encode_signed_leb128(63), vec![0x3F]);
        // 64 needs two bytes because bit 6 is set (would look negative in one byte)
        assert_eq!(encode_signed_leb128(64), vec![0xC0, 0x00]);
    }

    #[test]
    fn test_signed_leb128_negative() {
        assert_eq!(encode_signed_leb128(-1), vec![0x7F]);
        assert_eq!(encode_signed_leb128(-64), vec![0x40]);
        assert_eq!(encode_signed_leb128(-65), vec![0xBF, 0x7F]);
    }

    // ── Module header test ───────────────────────────────────────────────

    #[test]
    fn test_magic_number_and_version() {
        let program = make_program(vec![]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        // Magic number: \0asm
        assert_eq!(&bytes[0..4], &[0x00, 0x61, 0x73, 0x6D]);
        // Version 1
        assert_eq!(&bytes[4..8], &[0x01, 0x00, 0x00, 0x00]);
    }

    // ── Section structure test ───────────────────────────────────────────

    #[test]
    fn test_section_structure_with_function() {
        let func = make_add_function();
        let program = make_program(vec![Item::Function(func)]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        // Verify header
        assert_eq!(&bytes[0..4], &WASM_MAGIC);
        assert_eq!(&bytes[4..8], &WASM_VERSION);

        // After header, sections follow. Each starts with a section ID byte.
        // Walk through and collect section IDs.
        let mut pos = 8;
        let mut section_ids = Vec::new();
        while pos < bytes.len() {
            let id = bytes[pos];
            section_ids.push(id);
            pos += 1;
            // Read section size (LEB128)
            let (size, bytes_read) = read_unsigned_leb128(&bytes[pos..]);
            pos += bytes_read;
            pos += size as usize;
        }

        // We expect: type(1), import(2), function(3), global(6), export(7),
        // code(10), data(11)
        assert!(section_ids.contains(&SECTION_TYPE));
        assert!(section_ids.contains(&SECTION_IMPORT));
        assert!(section_ids.contains(&SECTION_FUNCTION));
        assert!(section_ids.contains(&SECTION_GLOBAL));
        assert!(section_ids.contains(&SECTION_EXPORT));
        assert!(section_ids.contains(&SECTION_CODE));
        assert!(section_ids.contains(&SECTION_DATA));

        // Sections must be in ascending order per the spec
        for i in 1..section_ids.len() {
            assert!(section_ids[i] >= section_ids[i - 1],
                "sections out of order: {:?}", section_ids);
        }
    }

    #[test]
    fn test_export_present_for_pub_function() {
        let func = make_add_function();
        let program = make_program(vec![Item::Function(func)]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        // The export section should contain the string "add"
        let add_bytes = b"add";
        let found = bytes.windows(add_bytes.len()).any(|w| w == add_bytes);
        assert!(found, "exported function name 'add' not found in binary");
    }

    #[test]
    fn test_string_interning() {
        let mut intern = StringIntern::new(1024);
        let (off1, len1) = intern.intern("hello");
        assert_eq!(off1, 1024);
        assert_eq!(len1, 5);

        // Same string returns same offset
        let (off2, len2) = intern.intern("hello");
        assert_eq!(off1, off2);
        assert_eq!(len1, len2);

        // Different string comes after
        let (off3, len3) = intern.intern("world");
        assert_eq!(off3, 1029);
        assert_eq!(len3, 5);

        assert_eq!(intern.data, b"helloworld");
    }

    /// Helper to decode an unsigned LEB128 from a byte slice.
    /// Returns (value, number_of_bytes_consumed).
    fn read_unsigned_leb128(bytes: &[u8]) -> (u64, usize) {
        let mut result: u64 = 0;
        let mut shift: u32 = 0;
        for (i, &byte) in bytes.iter().enumerate() {
            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return (result, i + 1);
            }
            shift += 7;
        }
        (result, bytes.len())
    }

    // ── Full program with component ──────────────────────────────────────

    #[test]
    fn test_emit_component_program() {
        let program = make_program(vec![
            Item::Component(Component {
                name: "Counter".into(),
                type_params: vec![],
                props: vec![],
                state: vec![StateField {
                    name: "count".into(),
                    ty: Some(Type::Named("i32".into())),
                    mutable: true,
                    secret: false,
                    atomic: false,
                    initializer: Expr::Integer(0),
                    ownership: Ownership::Owned,
                }],
                methods: vec![Function {
                    name: "increment".into(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block {
                        stmts: vec![Stmt::Expr(Expr::Integer(1))],
                        span: empty_span(),
                    },
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: empty_span(),
                }],
                styles: vec![],
                transitions: vec![],
                trait_bounds: vec![],
                render: RenderBlock {
                    body: TemplateNode::Fragment(vec![]),
                    span: empty_span(),
                },
                permissions: None,
                gestures: vec![],
                skeleton: None,
                error_boundary: None,
                chunk: None,
                on_destroy: None,
                a11y: None,
                shortcuts: vec![],
                span: empty_span(),
            }),
        ]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        // Valid WASM header
        assert_eq!(&bytes[0..4], &WASM_MAGIC);
        assert_eq!(&bytes[4..8], &WASM_VERSION);
        assert!(bytes.len() > 8);
    }

    // ── Store with actions ───────────────────────────────────────────────

    #[test]
    fn test_emit_store_program() {
        let program = make_program(vec![
            Item::Store(StoreDef {
                name: "AppStore".into(),
                signals: vec![],
                actions: vec![],
                computed: vec![],
                effects: vec![],
                selectors: vec![],
                is_pub: false,
                span: empty_span(),
            }),
            Item::Function(make_add_function()),
        ]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        assert_eq!(&bytes[0..4], &WASM_MAGIC);
        // Should contain "add" export
        assert!(bytes.windows(3).any(|w| w == b"add"));
    }

    // ── Memory section (imported) ────────────────────────────────────────

    #[test]
    fn test_import_section_contains_memory() {
        let program = make_program(vec![Item::Function(make_add_function())]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        // The import section should contain "env" and "memory"
        assert!(bytes.windows(3).any(|w| w == b"env"), "missing 'env' in imports");
        assert!(bytes.windows(6).any(|w| w == b"memory"), "missing 'memory' in imports");
    }

    // ── Import section has DOM functions ──────────────────────────────────

    #[test]
    fn test_import_section_has_dom_functions() {
        let program = make_program(vec![Item::Function(make_add_function())]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        // Should contain "dom" module and "createElement" function
        assert!(bytes.windows(3).any(|w| w == b"dom"), "missing 'dom' in imports");
        assert!(bytes.windows(13).any(|w| w == b"createElement"), "missing 'createElement'");
        assert!(bytes.windows(7).any(|w| w == b"setText"), "missing 'setText'");
        assert!(bytes.windows(11).any(|w| w == b"appendChild"), "missing 'appendChild'");
    }

    // ── Data section for strings ─────────────────────────────────────────

    #[test]
    fn test_data_section_present_with_strings() {
        let program = make_program(vec![
            Item::Function(Function {
                name: "greet".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block {
                    stmts: vec![Stmt::Expr(Expr::StringLit("hello world".into()))],
                    span: empty_span(),
                },
                is_pub: true,
                is_async: false,
                must_use: false,
                span: empty_span(),
            }),
        ]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        // Data section (11) should be present
        let mut pos = 8;
        let mut found_data = false;
        while pos < bytes.len() {
            let id = bytes[pos];
            pos += 1;
            let (size, read) = read_unsigned_leb128(&bytes[pos..]);
            pos += read;
            if id == SECTION_DATA {
                found_data = true;
            }
            pos += size as usize;
        }
        assert!(found_data, "data section not found");

        // The string "hello world" should be in the binary
        assert!(bytes.windows(11).any(|w| w == b"hello world"));
    }

    // ── Type section for function types ──────────────────────────────────

    #[test]
    fn test_type_section_present() {
        let program = make_program(vec![Item::Function(make_add_function())]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        let mut pos = 8;
        let mut found_type = false;
        while pos < bytes.len() {
            let id = bytes[pos];
            pos += 1;
            let (size, read) = read_unsigned_leb128(&bytes[pos..]);
            pos += read;
            if id == SECTION_TYPE {
                found_type = true;
                // Type section content should start with count, then func type entries
                // Each entry starts with TYPE_FUNC (0x60)
                let content_start = pos;
                let (count, _) = read_unsigned_leb128(&bytes[content_start..]);
                assert!(count > 0, "type section has zero entries");
            }
            pos += size as usize;
        }
        assert!(found_type, "type section not found");
    }

    // ── Code section for function bodies ─────────────────────────────────

    #[test]
    fn test_code_section_present() {
        let program = make_program(vec![Item::Function(make_add_function())]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        let mut pos = 8;
        let mut found_code = false;
        while pos < bytes.len() {
            let id = bytes[pos];
            pos += 1;
            let (size, read) = read_unsigned_leb128(&bytes[pos..]);
            pos += read;
            if id == SECTION_CODE {
                found_code = true;
            }
            pos += size as usize;
        }
        assert!(found_code, "code section not found");
    }

    // ── Multiple functions ───────────────────────────────────────────────

    #[test]
    fn test_multiple_functions() {
        let func1 = make_add_function();
        let func2 = Function {
            name: "sub".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![
                Param { name: "a".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false },
                Param { name: "b".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false },
            ],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: Block {
                stmts: vec![
                    Stmt::Return(Some(Expr::Binary {
                        op: BinOp::Sub,
                        left: Box::new(Expr::Ident("a".into())),
                        right: Box::new(Expr::Ident("b".into())),
                    })),
                ],
                span: empty_span(),
            },
            is_pub: true,
            is_async: false,
            must_use: false,
            span: empty_span(),
        };
        let program = make_program(vec![Item::Function(func1), Item::Function(func2)]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        // Both names should appear in exports
        assert!(bytes.windows(3).any(|w| w == b"add"));
        assert!(bytes.windows(3).any(|w| w == b"sub"));
    }

    // ── Private function not exported ────────────────────────────────────

    #[test]
    fn test_private_function_not_exported() {
        let func = Function {
            name: "secret".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: Block {
                stmts: vec![Stmt::Return(Some(Expr::Integer(42)))],
                span: empty_span(),
            },
            is_pub: false,
            is_async: false,
            must_use: false,
            span: empty_span(),
        };
        let program = make_program(vec![Item::Function(func)]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        // Extract export section content — "secret" should NOT be in exports
        // (it might appear in the code section as a string literal, so we check
        // specifically in the export section)
        let mut pos = 8;
        while pos < bytes.len() {
            let id = bytes[pos];
            pos += 1;
            let (size, read) = read_unsigned_leb128(&bytes[pos..]);
            pos += read;
            if id == SECTION_EXPORT {
                let export_content = &bytes[pos..pos + size as usize];
                let export_str = String::from_utf8_lossy(export_content);
                // "secret" should not be in export section
                // (though "memory" and "alloc" might be, that's fine)
                assert!(!export_str.contains("secret"), "private fn should not be exported");
            }
            pos += size as usize;
        }
    }

    // ── String interning deduplication ───────────────────────────────────

    #[test]
    fn test_string_intern_empty() {
        let mut intern = StringIntern::new(0);
        let (off, len) = intern.intern("");
        assert_eq!(off, 0);
        assert_eq!(len, 0);
    }

    #[test]
    fn test_string_intern_multiple_different() {
        let mut intern = StringIntern::new(100);
        let (o1, l1) = intern.intern("abc");
        let (o2, l2) = intern.intern("def");
        let (o3, l3) = intern.intern("ghi");
        assert_eq!(o1, 100);
        assert_eq!(l1, 3);
        assert_eq!(o2, 103);
        assert_eq!(l2, 3);
        assert_eq!(o3, 106);
        assert_eq!(l3, 3);
        assert_eq!(intern.data, b"abcdefghi");
    }

    // ── Global section present ───────────────────────────────────────────

    #[test]
    fn test_global_section_present() {
        let program = make_program(vec![Item::Function(make_add_function())]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        let mut pos = 8;
        let mut found_global = false;
        while pos < bytes.len() {
            let id = bytes[pos];
            pos += 1;
            let (size, read) = read_unsigned_leb128(&bytes[pos..]);
            pos += read;
            if id == SECTION_GLOBAL {
                found_global = true;
            }
            pos += size as usize;
        }
        assert!(found_global, "global section not found");
    }

    // ── Section encoding helper ──────────────────────────────────────────

    #[test]
    fn test_encode_section_helper() {
        let content = vec![0x01, 0x02, 0x03];
        let section = encode_section(SECTION_TYPE, &content);
        assert_eq!(section[0], SECTION_TYPE);
        // LEB128 of 3 is just 0x03
        assert_eq!(section[1], 0x03);
        assert_eq!(&section[2..], &content);
    }

    // ── Group locals helper ──────────────────────────────────────────────

    #[test]
    fn test_group_locals_same_type() {
        let locals = vec![
            ("a".into(), WasmValType::I32),
            ("b".into(), WasmValType::I32),
            ("c".into(), WasmValType::I32),
        ];
        let groups = group_locals(&locals);
        assert_eq!(groups, vec![(3, WasmValType::I32)]);
    }

    #[test]
    fn test_group_locals_mixed_types() {
        let locals = vec![
            ("a".into(), WasmValType::I32),
            ("b".into(), WasmValType::F64),
            ("c".into(), WasmValType::F64),
            ("d".into(), WasmValType::I32),
        ];
        let groups = group_locals(&locals);
        assert_eq!(groups, vec![
            (1, WasmValType::I32),
            (2, WasmValType::F64),
            (1, WasmValType::I32),
        ]);
    }

    #[test]
    fn test_group_locals_empty() {
        let locals: Vec<(String, WasmValType)> = vec![];
        let groups = group_locals(&locals);
        assert!(groups.is_empty());
    }

    // ── AST type to valtype ──────────────────────────────────────────────

    #[test]
    fn test_ast_type_to_valtype_mapping() {
        assert_eq!(ast_type_to_valtype(&Type::Named("i32".into())), WasmValType::I32);
        assert_eq!(ast_type_to_valtype(&Type::Named("i64".into())), WasmValType::I64);
        assert_eq!(ast_type_to_valtype(&Type::Named("u64".into())), WasmValType::I64);
        assert_eq!(ast_type_to_valtype(&Type::Named("f32".into())), WasmValType::F32);
        assert_eq!(ast_type_to_valtype(&Type::Named("f64".into())), WasmValType::F64);
        assert_eq!(ast_type_to_valtype(&Type::Named("String".into())), WasmValType::I32);
        assert_eq!(ast_type_to_valtype(&Type::Generic {
            name: "Vec".into(),
            args: vec![Type::Named("i32".into())],
        }), WasmValType::I32);
    }

    // ── Empty program produces valid WASM ────────────────────────────────

    #[test]
    fn test_empty_program_valid_wasm() {
        let program = make_program(vec![]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        assert_eq!(&bytes[0..4], &WASM_MAGIC);
        assert_eq!(&bytes[4..8], &WASM_VERSION);
        // Should still have sections (imports at minimum)
        assert!(bytes.len() > 8);
    }
}

#[cfg(test)]
mod coverage_wasm_binary_tests {
    use super::*;
    use crate::token::Span;

    fn span() -> Span {
        Span { start: 0, end: 0, line: 1, col: 1 }
    }

    fn block(stmts: Vec<Stmt>) -> Block {
        Block { stmts, span: span() }
    }

    fn make_program(items: Vec<Item>) -> Program {
        Program { items }
    }

    fn read_unsigned_leb128(bytes: &[u8]) -> (u64, usize) {
        let mut result: u64 = 0;
        let mut shift: u32 = 0;
        for (i, &byte) in bytes.iter().enumerate() {
            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return (result, i + 1);
            }
            shift += 7;
        }
        (result, bytes.len())
    }

    // ── LEB128 edge cases ───────────────────────────────────────────────

    #[test]
    fn unsigned_leb128_large_value() {
        // Test a value that requires 3+ bytes
        let bytes = encode_unsigned_leb128(0x4000); // 16384
        assert!(bytes.len() >= 3);
        // Verify round-trip
        let (val, _) = read_unsigned_leb128(&bytes);
        assert_eq!(val, 0x4000);
    }

    #[test]
    fn unsigned_leb128_max_small() {
        assert_eq!(encode_unsigned_leb128(127), vec![0x7F]);
    }

    #[test]
    fn signed_leb128_large_positive() {
        let bytes = encode_signed_leb128(1024);
        assert!(bytes.len() >= 2);
    }

    #[test]
    fn signed_leb128_large_negative() {
        let bytes = encode_signed_leb128(-1024);
        assert!(bytes.len() >= 2);
    }

    #[test]
    fn signed_leb128_min_two_byte_boundary() {
        // -65 requires two bytes
        let bytes = encode_signed_leb128(-65);
        assert_eq!(bytes.len(), 2);
    }

    // ── WasmValType::to_byte ────────────────────────────────────────────

    #[test]
    fn valtype_to_byte_all_variants() {
        assert_eq!(WasmValType::I32.to_byte(), 0x7F);
        assert_eq!(WasmValType::I64.to_byte(), 0x7E);
        assert_eq!(WasmValType::F32.to_byte(), 0x7D);
        assert_eq!(WasmValType::F64.to_byte(), 0x7C);
    }

    // ── ast_type_to_valtype extended ────────────────────────────────────

    #[test]
    fn ast_type_array_maps_to_i32() {
        assert_eq!(ast_type_to_valtype(&Type::Array(Box::new(Type::Named("i32".into())))), WasmValType::I32);
    }

    #[test]
    fn ast_type_option_maps_to_i32() {
        assert_eq!(ast_type_to_valtype(&Type::Option(Box::new(Type::Named("i32".into())))), WasmValType::I32);
    }

    #[test]
    fn ast_type_tuple_maps_to_i32() {
        assert_eq!(ast_type_to_valtype(&Type::Tuple(vec![Type::Named("i32".into())])), WasmValType::I32);
    }

    #[test]
    fn ast_type_result_maps_to_i32() {
        assert_eq!(ast_type_to_valtype(&Type::Result {
            ok: Box::new(Type::Named("i32".into())),
            err: Box::new(Type::Named("String".into())),
        }), WasmValType::I32);
    }

    #[test]
    fn ast_type_reference_maps_to_i32() {
        assert_eq!(ast_type_to_valtype(&Type::Reference {
            mutable: false,
            lifetime: None,
            inner: Box::new(Type::Named("i32".into())),
        }), WasmValType::I32);
    }

    // ── StringIntern ────────────────────────────────────────────────────

    #[test]
    fn string_intern_deduplication() {
        let mut intern = StringIntern::new(0);
        let (o1, l1) = intern.intern("hello");
        let (o2, l2) = intern.intern("hello");
        assert_eq!(o1, o2);
        assert_eq!(l1, l2);
        assert_eq!(intern.data.len(), 5); // stored only once
    }

    #[test]
    fn string_intern_unicode() {
        let mut intern = StringIntern::new(256);
        let (off, len) = intern.intern("héllo");
        assert_eq!(off, 256);
        assert_eq!(len, "héllo".len() as u32); // 6 bytes in UTF-8
    }

    // ── group_locals ────────────────────────────────────────────────────

    #[test]
    fn group_locals_single_item() {
        let locals = vec![("x".into(), WasmValType::F64)];
        let groups = group_locals(&locals);
        assert_eq!(groups, vec![(1, WasmValType::F64)]);
    }

    #[test]
    fn group_locals_alternating_types() {
        let locals = vec![
            ("a".into(), WasmValType::I32),
            ("b".into(), WasmValType::I64),
            ("c".into(), WasmValType::I32),
            ("d".into(), WasmValType::I64),
        ];
        let groups = group_locals(&locals);
        assert_eq!(groups.len(), 4);
    }

    // ── encode_name ─────────────────────────────────────────────────────

    #[test]
    fn encode_name_produces_length_prefixed_bytes() {
        let mut out = Vec::new();
        encode_name(&mut out, "test");
        assert_eq!(out[0], 4); // length
        assert_eq!(&out[1..], b"test");
    }

    #[test]
    fn encode_name_empty_string() {
        let mut out = Vec::new();
        encode_name(&mut out, "");
        assert_eq!(out[0], 0);
        assert_eq!(out.len(), 1);
    }

    // ── encode_section ──────────────────────────────────────────────────

    #[test]
    fn encode_section_empty_content() {
        let section = encode_section(SECTION_DATA, &[]);
        assert_eq!(section[0], SECTION_DATA);
        assert_eq!(section[1], 0); // zero length
        assert_eq!(section.len(), 2);
    }

    // ── Full program with test blocks ───────────────────────────────────

    #[test]
    fn emit_program_with_tests() {
        let program = make_program(vec![
            Item::Test(TestDef {
                name: "addition works".into(),
                body: block(vec![
                    Stmt::Expr(Expr::Assert {
                        condition: Box::new(Expr::Bool(true)),
                        message: Some("should be true".into()),
                    }),
                ]),
                span: span(),
            }),
        ]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        assert_eq!(&bytes[0..4], &[0x00, 0x61, 0x73, 0x6D]);
        // Should contain __test_addition_works and __run_tests exports
        assert!(bytes.windows(21).any(|w| w == b"__test_addition_works"));
        assert!(bytes.windows(11).any(|w| w == b"__run_tests"));
    }

    #[test]
    fn emit_program_with_multiple_tests() {
        let program = make_program(vec![
            Item::Test(TestDef {
                name: "first".into(),
                body: block(vec![Stmt::Expr(Expr::Bool(true))]),
                span: span(),
            }),
            Item::Test(TestDef {
                name: "second".into(),
                body: block(vec![Stmt::Expr(Expr::Bool(false))]),
                span: span(),
            }),
        ]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);

        assert!(bytes.windows(12).any(|w| w == b"__test_first"));
        assert!(bytes.windows(13).any(|w| w == b"__test_second"));
        assert!(bytes.windows(11).any(|w| w == b"__run_tests"));
    }

    // ── Binary operations ───────────────────────────────────────────────

    #[test]
    fn emit_all_binary_ops() {
        let ops = vec![
            BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div, BinOp::Mod,
            BinOp::Eq, BinOp::Neq, BinOp::Lt, BinOp::Gt, BinOp::Lte,
            BinOp::Gte, BinOp::And, BinOp::Or,
        ];
        for op in ops {
            let program = make_program(vec![Item::Function(Function {
                name: "op_test".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![
                    Param { name: "a".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false },
                    Param { name: "b".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false },
                ],
                return_type: Some(Type::Named("i32".into())),
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(Some(Expr::Binary {
                    op: op.clone(),
                    left: Box::new(Expr::Ident("a".into())),
                    right: Box::new(Expr::Ident("b".into())),
                }))]),
                is_pub: true,
                is_async: false,
                must_use: false,
                span: span(),
            })]);
            let mut emitter = WasmBinaryEmitter::new();
            let bytes = emitter.emit(&program);
            assert!(bytes.len() > 8, "binary op {:?} should produce valid wasm", op);
        }
    }

    // ── Unary operations ────────────────────────────────────────────────

    #[test]
    fn emit_unary_neg() {
        let program = make_program(vec![Item::Function(Function {
            name: "negate".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "x".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::Unary {
                op: UnaryOp::Neg,
                operand: Box::new(Expr::Ident("x".into())),
            }))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    #[test]
    fn emit_unary_not() {
        let program = make_program(vec![Item::Function(Function {
            name: "logical_not".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "x".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::Unary {
                op: UnaryOp::Not,
                operand: Box::new(Expr::Ident("x".into())),
            }))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    // ── Expression encoding ─────────────────────────────────────────────

    #[test]
    fn emit_float_expr() {
        let program = make_program(vec![Item::Function(Function {
            name: "get_pi".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: Some(Type::Named("f64".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::Float(3.14)))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        // F64 const opcode should appear
        assert!(bytes.contains(&OP_F64_CONST));
    }

    #[test]
    fn emit_bool_expr() {
        let program = make_program(vec![Item::Function(Function {
            name: "get_true".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::Bool(true)))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    #[test]
    fn emit_string_lit_expr() {
        let program = make_program(vec![Item::Function(Function {
            name: "greet".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::StringLit("test string".into()))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.windows(11).any(|w| w == b"test string"));
    }

    #[test]
    fn emit_if_else_expr() {
        let program = make_program(vec![Item::Function(Function {
            name: "check".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "x".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::If {
                condition: Box::new(Expr::Ident("x".into())),
                then_block: block(vec![Stmt::Expr(Expr::Integer(1))]),
                else_block: Some(block(vec![Stmt::Expr(Expr::Integer(0))])),
            }))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_IF));
        assert!(bytes.contains(&OP_ELSE));
    }

    #[test]
    fn emit_if_no_else() {
        let program = make_program(vec![Item::Function(Function {
            name: "check".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "x".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::If {
                condition: Box::new(Expr::Ident("x".into())),
                then_block: block(vec![Stmt::Expr(Expr::Integer(1))]),
                else_block: None,
            }))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_IF));
    }

    #[test]
    fn emit_assign_expr() {
        let program = make_program(vec![Item::Function(Function {
            name: "mutate".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![
                Stmt::Let { name: "x".into(), ty: None, value: Expr::Integer(0), mutable: true, secret: false, ownership: Ownership::Owned },
                Stmt::Expr(Expr::Assign {
                    target: Box::new(Expr::Ident("x".into())),
                    value: Box::new(Expr::Integer(42)),
                }),
            ]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_LOCAL_SET));
    }

    #[test]
    fn emit_block_expr() {
        let program = make_program(vec![Item::Function(Function {
            name: "do_block".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::Block(block(vec![
                Stmt::Expr(Expr::Integer(1)),
            ])))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    #[test]
    fn emit_index_expr() {
        let program = make_program(vec![Item::Function(Function {
            name: "get_elem".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "arr".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::Index {
                object: Box::new(Expr::Ident("arr".into())),
                index: Box::new(Expr::Integer(0)),
            }))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_I32_LOAD));
        assert!(bytes.contains(&OP_I32_MUL));
    }

    #[test]
    fn emit_borrow_and_borrow_mut() {
        let program = make_program(vec![Item::Function(Function {
            name: "borrow_test".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "x".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: block(vec![
                Stmt::Expr(Expr::Borrow(Box::new(Expr::Ident("x".into())))),
                Stmt::Return(Some(Expr::BorrowMut(Box::new(Expr::Ident("x".into()))))),
            ]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    #[test]
    fn emit_field_access() {
        let program = make_program(vec![Item::Function(Function {
            name: "get_field".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "obj".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::FieldAccess {
                object: Box::new(Expr::Ident("obj".into())),
                field: "x".into(),
            }))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_I32_LOAD));
    }

    #[test]
    fn emit_fn_call() {
        let program = make_program(vec![
            Item::Function(Function {
                name: "helper".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: Some(Type::Named("i32".into())),
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(Some(Expr::Integer(1)))]),
                is_pub: false,
                is_async: false,
                must_use: false,
                span: span(),
            }),
            Item::Function(Function {
                name: "caller".into(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: Some(Type::Named("i32".into())),
                trait_bounds: vec![],
                body: block(vec![Stmt::Return(Some(Expr::FnCall {
                    callee: Box::new(Expr::Ident("helper".into())),
                    args: vec![],
                }))]),
                is_pub: true,
                is_async: false,
                must_use: false,
                span: span(),
            }),
        ]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_CALL));
    }

    #[test]
    fn emit_method_call() {
        let program = make_program(vec![Item::Function(Function {
            name: "test_method".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "obj".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::MethodCall {
                object: Box::new(Expr::Ident("obj".into())),
                method: "do_thing".into(),
                args: vec![Expr::Integer(1)],
            })]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    // ── Assert and AssertEq expressions ─────────────────────────────────

    #[test]
    fn emit_assert_expr() {
        let program = make_program(vec![Item::Function(Function {
            name: "test_assert".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::Assert {
                condition: Box::new(Expr::Bool(true)),
                message: Some("custom msg".into()),
            })]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_IF));
        assert!(bytes.windows(10).any(|w| w == b"custom msg"));
    }

    #[test]
    fn emit_assert_no_message() {
        let program = make_program(vec![Item::Function(Function {
            name: "test_assert_default".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::Assert {
                condition: Box::new(Expr::Bool(true)),
                message: None,
            })]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.windows(16).any(|w| w == b"assertion failed"));
    }

    #[test]
    fn emit_assert_eq() {
        let program = make_program(vec![Item::Function(Function {
            name: "test_eq".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::AssertEq {
                left: Box::new(Expr::Integer(1)),
                right: Box::new(Expr::Integer(1)),
                message: Some("should be equal".into()),
            })]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_I32_EQ));
        assert!(bytes.windows(15).any(|w| w == b"should be equal"));
    }

    #[test]
    fn emit_assert_eq_no_message() {
        let program = make_program(vec![Item::Function(Function {
            name: "test_eq_default".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::AssertEq {
                left: Box::new(Expr::Integer(1)),
                right: Box::new(Expr::Integer(2)),
                message: None,
            })]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.windows(13).any(|w| w == b"assert_eq fai"));
    }

    // ── TryCatch expression ─────────────────────────────────────────────

    #[test]
    fn emit_try_catch() {
        let program = make_program(vec![Item::Function(Function {
            name: "safe_call".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::TryCatch {
                body: Box::new(Expr::Integer(1)),
                error_binding: "err".into(),
                catch_body: Box::new(Expr::Integer(0)),
            })]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_BLOCK));
        assert!(bytes.contains(&OP_BR));
    }

    // ── FormatString expression ─────────────────────────────────────────

    #[test]
    fn emit_format_string_with_literal() {
        let program = make_program(vec![Item::Function(Function {
            name: "fmt".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::FormatString {
                parts: vec![
                    FormatPart::Literal("hello ".into()),
                    FormatPart::Literal("world".into()),
                ],
            })]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.windows(6).any(|w| w == b"hello "));
        assert!(bytes.windows(5).any(|w| w == b"world"));
    }

    #[test]
    fn emit_format_string_with_expression() {
        let program = make_program(vec![Item::Function(Function {
            name: "fmt_expr".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "x".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::FormatString {
                parts: vec![
                    FormatPart::Literal("val=".into()),
                    FormatPart::Expression(Box::new(Expr::Ident("x".into()))),
                ],
            })]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.windows(4).any(|w| w == b"val="));
    }

    #[test]
    fn emit_format_string_empty() {
        let program = make_program(vec![Item::Function(Function {
            name: "fmt_empty".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::FormatString {
                parts: vec![],
            })]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    // ── Try (?) operator ────────────────────────────────────────────────

    #[test]
    fn emit_try_operator() {
        let program = make_program(vec![Item::Function(Function {
            name: "try_op".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "x".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::Try(Box::new(Expr::Ident("x".into())))))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_BR_IF));
        assert!(bytes.contains(&OP_RETURN));
    }

    // ── Unsupported expression fallback (nop) ───────────────────────────

    #[test]
    fn emit_unsupported_expr_produces_nop() {
        let program = make_program(vec![Item::Function(Function {
            name: "nop_test".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::Await(Box::new(Expr::Integer(1))))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_NOP));
    }

    // ── Statement encoding ──────────────────────────────────────────────

    #[test]
    fn emit_signal_stmt() {
        let program = make_program(vec![Item::Function(Function {
            name: "sig_test".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![
                Stmt::Signal { name: "count".into(), ty: Some(Type::Named("i32".into())), value: Expr::Integer(0), secret: false, atomic: false },
            ]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_LOCAL_SET));
    }

    #[test]
    fn emit_return_none() {
        let program = make_program(vec![Item::Function(Function {
            name: "early_return".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(None)]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_RETURN));
    }

    #[test]
    fn emit_yield_stmt() {
        let program = make_program(vec![Item::Function(Function {
            name: "gen".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Yield(Expr::Integer(42))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    // ── LetDestructure with patterns ────────────────────────────────────

    #[test]
    fn emit_let_destructure_tuple() {
        let program = make_program(vec![Item::Function(Function {
            name: "destructure_test".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "t".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::LetDestructure {
                pattern: Pattern::Tuple(vec![
                    Pattern::Ident("a".into()),
                    Pattern::Ident("b".into()),
                ]),
                value: Expr::Ident("t".into()),
                ty: None,
            }]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_I32_LOAD));
    }

    #[test]
    fn emit_let_destructure_array() {
        let program = make_program(vec![Item::Function(Function {
            name: "arr_destructure".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "arr".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::LetDestructure {
                pattern: Pattern::Array(vec![
                    Pattern::Ident("first".into()),
                    Pattern::Wildcard,
                    Pattern::Ident("third".into()),
                ]),
                value: Expr::Ident("arr".into()),
                ty: None,
            }]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    #[test]
    fn emit_let_destructure_struct() {
        let program = make_program(vec![Item::Function(Function {
            name: "struct_destructure".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "s".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::LetDestructure {
                pattern: Pattern::Struct {
                    name: "Point".into(),
                    fields: vec![
                        ("x".into(), Pattern::Ident("px".into())),
                        ("y".into(), Pattern::Ident("py".into())),
                    ],
                    rest: false,
                },
                value: Expr::Ident("s".into()),
                ty: None,
            }]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&OP_I32_LOAD));
    }

    #[test]
    fn emit_let_destructure_wildcard_and_literal() {
        let program = make_program(vec![Item::Function(Function {
            name: "pattern_test".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "v".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::LetDestructure {
                pattern: Pattern::Tuple(vec![
                    Pattern::Wildcard,
                    Pattern::Literal(Expr::Integer(42)),
                ]),
                value: Expr::Ident("v".into()),
                ty: None,
            }]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    #[test]
    fn emit_let_destructure_variant() {
        let program = make_program(vec![Item::Function(Function {
            name: "variant_test".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "v".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::LetDestructure {
                pattern: Pattern::Variant {
                    name: "Some".into(),
                    fields: vec![Pattern::Ident("val".into())],
                },
                value: Expr::Ident("v".into()),
                ty: None,
            }]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    // ── collect_block_locals ────────────────────────────────────────────

    #[test]
    fn collect_locals_from_signal() {
        let blk = block(vec![
            Stmt::Signal { name: "s".into(), ty: Some(Type::Named("f64".into())), value: Expr::Float(0.0), secret: false, atomic: false },
        ]);
        let mut locals = Vec::new();
        collect_block_locals(&blk, &mut locals);
        assert_eq!(locals.len(), 1);
        assert_eq!(locals[0].0, "s");
        assert_eq!(locals[0].1, WasmValType::F64);
    }

    #[test]
    fn collect_locals_from_let_no_type() {
        let blk = block(vec![
            Stmt::Let { name: "x".into(), ty: None, value: Expr::Integer(0), mutable: false, secret: false, ownership: Ownership::Owned },
        ]);
        let mut locals = Vec::new();
        collect_block_locals(&blk, &mut locals);
        assert_eq!(locals.len(), 1);
        assert_eq!(locals[0].1, WasmValType::I32); // default
    }

    #[test]
    fn collect_locals_from_let_destructure() {
        let blk = block(vec![
            Stmt::LetDestructure {
                pattern: Pattern::Tuple(vec![
                    Pattern::Ident("a".into()),
                    Pattern::Ident("b".into()),
                ]),
                value: Expr::Integer(0),
                ty: None,
            },
        ]);
        let mut locals = Vec::new();
        collect_block_locals(&blk, &mut locals);
        assert_eq!(locals.len(), 2);
    }

    #[test]
    fn collect_locals_skips_return_and_expr() {
        let blk = block(vec![
            Stmt::Return(Some(Expr::Integer(0))),
            Stmt::Expr(Expr::Integer(1)),
            Stmt::Yield(Expr::Integer(2)),
        ]);
        let mut locals = Vec::new();
        collect_block_locals(&blk, &mut locals);
        assert_eq!(locals.len(), 0);
    }

    // ── collect_pattern_locals_wasm ─────────────────────────────────────

    #[test]
    fn collect_pattern_locals_struct() {
        let pattern = Pattern::Struct {
            name: "P".into(),
            fields: vec![
                ("x".into(), Pattern::Ident("px".into())),
                ("y".into(), Pattern::Ident("py".into())),
            ],
            rest: false,
        };
        let mut locals = Vec::new();
        collect_pattern_locals_wasm(&pattern, &mut locals);
        assert_eq!(locals.len(), 2);
    }

    #[test]
    fn collect_pattern_locals_array() {
        let pattern = Pattern::Array(vec![
            Pattern::Ident("a".into()),
            Pattern::Wildcard,
            Pattern::Ident("c".into()),
        ]);
        let mut locals = Vec::new();
        collect_pattern_locals_wasm(&pattern, &mut locals);
        assert_eq!(locals.len(), 2);
    }

    #[test]
    fn collect_pattern_locals_variant_and_literal() {
        let pattern = Pattern::Variant { name: "Some".into(), fields: vec![Pattern::Ident("v".into())] };
        let mut locals = Vec::new();
        collect_pattern_locals_wasm(&pattern, &mut locals);
        // Variant doesn't recursively collect in the impl
        assert_eq!(locals.len(), 0);

        let lit_pat = Pattern::Literal(Expr::Integer(0));
        collect_pattern_locals_wasm(&lit_pat, &mut locals);
        assert_eq!(locals.len(), 0);
    }

    // ── FuncSig dedup / intern_type ─────────────────────────────────────

    #[test]
    fn intern_type_deduplicates() {
        let mut emitter = WasmBinaryEmitter::new();
        let sig1 = FuncSig { params: vec![WasmValType::I32], results: vec![WasmValType::I32] };
        let sig2 = FuncSig { params: vec![WasmValType::I32], results: vec![WasmValType::I32] };
        let idx1 = emitter.intern_type(sig1);
        let idx2 = emitter.intern_type(sig2);
        assert_eq!(idx1, idx2);
    }

    #[test]
    fn intern_type_different_sigs() {
        let mut emitter = WasmBinaryEmitter::new();
        let sig1 = FuncSig { params: vec![WasmValType::I32], results: vec![WasmValType::I32] };
        let sig2 = FuncSig { params: vec![WasmValType::F64], results: vec![WasmValType::F64] };
        let idx1 = emitter.intern_type(sig1);
        let idx2 = emitter.intern_type(sig2);
        assert_ne!(idx1, idx2);
    }

    // ── Data section edge cases ─────────────────────────────────────────

    #[test]
    fn data_section_empty_when_no_strings() {
        let program = make_program(vec![Item::Function(Function {
            name: "no_strings".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: Some(Type::Named("i32".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::Integer(42)))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        // Data section should still be present (empty segment)
        let mut pos = 8;
        let mut found_data = false;
        while pos < bytes.len() {
            let id = bytes[pos];
            pos += 1;
            let (size, read) = read_unsigned_leb128(&bytes[pos..]);
            pos += read;
            if id == SECTION_DATA {
                found_data = true;
            }
            pos += size as usize;
        }
        assert!(found_data);
    }

    // ── SelfExpr ────────────────────────────────────────────────────────

    #[test]
    fn emit_self_expr() {
        // SelfExpr only does something if "self" is in the local_map
        // which happens when a param named "self" is present (but it's filtered)
        // This tests the fallthrough case (no self in map)
        let program = make_program(vec![Item::Function(Function {
            name: "self_test".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::SelfExpr)]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    // ── Function with various param types ───────────────────────────────

    #[test]
    fn emit_function_with_i64_params() {
        let program = make_program(vec![Item::Function(Function {
            name: "add64".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![
                Param { name: "a".into(), ty: Type::Named("i64".into()), ownership: Ownership::Owned, secret: false },
                Param { name: "b".into(), ty: Type::Named("i64".into()), ownership: Ownership::Owned, secret: false },
            ],
            return_type: Some(Type::Named("i64".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::Ident("a".into())))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        // Should have i64 valtype (0x7E) in the type section
        assert!(bytes.contains(&VALTYPE_I64));
    }

    #[test]
    fn emit_function_with_f32_param() {
        let program = make_program(vec![Item::Function(Function {
            name: "f32_func".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![
                Param { name: "x".into(), ty: Type::Named("f32".into()), ownership: Ownership::Owned, secret: false },
            ],
            return_type: Some(Type::Named("f32".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Return(Some(Expr::Ident("x".into())))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.contains(&VALTYPE_F32));
    }

    // ── Alloc body encoding ─────────────────────────────────────────────

    #[test]
    fn alloc_body_has_correct_opcodes() {
        let emitter = WasmBinaryEmitter::new();
        let body = emitter.encode_alloc_body();
        // Should contain: global.get, local.set, global.get, local.get, i32.add, global.set, local.get, end
        assert!(body.contains(&OP_GLOBAL_GET));
        assert!(body.contains(&OP_LOCAL_SET));
        assert!(body.contains(&OP_I32_ADD));
        assert!(body.contains(&OP_GLOBAL_SET));
        assert!(body.contains(&OP_LOCAL_GET));
        assert!(body.contains(&OP_END));
    }

    // ── Test with locals of different types ──────────────────────────────

    #[test]
    fn emit_function_with_mixed_locals() {
        let program = make_program(vec![Item::Function(Function {
            name: "mixed".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![
                Stmt::Let { name: "a".into(), ty: Some(Type::Named("i32".into())), value: Expr::Integer(0), mutable: false, secret: false, ownership: Ownership::Owned },
                Stmt::Let { name: "b".into(), ty: Some(Type::Named("f64".into())), value: Expr::Float(0.0), mutable: false, secret: false, ownership: Ownership::Owned },
                Stmt::Let { name: "c".into(), ty: Some(Type::Named("i64".into())), value: Expr::Integer(0), mutable: false, secret: false, ownership: Ownership::Owned },
                Stmt::Let { name: "d".into(), ty: Some(Type::Named("f32".into())), value: Expr::Float(0.0), mutable: false, secret: false, ownership: Ownership::Owned },
            ]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    // ── Test body with assert_eq inside ─────────────────────────────────

    #[test]
    fn emit_test_body_with_assertions() {
        let program = make_program(vec![
            Item::Test(TestDef {
                name: "math works".into(),
                body: block(vec![
                    Stmt::Let { name: "x".into(), ty: None, value: Expr::Integer(5), mutable: false, secret: false, ownership: Ownership::Owned },
                    Stmt::Expr(Expr::AssertEq {
                        left: Box::new(Expr::Ident("x".into())),
                        right: Box::new(Expr::Integer(5)),
                        message: Some("x should be 5".into()),
                    }),
                ]),
                span: span(),
            }),
        ]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.windows(13).any(|w| w == b"x should be 5"));
    }

    // ── FnCall with non-ident callee ────────────────────────────────────

    #[test]
    fn emit_fn_call_with_complex_callee() {
        // When callee is not an Ident, no call is generated
        let program = make_program(vec![Item::Function(Function {
            name: "complex".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::FnCall {
                callee: Box::new(Expr::FieldAccess {
                    object: Box::new(Expr::Ident("obj".into())),
                    field: "method".into(),
                }),
                args: vec![],
            })]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    // ── Assign to non-ident target ──────────────────────────────────────

    #[test]
    fn emit_assign_to_non_ident() {
        let program = make_program(vec![Item::Function(Function {
            name: "assign_field".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param { name: "obj".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned, secret: false }],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::Assign {
                target: Box::new(Expr::FieldAccess {
                    object: Box::new(Expr::Ident("obj".into())),
                    field: "x".into(),
                }),
                value: Box::new(Expr::Integer(1)),
            })]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }

    // ── Ident not in local_map ──────────────────────────────────────────

    #[test]
    fn emit_unknown_ident() {
        let program = make_program(vec![Item::Function(Function {
            name: "unknown".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::Ident("nonexistent".into()))]),
            is_pub: true,
            is_async: false,
            must_use: false,
            span: span(),
        })]);
        let mut emitter = WasmBinaryEmitter::new();
        let bytes = emitter.emit(&program);
        assert!(bytes.len() > 8);
    }
}
