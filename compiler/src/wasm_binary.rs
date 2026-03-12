use crate::ast::*;
use std::collections::HashMap;

// ── WebAssembly binary format constants ──────────────────────────────────────

const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D]; // \0asm
const WASM_VERSION: [u8; 4] = [0x01, 0x00, 0x00, 0x00]; // version 1

// Section IDs
const SECTION_TYPE: u8 = 1;
const SECTION_IMPORT: u8 = 2;
const SECTION_FUNCTION: u8 = 3;
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
const KIND_GLOBAL: u8 = 0x03;

// Global mutability
const GLOBAL_CONST: u8 = 0x00;
const GLOBAL_MUT: u8 = 0x01;

// Limits
const LIMITS_MIN_ONLY: u8 = 0x00;

// WASM opcodes
const OP_UNREACHABLE: u8 = 0x00;
const OP_NOP: u8 = 0x01;
const OP_BLOCK: u8 = 0x02;
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
const OP_LOCAL_TEE: u8 = 0x22;
const OP_GLOBAL_GET: u8 = 0x23;
const OP_GLOBAL_SET: u8 = 0x24;
const OP_I32_LOAD: u8 = 0x28;
const OP_I32_STORE: u8 = 0x36;
const OP_I32_CONST: u8 = 0x41;
const OP_I64_CONST: u8 = 0x42;
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
                Param { name: "a".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned },
                Param { name: "b".into(), ty: Type::Named("i32".into()), ownership: Ownership::Owned },
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
}
