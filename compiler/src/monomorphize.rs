/// Monomorphization pass: specializes generic functions for each concrete type
/// they are called with.
///
/// The pass has three stages:
/// 1. **Collection**: Walk all function calls. For calls to generic functions,
///    record `(fn_name, Vec<concrete_types>)`.
/// 2. **Cloning**: For each unique instantiation, clone the function AST with
///    type parameters substituted. Rename to `fn_name__type1_type2`.
/// 3. **Rewriting**: Replace call sites to use monomorphized names.
///
/// After this pass, no `Type::Generic` should remain in function bodies
/// (generic structs still use existing Vec/HashMap paths).

use std::collections::{HashMap, HashSet};
use crate::ast::*;

/// A unique instantiation of a generic function.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Instantiation {
    pub fn_name: String,
    pub concrete_types: Vec<String>,
}

impl Instantiation {
    pub fn mangled_name(&self) -> String {
        if self.concrete_types.is_empty() {
            self.fn_name.clone()
        } else {
            format!("{}__{}", self.fn_name, self.concrete_types.join("_"))
        }
    }
}

/// Run the monomorphization pass on a program.
/// Returns the number of instantiations generated.
pub fn monomorphize(program: &mut Program) -> usize {
    // Step 1: Collect generic function definitions
    let mut generic_fns: HashMap<String, Function> = HashMap::new();
    for item in &program.items {
        if let Item::Function(f) = item {
            if !f.type_params.is_empty() {
                generic_fns.insert(f.name.clone(), f.clone());
            }
        }
    }

    if generic_fns.is_empty() {
        return 0;
    }

    // Step 2: Collect instantiations by walking all expressions
    let mut instantiations: HashSet<Instantiation> = HashSet::new();
    for item in &program.items {
        collect_instantiations_in_item(item, &generic_fns, &mut instantiations);
    }

    if instantiations.is_empty() {
        return 0;
    }

    // Step 3: Clone and specialize
    let mut new_items: Vec<Item> = Vec::new();
    for inst in &instantiations {
        if let Some(generic_fn) = generic_fns.get(&inst.fn_name) {
            let specialized = specialize_function(generic_fn, &inst.concrete_types);
            new_items.push(Item::Function(specialized));
        }
    }

    // Step 4: Rewrite call sites
    for item in program.items.iter_mut() {
        rewrite_calls_in_item(item, &generic_fns, &instantiations);
    }

    // Step 5: Add specialized functions to the program
    let count = new_items.len();
    program.items.extend(new_items);

    count
}

/// Collect all instantiations of generic functions in an item.
fn collect_instantiations_in_item(
    item: &Item,
    generic_fns: &HashMap<String, Function>,
    out: &mut HashSet<Instantiation>,
) {
    match item {
        Item::Function(f) => {
            collect_instantiations_in_block(&f.body, generic_fns, out);
        }
        Item::Component(c) => {
            for method in &c.methods {
                collect_instantiations_in_block(&method.body, generic_fns, out);
            }
        }
        Item::Store(s) => {
            for action in &s.actions {
                collect_instantiations_in_block(&action.body, generic_fns, out);
            }
        }
        Item::Impl(imp) => {
            for method in &imp.methods {
                collect_instantiations_in_block(&method.body, generic_fns, out);
            }
        }
        Item::Test(t) => {
            collect_instantiations_in_block(&t.body, generic_fns, out);
        }
        _ => {}
    }
}

fn collect_instantiations_in_block(
    block: &Block,
    generic_fns: &HashMap<String, Function>,
    out: &mut HashSet<Instantiation>,
) {
    for stmt in &block.stmts {
        collect_instantiations_in_stmt(stmt, generic_fns, out);
    }
}

fn collect_instantiations_in_stmt(
    stmt: &Stmt,
    generic_fns: &HashMap<String, Function>,
    out: &mut HashSet<Instantiation>,
) {
    match stmt {
        Stmt::Let { value, .. } => {
            collect_instantiations_in_expr(value, generic_fns, out);
        }
        Stmt::Expr(e) => {
            collect_instantiations_in_expr(e, generic_fns, out);
        }
        Stmt::Return(Some(e)) => {
            collect_instantiations_in_expr(e, generic_fns, out);
        }
        Stmt::Signal { value, .. } => {
            collect_instantiations_in_expr(value, generic_fns, out);
        }
        Stmt::Yield(e) => {
            collect_instantiations_in_expr(e, generic_fns, out);
        }
        _ => {}
    }
}

fn collect_instantiations_in_expr(
    expr: &Expr,
    generic_fns: &HashMap<String, Function>,
    out: &mut HashSet<Instantiation>,
) {
    match expr {
        Expr::FnCall { callee, args } => {
            if let Expr::Ident(name) = callee.as_ref() {
                if let Some(generic_fn) = generic_fns.get(name) {
                    // Infer concrete types from argument expressions
                    let concrete_types: Vec<String> = generic_fn.type_params.iter()
                        .enumerate()
                        .map(|(i, _tp)| {
                            if i < args.len() {
                                infer_concrete_type(&args[i])
                            } else {
                                "i32".to_string() // default fallback
                            }
                        })
                        .collect();

                    out.insert(Instantiation {
                        fn_name: name.clone(),
                        concrete_types,
                    });
                }
            }
            // Recurse into args
            for arg in args {
                collect_instantiations_in_expr(arg, generic_fns, out);
            }
        }
        Expr::Binary { left, right, .. } => {
            collect_instantiations_in_expr(left, generic_fns, out);
            collect_instantiations_in_expr(right, generic_fns, out);
        }
        Expr::Unary { operand, .. } => {
            collect_instantiations_in_expr(operand, generic_fns, out);
        }
        Expr::If { condition, then_block, else_block } => {
            collect_instantiations_in_expr(condition, generic_fns, out);
            collect_instantiations_in_block(then_block, generic_fns, out);
            if let Some(eb) = else_block {
                collect_instantiations_in_block(eb, generic_fns, out);
            }
        }
        Expr::Block(block) => {
            collect_instantiations_in_block(block, generic_fns, out);
        }
        Expr::For { iterator, body, .. } => {
            collect_instantiations_in_expr(iterator, generic_fns, out);
            collect_instantiations_in_block(body, generic_fns, out);
        }
        Expr::While { condition, body } => {
            collect_instantiations_in_expr(condition, generic_fns, out);
            collect_instantiations_in_block(body, generic_fns, out);
        }
        Expr::MethodCall { object, args, .. } => {
            collect_instantiations_in_expr(object, generic_fns, out);
            for arg in args {
                collect_instantiations_in_expr(arg, generic_fns, out);
            }
        }
        Expr::FieldAccess { object, .. } => {
            collect_instantiations_in_expr(object, generic_fns, out);
        }
        Expr::Index { object, index } => {
            collect_instantiations_in_expr(object, generic_fns, out);
            collect_instantiations_in_expr(index, generic_fns, out);
        }
        Expr::Assign { target, value } => {
            collect_instantiations_in_expr(target, generic_fns, out);
            collect_instantiations_in_expr(value, generic_fns, out);
        }
        Expr::Borrow(inner) | Expr::BorrowMut(inner) | Expr::Await(inner) => {
            collect_instantiations_in_expr(inner, generic_fns, out);
        }
        Expr::Closure { body, .. } => {
            collect_instantiations_in_expr(body, generic_fns, out);
        }
        Expr::ArrayLit(elems) => {
            for e in elems {
                collect_instantiations_in_expr(e, generic_fns, out);
            }
        }
        _ => {}
    }
}

/// Infer a concrete type name from an argument expression.
fn infer_concrete_type(expr: &Expr) -> String {
    match expr {
        Expr::Integer(_) => "i32".to_string(),
        Expr::Float(_) => "f64".to_string(),
        Expr::StringLit(_) => "String".to_string(),
        Expr::Bool(_) => "bool".to_string(),
        _ => "i32".to_string(), // default for complex expressions
    }
}

/// Clone a generic function and substitute type parameters with concrete types.
fn specialize_function(func: &Function, concrete_types: &[String]) -> Function {
    let type_map: HashMap<String, String> = func.type_params.iter()
        .zip(concrete_types.iter())
        .map(|(tp, ct)| (tp.clone(), ct.clone()))
        .collect();

    let mangled = if concrete_types.is_empty() {
        func.name.clone()
    } else {
        format!("{}__{}", func.name, concrete_types.join("_"))
    };

    let mut specialized = func.clone();
    specialized.name = mangled;
    specialized.type_params = vec![]; // No longer generic

    // Substitute types in params
    for param in &mut specialized.params {
        param.ty = substitute_type(&param.ty, &type_map);
    }

    // Substitute return type
    if let Some(ref mut ret) = specialized.return_type {
        *ret = substitute_type(ret, &type_map);
    }

    // Substitute types in body
    substitute_types_in_block(&mut specialized.body, &type_map);

    specialized
}

fn substitute_type(ty: &Type, type_map: &HashMap<String, String>) -> Type {
    match ty {
        Type::Named(name) => {
            if let Some(concrete) = type_map.get(name) {
                Type::Named(concrete.clone())
            } else {
                ty.clone()
            }
        }
        Type::Generic { name, args } => {
            Type::Generic {
                name: name.clone(),
                args: args.iter().map(|a| substitute_type(a, type_map)).collect(),
            }
        }
        Type::Reference { mutable, lifetime, inner } => {
            Type::Reference {
                mutable: *mutable,
                lifetime: lifetime.clone(),
                inner: Box::new(substitute_type(inner, type_map)),
            }
        }
        Type::Array(inner) => Type::Array(Box::new(substitute_type(inner, type_map))),
        Type::Option(inner) => Type::Option(Box::new(substitute_type(inner, type_map))),
        Type::Result { ok, err } => Type::Result {
            ok: Box::new(substitute_type(ok, type_map)),
            err: Box::new(substitute_type(err, type_map)),
        },
        Type::Tuple(elems) => Type::Tuple(elems.iter().map(|e| substitute_type(e, type_map)).collect()),
        Type::Function { params, ret } => Type::Function {
            params: params.iter().map(|p| substitute_type(p, type_map)).collect(),
            ret: Box::new(substitute_type(ret, type_map)),
        },
    }
}

fn substitute_types_in_block(block: &mut Block, type_map: &HashMap<String, String>) {
    for stmt in &mut block.stmts {
        substitute_types_in_stmt(stmt, type_map);
    }
}

fn substitute_types_in_stmt(stmt: &mut Stmt, type_map: &HashMap<String, String>) {
    match stmt {
        Stmt::Let { ty, value, .. } => {
            if let Some(t) = ty {
                *t = substitute_type(t, type_map);
            }
            substitute_types_in_expr(value, type_map);
        }
        Stmt::Expr(e) => {
            substitute_types_in_expr(e, type_map);
        }
        Stmt::Return(Some(e)) => {
            substitute_types_in_expr(e, type_map);
        }
        _ => {}
    }
}

fn substitute_types_in_expr(expr: &mut Expr, type_map: &HashMap<String, String>) {
    match expr {
        Expr::FnCall { callee, args } => {
            substitute_types_in_expr(callee, type_map);
            for arg in args {
                substitute_types_in_expr(arg, type_map);
            }
        }
        Expr::Binary { left, right, .. } => {
            substitute_types_in_expr(left, type_map);
            substitute_types_in_expr(right, type_map);
        }
        Expr::If { condition, then_block, else_block } => {
            substitute_types_in_expr(condition, type_map);
            substitute_types_in_block(then_block, type_map);
            if let Some(eb) = else_block {
                substitute_types_in_block(eb, type_map);
            }
        }
        Expr::For { iterator, body, .. } => {
            substitute_types_in_expr(iterator, type_map);
            substitute_types_in_block(body, type_map);
        }
        Expr::While { condition, body } => {
            substitute_types_in_expr(condition, type_map);
            substitute_types_in_block(body, type_map);
        }
        Expr::Block(block) => {
            substitute_types_in_block(block, type_map);
        }
        Expr::StructInit { fields, .. } => {
            for (_, val) in fields {
                substitute_types_in_expr(val, type_map);
            }
        }
        _ => {}
    }
}

/// Rewrite function call sites: replace calls to generic functions with their
/// monomorphized versions.
fn rewrite_calls_in_item(
    item: &mut Item,
    generic_fns: &HashMap<String, Function>,
    instantiations: &HashSet<Instantiation>,
) {
    match item {
        Item::Function(f) => {
            rewrite_calls_in_block(&mut f.body, generic_fns, instantiations);
        }
        Item::Component(c) => {
            for method in &mut c.methods {
                rewrite_calls_in_block(&mut method.body, generic_fns, instantiations);
            }
        }
        Item::Store(s) => {
            for action in &mut s.actions {
                rewrite_calls_in_block(&mut action.body, generic_fns, instantiations);
            }
        }
        Item::Impl(imp) => {
            for method in &mut imp.methods {
                rewrite_calls_in_block(&mut method.body, generic_fns, instantiations);
            }
        }
        Item::Test(t) => {
            rewrite_calls_in_block(&mut t.body, generic_fns, instantiations);
        }
        _ => {}
    }
}

fn rewrite_calls_in_block(
    block: &mut Block,
    generic_fns: &HashMap<String, Function>,
    instantiations: &HashSet<Instantiation>,
) {
    for stmt in &mut block.stmts {
        rewrite_calls_in_stmt(stmt, generic_fns, instantiations);
    }
}

fn rewrite_calls_in_stmt(
    stmt: &mut Stmt,
    generic_fns: &HashMap<String, Function>,
    instantiations: &HashSet<Instantiation>,
) {
    match stmt {
        Stmt::Let { value, .. } => {
            rewrite_calls_in_expr(value, generic_fns, instantiations);
        }
        Stmt::Expr(e) => {
            rewrite_calls_in_expr(e, generic_fns, instantiations);
        }
        Stmt::Return(Some(e)) => {
            rewrite_calls_in_expr(e, generic_fns, instantiations);
        }
        _ => {}
    }
}

fn rewrite_calls_in_expr(
    expr: &mut Expr,
    generic_fns: &HashMap<String, Function>,
    instantiations: &HashSet<Instantiation>,
) {
    match expr {
        Expr::FnCall { callee, args } => {
            // First recurse into args
            for arg in args.iter_mut() {
                rewrite_calls_in_expr(arg, generic_fns, instantiations);
            }
            // Then check if this is a call to a generic function
            if let Expr::Ident(name) = callee.as_ref() {
                if let Some(generic_fn) = generic_fns.get(name) {
                    let concrete_types: Vec<String> = generic_fn.type_params.iter()
                        .enumerate()
                        .map(|(i, _)| {
                            if i < args.len() {
                                infer_concrete_type(&args[i])
                            } else {
                                "i32".to_string()
                            }
                        })
                        .collect();

                    let inst = Instantiation {
                        fn_name: name.clone(),
                        concrete_types,
                    };

                    if instantiations.contains(&inst) {
                        *callee = Box::new(Expr::Ident(inst.mangled_name()));
                    }
                }
            }
        }
        Expr::Binary { left, right, .. } => {
            rewrite_calls_in_expr(left, generic_fns, instantiations);
            rewrite_calls_in_expr(right, generic_fns, instantiations);
        }
        Expr::If { condition, then_block, else_block } => {
            rewrite_calls_in_expr(condition, generic_fns, instantiations);
            rewrite_calls_in_block(then_block, generic_fns, instantiations);
            if let Some(eb) = else_block {
                rewrite_calls_in_block(eb, generic_fns, instantiations);
            }
        }
        Expr::For { iterator, body, .. } => {
            rewrite_calls_in_expr(iterator, generic_fns, instantiations);
            rewrite_calls_in_block(body, generic_fns, instantiations);
        }
        Expr::While { condition, body } => {
            rewrite_calls_in_expr(condition, generic_fns, instantiations);
            rewrite_calls_in_block(body, generic_fns, instantiations);
        }
        Expr::Block(block) => {
            rewrite_calls_in_block(block, generic_fns, instantiations);
        }
        Expr::Assign { target, value } => {
            rewrite_calls_in_expr(target, generic_fns, instantiations);
            rewrite_calls_in_expr(value, generic_fns, instantiations);
        }
        Expr::MethodCall { object, args, .. } => {
            rewrite_calls_in_expr(object, generic_fns, instantiations);
            for arg in args {
                rewrite_calls_in_expr(arg, generic_fns, instantiations);
            }
        }
        Expr::Closure { body, .. } => {
            rewrite_calls_in_expr(body, generic_fns, instantiations);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Static Trait Dispatch
// ---------------------------------------------------------------------------

/// Resolve a trait method call to a concrete `{Type}_{method}` name.
/// Given a trait name, method name, and concrete type, returns the mangled name
/// that the codegen should use.
pub fn resolve_trait_method(concrete_type: &str, method_name: &str) -> String {
    format!("{}_{}", concrete_type, method_name)
}

/// Build a map of (trait_name, method_name) -> Vec<(concrete_type, Function)>
/// from all `impl TraitName for Type` blocks in the program.
pub fn build_trait_impl_map(program: &Program) -> HashMap<(String, String), Vec<(String, Function)>> {
    let mut map: HashMap<(String, String), Vec<(String, Function)>> = HashMap::new();
    for item in &program.items {
        if let Item::Impl(imp) = item {
            for trait_name in &imp.trait_impls {
                for method in &imp.methods {
                    let key = (trait_name.clone(), method.name.clone());
                    map.entry(key)
                        .or_default()
                        .push((imp.target.clone(), method.clone()));
                }
            }
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Span;

    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    fn block(stmts: Vec<Stmt>) -> Block {
        Block { stmts, span: span() }
    }

    #[test]
    fn test_monomorphize_identity_i32() {
        // fn identity<T>(x: T) -> T { return x; }
        // fn main() { let a = identity(42); }
        let mut prog = Program {
            items: vec![
                Item::Function(Function {
                    name: "identity".to_string(),
                    lifetimes: vec![],
                    type_params: vec!["T".to_string()],
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Named("T".into()),
                        ownership: Ownership::Owned,
                        secret: false,
                    }],
                    return_type: Some(Type::Named("T".into())),
                    trait_bounds: vec![],
                    body: block(vec![
                        Stmt::Return(Some(Expr::Ident("x".into()))),
                    ]),
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: span(),
                }),
                Item::Function(Function {
                    name: "main".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: block(vec![
                        Stmt::Let {
                            name: "a".to_string(),
                            ty: None,
                            value: Expr::FnCall {
                                callee: Box::new(Expr::Ident("identity".into())),
                                args: vec![Expr::Integer(42)],
                            },
                            mutable: false,
                            secret: false,
                            ownership: Ownership::Owned,
                        },
                    ]),
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: span(),
                }),
            ],
        };

        let count = monomorphize(&mut prog);
        assert_eq!(count, 1, "Should generate 1 instantiation");

        // Check that identity__i32 was added
        let has_specialized = prog.items.iter().any(|item| {
            if let Item::Function(f) = item {
                f.name == "identity__i32"
            } else {
                false
            }
        });
        assert!(has_specialized, "Should have identity__i32 function");

        // Check that the call site was rewritten
        if let Item::Function(main_fn) = &prog.items[1] {
            if let Stmt::Let { value, .. } = &main_fn.body.stmts[0] {
                if let Expr::FnCall { callee, .. } = value {
                    if let Expr::Ident(name) = callee.as_ref() {
                        assert_eq!(name, "identity__i32", "Call site should be rewritten");
                    }
                }
            }
        }
    }

    #[test]
    fn test_monomorphize_identity_string() {
        let mut prog = Program {
            items: vec![
                Item::Function(Function {
                    name: "identity".to_string(),
                    lifetimes: vec![],
                    type_params: vec!["T".to_string()],
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Named("T".into()),
                        ownership: Ownership::Owned,
                        secret: false,
                    }],
                    return_type: Some(Type::Named("T".into())),
                    trait_bounds: vec![],
                    body: block(vec![
                        Stmt::Return(Some(Expr::Ident("x".into()))),
                    ]),
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: span(),
                }),
                Item::Function(Function {
                    name: "main".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: block(vec![
                        Stmt::Let {
                            name: "a".to_string(),
                            ty: None,
                            value: Expr::FnCall {
                                callee: Box::new(Expr::Ident("identity".into())),
                                args: vec![Expr::StringLit("hello".into())],
                            },
                            mutable: false,
                            secret: false,
                            ownership: Ownership::Owned,
                        },
                    ]),
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: span(),
                }),
            ],
        };

        let count = monomorphize(&mut prog);
        assert_eq!(count, 1);

        let has_specialized = prog.items.iter().any(|item| {
            if let Item::Function(f) = item { f.name == "identity__String" } else { false }
        });
        assert!(has_specialized, "Should have identity__String function");
    }

    #[test]
    fn test_monomorphize_multiple_instantiations() {
        let mut prog = Program {
            items: vec![
                Item::Function(Function {
                    name: "identity".to_string(),
                    lifetimes: vec![],
                    type_params: vec!["T".to_string()],
                    params: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Named("T".into()),
                        ownership: Ownership::Owned,
                        secret: false,
                    }],
                    return_type: Some(Type::Named("T".into())),
                    trait_bounds: vec![],
                    body: block(vec![
                        Stmt::Return(Some(Expr::Ident("x".into()))),
                    ]),
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: span(),
                }),
                Item::Function(Function {
                    name: "main".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: block(vec![
                        Stmt::Let {
                            name: "a".to_string(),
                            ty: None,
                            value: Expr::FnCall {
                                callee: Box::new(Expr::Ident("identity".into())),
                                args: vec![Expr::Integer(42)],
                            },
                            mutable: false,
                            secret: false,
                            ownership: Ownership::Owned,
                        },
                        Stmt::Let {
                            name: "b".to_string(),
                            ty: None,
                            value: Expr::FnCall {
                                callee: Box::new(Expr::Ident("identity".into())),
                                args: vec![Expr::StringLit("hello".into())],
                            },
                            mutable: false,
                            secret: false,
                            ownership: Ownership::Owned,
                        },
                    ]),
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: span(),
                }),
            ],
        };

        let count = monomorphize(&mut prog);
        assert_eq!(count, 2, "Should generate 2 instantiations (i32 and String)");
    }

    #[test]
    fn test_monomorphize_no_generics() {
        let mut prog = Program {
            items: vec![
                Item::Function(Function {
                    name: "add".to_string(),
                    lifetimes: vec![],
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    trait_bounds: vec![],
                    body: block(vec![]),
                    is_pub: false,
                    is_async: false,
                    must_use: false,
                    span: span(),
                }),
            ],
        };

        let count = monomorphize(&mut prog);
        assert_eq!(count, 0, "Should not generate anything for non-generic functions");
    }

    #[test]
    fn test_instantiation_mangled_name() {
        let inst = Instantiation {
            fn_name: "identity".to_string(),
            concrete_types: vec!["i32".to_string()],
        };
        assert_eq!(inst.mangled_name(), "identity__i32");

        let inst2 = Instantiation {
            fn_name: "pair".to_string(),
            concrete_types: vec!["i32".to_string(), "String".to_string()],
        };
        assert_eq!(inst2.mangled_name(), "pair__i32_String");
    }

    #[test]
    fn test_substitute_type_named() {
        let mut map = HashMap::new();
        map.insert("T".to_string(), "i32".to_string());

        let result = substitute_type(&Type::Named("T".into()), &map);
        assert!(matches!(result, Type::Named(n) if n == "i32"));

        let result2 = substitute_type(&Type::Named("String".into()), &map);
        assert!(matches!(result2, Type::Named(n) if n == "String"));
    }

    #[test]
    fn test_infer_concrete_type_from_literal() {
        assert_eq!(infer_concrete_type(&Expr::Integer(42)), "i32");
        assert_eq!(infer_concrete_type(&Expr::Float(3.14)), "f64");
        assert_eq!(infer_concrete_type(&Expr::StringLit("hi".into())), "String");
        assert_eq!(infer_concrete_type(&Expr::Bool(true)), "bool");
    }

    #[test]
    fn test_resolve_trait_method() {
        assert_eq!(resolve_trait_method("Point", "display"), "Point_display");
        assert_eq!(resolve_trait_method("Circle", "area"), "Circle_area");
    }

    #[test]
    fn test_build_trait_impl_map() {
        let prog = Program {
            items: vec![
                Item::Impl(ImplBlock {
                    target: "Point".to_string(),
                    trait_impls: vec!["Display".to_string()],
                    methods: vec![
                        Function {
                            name: "display".to_string(),
                            lifetimes: vec![],
                            type_params: vec![],
                            params: vec![],
                            return_type: Some(Type::Named("String".into())),
                            trait_bounds: vec![],
                            body: block(vec![]),
                            is_pub: false,
                            is_async: false,
                            must_use: false,
                            span: span(),
                        },
                    ],
                    span: span(),
                }),
            ],
        };

        let map = build_trait_impl_map(&prog);
        let key = ("Display".to_string(), "display".to_string());
        assert!(map.contains_key(&key));
        assert_eq!(map[&key].len(), 1);
        assert_eq!(map[&key][0].0, "Point");
    }
}
