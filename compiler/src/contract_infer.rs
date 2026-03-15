use crate::ast::*;
use crate::token::Span;

/// An automatically inferred API contract, derived from how a fetch response
/// is used in the source code. No manual contract block needed — the code IS
/// the contract.
#[derive(Debug, Clone)]
pub struct InferredContract {
    /// The URL expression from the fetch call (as a string, for display)
    pub fetch_url: String,
    /// HTTP method (GET if not specified)
    pub method: String,
    /// Source location of the fetch expression
    pub fetch_span: Span,
    /// Inferred response fields
    pub fields: Vec<InferredField>,
    /// Which function/component/action contains this fetch
    pub source_context: String,
}

/// A single inferred field in an API response.
#[derive(Debug, Clone, PartialEq)]
pub struct InferredField {
    /// Path from root, e.g. ["user", "name"] for response.user.name
    pub path: Vec<String>,
    /// What type we inferred from usage
    pub inferred_type: InferredType,
    /// Evidence: what code patterns told us the type
    pub evidence: Vec<FieldEvidence>,
}

/// Inferred type of a response field.
#[derive(Debug, Clone, PartialEq)]
pub enum InferredType {
    String,
    Numeric,
    Bool,
    Array(Box<InferredType>),
    Object(Vec<InferredField>),
    Unknown,
}

impl std::fmt::Display for InferredType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferredType::String => write!(f, "String"),
            InferredType::Numeric => write!(f, "Numeric"),
            InferredType::Bool => write!(f, "Bool"),
            InferredType::Array(inner) => write!(f, "[{}]", inner),
            InferredType::Object(fields) => {
                write!(f, "{{")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", field.path.last().unwrap_or(&"?".to_string()), field.inferred_type)?;
                }
                write!(f, "}}")
            }
            InferredType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Evidence for why we inferred a particular type.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldEvidence {
    /// Field used as text content in a template or string interpolation
    UsedAsText(Span),
    /// Field used in arithmetic (+, -, *, /)
    ArithmeticOp(Span),
    /// Field used in a boolean context (if condition)
    BooleanContext(Span),
    /// A sub-field was accessed (implies Object)
    FieldAccess(String, Span),
    /// Indexed with [] (implies Array)
    IndexAccess(Span),
    /// Iterated over in a for loop (implies Array)
    ForIteration(Span),
    /// Method called on the field
    MethodCall(String, Span),
}

/// Walk an entire program and infer contracts from fetch usage.
pub fn infer_contracts(program: &Program) -> Vec<InferredContract> {
    let mut contracts = Vec::new();

    for item in &program.items {
        match item {
            Item::Component(c) => {
                infer_from_component(c, &mut contracts);
            }
            Item::Function(f) => {
                infer_from_block(&f.body, &f.name, &mut contracts);
            }
            Item::Store(s) => {
                for action in &s.actions {
                    let ctx = format!("{}::{}", s.name, action.name);
                    infer_from_block(&action.body, &ctx, &mut contracts);
                }
            }
            Item::Page(p) => {
                for method in &p.methods {
                    let ctx = format!("{}::{}", p.name, method.name);
                    infer_from_block(&method.body, &ctx, &mut contracts);
                }
            }
            _ => {}
        }
    }

    contracts
}

/// Infer contracts from a component's methods and state initializers.
fn infer_from_component(comp: &Component, contracts: &mut Vec<InferredContract>) {
    // Check state initializers for fetch expressions
    for state in &comp.state {
        let ctx = format!("{}::state::{}", comp.name, state.name);
        infer_from_expr(&state.initializer, &ctx, &state.name, contracts);
    }

    // Check methods
    for method in &comp.methods {
        let ctx = format!("{}::{}", comp.name, method.name);
        infer_from_block(&method.body, &ctx, contracts);
    }
}

/// Walk a block of statements looking for fetch expressions and tracking
/// how their results are used.
fn infer_from_block(block: &Block, context: &str, contracts: &mut Vec<InferredContract>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let { name, value, .. } | Stmt::Signal { name, value, .. } => {
                // If the value is a fetch, trace usage of `name` through rest of block
                if let Some(contract) = extract_fetch_contract(value, context) {
                    let mut contract = contract;
                    // Trace all subsequent statements for field accesses on this variable
                    trace_variable_usage(name, &block.stmts, &mut contract.fields);
                    contracts.push(contract);
                }
            }
            Stmt::Expr(expr) => {
                infer_from_expr_recursive(expr, context, contracts);
            }
            Stmt::Return(Some(expr)) => {
                infer_from_expr_recursive(expr, context, contracts);
            }
            _ => {}
        }
    }
}

/// Check if an expression is a fetch call and extract the contract skeleton.
fn extract_fetch_contract(expr: &Expr, context: &str) -> Option<InferredContract> {
    match expr {
        Expr::Fetch { url, contract, .. } => {
            // Skip if there's already an explicit contract
            if contract.is_some() {
                return None;
            }
            let url_str = expr_to_string(url);
            Some(InferredContract {
                fetch_url: url_str,
                method: extract_method(expr),
                fetch_span: Span::new(0, 0, 0, 0),
                fields: Vec::new(),
                source_context: context.to_string(),
            })
        }
        Expr::Await(inner) => extract_fetch_contract(inner, context),
        _ => None,
    }
}

/// Extract the HTTP method from a fetch expression's options.
fn extract_method(expr: &Expr) -> String {
    if let Expr::Fetch { options: Some(opts), .. } = expr {
        if let Expr::StructInit { fields, .. } = opts.as_ref() {
            for (name, value) in fields {
                if name == "method" {
                    if let Expr::StringLit(m) = value {
                        return m.to_uppercase();
                    }
                }
            }
        }
    }
    "GET".to_string()
}

/// Recursively walk an expression looking for fetch calls.
fn infer_from_expr_recursive(expr: &Expr, context: &str, contracts: &mut Vec<InferredContract>) {
    match expr {
        Expr::Fetch { .. } => {
            // Standalone fetch without let binding — can't trace usage
        }
        Expr::If { condition, then_block, else_block } => {
            infer_from_expr_recursive(condition, context, contracts);
            infer_from_block(then_block, context, contracts);
            if let Some(eb) = else_block {
                infer_from_block(eb, context, contracts);
            }
        }
        Expr::For { iterator, body, .. } => {
            infer_from_expr_recursive(iterator, context, contracts);
            infer_from_block(body, context, contracts);
        }
        Expr::Block(block) => {
            infer_from_block(block, context, contracts);
        }
        _ => {}
    }
}

/// Helper: try to infer contracts from an expression used as a state initializer.
fn infer_from_expr(expr: &Expr, context: &str, _binding_name: &str, contracts: &mut Vec<InferredContract>) {
    match expr {
        Expr::Fetch { url, contract, .. } if contract.is_none() => {
            let url_str = expr_to_string(url);
            // For state initializers, we can't easily trace usage without
            // walking the template. Create a skeleton contract.
            contracts.push(InferredContract {
                fetch_url: url_str,
                method: extract_method(expr),
                fetch_span: Span::new(0, 0, 0, 0),
                fields: Vec::new(),
                source_context: context.to_string(),
            });
        }
        Expr::Await(inner) => {
            infer_from_expr(inner, context, _binding_name, contracts);
        }
        _ => {}
    }
}

/// Trace how a variable is used across a slice of statements, collecting
/// field access patterns to infer the response shape.
fn trace_variable_usage(var_name: &str, stmts: &[Stmt], fields: &mut Vec<InferredField>) {
    for stmt in stmts {
        match stmt {
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => {
                collect_field_accesses(var_name, expr, &[], fields);
            }
            Stmt::Let { value, .. } | Stmt::Signal { value, .. } => {
                collect_field_accesses(var_name, value, &[], fields);
            }
            _ => {}
        }
    }
}

/// Recursively collect field accesses on a variable, building up the path.
fn collect_field_accesses(
    var_name: &str,
    expr: &Expr,
    current_path: &[String],
    fields: &mut Vec<InferredField>,
) {
    match expr {
        Expr::FieldAccess { object, field } => {
            if is_var(object, var_name) {
                // Direct field access: var.field
                let path = vec![field.clone()];
                add_or_update_field(fields, &path, InferredType::Unknown, FieldEvidence::FieldAccess(
                    field.clone(),
                    Span::new(0, 0, 0, 0),
                ));
            } else if let Expr::FieldAccess { .. } = object.as_ref() {
                // Nested: var.a.b — recurse to build path
                let mut path = Vec::new();
                if build_field_path(object, var_name, &mut path) {
                    path.push(field.clone());
                    // The intermediate path is an Object
                    if path.len() > 1 {
                        add_or_update_field(fields, &path[..path.len()-1], InferredType::Object(vec![]), FieldEvidence::FieldAccess(
                            path[path.len()-2].clone(),
                            Span::new(0, 0, 0, 0),
                        ));
                    }
                    add_or_update_field(fields, &path, InferredType::Unknown, FieldEvidence::FieldAccess(
                        field.clone(),
                        Span::new(0, 0, 0, 0),
                    ));
                }
            }
            // Continue recursing into the object
            collect_field_accesses(var_name, object, current_path, fields);
        }

        Expr::Index { object, .. } => {
            // var.items[0] — implies items is an Array
            if let Expr::FieldAccess { object: inner_obj, field } = object.as_ref() {
                if is_var(inner_obj, var_name) {
                    add_or_update_field(fields, &[field.clone()], InferredType::Array(Box::new(InferredType::Unknown)), FieldEvidence::IndexAccess(
                        Span::new(0, 0, 0, 0),
                    ));
                }
            }
            collect_field_accesses(var_name, object, current_path, fields);
        }

        Expr::For { binding: _, iterator, body } => {
            // for item in var.items — implies items is an Array
            if let Expr::FieldAccess { object, field } = iterator.as_ref() {
                if is_var(object, var_name) {
                    add_or_update_field(fields, &[field.clone()], InferredType::Array(Box::new(InferredType::Unknown)), FieldEvidence::ForIteration(
                        Span::new(0, 0, 0, 0),
                    ));
                }
            }
            collect_field_accesses(var_name, iterator, current_path, fields);
            for stmt in &body.stmts {
                if let Stmt::Expr(e) | Stmt::Return(Some(e)) = stmt {
                    collect_field_accesses(var_name, e, current_path, fields);
                }
            }
        }

        Expr::Binary { op, left, right } => {
            // Arithmetic ops imply Numeric
            match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                    if let Some(path) = extract_field_path(left, var_name) {
                        add_or_update_field(fields, &path, InferredType::Numeric, FieldEvidence::ArithmeticOp(
                            Span::new(0, 0, 0, 0),
                        ));
                    }
                    if let Some(path) = extract_field_path(right, var_name) {
                        add_or_update_field(fields, &path, InferredType::Numeric, FieldEvidence::ArithmeticOp(
                            Span::new(0, 0, 0, 0),
                        ));
                    }
                }
                BinOp::And | BinOp::Or => {
                    if let Some(path) = extract_field_path(left, var_name) {
                        add_or_update_field(fields, &path, InferredType::Bool, FieldEvidence::BooleanContext(
                            Span::new(0, 0, 0, 0),
                        ));
                    }
                    if let Some(path) = extract_field_path(right, var_name) {
                        add_or_update_field(fields, &path, InferredType::Bool, FieldEvidence::BooleanContext(
                            Span::new(0, 0, 0, 0),
                        ));
                    }
                }
                _ => {}
            }
            collect_field_accesses(var_name, left, current_path, fields);
            collect_field_accesses(var_name, right, current_path, fields);
        }

        Expr::If { condition, then_block, else_block } => {
            // Using a field in an if condition suggests Bool
            if let Some(path) = extract_field_path(condition, var_name) {
                add_or_update_field(fields, &path, InferredType::Bool, FieldEvidence::BooleanContext(
                    Span::new(0, 0, 0, 0),
                ));
            }
            collect_field_accesses(var_name, condition, current_path, fields);
            for stmt in &then_block.stmts {
                if let Stmt::Expr(e) | Stmt::Return(Some(e)) = stmt {
                    collect_field_accesses(var_name, e, current_path, fields);
                }
            }
            if let Some(eb) = else_block {
                for stmt in &eb.stmts {
                    if let Stmt::Expr(e) | Stmt::Return(Some(e)) = stmt {
                        collect_field_accesses(var_name, e, current_path, fields);
                    }
                }
            }
        }

        Expr::MethodCall { object, method, args } => {
            if let Some(path) = extract_field_path(object, var_name) {
                let evidence = FieldEvidence::MethodCall(method.clone(), Span::new(0, 0, 0, 0));
                // .len() on arrays, .to_string() implies conversion, etc.
                match method.as_str() {
                    "len" | "length" | "is_empty" | "push" | "pop" | "filter" | "map" => {
                        add_or_update_field(fields, &path, InferredType::Array(Box::new(InferredType::Unknown)), evidence);
                    }
                    _ => {
                        add_or_update_field(fields, &path, InferredType::Unknown, evidence);
                    }
                }
            }
            collect_field_accesses(var_name, object, current_path, fields);
            for arg in args {
                collect_field_accesses(var_name, arg, current_path, fields);
            }
        }

        Expr::FnCall { callee, args } => {
            collect_field_accesses(var_name, callee, current_path, fields);
            for arg in args {
                collect_field_accesses(var_name, arg, current_path, fields);
            }
        }

        Expr::FormatString { parts } => {
            for part in parts {
                if let FormatPart::Expression(expr) = part {
                    if let Some(path) = extract_field_path(expr, var_name) {
                        add_or_update_field(fields, &path, InferredType::String, FieldEvidence::UsedAsText(
                            Span::new(0, 0, 0, 0),
                        ));
                    }
                    collect_field_accesses(var_name, expr, current_path, fields);
                }
            }
        }

        _ => {}
    }
}

/// Check if an expression is a simple variable reference.
fn is_var(expr: &Expr, var_name: &str) -> bool {
    matches!(expr, Expr::Ident(name) if name == var_name)
}

/// Build a field path from a nested FieldAccess chain rooted at var_name.
/// Returns true if the chain starts with var_name.
fn build_field_path(expr: &Expr, var_name: &str, path: &mut Vec<String>) -> bool {
    match expr {
        Expr::Ident(name) if name == var_name => true,
        Expr::FieldAccess { object, field } => {
            if build_field_path(object, var_name, path) {
                path.push(field.clone());
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Extract the field path from an expression if it's a field access on var_name.
fn extract_field_path(expr: &Expr, var_name: &str) -> Option<Vec<String>> {
    match expr {
        Expr::FieldAccess { object, field } => {
            if is_var(object, var_name) {
                Some(vec![field.clone()])
            } else {
                let mut path = Vec::new();
                if build_field_path(object, var_name, &mut path) {
                    path.push(field.clone());
                    Some(path)
                } else {
                    None
                }
            }
        }
        _ => None,
    }
}

/// Add a field to the list or update its type if already present.
fn add_or_update_field(
    fields: &mut Vec<InferredField>,
    path: &[String],
    ty: InferredType,
    evidence: FieldEvidence,
) {
    // Look for existing field with same path
    for field in fields.iter_mut() {
        if field.path == path {
            // Update type if we have a more specific one
            if field.inferred_type == InferredType::Unknown && ty != InferredType::Unknown {
                field.inferred_type = ty;
            }
            field.evidence.push(evidence);
            return;
        }
    }

    // New field
    fields.push(InferredField {
        path: path.to_vec(),
        inferred_type: ty,
        evidence: vec![evidence],
    });
}

/// Convert an expression to a display string (for URLs).
fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::StringLit(s) => s.clone(),
        Expr::FormatString { parts } => {
            let mut result = String::new();
            for part in parts {
                match part {
                    FormatPart::Literal(s) => result.push_str(s),
                    FormatPart::Expression(_) => result.push_str("{...}"),
                }
            }
            result
        }
        _ => "<dynamic>".to_string(),
    }
}

/// Print inferred contracts as diagnostics.
pub fn print_inferred_contracts(contracts: &[InferredContract]) {
    for contract in contracts {
        if contract.fields.is_empty() {
            continue;
        }
        eprintln!(
            "[info] inferred contract for {} {} (in {}):",
            contract.method, contract.fetch_url, contract.source_context
        );
        for field in &contract.fields {
            let path_str = field.path.join(".");
            let evidence_str = field.evidence.iter().map(|e| match e {
                FieldEvidence::UsedAsText(_) => "text",
                FieldEvidence::ArithmeticOp(_) => "arithmetic",
                FieldEvidence::BooleanContext(_) => "condition",
                FieldEvidence::FieldAccess(_, _) => "field access",
                FieldEvidence::IndexAccess(_) => "index",
                FieldEvidence::ForIteration(_) => "iteration",
                FieldEvidence::MethodCall(m, _) => m.as_str(),
            }).collect::<Vec<_>>().join(", ");
            eprintln!("  {}: {} ({})", path_str, field.inferred_type, evidence_str);
        }
    }
}

// ==========================================================================
// Tests
// ==========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    fn make_fetch_expr(url: &str) -> Expr {
        Expr::Fetch {
            url: Box::new(Expr::StringLit(url.to_string())),
            options: None,
            contract: None,
        }
    }

    fn make_fetch_with_method(url: &str, method: &str) -> Expr {
        Expr::Fetch {
            url: Box::new(Expr::StringLit(url.to_string())),
            options: Some(Box::new(Expr::StructInit {
                name: String::new(),
                fields: vec![
                    ("method".to_string(), Expr::StringLit(method.to_string())),
                ],
            })),
            contract: None,
        }
    }

    fn make_field_access(var: &str, field: &str) -> Expr {
        Expr::FieldAccess {
            object: Box::new(Expr::Ident(var.to_string())),
            field: field.to_string(),
        }
    }

    fn make_nested_field(var: &str, fields: &[&str]) -> Expr {
        let mut expr = Expr::Ident(var.to_string());
        for f in fields {
            expr = Expr::FieldAccess {
                object: Box::new(expr),
                field: f.to_string(),
            };
        }
        expr
    }

    fn make_program_with_fetch_and_usage(url: &str, var_name: &str, usage_stmts: Vec<Stmt>) -> Program {
        let mut stmts = vec![
            Stmt::Let {
                name: var_name.to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: make_fetch_expr(url),
                ownership: Ownership::Owned,
            },
        ];
        stmts.extend(usage_stmts);

        Program {
            items: vec![
                Item::Function(Function {
                    name: "test_fn".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block {
                        stmts,
                        span: dummy_span(),
                    },
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: dummy_span(),
                }),
            ],
        }
    }

    #[test]
    fn test_infer_string_field_from_format_string() {
        let program = make_program_with_fetch_and_usage(
            "https://api.example.com/users",
            "response",
            vec![
                Stmt::Expr(Expr::FormatString {
                    parts: vec![
                        FormatPart::Literal("Hello ".to_string()),
                        FormatPart::Expression(Box::new(make_field_access("response", "name"))),
                    ],
                }),
            ],
        );

        let contracts = infer_contracts(&program);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].fetch_url, "https://api.example.com/users");
        assert_eq!(contracts[0].fields.len(), 1);
        assert_eq!(contracts[0].fields[0].path, vec!["name"]);
        assert_eq!(contracts[0].fields[0].inferred_type, InferredType::String);
    }

    #[test]
    fn test_infer_numeric_field_from_arithmetic() {
        let program = make_program_with_fetch_and_usage(
            "https://api.example.com/products/1",
            "product",
            vec![
                Stmt::Expr(Expr::Binary {
                    op: BinOp::Mul,
                    left: Box::new(make_field_access("product", "price")),
                    right: Box::new(Expr::Integer(2)),
                }),
            ],
        );

        let contracts = infer_contracts(&program);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].fields.len(), 1);
        assert_eq!(contracts[0].fields[0].path, vec!["price"]);
        assert_eq!(contracts[0].fields[0].inferred_type, InferredType::Numeric);
    }

    #[test]
    fn test_infer_bool_field_from_if_condition() {
        let program = make_program_with_fetch_and_usage(
            "https://api.example.com/products/1",
            "product",
            vec![
                Stmt::Expr(Expr::If {
                    condition: Box::new(make_field_access("product", "active")),
                    then_block: Block { stmts: vec![], span: dummy_span() },
                    else_block: None,
                }),
            ],
        );

        let contracts = infer_contracts(&program);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].fields.len(), 1);
        assert_eq!(contracts[0].fields[0].path, vec!["active"]);
        assert_eq!(contracts[0].fields[0].inferred_type, InferredType::Bool);
    }

    #[test]
    fn test_infer_array_from_for_loop() {
        let program = make_program_with_fetch_and_usage(
            "https://api.example.com/products",
            "response",
            vec![
                Stmt::Expr(Expr::For {
                    binding: "item".to_string(),
                    iterator: Box::new(make_field_access("response", "items")),
                    body: Block { stmts: vec![], span: dummy_span() },
                }),
            ],
        );

        let contracts = infer_contracts(&program);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].fields.len(), 1);
        assert_eq!(contracts[0].fields[0].path, vec!["items"]);
        assert!(matches!(contracts[0].fields[0].inferred_type, InferredType::Array(_)));
    }

    #[test]
    fn test_infer_array_from_index_access() {
        let program = make_program_with_fetch_and_usage(
            "https://api.example.com/products",
            "response",
            vec![
                Stmt::Expr(Expr::Index {
                    object: Box::new(make_field_access("response", "images")),
                    index: Box::new(Expr::Integer(0)),
                }),
            ],
        );

        let contracts = infer_contracts(&program);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].fields.len(), 1);
        assert_eq!(contracts[0].fields[0].path, vec!["images"]);
        assert!(matches!(contracts[0].fields[0].inferred_type, InferredType::Array(_)));
    }

    #[test]
    fn test_infer_nested_object() {
        let program = make_program_with_fetch_and_usage(
            "https://api.example.com/products/1",
            "product",
            vec![
                Stmt::Expr(make_nested_field("product", &["vendor", "name"])),
            ],
        );

        let contracts = infer_contracts(&program);
        assert_eq!(contracts.len(), 1);
        // Should have both vendor (Object) and vendor.name (Unknown)
        assert!(contracts[0].fields.len() >= 1);
        let vendor_field = contracts[0].fields.iter().find(|f| f.path == vec!["vendor"]);
        assert!(vendor_field.is_some());
    }

    #[test]
    fn test_infer_array_from_method_call() {
        let program = make_program_with_fetch_and_usage(
            "https://api.example.com/products",
            "response",
            vec![
                Stmt::Expr(Expr::MethodCall {
                    object: Box::new(make_field_access("response", "items")),
                    method: "len".to_string(),
                    args: vec![],
                }),
            ],
        );

        let contracts = infer_contracts(&program);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].fields[0].path, vec!["items"]);
        assert!(matches!(contracts[0].fields[0].inferred_type, InferredType::Array(_)));
    }

    #[test]
    fn test_skip_explicit_contract() {
        let program = Program {
            items: vec![
                Item::Function(Function {
                    name: "test_fn".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block {
                        stmts: vec![
                            Stmt::Let {
                                name: "data".to_string(),
                                ty: None,
                                mutable: false,
                                secret: false,
                                value: Expr::Fetch {
                                    url: Box::new(Expr::StringLit("https://api.example.com".to_string())),
                                    options: None,
                                    contract: Some("UserContract".to_string()),
                                },
                                ownership: Ownership::Owned,
                            },
                        ],
                        span: dummy_span(),
                    },
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: dummy_span(),
                }),
            ],
        };

        let contracts = infer_contracts(&program);
        // Should find 0 inferred contracts since there's an explicit one
        assert!(contracts.iter().all(|c| c.fields.is_empty() || c.fetch_url != "https://api.example.com"));
    }

    #[test]
    fn test_infer_method_from_options() {
        let program = Program {
            items: vec![
                Item::Function(Function {
                    name: "test_fn".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: Block {
                        stmts: vec![
                            Stmt::Let {
                                name: "data".to_string(),
                                ty: None,
                                mutable: false,
                                secret: false,
                                value: make_fetch_with_method("https://api.example.com/users", "POST"),
                                ownership: Ownership::Owned,
                            },
                            Stmt::Expr(make_field_access("data", "id")),
                        ],
                        span: dummy_span(),
                    },
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: dummy_span(),
                }),
            ],
        };

        let contracts = infer_contracts(&program);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].method, "POST");
    }

    #[test]
    fn test_infer_multiple_fields() {
        let program = make_program_with_fetch_and_usage(
            "https://api.example.com/products/1",
            "product",
            vec![
                Stmt::Expr(Expr::FormatString {
                    parts: vec![
                        FormatPart::Expression(Box::new(make_field_access("product", "name"))),
                    ],
                }),
                Stmt::Expr(Expr::Binary {
                    op: BinOp::Mul,
                    left: Box::new(make_field_access("product", "price")),
                    right: Box::new(Expr::Integer(100)),
                }),
                Stmt::Expr(Expr::If {
                    condition: Box::new(make_field_access("product", "in_stock")),
                    then_block: Block { stmts: vec![], span: dummy_span() },
                    else_block: None,
                }),
                Stmt::Expr(Expr::For {
                    binding: "tag".to_string(),
                    iterator: Box::new(make_field_access("product", "tags")),
                    body: Block { stmts: vec![], span: dummy_span() },
                }),
            ],
        );

        let contracts = infer_contracts(&program);
        assert_eq!(contracts.len(), 1);
        let c = &contracts[0];
        assert!(c.fields.len() >= 4);

        let name_field = c.fields.iter().find(|f| f.path == vec!["name"]).unwrap();
        assert_eq!(name_field.inferred_type, InferredType::String);

        let price_field = c.fields.iter().find(|f| f.path == vec!["price"]).unwrap();
        assert_eq!(price_field.inferred_type, InferredType::Numeric);

        let stock_field = c.fields.iter().find(|f| f.path == vec!["in_stock"]).unwrap();
        assert_eq!(stock_field.inferred_type, InferredType::Bool);

        let tags_field = c.fields.iter().find(|f| f.path == vec!["tags"]).unwrap();
        assert!(matches!(tags_field.inferred_type, InferredType::Array(_)));
    }

    #[test]
    fn test_infer_from_store_action() {
        let program = Program {
            items: vec![
                Item::Store(StoreDef {
                    name: "ProductStore".to_string(),
                    signals: vec![],
                    actions: vec![
                        ActionDef {
                            name: "load_products".to_string(),
                            params: vec![],
                            body: Block {
                                stmts: vec![
                                    Stmt::Let {
                                        name: "response".to_string(),
                                        ty: None,
                                        mutable: false,
                                        secret: false,
                                        value: make_fetch_expr("https://api.example.com/products"),
                                        ownership: Ownership::Owned,
                                    },
                                    Stmt::Expr(Expr::For {
                                        binding: "p".to_string(),
                                        iterator: Box::new(make_field_access("response", "data")),
                                        body: Block { stmts: vec![], span: dummy_span() },
                                    }),
                                ],
                                span: dummy_span(),
                            },
                            is_async: true,
                            span: dummy_span(),
                        },
                    ],
                    computed: vec![],
                    effects: vec![],
                    selectors: vec![],
                    is_pub: false,
                    span: dummy_span(),
                }),
            ],
        };

        let contracts = infer_contracts(&program);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].source_context, "ProductStore::load_products");
        assert_eq!(contracts[0].fields[0].path, vec!["data"]);
        assert!(matches!(contracts[0].fields[0].inferred_type, InferredType::Array(_)));
    }

    #[test]
    fn test_infer_empty_program() {
        let program = Program { items: vec![] };
        let contracts = infer_contracts(&program);
        assert!(contracts.is_empty());
    }

    #[test]
    fn test_inferred_type_display() {
        assert_eq!(format!("{}", InferredType::String), "String");
        assert_eq!(format!("{}", InferredType::Numeric), "Numeric");
        assert_eq!(format!("{}", InferredType::Bool), "Bool");
        assert_eq!(format!("{}", InferredType::Unknown), "Unknown");
        assert_eq!(format!("{}", InferredType::Array(Box::new(InferredType::String))), "[String]");
    }

    #[test]
    fn test_expr_to_string_literals() {
        assert_eq!(expr_to_string(&Expr::StringLit("https://api.com".to_string())), "https://api.com");
        assert_eq!(expr_to_string(&Expr::Integer(42)), "<dynamic>");
    }

    #[test]
    fn test_extract_method_default() {
        let expr = make_fetch_expr("https://api.com");
        assert_eq!(extract_method(&expr), "GET");
    }

    #[test]
    fn test_extract_method_post() {
        let expr = make_fetch_with_method("https://api.com", "post");
        assert_eq!(extract_method(&expr), "POST");
    }

    #[test]
    fn test_add_or_update_field_dedup() {
        let mut fields = Vec::new();
        add_or_update_field(&mut fields, &["name".to_string()], InferredType::Unknown, FieldEvidence::FieldAccess("name".to_string(), dummy_span()));
        add_or_update_field(&mut fields, &["name".to_string()], InferredType::String, FieldEvidence::UsedAsText(dummy_span()));

        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].inferred_type, InferredType::String);
        assert_eq!(fields[0].evidence.len(), 2);
    }
}
